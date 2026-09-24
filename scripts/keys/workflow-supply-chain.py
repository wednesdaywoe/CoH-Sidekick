#!/usr/bin/env python3
"""Nothing that builds or publishes this project comes from an unpinned source.

Eleven rules, one question: when CI produces an artifact, can somebody say where every byte
of it came from? Seven are about the inputs (F14 twice, F43, F42, F15, F13, F03), one is about
the output (F40), one is about the permissions the whole thing runs under (F59), one is about a
setting that silently is not applied at all (F13 again), and the eleventh is about WHO can start
a job at all (F03 again) -- the one rule here that is not about an artifact's bytes, kept in this
file because it reads the same four workflows and answers with them.

Two of them are about the same tool and the difference is worth stating once: `rc-bundle.yml`
BUILDS `dx` from a pinned source, because what it cuts is the artifact users install;
`ci.yml`'s `web` job DOWNLOADS the prebuilt one and checks it against a digest, because
building it cost 17m15s of a 21m25s job and kept that job on a personal machine. Different
acquisitions, same requirement: a name is not a pin.

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
worse than no pin, because it passes review. The rule is therefore: no `command -v dx` anywhere,
every `cargo install dioxus-cli` names `--locked`, `--version` and `--features no-downloads`,
and all of them name the SAME version — three legs that quietly drift apart is the failure a
single grep would miss.

**"Anywhere" is new, and it is F42's scope corrected rather than widened on principle.** This
rule named `rc-bundle.yml` by filename because `ci.yml`'s `web` job was argued exempt: "what
this job produces is a wasm test build nobody installs." That is true of the consequence and
false of the mechanism, and the measurement is what settled it — jpc carried
`dioxus 0.7.9 (bfcc111)` while the released 0.7.9 is `(3e43ffa)`. Two different binaries
answering to the version this repository pins, with no step recording which one graded a pull
request. F14's own closing note had started to say this about `ci.yml` and the sentence was
never finished.

**The seventh rule is the tool that tool runs (F15).** `dx bundle --package-types appimage` shells
out to `linuxdeploy`, which dioxus-cli fetches from a mutable release tag with no hash and caches
forever. `rc-linuxdeploy.py` seeds that cache by digest, and `--features no-downloads` above turns
a missed seed into a refusal instead of a silent unverified download. So: every AppImage bundle in
the release workflow must have a seed step before it — "before" for the same reason the checksum
rule says "before".

**The eighth rule is a value that is not there (F13).** A job-level `env:` entry reading
`${{ runner.temp }}` does not fail -- GitHub does not resolve the `runner` context at that
level, so it expands to the EMPTY STRING and the variable ships with a hole in it. F13's
`CARGO_TARGET_DIR: ${{ runner.temp }}/rc-target` was written that way and would have pointed
the release build at `/rc-target`, at the filesystem root, with no error anywhere. The same
holds for `job`, `steps` and `env`. Caught by reading GitHub's context-availability table
rather than by running the workflow, which is a thing this repository cannot do -- so the
rule is here precisely because the ordinary way of finding it is closed.

**The ninth rule is the machine's own crate registry (F13).** The two release legs that run on
persistent boxes shared their `$CARGO_HOME` with `ci.yml`, and F13 said for months that nothing
below the target directory was closable by editing this file. Measured on cargo 1.96.1, that was
wrong. An extracted registry source tree is trusted forever -- modern cargo writes no
`.cargo-checksum.json`, only `.cargo-ok` holding `{"v":1}` -- so source edited in place compiled
with a clean exit and reached the output binary, and the job-local `CARGO_TARGET_DIR` the row had
already landed did nothing about it, because the poison sits upstream of the target directory.
Nor does the lockfile help a cache: a `.crate` repackaged with tampered source, whose sha256 does
not match `Cargo.lock`, extracted and compiled `--offline` without a word. **Verification is
download-time only.** An empty `$CARGO_HOME` therefore refuses what a populated one accepts, so
the rule is that every self-hosted leg sets `CARGO_HOME` under `$RUNNER_TEMP` -- which moves the
trust root off the box and onto `Cargo.lock`, a file in this repository. Scoped per job rather
than per file, because one leg carrying the line would otherwise vouch for a leg that does not.

**The tenth rule is a tool that arrives already built (F03).** `ci.yml`'s `web` job fetches the
prebuilt `dx` upstream publishes for the version this repository pins. That is not the cache
shape F14 forbids -- a cache has no lockfile, no version and no provenance, and a release asset
at an immutable tag has all three -- but it IS a download, and F15 measured what a download is
worth without a hash: a fixed linuxdeploy URL whose asset was replaced two years after the
release was published. So the rule is that a step fetching a dioxus release asset names a
64-character digest and checks it in that same step. Scoped to the step because the `env:`
carrying the URL and the `run:` doing the verify are two halves of one, and a verify one step
later guards nothing. Sixty-four characters exactly: F40's own guard passed a thirteen-mutation
sweep while truncating every digest to eight.

**How to grade this script**, since a passing run against a correct tree says nothing about
whether the check works: `--self-test` re-runs every rule against text that deliberately breaks
it and fails if any is let through.
"""

