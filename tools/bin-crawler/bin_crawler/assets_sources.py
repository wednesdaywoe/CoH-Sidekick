"""The canonical game-install tree for each dataset and ring.

Reads `assets_sources.json`, which is the single place a path lives. The
registry exists because a path typed at the command line is the one input
nothing downstream can check: an export from the wrong tree is internally
self-consistent, so it passes the staleness guard, converter validation and the
contract totals alike (DATA-GAP-REGISTER PROV-1). Naming trees instead of
typing paths removes the typo from the loop, and the `provenance` check in
`tools/export-integrity.py` holds every committed manifest to the path this
registry names.

`--source <dataset>[:<ring>]` on each exporter resolves through here. Rings
other than `exportable_ring` resolve fine — reading the open beta to see what a
patch does is the point of having it — but the gate rejects an export STAMPED
with one, so a beta read cannot quietly become the committed tree.

Roots and rings
---------------
A dataset's paths split into the part that varies by machine and the part that
does not. The **root** is the install directory, which differs on every
workstation; the **ring** is a fixed subdirectory inside it (`live`, `beta`,
`experimental`) that is the same everywhere the game is installed. So each
dataset lists every root it has been seen at, and each ring names only its
subpath.

The root in use is chosen by which one exists on this machine. Exactly one is
the normal case — the other workstations' roots simply are not there. Zero
existing roots resolves to the first candidate anyway, so callers keep producing
their own "registered at X, which does not exist" error rather than a different
failure from in here. More than one is refused outright: two plausible installs
is precisely the ambiguity the registry exists to prevent, and guessing would
undo the point. Set ``BIN_CRAWLER_ASSETS_HOST`` to a root's ``host`` label to
break the tie by name — never by path, since selecting among registered roots
keeps the no-typed-paths invariant intact.

Rejections are subpaths too, so a trap named once is rejected on every machine.
Naming them absolutely (as schema 1 did) silently stopped rejecting the moment
the workstation changed, which is the opposite of what a trap list is for.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

_REGISTRY_PATH = Path(__file__).resolve().parent / "assets_sources.json"
_HOST_ENV = "BIN_CRAWLER_ASSETS_HOST"


class UnknownSource(ValueError):
    """A dataset/ring the registry does not name, or a path it rejects."""


class AmbiguousRoot(UnknownSource):
    """Several registered roots for one dataset exist on this machine."""


def _registry() -> dict:
    return json.loads(_REGISTRY_PATH.read_text())


def datasets() -> list[str]:
    return list(_registry()["datasets"])


def rings(dataset: str) -> list[str]:
    return list(_dataset(dataset)["rings"])


def _dataset(dataset: str) -> dict:
    known = _registry()["datasets"]
    if dataset not in known:
        raise UnknownSource(
            f"Unknown dataset {dataset!r}. Known: {', '.join(known)}"
        )
    return known[dataset]


def _expand(path: str) -> Path:
    """Registry paths may be written home-relative so one entry fits any user."""
    return Path(os.path.expanduser(path))


def _comparable(path: str | Path) -> str:
    """A tree path in the one spelling comparisons may use: symlinks followed.

    BOTH sides of every path-vs-registry comparison go through here. Resolving
    only the ARGUMENT was the original bug: where an install sits behind a
    symlink (this workstation has `~/Games` -> `/mnt/games-1tb/Games`), the
    registry side kept the `~` spelling, so no Sweet Tea path matched either
    side and `rejection_reason` stopped rejecting the `piggs` decoy under BOTH
    spellings while Homecoming's traps, whose root has no symlink, kept firing.
    That is schema 1's disease by another route: a trap that quietly stops
    rejecting when the workstation changes.

    Resolution is applied to the COMPOSED path, not just the root, so a symlink
    anywhere along it cannot reintroduce the mismatch. `resolve()` is
    non-strict, so a root belonging to another workstation still normalises to
    a comparable absolute path instead of raising.

    Deliberately not folded into `_expand`: `root()` and `canonical_path()`
    feed error messages and the `assets_dir` an exporter stamps, and those must
    keep the registry's own spelling rather than shift to this machine's mount.
    """
    return Path(os.path.expanduser(str(path))).resolve().as_posix()


def _roots(dataset: str) -> list[dict]:
    entry = _dataset(dataset)
    roots = entry.get("roots")
    if not roots:
        raise UnknownSource(f"{dataset} names no roots in the registry.")
    return roots


def root_report(dataset: str) -> dict:
    """Which root this machine resolves to, and every candidate considered.

    Exposed so callers can explain themselves — the chosen root, whether it is
    actually present, and what else was on the list — without re-implementing
    the precedence.
    """
    candidates = [
        {"host": r.get("host", "?"), "path": str(_expand(r["path"])),
         "exists": _expand(r["path"]).is_dir()}
        for r in _roots(dataset)
    ]
    present = [c for c in candidates if c["exists"]]

    forced = os.environ.get(_HOST_ENV)
    if forced:
        match = [c for c in candidates if c["host"].lower() == forced.lower()]
        if not match:
            raise UnknownSource(
                f"{_HOST_ENV}={forced!r} matches no root for {dataset}. "
                f"Hosts: {', '.join(c['host'] for c in candidates)}"
            )
        return {"chosen": match[0], "candidates": candidates, "via": _HOST_ENV}

    if len(present) == 1:
        return {"chosen": present[0], "candidates": candidates, "via": "exists"}
    if len(present) > 1:
        raise AmbiguousRoot(
            f"{len(present)} registered roots for {dataset} exist on this machine:\n"
            + "\n".join(f"  {c['host']}: {c['path']}" for c in present)
            + f"\nSet {_HOST_ENV} to one of their host labels, or drop the stale "
              f"root from assets_sources.json."
        )
    # None present: hand back the first candidate so the caller raises its own
    # "does not exist" error, naming a path from the registry as it always has.
    return {"chosen": candidates[0], "candidates": candidates, "via": "fallback"}


def root(dataset: str) -> str:
    """The install directory for `dataset` on this machine."""
    return root_report(dataset)["chosen"]["path"]


def resolve(source: str) -> tuple[str, str, str]:
    """Resolve ``<dataset>[:<ring>]`` to ``(dataset, ring, path)``.

    Defaults to the dataset's exportable ring, so `--source homecoming` means
    the tree users get.
    """
    dataset, _, ring = source.partition(":")
    ring = ring or exportable_ring(dataset)
    entry = _dataset(dataset)
    if ring not in entry["rings"]:
        raise UnknownSource(
            f"{dataset} has no ring {ring!r}. Known: {', '.join(entry['rings'])}"
        )
    subpath = entry["rings"][ring]["subpath"]
    return dataset, ring, str(Path(root(dataset)) / subpath)


def exportable_ring(dataset: str) -> str:
    """The one ring whose bytes may reach a committed export."""
    return _dataset(dataset)["exportable_ring"]


def ring_note(dataset: str, ring: str) -> str:
    """The registry's prose for a ring — what it is and when it gets checked."""
    entry = _dataset(dataset)
    if ring not in entry["rings"]:
        raise UnknownSource(
            f"{dataset} has no ring {ring!r}. Known: {', '.join(entry['rings'])}"
        )
    return entry["rings"][ring].get("note", "")


