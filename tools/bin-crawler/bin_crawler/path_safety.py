"""Is a parsed binary string safe to use as one path segment — SECURITY_AUDIT.md F78.

`export_powers.py` builds `exported_powers/<category>/<powerset>/` out of two
strings read from powers.bin, and every gate downstream treats that tree as
ground truth. Nothing between the binary and the filesystem looks at them: a
category of `..` writes a directory up, and `exported_powers/` is a trust root
nothing hashes, so the write leaves no trace to find afterwards either.

Wired into `_write_power_tree` at two call sites, and WHAT THAT COST is the part
worth reading. `_export_fingerprint.py` hashes `export_powers.py` byte for byte
and `src/data/export-staleness.test.ts` asserts every dataset's recorded
fingerprint equals the current source's, so adding the import declared all four
committed exports stale and the only green path back was re-running the export
from the gitignored `.pigg` archives. That was done: the wiring is provably
inert, zero of the 79,297 committed power files moved, and the whole diff is
this call plus a fingerprint bump in fourteen manifests.

This module is NOT itself inside that fingerprint — the glob is
`parser/**/*.py` plus the exporter entry modules, and this file is neither. So a
future change to the rule below changes what the exporter writes and no
staleness gate notices. That is true of every helper under `bin_crawler/` that
an exporter imports (`assets_dir.py` has the same property), and it is why
`tests/test_export_path_components.py` grades the rule against the whole
committed tree rather than leaning on the fingerprint to notice.
"""
from __future__ import annotations

import re

# Characters no path component may carry: the Windows-reserved set, both
# separators, and the C0 controls.
_UNSAFE_IN_COMPONENT = re.compile(r'[<>:"/\\|?*\x00-\x1f]')

# Windows opens these as devices whatever the extension, so a file named for one
# is not a file. No exported name is one today; this is here so that a fork that
# ships a `Con` powerset is a loud failure rather than a silent write to a port.
_WINDOWS_DEVICE_NAMES = (
    {'con', 'prn', 'aux', 'nul'}
    | {f'com{i}' for i in range(1, 10)}
    | {f'lpt{i}' for i in range(1, 10)}
)


def safe_path_component(value, what, permit_edge_dots=False):
    """Return `value` unchanged, or raise if it cannot be one path segment.

    A validator, not a rewriter. Rewriting would rename files in every committed
    tree; refusing is a no-op until the day a parse produces something that is
    not a name, which is the day the export should stop (Rule 1).

    `permit_edge_dots` is for the leaf filename only.
    `v_arachnos/explosive_drone/.json` ships in four forks, from a power whose
    name parses empty — a data question, not a traversal, so the leaf tolerates
    it rather than breaking the export over it.
    """
    if not isinstance(value, str) or not value:
        raise ValueError(f"{what} is empty or not a string: {value!r}")
    if value in ('.', '..'):
        raise ValueError(f"{what} is a traversal component: {value!r}")
    if _UNSAFE_IN_COMPONENT.search(value):
        raise ValueError(
            f"{what} carries a separator or reserved character: {value!r}. "
            f"It is read from the binary and used as a directory name, so it "
            f"must be one path segment (Rule 1)."
        )
    if not permit_edge_dots and value.strip(' .') != value:
        raise ValueError(f"{what} begins or ends with a space or dot: {value!r}")
    if value.rsplit('.', 1)[0].lower() in _WINDOWS_DEVICE_NAMES:
        raise ValueError(f"{what} is a Windows device name: {value!r}")
    return value
