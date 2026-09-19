#!/usr/bin/env python3
"""Nothing that builds or publishes this project comes from an unpinned source.

Three rules, one question: when CI produces an artifact, is every input to it something the
repository named? SECURITY_AUDIT.md F14 and F43.

**Why this exists.** SECURITY_AUDIT.md F14. `rc-bundle.yml` cuts the Windows release, and it
reached for `Swatinem/rust-cache@v2`. A tag is a mutable pointer: whoever can move `v2` in that
repository chooses what runs on the machine that produces the binary users install. Every other
action in either repository is published by GitHub itself under the `actions/` namespace, which
is the same trust root as the runner executing them, so those are left on their tags
deliberately — pinning them protects against nothing that is not already total. This is the one
action where the trust boundary is real, and it was the one not pinned.

**The second half is the cache, and it is the half that had already happened.** rust-cache caches
`${CARGO_HOME}/bin` by default. A Windows RC run wrote a 452 MB cache on `refs/heads/main` whose
path list includes that directory, so the next dispatch restores it, `command -v dx` finds the
restored binary, the install step short-circuits, and the shipped bundle is cut by a build tool
that came out of a cache rather than out of `cargo install --locked --version`. A cache is not a
supply chain — it has no lockfile, no version and no provenance — so `cache-bin: false` is set
and the CLI is built every cut. That costs one dioxus-cli build per release and buys the property
that the tool which produced the artifact is the one the workflow names.

**The third rule is `npm ci || npm install` (F43).** `npm ci` refuses when `package-lock.json`
and `package.json` disagree; `npm install` resolves whatever satisfies the ranges instead, and
rewrites the lockfile as it goes. The fallback therefore converts "the lockfile is stale, stop"
into "install something nobody reviewed, quietly" — Rule 1's shape exactly. It matters most where
it sat: the `pipeline` job runs `npm run regen` and then `git diff --exit-code`, so its verdict is
the authority for every committed dataset in the repository. A regen-diff that passes under a
drifted dependency tree does not merely fail to catch something, it certifies the data.

Measured before removing it: in run 35464084784 the step emitted no npm error at all, so `npm ci`
succeeded and the fallback has been dead code rather than load-bearing.

**How to grade this script**, since a passing run against a correct tree says nothing about
whether the check works: `--self-test` re-runs every rule against text that deliberately breaks
it and fails if any is let through.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github" / "workflows"

# `uses: owner/repo@ref` or `uses: owner/repo/path@ref`, with an optional trailing comment.
USES = re.compile(r"^\s*(?:-\s*)?uses:\s*([^\s#]+)\s*(?:#.*)?$")
SHA = re.compile(r"^[0-9a-f]{40}$")

# GitHub's own actions. Their tags are as trustworthy as the runner that executes them, so a tag
# here is a considered choice and not an oversight. Anything else must name a commit.
FIRST_PARTY = ("actions/", "github/")

# The step that must not hand a build tool to the job that cuts a release.
CACHES_CARGO_BIN = "Swatinem/rust-cache"

# An install that falls back off the lockfile. Matched on the fallback rather than on `npm ci`,
# because `npm ci` alone is the thing we want.
UNLOCKED_INSTALL = re.compile(r"npm\s+(?:ci|install)[^\n|&]*(?:\|\||&&|;)\s*npm\s+install\b")


def unpinned(text: str) -> list[tuple[int, str]]:
    """Every third-party `uses:` in `text` that names something other than a 40-hex commit."""
    found = []
    for number, line in enumerate(text.splitlines(), start=1):
        match = USES.match(line)
        if match is None:
            continue
        spec = match.group(1)
        if spec.startswith("./"):  # a local composite action is this repository
            continue
        if spec.startswith(FIRST_PARTY):
            continue
        _, _, ref = spec.partition("@")
        if not SHA.match(ref):
            found.append((number, spec))
    return found


def caches_a_build_tool(text: str) -> bool:
    """True when a rust-cache step is present and has not opted out of caching `$CARGO_HOME/bin`."""
    if CACHES_CARGO_BIN not in text:
        return False
    # The `with:` block of that step, read as the lines between it and the next step.
    after = text.split(CACHES_CARGO_BIN, 1)[1]
    block = after.split("\n      - ", 1)[0]
    return "cache-bin: false" not in block


def without_comment(line: str) -> str:
    """`line` up to its comment, if it has one.

    This exists because the check flagged the comment that explains the check — the same
    self-matching shape that let F01's guard pass with its call sites gutted. A comment is not a
    command, and a rule that cannot tell them apart makes writing down why a rule exists into a
    violation of it. `#` opens a comment at the start of a line or after whitespace, which holds
    for YAML and for the shell inside a `run:` block alike.
    """
    stripped = line.lstrip()
    if stripped.startswith("#"):
        return ""
    cut = re.search(r"\s#", line)
    return line[: cut.start()] if cut else line


def unlocked_installs(text: str) -> list[tuple[int, str]]:
    """Every line that would install dependencies off the lockfile when `npm ci` refuses."""
    return [
        (number, line.strip())
        for number, line in enumerate(text.splitlines(), start=1)
        if UNLOCKED_INSTALL.search(without_comment(line))
    ]


def self_test() -> int:
    """Grade both rules against text that breaks them. A check nobody has seen fail is a wish."""
    failures = []

    tagged = "      - uses: Swatinem/rust-cache@v2\n"
    if not unpinned(tagged):
        failures.append("a tag-pinned third-party action was not caught")

    pinned = "      - uses: Swatinem/rust-cache@" + "a" * 40 + " # v2.9.2\n"
    if unpinned(pinned):
        failures.append("a correctly pinned action was reported as unpinned")

    first_party = "      - uses: actions/checkout@v4\n"
    if unpinned(first_party):
        failures.append("a GitHub-published action on a tag was reported; that is allowed here")

    caching = pinned + "        with:\n          key: rc-windows\n\n      - name: next step\n"
    if not caches_a_build_tool(caching):
        failures.append("a rust-cache step with no cache-bin: false was not caught")

    opted_out = (
        pinned
        + "        with:\n          key: rc-windows\n          cache-bin: false\n\n      - name: next\n"
    )
    if caches_a_build_tool(opted_out):
        failures.append("cache-bin: false was not recognised")

    if not unlocked_installs("      - run: npm ci || npm install\n"):
        failures.append("an npm ci fallback to an unlocked install was not caught")
    if not unlocked_installs("        run: npm ci --foreground-scripts || npm install --no-audit\n"):
        failures.append("the fallback was not caught once either side carried flags")
    if unlocked_installs("      - run: npm ci\n"):
        failures.append("a plain npm ci was reported; that is the thing we want")
    if unlocked_installs("      - run: npm run build && npm test\n"):
        failures.append("an ordinary chained command was reported as an unlocked install")
    # The case this check got wrong about itself: the comment that explains the rule.
    if unlocked_installs("      # the `npm ci || npm install` that was here is F43\n"):
        failures.append("a comment describing the pattern was reported as the pattern")
    if unlocked_installs("      - run: npm ci  # not npm ci || npm install any more\n"):
        failures.append("a trailing comment was read as part of the command")

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: all three rules refuse what they are meant to refuse")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    problems = []
    files = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if not files:
        print(f"no workflows under {WORKFLOWS}; this check proved nothing", file=sys.stderr)
        return 1

    for path in files:
        text = path.read_text(encoding="utf-8")
        for number, spec in unpinned(text):
            problems.append(
                f"{path.relative_to(ROOT)}:{number}: {spec} names a mutable ref; "
                f"pin it to a 40-character commit and put the tag in a trailing comment"
            )
        for number, line in unlocked_installs(text):
            problems.append(
                f"{path.relative_to(ROOT)}:{number}: `{line}` installs off the lockfile when "
                f"`npm ci` refuses; let it refuse"
            )
        if caches_a_build_tool(text):
            problems.append(
                f"{path.relative_to(ROOT)}: a rust-cache step does not set `cache-bin: false`, "
                f"so $CARGO_HOME/bin is cached and a restored `dx` can cut the release"
            )

    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    if problems:
        return 1
    print(
        f"{len(files)} workflows: every third-party action names a commit, "
        f"no cache holds a build tool, no install falls off the lockfile"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
