"""Enum mappings extracted from Parse7 .ksy format specifications."""

EFFECT_AREA: dict[int, str] = {
    0: "SingleTarget",
    1: "Cone",
    2: "Sphere",
    3: "Location",
    4: "Chain",
    5: "Volume",
    6: "NamedVolume",
    7: "Map",
    8: "Room",
    9: "Touch",
    10: "Box",
}

# eDeathCastableSetting (cast_flags third word) — values per the i24 header,
# census-verified against the `.powers` oracle 2026-07-21.
CASTABLE_AFTER_DEATH: dict[int, str] = {
    0: "AliveOnly",
    1: "DeadOnly",
    2: "DeadOrAlive",
}

# ShowPowerSetting (powers.h:436) — the ShowInInventory tail word, all
# layouts. The def grammar also accepts kTrue/kFalse as aliases for
# Default/Never; the binary stores only these five (SHOWFLAGS-1).
SHOW_POWER_SETTING: dict[int, str] = {
    0: "Never",
    1: "Default",
    2: "Always",
    3: "IfUsable",
    4: "IfOwned",
}

POWER_TYPE: dict[int, str] = {
    0: "Click",
    1: "Auto",
    2: "Toggle",
    3: "Boost",
    4: "Inspiration",
    5: "GlobalBoost",
}

BOOST_TYPE: dict[int, str] = {
    0: "Science",
    1: "Mutation",
    2: "Magic",
    3: "Technology",
    4: "Natural",
    5: "Accuracy",
    6: "Buff_Defense",
    7: "Buff_ToHit",
    8: "Confuse",
    9: "Damage",
    10: "Debuff_Defense",
    11: "Debuff_ToHit",
    12: "Fear",
    13: "SpeedFlying",
    14: "Heal",
    15: "Immobilize",
    16: "Jump",
    17: "Knockback",
    18: "Recharge",
    19: "SpeedRunning",
    20: "Sleep",
    21: "Stun",
    22: "Range",
    23: "EnduranceDiscount",
    24: "Buff_Damage",
    25: "Debuff_Damage",
    26: "Radius",
    27: "Cone",
    28: "Taunt",
    29: "Slow",
    30: "Hold",
    31: "Intangible",
    32: "Interrupt",
    33: "Recovery",
    34: "Endurance_Drain",
    35: "Res_Damage",
    36: "Hamidon",
    37: "Incarnate_Judgement",
    38: "Incarnate_Interface",
    39: "Incarnate_Lore",
    40: "Incarnate_Destiny",
}

# Rebirth's BOOST_TYPE enum diverges from HC at TWO insertion points:
#   - position 10 (shifts every HC value ≥10 up by 1)
#   - position 36 (additional shift, so HC values ≥35 end up +2 from HC)
#
# Element-wise comparison on Arachnos_Soldier.Burst confirms the +1 shift
# for low values:
#
#   HC      [23, 22, 18, 10, 9, 5]   = EndDisc, Range, Recharge, DebuffDef, Damage, Accuracy
#   Rebirth [24, 23, 19, 11, 9, 5]   ← +1 on every value ≥10
#
# The +2 shift for high values shows up on resistance toggles like
# Brute Dark Armor / Dark Embrace:
#
#   HC      [35, 23, 18]              = Res_Damage, EndDisc, Recharge
#   Rebirth [37, 24, 19]              ← Res_Damage at 37 (+2), others +1
#
# Position 10 in Rebirth is used only by `Boosts.Attuned_Return_From_The_Grave_*`
# (Halloween-event self-rez set) — actual label unknown.
# Position 36 in Rebirth is used by `Inherent.Rest` and the Halloween
# `Attuned_Inexhaustibility_A` boost set — the second Rebirth-only addition,
# label unknown.
BOOST_TYPE_REBIRTH: dict[int, str] = {
    0: "Science",
    1: "Mutation",
    2: "Magic",
    3: "Technology",
    4: "Natural",
    5: "Accuracy",
    6: "Buff_Defense",
    7: "Buff_ToHit",
    8: "Confuse",
    9: "Damage",
    10: "Rebirth_Boost_10",  # placeholder — Rebirth-only event-set boost
    11: "Debuff_Defense",
    12: "Debuff_ToHit",
    13: "Fear",
    14: "SpeedFlying",
    15: "Heal",
    16: "Immobilize",
    17: "Jump",
    18: "Knockback",
    19: "Recharge",
    20: "SpeedRunning",
    21: "Sleep",
    22: "Stun",
    23: "Range",
    24: "EnduranceDiscount",
    25: "Buff_Damage",
    26: "Debuff_Damage",
    27: "Radius",
    28: "Cone",
    29: "Taunt",
    30: "Slow",
    31: "Hold",
    32: "Intangible",
    33: "Interrupt",
    34: "Recovery",
    35: "Endurance_Drain",
    36: "Rebirth_Boost_36",  # placeholder — second Rebirth-only addition (Rest, Inexhaustibility set)
    37: "Res_Damage",
    38: "Hamidon",
    39: "Incarnate_Judgement",
    40: "Incarnate_Interface",
    41: "Incarnate_Lore",
    42: "Incarnate_Destiny",
}

