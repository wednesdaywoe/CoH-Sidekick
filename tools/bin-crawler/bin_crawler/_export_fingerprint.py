"""Exporter source fingerprints — the anchor of the export-staleness guard.

`exported_powers/<dataset>/` (and its `tables/` subtree) is produced by the
Python bin parser reading the gitignored `.pigg` archives. CI has neither the
archives nor Python, so unlike `src/data/datasets/*/generated/` (which
regenerates from committed `exported_powers/`), these exports CANNOT be
regenerate-and-diffed in CI.

The failure this closes: a change to the parser (e.g. `_powers.py`,
`_classes.py`) that ships WITHOUT a matching re-export, so the committed JSON is
stale relative to the parser and every downstream fix is inert. The powers tree
bit twice (the 2026-07-06 tspy hybrid relabel; the incarnate converter
regenerating one dataset); the `tables/` tree was the WS3 gap — kept in sync
only because CLASSES-1 happened to regenerate it, guarded by nothing going
forward.

The guard: each exporter (`export_powers.py`, `export_classes.py`,
`export_entities.py`, `export_salvage.py`) stamps its output dir with a manifest
recording that exporter's SOURCE fingerprint at export time. A JS/vitest guard
(`src/data/export-staleness.test.ts`) recomputes
the same hash from the committed sources and asserts every dataset's manifest
matches. If the parser changed but a dataset was not re-exported, its recorded
fingerprint diverges from the current source and the guard goes red; the only
way to make it green is to actually re-export (which re-stamps).

Scope: every `.py` in the package — `parser/**/*.py`, the exporter entry
modules, and the helpers beside them. It is a directory GLOB, not a curated
allowlist (the project keeps burning on hand-maintained allowlists), so it is
self-maintaining as files come and go.

**The glob used to stop at `parser/` plus one entry module, and that was a
hole.** `export_powers.py` imports `path_safety.py`; every exporter imports
`assets_dir.py`; `export_entities.py` imports `export_powers.py` itself. None of
those were covered, so a change to the path rule or the source resolver changed
what the exporter WRITES with no staleness gate to notice — recorded as
deliberately unfixed in F78's row, and closed here because the digest work below
was paying for a re-export anyway. The cost of widening is near nothing: the
non-parser modules took 1–3 commits each over six months against `parser/`'s 63,
so parser churn already dominates when a re-export is forced.

It intentionally OVER-covers, and now maximally so: all five fingerprints fold
the same file set and are therefore EQUAL by construction. The five names and
the five manifest keys stay because each still answers a per-surface question
("is THIS tree current?") and the manifest schema is read by two guards; what
they no longer do is differ. A `_powers.py` edit forces a harmless
classes/entities/salvage re-export and vice-versa (near-zero diff) — the
accepted trade for a glob nobody has to maintain. Modules an exporter never
imports (`server.py`, `preflight.py`) are swept in too; at their churn that is
cheaper than a rule deciding which imports count.


**The tax, stated so the next person is not surprised by it.** The fingerprint
is over source BYTES, so editing a comment in any `.py` here — this file, a
docstring in `path_safety.py`, anything — invalidates all eighteen manifests
and the only way back to green is re-running every export (about twenty-five
minutes across the four datasets). That was already true of `parser/**` and its
63 commits in six months; widening the glob extended it to a dozen quieter
files. It is the price of a glob nobody maintains, and it is paid in machine
time rather than in a stale guard nobody notices. Batch source edits, then
re-export once.

The salvage surface (`export_salvage.py` → HC-only
`exported_powers/salvage.json`, stamped alongside as
`salvage_export_manifest.json`) is HC-only: Rebirth/Thunderspy piggs carry no
`salvage.bin`, so those exports early-return and no manifest is written.

The manifest carries a second, independent record alongside these fingerprints:
`source`, the assets tree the bytes were read from (`BinResolver.provenance()`,
guarded by `src/data/export-provenance.test.ts`). A fingerprint says which
exporter ran; it cannot say what it was pointed at — DATA-GAP-REGISTER PROV-1.

The JS side (`export-staleness.test.ts`) MUST replicate this algorithm byte for
byte: sorted (posix-relpath-from-bin_crawler, file-bytes) folded into sha256 as
`relpath\0content\0` per file. Keep the two in lockstep — a divergence surfaces
loudly as a permanently-red guard, never a silent gap.
"""
from __future__ import annotations

