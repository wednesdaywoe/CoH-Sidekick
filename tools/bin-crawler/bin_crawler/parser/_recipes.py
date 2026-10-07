"""Parser for baserecipes.bin — the DetailRecipe catalog.

DetailRecipes are the game's generic "consume things, receive a thing" records:
base-construction recipes, merit vouchers, costume unlocks — and the whole
incarnate crafting system. An incarnate ability's craft is one of these:
`Salvage` lists the incarnate salvage consumed, `PowerComponent` lists the
prerequisite incarnate POWERS consumed (a Tier 2 consumes the Tier 1, a Tier 4
consumes two Rares), and `IncarnateReward` names the ability granted. The
thread→salvage store is here too, as the `Conversion|…` tab's recipes.

Structure oracle: `Common/bases/DetailRecipe.c` `parse_detailrecipe` in the
released Homecoming server source (CoH2/source/Source-develop). The shipped
bins are NEWER than that snapshot, and the drift was mapped empirically against
the live records (every record of all three forks closes byte-exact under the
layouts below; a leftover byte raises):

- `Reward` (pchInventReward) serializes as a single string, not the snapshot's
  TOK_STRINGARRAY.
- Homecoming carries SIX reward slots where stock carries five, in a different
  order: invent, enhancement, power, incarnate, one corpus-wide-empty slot
  (position of stock's RecipeReward; exported raw as `reward_slot_5` so it
  fails visible the day it populates), then a ContactReward
  (`Alignment_Missions/….contact` values). Rebirth/Thunderspy carry the stock
  five: invent, enhancement, recipe, power, incarnate.
- Homecoming carries SEVEN trailing display strings where stock carries five.
  The first five are the snapshot's (CreateRequiresFail, ReceiveRequiresFail,
  Receive, AccountItemPurchase, ClaimConfirmation); the two Homecoming
  additions are exported positionally in `display_strings`.

Container: HC and Thunderspy are Parse7 (strings as table offsets); Rebirth is
Parse6 (inline pascal strings). TOK_LINK targets are inline pascal strings in
BOTH containers — the same rule `_boostsets._read_link` records.
"""
import struct
from dataclasses import dataclass, field

from ._reader import BinReader, Parse6BinReader, open_parse7


@dataclass
class RecipeComponent:
    """One consumed ingredient: `amount` of the linked salvage item or power."""
    amount: int
    name: str


@dataclass
class DetailRecipeRecord:
    source_file: str = ''
    name: str = ''
    display_name: str = ''            # P-hash message key
    display_help: str = ''            # P-hash message key
    display_short_help: str = ''
    icon: str = ''
    display_tab_name: str = ''        # P-hash → "Alpha|Vigor|Uncommon"
    detail_icon: str = ''
    workshops: list[str] = field(default_factory=list)
    salvage: list[RecipeComponent] = field(default_factory=list)
    power_components: list[RecipeComponent] = field(default_factory=list)
    additional_components: list[str] = field(default_factory=list)
    detail_reward: str = ''
    invent_reward: str = ''
    enhancement_reward: str = ''
    recipe_reward: str = ''           # stock slot; Homecoming's is `reward_slot_5`
    power_reward: str = ''
    incarnate_reward: str = ''
    reward_slot_5: str = ''           # HC-only; empty corpus-wide, kept raw
    contact_reward: str = ''          # HC-only
    visible_requires: list[str] = field(default_factory=list)
    rarity: int = 0
    level: int = 0
    level_min: int = 0
    level_max: int = 0
    creation_cost: list[str] = field(default_factory=list)
    sell_to_vendor: int = 0
    buy_from_vendor: int = 0
    num_uses: int = 0
    type: int = 0
    max_inv_amount: int = 0
    creates_enhancement: int = 0
    creates_inspiration: int = 0
    creates_salvage: int = 0
    creates_recipe: int = 0
    creates_requires: list[str] = field(default_factory=list)
    receive_requires: list[str] = field(default_factory=list)
    never_receive_requires: list[str] = field(default_factory=list)
    auction_requires: list[str] = field(default_factory=list)
    display_strings: list[str] = field(default_factory=list)
    flags: int = 0


def _read_link(r: BinReader) -> str:
    """One TOK_LINK target — the linked record's name, inline in both formats."""
    return Parse6BinReader.read_string(r)


def _read_link_array(r: BinReader) -> list[str]:
    return [_read_link(r) for _ in range(r.read_u4())]


def _require_fully_read(sub: BinReader, what: str) -> None:
    left = sub.remaining()
    if left:
        raise ValueError(
            f"baserecipes.bin: {what} has {left} unread byte(s) — the record "
            f"does not match the DetailRecipe layout, so its fields cannot be "
            f"trusted"
        )


