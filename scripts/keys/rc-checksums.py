#!/usr/bin/env python3
"""Write a SHA256SUMS manifest beside an RC artifact, and put the digests in the run's record.

**Why this exists.** F40. An RC artifact was handed to a tester as a zip from
an Actions run with nothing to check it against: no checksum, no signature, no notarization, no
provenance. Of those four, exactly one can be done from inside this repository today, and this is
it — the other three need a certificate somebody has to buy, and the row says so rather than
pretending otherwise.

A checksum is not a signature and does not claim to be. What it buys is narrow and real: a tester
who downloads a bundle twice, or is handed one by somebody else, can tell whether the bytes are
the bytes this run produced. The digests also go to `$GITHUB_STEP_SUMMARY`, so the record lives in
the run — which nobody can quietly edit — and not only inside the zip it describes, where an
attacker who could replace the zip could replace the manifest with it.

**Portability is the whole implementation problem.** The three legs of `rc-bundle.yml` run on
windows-latest, a self-hosted mac and a self-hosted Linux box; `sha256sum` is absent on macOS and
`shasum` is absent on some Windows images, so hashing is done here in Python — which every leg
already has, because `rc-sparse-checkout-covers-includes.py` runs on all three.

The macOS artifact is a `.app`, which is a directory, so the walk is recursive and the paths are
recorded relative to the artifact root. Sorted, so two runs over the same bytes produce the same
file byte for byte.

**How to grade this script**: `--self-test` builds a tree, hashes it, verifies the manifest it
wrote, then corrupts one byte and fails if that is not caught.
"""

from __future__ import annotations

import hashlib
import os
import sys
import tempfile
from pathlib import Path

MANIFEST_NAME = "SHA256SUMS"
CHUNK = 1 << 20


def digest(path: Path) -> str:
    """The file's SHA-256, read in chunks so a 400 MB bundle does not need 400 MB of memory."""
    sha = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(CHUNK):
            sha.update(chunk)
    return sha.hexdigest()


def entries(root: Path) -> list[tuple[str, str]]:
    """Every regular file under `root`, as (digest, path-relative-to-root), sorted by path.

    Symlinks are skipped rather than followed: a `.app` carries them, following one would hash a
    file twice under two names, and a manifest that disagrees with itself is worse than a short
    one. The manifest itself is excluded — it cannot contain its own digest.

    HIDDEN PATHS ARE SKIPPED TOO, and that is not tidiness. `actions/upload-artifact@v4` excludes
    dotfiles unless `include-hidden-files: true`, and this script runs BEFORE the upload — so
    hashing one writes a manifest describing a file the zip will not carry. Measured on rc1
    (run 35948659176): the Windows manifest listed 3,494 files, the artifact held 3,490, and the
    four missing were `.manifest.json` and three `.winres/` build intermediates. A tester running
    `sha256sum -c SHA256SUMS` — the single thing F40 added this file FOR — got four `FAILED open
    or read` lines and no way to tell that from tampering. A checksum that fails by construction
    is worse than none: it is a guard that lies. Only the Windows build drops dotfiles into its
    output today, but the rule lives here, where all three legs read it.
    """
    found = []
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        if relative == MANIFEST_NAME:
            continue
        if any(part.startswith(".") for part in relative.split("/")):
            continue
        found.append((digest(path), relative))
    return sorted(found, key=lambda pair: pair[1])


def manifest_text(root: Path) -> str:
    """The manifest, in the format `sha256sum -c` and `shasum -a 256 -c` both read."""
    return "".join(f"{sha}  {name}\n" for sha, name in entries(root))


def verify(root: Path, text: str) -> list[str]:
    """Paths in `text` whose bytes on disk no longer match it, plus any that are missing."""
    wrong = []
    for line in text.splitlines():
        if not line.strip():
            continue
        expected, _, name = line.partition("  ")
        path = root / name
        if not path.is_file() or digest(path) != expected:
            wrong.append(name)
    return wrong


