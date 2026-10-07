#!/usr/bin/env python3
"""The three guards `exported_powers/` was always documented to have, and a fourth.

`tools/bin-crawler/bin_crawler/` stamps every export with three claims, and until
now nothing anywhere checked one of them. Five exporters, eighteen manifests, and
the files named as the enforcement side —
`src/data/export-staleness.test.ts`, `src/data/export-provenance.test.ts`,
`src/data/export-contents.test.ts` — have **never existed in this repository**.
Not deleted: never written. They are cited in five exporters, in
`_export_fingerprint.py`, `_export_digest.py`, `assets_sources.py`,
`audit-dataset-roster.cjs`, and in the `note` field of fourteen
of the eighteen committed manifests. `git log --all --diff-filter=AD` over
`src/data/export-*.test.ts` returns nothing. This file is the three of them,
measured into existence rather than assumed.

The four questions, which are genuinely different:

  staleness   Did the exporter source change without a re-export?
              Recorded `*_fingerprint` vs the hash of the package as it is now.
  provenance  Was this read from the tree the registry sanctions, or from a
              beta ring or a decoy?  Recorded `source.assets_dir` vs
              `assets_sources.canonical_path`.
  contents    Are the bytes in the tree right now the bytes the exporter wrote?
              Recorded `content_digest`/`file_count` vs the committed files.
  classify    Can the registry still name its own trees and reject its own
              traps, under both the registry's spelling of a path and the
              resolved one?  See the symlink paragraph below.

`prov8-shard-drift.cjs` asks the fourth — has the GAME moved under a current
export — and needs the client installed, so it stays a key. These three need
only the repository and the Python package, so they are a gate.

**Written in Python, against the original design, and that is the point.** The
never-written guards were specified as TypeScript, and `_export_digest.py` spends
a paragraph worrying that "the JS side must replicate it byte for byte ... A
divergence surfaces as a permanently-red guard". Importing `_fold` from the
package removes that failure mode rather than managing it: there is one
implementation of "hash a set of files" and both sides call it.

**Both sides of every path comparison are resolved, and that is not cosmetic.**
On this workstation `~/Games/coh-sweettea` is a symlink to
`/mnt/games-1tb/Games/coh-sweettea`; the manifests record the mount point and the
registry names the symlink. Comparing the spellings fails. Writing this gate
found `assets_sources.dataset_for_path` and `rejection_reason` resolving their
ARGUMENT but not the registry side, so both returned None for every Sweet Tea
path under EITHER spelling: the registry could not tell its own sanctioned tree
from the `piggs` decoy, and gave both the same "does not name it" refusal, whose
remedy line offers `--allow-unregistered-source`. Fixed in `assets_sources.py`
by routing both sides of both comparisons through `_comparable`; the re-export that
the resulting fingerprint bump demanded was run at the
same time.

The fourth check below exists because of that bug. Staleness, provenance and
contents all read the registry and none of them asks whether the registry still
recognises anything -- provenance stayed green throughout, because it resolves
both sides itself. A trap list that has silently stopped rejecting is this
repository's recurring failure (registry schema 1 died of it when a workstation
changed), so it is now measured rather than assumed.

    python3 tools/export-integrity.py              # report all three
    python3 tools/export-integrity.py --gate       # exit 1 on any failure
    python3 tools/export-integrity.py --self-test  # prove each check can fail

BREAKS THE CLAIM: any surface reporting STALE, FOREIGN or TAMPERED. Stale says
the committed export does not match the parser that is supposed to have produced
it, so downstream fixes are inert. Foreign says it was read from a tree nobody
sanctioned. Tampered says a committed file was edited, added or deleted after the
export, which every downstream gate would otherwise absorb as ground truth.

A dataset whose install is not on this machine reports SKIPPED for provenance,
never OK. Staleness and contents still run: they need no install. Borrowed from
`prov8-shard-drift.cjs`, whose reasoning applies unchanged -- the absence of the
measurement must not read as a pass.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / 'tools' / 'bin-crawler'))

from bin_crawler import assets_sources  # noqa: E402
from bin_crawler._export_fingerprint import _fold  # noqa: E402
from bin_crawler import _export_fingerprint as fp  # noqa: E402

EXPORTED = REPO / 'exported_powers'

# The forks live in a subdirectory named for themselves; Homecoming is the flat
# root. Same layout rule `prov8-shard-drift.cjs` and `preflight.py` both encode.
FORKS = ('brainstorm', 'rebirth', 'thunderspy')

# A tree surface's manifest, and the two exporters that write exactly one file.
# This is a two-entry list rather than a rule because no spelling rule derives
# it: `incarnate_recipes_export_manifest.json` stamps `incarnate-recipes.json`,
# hyphen against underscore. A wrong entry here cannot pass silently -- it moves
# a digest and the surface goes TAMPERED.
TREE_MANIFEST = '_export_manifest.json'
SINGLE_FILE_SURFACES = {
    'salvage_export_manifest.json': 'salvage.json',
    'incarnate_recipes_export_manifest.json': 'incarnate-recipes.json',
}
MANIFEST_NAMES = {TREE_MANIFEST} | set(SINGLE_FILE_SURFACES)

# Which fingerprint function grades which manifest key. All five fold the same
# file set and are therefore equal by construction (see `_export_fingerprint.py`);
# they are kept apart because each answers a per-surface question, and checking
# them by name keeps that true if they ever diverge.
FINGERPRINTS = {
    'parser_fingerprint': fp.parser_fingerprint,
    'classes_fingerprint': fp.classes_fingerprint,
    'entities_fingerprint': fp.entities_fingerprint,
    'salvage_fingerprint': fp.salvage_fingerprint,
    'incarnate_recipes_fingerprint': fp.incarnate_recipes_fingerprint,
}


def dataset_of(manifest: Path) -> str:
    """Which dataset a manifest belongs to, from where it sits.

    Anchored on the last path component named `exported_powers` rather than on
    the module-level tree, so the self-test's sandboxed copy classifies the same
    way the real one does.
    """
    parts = manifest.parts
    try:
        start = len(parts) - 1 - parts[::-1].index('exported_powers')
    except ValueError:
        return 'homecoming'
    rest = parts[start + 1:]
    return rest[0] if rest and rest[0] in FORKS else 'homecoming'


def export_manifests(root: Path = EXPORTED) -> list[Path]:
    """Every export manifest, by exact name.

    By NAME, not by glob on `*manifest.json`: `thunderspy/dominator_assault/
    telekinetic_assault/manifest.json` is exported power data that happens to be
    called that, and swallowing it would drop a real file out of the digest set
    while looking like a stricter check.
    """
    return sorted(p for p in root.rglob('*.json') if p.name in MANIFEST_NAMES)


def tree_roots(root: Path = EXPORTED) -> list[Path]:
    return sorted({p.parent for p in root.rglob(TREE_MANIFEST)})


def owned_files(surface: Path, roots: list[Path]) -> list[Path]:
    """The files one tree surface wrote, rederived from the committed tree.

    `ExportTree` knows this set exactly because every write goes through it. Here
    it has to be rederived, by nearest-enclosing-manifest: everything under the
    surface root except what a nested surface owns, the manifests themselves (a
    file cannot carry its own hash), and the single-file surfaces that sit in the
    same directory. Disagreement shows up as a `file_count` mismatch before it
    shows up as a digest mismatch, which is the more legible failure.
    """
    nested = [r for r in roots if r != surface and surface in r.parents]
    claimed = {surface / target for name, target in SINGLE_FILE_SURFACES.items()
               if (surface / name).is_file()}
    out = []
    for p in surface.rglob('*'):
        if not p.is_file() or p.name in MANIFEST_NAMES or p in claimed:
            continue
        if any(n == p or n in p.parents for n in nested):
            continue
        out.append(p)
    return sorted(out)


def fold_files(files: list[Path], base: Path) -> str:
    return _fold([(f.relative_to(base).as_posix(), f.read_bytes()) for f in files])


def check_staleness(manifest: Path, data: dict) -> tuple[str, str]:
    keys = [k for k in data if k in FINGERPRINTS]
    if not keys:
        return 'SKIPPED', 'manifest records no fingerprint'
    for key in keys:
        current = FINGERPRINTS[key]()
        if data[key] != current:
            return 'STALE', (f'{key} recorded {data[key][:12]}, source is now '
                             f'{current[:12]} — re-export this dataset')
    return 'OK', f'{keys[0]} matches source'


def check_provenance(manifest: Path, data: dict) -> tuple[str, str]:
    source = data.get('source')
    if not source:
        return 'SKIPPED', 'pre-provenance manifest, records no source'
    recorded = source.get('assets_dir')
    if not recorded:
        return 'SKIPPED', 'source names no assets_dir'
    dataset = dataset_of(manifest)
    try:
        canonical = assets_sources.canonical_path(dataset)
    except SystemExit as exc:
        return 'SKIPPED', f'registry cannot resolve {dataset} here: {exc}'
    except Exception as exc:  # noqa: BLE001 — a registry fault must not read as a pass
        return 'SKIPPED', f'registry error for {dataset}: {type(exc).__name__}'
    rec, can = Path(recorded), Path(canonical)
    if not rec.exists() or not can.exists():
        # Without the install, the symlink cannot be followed and the two
        # spellings cannot be compared honestly. Say so instead of guessing.
        return 'SKIPPED', 'install not on this machine, paths unresolvable'
    if rec.resolve() != can.resolve():
        return 'FOREIGN', (f'exported from {recorded}, but {dataset} is '
                           f'sanctioned at {canonical}')
    return 'OK', f'{dataset}:{assets_sources.exportable_ring(dataset)}'


def check_contents(manifest: Path, data: dict, roots: list[Path]) -> tuple[str, str]:
    if 'content_digest' not in data:
        return 'SKIPPED', 'manifest records no content_digest'
    if manifest.name == TREE_MANIFEST:
        base = manifest.parent
        files = owned_files(base, roots)
    else:
        base = manifest.parent
        target = base / SINGLE_FILE_SURFACES[manifest.name]
        if not target.is_file():
            return 'TAMPERED', f'{target.name} is missing'
        files = [target]
    recorded_count = data.get('file_count')
    if recorded_count is not None and len(files) != recorded_count:
        return 'TAMPERED', (f'{len(files)} files present, manifest wrote '
                            f'{recorded_count} — a file was added or deleted')
    digest = fold_files(files, base)
    if digest != data['content_digest']:
        return 'TAMPERED', (f'digest {digest[:12]} over {len(files)} files, '
                            f'manifest recorded {data["content_digest"][:12]}')
    return 'OK', f'{len(files)} files'


CHECKS = ('staleness', 'provenance', 'contents')
FAILED = {'STALE', 'FOREIGN', 'TAMPERED', 'BLIND'}


def check_registry_classification(
        datasets: list[str] | None = None) -> list[tuple[str, str, str]]:
    """Can the registry still name its own trees, and do its traps still reject?

    One row per dataset. Every ring must classify back to itself and every
    rejected subpath must come back with a reason -- under the spelling the
    registry uses AND under the fully resolved spelling, because those differ
    wherever an install sits behind a symlink and a caller may hold either.

    This needs no install: it is path arithmetic, so it never SKIPs. A trap for
    a folder that is not on this machine is still a trap that must be named.

    BLIND is the verdict, and it is its own failure mode rather than a flavour
    of FOREIGN: a foreign tree is one the registry rejects correctly, while a
    blind registry has stopped classifying, so the exporter's refusal message
    stops distinguishing a known decoy from an unregistered folder and points
    the operator at `--allow-unregistered-source` for both.
    """
    rows = []
    for dataset in (datasets or assets_sources.datasets()):
        entry = assets_sources._registry()['datasets'][dataset]
        checked = 0
        symlinked = False
        failure = None
        for r in entry.get('roots', []):
            base = assets_sources._expand(r['path'])
            targets = [(base / e['subpath'], (dataset, ring), 'ring')
                       for ring, e in entry['rings'].items()]
            targets += [(base / sub, None, 'trap')
                        for sub in entry.get('rejected_subpaths', {})]
            for path, expect, kind in targets:
                real = path.resolve()
                spellings = [path] if real == path else [path, real]
                symlinked |= len(spellings) > 1
                for spelling in spellings:
                    checked += 1
                    if kind == 'ring':
                        got = assets_sources.dataset_for_path(spelling)
                        if got != expect and failure is None:
                            failure = (f'{spelling} classified as {got}, '
                                       f'registry names it {expect}')
                    else:
                        if (assets_sources.rejection_reason(spelling) is None
                                and failure is None):
                            failure = (f'{spelling} is a named trap and '
                                       f'rejection_reason returned None')
        if failure:
            rows.append((dataset, 'BLIND', failure))
        else:
            note = ' (both spellings, root behind a symlink)' if symlinked else ''
            rows.append((dataset, 'OK', f'{checked} paths classified{note}'))
    return rows


def run(root: Path = EXPORTED, quiet: bool = False) -> dict[str, int]:
    roots = tree_roots(root)
    tally = {'OK': 0, 'SKIPPED': 0, 'failed': 0}
    registry = check_registry_classification()
    for _, verdict, _ in registry:
        tally['failed' if verdict in FAILED else verdict] += 1
    tally['registry_checks'] = 0            # counted separately in the summary
    rows = []
    for manifest in export_manifests(root):
        data = json.loads(manifest.read_text())
        results = {
            'staleness': check_staleness(manifest, data),
            'provenance': check_provenance(manifest, data),
            'contents': check_contents(manifest, data, roots),
        }
        rows.append((manifest, results))
        for verdict, _ in results.values():
            tally['failed' if verdict in FAILED else verdict] += 1
    if not quiet:
        for dataset, verdict, detail in registry:
            print(f"{'RED ' if verdict in FAILED else 'ok  '} registry:{dataset}")
            print(f"       {'classify':11s} {verdict:8s} {detail}")
        for manifest, results in rows:
            rel = manifest.relative_to(root.parent).as_posix()
            worst = next((v for c in CHECKS for v, _ in [results[c]] if v in FAILED), None)
            print(f"{'RED ' if worst else 'ok  '} {rel}")
            for check in CHECKS:
                verdict, detail = results[check]
                if verdict != 'OK' or worst:
                    print(f"       {check:11s} {verdict:8s} {detail}")
    return tally


def self_test() -> bool:
    """Prove each check fails when it should, on a throwaway copy of one surface.

    A guard nobody has seen go red is a guard nobody knows is wired up; this repo
    has just spent an item on three that were never written at all. The copy is a
    real export surface so the checks run against their real shapes.
    """
    import shutil
    import tempfile

    ok = True
    with tempfile.TemporaryDirectory() as tmp:
        sandbox = Path(tmp) / 'exported_powers'
        shutil.copytree(EXPORTED / 'tables', sandbox / 'tables')
        roots = tree_roots(sandbox)
        mf = sandbox / 'tables' / TREE_MANIFEST
        pristine = json.loads(mf.read_text())

        # contents: edit a committed file the export wrote
        victim = next(p for p in sorted((sandbox / 'tables').rglob('*.json'))
                      if p.name not in MANIFEST_NAMES)
        original = victim.read_bytes()
        victim.write_bytes(original + b' ')
        verdict, detail = check_contents(mf, pristine, roots)
        ok &= verdict == 'TAMPERED'
        print(f"  {'ok  ' if verdict == 'TAMPERED' else 'FAIL'} edited file -> {verdict}: {detail}")
        victim.write_bytes(original)

        # contents: delete one
        victim.unlink()
        verdict, detail = check_contents(mf, pristine, roots)
        ok &= verdict == 'TAMPERED'
        print(f"  {'ok  ' if verdict == 'TAMPERED' else 'FAIL'} deleted file -> {verdict}: {detail}")
        victim.write_bytes(original)

        # contents: add a stray
        stray = sandbox / 'tables' / 'stray.json'
        stray.write_text('{}')
        verdict, detail = check_contents(mf, pristine, roots)
        ok &= verdict == 'TAMPERED'
        print(f"  {'ok  ' if verdict == 'TAMPERED' else 'FAIL'} stray file  -> {verdict}: {detail}")
        stray.unlink()

        # contents: clean again, to prove the failures were the edits
        verdict, detail = check_contents(mf, pristine, roots)
        ok &= verdict == 'OK'
        print(f"  {'ok  ' if verdict == 'OK' else 'FAIL'} restored     -> {verdict}: {detail}")

        # staleness: a manifest recording a fingerprint the source cannot produce
        bent = dict(pristine, classes_fingerprint='0' * 64)
        verdict, detail = check_staleness(mf, bent)
        ok &= verdict == 'STALE'
        print(f"  {'ok  ' if verdict == 'STALE' else 'FAIL'} bent hash    -> {verdict}: {detail}")

        # provenance: a manifest claiming a tree the registry does not sanction
        bent = dict(pristine, source=dict(pristine['source'], assets_dir=str(REPO)))
        verdict, detail = check_provenance(mf, bent)
        ok &= verdict == 'FOREIGN'
        print(f"  {'ok  ' if verdict == 'FOREIGN' else 'FAIL'} foreign tree -> {verdict}: {detail}")

    # classify: put the original bug back, verbatim, and require BLIND.
    #
    # The pre-fix `dataset_for_path` and `rejection_reason` are reinstated below
    # exactly as they read before the fix: `Path(path).resolve()` on the
    # ARGUMENT, and the registry side left at whatever `_expand` produced. That
    # asymmetry is the whole bug, so nothing short of the original bodies proves
    # this check would have caught it.
    #
    # It reddens only where a root sits behind a symlink, so on a workstation
    # with none this clause proves nothing, and says so rather than passing.
    def behind_a_symlink(dataset: str) -> bool:
        roots = assets_sources._registry()['datasets'][dataset].get('roots', [])
        return any(assets_sources._expand(r['path']).resolve()
                   != assets_sources._expand(r['path']) for r in roots)

    symlinked = [d for d in assets_sources.datasets() if behind_a_symlink(d)]
    if not symlinked:
        print('  --   blind registry: no root on this machine sits behind a '
              'symlink, so the bug cannot be reproduced here')
    else:
        def pre_fix_dataset_for_path(path):
            resolved = Path(path).resolve().as_posix()
            for name, entry, base in assets_sources._all_roots():
                for ring, ring_entry in entry['rings'].items():
                    if (base / ring_entry['subpath']).as_posix() == resolved:
                        return name, ring
            return None

        def pre_fix_rejection_reason(path):
            resolved = Path(path).resolve().as_posix()
            for _, entry, base in assets_sources._all_roots():
                for subpath, reason in entry.get('rejected_subpaths', {}).items():
                    if (base / subpath).as_posix() == resolved:
                        return reason
            return None

        good = (assets_sources.dataset_for_path, assets_sources.rejection_reason)
        try:
            assets_sources.dataset_for_path = pre_fix_dataset_for_path
            assets_sources.rejection_reason = pre_fix_rejection_reason
            rows = check_registry_classification(symlinked)
            blind = [d for d, v, _ in rows if v == 'BLIND']
            ok &= len(blind) == len(symlinked)
            print(f"  {'ok  ' if len(blind) == len(symlinked) else 'FAIL'} "
                  f"blind reg.   -> {len(blind)}/{len(symlinked)} symlinked "
                  f"datasets BLIND ({', '.join(symlinked)})")
            # ...and the datasets whose roots carry no symlink must stay OK,
            # which is why the bug survived: it looked like a working registry.
            others = [d for d in assets_sources.datasets() if d not in symlinked]
            if others:
                rows = check_registry_classification(others)
                quiet = all(v == 'OK' for _, v, _ in rows)
                ok &= quiet
                print(f"  {'ok  ' if quiet else 'FAIL'} bug is local -> "
                      f"{', '.join(others)} still classify with the bug in place")
        finally:
            assets_sources.dataset_for_path, assets_sources.rejection_reason = good
        rows = check_registry_classification()
        clean = all(v == 'OK' for _, v, _ in rows)
        ok &= clean
        print(f"  {'ok  ' if clean else 'FAIL'} restored     -> "
              f"{'all datasets classify' if clean else 'still BLIND'}")
    return ok


def main() -> int:
    argv = sys.argv[1:]
    if '--self-test' in argv:
        print('self-test: each check must fail on a tampered copy of tables/')
        good = self_test()
        print('self-test ok' if good else 'SELF-TEST FAILED')
        return 0 if good else 1

    gate = '--gate' in argv
    tally = run()
    tally.pop('registry_checks', None)
    n_manifests = len(export_manifests())
    total = sum(tally.values())
    print(f"\n{tally['OK']} ok, {tally['SKIPPED']} skipped, {tally['failed']} failed "
          f"({total} checks: {len(CHECKS) * n_manifests} over {n_manifests} "
          f"manifests, {total - len(CHECKS) * n_manifests} over the registry)")
    if tally['SKIPPED']:
        print("skipped is not passed — a check that could not run measured nothing.")
    if tally['failed']:
        print(f"\nGATE FAIL — {tally['failed']} check(s) failed. "
              f"See the module docstring for what each verdict means.")
        return 1 if gate else 0
    print('GATE PASS' if gate else 'all measurable checks green')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
