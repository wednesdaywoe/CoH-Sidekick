"""Parse boostsets.bin (IO enhancement set definitions).

Each record describes one IO set (Bonesnap, Hecatomb, Call_to_Arms, etc.): the
powers it can slot into, the pieces that make it up, and the bonus tiers those
pieces earn.

The record is the game's own `ParseBoostSet` table (`Common/entity/boostset.h`),
field for field:

    Name              TOK_STRING       -> name
    DisplayName       TOK_STRING       -> display_name
    GroupName         TOK_STRING       -> group_name
    ConversionGroups  TOK_STRINGARRAY  -> conversion_groups
    Powers            TOK_LINKARRAY    -> allowed_powers
    BoostLists        TOK_STRUCT       -> boostlists
    Bonuses           TOK_STRUCT       -> bonuses
    MinLevel          TOK_INT          -> min_level
    MaxLevel          TOK_INT          -> max_level
    StoreProduct      TOK_STRING       -> store_product

Two encodings, one layout. Parse7 (Homecoming, Thunderspy) writes TOK_STRING and
TOK_STRINGARRAY members as u4 offsets into the file's string table; Parse6
(Rebirth) writes them inline. TOK_LINK targets — power full_names — are inline
in both. That is the whole difference, so both formats share `_parse_record`.

`ConversionGroups` is where this reader used to guess. It is a counted array:
`[]`, `["ECVeryRare"]`, or `["ECUncommon", "ECMelee"]`, and its count sits in the
u4 the old reader called an "opaque flag". Reading it instead as two optional
scalars — a rarity, then maybe a category, told apart by sniffing for an "EC"
prefix — worked for every set that states a rarity and misread the one set that
does not: Thunderspy's Overwhelming Force declares zero conversion groups, so its
power count landed in the rarity slot and a 1,905-power set with six pieces and
five bonus tiers decoded into three bytes of garbage. BOOST-1 in
docs/DATA-GAP-REGISTER.md has the full account.

Every record is byte-accounted. `_parse_record` raises if a record does not end
exactly where the struct says it should, which is what turns the next fork's
added field into a failed export rather than authoritative garbage.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Iterable

from ._reader import BinReader, open_parse7, Parse6BinReader


@dataclass
class BoostListEntry:
    """One slot in a set — typically two boost variants (Crafted + Attuned)
    that share the same aspects but differ in attunement behavior."""
    boosts: list[str] = field(default_factory=list)


@dataclass
class BoostBonusEntry:
    """One set bonus tier — fires when between MinBoosts and MaxBoosts pieces
    are slotted. Values come from looking up `auto_powers` in powers.bin."""
    display_text: str = ''
    min_boosts: int = 0
    max_boosts: int = 0
    # The RPN expression gating this tier, in the same token vocabulary as a
    # power's Requires. Two rules live here and nowhere else: `isPVPMap?` marks
    # the PvP-only tiers, and a `<piece> PowerBoostsSlotted> 1 >=` pair marks the
    # global a unique piece grants on its own. Exported, not yet consumed —
    # BONUS-REQ-1 in docs/DATA-GAP-REGISTER.md.
    requires: list[str] = field(default_factory=list)
    auto_powers: list[str] = field(default_factory=list)
    bonus_power: str = ''


@dataclass
class BoostSetRecord:
    name: str
    display_name: str
    # The heading a power files this set under — "Melee Damage", "PBAoE Damage",
    # "Blaster Archetype Sets". Stated on every record on all three forks, and the
    # field the client itself groups by; `_resolve_category` explains why nothing
    # else may stand in for it.
    group_name: str
    # The pools the enhancement converter draws this set from, verbatim. Element 0
    # is the rarity/event group ("ECUncommon", "ECWinter", "ECATO2"); element 1,
    # when the set states one, narrows it ("ECMelee"). Nothing in the corpus states
    # more than two, and `_parse_record` raises if one ever does. Neither element
    # is the slot category, though on Homecoming they happen to imply it.
    conversion_groups: list[str] = field(default_factory=list)
    allowed_powers: list[str] = field(default_factory=list)
    boostlists: list[BoostListEntry] = field(default_factory=list)
    bonuses: list[BoostBonusEntry] = field(default_factory=list)
    min_level: int = 0
    max_level: int = 0
    # Product code the account must own for this set to be convertible. Empty on
    # every Homecoming set; carried by Rebirth's and Thunderspy's Winter
    # ("COWPWIWA") and second-wave ATO ("cosprovi") sets.
    store_product: str = ''
    # One int the Parse6 record carries between MaxLevel and StoreProduct that
    # the Parse7 forks do not. It is 1 on exactly the ten ECVeryRare level-50
    # sets and 0 on the other 223, which is not enough to name it — kept raw so
    # it is recoverable rather than discarded.
    parse6_tail_int: int = 0
    # `conversion_groups` element 0 and element 1 under their own names, filled by
    # `_resolve_planner_fields`. Both verbatim; empty means the record states none.
    rarity: str = ''
    category: str = ''



def _read_link(r: BinReader) -> str:
    """Read one TOK_LINK target — a power full_name.

    Links are stored as the target's name inline (u16 length, chars, padded to
    four bytes) in BOTH formats; only TOK_STRING members move to Parse7's string
    table. That inline read is exactly `Parse6BinReader.read_string`, so borrow
    it rather than keep a second copy of the padding rule.
    """
    return Parse6BinReader.read_string(r)


def _read_link_array(r: BinReader) -> list[str]:
    """Read a TOK_LINKARRAY: u4 count, then that many inline names."""
    return [_read_link(r) for _ in range(r.read_u4())]


def _require_fully_read(sub: BinReader, what: str) -> None:
    """Assert a length-prefixed record ended exactly where the struct says.

    The prefix states the record's size and the parse table states its fields,
    so the two agreeing is the only evidence that the fields were read at the
    offsets they were written to. Every record in all three forks satisfies this;
    a fork that adds a field breaks the export here instead of quietly shifting
    every field after it.
    """
    left = sub.remaining()
    if left:
        raise ValueError(
            f"boostsets.bin: {what} has {left} unread byte(s) — the record does "
            f"not match ParseBoostSet, so its fields cannot be trusted"
        )


def _parse_boostlist(sub: BinReader) -> BoostListEntry:
    """ParseBoostList — one slot's boost variants."""
    entry = BoostListEntry(boosts=_read_link_array(sub))
    _require_fully_read(sub, "a BoostList")
    return entry


