#!/usr/bin/env python3
"""The AppImage leg's bundling tool is fetched by digest, or the leg stops.

**Why this exists.** F15. `dx bundle --package-types appimage` does not build
an AppImage itself; it runs `linuxdeploy`, which it downloads. In dioxus-cli 0.7.9
(`src/bundler/tools.rs:170-195`) `ensure_linuxdeploy` fetches that binary with `download_bytes`,
writes it 0755 and caches it forever -- while `ensure_nsis` (:126) and `ensure_wix` (:155), ten
lines up in the same file, go through `download_and_verify` against a pinned hash. The AppImage
path is the one of the three with no hash.

**The tag is mutable, and that is measured rather than argued.** The URL is
`.../releases/download/linuxdeploy/linuxdeploy-x86_64.AppImage` -- `linuxdeploy` is the release
tag. That release was created 2021-08-10 and published 2022-07-14; its four assets were last
updated **2024-07-29**. A fixed URL whose bytes changed two years after the release was published
is the definition of the thing a digest exists to pin.

**And the cache is the second half.** `ensure_linuxdeploy` returns early when the file is already
there, with no check of any kind. So one bad fetch -- or one write by anything else that can
reach the runner's home directory, which on a persistent self-hosted box is F13's whole subject
-- is executed by every RC cut afterwards, and no later run would notice. The jpc runner's copy
predates this script by three days.

**What this does about it.** Seeds the tools directory dx will look in, by digest:

- absent: download, verify against `PINNED`, write 0755 -- so dx finds it and never fetches.
- present and matching: leave it.
- present and NOT matching: **refuse**. Not overwrite. A cached binary whose bytes are not the
  pinned bytes is the finding's second half having already happened, and quietly replacing it
  would destroy the only evidence that it did.

**The belt this is the braces for**: `rc-bundle.yml` installs the CLI with
`--features no-downloads`, under which `ensure_linuxdeploy` bails instead of fetching. So if this
script does not run, or seeds the wrong directory, the leg fails loudly rather than falling back
to the unverified download. That is the whole reason the feature is worth a rebuild of the CLI.

**`NO_DOWNLOADS=1` does not do this, and looks like it does.** dioxus-cli has a
`CliSettings::prefer_no_downloads()` that honours that variable -- and `bundler/tools.rs` never
calls it. It is read by `tailwind.rs`, `esbuild.rs`, `wasm_opt.rs` and `wasm_bindgen.rs` only.
The three bundler tools test `cfg!(feature = "no-downloads")` directly, so the environment
variable is inert on exactly the path this row is about.

**How to grade this script**: `--self-test` runs every refusal against bytes that should trigger
it, including a known SHA-256 vector rather than only the script's own output -- F40's guard
passed a thirteen-mutation sweep while truncating every digest to eight characters, because
nothing there compared a digest to anything but another digest.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import shutil
import stat
import sys
import tempfile
import urllib.request
from pathlib import Path

# The release dioxus-cli fetches from, spelled the way it spells it
# (`LINUXDEPLOY_URL_BASE` in `src/bundler/tools.rs:25`). `linuxdeploy` is the tag.
URL_BASE = "https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy"

# Measured on 2026-09-20 against the asset the tag served, and against the copy the jpc runner
# had already cached on 2026-09-17 -- the two agreed, so nothing has been swapped under the tag
# in the window this project has been using it.
#
# ONE ARCHITECTURE ON PURPOSE. The RC's Linux leg is x86_64 and nothing else is built here, so
# pinning an aarch64 digest would be pinning a number nobody has checked. An unpinned arch is
# refused below rather than waved through, which is the Rule 1 shape: a new arch is a decision
# somebody makes, not a download that happens.
#
# To re-pin: download the asset, verify it by whatever means you trust, and replace the digest
# here in the same commit that says why.
PINNED = {
    "x86_64": "e762bea85c8eb0d4b3508d46e5c1f037f717d0f9303ae3b4aafc8b04991fa1ef",
}

# `Arch::linuxdeploy_arch` (`src/bundler/mod.rs:516`), for the machines this project builds on.
# Not the whole table: the entries below are the ones a runner here can report.
MACHINE_TO_ARCH = {
    "x86_64": "x86_64",
    "amd64": "x86_64",
    "aarch64": "aarch64",
    "arm64": "aarch64",
}

CHUNK = 1 << 20

# What `ensure_linuxdeploy` sets (`Permissions::from_mode(0o755)`), matched so that a seeded
# tools directory is indistinguishable from one dx filled in itself.
MODE = 0o755


def digest(path: Path) -> str:
    """SHA-256 of `path`, read in chunks because the AppImage is 13 MB."""
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(CHUNK):
            hasher.update(chunk)
    return hasher.hexdigest()


def tools_dir() -> Path:
    """Where dx caches its bundling tools, resolved the way dx resolves it.

    `Workspace::dioxus_data_dir` (`src/workspace.rs:600-613`) then `::tools_dir`: `DX_HOME` wins
    outright; otherwise `~/.dx` on macOS and Windows, and `dirs::data_dir()` -- which is
    `$XDG_DATA_HOME` or `~/.local/share` -- with `.dx` under it everywhere else. The leading dot
    is kept on the XDG path too; that is dx's spelling, not a typo here.

    `rc-bundle.yml` sets `DX_HOME` explicitly, so in CI only the first branch is taken. The rest
    exists so this script can be run and graded on a developer's box against the cache dx
    actually used, which is how the jpc runner's three-day-old copy was found.
    """
    if home := os.environ.get("DX_HOME"):
        return Path(home) / "tools"
    if sys.platform in ("darwin", "win32"):
        return Path.home() / ".dx" / "tools"
    data = os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local" / "share")
    return Path(data) / ".dx" / "tools"


def arch_name(machine: str) -> str:
    """dx's architecture spelling for `machine`, or a refusal."""
    arch = MACHINE_TO_ARCH.get(machine.lower())
    if arch is None:
        raise SystemExit(
            f"::error::this machine reports `{machine}`, which is not an architecture this "
            f"script knows how to name for linuxdeploy; add it to MACHINE_TO_ARCH and pin its "
            f"digest in the same commit"
        )
    if arch not in PINNED:
        raise SystemExit(
            f"::error::linuxdeploy for `{arch}` has no pinned digest, so there is nothing to "
            f"verify a download against; pin one in PINNED before building this architecture"
        )
    return arch


