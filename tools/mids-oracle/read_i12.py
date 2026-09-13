#!/usr/bin/env python3
"""
DSH1 — Mids Reborn I12.mhd oracle reader (PoC).

Ports the MidsReborn `BinaryReader` deserialization layout to Python so we can
diff every power's effect *structure* against a Mids-derived oracle (the
Deductive Schema Harness — streams/DEDUCTIVE_SCHEMA_HARNESS.md).

Trust boundary (see the plan doc): Mids is a STRUCTURAL oracle only. This reader
emits effects at *template granularity* — the same abstraction level as our bin
parser — so it is a clean parser-to-parser diff, no Mids calc engine required.
TRUST sub-effect count/identity, EffectType/DamageType/MezType/PvMode/Resistible/
ModifierTable selection, Aspect/AttribType. DISTRUST exact Scale/Mag/Duration.

Layout source of truth (verbatim field order, do not reorder):
  - DatabaseAPI.LoadMainDatabase   (top-level: header markers + counts)
  - Power(BinaryReader)            Core/Base/Data_Classes/Power.cs:213
  - Effect(BinaryReader)           Core/Base/Data_Classes/Effect.cs:87
  - Requirement(BinaryReader)      Core/Requirement.cs:67
  - enums                          Core/Enums.cs

The single desync risk the plan flags is the `+1` array idiom: Mids writes
`Length - 1` as the count and reads back `count + 1` records (an off-by-one baked
into the format). Power/Requirement arrays and the Effects/SetTypes lists all use
it; the Effect *conditionals* loop is the one exact-count (`< count`) exception.

Self-check (gate 1): after reading `count + 1` powers we MUST land exactly on the
`BEGIN:SUMMONS` string. If a single field is misread the stream desyncs and this
assertion fails at a reportable byte offset — that is the proof the layout is right.

Usage:
    python3 read_i12.py                          # HC I12.mhd -> stdout JSONL
    python3 read_i12.py --out oracle.jsonl       # to a file
    python3 read_i12.py --grep "Trick_Arrow"     # only powers whose full_name matches
    python3 read_i12.py --limit 50               # first 50 powers only (quick parse check)
    python3 read_i12.py --mhd /path/to/I12.mhd   # a different DB (e.g. Generic, Thunderspy)
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import struct
import sys

# --- enum tables (Core/Enums.cs, 0-indexed, no gaps for these) --------------

E_ASPECT = ["Res", "Max", "Abs", "Str", "Cur"]
E_ATTRIB_TYPE = ["Magnitude", "Duration", "Expression"]
E_DAMAGE = [
    "None", "Smashing", "Lethal", "Fire", "Cold", "Energy", "Negative", "Toxic",
    "Psionic", "Special", "Melee", "Ranged", "AoE", "Unique1", "Unique2", "Unique3",
]
E_EFFECT_CLASS = ["Primary", "Secondary", "Tertiary", "Special", "Ignored", "DisplayOnly"]
E_POWER_TYPE = ["Click", "Auto", "Toggle", "Boost", "Inspiration", "GlobalBoost"]
E_EFFECT_TYPE = [
    "None", "Accuracy", "ViewAttrib", "Damage", "DamageBuff", "Defense", "DropToggles",
    "Endurance", "EnduranceDiscount", "Enhancement", "Fly", "SpeedFlying", "GrantPower",
    "Heal", "HitPoints", "InterruptTime", "JumpHeight", "SpeedJumping", "Meter", "Mez",
    "MezResist", "MovementControl", "MovementFriction", "PerceptionRadius", "Range",
    "RechargeTime", "Recovery", "Regeneration", "ResEffect", "Resistance", "RevokePower",
    "Reward", "SpeedRunning", "SetCostume", "SetMode", "Slow", "StealthRadius",
    "StealthRadiusPlayer", "EntCreate", "ThreatLevel", "ToHit", "Translucency",
    "XPDebtProtection", "SilentKill", "Elusivity", "GlobalChanceMod", "LevelShift",
    "UnsetMode", "Rage", "MaxRunSpeed", "MaxJumpSpeed", "MaxFlySpeed", "DesignerStatus",
    "PowerRedirect", "TokenAdd", "ExperienceGain", "InfluenceGain", "PrestigeGain",
    "AddBehavior", "RechargePower", "RewardSourceTeam", "VisionPhase", "CombatPhase",
    "ClearFog", "SetSZEValue", "ExclusiveVisionPhase", "Absorb", "XAfraid", "XAvoid",
    "BeastRun", "ClearDamagers", "EntCreate_x", "Glide", "Hoverboard", "Jumppack",
    "MagicCarpet", "NinjaRun", "Null", "NullBool", "Stealth", "SteamJump", "Walk",
    "XPDebt", "ForceMove", "ModifyAttrib", "ExecutePower",
]
E_MEZ = [
    "None", "Confused", "Held", "Immobilized", "Knockback", "Knockup", "OnlyAffectsSelf",
    "Placate", "Repel", "Sleep", "Stunned", "Taunt", "Terrorized", "Untouchable",
    "Teleport", "ToggleDrop", "Afraid", "Avoid", "CombatPhase", "Intangible",
]
E_PVX = ["Any", "PvE", "PvP"]
E_TO_WHO = ["Unspecified", "Target", "Self", "All"]
E_SPECIAL_CASE = [
    "None", "Hidden", "Domination", "Scourge", "Mezzed", "CriticalHit", "CriticalBoss",
    "CriticalMinion", "Robot", "Assassination", "Containment", "Defiance",
    "TargetDroneActive", "Combo", "VersusSpecial", "NotDisintegrated", "Disintegrated",
    "NotAccelerated", "Accelerated", "NotDelayed", "Delayed", "ComboLevel0", "ComboLevel1",
    "ComboLevel2", "ComboLevel3", "FastMode", "NotAssassination", "PerfectionOfBody0",
    "PerfectionOfBody1", "PerfectionOfBody2", "PerfectionOfBody3", "PerfectionOfMind0",
    "PerfectionOfMind1", "PerfectionOfMind2", "PerfectionOfMind3", "PerfectionOfSoul0",
    "PerfectionOfSoul1", "PerfectionOfSoul2", "PerfectionOfSoul3", "TeamSize1", "TeamSize2",
    "TeamSize3", "NotComboLevel3", "ToHit97", "DefensiveAdaptation", "EfficientAdaptation",
    "OffensiveAdaptation", "NotDefensiveAdaptation", "NotDefensiveNorOffensiveAdaptation",
    "BoxingBuff", "KickBuff", "Supremacy", "SupremacyAndBuffPwr", "PetTier2", "PetTier3",
    "PackMentality", "NotPackMentality", "FastSnipe", "NotFastSnipe", "CrossPunchBuff",
    "NotCrossPunchBuff", "NotBoxingBuff", "NotKickBuff",
]
E_STACKING = ["No", "Yes"]


def enum_name(table, value):
    if 0 <= value < len(table):
        return table[value]
    return f"Unknown({value})"


# --- .NET BinaryReader primitives -------------------------------------------

class Reader:
    """A cursor over the .mhd bytes mirroring .NET BinaryReader (little-endian,
    UTF-8 strings with a 7-bit-encoded length prefix)."""

    __slots__ = ("buf", "pos")

    def __init__(self, buf: bytes, pos: int = 0):
        self.buf = buf
        self.pos = pos

    def int32(self) -> int:
        v = struct.unpack_from("<i", self.buf, self.pos)[0]
        self.pos += 4
        return v

    def int64(self) -> int:
        v = struct.unpack_from("<q", self.buf, self.pos)[0]
        self.pos += 8
        return v

    def single(self) -> float:
        v = struct.unpack_from("<f", self.buf, self.pos)[0]
        self.pos += 4
        return v

    def boolean(self) -> bool:
        v = self.buf[self.pos]
        self.pos += 1
        return v != 0

    def _7bit_len(self) -> int:
        # .NET BinaryReader.Read7BitEncodedInt: base-128, low 7 bits per byte,
        # high bit = continuation. Strings >= 128 bytes use multiple length bytes.
        result = 0
        shift = 0
        while True:
            b = self.buf[self.pos]
            self.pos += 1
            result |= (b & 0x7F) << shift
            if (b & 0x80) == 0:
                return result
            shift += 7
            if shift > 35:
                raise ValueError(f"7-bit length too long at pos {self.pos}")

    def string(self) -> str:
        n = self._7bit_len()
        s = self.buf[self.pos:self.pos + n].decode("utf-8", errors="replace")
        self.pos += n
        return s


# --- record readers (verbatim field order from the C# constructors) ----------

def read_requirement(r: Reader) -> dict:
    """Requirement(BinaryReader) — Core/Requirement.cs:67. All four arrays use the
    count+1 idiom. We keep ClassName (used to attribute a power to an AT) and
    discard the rest, but every field MUST be consumed to stay byte-aligned."""
    class_name = [r.string() for _ in range(r.int32() + 1)]
    _class_name_not = [r.string() for _ in range(r.int32() + 1)]
    for _ in range(r.int32() + 1):          # PowerID: count+1 pairs
        r.string(); r.string()
    for _ in range(r.int32() + 1):          # PowerIDNot: count+1 pairs
        r.string(); r.string()
    return {"class_name": class_name}


def read_effect(r: Reader) -> dict:
    """Effect(BinaryReader) — Core/Base/Data_Classes/Effect.cs:87.
    Every field is read for alignment; we project only the structurally-meaningful
    ones into the JSON (the DSH identity + advisory numerics)."""
    power_full_name = r.string()
    unique_id = r.int32()
    effect_class = r.int32()
    effect_type = r.int32()
    damage_type = r.int32()
    mez_type = r.int32()
    et_modifies = r.int32()
    summon = r.string()
    delayed_time = r.single()
    ticks = r.int32()
    stacking = r.int32()
    base_probability = r.single()
    suppression = r.int32()
    buffable = r.boolean()
    resistible = r.boolean()
    special_case = r.int32()
    _variable_modified_override = r.boolean()
    ignore_scaling = r.boolean()
    pv_mode = r.int32()
    to_who = r.int32()
    _display_pct_override = r.int32()
    scale = r.single()
    n_magnitude = r.single()
    n_duration = r.single()
    attrib_type = r.int32()
    aspect = r.int32()
    modifier_table = r.string()
    _near_ground = r.boolean()
    _cancel_on_miss = r.boolean()
    _requires_tohit_check = r.boolean()
    uid_class_name = r.string()
    _nid_class_name = r.int32()
    # Expressions { Duration, Magnitude, Probability } — object-initializer order.
    expr_duration = r.string()
    expr_magnitude = r.string()
    expr_probability = r.string()
    _reward = r.string()
    effect_id = r.string()
    _ignore_ed = r.boolean()
    _override = r.string()
    procs_per_minute = r.single()
    _power_attribs = r.int32()
    # AtrOrig* (12) then AtrMod* (12). int32 for Arc / EffectArea / MaxTargets.
    for _ in range(2):
        r.single()          # Accuracy
        r.single()          # ActivatePeriod
        r.int32()           # Arc
        r.single()          # CastTime
        r.int32()           # EffectArea
        r.single()          # EnduranceCost
        r.single()          # InterruptTime
        r.int32()           # MaxTargets
        r.single()          # Radius
        r.single()          # Range
        r.single()          # RechargeTime
        r.single()          # SecondaryRange
    conditionals = []
    cond_count = r.int32()                  # exact count (< count), NOT count+1
    for _ in range(cond_count):
        conditionals.append([r.string(), r.string()])

    eff = {
        "effect_class": enum_name(E_EFFECT_CLASS, effect_class),
        "effect_type": enum_name(E_EFFECT_TYPE, effect_type),
        "damage_type": enum_name(E_DAMAGE, damage_type),
        "mez_type": enum_name(E_MEZ, mez_type),
        "et_modifies": enum_name(E_EFFECT_TYPE, et_modifies),
        "pv_mode": enum_name(E_PVX, pv_mode),
        "to_who": enum_name(E_TO_WHO, to_who),
        "resistible": resistible,
        "buffable": buffable,
        "ignore_scaling": ignore_scaling,
        "special_case": enum_name(E_SPECIAL_CASE, special_case),
        "attrib_type": enum_name(E_ATTRIB_TYPE, attrib_type),
        "aspect": enum_name(E_ASPECT, aspect),
        "modifier_table": modifier_table,
        "stacking": enum_name(E_STACKING, stacking),
        # advisory numerics — DISTRUST for equality, keep for context/skew tier
        "scale": round(scale, 6),
        "mag": round(n_magnitude, 6),
        "duration": round(n_duration, 6),
        "ticks": ticks,
        "base_probability": round(base_probability, 6),
        "delayed_time": round(delayed_time, 6),
        "procs_per_minute": round(procs_per_minute, 6),
    }
    if summon:
        eff["summon"] = summon
    if uid_class_name:
        eff["uid_class"] = uid_class_name
    if effect_id:
        eff["effect_id"] = effect_id
    if conditionals:
        eff["conditionals"] = conditionals
    for key, val in (("expr_duration", expr_duration),
                     ("expr_magnitude", expr_magnitude),
                     ("expr_probability", expr_probability)):
        if val:
            eff[key] = val
    if unique_id:
        eff["unique_id"] = unique_id
    if power_full_name:
        eff["power_full_name"] = power_full_name
    return eff


def read_power(r: Reader) -> dict:
    """Power(BinaryReader) — Core/Base/Data_Classes/Power.cs:213."""
    _static_index = r.int32()
    full_name = r.string()
    group_name = r.string()
    set_name = r.string()
    power_name = r.string()
    display_name = r.string()
    available = r.int32()
    requires = read_requirement(r)
    _modes_required = r.int32()
    _modes_disallowed = r.int32()
    power_type = r.int32()
    _accuracy = r.single()
    _attack_types = r.int32()
    for _ in range(r.int32() + 1):          # GroupMembership: count+1 strings
        r.string()
    _entities_affected = r.int32()
    _entities_auto_hit = r.int32()
    _target = r.int32()
    _target_los = r.boolean()
    _range = r.single()
    _target_secondary = r.int32()
    _range_secondary = r.single()
    _end_cost = r.single()
    _interrupt_time = r.single()
    _cast_time = r.single()
    _recharge_time = r.single()
    _base_recharge_time = r.single()
    _activate_period = r.single()
    _effect_area = r.int32()
    _radius = r.single()
    _arc = r.int32()
    _max_targets = r.int32()
    _max_boosts = r.string()
    _cast_flags = r.int32()
    _ai_report = r.int32()
    _num_charges = r.int32()
    _usage_time = r.int32()
    _life_time = r.int32()
    _life_time_in_game = r.int32()
    _num_allowed = r.int32()
    _do_not_save = r.boolean()
    for _ in range(r.int32() + 1):          # BoostsAllowed: count+1 strings
        r.string()
    _cast_through_hold = r.boolean()
    ignore_strength = r.boolean()
    _desc_short = r.string()
    _desc_long = r.string()
    for _ in range(r.int32() + 1):          # Enhancements: count+1 int32
        r.int32()
    set_type_count = r.int32()              # SetTypes: reads set_type_count+1 (i <= n)
    for _ in range(set_type_count + 1):
        r.int32()
    _click_buff = r.boolean()
    _always_toggle = r.boolean()
    level = r.int32()
    _allow_front_loading = r.boolean()
    _variable_enabled = r.boolean()
    _variable_override = r.boolean()
    _variable_name = r.string()
    _variable_min = r.int32()
    _variable_max = r.int32()
    for _ in range(r.int32() + 1):          # UIDSubPower: count+1 strings
        r.string()
    for _ in range(r.int32() + 1):          # IgnoreEnh: count+1 int32
        r.int32()
    for _ in range(r.int32() + 1):          # Ignore_Buff: count+1 int32
        r.int32()
    _skip_max = r.boolean()
    _inherent_type = r.int32()
    _display_location = r.int32()
    _mutex_auto = r.boolean()
    _mutex_ignore = r.boolean()
    _absorb_summon_effects = r.boolean()
    _absorb_summon_attributes = r.boolean()
    _show_summon_anyway = r.boolean()
    _never_auto_update = r.boolean()
    _never_auto_update_requirements = r.boolean()
    _include_flag = r.boolean()
    _forced_class = r.string()
    _sort_override = r.boolean()
    _boost_boostable = r.boolean()
    _boost_use_player_level = r.boolean()
    effects = [read_effect(r) for _ in range(r.int32() + 1)]  # Effects: count+1
    _hidden_power = r.boolean()
    _active = r.boolean()
    _taken = r.boolean()
    _stacks = r.int32()
    _variable_start = r.int32()

    return {
        "full_name": full_name,
        "group": group_name,
        "set": set_name,
        "power": power_name,
        "display": display_name,
        # `available` is the 0-based level the power unlocks at; `level` is Mids' own
        # ordering level. Both are carried because the two disagree for granted powers,
        # and MBDIMPORT-2's name join uses availability to tell a renamed power from a
        # reused name — a display match that disagrees on when the power unlocks is a
        # coincidence, not an identity.
        "available": available,
        "level": level,
        "power_type": enum_name(E_POWER_TYPE, power_type),
        "class_name": requires["class_name"],
        "ignore_strength": ignore_strength,
        "effects": effects,
    }


# --- top-level -------------------------------------------------------------

POWERS_MARKER = b"\x0cBEGIN:POWERS"     # .NET string: 0x0c len prefix + "BEGIN:POWERS"
SUMMONS_MARKER = "BEGIN:SUMMONS"

# The Homecoming main database, in the Wine prefix `tools/mids-oracle/mids-wine.sh`
# drives. This pointed into `MidsReborn-master/` until PROV-3 (2026-09-13) — a vendored,
# gitignored source tree that holds only the two `EnhDB.mhd` files on this checkout, so the
# default resolved to nothing and DSH5 could not be rerun as checked out. A path into a
# gitignored tree is not a citation; `provenance()` below is what an artefact should carry.
DEFAULT_MHD = os.path.expanduser(
    "~/Games/mids-reborn/drive_c/MidsReborn/Databases/Homecoming/I12.mhd"
)


def read_header(buf: bytes) -> tuple[str, str]:
    """The first two records are the database name and its version string."""
    r = Reader(buf, 0)
    return r.string(), r.string()


def _cite_path(path: str) -> str:
    """A path a reader can place. Inside the repo it is repo-relative; outside it is
    home-relative, because the databases live in a Wine prefix and `os.path.relpath`
    renders that as a `../../` chain that reads like a repo path and is not one."""
    full = os.path.abspath(os.path.expanduser(path))
    repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    if full.startswith(repo + os.sep):
        return os.path.relpath(full, repo)
    home = os.path.expanduser("~")
    return os.path.join("~", os.path.relpath(full, home)) if full.startswith(home + os.sep) else full


def provenance(path: str, buf: bytes, power_count: int) -> dict:
    """What a committed artefact must say about the database it was derived from.

    PROV-3: two artefacts here were built from a Mids `I12.mhd` and recorded it two
    ways. The name bridge stated version + sha256; `oracle_divergence_rules.json`
    stated a PATH into a gitignored tree and nothing else. Only one of those can be
    checked, and when they were finally compared they turned out to be different
    databases — 10,986 powers against 11,002 — which nothing in either file said.

    So both go through this one function now. Two formats is the defect; a shared
    primitive is why it cannot come back. `database` alone names no fork (the header
    reads "Mids Reborn Powers Database" in all of them) and `version` alone names none
    either — see MBDEXPORT-2 — so the sha256 is the identifying field and the rest is
    what a human reads.
    """
    name, version = read_header(buf)
    return {
        "path": _cite_path(path),
        "database": name,
        "version": version,
        "sha256": hashlib.sha256(buf).hexdigest(),
        "powerCount": power_count,
    }


def read_powers(buf: bytes):
    """Seek to the Powers section, read count+1 powers, and prove alignment by
    asserting we land on BEGIN:SUMMONS. Yields power dicts; returns (count, ok)."""
    marker = buf.find(POWERS_MARKER)
    if marker < 0:
        raise ValueError("BEGIN:POWERS marker not found — not an I12 main DB?")
    r = Reader(buf, marker + len(POWERS_MARKER))
    declared = r.int32()
    total = declared + 1                 # the +1 idiom
    powers = []
    for i in range(total):
        start = r.pos
        try:
            powers.append(read_power(r))
        except Exception as e:  # noqa: BLE001 — report where the stream desynced
            raise ValueError(
                f"desync reading power #{i}/{total} (started at byte {start}, "
                f"reached {r.pos}): {e}"
            ) from e
    # gate 1: the byte immediately after the last power MUST be BEGIN:SUMMONS.
    landed = r.string()
    ok = landed == SUMMONS_MARKER
    if not ok:
        raise ValueError(
            f"alignment check FAILED: after {total} powers expected "
            f"'{SUMMONS_MARKER}', got '{landed[:40]}' at byte {r.pos}. "
            "A field in Power/Effect/Requirement is misread."
        )
    return powers, total


def main(argv=None):
    ap = argparse.ArgumentParser(description="Mids I12.mhd oracle reader (DSH1 PoC)")
    ap.add_argument("--mhd", default=DEFAULT_MHD, help="path to I12.mhd (default: HC)")
    ap.add_argument("--out", default="-", help="output JSONL path ('-' = stdout)")
    ap.add_argument("--grep", default=None, help="only powers whose full_name contains this (case-insensitive)")
    ap.add_argument("--limit", type=int, default=None, help="emit only the first N (post-filter) powers")
    args = ap.parse_args(argv)

    mhd_path = os.path.abspath(args.mhd)
    if not os.path.isfile(mhd_path):
        print(f"error: no such file: {mhd_path}", file=sys.stderr)
        return 2
    with open(mhd_path, "rb") as fh:
        buf = fh.read()

    powers, total = read_powers(buf)

    needle = args.grep.lower() if args.grep else None
    selected = [p for p in powers if needle is None or needle in p["full_name"].lower()]
    if args.limit is not None:
        selected = selected[:args.limit]

    out = sys.stdout if args.out == "-" else open(args.out, "w", encoding="utf-8")
    try:
        for p in selected:
            out.write(json.dumps(p, ensure_ascii=False) + "\n")
    finally:
        if out is not sys.stdout:
            out.close()

    n_eff = sum(len(p["effects"]) for p in powers)
    print(
        f"[read_i12] {os.path.basename(mhd_path)}: parsed {total} powers "
        f"({n_eff} effects) — alignment OK (landed on BEGIN:SUMMONS). "
        f"emitted {len(selected)}"
        + (f" (grep={args.grep!r})" if needle else "")
        + (f" (limit={args.limit})" if args.limit is not None else ""),
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
