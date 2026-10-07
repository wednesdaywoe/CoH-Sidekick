"""Parser for villaindef.bin (entity / villain / pet definitions).

The bin holds ~8,800 entity records covering all NPC critters, AVs, and the
pet entities the planner actually cares about (`Pets_*`, `MastermindPets_*`,
`IncarnatePets_*`, `Villain_Pets_*`).

The full record format is rich (gender, badge stats, conditions, costumes,
etc.). The planner only consumes a slice of it via
`scripts/convert-pet-entities.cjs`:

  - `name`
  - `commandable_pet`
  - `copy_creator_mods`
  - `defaults.character_class_name`
  - `defaults.powers` (the wildcard-expandable [cat, set, pow, level] list)
  - `levels[0].display_names[0]` (primary display name)

Every flavor reads the whole record: the leading scalar block, the powers and
levels struct_arrays, then the trailing block field by field down
`ParseVillainDef[]` (`Common/gameComm/VillainDef.c:165`), with anything past
`Flags` preserved verbatim as `tail_raw`.

The one schema axis that differs is the `levels` element, and it does not track
the container format — see `detect_level_ints`.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from ._reader import open_parse7, BinReader, Parse6BinReader


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class EntityPower:
    """One entry in `defaults.powers` — a wildcardable powerset reference."""
    power_category: str = ""
    power_set: str = ""
    power: str = ""
    level: int = 0


@dataclass
class EntityLevel:
    """One entry in `levels[]` — per-level display name + costume.

    `level` is `ParseVillainLevelDef`'s `Level`, which every flavor carries.
    `max_level` is Homecoming's alone: it widened the field into a RANGE whose
    start is `Level`, so one HC element covers 1-55 where a fork writes 54
    elements. On a fork the record states no upper bound, so this is `None`
    rather than a copy of `level` — an absent field is not a defaulted one, and
    a fabricated bound would read as authored data.
    """
    level: int = 0
    max_level: int | None = None
    display_names: list[str] = field(default_factory=list)
    costumes: list[str] = field(default_factory=list)
    experience: int = 0


@dataclass
class EntityRecord:
    """A villaindef.bin entity record.

    Field names and order follow `ParseVillainDef[]` in the game source
    (`Common/gameComm/VillainDef.c:165`), which is what the serializer walks.
    `TOK_REDUNDANTNAME` entries there are parse-time synonyms for a member
    already listed, so they are written once: `SuccessRewards` shares
    `additional_rewards` with `AdditionalRewards`, and `StatusFailureRewards`
    shares `skill_status_rewards` with `IntegrityFailureRewards`. Counting them
    twice is what previously pushed the walk two slots out of alignment.
    """
    name: str = ""
    character_class_name: str = ""        # `Class` in the descriptor — raw "Class_Minion_Pets" form
    gender_raw: int = 0                   # `Gender` (u4 enum: 1=MALE, 2=NEUTER?, etc.)
    description: str = ""                 # `DisplayDescription` (P-hash or empty)
    group_description: str = ""           # `GroupDescription` (was previously misread as display_name)
    display_class_name: str = ""
    ai_config: str = ""
    villain_group_raw: int = 0            # `VillainGroup` (u4 enum)
    powers: list[EntityPower] = field(default_factory=list)
    levels: list[EntityLevel] = field(default_factory=list)
    rank_raw: int = 0                     # `Rank` (u4 `ParseVillainRankEnum`) — comes AFTER Level in the binary
    ally: str = ""
    gang: str = ""
    exclusion_raw: int = 0                # `Exclusion` (u4 `ParseVillainExclusion`)
    ignore_combat_mods: bool = False
    copy_creator_mods: bool = False
    ignore_reduction: bool = False
    can_zone: bool = False
    spawn_limit: int = -1
    spawn_limit_mission: int = -2         # -2 = "unspecified"; the game then defaults it to spawn_limit
    additional_rewards: list[str] = field(default_factory=list)
    favorite_weapon: str = ""
    skill_hp_rewards: list[str] = field(default_factory=list)
    skill_status_rewards: list[str] = field(default_factory=list)
    reward_scale: float = 1.0
    power_tags: list[str] = field(default_factory=list)
    special_pet_power: str = ""
    # Source path the def was compiled from (e.g.
    # "DEFS/VILLAINS/PETS.VILLAIN") — keep it for debugging/provenance.
    source_file: str = ""
    file_age: int = 0
    pet_command_strings: list[list[str]] = field(default_factory=list)
    pet_visibility: int = -1
    commandable_pet: int = 0              # `PetCommandability` — 0 or 1
    badge_stat: str = ""
    flags_raw: int = 0                    # `Flags` (u4 `ParseVillainDefFlags`)
    # Whatever the record carries past `Flags`. The parse table continues with
    # `ScriptDef`, which is `TOK_NULLSTRUCT` on the client — kept raw rather
    # than dropped so a future decode has the bytes to work from.
    tail_raw: bytes = b""

    # Backward-compat: the old API exposed `display_name`. Keep it as a
    # property pointing at the field we actually parse (group_description),
    # since that's what the previous code was reading at u4[4].
    @property
    def display_name(self) -> str:
        return self.group_description

    @display_name.setter
    def display_name(self, value: str) -> None:
        self.group_description = value


# ---------------------------------------------------------------------------
# Record walk — shared by every flavor; only the two element readers fork
# ---------------------------------------------------------------------------

def _parse_power_sub(r: BinReader) -> EntityPower:
    """Each powers sub-record is 6 u4s = 24 bytes:
    cat / set / power / level / 2 trailing flags (almost always 0).
    """
    cat = r.read_string()
    pset = r.read_string()
    power = r.read_string()
    level = r.read_u4()
    r.read_u4()  # unknown trailing 1
    r.read_u4()  # unknown trailing 2
    return EntityPower(power_category=cat, power_set=pset, power=power, level=level)


def _read_header(r: BinReader, rec: EntityRecord, power_sub) -> EntityRecord:
    """`ParseVillainDef[]` from `Name` down to `Power` — identical on every flavor.

    Shared with `detect_level_ints` rather than duplicated into it, so the probe
    cannot drift from the parse it is choosing a shape for.
    """
    rec.name = r.read_string()
    rec.character_class_name = r.read_string()
    rec.gender_raw = r.read_u4()
    rec.description = r.read_string()
    rec.group_description = r.read_string()
    rec.display_class_name = r.read_string()
    rec.ai_config = r.read_string()
    rec.villain_group_raw = r.read_u4()
    rec.powers = r.read_struct_array(power_sub)
    return rec


def _make_level_sub(level_ints: int):
    """A `levels[]` element reader for a schema carrying `level_ints` leading ints.

    `ParseVillainLevelDef` (`VillainDef.c:123`) is `Level`, `DisplayNames`,
    `Costumes`, `XP` — one leading int. Homecoming widened `Level` into a range
    and writes two. Nothing else about the element moved.

    The element is consumed exactly, and a residue raises: this is the read that
    a wrong `level_ints` gets wrong, so it has to be the read that says so. Both
    of the recoveries that used to live here — a tolerance on the display-names
    count, and an `except ValueError` yielding empty lists — turned that signal
    into plausible data, and between them they emptied `display_names` AND
    `costumes` on 5,139 Thunderspy elements while every gate stayed green.
    """
    def parse(r: BinReader) -> EntityLevel:
        ints = [r.read_u4() for _ in range(level_ints)]
        element = EntityLevel(
            level=ints[0],
            max_level=ints[1] if level_ints > 1 else None,
            display_names=r.read_string_array(),
            costumes=r.read_string_array(),
        )
        element.experience = r.read_u4()
        if r.remaining():
            raise ValueError(
                f"levels[] element has {r.remaining()} bytes left over under a "
                f"{level_ints}-int schema — the element shape is wrong"
            )
        return element
    return parse


# How many whole records to try each candidate shape against. The shapes
# disagree on the very first populated element, so this is about crossing any
# run of empty ones, not about accumulating confidence.
_LEVEL_SHAPE_PROBE_RECORDS = 64


def detect_level_ints(data, reader_factory, power_sub) -> int:
    """How many leading ints this file's `levels[]` element carries.

    Detected, never configured — and NOT inferrable from the container, which is
    the trap that hid this. Thunderspy ships a Parse7 container over the i24
    one-int element, so a container-keyed rule reads its `Level` as HC's
    `min_level` and its display-names COUNT as `max_level`. Measured over whole
    corpora, the shapes are mutually exclusive rather than merely better-fitting:

        homecoming  2-int fits 8875/8875, 1-int fits 52
        thunderspy  1-int fits 7309/7309, 2-int fits 0
        rebirth     1-int fits 7187/7187, 2-int fits 0

    So the discriminator is exact consumption of the element, and ambiguity is
    an error rather than a tie to break: if both shapes fit, or neither does,
    the file is not one we know how to read and saying so beats picking.
    """
    def probe(candidate: int | None) -> int:
        """Elements the probe window yields at `candidate`, or -1 if it does not fit.

        `candidate=None` only counts what the struct_array DECLARES, without reading
        an element — the one measurement that does not depend on the width, and so
        the only way to tell "this window has no levels in it" from "no width fits".
        """
        r = reader_factory(data)
        r.read_u4()
        count = r.read_u4()
        seen = 0
        for _ in range(min(count, _LEVEL_SHAPE_PROBE_RECORDS)):
            rec_len = r.read_u4()
            sub = r.sub_reader(rec_len)
            try:
                _read_header(sub, EntityRecord(), power_sub)
                if candidate is None:
                    seen += sub.peek_u4()
                else:
                    seen += len(sub.read_struct_array(_make_level_sub(candidate)))
            except ValueError:
                return seen if candidate is None else -1
            r.skip(rec_len)
        return seen

    if not probe(None):
        raise ValueError(
            f"villaindef levels[] element width is ungradeable: the first "
            f"{_LEVEL_SHAPE_PROBE_RECORDS} records declare no level element between them, so "
            f"every width fits vacuously. Widen the probe rather than picking one."
        )

    fits = [c for c in (1, 2) if probe(c) > 0]
    if len(fits) != 1:
        raise ValueError(
            f"villaindef levels[] element width is undecidable: {fits or 'no'} width(s) fit the "
            f"first {_LEVEL_SHAPE_PROBE_RECORDS} records, and exactly one must. Two fitting means "
            f"the probe cannot tell them apart here; none means this is a schema we cannot read."
        )
    return fits[0]


def _parse_entity(r: BinReader, power_sub, level_sub) -> EntityRecord:
    """Parse one VillainDef record, whole, down `ParseVillainDef[]`.

    One walk for every flavor: the container decides how a string and a powers
    element are encoded (`power_sub`) and the file's own schema decides the
    `levels` element (`level_sub`), but the FIELD ORDER is the parse table's and
    does not fork. Field order also matches the Parse7 descriptor at 0x1408fa9f0
    — see Ghidra's `bin_serializer_report.txt`.
    """
    rec = _read_header(r, EntityRecord(), power_sub)
    rec.levels = r.read_struct_array(level_sub)
    _read_tail_flags(r, rec)
    return rec


def _read_tail_flags(r: BinReader, rec: EntityRecord) -> None:
    """The block after `levels`, read straight down `ParseVillainDef[]`.

    An earlier version could not place this block and read most of it as
    anonymous "zero" slots, inferring `commandable_pet`/`can_zone` from the
    class name instead. It was two slots out because it counted the two
    `TOK_REDUNDANTNAME` reward aliases as fields of their own (see
    `EntityRecord`); with those collapsed the table lands exactly, which four
    independent checks confirm across all 8,875 HC records:

      - `copy_creator_mods` stays where the old walk already had it,
      - `pet_visibility` carries the parse table's declared `-1` default on
        727 of 744 pet records,
      - `source_file` resolves to real paths (`DEFS/VILLAINS/PETS.VILLAIN`),
      - every boolflag reads 0/1 and `flags` reads 0 on every record.

    All four reproduce on Rebirth's Parse6, which reaches this block only once
    its `levels` element is read at the right width: `pet_visibility` is -1 on
    7,165 of 7,187, every `source_file` is a `DEFS/…` path, `ally` is one of
    {Hero, Monster, Villain} or empty, `flags` is 0 throughout — and `tail_raw`
    lands on 4 bytes for every record, which a misaligned `levels` cannot do.

    `PetCommandability` is the authoritative `commandable_pet`: the server
    assigns it verbatim in `MapServer/src/entity/character_pet.c:569`
    (`pPet->commandablePet = pPet->villainDef->petCommadability`).
    """
    rec.rank_raw = r.read_u4()
    rec.ally = r.read_string()
    rec.gang = r.read_string()
    rec.exclusion_raw = r.read_u4()
    rec.ignore_combat_mods = bool(r.read_u4() & 1)
    rec.copy_creator_mods = bool(r.read_u4() & 1)
    rec.ignore_reduction = bool(r.read_u4() & 1)
    rec.can_zone = bool(r.read_u4() & 1)
    rec.spawn_limit = r.read_s4()
    rec.spawn_limit_mission = r.read_s4()
    rec.additional_rewards = r.read_string_array()
    rec.favorite_weapon = r.read_string()
    rec.skill_hp_rewards = r.read_string_array()
    rec.skill_status_rewards = r.read_string_array()
    rec.reward_scale = r.read_f4()
    rec.power_tags = r.read_string_array()
    rec.special_pet_power = r.read_string()
    rec.source_file = r.read_string()
    rec.file_age = r.read_u4()
    rec.pet_command_strings = r.read_struct_array(_parse_pet_command_strings_sub)
    rec.pet_visibility = r.read_s4()
    rec.commandable_pet = r.read_s4()
    rec.badge_stat = r.read_string()
    rec.flags_raw = r.read_u4()
    rec.tail_raw = r.read_raw(r.remaining())


def _parse_pet_command_strings_sub(r: BinReader) -> list[str]:
    """One `PetCommandStrings` record — the eleven parallel response lists of
    `ParsePetCommandStrings[]` (Passive, Defensive, Aggressive, AttackTarget,
    AttackNoTarget, StayHere, UsePower, UsePowerNone, FollowMe, GotoSpot,
    Dismiss), flattened because nothing downstream distinguishes the commands.

    Written to the table rather than to a permissive "read until exhausted"
    loop, so a wrong shape raises out of the bounded sub-reader instead of
    silently returning a short list that looks like data. That bet paid: the
    shape rested on the parse table alone while Homecoming was the only corpus
    (69 of 8,875 records populate it), and it reads clean on the two forks that
    populate it too — 61 Rebirth records, 73 Thunderspy.
    """
    return [s for _ in range(11) for s in r.read_string_array()]


def parse_entities(bin_path_or_data) -> list[EntityRecord]:
    """Parse villaindef.bin into EntityRecord list.

    Accepts a file path (str/Path) or raw bytes.

    The container picks the string and powers-element encoding; the file's own
    `levels` element width is detected separately, because the two axes are
    independent — Thunderspy ships a Parse7 container over the i24 one-int
    element.
    """
    if isinstance(bin_path_or_data, (bytes, memoryview)):
        data = bin_path_or_data
    else:
        from pathlib import Path
        data = Path(bin_path_or_data).read_bytes()

    r = open_parse7(data)
    parse6 = isinstance(r, Parse6BinReader)
    power_sub = _parse_power_sub_p6 if parse6 else _parse_power_sub
    level_sub = _make_level_sub(detect_level_ints(data, open_parse7, power_sub))

    r.read_u4()  # block_size
    count = r.read_u4()

    records = []
    for _ in range(count):
        rec_len = r.read_u4()
        sub = r.sub_reader(rec_len)
        records.append(_parse_entity(sub, power_sub, level_sub))
        r.skip(rec_len)
    return records


def _parse_power_sub_p6(r: Parse6BinReader) -> EntityPower:
    """Parse6's powers element: the Parse7 fields, minus its two trailing u4s.

    The element is length-prefixed, so the tail is drained rather than assumed —
    Parse6 pads inline strings to 4-byte alignment, which makes the residue a
    property of the strings in this element, not a constant.
    """
    cat = r.read_string()
    pset = r.read_string()
    power = r.read_string()
    level = r.read_u4()
    while r.remaining() >= 4:
        r.read_u4()
    return EntityPower(power_category=cat, power_set=pset, power=power, level=level)


