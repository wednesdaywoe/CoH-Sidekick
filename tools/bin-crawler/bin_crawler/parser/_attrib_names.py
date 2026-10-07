"""Parse the mode-name table out of ``attrib_names.bin``.

Background
----------
A ``Set_Mode`` AttribMod's ``magnitude`` field is not a stat value — it is a
1-based index into the game's global **mode** registry (the ``ppchMode``
sub-array of the ``AttribNames`` struct in ``attrib_names.bin``). In the
human-readable ``.powers`` source that index is written as the mode token
itself, e.g. ``Magnitude kDomination_Active`` / ``Magnitude kGranite_Mode``.
The compiled bin stores the resolved index (``Domination_Active`` -> 47),
which is opaque without this table.

``attrib_names.bin`` is otherwise never read by the parser (``ATTRIB_NAME`` in
``_enums.py`` is a hand-maintained dict); this module is the sole consumer.

Binary layout
-------------
Parse7 ``CrypticS`` header + string table, then the ``AttribNames`` struct
(a length-prefixed sequence of sub-arrays: Damage / Defense / Boost / Group /
**Mode** / ...). The mode sub-array is a run of fixed 16-byte records::

    u4 flag (0)   u4 type (12)   str name (offset)   str display (offset)

where ``name`` is a C-identifier mode token (``Domination``, ``Granite_Mode``,
``DefensiveAdaptation``) and ``display`` is its human label (``Domination``,
``Granite Form``, ...). We locate the mode array as the longest contiguous run
of such records (the ``type==12`` + identifier-name shape is distinctive). The
``Set_Mode`` magnitude is a direct index into that run — its first record
(index 0) is the ``ServerTrayOverride`` system slot, so ``Peacebringer_Blaster_Mode``
is 1, ``Domination`` 45, ``Domination_Active`` 46, ``RestedAdaptation`` 153.

Verification (HC, ``assets/live`` 2026): 214 records (indices 0..213); broad
cross-check against the authoritative ``.powers`` oracle resolved **183/184**
``Set_Mode`` templates (mag>=2) to the exact oracle mode token (the 1 outlier is
a mode that postdates the oracle snapshot, not a mapping error). Mode indices are
**per-server** — each server's ``attrib_names.bin`` reorders them, so parse the
table from the same source you parsed ``powers.bin`` from.

Parse6 (Rebirth) layout differs: no string table. The payload is a u4 total
size followed by the AttribNames sub-arrays back to back, each a u4 count of
size-prefixed records (u4 byte size EXCLUDING the size word itself, then
inline pascal strings: name, display, plus one trailing u4). The mode
sub-array is identified by its anchor: its first record is always the
``ServerTrayOverride`` system slot (index 0), same as HC. Verified (Rebirth
2026-07-07): 7 sub-arrays (Damage/Defense/Boost/Group/Mode/Elusivity/
StackKeys), mode array = 139 records; index 36 ``Domination`` / 37
``Domination_Active`` / 98 ``FastMode`` / 113 ``DefensiveAdaptation`` confirmed
against powers.bin mode gates (Titan Weapons Follow_Through requires 98, DP
ammo toggles require 73, everything disallows 18 ``Disable_All`` — mirroring
HC's exports). Thunderspy's attrib_names.bin is Parse7-wrapped and parses via
the Parse7 heuristic unchanged (123 modes, same anchor).

Note on mag==1: mode #1 is ``Peacebringer_Blaster_Mode`` (Bright Nova). This
used to be unresolvable because attrib index 118 collapsed three distinct
engine attribs — ``kXPDebtProtection`` / ``kSetMode`` / ``kSetCostume`` — onto
``Set_Mode`` (the ``raw // 4`` truncation bug), so a mag==1 ``Set_Mode`` could
be a genuine mode set OR a misdecoded costume/XP-debt template. That is now
fixed at the source: ``resolve_attrib`` reads the byte-granular sub-index, so
only genuine ``kSetMode`` templates (raw 473) are labeled ``Set_Mode`` and
mag==1 resolves correctly to ``Peacebringer_Blaster_Mode``. Callers no longer
need a ``mag >= 2`` gate (mode #0 ``ServerTrayOverride`` is the only system slot
to skip). See HOMECOMING_PARSER.md "attrib-118 misdecode".
"""

