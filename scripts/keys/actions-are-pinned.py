#!/usr/bin/env python3
"""No workflow runs a third-party action by a mutable tag, and no cache supplies a build tool.

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

**How to grade this script**, since a passing run against a correct tree says nothing about
whether the check works: `--self-test` re-runs both rules against text that deliberately breaks
them and fails if either is let through.
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

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: both rules refuse what they are meant to refuse")
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
        if caches_a_build_tool(text):
            problems.append(
                f"{path.relative_to(ROOT)}: a rust-cache step does not set `cache-bin: false`, "
                f"so $CARGO_HOME/bin is cached and a restored `dx` can cut the release"
            )

    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"{len(files)} workflows: every third-party action names a commit, no cache holds a build tool")
    return 0


if __name__ == "__main__":
    sys.exit(main())