import hashlib
from pathlib import Path

# Package root = the `bin_crawler` dir that contains parser/ and export_powers.py.
_PKG_ROOT = Path(__file__).resolve().parent


def _fold(entries: list[tuple[str, bytes]]) -> str:
    """sha256 hex of `(relpath, bytes)` pairs, deterministic and order-independent.

    Folded in sorted-relpath order as `relpath\\0<bytes>\\0`, so the caller need
    not pre-sort. This is THE "hash a set of files" primitive for the package:
    `_export_digest.ExportTree` folds an export's OUTPUT with it, the
    fingerprints below fold the exporter's SOURCE with it, and
    `src/data/export-staleness.test.ts` + `src/data/export-contents.test.ts`
    replicate it in TS. One algorithm, four call sites, no drift.
    """
    ordered = sorted(entries, key=lambda e: e[0])
    h = hashlib.sha256()
    for rel, data in ordered:
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(data)
        h.update(b"\0")
    return h.hexdigest()


def _package_py_files(pkg_root: Path) -> list[Path]:
    """Every `.py` in the package, sorted, `__pycache__` excluded.

    The whole package, not just `parser/` — see the module docstring on why the
    glob widened and what it cost.
    """
    return [p for p in sorted(pkg_root.rglob("*.py"))
            if "__pycache__" not in p.parts]


def _fingerprint(pkg_root: Path, files: list[Path]) -> str:
    """sha256 hex of `files`, keyed by POSIX relpath from `bin_crawler`."""
    return _fold([(f.relative_to(pkg_root).as_posix(), f.read_bytes())
                  for f in files])


def parser_fingerprint(pkg_root: Path | None = None) -> str:
    """sha256 of the powers-exporter source: every .py in the package.

    Equal to the other fingerprints by construction — see the module docstring.
    """
    root = pkg_root or _PKG_ROOT
    return _fingerprint(root, _package_py_files(root))


def classes_fingerprint(pkg_root: Path | None = None) -> str:
    """sha256 of the classes-exporter source: every .py in the package.

    Equal to the other fingerprints by construction — see the module docstring.
    """
    root = pkg_root or _PKG_ROOT
    return _fingerprint(root, _package_py_files(root))


def entities_fingerprint(pkg_root: Path | None = None) -> str:
    """sha256 of the entities-exporter source: every .py in the package.

    Equal to the other fingerprints by construction — see the module docstring.
    """
    root = pkg_root or _PKG_ROOT
    return _fingerprint(root, _package_py_files(root))


def salvage_fingerprint(pkg_root: Path | None = None) -> str:
    """sha256 of the salvage-exporter source: every .py in the package.

    Equal to the other fingerprints by construction — see the module docstring.
    """
    root = pkg_root or _PKG_ROOT
    return _fingerprint(root, _package_py_files(root))


def incarnate_recipes_fingerprint(pkg_root: Path | None = None) -> str:
    """sha256 of the recipes-exporter source: every .py in the package.

    Equal to the other fingerprints by construction — see the module docstring.
    """
    root = pkg_root or _PKG_ROOT
    return _fingerprint(root, _package_py_files(root))


if __name__ == "__main__":
    print(f"parser_fingerprint   {parser_fingerprint()}")
    print(f"classes_fingerprint  {classes_fingerprint()}")
    print(f"entities_fingerprint {entities_fingerprint()}")
    print(f"salvage_fingerprint  {salvage_fingerprint()}")
    print(f"incarnate_recipes_fingerprint {incarnate_recipes_fingerprint()}")