def self_test() -> int:
    """A manifest nobody has seen refuse anything is a list of numbers."""
    failures = []

    # A known vector first, because every other case here compares digests only against each
    # other — under which a truncated or differently-named hash verifies perfectly and is
    # nonsense to `sha256sum -c`. This is the line that says the numbers are SHA-256.
    with tempfile.TemporaryDirectory() as vector:
        known = Path(vector) / "abc.txt"
        known.write_bytes(b"abc")
        if digest(known) != "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad":
            failures.append(f"SHA-256('abc') came back as {digest(known)}")

    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        (root / "nested").mkdir()
        (root / "Sidekick.exe").write_bytes(b"binary one")
        (root / "nested" / "asset.bin").write_bytes(b"binary two")

        # What the upload will silently drop, in BOTH shapes rc1 produced: a dotfile at the root
        # (`.manifest.json`) and a file inside a dot-DIRECTORY (`.winres/resource.rc`). The
        # second is the one a naive `name.startswith(".")` on the basename alone would miss.
        (root / ".manifest.json").write_bytes(b"bundler metadata")
        (root / ".winres").mkdir()
        (root / ".winres" / "resource.rc").write_bytes(b"icon resource")

        # A `.app` carries symlinks — `Contents/MacOS` and the framework `Current` links are
        # the usual ones. Following them hashes a file twice under two names, so the tree under
        # test has one. Without this the walk could follow links and nothing here would say so.
        (root / "Current").symlink_to(root / "Sidekick.exe")

        text = manifest_text(root)
        if "Current" in text:
            failures.append("a symlink was followed, so one file is hashed under two names")
        if len(text.splitlines()) != 2:
            failures.append(f"expected two entries, got: {text!r}")
        if "nested/asset.bin" not in text:
            failures.append("the walk is not recursive, so a .app would hash as nothing")
        if ".manifest.json" in text:
            failures.append("a root dotfile was hashed; upload-artifact will not carry it")
        if ".winres" in text:
            failures.append("a file under a dot-directory was hashed; the upload drops it")
        if verify(root, text):
            failures.append("a manifest did not verify against the bytes it was just made from")

        # The half that matters: it has to notice.
        (root / "Sidekick.exe").write_bytes(b"binary ONE")
        if verify(root, text) != ["Sidekick.exe"]:
            failures.append("a changed byte was not caught")

        (root / "Sidekick.exe").unlink()
        if verify(root, text) != ["Sidekick.exe"]:
            failures.append("a missing file was not caught")

        # And it must not hash its own manifest, which cannot contain its own digest.
        (root / MANIFEST_NAME).write_text(text, encoding="utf-8")
        if MANIFEST_NAME in manifest_text(root):
            failures.append("the manifest listed itself")

    # The empty artifact, which is the quiet failure this file exists against — and which the
    # rest of this self-test could not see, because it only ever exercised the two helpers and
    # the refusal lives in `main`. A mutation that deleted that refusal passed everything here.
    with tempfile.TemporaryDirectory() as empty:
        if manifest_text(Path(empty)):
            failures.append("an empty directory produced a manifest")
        argv = sys.argv
        try:
            sys.argv = [argv[0], empty]
            if main() == 0:
                failures.append("an empty artifact was checksummed as nothing and accepted")
        finally:
            sys.argv = argv

    # And a path that is not a directory at all.
    argv = sys.argv
    try:
        sys.argv = [argv[0], str(Path(__file__))]
        if main() == 0:
            failures.append("a non-directory was accepted as an artifact")
    finally:
        sys.argv = argv

    for failure in failures:
        print(f"SELF-TEST FAILED: {failure}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: the manifest verifies what it should and refuses what it should")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <artifact-dir> | --self-test", file=sys.stderr)
        return 2

    root = Path(sys.argv[1]).resolve()
    if not root.is_dir():
        print(f"::error::{root} is not a directory; there is nothing to check", file=sys.stderr)
        return 1

    text = manifest_text(root)
    if not text:
        # An empty manifest beside an empty artifact is the quiet failure this whole file is
        # against: `if-no-files-found: error` on the upload would catch it, but only afterwards.
        print(f"::error::no files under {root} to checksum", file=sys.stderr)
        return 1

    (root / MANIFEST_NAME).write_text(text, encoding="utf-8")

    wrong = verify(root, text)
    if wrong:
        print(f"::error::the manifest does not match what is on disk: {wrong}", file=sys.stderr)
        return 1

    count = len(text.splitlines())
    print(f"{MANIFEST_NAME}: {count} file(s)")
    print(text, end="")

    # The run's own record, which is the copy an attacker who replaced the zip cannot reach.
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as handle:
            handle.write(f"### `{root.name}` — SHA-256\n\n```\n{text}```\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