def _parse_component(sub: BinReader) -> RecipeComponent:
    comp = RecipeComponent(amount=sub.read_u4(), name=_read_link(sub))
    _require_fully_read(sub, "a recipe component")
    return comp


# The two shipped record layouts (module doc). Which reward slots exist, in
# wire order, and how many trailing display strings follow the requires block.
_HC_REWARD_SLOTS = (
    'invent_reward', 'enhancement_reward', 'power_reward',
    'incarnate_reward', 'reward_slot_5', 'contact_reward',
)
_STOCK_REWARD_SLOTS = (
    'invent_reward', 'enhancement_reward', 'recipe_reward',
    'power_reward', 'incarnate_reward',
)
_DISPLAY_STRING_COUNT = {True: 7, False: 5}


def _parse_record(sub: BinReader, *, hc_schema: bool) -> DetailRecipeRecord:
    rec = DetailRecipeRecord(
        source_file=sub.read_string(),
        name=sub.read_string(),
        display_name=sub.read_string(),
        display_help=sub.read_string(),
        display_short_help=sub.read_string(),
        icon=sub.read_string(),
        display_tab_name=sub.read_string(),
        detail_icon=_read_link(sub),
        workshops=_read_link_array(sub),
    )
    rec.salvage = sub.read_struct_array(_parse_component)
    rec.power_components = sub.read_struct_array(_parse_component)
    rec.additional_components = sub.read_string_array()
    rec.detail_reward = _read_link(sub)
    for slot in (_HC_REWARD_SLOTS if hc_schema else _STOCK_REWARD_SLOTS):
        setattr(rec, slot, sub.read_string())
    rec.visible_requires = sub.read_string_array()
    rec.rarity = sub.read_s4()
    rec.level = sub.read_s4()
    rec.level_min = sub.read_s4()
    rec.level_max = sub.read_s4()
    rec.creation_cost = sub.read_string_array()
    rec.sell_to_vendor = sub.read_s4()
    rec.buy_from_vendor = sub.read_s4()
    rec.num_uses = sub.read_s4()
    rec.type = sub.read_s4()
    rec.max_inv_amount = sub.read_s4()
    rec.creates_enhancement = sub.read_s4()
    rec.creates_inspiration = sub.read_s4()
    rec.creates_salvage = sub.read_s4()
    rec.creates_recipe = sub.read_s4()
    rec.creates_requires = sub.read_string_array()
    rec.receive_requires = sub.read_string_array()
    rec.never_receive_requires = sub.read_string_array()
    rec.auction_requires = sub.read_string_array()
    rec.display_strings = [
        sub.read_string() for _ in range(_DISPLAY_STRING_COUNT[hc_schema])
    ]
    rec.flags = sub.read_u4()
    _require_fully_read(sub, f"recipe {rec.name}")
    return rec


def _detect_schema(r: BinReader, first_rec_len: int) -> bool:
    """True = Homecoming layout, False = stock.

    Decided by trial parse, not by container: Thunderspy is a Parse7 file
    carrying the STOCK layout, so the reader class cannot answer this. The two
    layouts differ by 12 bytes of fixed-size fields, so on any record exactly
    one closes byte-exact; a corpus where neither does is a schema change that
    must stop the export rather than ship a guess. `sub_reader` never advances
    the parent, so probing consumes nothing.
    """
    for candidate in (True, False):
        try:
            _parse_record(r.sub_reader(first_rec_len), hc_schema=candidate)
        except (ValueError, struct.error):
            continue
        return candidate
    raise ValueError(
        "baserecipes.bin: the first record closes under neither the Homecoming "
        "nor the stock DetailRecipe layout — the schema changed; refusing to guess"
    )


def parse_recipes(bin_path_or_data) -> list[DetailRecipeRecord]:
    """Parse baserecipes.bin into a DetailRecipeRecord list, byte-accounted.

    Raises on any record that does not close exactly under the fork's layout.
    """
    r = open_parse7(bin_path_or_data)
    r.read_u4()  # block_size
    count = r.read_u4()
    if count == 0:
        raise ValueError("baserecipes.bin: zero records")

    records: list[DetailRecipeRecord] = []
    hc_schema = None
    for _ in range(count):
        rec_len = r.read_u4()
        if hc_schema is None:
            hc_schema = _detect_schema(r, rec_len)
        records.append(_parse_record(r.sub_reader(rec_len), hc_schema=hc_schema))
        r.skip(rec_len)
    return records