from __future__ import annotations

import os
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

# A `dx` that arrives PREBUILT rather than built, which `ci.yml`'s `web` job does because
# building it cost 17m15s of a 21m25s job. 64 hex characters exactly: a digest truncated to
# eight is the trap F40's own guard fell into, and `sha256sum -c` refuses a short line anyway,
# so the rule wants the full one written down where review can see it.
DX_ASSET = re.compile(r"https://github\.com/DioxusLabs/dioxus/releases/download/\S+")
SHA256_LITERAL = re.compile(r"\b[0-9a-f]{64}\b")
SHA256_CHECK = re.compile(r"\bsha256sum\s+(?:-c\b|--check\b)")

# Step boundaries. Six spaces is where `jobs.<id>.steps` sits in both trees' workflows; matched
# by indentation rather than parsed, for `job_env_uses_step_context`'s reason one rule down.
STEP_START = re.compile(r"^ {6}-\s+\S")


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


def dx_presence_tests(text: str) -> list[tuple[int, str]]:
    """Every `command -v dx`, in any workflow.

    F42 scoped this rule to `rc-bundle.yml` on an argument that has since been measured false.
    `ci.yml`'s `web` job was exempted because "what this job produces is a wasm test build
    nobody installs" — true of the consequence, and not of the mechanism. jpc carried
    `dioxus 0.7.9 (bfcc111)` while the released 0.7.9 is `(3e43ffa)`: two different binaries
    answering to the pinned version, and which one graded a pull request was whichever the box
    happened to have. A tool nothing records is the state the rule exists to end, whether or not
    its output ships, so the scope is now every workflow.
    """
    return [
        (number, "`command -v dx` makes the pin conditional on what is on the box")
        for number, line in enumerate(text.splitlines(), start=1)
        if DX_PRESENCE_TEST.search(without_comment(line))
    ]


def build_tool_unpinned(text: str) -> list[tuple[int, str]]:
    """Every way the release workflow's `dx` could be something other than the version it names.

    Two shapes, because they fail differently. A missing flag makes the pin incomplete. Two legs
    naming two versions makes it unanswerable which one cut the artifact, which is the same
    question this whole file asks. The third shape, `command -v dx`, moved to
    `dx_presence_tests` when it stopped being a rule about this file alone.
    """
    found = []
    versions: dict[str, int] = {}

    for number, line in enumerate(text.splitlines(), start=1):
        bare = without_comment(line)
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


def dx_downloads_unverified(text: str) -> list[tuple[int, str]]:
    """Steps that fetch a prebuilt `dx` without pinning it to a digest and checking it.

    A release-asset URL names a version. Only a hash says the bytes behind that name did not
    change, and F15 MEASURED that substitution one workflow over: a fixed linuxdeploy URL whose
    asset was replaced two years after the release was published. So an unhashed fetch is a
    mutable input wearing a version number, and this rule is what keeps `ci.yml`'s `web` job
    from drifting back into one.

    Scoped to the STEP. A digest three steps away does not guard this download, and the `env:`
    that carries the URL and the `run:` that verifies it are two halves of one step -- which is
    also why this cannot be a per-line rule.
    """
    steps: list[list[tuple[int, str]]] = [[]]
    for number, line in enumerate(text.splitlines(), start=1):
        if STEP_START.match(line):
            steps.append([])
        steps[-1].append((number, without_comment(line)))

    found = []
    for step in steps:
        hits = [number for number, line in step if DX_ASSET.search(line)]
        if not hits:
            continue
        body = "\n".join(line for _, line in step)
        if not SHA256_LITERAL.search(body):
            found.append((hits[0], "fetches a prebuilt `dx` and names no 64-character digest"))
        elif not SHA256_CHECK.search(body):
            found.append((hits[0], "names a digest for the `dx` it fetches and never checks it"))
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