# Event IDs used in AttribMod CancelEvents / RequiredEvents / Suppress arrays.
# Named by a positional def↔binary census (2026-07-20): raw-defs Suppress /
# CancelEvents keyword lines aligned element-wise against the exported record
# ids over 1,053 suppress / 437 cancel powers — every id below is unanimous.
# The mez block is consecutive (23..28 Stunned→Confused); an earlier partial
# table skipped 24 and drifted afterwards (27 was mislabeled Confused — it's
# Terrorized — and 33 "MissionObjectInteract" is Knocked).
# NB: CoD2's per-record event names are shifted one enum slot (its name for
# id N is the authored keyword for N+1), so CoD2 is NOT a valid oracle for
# this table — the authored .powers text is.
# Mapping is partial — IDs we haven't confirmed yet are passed through as ints.
EVENT_NAME: dict[int, str] = {
    1: "ActivateAttackClick",
    2: "Attacked",
    4: "Helped",
    12: "AttackedByOther",
    15: "HitByOther",
    17: "HitByFoe",
    21: "Damaged",
    23: "Stunned",
    24: "Immobilized",
    25: "Held",
    26: "Sleep",
    27: "Terrorized",
    28: "Confused",
    29: "Untouchable",
    33: "Knocked",
    34: "Repelled",
    35: "Teleported",
    37: "MissionObjectClick",
    41: "CommandedPet",
    42: "PetAttacked",
    47: "PseudoPetAttacked",
    48: "PseudoPetHelped",
}


# --- Event-enum generations ---------------------------------------------
#
# HC renumbers this enum when a patch inserts a new event, and the shift is
# SILENT: ids keep resolving, just to the neighbouring name. The 2026-07-30
# closed beta (assets/experimental) inserted one event at id 32, so every id
# >= 32 moved up one — `Knocked` (33) started reading as `Repelled` (34) and
# `MissionObjectClick` (37) as the fail-loud `Event_38`. Verified by
# histogramming every suppress/cancel id across full live and beta exports:
# ids <= 29 identical, 33/37/41/47/48 each vanish and reappear at +1, and a
# genuinely new id 32 arrives with 72 uses.
#
# The beta table is DERIVED from the live one rather than retyped, so the
# insertion point is the only thing stated and the names cannot drift apart.

def _insert_event(base: dict[int, str], at: int) -> dict[int, str]:
    """`base` renumbered as if one new event were inserted at id `at`."""
    return {(k + 1 if k >= at else k): v for k, v in base.items()}


EVENT_NAME_HC_BETA_2026_07: dict[int, str] = _insert_event(EVENT_NAME, 32)

# Ordered candidates for select_event_table(). Add a new entry when a patch
# renumbers again; do NOT edit EVENT_NAME in place (live must keep its ids).
EVENT_TABLE_CANDIDATES: tuple[tuple[str, dict[int, str]], ...] = (
    ("hc-live", EVENT_NAME),
    ("hc-beta-2026-07", EVENT_NAME_HC_BETA_2026_07),
)

# Ids that are unnamed in EVERY candidate (never observed in any corpus, so
# there is nothing to distinguish tables by). Excluded from scoring so they
# can't drag every candidate's coverage down equally.
_EVENT_IDS_NEVER_NAMED = frozenset(
    i for i in range(64)
    if not any(i in tbl for _, tbl in EVENT_TABLE_CANDIDATES)
)


def select_event_table(id_counts: dict[int, int],
                       min_coverage: float = 0.9) -> tuple[str, dict[int, str]]:
    """Pick the event table that best explains the ids a build actually uses.

    Scores each candidate by the share of observed event OCCURRENCES it can
    name (weighted by count, so one rare stray can't outvote a common event).
    A wrong table leaves the common ids unnamed, which is exactly the signal.

    Raises ValueError when even the best candidate falls below `min_coverage`
    — that means the build renumbered in a way nobody has modelled yet, and
    guessing would silently mislabel. Fail loud instead.
    """
    scorable = {i: n for i, n in id_counts.items()
                if i not in _EVENT_IDS_NEVER_NAMED}
    total = sum(scorable.values())
    if not total:
        return EVENT_TABLE_CANDIDATES[0]

    scored = [
        (sum(n for i, n in scorable.items() if i in tbl) / total, name, tbl)
        for name, tbl in EVENT_TABLE_CANDIDATES
    ]
    scored.sort(key=lambda s: -s[0])
    best_cov, best_name, best_tbl = scored[0]
    if best_cov < min_coverage:
        unnamed = sorted((n, i) for i, n in scorable.items() if i not in best_tbl)
        raise ValueError(
            "no known event-enum generation explains this build "
            f"(best '{best_name}' names only {best_cov:.1%} of event uses). "
            f"Most-used unnamed ids: {[i for _, i in unnamed[-6:]][::-1]}. "
            "A patch likely renumbered the event enum — add a candidate to "
            "EVENT_TABLE_CANDIDATES rather than editing EVENT_NAME."
        )
    return best_name, best_tbl


# Inverse of the live table, for recovering the raw id from a name that was
# already resolved at parse time (so recalibration needs no schema change).
EVENT_ID_BY_LIVE_NAME: dict[str, int] = {v: k for k, v in EVENT_NAME.items()}


def event_id_from_name(name: str) -> int | None:
    """Recover the raw event id from a name produced with the live table."""
    if name in EVENT_ID_BY_LIVE_NAME:
        return EVENT_ID_BY_LIVE_NAME[name]
    if name.startswith("Event_"):
        try:
            return int(name[len("Event_"):])
        except ValueError:
            return None
    return None