def fetch(url: str, into: Path) -> None:
    """Download `url` to `into`. No verification here -- that is the caller's, deliberately."""
    with urllib.request.urlopen(url) as response:  # a literal https URL, built from URL_BASE
        if response.status != 200:
            raise SystemExit(f"::error::{url} answered {response.status}")
        with into.open("wb") as handle:
            shutil.copyfileobj(response, handle)


def seed(directory: Path, arch: str, *, offline: bool = False) -> int:
    """Put a linuxdeploy matching `PINNED[arch]` in `directory`, or explain why there is not one."""
    expected = PINNED[arch]
    target = directory / f"linuxdeploy-{arch}.AppImage"

    if target.exists():
        found = digest(target)
        if found == expected:
            print(f"{target}: already present and matches the pinned digest")
            return 0
        print(
            f"::error::{target} is cached but its SHA-256 is {found}, not the pinned "
            f"{expected}. dx would execute this file without checking it. NOT overwritten: a "
            f"cached tool whose bytes are not the pinned bytes is evidence, and replacing it "
            f"destroys the evidence. Move it aside, work out where it came from, then re-run.",
            file=sys.stderr,
        )
        return 1

    if offline:
        print(f"::error::{target} is absent and this run may not download", file=sys.stderr)
        return 1

    url = f"{URL_BASE}/linuxdeploy-{arch}.AppImage"
    directory.mkdir(parents=True, exist_ok=True)
    # Downloaded beside the target and renamed only once it verifies, so an interrupted or
    # tampered fetch cannot leave something at the path dx will run.
    handle, staging_name = tempfile.mkstemp(dir=directory, prefix=".linuxdeploy-", suffix=".part")
    os.close(handle)
    staging = Path(staging_name)
    try:
        fetch(url, staging)
        found = digest(staging)
        if found != expected:
            print(
                f"::error::{url} served bytes whose SHA-256 is {found}, not the pinned "
                f"{expected}. The tag is mutable; either it moved or something is between this "
                f"runner and GitHub. Nothing was written.",
                file=sys.stderr,
            )
            return 1
        # 0755 exactly, which is what `ensure_linuxdeploy` writes. Not `mode | 0o111`: the
        # staging file comes from `mkstemp` at 0600, and OR-ing the execute bits onto that
        # gives 0711 -- runnable, and not the mode dx would have left behind.
        staging.chmod(MODE)
        staging.replace(target)
    finally:
        staging.unlink(missing_ok=True)

    print(f"{target}: downloaded and verified against the pinned digest")
    return 0


