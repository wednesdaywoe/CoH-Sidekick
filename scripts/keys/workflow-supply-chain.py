#!/usr/bin/env python3
"""Nothing that builds or publishes this project comes from an unpinned source.

Seven rules, one question: when CI produces an artifact, can somebody say where every byte of
it came from? Five are about the inputs (F14 twice, F43, F42, F15), one is about the output
(F40), and one is about the permissions the whole thing runs under (F59).

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

**The sixth rule is the build tool itself (F42).** All three legs of `rc-bundle.yml` ran
`command -v dx >/dev/null 2>&1 || cargo install dioxus-cli --locked --version 0.7.9`. That reads
as a pin. It behaves as "use the pin unless the box already has a `dx`" — and two of the three
legs run on persistent self-hosted machines that have had one since July, so on those two the
pin never fired and no step recorded which binary had cut the release. A conditional pin is
worse than no pin, because it passes review. The rule is therefore: no `command -v dx` anywhere
in that file, every `cargo install dioxus-cli` names `--locked`, `--version` and
`--features no-downloads`, and all of them name the SAME version — three legs that quietly drift
apart is the failure a single grep would miss.

**The seventh rule is the tool that tool runs (F15).** `dx bundle --package-types appimage` shells
out to `linuxdeploy`, which dioxus-cli fetches from a mutable release tag with no hash and caches
forever. `rc-linuxdeploy.py` seeds that cache by digest, and `--features no-downloads` above turns
a missed seed into a refusal instead of a silent unverified download. So: every AppImage bundle in
the release workflow must have a seed step before it — "before" for the same reason the checksum
rule says "before".

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

# The release workflow's own build tool. `command -v dx` is the conditional that made the pin
# advisory; the flags are what each install must name; the seed is what must precede an AppImage.
DX_PRESENCE_TEST = re.compile(r"command\s+-v\s+dx\b")
DX_INSTALL = re.compile(r"cargo\s+install\s+dioxus-cli\b[^\n]*")
DX_VERSION = re.compile(r"--version[= ]\s*([0-9][^\s]*)")
DX_REQUIRED_FLAGS = ("--locked", "--version", "--features no-downloads")
LINUXDEPLOY_SEED = "rc-linuxdeploy.py"
APPIMAGE_BUNDLE = re.compile(r"\bdx\s+bundle\b[^\n]*\bappimage\b")


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


def build_tool_unpinned(text: str) -> list[tuple[int, str]]:
    """Every way the release workflow's `dx` could be something other than the version it names.

    Three shapes, because they fail differently. A `command -v dx` makes the pin conditional on
    the machine. A missing flag makes it incomplete. Two legs naming two versions makes it
    unanswerable which one cut the artifact, which is the same question this whole file asks.
    """
    found = []
    versions: dict[str, int] = {}

    for number, line in enumerate(text.splitlines(), start=1):
        bare = without_comment(line)
        if DX_PRESENCE_TEST.search(bare):
            found.append(
                (number, "`command -v dx` makes the pin conditional on what is on the box")
            )
        install = DX_INSTALL.search(bare)
        if install is None:
            continue
        command = install.group(0)
        for flag in DX_REQUIRED_FLAGS:
            if flag not in command:
                found.append((number, f"this `cargo install dioxus-cli` does not name `{flag}`"))
        version = DX_VERSION.search(command)
        if version is not None:
            versions.setdefault(version.group(1), number)

    if len(versions) > 1:
        named = ", ".join(sorted(versions))
        for version, number in sorted(versions.items(), key=lambda pair: pair[1]):
            found.append(
                (number, f"this leg installs dioxus-cli {version}; the file names {named}")
            )
    return found


def unseeded_appimages(text: str) -> list[int]:
    """Line numbers of AppImage bundles with no digest-verified linuxdeploy seeded before them.

    "Before" for `unchecksummed_uploads`' reason: a seed step is a property of the job it runs
    in, and one of them somewhere in the file does not cover a second job that has none.
    """
    unguarded = []
    since_seed = None
    for number, line in enumerate(text.splitlines(), start=1):
        bare = without_comment(line)
        if LINUXDEPLOY_SEED in bare:
            since_seed = number
        elif APPIMAGE_BUNDLE.search(bare):
            if since_seed is None:
                unguarded.append(number)
            since_seed = None
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

    good_install = (
        "          cargo install dioxus-cli --locked --version 0.7.9 --features no-downloads\n"
    )
    if build_tool_unpinned(good_install):
        failures.append("a fully pinned dioxus-cli install was reported")
    if not build_tool_unpinned("      - run: command -v dx >/dev/null 2>&1 || " + good_install):
        failures.append("a `command -v dx` short-circuit was not caught")
    if not build_tool_unpinned("          cargo install dioxus-cli --version 0.7.9 --features no-downloads\n"):
        failures.append("an install with no --locked was not caught")
    if not build_tool_unpinned("          cargo install dioxus-cli --locked --features no-downloads\n"):
        failures.append("an install with no --version was not caught")
    if not build_tool_unpinned("          cargo install dioxus-cli --locked --version 0.7.9\n"):
        failures.append("an install with no --features no-downloads was not caught")
    # Two legs, two versions: the case a grep for `--version 0.7.9` passes and this must not.
    drifted = good_install + good_install.replace("0.7.9", "0.7.8")
    if len(build_tool_unpinned(drifted)) != 2:
        failures.append("two legs installing two different dioxus-cli versions were not caught")
    if build_tool_unpinned(good_install + good_install):
        failures.append("two legs installing the SAME version were reported as drifted")
    # The comment that explains the rule is not the rule, as F43's case above already found out.
    if build_tool_unpinned("      # the `command -v dx ||` that was here is F42\n"):
        failures.append("a comment describing the short-circuit was read as the short-circuit")

    seeded = (
        "        run: python3 scripts/keys/rc-linuxdeploy.py\n"
        "      - run: dx bundle --platform linux --package-types appimage --release\n"
    )
    if unseeded_appimages(seeded):
        failures.append("a seeded AppImage bundle was reported as unseeded")
    if not unseeded_appimages("      - run: dx bundle --package-types appimage --release\n"):
        failures.append("an AppImage bundle with no linuxdeploy seed before it was not caught")
    if len(unseeded_appimages(seeded + "      - run: dx bundle --package-types appimage\n")) != 1:
        failures.append("one seed step was read as covering a later, separate AppImage job")
    if unseeded_appimages("      - run: dx bundle --platform macos --package-types macos\n"):
        failures.append("a macOS bundle was reported; it runs no linuxdeploy")

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
    print("self-test: all seven rules refuse what they are meant to refuse")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    problems = []
    files = sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
    if not files:
        print(f"no workflows under {WORKFLOWS}; this check proved nothing", file=sys.stderr)
        return 1

    release_workflows = 0
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
            release_workflows += 1
            for number in unchecksummed_uploads(text):
                problems.append(
                    f"{path.relative_to(ROOT)}:{number}: an RC artifact is uploaded with no "
                    f"{CHECKSUMS} step before it, so nobody can tell this bundle from another"
                )
            for number, why in build_tool_unpinned(text):
                problems.append(
                    f"{path.relative_to(ROOT)}:{number}: {why}, so the CLI that cut the release "
                    f"is not the one this file names"
                )
            for number in unseeded_appimages(text):
                problems.append(
                    f"{path.relative_to(ROOT)}:{number}: an AppImage is bundled with no "
                    f"{LINUXDEPLOY_SEED} step before it, so dx fetches linuxdeploy from a "
                    f"mutable tag with no hash and runs it"
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
    # Three of the seven rules only have anything to say about `rc-bundle.yml`, and this script
    # is mirrored into a repository that does not have one. Reporting them as satisfied there
    # is a green that means nothing -- the shape that let F86's first patch pass while serving
    # nothing -- so the summary says which half actually ran.
    everywhere = (
        f"{len(files)} workflows: every third-party action names a commit, no cache holds a "
        f"build tool, no install falls off the lockfile, every workflow states its permissions"
    )
    if release_workflows:
        print(
            f"{everywhere}. And in {RELEASE_WORKFLOW}: the CLI is pinned unconditionally, "
            f"linuxdeploy is seeded by digest, every artifact is checksummed"
        )
    else:
        print(
            f"{everywhere}. No {RELEASE_WORKFLOW} here, so the three release rules ran on nothing"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