# Contexts GitHub does NOT resolve inside a job-level `env:` block. Its context-availability
# table allows `github`, `needs`, `strategy`, `matrix`, `vars`, `secrets` and `inputs` there;
# everything below is admitted only from `jobs.<id>.steps.env` downwards.
STEP_ONLY_CONTEXTS = ("runner", "job", "steps", "env")
JOB_ENV_EXPR = re.compile(r"\$\{\{\s*(" + "|".join(STEP_ONLY_CONTEXTS) + r")\." )


def job_env_uses_step_context(text: str) -> list[tuple[int, str]]:
    """Job-level `env:` entries referencing a context that only exists further down.

    SECURITY_AUDIT.md F13 nearly shipped on this. `CARGO_TARGET_DIR: ${{ runner.temp }}/rc-target`
    in a job-level `env:` block does not fail -- **it expands to the empty string**, so the
    release build would have been pointed at `/rc-target`, an absolute path at the filesystem
    root. There is no error, no warning and no log line saying a context was dropped; the only
    symptom is a value with a hole in it, which is the same class as F36's `beforeSend` and
    F40's truncated digest -- a thing that looks configured and is not.

    Scoped by INDENTATION rather than by parsing, because the distinction the rule needs is
    exactly a structural one and a regex over the whole file cannot make it: a job's `env:` sits
    at four spaces under `jobs: <id>:`, a step's at eight or more under `- name:`. Four spaces is
    the job level and is refused; deeper is a step and is fine. `$RUNNER_TEMP` -- the shell's own
    copy -- is the fix, and it is not matched here because it is not an expression.
    """
    found = []
    in_job_env = False
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        indent = len(line) - len(line.lstrip())
        if indent == 4 and stripped == "env:":
            in_job_env = True
            continue
        # Any line back at or above the job-key level closes the block.
        if in_job_env and indent <= 4:
            in_job_env = False
        if in_job_env and (match := JOB_ENV_EXPR.search(line)):
            found.append((number, match.group(1)))
    return found


# A job's `runs-on:` naming a persistent box, and the isolation that box's legs must declare.
# Matched on the assignment written into `$GITHUB_ENV`, which is the only spelling that works:
# rule 8 above is why the `${{ runner.temp }}` form is not an alternative.
SELF_HOSTED = re.compile(r"^\s*runs-on:.*\bself-hosted\b")
CARGO_HOME_ISOLATED = re.compile(r"CARGO_HOME=\$(?:RUNNER_TEMP|\{RUNNER_TEMP\})/")


def release_jobs(text: str) -> list[tuple[str, int, str]]:
    """Split a workflow into `(job id, line number, body)`, by the indentation of the job key.

    Same structural reason as rule 8: a job is a two-space key under `jobs:`, and the rule below
    is about what a PARTICULAR job does, so a regex over the whole file cannot answer it -- one
    leg carrying the line would vouch for the two that do not.
    """
    jobs, current, start, body = [], None, 0, []
    in_jobs = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.rstrip() == "jobs:":
            in_jobs = True
            continue
        if not in_jobs:
            continue
        stripped = line.strip()
        indent = len(line) - len(line.lstrip())
        if stripped and not stripped.startswith("#") and indent == 2 and stripped.endswith(":"):
            if current:
                jobs.append((current, start, "\n".join(body)))
            current, start, body = stripped[:-1], number, []
            continue
        if current:
            body.append(line)
    if current:
        jobs.append((current, start, "\n".join(body)))
    return jobs