from __future__ import annotations

import re

from ._reader import BinReader, Parse6BinReader, open_parse7

_IDENT_RE = re.compile(r"^[A-Za-z][A-Za-z0-9_]*$")

# The mode sub-array's first record on every known server: a system slot no
# player power sets. Used as the anchor to identify the mode array among the
# same-shaped AttribNames sub-arrays in the Parse6 layout.
_MODE_ANCHOR = "ServerTrayOverride"

# Minimum plausible length for the mode run — guards against a stray short run
# of same-shaped records being mistaken for the mode table.
_MIN_RUN = 32

# Per-mode record stride in bytes: flag(u4) type(u4) name(str) display(str).
_REC = 16


def _scan_record_runs_parse7(br: BinReader) -> list[list[str]]:
    """Scan a Parse7 ``attrib_names.bin`` for every contiguous run of the
    16-byte AttribNames records (flag u4, type u4 == 12, name str, display
    str) and return each run's name list, in file order. The AttribNames
    sub-arrays (Damage / Defense / Boost / Group / Mode / Elusivity /
    StackKeys) all share this record shape, so callers pick their sub-array
    by run property (longest for Mode, marker names for StackKeys).
    """
    buf = br._data
    strtab_base = br._strtab_base
    strtab = br._strtab_data
    n = len(buf)

    def u4(pos: int) -> int:
        return int.from_bytes(buf[pos:pos + 4], "little")

    def resolve(off: int) -> str | None:
        if off == 0:
            return ""
        p = strtab_base + off
        if p < 0 or p >= len(strtab):
            return None
        e = p
        while e < len(strtab) and strtab[e] != 0:
            e += 1
        try:
            return bytes(strtab[p:e]).decode("ascii")
        except UnicodeDecodeError:
            return None

    def record_name(pos: int) -> str | None:
        """If a valid record starts at ``pos``, return its name, else None."""
        if pos + _REC > n:
            return None
        if u4(pos + 4) != 12:  # the record `type` constant
            return None
        name = resolve(u4(pos + 8))
        if not name or not _IDENT_RE.match(name):
            return None
        disp = resolve(u4(pos + 12))  # display: may be empty or contain spaces
        if disp is None:
            return None
        return name

    runs: list[list[str]] = []
    pos = br.pos
    while pos + _REC <= n:
        if record_name(pos) is not None:
            names: list[str] = []
            while (nm := record_name(pos)) is not None:
                names.append(nm)
                pos += _REC
            runs.append(names)
        else:
            pos += 4
    return runs


def parse_mode_table(data: bytes) -> dict[int, str]:
    """Return ``{mode_index: mode_name}`` (1-based) from ``attrib_names.bin``.

    Returns an empty dict if the mode array cannot be located (e.g. an
    unexpected layout on a server variant), so callers degrade to emitting no
    mode names rather than crashing.
    """
    try:
        br = open_parse7(data)
    except Exception:
        return {}
    if isinstance(br, Parse6BinReader):
        return _parse_mode_table_parse6(br)
    try:
        br = BinReader(data)
    except Exception:
        return {}

    # The mode sub-array is the longest record run in the file.
    runs = _scan_record_runs_parse7(br)
    best = max(runs, key=len, default=[])
    if len(best) < _MIN_RUN:
        return {}

    # The ``Set_Mode`` magnitude is a direct index into this run: the run's
    # first record is index 0 (``ServerTrayOverride``, a system slot no player
    # power sets), so ``Peacebringer_Blaster_Mode`` is 1, ``Domination`` is 45,
    # ``Domination_Active`` 46, ``RestedAdaptation`` 153, and so on.
    return {i: name for i, name in enumerate(best) if name}


