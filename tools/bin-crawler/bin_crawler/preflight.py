"""Check the game installs before an export that will force a regen.

    py -3 -m bin_crawler.preflight                 # every dataset's exportable ring
    py -3 -m bin_crawler.preflight --source homecoming:open_beta
    py -3 -m bin_crawler.preflight --quick         # skip the per-file checksums

Three questions, in the order that makes an export worth starting:

  1. **Is the tree the one we mean?** Resolved from `assets_sources.json`, so
     the answer is a name rather than a typed path (DATA-GAP-REGISTER PROV-2).
  2. **Are the archives intact?** Each `.pigg` entry carries an MD5 of its
     decompressed body — the same field the game launchers validate against —
     so a torn download or a half-written patch is detectable offline, which is
     otherwise only visible by running the launcher.
  3. **Has anything moved since the committed export?** Each manifest records
     the digest of every archive it read, so comparing them says whether a
     re-export would carry new data, and exactly which archives changed. A
     patch that touches no archive we read means there is nothing to re-export.

Question 3 is the one the register said the repo could not answer. It still
cannot tell you whether a tree is CURRENT — only the server knows that — but it
can tell you the tree is not what you last exported from, which is the signal
that a regen is owed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bin_crawler import assets_sources
from bin_crawler.parser._pigg import BinResolver

# The committed manifests that record what each dataset was last exported from.
# Keyed the way exported_powers/ is laid out: HC at the tree root, the forks
# nested under their own subdirectory.
_MANIFEST_SUBTREES = {'powers': '', 'tables': 'tables', 'entities': 'entities'}

# Anchored to this file rather than the working directory. `bin_crawler` is not
# installed, so it imports only from `tools/bin-crawler` — and from there a
# relative `exported_powers` resolves to nothing, which made question 3 announce
# "no committed export" and exit OK on every run from the one directory the
# module can be run from. Absence must mean absence, not a wrong cwd.
_EXPORTED = Path(__file__).resolve().parents[3] / 'exported_powers'


def _dataset_root(dataset: str) -> Path:
    return _EXPORTED if dataset == 'homecoming' else _EXPORTED / dataset


def _committed_digests(dataset: str) -> dict[str, dict[str, str]]:
    """{surface -> {archive name -> sha256}} for one dataset's committed trees."""
    out: dict[str, dict[str, str]] = {}
    for surface, sub in _MANIFEST_SUBTREES.items():
        path = _dataset_root(dataset) / sub / '_export_manifest.json'
        if not path.is_file():
            continue
        source = json.loads(path.read_text()).get('source')
        if not source:
            continue  # a pre-provenance manifest; the gate reports it
        out[surface] = {f['name']: f['sha256'] for f in source['sources']}
    return out


# The entry record `_build_index` reads. Note it unpacks 52 bytes but entries
# are ENTRY_SIZE (48) apart, so the trailing word overlaps the next entry and is
# unused — read the same way here rather than by calcsize, or every entry after
# the first lands mid-record.
_ENTRY_FORMAT = '<IIIIIII4II'
_MD5_SLOT = slice(7, 11)


def verify_archive(pigg_path: Path) -> list[str]:
    """Check every entry's body against the MD5 the archive records for it.

    Returns the failures; empty means the archive is intact. The `4I` slot of
    each entry record is an MD5 of the decompressed body — the field the game
    launchers validate — verified here against the extracted bytes rather than
    trusted, so a truncated or half-patched archive fails loudly instead of
    parsing into plausible-looking game data.
    """
    from pigg_wrangler.pigg import PIGG_SIGNATURE, HEADER_SIZE, ENTRY_SIZE

    data = pigg_path.read_bytes()
    signature, _, _, _, entry_count = struct.unpack_from('<IHHII', data, 0)
    if signature != PIGG_SIGNATURE:
        return [f'bad PIGG signature {signature:#x}']

    failures: list[str] = []
    offset = HEADER_SIZE
    for index in range(entry_count):
        values = struct.unpack_from(_ENTRY_FORMAT, data, offset)
        offset += ENTRY_SIZE
        size, body_offset, compressed = values[2], values[4], values[11]
        recorded = struct.pack('<4I', *values[_MD5_SLOT])
        raw = data[body_offset:body_offset + (compressed or size)]
        try:
            body = zlib.decompress(raw) if compressed and compressed != size else raw
        except zlib.error as exc:
            failures.append(f'entry {index}: cannot decompress ({exc})')
            continue
        if hashlib.md5(body).digest() != recorded:
            failures.append(f'entry {index}: checksum mismatch')
    return failures


def check(dataset: str, ring: str, path: str, *, quick: bool) -> bool:
    print(f'\n{dataset}:{ring}')
    print(f'  {path}')

    tree = Path(path)
    if not tree.is_dir():
        print('  MISSING — the install is not at the registered path.')
        return False

    resolver = BinResolver(tree)
    if not resolver.has_data:
        print('  NO DATA — no .pigg archives or loose .bin files here.')
        return False

    provenance = resolver.provenance()
    current = {f['name']: f['sha256'] for f in provenance['sources']}

    ok = True
    if quick:
        print(f'  {len(current)} archives (checksums skipped)')
    else:
        for source in provenance['sources']:
            archive = tree / source['name']
            failures = verify_archive(archive)
            if failures:
                ok = False
                print(f'  CORRUPT {source["name"]}: {len(failures)} bad entries')
                for line in failures[:3]:
                    print(f'      {line}')
            else:
                print(f'  ok {source["name"]:28} {source["modified"][:10]}')

    committed = _committed_digests(dataset)
    if not committed:
        print('  (no committed export to compare against)')
        return ok

    for surface, digests in committed.items():
        changed = sorted(
            name for name in set(digests) | set(current)
            if digests.get(name) != current.get(name)
        )
        if changed:
            print(f'  CHANGED since {surface} export: {", ".join(changed)}')
        else:
            print(f'  unchanged since {surface} export')
    return ok


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--source', default=None, metavar='DATASET[:RING]',
                    help='One tree to check (default: every dataset\'s exportable ring)')
    ap.add_argument('--quick', action='store_true',
                    help='Resolve and compare digests, but skip per-entry checksums')
    args = ap.parse_args()

    if args.source:
        targets = [assets_sources.resolve(args.source)]
    else:
        targets = [assets_sources.resolve(name) for name in assets_sources.datasets()]

    ok = all(check(*target, quick=args.quick) for target in targets)
    print('\nPREFLIGHT ' + ('OK' if ok else 'FAILED'))
    return 0 if ok else 1


if __name__ == '__main__':
    raise SystemExit(main())