def unisolated_cargo_home(text: str) -> list[tuple[int, str]]:
    """Release legs on persistent boxes that build out of the machine's shared `$CARGO_HOME`.

    SECURITY_AUDIT.md F13. The row said for months that nothing below the target directory was
    closable by editing this file. Measured on cargo 1.96.1, that was wrong, and the shared
    registry was the sharpest thing in the list:

      * an extracted source tree is trusted forever -- modern cargo writes no
        `.cargo-checksum.json`, only `.cargo-ok` holding `{"v":1}`, so there is not even a
        per-file hash to disagree with. Source edited under `registry/src/` compiled clean and
        reached the output binary, and a job-local `CARGO_TARGET_DIR` did nothing about it
        because the poison sits upstream of the target directory;
      * verification is DOWNLOAD-time only -- a `.crate` repackaged with tampered source, whose
        sha256 does not match `Cargo.lock`, extracted and compiled `--offline` without a word;
      * an EMPTY `$CARGO_HOME` refuses a crate the lockfile disagrees with.

    So the isolation is what makes `Cargo.lock` load-bearing, and `Cargo.lock` is in the
    repository rather than on the box. That is the whole value: the trust root moves from
    "whoever can write this machine's home directory" to "whoever can push". It also takes
    `$CARGO_HOME/config.toml` out of the build, which is not a lesser door -- `rustflags` and
    `linker` set there reach the compiler, measured the same day.

    Scoped to jobs whose `runs-on` names `self-hosted`, so a future persistent leg is covered the
    day it is added and the ephemeral Windows leg is not asked for something it does not need.
    """
    found = []
    for name, number, body in release_jobs(text):
        if not any(SELF_HOSTED.match(line) for line in body.splitlines()):
            continue
        if not CARGO_HOME_ISOLATED.search(body):
            found.append((number, name))
    return found


FORK_GUARD = re.compile(
    r"github\.event\.pull_request\.head\.repo\.full_name\s*==\s*github\.repository"
)
PULL_REQUEST_REACHABLE = re.compile(r"\bpull_request\b")


def fork_reachable_self_hosted(text: str, repo_is_private: bool) -> list[tuple[int, str, str]]:
    """Self-hosted jobs a stranger's pull request can start.

    SECURITY_AUDIT.md F03, and the reason that row can be ACCEPTED rather than left open. The
    filed finding is closed -- `pull_request` carries no branch filter, so a public repository
    would have let any GitHub user run arbitrary code on the mac mini, jpc and espresso, and all
    but one PR-reachable job is hosted now. The residual is `rust`, which stays on the mac
    because it is core-bound: 9m32s on 12 cores is 114 core-minutes, and a 2-core hosted runner
    measured 60.3 minutes before being cancelled.

    The row's exit condition is "`rust` moves to `ubuntu-latest` when this repository goes
    public". **A condition nobody can fire is not an exit condition**, which is the trap
    `road-to-1.0.md` records against F58: its first one triggered on a signal that would have
    stayed true forever. So this is the firing mechanism rather than a sentence.

    Two rules, and which one applies is the repository's own visibility:

      * **private** -- a PR-reachable self-hosted job must carry the fork guard in its `if:`, so
        a fork's pull request skips it and the only caller left is somebody who can already
        push here.
      * **public** -- there is no guard that makes this acceptable, because `pull_request` from
        a fork is then any GitHub account. The job must be hosted. This is the clause that
        fires: the day the repository's visibility flips, CI goes red naming `rust`, and F03
        stops being accepted without anybody having to remember it was.

    Visibility comes from the caller, because a file in the repository cannot know it. CI passes
    `github.event.repository.private`; a bare local run has no event and assumes private, which
    is the reading that checks MORE (a missing guard is still reported) rather than less.
    """
    if not PULL_REQUEST_REACHABLE.search(text.split("jobs:", 1)[0]):
        return []
    found = []
    for name, number, body in release_jobs(text):
        if not any(SELF_HOSTED.match(line) for line in body.splitlines()):
            continue
        # A job with no `pull_request` arm at all cannot be reached by one. `mutants-diff` is
        # the standing example -- `schedule` and `workflow_dispatch` only, both of which need
        # write access or are the repository's own.
        if not PULL_REQUEST_REACHABLE.search(body):
            continue
        if not repo_is_private:
            found.append((number, name, "runs on a self-hosted box and this repository is "
                                        "PUBLIC, so any GitHub account can start it from a fork "
                                        "-- no `if:` guard fixes that; move it to a hosted "
                                        "runner (SECURITY_AUDIT.md F03's exit condition)"))
        elif not FORK_GUARD.search(body):
            found.append((number, name, "runs on a self-hosted box on `pull_request` with no "
                                        "fork guard, so a fork's PR executes on it"))
    return found


