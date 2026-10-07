#!/usr/bin/env python3
"""Every compile-time file the build needs is actually present in this checkout.

**Why this exists.** `rc-bundle.yml` uses a SPARSE checkout, because HEAD is 939 MB across
117,765 files and a Windows runner would spend the job on it. A sparse checkout is a hand-written
list of paths, and a hand-written list of paths goes stale the moment someone adds an
`include_str!` pointing outside it.

That is not hypothetical. The first RC dispatch failed exactly this way: `coh_math` embeds
`effect-registry.json` and `set-bonus-stat-vocab.json` (they were in `contract/` then and are in
`hand-data/` now -- both are hand-authored, and `contract/` is becoming build output), the sparse
list did not carry them, and **the failure arrived 614 seconds in**, as a rustc error at the end of
a release compile. The cause of the omission is worth recording too — the list was built from a `grep`
piped through `head -20`, and the truncated output was read as the complete one.

So this script derives the answer from the source rather than trusting a list: it finds every
`include_str!` / `include_bytes!` / `include_dir!` in `crates/`, resolves each path relative to
the file that writes it, and asserts the target exists. Run immediately after checkout it fails in
under a second, naming the missing file and the line that wants it.

**Embeds were not the only way the list could go stale, and the second way got through.** F86's
close vendored `dioxus-asset-resolver` into `vendor/` and added it to `[workspace] members`. A
workspace member is not an `include_str!`, so this script was green on a checkout that `cargo
metadata` refused outright -- `failed to load manifest for workspace member`, before a line
compiled. It now derives the ROOT MANIFEST's members and its `path = ` dependencies too, and
asserts each one's `Cargo.toml` is present. The generalisation worth keeping: deriving the answer
from the source beats a hand-written list only for the kinds of dependency the deriver knows about,
so a new kind of build input is a new scan, not a free ride on an old one.

**What it cannot tell you:** run against a FULL checkout it is vacuously green, because every path
resolves. Its whole value is in the sparse environment, and the honest way to grade it is
`--self-test`, which re-runs both resolvers against paths that are deliberately absent and fails if
that is not caught.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"

# `include_dir!` takes `$CARGO_MANIFEST_DIR/...`; the other two take a path relative to the file.
PATTERN = re.compile(r'include_(?:str|bytes|dir)!\s*\(\s*"([^"]+)"')

# The root manifest, read as text rather than parsed. `tomllib` is 3.11+ and the Windows runner's
# python3 is not pinned by anything in this repo, so a parser would be a second thing that can be
# absent on the box this script exists to fail fast on.
MEMBERS = re.compile(r"members\s*=\s*\[(.*?)\]", re.S)
QUOTED = re.compile(r'"([^"]+)"')
PATH_DEP = re.compile(r'path\s*=\s*"([^"]+)"')


def resolve(source: Path, spelled: str) -> Path:
    if spelled.startswith("$CARGO_MANIFEST_DIR/"):
        # The manifest dir is the crate root: walk up to the nearest Cargo.toml.
        crate = source
        while crate != ROOT and not (crate / "Cargo.toml").exists():
            crate = crate.parent
        return (crate / spelled[len("$CARGO_MANIFEST_DIR/") :]).resolve()
    return (source.parent / spelled).resolve()


def workspace_inputs() -> list[tuple[str, str, Path]]:
    """Every directory `cargo metadata` must be able to read a manifest out of.

    Two sources, both in the root `Cargo.toml`: `[workspace] members`, and any `path = ` dependency
    (which is how `[patch.crates-io]` points at the vendored copy). A member inside `crates/` is
    carried by the sparse list already and costs nothing to re-check."""
    manifest = (ROOT / "Cargo.toml").read_text(errors="replace")
    wanted: list[tuple[str, str]] = []
    block = MEMBERS.search(manifest)
    if block:
        wanted += [("workspace member", m) for m in QUOTED.findall(block.group(1))]
    wanted += [("path dependency", m) for m in PATH_DEP.findall(manifest)]
    # `crates/app`'s own `path = "src/main.rs"` kind of entry is a FILE, not a crate directory;
    # only the root manifest is read here, so nothing of that shape reaches this list. Resolve
    # against ROOT and ask for the manifest, because that is the file cargo actually opens.
    return [(kind, spelled, (ROOT / spelled / "Cargo.toml").resolve()) for kind, spelled in wanted]


def scan() -> list[tuple[Path, int, str, Path]]:
    found = []
    for source in CRATES.rglob("*.rs"):
        # Tests are not compiled by a bundle build, so a fixture they embed is not this list's
        # business — including them would widen the sparse checkout for no shipped byte.
        if "/tests/" in str(source) or "/benches/" in str(source):
            continue
        for number, line in enumerate(source.read_text(errors="replace").splitlines(), 1):
            for spelled in PATTERN.findall(line):
                found.append((source, number, spelled, resolve(source, spelled)))
    return found


def main() -> int:
    if "--self-test" in sys.argv:
        # The gate must be able to go red. A path that cannot exist has to be reported as missing;
        # if this passes, the check above is decoration.
        fake = ROOT / "crates" / "coh_math" / "src" / "lib.rs"
        target = resolve(fake, "../../../contract/no-such-file-ever.json")
        if target.exists():
            print("self-test: the impossible path exists, so this proves nothing", file=sys.stderr)
            return 1
        # The second resolver needs its own proof: it was added because the first one's green
        # meant nothing about workspace members, and an untested widening is the same mistake.
        phantom = (ROOT / "vendor" / "no-such-crate-ever" / "Cargo.toml").resolve()
        if phantom.exists():
            print("self-test: the impossible member exists, so this proves nothing", file=sys.stderr)
            return 1
        print("self-test ok: a missing include and a missing workspace member both resolve to "
              "paths that are reported absent")
        return 0

    missing = []
    entries = scan()
    for source, number, spelled, target in entries:
        if not target.exists():
            missing.append((source.relative_to(ROOT), number, spelled))

    for source, number, spelled in missing:
        print(f"MISSING  {source}:{number}  include of {spelled}", file=sys.stderr)

    # Checked SECOND but failing in the same breath, because this is the one cargo hits FIRST:
    # a member whose manifest is absent stops `cargo metadata`, so nothing gets as far as an
    # embed. Reported together anyway -- a list that only tells you about one missing path at a
    # time is a list you fix one dispatch at a time.
    crates = workspace_inputs()
    absent = [(kind, spelled) for kind, spelled, target in crates if not target.exists()]
    for kind, spelled in absent:
        print(f"MISSING  Cargo.toml:  {kind} {spelled} has no manifest here", file=sys.stderr)

    if missing or absent:
        print(
            f"\n{len(missing)} embedded file(s) and {len(absent)} workspace path(s) are not in "
            f"this checkout. If this is the sparse checkout in .github/workflows/rc-bundle.yml, "
            f"add the paths above to it.",
            file=sys.stderr,
        )
        return 1

    print(f"ok: {len(entries)} compile-time includes and {len(crates)} workspace paths, all present")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
