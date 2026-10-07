"""Parser for classes.bin and villain_classes.bin (archetype definitions).

Every record is read SEQUENTIALLY, field for field, against the serialized
`ParseCharacterClass` layout — no anchor scans, no fixed byte-deltas. The walk
is byte-accounted: a record that does not consume exactly its declared length
raises, so a schema change in any dataset fails loudly instead of shipping a
misread (the WS8 mandate; the old fixed-delta layout drifted silently on
Thunderspy and exported a wrong-row HP cap for Rebirth Kheldians).

Layout oracle: the released i24 server source (`Common/entity/classes.h`,
`odasm/coh-server-original`) gives the base field order. All three datasets
extend it the same way — the archetype-select stat bars and screenshot names
that i24 hardcoded in the client (`uiArchetype.c` `aStats`/`sImages`, byte-
identical values) plus a class-mechanic tip string — but splice the additions
in differently, and Homecoming also reshapes the i24 prefix:

Prefix after DisplayHelp:
  i24 / Thunderspy / Rebirth:
    AllowedOrigins[] SpecialRestrictions[] StoreRequires LockedTooltip
    ProductCode ReductionClass ReduceAsArchvillain(u4) LevelUpRespecs(u4[])
  Homecoming (freemium fields deleted, one field added):
    VillainRank(u4)  — VR_NONE(0) for players, VR_SMALL(1)..VR_DESTRUCTIBLE(11)
                       for NPC classes (i24 `VillainDef.h` enum, exact match)
    SpecialRestrictions[] <always-0 word> ReductionClass
    ReduceAsArchvillain(u4) LevelUpRespecs(u4[])

Then (all datasets): DisplayShortHelp, Icon.
  Homecoming inserts here: ArchetypeShots[] (string array), then the four
  categories, then MechanicTip (string, e.g. "RageTip"), CreationStats
  (counted int array, the six AT-select bars; empty for NPCs), and a
  playstyle-filter bitmask word. Thunderspy/Rebirth: just the four categories
  (their copies of these additions live in the record tail instead).

Attrib block (all datasets, fully self-describing):
  10 struct-arrays of CharacterAttributes (AttribMin, AttribBase, StrengthMin,
  ResistanceMin, DiminStr/Cur/Res × In/Out) — each element a flat float run
  (HC 117 floats = 468 B, Thunderspy/Rebirth 116 = 464 B, matching each
  dataset's sizeof(CharacterAttributes) from the special-attrib bases);
  4 struct-arrays of CharacterAttributesTable (AttribMaxTable, AttribMaxMax-
  Table, StrengthMaxTable, ResistanceMaxTable) — each element a run of
  count-prefixed f32 arrays, one per attrib in table order (`ParseCharacter-
  AttributesTable`: DamageType00-19, HitPoints, ... — NB the table order
  differs from the struct order: no Absorb slot after HitPoints, Defense
  serialized twice, Absorb last);
  ModTable (the named per-level modifier tables).

Tail after ModTable:
  Homecoming: ConnectHPAndStatus(u4) DefiantScale(f4) + one HC-added float
    (varies per class, meaning unknown — captured raw).
  Thunderspy/Rebirth: ConnectHPAndStatus(u4), DefiantHitPointsAttrib raw
    (1 word; 2 on Thunderspy — its 64-bit binner serializes the size_t as 8
    bytes), DefiantScale(f4), then the fork copy of the shared additions:
    mechanic-bar id (1=PrimalEnergy 2=Rage 3=Domination), MechanicTip,
    one unidentified word (non-zero only on locked epic ATs — captured raw),
    six CreationStats ints (−1 = the "?" bars on Kheldians), three
    ArchetypeShots strings, the playstyle bitmask, and five words observed
    always zero (the four `_Final*_` shared-memory pointer blobs of the i24
    tail plus one more) — captured raw when non-zero.

Verified 2026-07-21: all 191 records across the six bins (3 datasets ×
classes/villain_classes) walk byte-clean, and the exported attribs/named
tables reproduce the committed game-verified values for every player
archetype on every dataset. Known deliberate diffs vs the old heuristics are
documented in DATA-GAP-REGISTER.md (CLASSES-1/WS8): the old cap-delta read
the AttribMaxMax DamageType00 row instead of HitPoints (wrong for Rebirth
Kheldians), and every old NPC-class attrib/header misread is replaced by the
structural values.
"""

from ._reader import open_parse7, BinReader, Parse6BinReader
from ._dataclasses import ClassRecord
from ._enums import ATTRIB_NAME, ATTRIB_NAME_REBIRTH, ATTRIB_NAME_THUNDERSPY