def self_test() -> int:
    """Grade every rule against text that breaks it. A check nobody has seen fail is a wish."""
    failures = []

    # The F13 near-miss, in the shape it was actually written in.
    job_env = (
        "jobs:\n"
        "  macos:\n"
        "    runs-on: [self-hosted, macmini]\n"
        "    env:\n"
        "      CARGO_TARGET_DIR: ${{ runner.temp }}/rc-target\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    if not job_env_uses_step_context(job_env):
        failures.append("`runner` in a job-level env: was not caught; it expands to empty")

    # The same context one level down, where GitHub does resolve it. Refusing this would send
    # somebody to break a step that works.
    step_env = (
        "jobs:\n"
        "  macos:\n"
        "    steps:\n"
        "      - name: build\n"
        "        env:\n"
        "          CARGO_TARGET_DIR: ${{ runner.temp }}/rc-target\n"
    )
    if job_env_uses_step_context(step_env):
        failures.append("a step-level env: using `runner` was refused; that spelling is legal")

    # The fix, which must not read as the defect: `$RUNNER_TEMP` is a shell variable, not an
    # expression, and never passes through the expression evaluator at all.
    shell_var = (
        "jobs:\n"
        "  macos:\n"
        "    env:\n"
        "      CARGO_TARGET_DIR: $RUNNER_TEMP/rc-target\n"
    )
    if job_env_uses_step_context(shell_var):
        failures.append("`$RUNNER_TEMP` was reported as an unavailable context; it is a shell var")

    # A context that IS available at job level. Flagging it would be a false refusal.
    allowed = (
        "jobs:\n"
        "  macos:\n"
        "    env:\n"
        "      LABEL: ${{ github.ref_name }}\n"
    )
    if job_env_uses_step_context(allowed):
        failures.append("`github` in a job-level env: was refused; that context is available")

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
    if not dx_presence_tests("      - run: command -v dx >/dev/null 2>&1 || " + good_install):
        failures.append("a `command -v dx` short-circuit was not caught")
    if build_tool_unpinned("      - run: command -v dx >/dev/null 2>&1 || " + good_install):
        failures.append("the presence test was still reported by the rule it moved out of")
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
    if dx_presence_tests("      # the `command -v dx ||` that was here is F42\n"):
        failures.append("a comment describing the short-circuit was read as the short-circuit")

    # The tenth rule: a prebuilt `dx`, and the digest that is the actual pin.
    dx_url = (
        "https://github.com/DioxusLabs/dioxus/releases/download/v0.7.9/"
        "dx-x86_64-unknown-linux-gnu.tar.gz"
    )
    digest = "3b132551b480bc96f938f9f0d37936ee1190f994977539dcc347eaf38540d005"
    pinned = (
        "      - name: dioxus-cli 0.7.9, pinned by digest\n"
        "        env:\n"
        f"          DX_URL: {dx_url}\n"
        f"          DX_SHA256: {digest}\n"
        "        run: |\n"
        '          curl -sSfL -o "$staging/dx.tar.gz" "$DX_URL"\n'
        '          printf \'%s  %s\\n\' "$DX_SHA256" "$staging/dx.tar.gz" | sha256sum -c -\n'
    )
    if dx_downloads_unverified(pinned):
        failures.append("a digest-pinned prebuilt dx fetch was reported")
    if not dx_downloads_unverified(pinned.replace(f"          DX_SHA256: {digest}\n", "")):
        failures.append("a prebuilt dx fetch naming no digest was not caught")
    # F40's own guard passed thirteen mutations while truncating every digest to eight
    # characters, so the short digest is a case rather than a footnote.
    if not dx_downloads_unverified(pinned.replace(digest, digest[:8])):
        failures.append("a dx digest truncated to eight characters was read as a digest")
    if not dx_downloads_unverified(pinned.replace(" | sha256sum -c -", "")):
        failures.append("a dx digest that is named and never checked was not caught")
    # The step is the unit: a verify in the NEXT step does not guard this download.
    split = pinned.replace(
        '          printf \'%s  %s\\n\' "$DX_SHA256" "$staging/dx.tar.gz" | sha256sum -c -\n',
        '      - run: printf \'%s\\n\' "$DX_SHA256" | sha256sum -c -\n',
    )
    if not dx_downloads_unverified(split):
        failures.append("a verify one step later was read as guarding the fetch before it")
    if dx_downloads_unverified("      - run: cargo install dioxus-cli --locked\n"):
        failures.append("an install-from-source was reported by the prebuilt-download rule")

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

    # Rule 11 (F03). Four cases, because the rule has two arms and each has a way of being
    # wrong in the direction that reads as green.
    pr_workflow = "on:\n  push:\n  pull_request:\n\njobs:\n"
    guarded = (
        "  rust:\n"
        "    if: >-\n"
        "      github.event_name == 'push' ||\n"
        "      (github.event_name == 'pull_request' &&\n"
        "      github.event.pull_request.head.repo.full_name == github.repository)\n"
        "    runs-on: [self-hosted, macmini]\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    unguarded = (
        "  rust:\n"
        "    if: github.event_name == 'push' || github.event_name == 'pull_request'\n"
        "    runs-on: [self-hosted, macmini]\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    # Reachable only by triggers that need write access. `mutants-diff` is this shape, and
    # reporting it would send somebody to fix a job no fork can start.
    dispatch_only = (
        "  mutants-diff:\n"
        "    if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'\n"
        "    runs-on: [self-hosted, mutants]\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    if fork_reachable_self_hosted(pr_workflow + guarded, repo_is_private=True):
        failures.append("a fork-guarded self-hosted job was reported while the repo is private")
    if not fork_reachable_self_hosted(pr_workflow + unguarded, repo_is_private=True):
        failures.append("an unguarded self-hosted job on `pull_request` was not caught")
    # The clause that makes F03's exit condition fire. The guard is present and is NOT enough.
    if not fork_reachable_self_hosted(pr_workflow + guarded, repo_is_private=False):
        failures.append(
            "a self-hosted job survived the public-repository arm because it carries a fork "
            "guard -- that guard is what stops a FORK, and on a public repository a fork is "
            "any GitHub account, which is F03 as filed"
        )
    if fork_reachable_self_hosted(pr_workflow + dispatch_only, repo_is_private=False):
        failures.append("a job with no pull_request arm was reported as fork-reachable")

    if declares_permissions("name: CI\non:\n  push:\n\njobs:\n  build:\n"):
        failures.append("a workflow with no permissions block was reported as having one")
    if not declares_permissions("name: CI\non:\n  push:\n\npermissions:\n  contents: read\n"):
        failures.append("a top-level permissions block was not recognised")
    if declares_permissions("jobs:\n  build:\n    permissions:\n      contents: read\n"):
        failures.append(
            "a job-level permissions block was accepted as top-level; it binds one job and "
            "leaves the rest on the repository default"
        )

    # Rule 9 (F13). One persistent leg isolated, one not: the case a grep over the whole file
    # passes, because the isolated leg's line vouches for the leg that has none.
    isolated_leg = (
        "  linux:\n"
        "    runs-on: [self-hosted, jpc]\n"
        "    steps:\n"
        "      - name: build from a job-local cargo home\n"
        "        run: |\n"
        '          echo "CARGO_HOME=$RUNNER_TEMP/rc-cargo" >> "$GITHUB_ENV"\n'
    )
    bare_leg = (
        "  macos:\n"
        "    runs-on: [self-hosted, macmini]\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    ephemeral_leg = (
        "  windows:\n"
        "    runs-on: windows-latest\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    if unisolated_cargo_home("jobs:\n" + isolated_leg):
        failures.append("an isolated self-hosted leg was reported as building out of the box")
    if not unisolated_cargo_home("jobs:\n" + bare_leg):
        failures.append(
            "a self-hosted leg with no CARGO_HOME isolation was not caught; it cuts the "
            "release out of a registry ordinary CI can write"
        )
    if unisolated_cargo_home("jobs:\n" + ephemeral_leg):
        failures.append("the ephemeral Windows leg was asked to isolate a home it does not share")
    # The case the whole-file grep gets wrong, and the reason this rule splits by job at all.
    both = "jobs:\n" + isolated_leg + bare_leg
    caught = unisolated_cargo_home(both)
    if len(caught) != 1 or caught[0][1] != "macos":
        failures.append(
            "one leg's isolation was read as covering a second leg that has none; this rule "
            "exists to tell two jobs apart"
        )
    # `${{ runner.temp }}` is the spelling rule 8 refuses, so it must not satisfy this one
    # either -- it expands to the empty string and the home lands at /rc-cargo.
    hollow = (
        "  linux:\n"
        "    runs-on: [self-hosted, jpc]\n"
        "    env:\n"
        "      CARGO_HOME: ${{ runner.temp }}/rc-cargo\n"
        "    steps:\n"
        "      - uses: actions/checkout@v4\n"
    )
    if not unisolated_cargo_home("jobs:\n" + hollow):
        failures.append(
            "the hollow `${{ runner.temp }}` spelling satisfied the isolation rule; it expands "
            "to the empty string and rule 8 refuses it"
        )

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: all eleven rules refuse what they are meant to refuse")
    return 0


def repo_is_private() -> bool:
    """Is this repository private, as the caller sees it?

    `REPO_IS_PRIVATE` is set by CI from `github.event.repository.private`. Absent -- a local run,
    where there is no event to read -- it assumes private, because that is the reading under
    which the OTHER clause still applies: a missing fork guard is reported either way, and only
    the stricter public rule needs the fact to be known. Assuming public locally would red the
    tree on every developer's machine for a condition that is not true yet.
    """
    raw = os.environ.get("REPO_IS_PRIVATE")
    return raw is None or raw.strip().lower() not in ("false", "0", "no")


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
        for number, why in dx_presence_tests(text):
            problems.append(
                f"{path.relative_to(ROOT)}:{number}: {why}, so no step records which `dx` ran"
            )
        for number, why in dx_downloads_unverified(text):
            problems.append(
                f"{path.relative_to(ROOT)}:{number}: this step {why}, so the version in the URL "
                f"is a name and nothing says the bytes behind it did not change"
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
            for number, name in unisolated_cargo_home(text):
                problems.append(
                    f"{path.relative_to(ROOT)}:{number}: job `{name}` runs on a persistent box "
                    f"and never sets CARGO_HOME under $RUNNER_TEMP, so the release is built out "
                    f"of a registry every ordinary CI run can write — and cargo verifies a crate "
                    f"against Cargo.lock on DOWNLOAD only, never on the way out of the cache"
                )
        for number, name, why in fork_reachable_self_hosted(text, repo_is_private()):
            problems.append(f"{path.relative_to(ROOT)}:{number}: job `{name}` {why}")
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
        for number, context in job_env_uses_step_context(text):
            problems.append(
                f"{path.relative_to(ROOT)}:{number}: a job-level `env:` reads `{context}.`, "
                f"which GitHub does not resolve there — it expands to the empty string and the "
                f"variable ships with a hole in it. Use the shell's copy in a `run:` "
                f"(`$RUNNER_TEMP`) or move the `env:` onto the step"
            )

    for problem in problems:
        print(f"::error::{problem}", file=sys.stderr)
    if problems:
        return 1
    # Four of the nine rules only have anything to say about `rc-bundle.yml`, and this script
    # is mirrored into a repository that does not have one. Reporting them as satisfied there
    # is a green that means nothing -- the shape that let F86's first patch pass while serving
    # nothing -- so the summary says which half actually ran.
    everywhere = (
        f"{len(files)} workflows: every third-party action names a commit, no cache holds a "
        f"build tool, no `dx` comes off the box or off an unhashed download, no install "
        f"falls off the lockfile, no job-level env: reads a context that is not there, "
        f"every workflow states its permissions"
    )
    if release_workflows:
        print(
            f"{everywhere}. And in {RELEASE_WORKFLOW}: the CLI is pinned unconditionally, "
            f"linuxdeploy is seeded by digest, every artifact is checksummed, every "
            f"self-hosted leg builds from its own cargo home"
        )
    else:
        print(
            f"{everywhere}. No {RELEASE_WORKFLOW} here, so the four release rules ran on nothing"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