def _parse_mode_table_parse6(br: Parse6BinReader) -> dict[int, str]:
    """Parse6 (Rebirth) variant: walk the AttribNames sub-arrays and return
    the one anchored by ``ServerTrayOverride``. Each sub-array is a u4 count
    of records; each record is a u4 byte size (excluding the size word)
    followed by two inline pascal strings (name, display) and a trailing u4.
    """
    try:
        br.read_u4()  # total payload size
        while br.remaining() >= 4:
            count = br.read_u4()
            if count > 10000:  # implausible — bail rather than loop on garbage
                return {}
            names: list[str] = []
            for _ in range(count):
                if br.remaining() < 4:
                    return {}
                size = br.read_u4()
                start = br.pos
                name = br.read_string()
                consumed = br.pos - start
                if size < consumed or size > br.remaining() + consumed:
                    return {}
                br.skip(size - consumed)  # display string + trailing u4
                names.append(name)
            if names and names[0] == _MODE_ANCHOR:
                return {i: n for i, n in enumerate(names) if n}
    except Exception:
        pass
    return {}


# The StackKeys registry's first record on every known server (HC, Rebirth,
# Thunderspy): engine test filler no shipped power references. Used as the
# anchor to identify the StackKeys sub-array among the same-shaped
# AttribNames runs, exactly like _MODE_ANCHOR for the mode table. (The
# registry CONTENT is server-specific — HC added TravelBuff/StealthToggle/
# TravelMaxBuff etc. post-i25; Rebirth and Thunderspy still carry the short
# i25-era list — so content markers can't identify it.)
_STACK_KEY_ANCHOR = "TestStack"


def parse_stack_key_table(data: bytes) -> dict[int, str]:
    """Return ``{stack_key_id: key_name}`` from ``attrib_names.bin``'s
    StackKeys registry (e.g. ``{28: "TravelBuff", 27: "StealthToggle"}`` on
    HC). Returns an empty dict when the registry can't be located so callers
    degrade to emitting placeholder ``Key<N>`` names rather than crashing.

    The serialized ID is a DIRECT 0-based index into the registry on every
    known format, with 0xFFFFFFFF (-1) = no key (normalized to id 0 by the
    template parsers; registry[0] is the unreferenced TestStack filler, so
    the collision is moot):

    - **Parse7 (HC)**: verified 10/10 against the CoD2 ``.powers`` oracle —
      kPvPMezProt id=2, kIcyBastion 24, kPowerBoost 25, kStealthToggle 28,
      kTravelBuff 29, kTravelDebuff 30, kTravelMaxBuff 31, ... kHasteBuff 35.
    - **Parse6 (Rebirth)**: verified structurally — Rebirth Support_Genome
      damage-buff templates carry id 1 → registry[1] = HybridSupport,
      defense templates id 2 → HybridDefense, mirroring HC's Support_Genome
      key assignment.

    (Thunderspy's powers.bin goes through the heuristic tail-scan template
    parser, which does not decode stack fields at all — its exports carry no
    stack keys regardless of this table.)
    """
    try:
        br = open_parse7(data)
    except Exception:
        return {}
    if isinstance(br, Parse6BinReader):
        return _parse_stack_key_table_parse6(br)
    try:
        br = BinReader(data)
    except Exception:
        return {}

    for names in _scan_record_runs_parse7(br):
        if names and names[0] == _STACK_KEY_ANCHOR:
            return {i: name for i, name in enumerate(names) if name}
    return {}


def _parse_stack_key_table_parse6(br: Parse6BinReader) -> dict[int, str]:
    """Parse6 (Rebirth) variant: same sub-array walk as the mode table, but
    selecting the sub-array anchored by ``TestStack`` (Rebirth's AttribNames
    order puts StackKeys after Mode/Elusivity, so the walk continues past the
    mode array instead of returning at its anchor). Direct indexing — see
    parse_stack_key_table.
    """
    try:
        br.read_u4()  # total payload size
        while br.remaining() >= 4:
            count = br.read_u4()
            if count > 10000:  # implausible — bail rather than loop on garbage
                return {}
            names: list[str] = []
            for _ in range(count):
                if br.remaining() < 4:
                    return {}
                size = br.read_u4()
                start = br.pos
                name = br.read_string()
                consumed = br.pos - start
                if size < consumed or size > br.remaining() + consumed:
                    return {}
                br.skip(size - consumed)  # display string + trailing u4
                names.append(name)
            if names and names[0] == _STACK_KEY_ANCHOR:
                return {i: n for i, n in enumerate(names) if n}
    except Exception:
        pass
    return {}