def _all_roots() -> list[tuple[str, dict, Path]]:
    """Every (dataset, entry, root) pair the registry names, machine or not.

    Path classification walks all of them, not just this workstation's, so a
    tree carried over from another box is still recognised for what it is.
    """
    out = []
    for name, entry in _registry()["datasets"].items():
        for r in entry.get("roots", []):
            out.append((name, entry, _expand(r["path"])))
    return out


def rejection_reason(path: str | Path) -> str | None:
    """Why this tree must not be exported from, if the registry rejects it.

    Rejections are named rather than merely omitted because the traps here look
    like the real thing: Homecoming's `closedbeta` folder is not the closed beta
    (that is `experimental`), its `issue24` folder is an ancient snapshot beside
    the live rings, and the Sweet Tea `piggs` folder resolves `powers.bin` while
    holding an unrelated corpus.
    """
    resolved = _comparable(path)
    for _, entry, base in _all_roots():
        for subpath, reason in entry.get("rejected_subpaths", {}).items():
            if _comparable(base / subpath) == resolved:
                return reason
    return None


def canonical_path(dataset: str) -> str:
    """The path a committed export of `dataset` must have been read from."""
    return resolve(f"{dataset}:{exportable_ring(dataset)}")[2]


def dataset_for_path(path: str | Path) -> tuple[str, str] | None:
    """The ``(dataset, ring)`` this tree is, or None if the registry omits it."""
    resolved = _comparable(path)
    for name, entry, base in _all_roots():
        for ring, ring_entry in entry["rings"].items():
            if _comparable(base / ring_entry["subpath"]) == resolved:
                return name, ring
    return None