def self_test() -> int:
    """Grade every refusal against bytes that should trigger it. Thirteen cases."""
    global fetch  # the download is stubbed below; see the cases that need one
    failures = []

    # A KNOWN VECTOR FIRST, not the script's own output. F40's guard ran thirteen mutations
    # green while truncating every digest to eight characters, because every case there
    # compared a digest only to another digest -- under which a truncated hash is perfectly
    # self-consistent and nonsense to anything else. This line reds SHA-1, a truncation, and a
    # read that stops after the first byte.
    with tempfile.TemporaryDirectory() as raw:
        scratch = Path(raw)
        known = scratch / "abc"
        known.write_bytes(b"abc")
        if digest(known) != "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad":
            failures.append("SHA-256 of b'abc' is not the published value for it")

        # Chunked reads that stop after one chunk hash a prefix and call it the file.
        big = scratch / "big"
        big.write_bytes(b"\x00" * (CHUNK + 7) + b"tail")
        if digest(big) == digest_of_prefix(big, CHUNK):
            failures.append("a file longer than one chunk hashed the same as its first chunk")

    test_arch = "selftest"
    body = b"a linuxdeploy that is not one"
    PINNED[test_arch] = hashlib.sha256(body).hexdigest()
    MACHINE_TO_ARCH["selftestmachine"] = test_arch
    original_fetch = fetch
    try:
        # Present and matching: accepted, and left exactly as it was.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            target = directory / f"linuxdeploy-{test_arch}.AppImage"
            target.write_bytes(body)
            before = target.stat().st_mtime_ns
            if seed(directory, test_arch, offline=True) != 0:
                failures.append("a cached tool matching the pinned digest was refused")
            if target.read_bytes() != body or target.stat().st_mtime_ns != before:
                failures.append("a cached tool that matched was rewritten anyway")

        # Present and NOT matching: refused, and the evidence survives.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            target = directory / f"linuxdeploy-{test_arch}.AppImage"
            target.write_bytes(b"something else entirely")
            if seed(directory, test_arch, offline=True) == 0:
                failures.append("a cached tool whose digest does not match was accepted")
            if target.read_bytes() != b"something else entirely":
                failures.append(
                    "a mismatching cached tool was overwritten; that destroys the only evidence "
                    "that the cache had been written by something other than this script"
                )

        # Absent, offline: refused rather than skipped.
        with tempfile.TemporaryDirectory() as raw:
            if seed(Path(raw), test_arch, offline=True) == 0:
                failures.append("an absent tool was treated as fine when downloading was refused")

        # Absent, and the download verifies: written, and executable.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            fetch = lambda url, into: into.write_bytes(body)
            if seed(directory, test_arch) != 0:
                failures.append("a download matching the pinned digest was refused")
            target = directory / f"linuxdeploy-{test_arch}.AppImage"
            if not target.exists() or target.read_bytes() != body:
                failures.append("a verified download was not written")
            elif stat.S_IMODE(target.stat().st_mode) != MODE:
                failures.append(
                    f"a verified download landed at {stat.S_IMODE(target.stat().st_mode):04o} "
                    f"rather than {MODE:04o}; dx runs this file"
                )

        # Absent, and the download does NOT verify: nothing at the path dx would run, and no
        # staging file left behind for a later run to trip over.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            fetch = lambda url, into: into.write_bytes(b"swapped under the tag")
            if seed(directory, test_arch) == 0:
                failures.append("a download whose digest did not match was accepted")
            if (directory / f"linuxdeploy-{test_arch}.AppImage").exists():
                failures.append("a download that failed verification was written anyway")
            if list(directory.glob(".linuxdeploy-*")):
                failures.append("a failed download left its staging file in the tools directory")

        # An interrupted download must not leave a partial file at the target path either.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)

            def die(url, into):
                into.write_bytes(body[:5])
                raise OSError("connection reset")

            fetch = die
            try:
                seed(directory, test_arch)
            except OSError:
                pass
            if (directory / f"linuxdeploy-{test_arch}.AppImage").exists():
                failures.append("an interrupted download left a partial file where dx looks")
            if list(directory.glob(".linuxdeploy-*")):
                failures.append("an interrupted download left its staging file behind")
    finally:
        fetch = original_fetch
        del PINNED[test_arch]
        del MACHINE_TO_ARCH["selftestmachine"]

    # An architecture nobody pinned is a refusal, not a default to x86_64.
    MACHINE_TO_ARCH["unpinnedmachine"] = "unpinnedarch"
    try:
        arch_name("unpinnedmachine")
        failures.append("an architecture with no pinned digest was accepted")
    except SystemExit:
        pass
    finally:
        del MACHINE_TO_ARCH["unpinnedmachine"]

    try:
        arch_name("s390x")
        failures.append("a machine this script has no spelling for was accepted")
    except SystemExit:
        pass

    if arch_name("AMD64") != "x86_64":
        failures.append("a machine name was not matched case-insensitively")

    # DX_HOME wins outright; it is what `rc-bundle.yml` sets and the only branch CI takes.
    before = os.environ.get("DX_HOME")
    os.environ["DX_HOME"] = "/nonexistent/dx-home"
    if tools_dir() != Path("/nonexistent/dx-home/tools"):
        failures.append("DX_HOME did not decide the tools directory")
    if before is None:
        del os.environ["DX_HOME"]
    else:
        os.environ["DX_HOME"] = before

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: every refusal refuses, and the digest is graded against a known vector")
    return 0


def digest_of_prefix(path: Path, length: int) -> str:
    """SHA-256 of the first `length` bytes -- the self-test's stand-in for a one-chunk read."""
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        hasher.update(handle.read(length))
    return hasher.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="grade this script's refusals")
    parser.add_argument(
        "--offline",
        action="store_true",
        help="verify a cached linuxdeploy but refuse to fetch a missing one",
    )
    arguments = parser.parse_args()

    if arguments.self_test:
        return self_test()

    directory = tools_dir()
    arch = arch_name(platform.machine())
    print(f"linuxdeploy-{arch} in {directory}")
    return seed(directory, arch, offline=arguments.offline)


if __name__ == "__main__":
    sys.exit(main())