ATTRIB_NAME: dict[int, str] = {
    # Binary stores attrib indices as value * 4 (byte offsets).
    # Divide binary value by 4 to get the index used here.
    # Verified by cross-referencing 7,687 powers against CoD2 data.

    # Damage types (0-16)
    0: "Smashing_Dmg",
    1: "Lethal_Dmg",
    2: "Fire_Dmg",
    3: "Cold_Dmg",
    4: "Energy_Dmg",
    5: "Negative_Energy_Dmg",
    6: "Psionic_Dmg",
    7: "Heal_Dmg",
    8: "Special_Dmg",
    9: "Toxic_Dmg",
    10: "Radiation_Dmg",
    11: "Electrical_Dmg",
    12: "Sonic_Dmg",
    13: "Quantum_Dmg",
    14: "Unique1_Dmg",
    15: "Unique2_Dmg",
    # CoD2 names 16 Unique3_Dmg (DevouringEarth Avatar Corruption/Grasp_of_Gaea,
    # Divine_Core_Oscillation are the only users).
    16: "Unique3_Dmg",

    # Core character attribs (20-36)
    20: "HitPoints",
    21: "Absorb",
    22: "Endurance",
    24: "Rage",
    25: "ToHit",
    26: "Ranged",          # Positional defense
    27: "Melee",           # Positional defense
    28: "Area",            # Positional defense
    29: "Smashing",        # Typed defense
    30: "Lethal",
    31: "Fire",
    32: "Cold",
    33: "Energy",
    34: "Negative_Energy",
    35: "Psionic",
    36: "Toxic",

    # Movement & misc (46-62)
    46: "Base_Defense",
    47: "RunningSpeed",
    48: "FlyingSpeed",
    50: "JumpingSpeed",
    51: "JumpHeight",
    52: "MovementControl",
    53: "MovementFriction",
    54: "Stealth",
    55: "StealthRadius_PVE",
    56: "StealthRadius_PVP",
    57: "PerceptionRadius",
    58: "Regeneration",
    59: "Recovery",
    61: "ThreatLevel",
    62: "Taunt",
    63: "Placate",

    # Status effects (64-76)
    64: "Confused",
    65: "Afraid",
    66: "Terrorized",
    67: "Held",
    68: "Immobilized",
    69: "Stunned",
    70: "Sleep",
    71: "Fly",
    72: "Jump Pack",
    73: "Teleport",
    74: "Untouchable",
    75: "Intangible",
    76: "OnlyAffectsSelf",

    # XP/Influence (77-79)
    77: "ExperienceGain",
    78: "InfluenceGain",
    79: "PrestigeGain",

    # Combat (80-92)
    80: "Evade",
    81: "Knockup",
    82: "Knockback",
    83: "Repel",
    84: "Accuracy",
    # 85/86 proven via StrengthsDisallowed against the `.powers` oracle
    # (Fiery Aura Burn `kRadius` → offset 340, Energy Melee Stun `kArc` →
    # offset 344); matches the CharacterAttributes struct order
    # (fAccuracy, fRadius, fArc, fRange).
    85: "Radius",
    86: "Arc",
    87: "Range",
    90: "RechargeTime",
    # 91 sits between RechargeTime and EnduranceDiscount in the character-attrib
    # struct; CoD2 names it InterruptTime and its only users are the incarnate
    # alpha interrupt-reduction boosts (identical index on Rebirth).
    91: "InterruptTime",
    92: "EnduranceDiscount",

    # Meter & Elusivity (94-115)
    94: "Meter",
    95: "Ranged_Elusivity",
    96: "Melee_Elusivity",
    97: "Area_Elusivity",
    98: "Smashing_Elusivity",
    99: "Lethal_Elusivity",
    100: "Fire_Elusivity",
    101: "Cold_Elusivity",
    102: "Energy_Elusivity",
    103: "Negative_Energy_Elusivity",
    104: "Psionic_Elusivity",
    115: "ElusivityBase",

    # Meta/scripting region (indices 117-128) — SUPERSEDED. These entries look
    # "multi-purpose" only because `raw // 4` collapses them: unlike normal
    # (4-byte-aligned) attribs, the engine packs several distinct "special"
    # attribs into each of these indices and discriminates them with the low
    # 2 bits of the raw value (raw % 4). Dividing by 4 throws that sub-index
    # away and merges 2-4 unrelated attribs into one. Resolve special attribs
    # via SPECIAL_ATTRIB_BY_RAW / resolve_attrib() below (keyed on the RAW
    # value), NOT these collapsed entries — kept only for reference / any direct
    # index reader. See HOMECOMING_PARSER.md "attrib-118 misdecode".
    117: "Create_Entity",        # really the +1 slot; see SPECIAL_ATTRIB_BY_RAW
    118: "Set_Mode",             # really the +1 slot
    119: "Null",
    120: "Grant_Power",          # NB: the +2 slot (raw 482) is the OPPOSITE, Revoke_Power
    121: "Global_Chance_Mod",
    122: "Combat_Phase",
    123: "Recharge_Power",
    124: "Designer_Status",
    125: "Add_Behavior",
    126: "Set_Token",
    127: "Cancel_Effects",       # really Cancel_Mods (raw 511)
    128: "Execute_Power",
}


# ---------------------------------------------------------------------------
# Special-attrib region — byte-granular sub-index (HC / Parse7 & Veracity)
# ---------------------------------------------------------------------------
# Attrib indices 117-128 (raw u4 values 468-515) are NOT plain float-offset
# attribs. The engine packs several distinct "special" attribs into each
# collapsed `raw // 4` index and uses the low 2 bits (`raw % 4`) as a real
# sub-index. The normal decode path divides by 4 and so merges 2-4 unrelated
# attribs into one — the root cause of the historic "attrib-118 misdecode"
# (kXPDebtProtection / kSetCostume / kSetMode all landing on index 118) and of
# the broader "these indices are multi-purpose" note. This map keys the RAW u4
# value directly and MUST be consulted before the `// 4` lookup.
#
# Verified by cross-referencing every occurring special raw value against the
# HC `.powers` oracle (1,905 matched powers, 2026-07-04). A handful of rare
# raw values still lack an oracle name (475, 479, 485, 490, 501 — all count<=10
# NPC/prestige/proc edge cases) and fall through to "Special(<raw>)" rather than
# borrow a sibling's (wrong) name.
SPECIAL_ATTRIB_BY_RAW: dict[int, str] = {
    # idx 117
    468: "Translucency", 469: "Create_Entity", 470: "Clear_Damagers", 471: "Silent_Kill",
    # idx 118 (all three formerly collapsed to "Set_Mode")
    472: "XPDebtProtection", 473: "Set_Mode", 474: "Set_Costume",
    # idx 119
    476: "Null", 477: "Avoid", 478: "Reward",
    # idx 120 (482 Revoke_Power is the opposite of 481 Grant_Power)
    480: "Drop_Toggles", 481: "Grant_Power", 482: "Revoke_Power", 483: "Unset_Mode",
    # idx 121
    484: "Global_Chance_Mod", 486: "Grant_Boosted_Power", 487: "View_Attributes",
    # idx 122
    489: "Reward_Source_Team", 491: "Combat_Phase",
    # idx 123
    492: "Combat_Mod_Shift", 493: "Recharge_Power", 494: "Vision_Phase", 495: "Ninja_Run",
    # idx 124
    498: "Steam_Jump", 499: "Designer_Status",
    # idx 125
    500: "Exclusive_Vision_Phase", 502: "Set_Script_Value", 503: "Add_Behavior",
    # idx 126
    505: "Token_Add", 506: "Token_Set",
    # idx 127
    508: "Script_Notify", 509: "Force_Move", 511: "Cancel_Mods",
    # idx 128
    512: "Execute_Power",
}