# Attrib indices used by the export. Table order (ParseCharacterAttributes-
# Table) puts HitPoints right after the 20 damage types on every dataset;
# the scalar struct order (ParseCharacterAttributes) has ThreatLevel at 61 on
# every dataset (the fork layout divergences — Rebirth's Accuracy band,
# Thunderspy's recharge band, HC's extra slot — all sit past the indices used
# here). Verified value-for-value against the committed game-verified curves
# for all 45 player archetypes.
_IDX_TABLE_HITPOINTS = 20
_IDX_TABLE_DAMAGE = 0
_IDX_SCALAR_THREAT = 61
# Absorb is serialized LAST in the i24 table order, one slot past ElusivityBase
# (`ParseCharacterAttributesTable`), which puts it at 115 on the forks (116-row
# tables) — and also at 115 on Homecoming, whose 117-row table inserts one
# attrib before Absorb and appends one after it. Landing on the same index in
# all three datasets is a property, not an assumption: `_absorb_cap` re-proves
# it per record against the signature below before reading a value.
_IDX_TABLE_ABSORB = 115
_PLAYER_LEVELS = 50             # planner uses levels 1-50


def _read_attrib_struct_array(r) -> list[list[float]]:
    """Struct-array of CharacterAttributes: u4 count, per element u4 byte size
    + a flat float run. The element size is the dataset's
    sizeof(CharacterAttributes); trusting it keeps the read layout-agnostic."""
    count = r.read_u4()
    out = []
    for _ in range(count):
        elem_size = r.read_u4()
        if elem_size % 4 != 0:
            raise ValueError(f"CharacterAttributes element size {elem_size} "
                             f"is not float-aligned")
        out.append([r.read_f4() for _ in range(elem_size // 4)])
    return out


def _read_attrib_table_array(r) -> list[list[list[float]]]:
    """Struct-array of CharacterAttributesTable: per element, a run of
    count-prefixed f32 arrays (one per attrib, table order) filling exactly
    the element's declared byte length."""
    count = r.read_u4()
    out = []
    for _ in range(count):
        elem_size = r.read_u4()
        end = r.pos + elem_size
        arrays = []
        while r.pos < end:
            value_count = r.read_u4()
            if r.pos + value_count * 4 > end:
                raise ValueError(
                    f"CharacterAttributesTable f32 array (count {value_count}) "
                    f"overruns its element")
            arrays.append([r.read_f4() for _ in range(value_count)])
        out.append(arrays)
    return out


def _read_named_tables(r) -> dict[str, list[float]]:
    """ModTable: struct-array of NamedTable (name + per-level float values)."""
    count = r.read_u4()
    tables = {}
    for _ in range(count):
        elem_size = r.read_u4()
        end = r.pos + elem_size
        table_name = r.read_string()
        value_count = r.read_u4()
        tables[table_name] = [r.read_f4() for _ in range(value_count)]
        if r.pos != end:
            raise ValueError(f"named table {table_name!r} consumed "
                             f"{r.pos - (end - elem_size)} of {elem_size} bytes")
    return tables


def _absorb_cap(name: str, attrib_max, attrib_max_max) -> list[float] | None:
    """The per-level Absorb ceiling — AttribMaxMaxTable's Absorb row.

    This is the value `ClampMax` clamps `attrMax.fAbsorb` against
    (`Common/entity/character_attribs.c`: `CLAMP_MAX(fAbsorb)` against
    `pclass->pattrMaxMax[iCombatLevel]`), so an absorb shield can never exceed
    it. It runs close to the class's base max HP but is authored separately and
    genuinely diverges — Homecoming's Brute caps 20 points under its base HP at
    level 1, and every dataset caps the Dominator on the 100→1070.9 curve while
    its HP runs 99→1017.4. Deriving it from the HP table would ship those as
    wrong numbers, so it is read as its own row.

    Absorb's identity at [`_IDX_TABLE_ABSORB`] is re-proven per record rather
    than trusted: AttribMaxTable's row must be all zero (a character starts with
    no absorb) while AttribMaxMaxTable's carries a curve. A slide onto either
    neighbour breaks that — HitPoints and the Homecoming-appended tail attrib
    both start non-zero, ElusivityBase caps at zero.
    """
    if len(attrib_max_max[0]) <= _IDX_TABLE_ABSORB:
        return None
    cap = attrib_max_max[0][_IDX_TABLE_ABSORB]
    base = attrib_max[0][_IDX_TABLE_ABSORB] if len(attrib_max[0]) > _IDX_TABLE_ABSORB else []
    if len(cap) < _PLAYER_LEVELS:
        raise ValueError(f"{name}: Absorb cap row has {len(cap)} levels, "
                         f"expected at least {_PLAYER_LEVELS} — table drift")
    if any(v != 0.0 for v in base) or not any(v != 0.0 for v in cap):
        raise ValueError(f"{name}: attrib table index {_IDX_TABLE_ABSORB} does "
                         f"not carry Absorb (base row must be all zero over a "
                         f"non-zero cap row) — table drift")
    return cap[:_PLAYER_LEVELS]


# The reduction aspects whose net strength `ClampStrength` bounds, keyed by the
# export prefix each pair is written under. Attribute names, not game proper
# nouns: these are the CharacterAttributes field labels the maps in `_enums`
# carry, so a rename upstream raises in `_attrib_struct_index` rather than
# silently reading a neighbouring slot.
_CLAMPED_STRENGTHS = {
    "recharge": "RechargeTime",
    "endurance": "EnduranceDiscount",
}

# The four travel axes the planner tracks, keyed by the export name each is
# written under. Addressed by attribute name for the same reason as
# `_CLAMPED_STRENGTHS`; SpeedSwimming sits between FlyingSpeed and
# JumpingSpeed and is deliberately absent (no swim mechanic to plan around).
_MOVEMENT_ATTRIBS = {
    "run_speed": "RunningSpeed",
    "fly_speed": "FlyingSpeed",
    "jump_speed": "JumpingSpeed",
    "jump_height": "JumpHeight",
}

# The attributes whose ceiling is a plain AttribMaxTable row over an AttribBase
# scalar, keyed by the export prefix each pair is written under
# (`<prefix>_base` / `<prefix>_cap`). Both halves are needed: the ceiling is an
# ABSOLUTE attribute value, and the per-archetype percentage a planner shows is
# only recoverable against that class's own base (Arachnos classes author a
# higher regeneration base under the same 5.0 ceiling than every other class).
# Addressed by attribute name for the same reason as `_CLAMPED_STRENGTHS`.
# DATA-GAP-REGISTER CAPS-1.
_CEILED_ATTRIBS = {
    "to_hit": "ToHit",
    "regeneration": "Regeneration",
    "recovery": "Recovery",
}

# The per-type defense slots, in CharacterAttributes order. Every non-empty row
# carries the same curve as every other on the same class (measured over all 45
# player archetypes of the three datasets), so the export writes ONE defense
# ceiling — but [`_defense_cap`] proves that agreement per record rather than
# reading one row and assuming. `Base_Defense` (the aggregate `fDefense` slot)
# is deliberately out: Homecoming authors it as all zeros where both forks carry
# the same curve as the typed rows, so including it would read as a zero ceiling
# on one dataset.
_DEFENSE_ATTRIBS = ["Ranged", "Melee", "Area", "Smashing", "Lethal", "Fire",
                    "Cold", "Energy", "Negative_Energy", "Psionic", "Toxic"]

# The attributes whose BASE is itself a per-level AttribMaxTable row, with the
# ceiling one table further out in AttribMaxMaxTable — HitPoints' shape, the one
# `ClampMax` bounds rather than `ClampCur`. Keyed by the export name the base is
# written under; the ceiling goes to `<name>_cap`. HitPoints keeps its own index
# constant above because that index is what the table order is anchored by;
# Endurance is addressed by attribute name like everything else here.
_MAX_STYLE_ATTRIBS = {"max_endurance": "Endurance"}


def _attrib_struct_index(flavor: str, attrib: str) -> int:
    """`attrib`'s index in the CharacterAttributes STRUCT order, per dataset —
    single-sourced from the measured attrib maps in `_enums` (RechargeTime at
    HC/Rebirth 90, Thunderspy 89: its struct is one field short from 89 up;
    EnduranceDiscount sits two further along on every dataset). A map carrying
    zero or several entries for the name is layout drift."""
    table = {"hc": ATTRIB_NAME,
             "rebirth": ATTRIB_NAME_REBIRTH,
             "thunderspy": ATTRIB_NAME_THUNDERSPY}[flavor]
    matches = [i for i, n in table.items() if n == attrib]
    if len(matches) != 1:
        raise ValueError(f"{flavor}: attrib map names {attrib} at "
                         f"{matches} — expected exactly one index")
    return matches[0]


def _table_index(struct_index: int) -> int:
    """The CharacterAttributesTable index of a struct-order attrib.

    `ParseCharacterAttributesTable` has no Absorb slot after HitPoints (it
    serializes Absorb last instead), so every attrib past struct 21 sits one
    slot earlier in table order. Anchored per dataset by Endurance's flat-100
    row landing at table 21 and Absorb's at table 115."""
    return struct_index - 1


def _movement_scales(name, attrib_min, attrib_base, attrib_max,
                     movement_struct_indices):
    """Per-axis `(floor scalar, base scalar, per-level cap row)` in movement
    SCALE units.

    The scale is the multiplier the server hands the physics layer:
    `setSpeed(ent, surf, attrCur.fSpeedRunning)` scales
    `BASE_PLAYER_FORWARDS_SPEED` (0.7 ft/tick × 30 ticks/s = 21 ft/s), and
    `setJumpHeight` takes `attrCur.fJumpHeight` the same way
    (`Common/entity/character_tick.c`, `MapServer/src/entity/entGameActions.c`).
    Base is AttribBase's scalar — 1.0 run/jump/height and 1.5 fly on every
    class of all three datasets — and the two bounds are the pair `ClampCur`
    holds `attrCur` between: AttribMin's scalar below, the AttribMaxTable row
    above. The ceiling is per-level on the forks (Thunderspy scales 4.5 → 6.46
    run over levels 1-50; Rebirth authors its own 4× band) and flat on
    Homecoming; the floor is a scalar on every dataset.

    The floor is not zero and could not have been guessed as zero: every player
    class of all three datasets floors run and fly at 0.1 — a grounded character
    still crawls at a tenth of base — while jump speed and height floor at 0.0.
    NPC classes author their own (a 1.95 run floor, a 50.0 jump height), which
    is why it is read per class rather than stated once.

    All three are exported raw. The unit projection (mph, feet) belongs to
    whoever displays them, and lives in the client rather than in any bin.

    Unlike [`_absorb_cap`] there is no value signature re-proving the rows'
    identity, because there is no value a movement row cannot hold: NPC classes
    author a 0.0 run base (Class_Minion_Turret cannot move) and a jump-height
    ceiling BELOW the default scale (Class_Minion_Grunt caps at 0.799 over a
    1.0 base). Absorb needs its own proof because table 115 is a derived
    constant; these indices come from the per-dataset attrib NAME maps via
    [`_attrib_struct_index`], which raises on a rename, so the anchor is the
    name and any added value rule would only reject real data."""
    out_floor, out_base, out_cap = {}, {}, {}
    for axis, struct_index in movement_struct_indices.items():
        table_index = _table_index(struct_index)
        if (len(attrib_min[0]) <= struct_index
                or len(attrib_base[0]) <= struct_index
                or len(attrib_max[0]) <= table_index):
            raise ValueError(f"{name}: movement axis {axis} sits past the end "
                             f"of the attrib struct/table — layout drift")
        cap = attrib_max[0][table_index]
        if len(cap) < _PLAYER_LEVELS:
            raise ValueError(f"{name}: movement axis {axis} cap row has "
                             f"{len(cap)} levels, expected at least "
                             f"{_PLAYER_LEVELS} — table drift")
        out_floor[axis] = attrib_min[0][struct_index]
        out_base[axis] = attrib_base[0][struct_index]
        out_cap[axis] = cap[:_PLAYER_LEVELS]
    return out_floor, out_base, out_cap


def _ceiling_row(name, table, attrib, table_index):
    """One attribute's per-level row out of a CharacterAttributesTable.

    For `_CEILED_ATTRIBS` the caller passes AttribMaxTable — the ceiling
    `ClampCur` bounds `attrCur` against, the same table [`_movement_scales`]
    reads its axis ceilings from. AttribMaxMaxTable is deliberately NOT read for
    those: it bounds `attrMax`, which is what a Max-aspect mod raises, and the
    export models that the way movement already does — carry the AttribMaxTable
    row and let the calc add whatever the build's own powers raised it by.
    (Measured 2026-08-01: the two tables are identical row-for-row on all 45
    player archetypes for every attribute in `_CEILED_ATTRIBS` and
    `_DEFENSE_ATTRIBS`, so nothing raises those ceilings today — but the shape
    does not depend on that staying true.) For `_MAX_STYLE_ATTRIBS` both tables
    are read, because there the two genuinely differ.
    """
    if len(table[0]) <= table_index:
        raise ValueError(f"{name}: {attrib} sits past the end of the attrib "
                         f"table — layout drift")
    row = table[0][table_index]
    if len(row) < _PLAYER_LEVELS:
        raise ValueError(f"{name}: {attrib} ceiling row has {len(row)} levels, "
                         f"expected at least {_PLAYER_LEVELS} — table drift")
    return row[:_PLAYER_LEVELS]


def _authored_defense_attribs(attrib_max, defense_struct_indices) -> dict:
    """The typed-defense slots this dataset actually authors, as
    `{attrib: struct_index}`.

    Authorship is asked ONCE, of AttribMaxTable, and both defense bounds read
    the answer — because the two tables disagree about how an unauthored slot
    LOOKS. AttribMaxTable leaves it an empty row, which is unmistakable;
    AttribMin leaves it a bare `0.0`, which is a perfectly legal floor (
    Class_Boss_RularuuCoP floors every defense slot at 0.0 on all three
    datasets) and so cannot be told from real data on its own. Both forks stop
    authoring at Psionic and leave Toxic that way, where Homecoming authors all
    eleven; reading the fork's Toxic `0.0` as a floor would ship "toxic defense
    can never go negative," a claim the dataset never makes.
    """
    return {attrib: struct_index
            for attrib, struct_index in defense_struct_indices.items()
            if len(attrib_max[0]) > _table_index(struct_index)
            and len(attrib_max[0][_table_index(struct_index)]) >= _PLAYER_LEVELS}


def _defense_cap(name, attrib_max, authored) -> list[float] | None:
    """The per-level defense ceiling — one curve, proven against every typed
    defense row the dataset authors.

    The game clamps defense (`CLAMP_CUR(fDefenseType[0..19])`), just at a value
    three to five times the purple-patch softcap, which is a threshold rather
    than a clamp and stays exposed separately. So this ceiling never contradicts
    the softcap; it sits far above it.

    Returns None when the dataset authors no populated typed-defense row at all,
    and raises when two populated rows disagree — that disagreement would mean
    defense has become a per-type ceiling and one number can no longer carry it.
    An unauthored slot is not a disagreement (see
    [`_authored_defense_attribs`]).
    """
    agreed, agreed_by = None, None
    for attrib, struct_index in authored.items():
        row = attrib_max[0][_table_index(struct_index)][:_PLAYER_LEVELS]
        if agreed is None:
            agreed, agreed_by = row, attrib
        elif row != agreed:
            raise ValueError(
                f"{name}: defense ceiling row {attrib} disagrees with "
                f"{agreed_by} — defense is now a per-type ceiling and the "
                f"export must carry one row per type")
    return agreed


def _defense_floor(name, attrib_min, authored) -> float | None:
    """The defense floor — AttribMin's scalar, the other half of the pair
    `ClampCur` holds `attrCur.fDefenseType[]` between.

    −1.0 on every player archetype of all three datasets, and the reason it has
    to be read is that the game writes "your defense is negated" as a saturating
    magnitude rather than as a switch, exactly as it writes grounding
    (MOVEMIN-1). Thunderspy's Organic Armor states `Defense −500 ×
    Melee_Buff_Def` on all seven typed slots while Defensive Adaptation is up —
    −50.0 on a Tanker at level 50, fifty times past this floor — and the power's
    own description says why: it "negates your defense, instead applying a
    constant 1 percent absorb shield." Without the floor there is no number that
    debuff resolves to. DATA-GAP-REGISTER ATTRMIN-1.

    Read at the slots [`_authored_defense_attribs`] names, for the reason given
    there, and proven the same way as the ceiling: one number for the family, or
    a raise. Returns None when the dataset authors no typed-defense slot at all.
    """
    agreed, agreed_by = None, None
    for attrib, struct_index in authored.items():
        if len(attrib_min[0]) <= struct_index:
            raise ValueError(f"{name}: defense slot {attrib} sits past the end "
                             f"of the AttribMin struct — layout drift")
        value = attrib_min[0][struct_index]
        if agreed is None:
            agreed, agreed_by = value, attrib
        elif value != agreed:
            raise ValueError(
                f"{name}: defense floor {attrib}={value} disagrees with "
                f"{agreed_by}={agreed} — defense is now a per-type floor and "
                f"the export must carry one per type")
    return agreed


def _extract_attribs(name, attrib_min, attrib_base, attrib_max, attrib_max_max,
                     strength_max, resistance_max, strength_min,
                     struct_indices) -> dict:
    """The planner's attrib values, addressed structurally.

    `struct_indices` bundles the per-dataset CharacterAttributes STRUCT indices
    of every attribute addressed by NAME here, grouped by the table that names
    them: `clamped`, `movement`, `ceiled`, `max_style`, `defense`. They are
    resolved by the caller through [`_attrib_struct_index`], which raises on a
    rename, so this function stays free of dataset knowledge.

    hit_points / hp_cap: the HitPoints per-level rows of AttribMaxTable /
    AttribMaxMaxTable. NB hp_cap must come from the HitPoints row, not the
    (usually identical) damage-type rows — Rebirth raised only the real
    HitPoints row for Kheldians, which the old fixed-delta read missed.
    absorb_cap: AttribMaxMaxTable Absorb row (see [`_absorb_cap`]).
    resistance_cap: ResistanceMaxTable DamageType00 (flat per-level run).
    base_threat: AttribBase ThreatLevel scalar.
    damage_cap: StrengthMaxTable DamageType00 at level 50.
    <aspect>_floor / <aspect>_cap for each of `_CLAMPED_STRENGTHS`: the net-
    strength clamp bounds (`ClampStrength`, `Common/entity/character_attribs.c`:
    net strength clamps between StrengthMin — struct order — and
    StrengthMaxTable[level] — table order, see [`_table_index`]).
    movement_floor / movement_base / movement_cap for each of
    `_MOVEMENT_ATTRIBS`: the travel scales and the pair of bounds `ClampCur`
    holds them between (see [`_movement_scales`]).
    <prefix>_base / <prefix>_cap for each of `_CEILED_ATTRIBS`: the AttribBase
    scalar and the per-level AttribMaxTable ceiling `ClampCur` bounds it against
    (see [`_ceiling_row`]). DATA-GAP-REGISTER CAPS-1.
    <name> / <name>_cap for each of `_MAX_STYLE_ATTRIBS`: HitPoints' shape —
    the AttribMaxTable row and the AttribMaxMaxTable ceiling over it.
    defense_cap / defense_floor: the pair `ClampCur` holds typed defense
    between — one per-level ceiling curve and one scalar floor, each proven
    against every typed defense slot the dataset authors (see [`_defense_cap`],
    [`_defense_floor`], [`_authored_defense_attribs`]).

    recharge: 0.25 / 5.0 on every player class of all three datasets — the −75%
    recharge-debuff floor and the +400% recharge cap.
    endurance: 0.0001 / 5.0 likewise. The floor is the same epsilon the server
    adds to the divisor at every consumption site (`fEnduranceCost /
    (fEnduranceDiscount + 0.0001f)`, character_tick.c / character_combat_eval.c
    / attrib_description.c), i.e. a divide guard rather than a real floor:
    unlike recharge, an endurance PENALTY is not bounded, so a debuffed power
    can cost arbitrarily more.
    """
    out: dict[str, object] = {}
    if attrib_max and len(attrib_max[0]) > _IDX_TABLE_HITPOINTS:
        hp = attrib_max[0][_IDX_TABLE_HITPOINTS]
        if len(hp) >= _PLAYER_LEVELS:
            out["hit_points"] = hp[:_PLAYER_LEVELS]
    if attrib_max_max and len(attrib_max_max[0]) > _IDX_TABLE_HITPOINTS:
        cap = attrib_max_max[0][_IDX_TABLE_HITPOINTS]
        if len(cap) >= _PLAYER_LEVELS:
            out["hp_cap"] = cap[:_PLAYER_LEVELS]
    if attrib_max and attrib_max_max:
        absorb_cap = _absorb_cap(name, attrib_max, attrib_max_max)
        if absorb_cap is not None:
            out["absorb_cap"] = absorb_cap
    if resistance_max and resistance_max[0] and resistance_max[0][_IDX_TABLE_DAMAGE]:
        out["resistance_cap"] = resistance_max[0][_IDX_TABLE_DAMAGE][0]
    if attrib_base and len(attrib_base[0]) > _IDX_SCALAR_THREAT:
        threat = attrib_base[0][_IDX_SCALAR_THREAT]
        if threat != 0.0:
            out["base_threat"] = threat
    if strength_max and strength_max[0]:
        dmg = strength_max[0][_IDX_TABLE_DAMAGE]
        if len(dmg) >= _PLAYER_LEVELS:
            out["damage_cap"] = dmg[_PLAYER_LEVELS - 1]
    for aspect, struct_index in struct_indices["clamped"].items():
        if strength_min and len(strength_min[0]) > struct_index:
            out[f"{aspect}_floor"] = strength_min[0][struct_index]
        table_index = _table_index(struct_index)
        if strength_max and strength_max[0] and len(strength_max[0]) > table_index:
            row = strength_max[0][table_index]
            if len(row) >= _PLAYER_LEVELS:
                out[f"{aspect}_cap"] = row[_PLAYER_LEVELS - 1]
    if attrib_min and attrib_base and attrib_max:
        floor, base, cap = _movement_scales(name, attrib_min, attrib_base,
                                            attrib_max,
                                            struct_indices["movement"])
        out["movement_floor"] = floor
        out["movement_base"] = base
        out["movement_cap"] = cap
    for prefix, struct_index in struct_indices["ceiled"].items():
        if attrib_base and len(attrib_base[0]) > struct_index:
            out[f"{prefix}_base"] = attrib_base[0][struct_index]
        if attrib_max:
            out[f"{prefix}_cap"] = _ceiling_row(name, attrib_max, prefix,
                                                _table_index(struct_index))
    for export_name, struct_index in struct_indices["max_style"].items():
        table_index = _table_index(struct_index)
        if attrib_max:
            out[export_name] = _ceiling_row(name, attrib_max, export_name,
                                            table_index)
        if attrib_max_max:
            out[f"{export_name}_cap"] = _ceiling_row(name, attrib_max_max,
                                                     export_name, table_index)
    if attrib_max:
        authored = _authored_defense_attribs(attrib_max,
                                             struct_indices["defense"])
        defense_cap = _defense_cap(name, attrib_max, authored)
        if defense_cap is not None:
            out["defense_cap"] = defense_cap
        if attrib_min:
            defense_floor = _defense_floor(name, attrib_min, authored)
            if defense_floor is not None:
                out["defense_floor"] = defense_floor
    return out


def _to_signed(word: int) -> int:
    return word - 0x100000000 if word >= 0x80000000 else word


def _read_class_record(r, flavor: str) -> ClassRecord:
    rec: dict = {}
    rec["name"] = r.read_string()
    rec["display_name"] = r.read_string()
    rec["display_help"] = r.read_string()

    if flavor == "hc":
        rec["villain_rank"] = r.read_u4()
        rec["special_restrictions"] = r.read_string_array()
        # One always-0 word sits between the restrictions and ReductionClass —
        # plausibly the emptied AllowedOrigins slot (HC deleted origin
        # restrictions along with the freemium Store/Locked/ProductCode
        # fields). Captured, surfaced if it ever goes non-zero.
        hc_prefix_word = r.read_u4()
        if hc_prefix_word != 0:
            rec.setdefault("extra_raw", []).append(("prefix", hc_prefix_word))
        rec["reduction_class"] = r.read_string()
    else:
        rec["allowed_origins"] = r.read_string_array()
        rec["special_restrictions"] = r.read_string_array()
        rec["store_requires"] = r.read_string()
        rec["locked_tooltip"] = r.read_string()
        rec["product_code"] = r.read_string()
        rec["reduction_class"] = r.read_string()

    rec["reduce_as_archvillain"] = bool(r.read_u4())
    rec["level_up_respecs"] = r.read_u4_array()

    rec["display_short_help"] = r.read_string()
    rec["icon"] = r.read_string()
    if flavor == "hc":
        rec["archetype_shots"] = r.read_string_array()
    rec["primary_category"] = r.read_string()
    rec["secondary_category"] = r.read_string()
    rec["pool_category"] = r.read_string()
    rec["epic_pool_category"] = r.read_string()
    if flavor == "hc":
        rec["mechanic_tip"] = r.read_string()
        rec["creation_stats"] = [_to_signed(w) for w in r.read_u4_array()]
        rec["playstyle_flags"] = r.read_u4()

    # AttribMin, AttribBase, StrengthMin, ResistanceMin, then the six
    # diminishing-returns tables; AttribMin, AttribBase and StrengthMin feed
    # the export.
    attrib_structs = [_read_attrib_struct_array(r) for _ in range(10)]
    attrib_min = attrib_structs[0]
    attrib_base = attrib_structs[1]
    strength_min = attrib_structs[2]
    attrib_max = _read_attrib_table_array(r)
    attrib_max_max = _read_attrib_table_array(r)
    strength_max = _read_attrib_table_array(r)
    resistance_max = _read_attrib_table_array(r)

    rec["named_tables"] = _read_named_tables(r)
    struct_indices = {
        "clamped": {aspect: _attrib_struct_index(flavor, attrib)
                    for aspect, attrib in _CLAMPED_STRENGTHS.items()},
        "movement": {axis: _attrib_struct_index(flavor, attrib)
                     for axis, attrib in _MOVEMENT_ATTRIBS.items()},
        "ceiled": {prefix: _attrib_struct_index(flavor, attrib)
                   for prefix, attrib in _CEILED_ATTRIBS.items()},
        "max_style": {export_name: _attrib_struct_index(flavor, attrib)
                      for export_name, attrib in _MAX_STYLE_ATTRIBS.items()},
        "defense": {attrib: _attrib_struct_index(flavor, attrib)
                    for attrib in _DEFENSE_ATTRIBS},
    }
    rec["attribs"] = _extract_attribs(rec["name"], attrib_min, attrib_base,
                                      attrib_max, attrib_max_max, strength_max,
                                      resistance_max, strength_min,
                                      struct_indices)

    rec["connect_hp_and_status"] = bool(r.read_u4())
    if flavor == "hc":
        rec["defiant_scale"] = r.read_f4()
        rec["tail_scalar_raw"] = r.read_f4()
    else:
        defiant_words = 2 if flavor == "thunderspy" else 1
        for _ in range(defiant_words):
            word = r.read_u4()
            if word != 0:
                rec.setdefault("extra_raw", []).append(("defiant", word))
        rec["defiant_scale"] = r.read_f4()
        rec["mechanic_bar_raw"] = r.read_u4()
        rec["mechanic_tip"] = r.read_string()
        rec["mechanic_gap_raw"] = r.read_u4()
        rec["creation_stats"] = [_to_signed(r.read_u4()) for _ in range(6)]
        rec["archetype_shots"] = [r.read_string() for _ in range(3)]
        rec["playstyle_flags"] = r.read_u4()
        for _ in range(5):
            word = r.read_u4()
            if word != 0:
                rec.setdefault("extra_raw", []).append(("tail", word))

    if r.remaining() != 0:
        raise ValueError(f"{rec['name']}: record walk left {r.remaining()} "
                         f"unread bytes — layout drift")
    return ClassRecord(**rec)


def _detect_parse7_flavor(r: BinReader) -> str:
    """Homecoming and Thunderspy are both Parse7 but serialize different
    prefixes. Decide once per file by trial-walking the first record under
    each schema — the byte-accounted walk succeeds for exactly one."""
    save_pos = r.pos
    verdicts = []
    for flavor in ("hc", "thunderspy"):
        r._pos = save_pos
        rec_len = r.read_u4()
        sub = r.sub_reader(rec_len)
        try:
            _read_class_record(sub, flavor)
            verdicts.append(flavor)
        except (ValueError, IndexError):
            pass
    r._pos = save_pos
    if len(verdicts) != 1:
        raise ValueError(
            f"Parse7 classes flavor detection failed: first record walks "
            f"clean under {verdicts or 'neither schema'} — layout drift")
    return verdicts[0]


def is_player_class(rec: ClassRecord) -> bool:
    """Whether `rec` is an archetype a player can be, not an NPC/pet class.

    The membership signal differs by dataset SCHEMA, not by name: Homecoming
    deleted i24's origin-restriction fields and added VillainRank (VR_NONE=0 on
    players, 1-11 on NPC ranks), while the forks kept i24's restriction gating,
    where every playable class carries Hero/Villain/Kheldian/Arachnos vocab and
    NPC classes carry none. Each field is present on exactly the dataset that
    owns it, so testing for presence picks the right one per record.

    Twin of `isPlayerClass` in `scripts/_player-classes.cjs`, which asks the same
    question of the same fields one hop downstream (the exported `tables/` JSON).
    """
    if rec.villain_rank is not None:
        return rec.villain_rank == 0
    return bool(rec.special_restrictions)


def player_class_names(records) -> tuple[str, ...]:
    """The `Class_*` tokens of `records`' player archetypes, in file order.

    These are the names the game's own gates compare against (`arch source>
    Class_Scrapper eq`), so they ship as the dataset spells them rather than as
    a derived id.
    """
    return tuple(rec.name for rec in records if is_player_class(rec))


def parse_classes(bin_path_or_data) -> list[ClassRecord]:
    """Parse classes.bin or villain_classes.bin into ClassRecord list."""
    r = open_parse7(bin_path_or_data)
    r.read_u4()  # block size
    count = r.read_u4()
    if isinstance(r, Parse6BinReader):
        flavor = "rebirth"
    else:
        flavor = _detect_parse7_flavor(r)

    records = []
    for _ in range(count):
        rec_len = r.read_u4()
        sub = r.sub_reader(rec_len)
        records.append(_read_class_record(sub, flavor))
        r.skip(rec_len)
    return records
