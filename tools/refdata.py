"""Where the non-shippable reference inputs live, now that they are not in the tree.

Two inputs this project reads are neither ours to ship nor present in any game
install: Homecoming's authored `.powers` source defs, and the third-party
Thunderspy Mids database drop. They used to sit in the repo as gitignored
directories, which is the worst of both — `git clone` produced a checkout where
every path resolved to nothing, and a gate whose input is missing self-skips.
A self-skip reads exactly like a pass (the reasoning defdiff.py states at its
`DEFS`, aimed at itself).

So they live in a reference tree OUTSIDE the repo, resolved here, and this module
is the ONE place that says where. What absence means is the caller's to decide,
and the two answers are not interchangeable:

  * A gate over the `.powers` defs SKIPS, loudly, naming the variable. The defs
    are private and CI has never had them, so demanding them would turn every CI
    run red for a file it cannot be given. `required=False` is for those callers.
  * Everything else STOPS, naming the variable. That is the default, because the
    alternative is resolving to nothing and continuing, which reads like a pass.

A caller taking `required=False` still owes the reader a printed reason. Silence
is the failure this module exists to prevent, and it looks identical either way.

Every caller today passes `required=False`, because every one of them already had
its own check and its own message. The checked default is kept anyway, and kept as
the DEFAULT, so the next caller has to opt out of the guard rather than remember to
opt in. Both times this project lost data to a missing reference tree, it was to a
path nobody had thought to check.

The default is a SIBLING of the repo, not an absolute path. An absolute default
is right on exactly one machine and silently wrong on every other, which is the
failure PROV-3 hit when `MidsReborn-master/` was the default: the tree existed
on the machine the path was typed on and nowhere else.

Layout under the reference root:

    <root>/raw defs/            Homecoming authored .powers defs (4,943 files)
    <root>/Thunderspy/          The third-party Mids database drop
    <root>/MidsReborn-master/   Superseded vendored Mids databases, kept for history

Overrides, each an absolute path:

    COH_REFDATA     the reference root (default: ../coh-sidekick-refdata)
    COH_RAW_DEFS    the .powers defs alone, if they sit elsewhere

Mids Reborn is NOT resolved here. It is a real install, and `read_i12.py` already
spells its path once for everything downstream to derive from — the single-string
rule PROV-3 established. Adding a second spelling here is the defect that rule
exists to prevent.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

DEFAULT_REFDATA = REPO_ROOT.parent / "coh-sidekick-refdata"


def _require(path: Path, what: str, env_var: str) -> Path:
    """Return `path`, or stop the run explaining which variable points at `what`."""
    if path.exists():
        return path
    sys.exit(
        f"{what} not found at:\n"
        f"  {path}\n"
        f"It is not in the repo and not in any game install — see tools/refdata.py.\n"
        f"Set {env_var} to its location, or put it under {refdata_root()}."
    )


def refdata_root() -> Path:
    """The reference tree. Not checked for existence — callers ask for a member."""
    return Path(os.environ.get("COH_REFDATA") or DEFAULT_REFDATA).expanduser()


def raw_defs(*, required: bool = True) -> Path:
    """Homecoming's authored `.powers` defs — the parser's completeness oracle.

    `required=False` returns the path unchecked. The two gates over these defs take
    it because the defs are private and CI has never held them; both print a SKIP
    naming COH_RAW_DEFS rather than going quiet.
    """
    override = os.environ.get("COH_RAW_DEFS")
    path = Path(override).expanduser() if override else refdata_root() / "raw defs"
    if not required:
        return path
    return _require(path, "The `raw defs/` .powers oracle", "COH_RAW_DEFS")


def thunderspy_drop(filename: str, *, required: bool = True) -> Path:
    """One file from the Thunderspy database drop, e.g. `I12.mhd`.

    Only `I12.mhd` is the drop's own: it is the fork's rebuilt powers database and
    exists nowhere else. Everything beside it is Mids' Generic database byte for
    byte — `EnhDB.mhd` included — so read those from an installed Mids' `Generic`
    directory rather than from here. Verified by sha256, 2026-09-24.

    `required=False` returns the path unchecked, for a caller that resolves at
    import time and has its own check at the point of use. It is not permission to
    skip: a caller that takes it still has to stop on a miss.
    """
    path = refdata_root() / "Thunderspy" / filename
    if not required:
        return path
    return _require(path, f"The Thunderspy database drop's `{filename}`", "COH_REFDATA")
