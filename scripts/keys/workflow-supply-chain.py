#!/usr/bin/env python3
"""Nothing that builds or publishes this project comes from an unpinned source.

Four rules, one question: when CI produces an artifact, can somebody say where every byte of it
came from? Three are about the inputs (F14, F43) and the fourth is about the output (F40).

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

**The fourth rule is the RC artifact itself (F40).** A bundle handed to a tester with no
checksum is one nobody can tell apart from a different bundle, so every `upload-artifact` in
`rc-bundle.yml` must be preceded by `rc-checksums.py`. Scoped to that file by name: `ci.yml`
uploads logs and reports, which nobody verifies against anything.

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

# The workflow that hands bundles to people, and the step that must come before each upload.
RELEASE_WORKFLOW = "rc-bundle.yml"
CHECKSUMS = "rc-checksums.py"
UPLOAD = "uses: actions/upload-artifact"

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


def unchecksummed_uploads(text: str) -> list[int]:
    """Line numbers of artifact uploads in the release workflow with no checksum before them.

    "Before" rather than "anywhere in the file" because three jobs each upload their own bundle,
    so one checksum step and three uploads would otherwise read as covered.
    """
    unguarded = []
    since_checksum = None
    for number, line in enumerate(text.splitlines(), start=1):
        bare = without_comment(line)
        if CHECKSUMS in bare:
            since_checksum = number
        elif UPLOAD in bare:
            if since_checksum is None:
                unguarded.append(number)
            since_checksum = None
    return unguarded


def declares_permissions(text: str) -> bool:
    """Whether the workflow states a top-level `permissions:` block — SECURITY_AUDIT.md F59.

    Column 0 is the whole test, because column 0 is what makes it top-level. A `permissions:`
    nested under one job binds that job and leaves every other job on the repository default,
    which is the thing this rule exists to stop being invisible: that default is a setting in the
    web UI, it can be widened to read/write for every workflow at once, and nothing in the
    repository changes when it is.

    Deliberately not a check that the block is NARROW. What each workflow legitimately needs
    differs — the beta's `deploy.yml` needs `pages: write` and `id-token: write` — and a rule
    that guessed at the right set would be wrong for the first job that needed more. Stating the
    set is what is enforced; choosing it is a review.
    """
    return any(line.startswith("permissions:") for line in text.splitlines())


def self_test() -> int:
    """Grade every rule against text that breaks it. A check nobody has seen fail is a wish."""
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

    covered = "      - run: python3 scripts/keys/rc-checksums.py dist\n      - uses: actions/upload-artifact@v4\n"
    if unchecksummed_uploads(covered):
        failures.append("a checksummed upload was reported as unchecksummed")
    if not unchecksummed_uploads("      - uses: actions/upload-artifact@v4\n"):
        failures.append("an upload with no checksum before it was not caught")
    # Three jobs, one checksum: the second and third uploads are not covered by the first.
    three = covered + "      - uses: actions/upload-artifact@v4\n"
    if len(unchecksummed_uploads(three)) != 1:
        failures.append("one checksum step was read as covering a later, separate upload")

    if declares_permissions("name: CI\non:\n  push:\n\njobs:\n  build:\n"):
        failures.append("a workflow with no permissions block was reported as having one")
    if not declares_permissions("name: CI\non:\n  push:\n\npermissions:\n  contents: read\n"):
        failures.append("a top-level permissions block was not recognised")
    if declares_permissions("jobs:\n  build:\n    permissions:\n      contents: read\n"):
        failures.append(
            "a job-level permissions block was accepted as top-level; it binds one job and "
            "leaves the rest on the repository default"
        )

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: all five rules refuse what they are meant to refuse")
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
        if path.name == RELEASE_WORKFLOW:
            for number in unchecksummed_uploads(text):
                problems.append(
                    f"{path.relative_to(ROOT)}:{number}: an RC artifact is uploaded with no "
                    f"{CHECKSUMS} step before it, so nobody can tell this bundle from another"
                )
        if not declares_permissions(text):
            problems.append(
                f"{path.relative_to(ROOT)}: no top-level `permissions:` block, so every job in "
                f"it takes the repository default — a setting in the web UI that can be widened "
                f"to read/write for every workflow at once, with nothing here changing"
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
        f"no cache holds a build tool, no install falls off the lockfile, "
        f"every release artifact is checksummed, every workflow states its permissions"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
