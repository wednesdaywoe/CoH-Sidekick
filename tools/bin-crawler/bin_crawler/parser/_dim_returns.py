"""Parsers for the enhancement curve-layer bins: dim_returns.bin (Enhancement
Diversification tier curves) and the boost_effect_*.bin effectiveness tables.

Structure proven against the game's own TextParser tables
(Common/entity/powers_load.c `ParseDimReturnList` and character_attribs.h in
the released CoH server source), which is the authoritative oracle for this
file — better than CoD2 or Mids, both of which only carry derived values:

    DimReturnList        { ReturnSet: DimReturnSet[] }          (struct_array)
    DimReturnSet         { Default:   int
                           Boost:     int[]                     (boost-type enum)
                           AttribReturnSet: AttribDimReturnSet[] }
    AttribDimReturnSet   { Default:   int
                           Attrib:    int[]                     (attrib byte-offsets)
                           Return:    DimReturn[] }
    DimReturn            { Start, Handicap, Basis: f32 }        (12-byte element)

Semantics (powers_load.c `CreateDimReturnsData`): a power whose
boosts-allowed list contains one of a set's Boost types gets that set's tier
curves on the listed attributes; the Default set/curve (Schedule A) covers
everything unclaimed. This is the binary source for the ED schedule
assignment the planner previously hand-authored: Range / Buff_Defense /
Res_Damage / Buff_ToHit / Debuff_ToHit -> B, Interrupt -> C, Knockback -> D,
default -> A, with per-tier (start, handicap) = the ED thresholds and
effectiveness multipliers.

All three shipped datasets carry the file: HC and Thunderspy as Parse7,
Rebirth as Parse6 (Files1 container) — same field layout, no strings either
way. Attrib offsets and boost enum values are kept RAW alongside their
resolved names; the caller passes the dataset-correct enum tables.

boost_effect_above.bin / boost_effect_below.bin / boost_effect_boosters.bin
share one trivial layout — a single float array (`ParseBoostEffectivenessTable`
in boost.h: `{ "Effectiveness", TOK_EARRAY | TOK_F32_X }`). Above/below are
enhancement effectiveness at (boost level − character level) steps 0..3;
boosters is effectiveness by booster (+1..+5) combine count, the binary
source of the "+5% per boost level" rule.
"""

from ._dataclasses import AttribDimReturns, DimReturnSetRecord, DimReturnTier
from ._reader import open_parse7

# A DimReturn element is exactly {Start, Handicap, Basis} — three f32s. Any
# other element size means the layout shifted and nothing after it can be
# trusted.
_DIM_RETURN_ELEMENT_SIZE = 12

# Plausibility bound for struct-array counts. The shipped files carry 8 sets
# of 1 curve each; triple digits means we are reading data as a count.
_MAX_PLAUSIBLE_COUNT = 64


def _checked_count(value: int, what: str) -> int:
    if value > _MAX_PLAUSIBLE_COUNT:
        raise ValueError(f"implausible {what} count {value} — misaligned read")
    return value


def parse_dim_returns(bin_path_or_data, *, boost_type: dict[int, str],
                      attrib_name: dict[int, str]) -> list[DimReturnSetRecord]:
    """Parse dim_returns.bin into DimReturnSetRecord list.

    `boost_type` / `attrib_name` are the dataset-correct enum tables
    (BOOST_TYPE[_REBIRTH], ATTRIB_NAME[_REBIRTH|_THUNDERSPY] from _enums).
    Unknown values resolve to "Unknown(n)" so a new enum slot surfaces in the
    export instead of vanishing. Raises ValueError on any structural
    misalignment; every byte of every record must be consumed.
    """
    r = open_parse7(bin_path_or_data)
    r.read_u4()  # data block size (validated implicitly by full consumption)

    sets: list[DimReturnSetRecord] = []
    set_count = _checked_count(r.read_u4(), "DimReturnSet")
    for _ in range(set_count):
        set_size = r.read_u4()
        s = r.sub_reader(set_size)

        set_default = s.read_u4()
        boosts_raw = s.read_u4_array()
        _checked_count(len(boosts_raw), "Boost")

        returns: list[AttribDimReturns] = []
        return_count = _checked_count(s.read_u4(), "AttribDimReturnSet")
        for _ in range(return_count):
            ret_size = s.read_u4()
            rr = s.sub_reader(ret_size)

            ret_default = rr.read_u4()
            attribs_raw = rr.read_u4_array()
            _checked_count(len(attribs_raw), "Attrib")

            tiers: list[DimReturnTier] = []
            tier_count = _checked_count(rr.read_u4(), "DimReturn")
            for _ in range(tier_count):
                elem_size = rr.read_u4()
                if elem_size != _DIM_RETURN_ELEMENT_SIZE:
                    raise ValueError(
                        f"DimReturn element size {elem_size} != "
                        f"{_DIM_RETURN_ELEMENT_SIZE} — layout shifted")
                tiers.append(DimReturnTier(
                    start=rr.read_f4(),
                    handicap=rr.read_f4(),
                    basis=rr.read_f4(),
                ))

            if rr.remaining():
                raise ValueError(
                    f"AttribDimReturnSet has {rr.remaining()} unread bytes")
            s.skip(ret_size)

            returns.append(AttribDimReturns(
                is_default=bool(ret_default),
                attribs=[attrib_name.get(a // 4, f"Unknown({a // 4})")
                         for a in attribs_raw],
                attribs_raw=attribs_raw,
                tiers=tiers,
            ))

        if s.remaining():
            raise ValueError(f"DimReturnSet has {s.remaining()} unread bytes")
        r.skip(set_size)

        sets.append(DimReturnSetRecord(
            is_default=bool(set_default),
            boost_types=[boost_type.get(b, f"Unknown({b})")
                         for b in boosts_raw],
            boost_types_raw=boosts_raw,
            returns=returns,
        ))

    if r.remaining():
        raise ValueError(f"dim_returns.bin has {r.remaining()} trailing bytes")
    if not any(rec.is_default for rec in sets):
        raise ValueError("dim_returns.bin has no default DimReturnSet — "
                         "the Schedule-A catch-all is missing")
    return sets


def parse_boost_effect(bin_path_or_data) -> list[float]:
    """Parse one boost_effect_*.bin (above / below / boosters): a single
    float array. Raises ValueError on trailing bytes or implausible count."""
    r = open_parse7(bin_path_or_data)
    r.read_u4()  # data block size
    count = _checked_count(r.read_u4(), "Effectiveness")
    values = [r.read_f4() for _ in range(count)]
    if r.remaining():
        raise ValueError(
            f"boost_effect table has {r.remaining()} trailing bytes")
    return values