# Raw-value window of the special region: 117*4 .. 128*4+3. A raw attrib value
# in this window is byte-granular and resolved via SPECIAL_ATTRIB_BY_RAW;
# everything else uses the 4-aligned ATTRIB_NAME lookup.
SPECIAL_ATTRIB_MIN = 468
SPECIAL_ATTRIB_MAX = 515


def resolve_attrib(raw: int) -> str:
    """Resolve a raw AttribMod attrib value (u4) to its attrib name.

    Normal attribs are stored as ``index * 4`` (4-byte-aligned); the special
    region (indices 117-128) is byte-granular, so those raw values are looked
    up directly. Unmapped special values become ``Special(<raw>)`` (honest
    unknown) rather than borrowing a collapsed-index sibling's name; unmapped
    normal values become ``Unknown(<index>)`` as before.
    """
    if SPECIAL_ATTRIB_MIN <= raw <= SPECIAL_ATTRIB_MAX:
        return SPECIAL_ATTRIB_BY_RAW.get(raw, f"Special({raw})")
    return ATTRIB_NAME.get(raw // 4, f"Unknown({raw // 4})")


# Rebirth (Parse6) keeps a slightly different attrib index layout: its normal
# attribs shift where HC inserted struct members, and its meta/scripting block
# starts at index 116 (raw 464) vs HC's 117 (raw 468). Template attribs
# resolve through `resolve_attrib_rebirth` below, which routes the special
# window (raw 464-507) through the byte-granular SPECIAL_ATTRIB_BY_RAW_REBIRTH
# map — the `116: "Create_Entity"` entry here is the collapsed view of that
# window's first slots, kept for non-template consumers that look up 4-aligned
# indices directly (boost/dim_returns tables never carry specials, so the
# collapse is unreachable there in practice).
#
# Other unmapped Rebirth indices still report as Unknown until a player-
# visible bug surfaces; the unknown name reaches the converter as-is and is
# filtered out by attrib-type detection rather than producing wrong data.
ATTRIB_NAME_REBIRTH: dict[int, str] = {
    **ATTRIB_NAME,
    # Rebirth shifts Accuracy to 85 (HC: 84). Index 84 may still exist in
    # Rebirth as something else, but in every observed boost-piece context
    # 85 carries the Accuracy aspect. Confirmed by cross-referencing 802
    # Rebirth boost pieces (Rolling_Barrage_A, Bonesnap_A, etc.) — all
    # match the HC equivalent's Accuracy attrib exactly. The 254 non-boost
    # uses (5thColumn, BanishedPantheon critter defense toggles, aspect=
    # Resistance, scale=1, duration=999999) line up with HC's pattern of
    # the same attrib doing double duty as "accuracy-debuff resistance"
    # on AV/elite-boss defensive powers.
    #
    # Symptom before fix: every Acc/Dam (and Acc/Dmg/Rech, etc.) IO piece
    # on Rebirth surfaced as `Unknown(85)` and the io-sets extractor
    # heuristically mapped that to "Endurance" — making Rolling Barrage's
    # piece 1 read as "Damage/Endurance" instead of "Accuracy/Damage".
    85: "Accuracy",
    # Rebirth shifts Range to 88 (HC: 87) — the same +1 pattern as Accuracy.
    # Symptom before fix: Rebirth-only IO sets with a +Range set bonus
    # (Rolling Barrage 2-piece +7.5% Range, etc.) silently lost the tier
    # because the bonus extractor saw `Unknown(88)` and failed lookup.
    88: "Range",
    116: "Create_Entity",
}


# ---------------------------------------------------------------------------
# Special-attrib region — byte-granular sub-index (Rebirth / Parse6)
# ---------------------------------------------------------------------------
# Rebirth's special block sits at base 464 (= sizeof(CharacterAttributes) in
# the released i24 source, whose ESpecialAttrib enum this table transcribes
# verbatim), one word below HC's 468 — HC added one attrib to the struct.
# Unlike HC's map (oracle-derived, with holes where the `.powers` defs lack
# coverage), this one is complete: the released-source enum is the definitive
# structure oracle for a fork that tracks stock i24, and the assignment is
# pinned by three independent anchors (2026-07-21 census, 122,156 templates):
#   - payload contents: raw 500's PrimaryStringList is always a power
#     full-name (277 redirect templates: quick-snipe variants, Nature
#     Affinity pet delegates), raw 499's is AI behavior scripts
#     ("Invincible(1),DoNotDrawAggro(1),…"), raw 498's is script keys
#     ("ZoneEvent>…") — exactly PowerRedirect / AddBehavior / SetSZEValue;
#   - HC-twin agreement: for every raw with same-named HC powers, the
#     base-464 name matches the HC twin's (oracle-verified) attrib far more
#     often than the collapsed //4 label (Grant_Power 1485 vs 83, Null 460
#     vs 76, Set_Mode 312 vs 63, Translucency 218 vs 20, …);
#   - the i24 header enum order itself.
# The old collapsed `raw // 4` lookup mislabeled the ENTIRE band (Set_Mode
# surfaced as Create_Entity, Null as Set_Mode, Grant_Power as Null,
# Power_Redirect as Add_Behavior — the root of the "Parse6 lowers Grant_Power
# to Null" workaround in _parse6_tail_params and of Rebirth's missing
# redirects, WS7).
#
# The band is stored base-relative because its base is not a constant of the
# format — it IS sizeof(CharacterAttributes), so it moves whenever a fork adds
# an attrib to that struct. Stock i24 / Rebirth / Thunderspy-through-2026-07-09
# carry 116 attribs (base 464); HC carries 117 (base 468), and Thunderspy's
# 2026-07-30 build joined it by appending `ReflectDamage` at index 116 — see
# `select_special_attrib_base`.
SPECIAL_ATTRIB_BAND: tuple[str, ...] = (
    "Translucency", "Create_Entity", "Clear_Damagers", "Silent_Kill",
    "XPDebtProtection", "Set_Mode", "Set_Costume", "Glide",
    "Null", "Avoid", "Reward", "XPDebt",
    "Drop_Toggles", "Grant_Power", "Revoke_Power", "Unset_Mode",
    "Global_Chance_Mod", "Power_Chance_Mod", "Grant_Boosted_Power",
    "View_Attributes", "Reward_Source", "Reward_Source_Team",
    "Clear_Fog", "Combat_Phase", "Combat_Mod_Shift", "Recharge_Power",
    "Vision_Phase", "Ninja_Run", "Walk", "Beast_Run",
    "Steam_Jump", "Designer_Status", "Exclusive_Vision_Phase",
    "Hover_Board", "Set_Script_Value", "Add_Behavior", "Power_Redirect",
    "Magic_Carpet", "Token_Add", "Token_Set", "Token_Clear",
    "Lua_Exec", "Force_Move", "Parkour_Run",
)

# Create_Entity's offset within the band, which is where a summon's AttribMod
# resolves once the band base is right. It used to also anchor a byte-scan that
# recovered pet lists by matching `base + SPECIAL_ATTRIB_CREATE_ENTITY` in the
# raw bytes — the one band consumer that DELETED rather than mislabelled when the
# base went stale (TSPY-6). That scan is retired; pets come from the AttribMod's
# own Params payload, so this is a naming offset again and nothing more.
SPECIAL_ATTRIB_CREATE_ENTITY = SPECIAL_ATTRIB_BAND.index("Create_Entity")


def special_attrib_table(base: int) -> dict[int, str]:
    """The byte-granular special-attrib map anchored at `base`."""
    return {base + k: name for k, name in enumerate(SPECIAL_ATTRIB_BAND)}


# The two struct generations seen in the wild. A 116-attrib struct puts the band
# at 464, a 117-attrib one at 468.
SPECIAL_ATTRIB_BASE_CANDIDATES: tuple[int, ...] = (464, 468)

SPECIAL_ATTRIB_BY_RAW_REBIRTH: dict[int, str] = special_attrib_table(464)

SPECIAL_ATTRIB_MIN_REBIRTH = 464
SPECIAL_ATTRIB_MAX_REBIRTH = 464 + len(SPECIAL_ATTRIB_BAND) - 1


def select_special_attrib_base(raw_counts: dict[int, int],
                               min_coverage: float = 0.9) -> int:
    """Pick the special-attrib band base this build actually uses.

    The band base is `sizeof(CharacterAttributes)`, so adding one attrib to that
    struct slides the whole band up 4 and every name in it becomes its
    neighbour's — Create_Entity reads as Set_Mode, Grant_Power as
    Power_Chance_Mod. Thunderspy did exactly that on 2026-07-30 (appending
    `ReflectDamage`), and nothing in powers.bin announces it: the header checksum
    is content-derived and `_detect_format` passes because the LAYOUT did not
    change, only the struct's size.

    Calibrate from the corpus instead, off the band's BOTTOM edge. Normal attrib
    indices are stored 4-ALIGNED (index*4), so a raw that is not a multiple of 4
    is unambiguously a band member — and since the band starts at `base`, no
    unaligned raw can ever fall below it. The lowest one therefore pins the base
    to a single 4-word group:

        base = 4 * (lowest_unaligned_raw // 4)

    That is a structural bound, not a preference, which matters because scoring
    candidates by coverage does NOT work here: the band is 44 wide, so sliding it
    4 keeps almost every value inside either window. On the real 2026-07-30 build
    coverage separates the two bases by 0.02 percentage points — noise. The
    bottom edge separates them by a whole group, every time.

    The lowest raw is taken over values with real support, so one stray cannot
    drag the base down a group. Create_Entity (base+1) anchors it in practice:
    it is the most-used byte-granular attrib in any powers corpus (~29% of
    unaligned uses on both Thunderspy builds).

    Raises ValueError when the implied base is not a generation we model, or when
    the band it implies fails to name `min_coverage` of the band's own uses —
    rather than picking a base that renames the corpus plausibly-but-wrongly.
    """
    unaligned = {v: n for v, n in raw_counts.items() if v % 4}
    total = sum(unaligned.values())
    if not total:
        # No byte-granular attrib in this corpus — the band is unused, so the
        # base is unobservable and also cannot mislabel anything.
        return SPECIAL_ATTRIB_MIN_REBIRTH

    # Ignore raws too rare to be a real band slot (a corrupt record, a field this
    # parser misreads). 0.5% still leaves every genuine band member in play.
    floor = max(2, total * 0.005)
    supported = [v for v, n in unaligned.items() if n >= floor] or list(unaligned)
    base = 4 * (min(supported) // 4)

    if base not in SPECIAL_ATTRIB_BASE_CANDIDATES:
        raise ValueError(
            "no known CharacterAttributes generation explains this build's "
            f"special-attrib band: its lowest byte-granular attrib raw is "
            f"{min(supported)}, implying base {base}, but only "
            f"{list(SPECIAL_ATTRIB_BASE_CANDIDATES)} are modelled. A patch "
            f"likely resized CharacterAttributes — add {base} to "
            "SPECIAL_ATTRIB_BASE_CANDIDATES rather than editing the band."
        )

    named = sum(n for v, n in unaligned.items()
                if base <= v < base + len(SPECIAL_ATTRIB_BAND))
    if named / total < min_coverage:
        outside = sorted(((n, v) for v, n in unaligned.items()
                          if not (base <= v < base + len(SPECIAL_ATTRIB_BAND))),
                         reverse=True)[:6]
        raise ValueError(
            f"special-attrib base {base} names only {named / total:.1%} of this "
            f"build's byte-granular attrib uses. Most-used raws outside the "
            f"band: {[v for _, v in outside]}. The band itself may have grown — "
            "extend SPECIAL_ATTRIB_BAND rather than moving the base."
        )
    return base


def resolve_attrib_rebirth(raw: int) -> str:
    """Resolve a raw Rebirth (Parse6) attrib value (u4) to its attrib name.

    Same convention as `resolve_attrib`, with Rebirth's base-464 special
    window and its shifted normal-attrib table. The special map has no holes
    (source-enum-derived), so a special raw outside it means a new fork
    addition — surfaced as ``Special(<raw>)``, never a sibling's name.
    """
    if SPECIAL_ATTRIB_MIN_REBIRTH <= raw <= SPECIAL_ATTRIB_MAX_REBIRTH:
        return SPECIAL_ATTRIB_BY_RAW_REBIRTH.get(raw, f"Special({raw})")
    return ATTRIB_NAME_REBIRTH.get(raw // 4, f"Unknown({raw // 4})")


# Thunderspy (Parse6-derived schema, Parse7 frame) keeps yet another attrib-index
# layout in its upper band. Its front string-attribs are decoded directly, but the
# post-`requires` affected-attribute INDEX array (see _parse_effect_template_thunderspy)
# is decoded via this map.
#
# The divergence is a WHOLE-BAND −1 SHIFT, not a handful of entries: Thunderspy's
# CharacterAttributes struct is one field short of HC's somewhere at index 88/89
# (both unnamed in ATTRIB_NAME), so every attrib from there up sits one slot lower.
# The same fork trait shows in its AttribMod flags word, which likewise omits one bit
# (see `_parse_effect_template_thunderspy`).
#
# Measured against the Rebirth export as an oracle by matching single-attrib templates
# on (power, scale, table, aspect) — 19,527 pairs: indices 0-87 agree exactly, and
# every off-by-one with volume sits at 89 and above (89→90 RechargeTime ×574, 91→92
# EnduranceDiscount ×157, 93→94 Meter ×34, 118→119 Null ×297, 120→121 Global_Chance_Mod
# ×222, plus the Elusivity band). RechargeTime and InterruptTime were patched
# individually before the band was measured; the shift subsumes both, and the
# dim_returns evidence for InterruptTime (the Interrupt boost-type's ED record stores
# offset 360 where HC stores 364) is the same fact seen from the other side.
#
# The byte-granular special/scripting band (`SPECIAL_ATTRIB_BAND`) is read from raw
# values on its own path, but this table carries the band's COLLAPSED 4-aligned view
# in its top entries (index 116+ on the base-464 generation) — and the index-array
# reader tries this table FIRST, so a band raw that happens to be 4-aligned is named
# from here, not from the special map. The two therefore have to move together:
# `thunderspy_attrib_table` slides these entries by the same amount the band base
# moves. Fixing only the special map leaves every 4-aligned band value on the old
# name, which is how the 2026-07-30 build still mislabeled 866 powers after the
# band itself was re-anchored.
_TSPY_ATTRIB_SHIFT_FROM = 89

# Index of the band's collapsed view in the base-464 generation (464 // 4).
_TSPY_BAND_INDEX_BASE = 116

_ATTRIB_NAME_THUNDERSPY_BASE: dict[int, str] = {
    **{i: n for i, n in ATTRIB_NAME.items() if i < _TSPY_ATTRIB_SHIFT_FROM},
    **{i - 1: n for i, n in ATTRIB_NAME.items() if i > _TSPY_ATTRIB_SHIFT_FROM},
}


def thunderspy_attrib_table(special_base: int) -> dict[int, str]:
    """Thunderspy's normal-attrib table for a build whose band sits at `special_base`.

    Only the band's collapsed entries move; the normal attribs below it are
    unaffected, which is what the corpus shows — across the 2026-07-30 build the
    raw counts for indices 0-115 are unchanged while every band raw moved +4.
    """
    shift = special_base // 4 - _TSPY_BAND_INDEX_BASE
    if not shift:
        return dict(_ATTRIB_NAME_THUNDERSPY_BASE)
    return {(i + shift if i >= _TSPY_BAND_INDEX_BASE else i): n
            for i, n in _ATTRIB_NAME_THUNDERSPY_BASE.items()}


ATTRIB_NAME_THUNDERSPY: dict[int, str] = thunderspy_attrib_table(464)

ATTRIB_MOD_TYPE: dict[int, str] = {
    # Verified via Ghidra keyword table at 0x1408eb958 in cityofheroes.exe —
    # values 0/1 were swapped in the old parser (the .ksy spec had them backwards),
    # which is the root cause of the "CoD2 re-labels Magnitude as Duration for mez
    # templates" confusion: the binary always stored Duration=0, and CoD2 was right.
    0: "Duration",
    1: "Magnitude",
    2: "Constant",
    3: "Expression",
}

ATTRIB_MOD_ASPECT: dict[int, str] = {
    # Binary stores aspect as value * 8 (byte offset).
    # Divide by 8 to get the index used here.
    0: "Current",
    1: "Maximum",
    2: "Strength",
    3: "Resistance",
    4: "Absolute",
}

# ModApplicationType. The enum starts at kModApplicationType_OnTick — there is
# no "Immediate" value. An earlier table invented one at 0 and pushed
# OnTick/OnActivate/OnDeactivate down a slot (parking OnExpire at a
# nonexistent 6), soft-wronging ~44k exported templates. Pinned by pairwise
# CoD2 comparison over 10,198 aligned powers (45k templates): raw 0→OnTick,
# 1→OnActivate, 2→OnDeactivate, 3→OnExpire with zero systematic
# counterexamples, and raw 4/5 (OnEnable/OnDisable, already correct) matching
# CoD2 exactly at 606/606 apiece.
ATTRIB_MOD_APPLICATION: dict[int, str] = {
    0: "OnTick",
    1: "OnActivate",
    2: "OnDeactivate",
    3: "OnExpire",
    4: "OnEnable",
    5: "OnDisable",
}

# ModTarget (`Common/entity/attribmod.h:69`). Seven contiguous members; ours
# are the authored `.powers` spellings, which the same header aliases onto the
# engine names at :509-520 (`kSelf`→kCaster, `kTarget`→kAffected,
# `kTargetsOwnerAndAllPets`→kAffectedsOwnerAndAllPets).
#
# The pairs are not two flavours of one recipient: each `...OwnerAndAllPets`
# member names a DIFFERENT anchor to walk up from, and the walk is what the
# engine does with it (`character_combat.c:749` — resolve the anchor to its
# top-level owner, attach there, then recurse over that owner's pet list). So
# raw 1 is the caster plus a pet copy, raw 5 is each affected entity's owner
# plus a pet copy — the same suffix over two different anchors.
ATTRIB_MOD_TARGET: dict[int, str] = {
    0: "Self",
    1: "SelfAndPets",
    2: "TargetOnly",
    # Raw 3 was missing until 2026-08-13 (TARGETS-2), so a template carrying it
    # exported as `Unknown(3)` — visible, but nothing had ever looked. It is
    # unobserved across all three exports; the label comes from the source
    # enum's own ordering, which the six neighbours confirm exactly.
    3: "TargetOnlyAndPets",
    4: "AnyAffected",
    5: "AnyAffectedAndPets",
    # Marker-targeted mods spawn/apply at named map markers laid down by a
    # companion Locator power; the marker names live in the template's
    # TargetInfo record (see _read_target_info). Verified against CoD2's
    # per-template `target` corpus-wide: all 53 raw-6 templates are "Marker"
    # and CoD2 shows no "Caster" target anywhere. (The old "Caster" label was
    # never exercised — no raw-6 template had ever parsed successfully.)
    6: "Marker",
}

ATTRIB_MOD_STACK: dict[int, str] = {
    # All 11 values verified via Ghidra keyword table at 0x1408ee708 in
    # cityofheroes.exe (24-byte rows: { const char* name, uint64 value }).
    # The bulk-audit-driven corrections below are all confirmed by this table.
    0: "Stack",
    1: "Ignore",
    2: "Extend",
    3: "Replace",
    4: "Overlap",
    5: "StackThenIgnore",
    6: "Refresh",
    7: "RefreshToCount",
    8: "Maximize",
    9: "Suppress",
    10: "Continuous",
    # kCollective is NOT in this enum — it belongs to ATTRIB_MOD_CASTER_STACK.
}

# CasterStackType. Two values only — the authored `.powers` vocabulary is
# kIndividual (unwritten default) / kCollective, and nothing else. Round-trip
# proven on Grant_Cover: its 3 authored `CasterStackType kCollective`
# templates are exactly the raw-1 templates (matching CoD2's Collective for
# all 420 raw-1 player templates). An earlier table invented "Unlimited" at 1
# and parked Collective at an unattested 2 (zero occurrences across all three
# datasets) — raw 2 is left unmapped so it surfaces as Unknown(2) if it ever
# appears.
ATTRIB_MOD_CASTER_STACK: dict[int, str] = {
    0: "Individual",
    1: "Collective",
}

# FINDING (2026-09-26). NOTHING READS THIS TABLE. It was imported by
# `_powers.py` and by `export_powers.py` and used by neither; the lint gate added that day
# found both imports and they are gone, which leaves the table with no reader at all.
#
# The three strings themselves are very much alive — `is_pvp` is exported on every effect
# group as exactly this tri-state. They are just spelled somewhere else each time: as three
# module constants in `_requires.py`, as a comment on `EffectGroup.is_pvp` in
# `_dataclasses.py`, and as bare literals at roughly ten sites in `_powers.py`. So the
# lookup that looks like the authority is the one copy nobody consults.
#
# Left standing rather than deleted. The raw flag it decodes is read from the bin as an
# integer, and whether the branches in `_powers.py` cover the same three values this table
# does is a question for a re-export and the oracle, not for an edit.
PVP_FLAG: dict[int, str] = {
    0: "EITHER",
    1: "PVE_ONLY",
    2: "PVP_ONLY",
}

# Target-type enum layouts diverge by server family. Homecoming inserted
# DeadAny/DeadOrAliveAny after Any(21), shifting everything that was 22-34 in
# the i25 enum by +2 (proof in TARGET_TYPE_HC below). Rebirth, Thunderspy and
# Veracity keep the i25 layout — proven by round-trip: their raw 22 decodes
# teleport-location powers (Lightning_Rod, Pool Teleport), raw 24 decodes
# Soul_Extraction's dead-pet target, raw 33 decodes league rezzes — all
# nonsense under the HC layout, all exact under i25. Rebirth additionally
# appends values past Position(34) (raw 35 on its league-recall powers) that
# stay honestly Unknown until a Rebirth-side oracle names them.
TARGET_TYPE_HC: dict[int, str] = {
    0: "None",
    1: "Self",
    2: "Player",
    3: "PlayerHero",
    4: "PlayerVillain",
    5: "DeadPlayer",
    6: "DeadPlayerFriend",
    7: "DeadPlayerFoe",
    8: "Teammate",
    9: "DeadTeammate",
    10: "DeadOrAliveTeammate",
    11: "Villain",
    12: "DeadVillain",
    13: "NPC",
    14: "DeadOrAliveFriend",
    15: "DeadFriend",
    16: "Friend",
    17: "DeadOrAliveFoe",
    18: "DeadFoe",
    19: "Foe",
    20: "Location",
    21: "Any",
    # HC inserted DeadAny/DeadOrAliveAny after Any, completing the alive/dead/
    # dead-or-alive triple every other target family has; everything that was
    # 22-34 in the i25 enum shifts +2. Proven by round-tripping authored defs:
    # `Target kTeleport` (Pool.Teleportation.Teleport) stores 24,
    # `kDeadOrAliveAny` (Shock_Therapy.Defibrillate) stores 23,
    # `kDeadOrAliveLeaguemate` (Recall_Friend EntsAffected) stores 35 — and by
    # a corpus join of 12k CoD2 powers (60x raw 24 = "Location (Teleport)",
    # 88x raw 27 = "Own Pet (Alive)", 42x raw 35 = "Leaguemate", ...).
    # 22 is unobserved in any export; DeadAny is the family-pattern inference.
    22: "DeadAny",
    23: "DeadOrAliveAny",
    24: "Teleport",
    25: "DeadOrAliveMyPet",
    26: "DeadMyPet",
    27: "MyPet",
    28: "MyOwner",
    29: "MyCreator",
    30: "MyCreation",
    31: "DeadMyCreation",
    32: "DeadOrAliveMyCreation",
    33: "Leaguemate",
    34: "DeadLeaguemate",
    35: "DeadOrAliveLeaguemate",
    36: "Position",
    # Post-Position addition; CoD2 renders raw 37 as "Anything" (17 users,
    # e.g. Traps Time_Bomb target_type). No authored token observed yet.
    37: "Anything",
}

TARGET_TYPE_CLASSIC: dict[int, str] = {
    **{k: v for k, v in TARGET_TYPE_HC.items() if k <= 21},
    22: "Teleport",
    23: "DeadOrAliveMyPet",
    24: "DeadMyPet",
    25: "MyPet",
    26: "MyOwner",
    27: "MyCreator",
    28: "MyCreation",
    29: "DeadMyCreation",
    30: "DeadOrAliveMyCreation",
    31: "Leaguemate",
    32: "DeadLeaguemate",
    33: "DeadOrAliveLeaguemate",
    34: "Position",
}

# Parse6 (Rebirth/i24-era) power-event enum, for CancelEvents and the tail
# Suppress records. NOT the same numbering as HC's EVENT_NAME: pairing every
# Rebirth suppress record against the same power's HC template (2026-07-20,
# position-paired where counts and durations agree) shows identical ids only
# at the bottom of the table (1 ActivateAttackClick, 2 Attacked), a +4 shift
# through the mez/damage band (13→17 HitByFoe … 29→33 Knocked: HC inserted
# four events below 13), and +6 at 31→37 MissionObjectClick. Only the ids
# observed in the corpus are named — per fail-loud, everything else renders
# as Event_<id> rather than borrowing a possibly-shifted HC name.
#
# Thunderspy shares this numbering (WRAP-3, 2026-07-31). Position-pairing
# alone does NOT prove that: the five-element mez suppress block is written in
# a different ORDER on each side, and a permutation reads as a consistent 93%
# "majority" that maps 19→Held / 21→Stunned. Two order-free methods settle it
# instead, and they cover disjoint halves of the table:
#   * records forced by a duration unique within their array, or by being the
#     last unpinned element of an array whose others are already pinned —
#     unanimous for 1, 8, 19, 20, 21, 22, 23, 24, 29, 31;
#   * per-template set co-occurrence across every power the two forks share,
#     which ignores order entirely — the discriminating result for 2 (→2
#     Attacked), 13 (→17 HitByFoe) and 17 (→21 Damaged), each beating its
#     runner-up by a wide margin. It is deliberately NOT used for the mez
#     block, where those five ids always occur together and so score as a tie.
# Both methods reproduce the same shift running Rebirth→HC and Thunderspy→HC,
# and Thunderspy→Rebirth is the identity on every observed id.
#
# 25 = Untouchable is anchored semantically rather than by shift arithmetic:
# it is carried by the intangibility powers (Detention Field, Sonic Cage,
# Black Hole, Dimension Shift) on both forks, and Rebirth's Dimension Shift
# pairs (20, 25) against Homecoming's (Immobilized 24, Untouchable 29) with
# 20 already pinned. Both forks shipped it as the fail-loud Event_25 before.
EVENT_NAME_PARSE6: dict[int, str] = {
    1: "ActivateAttackClick",
    2: "Attacked",
    13: "HitByFoe",
    17: "Damaged",
    19: "Stunned",
    20: "Immobilized",
    21: "Held",
    22: "Sleep",
    23: "Terrorized",
    25: "Untouchable",
    29: "Knocked",
    31: "MissionObjectClick",
}

# AttribModParam Knock vec_start/vec_end reference points. Derived by pairing
# every live Knock params record against the CoD2 oracle (2026-07-20, 165
# paired templates): each observed index voted unanimously for one kKnock_*
# name (the two single stray votes were sequence-pairing artifacts on powers
# CoD2 lacks). Indices 0 and 4 are unobserved in the corpus and stay unnamed.
KNOCK_VEC_POSITION: dict[int, str] = {
    1: "Source",
    2: "Target",
    3: "MainTarget",
    5: "Up",
    6: "Down",
    7: "Facing",
    8: "Back",
}

# AttribModParam ScriptNotify event ids, from the same CoD2 pairing (84
# paired templates, unanimous). Authored defs write `Event FirstTick`.
NOTIFY_EVENT: dict[int, str] = {
    1: "Apply",
    2: "FirstTick",
}
