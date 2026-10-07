"""Parser for powersets.bin.

The record is `ParseBasePowerSet` (`Common/entity/powers_load.c:2228`), and the
walk below is that table in order, field for field:

  string   source, key, name            SourceFile, FullName, Name
  u4       system, shared               System (enum), Shared (bool)
  string   display, help, short_help, icon
  string[] costume_keys, costume_parts
  string   account × N                  SetAccountRequires / SetAccountTooltip /
                                        SetAccountProduct — see ACCOUNT_STRINGS
  string[] buy_requires                 SetBuyRequires
  string   buy_requires_failed          SetBuyRequiresFailedText
  u4       show_in_inventory, show_in_manage, show_in_info
  u4       specialize_at                SpecializeAt
  string[] specialize_requires          SpecializeRequires
  string[] powers                       Powers
  s4[]     (Parse6 only) power_indices   the resolved TOK_AUTOINTEARRAY ppPowers
  s4[]     available                    Available
  s4[]     ai_max_level, ai_min_rank_con, ai_max_rank_con,
           min_difficulty, max_difficulty
  <record ends — no bytes remain>

The table's trailing `ForceLevelBought` is absent from every shipped record: the
walk consumes each record to the byte on all three forks (21,055 records), which
is what `_expect_consumed` asserts. That total accounting is the misalignment
detector — a field read at the wrong width leaves a remainder rather than
silently shifting every field after it.

`available` is `const int *piAvailable` in the game (powers.h:1286) and is
compared signed: `baseset_BasePowerAvailableByIdx` returns `piAvailable[i] -
iLevel`, and "<= 0 means available now" (powers.c:545). Authored defs that
state no level requirement store -1, so reading the array unsigned turned that
into 4294967295 and every consumer had to special-case the sentinel.

The account-string count is the ONE thing that varies, and it is measured
rather than assumed (see ACCOUNT_STRINGS). Because an empty string and a zero
u4 are both four zero bytes, a fork carrying an always-empty account slot is
indistinguishable from one carrying an extra zeroed int; Thunderspy is read at
Rebirth's three because it matches Rebirth's schema everywhere else, and the
fields after the slot — `specialize_at`'s 23-on-the-VEAT-branches, the
show_in_* distribution — line up identically on all three forks either way.
"""

from ._reader import open_parse7, BinReader, Parse6BinReader
from ._dataclasses import PowersetRecord


class PowersetLayoutError(ValueError):
    """A record no known layout accounts for — a misalignment, never a skip."""


# How many account strings sit between `costume_parts` and `buy_requires`.
#
# Retail/Parse6 writes three (SetAccountRequires, SetAccountTooltip,
# SetAccountProduct); Homecoming trimmed them to one, except on four records
# (Wind_Control, Gadgetry, Utility_Belt) that carry two. Ordered most-likely
# first — every candidate is validated by the total byte account, so a wrong
# guess cannot survive into a record.
ACCOUNT_STRINGS = {True: (3,), False: (1, 2, 3)}


def parse_powersets(bin_path_or_data) -> list[PowersetRecord]:
    r = open_parse7(bin_path_or_data)
    candidates = ACCOUNT_STRINGS[isinstance(r, Parse6BinReader)]

    r.read_u4()                     # block_size
    count = r.read_u4()

    records = []
    for index in range(count):
        rec_len = r.read_u4()
        records.append(_parse_record(r, rec_len, candidates, index))
        r.skip(rec_len)
    return records


def _parse_record(r, rec_len: int, candidates, index: int) -> PowersetRecord:
    """The one record layout, tried at each known account-string count.

    A record no candidate accounts for RAISES. It used to be counted and
    dropped behind a bare `except: continue`, which is how a whole shifted
    record class stays invisible: the count printed to stderr, the powerset
    vanished from the export, and every gate downstream graded a corpus with a
    hole in it (CLAUDE.md — no silent recovery).
    """
    failures = []
    for n_account in candidates:
        try:
            return _walk(r.sub_reader(rec_len), n_account)
        except Exception as exc:
            failures.append(f"{n_account} account string(s): {exc}")
    raise PowersetLayoutError(
        f"powerset record {index} ({rec_len} bytes) matches no known layout — "
        + "; ".join(failures)
    )


def _walk(sub: BinReader, n_account: int) -> PowersetRecord:
    source = sub.read_string()
    key = sub.read_string()
    sub.read_string()                   # Name — the key already identifies the set
    sub.read_u4()                       # System (kPowerSystem_* enum)
    sub.read_u4()                       # Shared
    display = sub.read_string()
    help_text = sub.read_string()
    short_help = sub.read_string()
    icon = sub.read_string()
    sub.read_string_array()             # CostumeKeys — the costume pieces a set unlocks
    sub.read_string_array()             # CostumeParts
    for _ in range(n_account):
        sub.read_string()               # SetAccount{Requires,Tooltip,Product}

    buy_requires = sub.read_string_array()
    buy_requires_failed = sub.read_string()

    sub.read_u4()                       # ShowInInventory
    sub.read_u4()                       # ShowInManage
    sub.read_u4()                       # ShowInInfo
    specialize_at = sub.read_u4()
    specialize_requires = sub.read_string_array()

    powers = sub.read_string_array()
    if powers and not all("." in p for p in powers):
        raise ValueError("power names missing dots")

    # Parse6 serializes the resolved `ppPowers` pointer array ahead of
    # `Available`; Parse7 does not. Distinguished structurally rather than by
    # fork: the extra array is the one whose length does not match `powers`
    # (it is empty on Rebirth), and reading blindly past it gave Thunderspy an
    # all-zero second array, so every power showed as level 1.
    available = sub.read_s4_array()
    if len(available) != len(powers):
        available = sub.read_s4_array()

    if len(available) != len(powers):
        raise ValueError(
            f"available/powers mismatch: {len(available)} != {len(powers)}"
        )

    # NPC spawn data — which critter levels, ranks and difficulties draw this
    # set. Read into named slots rather than skipped so a width error surfaces
    # here, and deliberately NOT exported: no player build consults them.
    for _ in range(5):                  # AIMaxLevel, AIMinRankCon, AIMaxRankCon,
        sub.read_s4_array()             # MinDifficulty, MaxDifficulty

    force_level_bought = sub.read_s4()  # ForceLevelBought, the table's last field
    _expect_consumed(sub, key, force_level_bought)

    return PowersetRecord(
        source=source,
        key=key,
        display_name=display,
        help=help_text,
        short_help=short_help,
        icon=icon,
        powers=powers,
        available=available,
        buy_requires=buy_requires,
        buy_requires_failed=buy_requires_failed,
        specialize_at=specialize_at,
        specialize_requires=specialize_requires,
    )


def _expect_consumed(sub: BinReader, key: str, force_level_bought: int) -> None:
    """Every byte of the record is accounted for, or the walk is wrong.

    A correct walk lands exactly on the record boundary. Anything left over
    means a field above was read at the wrong width — the one signal that
    catches a shift the field values themselves would look plausible under.

    `ForceLevelBought` is the parse table's last field and is -1 (its default)
    on all 21,055 records across the three forks; it is checked rather than
    exported because a set that forces the level it is bought at would change
    what a pick costs, and this is where that would first be visible.
    """
    left = sub.remaining()
    if left:
        raise ValueError(f"{key}: {left} bytes unaccounted for at end of record")
    if force_level_bought != -1:
        raise ValueError(
            f"{key}: ForceLevelBought is {force_level_bought}, not the default -1 — "
            "a set that forces its buy level is unmodelled, not ignorable"
        )