def _parse_bonus(sub: BinReader) -> BoostBonusEntry:
    """ParseBoostSetBonus — one set bonus tier."""
    entry = BoostBonusEntry(
        display_text=sub.read_string(),
        min_boosts=sub.read_u4(),
        max_boosts=sub.read_u4(),
        requires=sub.read_string_array(),
        auto_powers=_read_link_array(sub),
        bonus_power=_read_link(sub),
    )
    _require_fully_read(sub, "a set bonus")
    return entry


def _parse_record(sub: BinReader, *, parse6: bool) -> BoostSetRecord:
    """ParseBoostSet — one IO set, every field, in table order."""
    rec = BoostSetRecord(
        name=sub.read_string(),
        display_name=sub.read_string(),
        group_name=sub.read_string(),
        conversion_groups=sub.read_string_array(),
    )
    if len(rec.conversion_groups) > 2:
        raise ValueError(
            f"boostsets.bin: {rec.name} states {len(rec.conversion_groups)} "
            f"conversion groups ({rec.conversion_groups}) — only the rarity and "
            f"category slots have a planner meaning, so a third would be dropped"
        )
    rec.allowed_powers = _read_link_array(sub)
    rec.boostlists = sub.read_struct_array(_parse_boostlist)
    rec.bonuses = sub.read_struct_array(_parse_bonus)
    rec.min_level = sub.read_u4()
    rec.max_level = sub.read_u4()
    if parse6:
        rec.parse6_tail_int = sub.read_u4()
    rec.store_product = sub.read_string()
    _require_fully_read(sub, f"set {rec.name}")
    return rec


def parse_boostsets(bin_path_or_data) -> list[BoostSetRecord]:
    """Parse boostsets.bin into a list of BoostSetRecord."""
    r = open_parse7(bin_path_or_data)
    parse6 = isinstance(r, Parse6BinReader)

    r.read_u4()  # block_size — unused
    count = r.read_u4()
    records = []
    for _ in range(count):
        rec_len = r.read_u4()
        records.append(_parse_record(r.sub_reader(rec_len), parse6=parse6))
        r.skip(rec_len)

    _resolve_planner_fields(records)
    return records


def _resolve_planner_fields(records: list[BoostSetRecord]) -> None:
    """Split `conversion_groups` into its two named slots, verbatim.

    Both slots belong to the enhancement *converter*: the game keys
    `htConversionLists` off them and `boostset_findConversionSet` reads element 0
    to decide what a piece may convert into (`Common/entity/boostset.c`,
    `Game/src/UI/uiConvertEnhancement.c`). Two thirds of the sets state only
    element 0, and a set that states nothing here is simply not convertible —
    absence is the record's answer, not a hole to fill.
    """
    for rec in records:
        rec.rarity = rec.conversion_groups[0] if rec.conversion_groups else ""
        rec.category = (rec.conversion_groups[1]
                        if len(rec.conversion_groups) > 1 else "")



def _resolve_category(s: BoostSetRecord) -> str:
    """The set's slot category — the heading a power lists it under.

    `GroupName` is that heading. The client builds a power's "AllowedBoostCategories"
    tooltip by walking the boostsets attached to it and collecting exactly this
    field (`Game/src/UI/uiCombineSpec.c`), and it is the only name the boostset
    path ever tests: `mapBoostSetGroupNameToBasePowerName` skips a record that
    states none, and nothing anywhere consults `DisplayName` to decide whether a
    record counts.

    Reading `ConversionGroups` instead — which is what this used to do, through an
    EC-label table, a Sprint-pool split and two curated override tables —
    reproduces `GroupName` on all 227 Homecoming sets and drifts on 28 Rebirth and
    26 Thunderspy ones, because the forks renamed categories the table pinned to
    Homecoming's wording. BOOST-2 in docs/DATA-GAP-REGISTER.md has the account.
    """
    return s.group_name


def build_power_category_index(sets: Iterable[BoostSetRecord]) -> dict[str, list[str]]:
    """Build power full_name → sorted list of slot categories it can take.

    This is `boostset_FixPointers` (`Common/entity/boostset.c`) reversed: the game
    walks every boostset and pushes it onto each power in its `Powers` list, with
    no test on any of the record's names. So every record here reaches its powers
    — a dev-looking record is still a record the client attaches.
    """
    idx: dict[str, set[str]] = {}
    for s in sets:
        planner_cat = _resolve_category(s)
        if not planner_cat:
            continue
        for p in s.allowed_powers:
            idx.setdefault(p, set()).add(planner_cat)
    return {k: sorted(v) for k, v in idx.items()}
