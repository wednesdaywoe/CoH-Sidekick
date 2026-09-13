#!/usr/bin/env python3
"""
DSH9 bootstrap — minimal enhancement oracle coverage diff.

Compares Mids EnhDB oracle identity surfaces to current Sidekick extracted data:
  - IO set display names (EnhancementSet.display_name) vs io-sets-raw.ts names
  - Proc enhancement tuples (set display name + enhancement name where is_proc)
    vs proc-data.ts (setName + ioName)

This is deliberately a LIGHTWEIGHT coverage lens to validate the new EnhDB reader
and provide an actionable worklist. It is not yet the full value-level DSH9 gate.
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import re
import subprocess
import sys
from collections import Counter
from typing import Iterable

import read_enhdb
import read_i12

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# Both spelled once, in the readers that own them (PROV-3). The `I12.mhd` this named
# has not existed on this checkout since before the row was opened, so DSH9 died on it.
DEFAULT_DB = read_enhdb.DEFAULT_MHD
DEFAULT_I12 = read_i12.DEFAULT_MHD

# Known naming drift between oracle and extracted data. Keep canonicalized for
# matching and report the deltas separately as "name normalization" noise.
NAME_ALIASES = {
    "ascendency of the dominator": "ascendancy of the dominator",
    "superior ascendency of the dominator": "superior ascendancy of the dominator",
    "cacophany": "cacophony",
    "numina's convalesence": "numina's convalescence",
}

# Explicit proc io-name drift aliases for edge cases not covered by the
# one-to-one auto-normalization pass.
PROC_IO_EXPLICIT_ALIASES: dict[tuple[str, str], str] = {
    ("stupefy", "chance of knockback"): "chance for knockback",
}


def _norm(s: str) -> str:
    return re.sub(r"\s+", " ", (s or "").strip().lower())


def _canon_name(s: str) -> str:
    n = _norm(s)
    return NAME_ALIASES.get(n, n)


def _run_tsx_json(expr: str) -> object:
    """Run a tiny tsx expression and parse JSON from stdout.

    Falls back to ValueError so the caller can switch to text parsing.
    """
    cmd = ["npx", "tsx", "-e", expr]
    proc = subprocess.run(cmd, cwd=REPO, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise ValueError(f"tsx failed: {proc.stderr.strip()}")
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        raise ValueError(f"tsx JSON parse failed: {e}") from e


def _extract_repo_set_bonus_map_text(io_sets_raw_ts: str) -> dict[str, dict[int, dict[str, float]]]:
    """Best-effort text parser fallback for set bonuses.

    Returns: canonical set name -> pieces -> stat -> value
    """
    with open(io_sets_raw_ts, encoding="utf-8") as fh:
        txt = fh.read()

    out: dict[str, dict[int, dict[str, float]]] = {}
    # Conservative block matcher: each top-level set has "bonuses" first.
    pat = re.compile(r'"(?P<id>[^"]+)"\s*:\s*\{\s*"bonuses"\s*:\s*\[(?P<bon>.*?)\]\s*,', re.S)
    for m in pat.finditer(txt):
        bon = m.group("bon")
        # Name appears later in the same set body.
        tail = txt[m.end() : m.end() + 2000]
        n = re.search(r'"name"\s*:\s*"([^"]+)"', tail)
        if not n:
            continue
        set_name = _canon_name(n.group(1))
        pieces_map: dict[int, dict[str, float]] = {}
        for b in re.finditer(r'\{\s*"effects"\s*:\s*\[(?P<fx>.*?)\]\s*,\s*"pieces"\s*:\s*(?P<p>\d+)\s*\}', bon, re.S):
            p = int(b.group("p"))
            stat_map: dict[str, float] = {}
            for e in re.finditer(r'"stat"\s*:\s*"([^"]+)"\s*,\s*"value"\s*:\s*(-?\d+(?:\.\d+)?)', b.group("fx")):
                stat_map[_norm(e.group(1))] = float(e.group(2))
            if stat_map:
                pieces_map[p] = stat_map
        if pieces_map:
            out[set_name] = pieces_map
    return out


def _extract_repo_set_bonus_map(dataset: str) -> dict[str, dict[int, dict[str, float]]]:
    # Preferred path: import the TS module directly so we never drift from app data.
    try:
        data = _run_tsx_json(
            "import { IO_SETS_RAW } from './src/data/datasets/%s/io-sets-raw.ts';"
            "const out={};"
            "for (const v of Object.values(IO_SETS_RAW)) {"
            "  const m={};"
            "  for (const b of (v.bonuses||[])) {"
            "    const sm={};"
            "    for (const e of (b.effects||[])) sm[(e.stat||'').toLowerCase()] = Number(e.value||0);"
            "    m[Number(b.pieces)] = sm;"
            "  }"
            "  out[(v.name||'').toLowerCase()] = m;"
            "}"
            "console.log(JSON.stringify(out));" % dataset
        )
        out: dict[str, dict[int, dict[str, float]]] = {}
        for set_name, tiers in (data or {}).items():
            canon = _canon_name(str(set_name))
            out[canon] = {}
            for p, stat_map in (tiers or {}).items():
                out[canon][int(p)] = {str(k): float(v) for k, v in (stat_map or {}).items()}
        return out
    except Exception:
        # Fallback: text parser when tsx/npm isn't available.
        io_sets_ts = os.path.join(REPO, "src", "data", "datasets", dataset, "io-sets-raw.ts")
        return _extract_repo_set_bonus_map_text(io_sets_ts)


def _extract_proc_pairs(proc_data_ts: str) -> set[tuple[str, str]]:
    with open(proc_data_ts, encoding="utf-8") as fh:
        txt = fh.read()
    set_names = re.findall(r'(?:"setName"|setName)\s*:\s*"([^"]+)"', txt)
    io_names = re.findall(r'(?:"ioName"|ioName)\s*:\s*"([^"]+)"', txt)
    n = min(len(set_names), len(io_names))
    return {(_norm(set_names[i]), _norm(io_names[i])) for i in range(n)}


def _extract_repo_proc_map() -> dict[tuple[str, str], dict]:
    # Preferred path: import proc-data.ts directly so we get structured effects.
    try:
        data = _run_tsx_json(
            "import { PROC_DATABASE } from './src/data/proc-data.ts';"
            "const out={};"
            "for (const v of Object.values(PROC_DATABASE)) {"
            "  const key=[(v.setName||'').toLowerCase(), (v.ioName||'').toLowerCase()].join('::');"
            "  out[key]={"
            "    setName:(v.setName||'').toLowerCase(),"
            "    ioName:(v.ioName||'').toLowerCase(),"
            "    ppm:v.ppm===null?null:Number(v.ppm),"
            "    categories:(v.effects||[]).map(e => (e.category||'').toLowerCase())"
            "  };"
            "}"
            "console.log(JSON.stringify(out));"
        )
        out: dict[tuple[str, str], dict] = {}
        for rec in (data or {}).values():
            k = (_canon_name(str(rec.get("setName", ""))), _norm(str(rec.get("ioName", ""))))
            out[k] = {
                "ppm": rec.get("ppm"),
                "categories": set(rec.get("categories") or []),
            }
        return out
    except Exception:
        # Fallback to older regex pair extraction when tsx isn't available.
        proc_data_ts = os.path.join(REPO, "src", "data", "proc-data.ts")
        return {
            (_canon_name(s), _norm(io)): {"ppm": None, "categories": set()}
            for s, io in _extract_proc_pairs(proc_data_ts)
        }


# Mids spells slow/recharge-debuff resistance as one ResEffect record per
# debuffed attrib; the repo carries a single slot for the whole triple.
SLOW_RESIST_ATTRIBS = {"SpeedRunning", "SpeedFlying", "RechargeTime"}


def _damage_name(dmg: str) -> str:
    m = {
        "Smashing": "smashing",
        "Lethal": "lethal",
        "Fire": "fire",
        "Cold": "cold",
        "Energy": "energy",
        "Negative": "negative",
        "Toxic": "toxic",
        "Psionic": "psionic",
        "Melee": "melee",
        "Ranged": "ranged",
        "AoE": "aoe",
    }
    return m.get(dmg, "")


def _mez_name(mez: str) -> str:
    m = {
        "Held": "hold_duration",
        "Stunned": "stun_duration",
        "Immobilized": "immobilize_duration",
        "Sleep": "sleep_duration",
        "Terrorized": "terror_duration",
        "Confused": "confuse_duration",
    }
    return m.get(mez, "")


def _is_mez_family_member(mez_raw_key: str) -> bool:
    return mez_raw_key in {
        "_mez_resist_raw_Immobilized",
        "_mez_resist_raw_Held",
        "_mez_resist_raw_Stunned",
        "_mez_resist_raw_Sleep",
        "_mez_resist_raw_Terrorized",
        "_mez_resist_raw_Confused",
    }


def _bonus_multiplier(effect: dict) -> float:
    """Convert an oracle effect's raw `scale` to the display units io-sets-raw.ts uses.

    These are NOT Mids constants — they do not appear in the MidsReborn C# source.
    io-sets-raw values come from the Sidekick extractor, not Mids, so each factor is
    an *empirical* unit-conversion calibrated against known set bonuses so the oracle
    projection lands in the same units as the repo. Anchors (verified against local HC,
    all resolved by power name — see _build_oracle_set_bonus_map):
      - default 100.0: scale is a fraction of the character base; ×100 -> percent.
        Covers damage buffs: Gladiator's Javelin p4 scale=0.025 -> 2.5 = repo damage.
      - HitPoints/Max 10.0: max-HP is stored as tenths of a percent;
        Gladiator's Javelin p3 scale=0.1875 -> 1.875 = repo maximum_hitpoints.
      - Endurance/Max 1.0: absolute end points, scale is already in display units.

    NOTE: a prior DamageBuff/Str ×250 special-case was removed — it had been
    calibrated against index-shifted (wrong) powers and over-reported damage 2.5×.
    With name-based link resolution the default ×100 matches the repo exactly.
    """
    et = effect.get("effect_type", "")
    asp = effect.get("aspect", "")
    if et == "HitPoints" and asp == "Max":
        return 10.0
    if et == "Endurance" and asp == "Max":
        return 1.0
    return 100.0


def _pair_dedup(stats: dict[str, float]) -> dict[str, float]:
    pairs = [
        ("damage_resistance_(cold)", "damage_resistance_(fire)"),
        ("damage_resistance_(lethal)", "damage_resistance_(smashing)"),
        ("damage_resistance_(energy)", "damage_resistance_(negative)"),
        ("damage_resistance_(psionic)", "damage_resistance_(toxic)"),
        ("defense_(cold)", "defense_(fire)"),
        ("defense_(lethal)", "defense_(smashing)"),
        ("defense_(energy)", "defense_(negative)"),
    ]
    out = dict(stats)
    for keep, drop in pairs:
        if keep in out and drop in out:
            out.pop(drop, None)
    return out


def _oracle_effect_to_stat(effect: dict) -> str | None:
    et = effect.get("effect_type", "")
    dmg = effect.get("damage_type", "")
    mez = effect.get("mez_type", "")
    mod = effect.get("et_modifies", "")
    asp = effect.get("aspect", "")

    if et == "Resistance":
        d = _damage_name(dmg)
        return f"damage_resistance_({d})" if d else None
    if et == "Defense":
        d = _damage_name(dmg)
        return f"defense_({d})" if d else None
    if et == "Recovery":
        return "recovery"
    if et == "Regeneration":
        return "regeneration"
    if et == "RechargeTime":
        return "recharge"
    if et == "ToHit":
        return "tohit"
    if et == "PerceptionRadius":
        return "perception"
    if et == "DamageBuff":
        return "damage"
    if et == "MezResist":
        if mez == "Repel":
            # Not one of the six the family fold collapses; the repo carries it
            # as its own stat, so it must not enter the family machinery.
            return "repel_resistance"
        # Full-family collapse to all is done in post-processing.
        return f"_mez_resist_raw_{mez}"
    if et == "Enhancement":
        if mod == "Accuracy":
            return "accuracy"
        if mod == "RechargeTime":
            return "recharge"
        if mod == "ToHit":
            return "tohit"
        if mod == "Range":
            return "range"
        if mod == "EnduranceDiscount":
            return "endurance_discount"
        if mod == "Heal":
            return "healing_strength"
        if mod == "Mez":
            if mez in {"Knockback", "Knockup"}:
                return "knockback_strength"
            return _mez_name(mez)
        if mod in {"SpeedRunning", "SpeedFlying", "SpeedJumping", "JumpHeight"}:
            return "increased_movement"
        if mod == "HitPoints" and asp == "Max":
            return "maximum_hitpoints"
        if mod == "Endurance" and asp == "Max":
            return "maximum_endurance"
    if et == "HitPoints" and asp == "Max":
        return "maximum_hitpoints"
    if et == "Endurance" and asp == "Max":
        return "maximum_endurance"
    if et == "SpeedFlying" and asp in {"Cur", "Str"}:
        return "increased_movement"
    if et == "SpeedRunning" and asp in {"Cur", "Str"}:
        return "increased_movement"
    if et == "SpeedJumping" and asp in {"Cur", "Str"}:
        return "increased_movement"
    if et == "JumpHeight" and asp in {"Cur", "Str"}:
        return "increased_movement"
    if et == "Heal" and asp == "Abs":
        return "healing_strength"
    if et == "Mez" and asp == "Cur" and mez in {"Knockback", "Knockup"}:
        return "knockback_protection"
    if et == "ResEffect" and asp == "Res" and mod in SLOW_RESIST_ATTRIBS:
        return "+res(recharge_debuff)"
    if et in {"MovementFriction", "Slow"}:
        return "+res(recharge_debuff)" if asp == "Res" else None
    return None


def _oracle_proc_categories(enh: dict) -> set[str]:
    cats: set[str] = set()
    for eff in enh.get("effects", []):
        mode = eff.get("mode", "")
        fx = eff.get("fx")
        if mode == "FX" and isinstance(fx, dict):
            et = fx.get("effect_type", "")
            map_fx = {
                "Damage": "damage",
                "Heal": "heal",
                "Absorb": "absorb",
                "Resistance": "resistance",
                "Defense": "defense",
                "ToHit": "tohit",
                "Regeneration": "regeneration",
                "Recovery": "recovery",
                "RechargeTime": "recharge",
                "SpeedRunning": "runspeed",
                "HitPoints": "maxhp",
                "MezResist": "mezresist",
                "Mez": "control",
                "Slow": "debuff",
            }
            c = map_fx.get(et)
            if c:
                cats.add(c)
        elif mode == "Enhancement":
            eid = ((eff.get("enhance") or {}).get("id_name") or "")
            map_enh = {
                "Damage": "damage",
                "Heal": "heal",
                "Absorb": "absorb",
                "Resistance": "resistance",
                "Defense": "defense",
                "ToHit": "tohit",
                "Regeneration": "regeneration",
                "Recovery": "recovery",
                "RechargeTime": "recharge",
                "SpeedRunning": "runspeed",
            }
            c = map_enh.get(eid)
            if c:
                cats.add(c)
    return cats


def _build_oracle_set_bonus_map(
    sets: list[dict], powers: list[dict]
) -> tuple[dict[str, dict[int, dict[str, float]]], collections.Counter]:
    # Resolve set-bonus links by power *name*, NOT by the stored `index`. The
    # EnhDB caches a power index that is offset by a constant (-22 on the local HC
    # DB — verified across all 1,138 links) relative to the I12 power array this
    # reader builds, so index resolution silently reads the wrong power (e.g. an
    # "Increased_Damage_4" link resolving to a Defense power). Mids stores the
    # full_name on every link precisely so it survives such index drift; keying on
    # it is revision-proof where a +offset hack would not be.
    by_name: dict[str, int] = {}
    for i, pw in enumerate(powers):
        fn = pw.get("full_name")
        if fn and fn not in by_name:
            by_name[fn] = i

    def _resolve(link: dict) -> dict | None:
        i = by_name.get(link.get("name") or "")
        return powers[i] if i is not None else None

    out: dict[str, dict[int, dict[str, float]]] = {}
    unmapped: collections.Counter = collections.Counter()
    for s in sets:
        set_name = _canon_name(s.get("display_name", ""))
        tiers: dict[int, dict[str, float]] = {}
        for b in s.get("bonus", []):
            p = int(b.get("slotted", 0) or 0)
            if p <= 0:
                continue
            acc = tiers.setdefault(p, {})
            mez_seen: set[str] = set()
            for link in b.get("links", []):
                power = _resolve(link)
                if power is None:
                    continue
                for eff in power.get("effects", []):
                    stat = _oracle_effect_to_stat(eff)
                    if not stat:
                        # An effect row this comparator has no name for is a
                        # gap in the mapper, not an absence in the data.
                        unmapped[
                            (
                                eff.get("effect_type", ""),
                                eff.get("aspect", ""),
                                eff.get("et_modifies", ""),
                                eff.get("mez_type", ""),
                            )
                        ] += 1
                        continue
                    if stat.startswith("_mez_resist_raw_"):
                        mez_seen.add(stat)
                        continue
                    value = round(abs(float(eff.get("scale", 0.0))) * _bonus_multiplier(eff), 4)
                    if stat not in acc:
                        acc[stat] = value
            # Collapse full mez-resist family to all.
            if len({k for k in mez_seen if _is_mez_family_member(k)}) >= 6:
                # value from first linked mez effect of this tier.
                for link in b.get("links", []):
                    power = _resolve(link)
                    if power is None:
                        continue
                    for eff in power.get("effects", []):
                        eff_stat = _oracle_effect_to_stat(eff)
                        if eff_stat and eff_stat.startswith("_mez_resist_raw_"):
                            acc["mez_resistance_(all)"] = round(abs(float(eff.get("scale", 0.0))) * _bonus_multiplier(eff), 4)
                            break
                    if "mez_resistance_(all)" in acc:
                        break

            # Collapse full typed families to all.
            dmg_res_members = {
                "damage_resistance_(smashing)",
                "damage_resistance_(lethal)",
                "damage_resistance_(fire)",
                "damage_resistance_(cold)",
                "damage_resistance_(energy)",
                "damage_resistance_(negative)",
                "damage_resistance_(psionic)",
                "damage_resistance_(toxic)",
            }
            if dmg_res_members.issubset(set(acc.keys())):
                acc["damage_resistance_(all)"] = acc["damage_resistance_(smashing)"]
                for k in dmg_res_members:
                    acc.pop(k, None)

            tiers[p] = _pair_dedup(acc)

        out[set_name] = tiers
    return out, unmapped


def _stat_vocabularies(
    oracle_map: dict[str, dict[int, dict[str, float]]],
    repo_map: dict[str, dict[int, dict[str, float]]],
) -> tuple[list[str], list[str]]:
    """Stat names one side can emit and the other has no slot for.

    Both sides name the same 33 stats, and nothing compared the two lists. A
    repo-side rename (MEZRES-1 spelled `defense_(area)` -> `defense_(aoe)`) or
    an unmapped oracle effect type therefore did not fail -- it split one
    matching stat into a `missing` line and an `extra` line and sat in the
    baseline looking like a finding. A name with no counterpart is a defect in
    this comparator, not a divergence between the two databases.
    """

    def stats(m: dict[str, dict[int, dict[str, float]]]) -> set[str]:
        return {k for tiers in m.values() for row in tiers.values() for k in row}

    o, r = stats(oracle_map), stats(repo_map)
    return sorted(o - r), sorted(r - o)


def _head(items: Iterable[str], n: int) -> list[str]:
    out = sorted(items)
    return out[:n]


def _classify_extra_proc_pair(
    set_name: str,
    io_name: str,
    rec: dict,
    oracle_set_names: set[str],
    oracle_proc_names_by_set: dict[str, set[str]],
) -> tuple[str, str]:
    """Classify repo-only proc entries into likely global/passive vs mapping gap.

    This is intentionally heuristic triage to prioritize follow-up work.
    """
    name = _norm(io_name)
    cats = set(rec.get("categories") or [])
    proc_like_name = ("chance" in name) or ("convert " in name) or (" recharge/" in name)
    ppm = rec.get("ppm")

    # Conversion slot effects are special mechanics (not chance procs), so
    # classify them with passive/special coverage, not mapping-gap proc misses.
    if name.startswith("convert "):
        return (
            "likely_non_proc_global_or_passive",
            "special conversion mechanic (non-chance proc coverage)",
        )

    # If the set doesn't exist in the local oracle at all, a proc-looking extra is
    # more likely oracle staleness/scope than an extractor mapping defect.
    if set_name not in oracle_set_names and proc_like_name:
        return (
            "likely_oracle_set_staleness",
            "proc-looking entry but set is absent in local oracle",
        )

    # Set exists in oracle and already has proc rows, but this extra proc-like
    # entry has no oracle counterpart. Treat as likely per-set oracle staleness
    # rather than extractor mapping gap.
    oracle_ios = oracle_proc_names_by_set.get(set_name, set())
    if proc_like_name and oracle_ios and name not in oracle_ios and ppm is not None:
        return (
            "likely_oracle_proc_staleness",
            "set exists in oracle, but this proc row has no oracle counterpart",
        )

    if proc_like_name:
        return ("likely_mapping_gap", "name indicates triggered/proc behavior")

    global_hints = [
        "+def",
        "+res",
        "+stealth",
        "buff ",
        " bonus",
        "protection",
        "resistance",
        "max hp",
        "pet ",
        "resolve",
    ]
    if any(h in name for h in global_hints):
        return ("likely_non_proc_global_or_passive", "name indicates passive/global bonus")

    if not cats:
        return ("likely_non_proc_global_or_passive", "no structured effect categories")

    if cats & {"damage", "heal", "absorb", "control", "debuff", "recharge", "tohit"}:
        return ("likely_mapping_gap", "structured categories look proc-like")

    return ("unknown", "insufficient signal")


def _signatures_from_values(value_details: dict[str, list[dict]]) -> dict[str, list[str]]:
    sig_missing = []
    sig_extra = []
    sig_mismatch = []
    for row in value_details.get("missing", []):
        sig_missing.append(f"{row['set']}|{row['pieces']}|{row['stat']}")
    for row in value_details.get("extra", []):
        sig_extra.append(f"{row['set']}|{row['pieces']}|{row['stat']}")
    for row in value_details.get("mismatch", []):
        sig_mismatch.append(
            f"{row['set']}|{row['pieces']}|{row['stat']}|{row['oracle']:.4f}|{row['repo']:.4f}"
        )
    return {
        "value_missing": sorted(set(sig_missing)),
        "value_extra": sorted(set(sig_extra)),
        "value_mismatch": sorted(set(sig_mismatch)),
    }


def _load_baseline(path: str) -> dict[str, list[str]]:
    with open(path, encoding="utf-8") as fh:
        data = json.load(fh)
    out: dict[str, list[str]] = {}
    for k, v in data.items():
        if isinstance(v, list):
            out[k] = [str(x) for x in v]
    return out


def _new_vs_baseline(current: dict[str, list[str]], baseline: dict[str, list[str]]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for k, cur in current.items():
        base = set(baseline.get(k, []))
        out[k] = sorted([x for x in cur if x not in base])
    return out


def _auto_normalize_proc_pairs(
    oracle_proc_map: dict[tuple[str, str], dict],
    repo_proc_map: dict[tuple[str, str], dict],
) -> tuple[dict[tuple[str, str], dict], dict[tuple[str, str], str]]:
    """Normalize proc pair name drift using one-to-one per-set mapping.

    If a set has exactly one missing oracle proc name and one extra repo proc name,
    treat it as a naming drift and remap the oracle key to the repo key.
    """
    oracle_pairs = set(oracle_proc_map.keys())
    repo_pairs = set(repo_proc_map.keys())
    missing = oracle_pairs - repo_pairs
    extra = repo_pairs - oracle_pairs

    missing_by_set: dict[str, list[str]] = {}
    extra_by_set: dict[str, list[str]] = {}
    for set_name, io_name in missing:
        missing_by_set.setdefault(set_name, []).append(io_name)
    for set_name, io_name in extra:
        extra_by_set.setdefault(set_name, []).append(io_name)

    aliases: dict[tuple[str, str], str] = {}
    for set_name in sorted(set(missing_by_set) & set(extra_by_set)):
        m = sorted(missing_by_set[set_name])
        e = sorted(extra_by_set[set_name])
        if len(m) == 1 and len(e) == 1:
            aliases[(set_name, m[0])] = e[0]

    normalized_oracle: dict[tuple[str, str], dict] = {}
    for (set_name, io_name), rec in oracle_proc_map.items():
        mapped_io = aliases.get((set_name, io_name), io_name)
        key = (set_name, mapped_io)
        prev = normalized_oracle.get(key)
        if prev is None:
            normalized_oracle[key] = {
                "ppm": rec.get("ppm"),
                "categories": set(rec.get("categories") or []),
            }
            continue
        prev_cats = set(prev.get("categories") or [])
        prev_cats.update(set(rec.get("categories") or []))
        prev["categories"] = prev_cats
        if prev.get("ppm") is None and rec.get("ppm") is not None:
            prev["ppm"] = rec.get("ppm")

    return normalized_oracle, aliases


def _apply_explicit_proc_aliases(
    oracle_proc_map: dict[tuple[str, str], dict],
    explicit_aliases: dict[tuple[str, str], str],
) -> tuple[dict[tuple[str, str], dict], dict[tuple[str, str], str]]:
    mapped: dict[tuple[str, str], str] = {}
    out: dict[tuple[str, str], dict] = {}
    for (set_name, io_name), rec in oracle_proc_map.items():
        mapped_io = explicit_aliases.get((set_name, io_name), io_name)
        if mapped_io != io_name:
            mapped[(set_name, io_name)] = mapped_io
        key = (set_name, mapped_io)
        prev = out.get(key)
        if prev is None:
            out[key] = {
                "ppm": rec.get("ppm"),
                "categories": set(rec.get("categories") or []),
            }
            continue
        prev_cats = set(prev.get("categories") or [])
        prev_cats.update(set(rec.get("categories") or []))
        prev["categories"] = prev_cats
        if prev.get("ppm") is None and rec.get("ppm") is not None:
            prev["ppm"] = rec.get("ppm")
    return out, mapped


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Minimal EnhDB oracle coverage diff (DSH9 bootstrap)")
    ap.add_argument("--mhd", default=DEFAULT_DB, help="path to EnhDB.mhd")
    ap.add_argument("--i12", default=DEFAULT_I12, help="path to I12.mhd (for value-aware set-bonus projection)")
    ap.add_argument("--dataset", choices=["homecoming", "rebirth", "thunderspy"], default="homecoming")
    ap.add_argument("--show", type=int, default=20, help="max examples to print per bucket")
    ap.add_argument("--strict", action="store_true", help="exit non-zero if missing coverage exists")
    ap.add_argument("--value-diff", action="store_true", help="enable value-aware set-bonus and proc-category comparisons")
    ap.add_argument("--triage-top", type=int, default=0, help="show top-N value mismatches by absolute delta")
    ap.add_argument("--triage-json", default=None, help="write triage summary JSON")
    ap.add_argument("--baseline", default=None, help="path to residual-signature baseline JSON")
    ap.add_argument("--baseline-out", default=None, help="write current residual signatures JSON")
    args = ap.parse_args(argv)

    mhd = os.path.abspath(args.mhd)
    if not os.path.isfile(mhd):
        print(f"error: no such EnhDB.mhd: {mhd}", file=sys.stderr)
        return 2

    io_sets_ts = os.path.join(REPO, "src", "data", "datasets", args.dataset, "io-sets-raw.ts")
    proc_data_ts = os.path.join(REPO, "src", "data", "proc-data.ts")
    if not os.path.isfile(io_sets_ts):
        print(f"error: no such io-sets file: {io_sets_ts}", file=sys.stderr)
        return 2
    if not os.path.isfile(proc_data_ts):
        print(f"error: no such proc-data file: {proc_data_ts}", file=sys.stderr)
        return 2

    enhancements, sets, version, _ = read_enhdb.read_enhdb(open(mhd, "rb").read())

    oracle_set_names = {_canon_name(s["display_name"]) for s in sets if s.get("display_name")}
    repo_bonus_map = _extract_repo_set_bonus_map(args.dataset)
    repo_set_names = set(repo_bonus_map.keys())

    missing_sets = oracle_set_names - repo_set_names
    extra_sets = repo_set_names - oracle_set_names

    oracle_proc_pairs = set()
    oracle_proc_map: dict[tuple[str, str], dict] = {}
    for enh in enhancements:
        if not enh.get("is_proc"):
            continue
        set_index = enh.get("set_index", -1)
        set_name = ""
        if isinstance(set_index, int) and 0 <= set_index < len(sets):
            set_name = sets[set_index].get("display_name", "")
        key = (_canon_name(set_name), _norm(enh.get("name", "")))
        oracle_proc_pairs.add(key)
        oracle_proc_map[key] = {
            "ppm": None,
            "categories": _oracle_proc_categories(enh),
        }

    repo_proc_map = _extract_repo_proc_map()
    # proc-data.ts is one file for all three forks, but the oracle DB is one
    # fork's. A proc whose SET is absent from this dataset's io-sets belongs to
    # another fork and can only ever read as `extra` here.
    off_fork_procs = sorted({k for k in repo_proc_map if k[0] not in repo_set_names})
    for k in off_fork_procs:
        repo_proc_map.pop(k, None)
    raw_oracle_proc_map = dict(oracle_proc_map)
    raw_oracle_proc_pairs = set(raw_oracle_proc_map.keys())
    oracle_proc_map, proc_aliases = _auto_normalize_proc_pairs(oracle_proc_map, repo_proc_map)
    oracle_proc_map, explicit_proc_aliases = _apply_explicit_proc_aliases(
        oracle_proc_map,
        PROC_IO_EXPLICIT_ALIASES,
    )
    oracle_proc_pairs = set(oracle_proc_map.keys())
    repo_proc_pairs = set(repo_proc_map.keys())

    missing_proc = oracle_proc_pairs - repo_proc_pairs
    extra_proc = repo_proc_pairs - oracle_proc_pairs

    value_mismatch_count = 0
    value_missing_stats = 0
    value_extra_stats = 0
    proc_category_mismatch = 0
    value_details: dict[str, list[dict]] = {"missing": [], "extra": [], "mismatch": []}
    proc_category_details: list[dict] = []
    extra_proc_classification: list[dict] = []

    if args.value_diff:
        i12 = os.path.abspath(args.i12)
        if not os.path.isfile(i12):
            print(f"error: no such I12.mhd: {i12}", file=sys.stderr)
            return 2
        i12_buf = open(i12, "rb").read()
        powers, _ = read_i12.read_powers(i12_buf)
        i12_provenance = read_i12.provenance(i12, i12_buf, len(powers))
        oracle_bonus_map, unmapped_effects = _build_oracle_set_bonus_map(sets, powers)
        vocab_oracle_only, vocab_repo_only = _stat_vocabularies(
            oracle_bonus_map, repo_bonus_map
        )

        for set_name in sorted(oracle_set_names & repo_set_names):
            o_tiers = oracle_bonus_map.get(set_name, {})
            r_tiers = repo_bonus_map.get(set_name, {})
            for p in sorted(set(o_tiers.keys()) & set(r_tiers.keys())):
                o_stats = o_tiers.get(p, {})
                r_stats = r_tiers.get(p, {})
                miss = set(o_stats.keys()) - set(r_stats.keys())
                extra = set(r_stats.keys()) - set(o_stats.keys())
                value_missing_stats += len(miss)
                value_extra_stats += len(extra)
                for stat in miss:
                    value_details["missing"].append({"set": set_name, "pieces": p, "stat": stat})
                for stat in extra:
                    value_details["extra"].append({"set": set_name, "pieces": p, "stat": stat})
                for stat in set(o_stats.keys()) & set(r_stats.keys()):
                    delta = abs(float(o_stats[stat]) - float(r_stats[stat]))
                    if delta > 0.02:
                        value_mismatch_count += 1
                        value_details["mismatch"].append(
                            {
                                "set": set_name,
                                "pieces": p,
                                "stat": stat,
                                "oracle": float(o_stats[stat]),
                                "repo": float(r_stats[stat]),
                                "delta": float(delta),
                            }
                        )

        # Category comparison stays on exact-name intersections so proc alias
        # normalization improves identity coverage without creating category noise.
        for key in sorted(raw_oracle_proc_pairs & repo_proc_pairs):
            oc = set(raw_oracle_proc_map.get(key, {}).get("categories") or [])
            rc = set(repo_proc_map.get(key, {}).get("categories") or [])
            if rc and oc != rc:
                proc_category_mismatch += 1
                proc_category_details.append(
                    {
                        "set": key[0],
                        "io": key[1],
                        "oracle": sorted(oc),
                        "repo": sorted(rc),
                    }
                )

    residual_signatures = {
        "missing_sets": sorted(f"{s}" for s in missing_sets),
        "extra_sets": sorted(f"{s}" for s in extra_sets),
        "missing_proc": sorted(f"{s}|{io}" for s, io in missing_proc),
        "extra_proc": sorted(f"{s}|{io}" for s, io in extra_proc),
        "proc_category_mismatch": sorted(
            f"{d['set']}|{d['io']}|{','.join(d['oracle'])}|{','.join(d['repo'])}" for d in proc_category_details
        ),
    }
    if args.value_diff:
        residual_signatures.update(_signatures_from_values(value_details))

    print(
        f"=== DSH9 bootstrap diff (dataset={args.dataset}, enhdb_version={version:.6g}) ===\n"
        f"oracle sets={len(oracle_set_names)} repo sets={len(repo_set_names)}\n"
        f"missing set names={len(missing_sets)} extra set names={len(extra_sets)}\n"
        f"oracle proc pairs={len(oracle_proc_pairs)} repo proc pairs={len(repo_proc_pairs)}\n"
        f"missing proc pairs={len(missing_proc)} extra proc pairs={len(extra_proc)}\n"
        f"proc aliases auto-normalized={len(proc_aliases)} explicit={len(explicit_proc_aliases)}\n"
        f"off-fork procs excluded={len(off_fork_procs)} (set absent from this dataset's io-sets)"
    )

    if args.value_diff:
        print(
            f"\nstat vocabulary: oracle-only={vocab_oracle_only or '[]'} "
            f"repo-only={vocab_repo_only or '[]'}\n"
            f"unmapped oracle effect rows={sum(unmapped_effects.values())} "
            f"in {len(unmapped_effects)} shapes"
        )
        for shape, n in unmapped_effects.most_common():
            print(f"  UNMAPPED {n:5d}  et={shape[0]} aspect={shape[1]} "
                  f"modifies={shape[2]} mez={shape[3]}")
        print(
            f"value-aware set-bonus residuals: missing_stats={value_missing_stats}, "
            f"extra_stats={value_extra_stats}, value_mismatches={value_mismatch_count}"
        )
        print(f"proc category mismatches (where repo has structured categories): {proc_category_mismatch}")

    if missing_sets:
        print("\n-- missing set names (oracle -> repo) --")
        for s in _head(missing_sets, args.show):
            print(f"  {s}")

    if extra_sets:
        print("\n-- extra set names (repo not in oracle) --")
        for s in _head(extra_sets, args.show):
            print(f"  {s}")

    if missing_proc:
        print("\n-- missing proc pairs (oracle -> repo) --")
        for s, io in sorted(missing_proc)[: args.show]:
            print(f"  {s} :: {io}")

    if extra_proc:
        print("\n-- extra proc pairs (repo not in oracle) --")
        for s, io in sorted(extra_proc)[: args.show]:
            print(f"  {s} :: {io}")

    oracle_proc_names_by_set: dict[str, set[str]] = {}
    for set_name, io_name in oracle_proc_pairs:
        oracle_proc_names_by_set.setdefault(set_name, set()).add(io_name)

    for s, io in sorted(extra_proc):
        rec = repo_proc_map.get((s, io), {})
        bucket, reason = _classify_extra_proc_pair(
            s,
            io,
            rec,
            oracle_set_names,
            oracle_proc_names_by_set,
        )
        extra_proc_classification.append(
            {
                "set": s,
                "io": io,
                "bucket": bucket,
                "reason": reason,
                "categories": sorted(set(rec.get("categories") or [])),
                "ppm": rec.get("ppm"),
            }
        )

    if extra_proc_classification:
        bucket_counts = Counter(row["bucket"] for row in extra_proc_classification)
        print(
            "\nextra proc triage buckets: "
            + ", ".join(f"{k}={v}" for k, v in sorted(bucket_counts.items()))
        )

        likely_gap_rows = [r for r in extra_proc_classification if r["bucket"] == "likely_mapping_gap"]
        if likely_gap_rows:
            print("\n-- likely mapping-gap extra procs --")
            for r in likely_gap_rows[: args.show]:
                print(f"  {r['set']} :: {r['io']} ({r['reason']})")

    if args.value_diff and args.triage_top > 0:
        top = sorted(value_details["mismatch"], key=lambda r: (-r["delta"], r["set"], r["pieces"], r["stat"]))[: args.triage_top]
        if top:
            print("\n-- top value mismatches (abs delta) --")
            for r in top:
                print(
                    f"  {r['set']} [pieces={r['pieces']}] {r['stat']}: "
                    f"oracle={r['oracle']:.4f} repo={r['repo']:.4f} delta={r['delta']:.4f}"
                )

        miss_counter = Counter(row["stat"] for row in value_details["missing"])
        extra_counter = Counter(row["stat"] for row in value_details["extra"])
        if miss_counter:
            print("\n-- top missing stats (oracle -> repo) --")
            for stat, n in miss_counter.most_common(args.triage_top):
                print(f"  {n:4d} {stat}")
        if extra_counter:
            print("\n-- top extra stats (repo -> oracle) --")
            for stat, n in extra_counter.most_common(args.triage_top):
                print(f"  {n:4d} {stat}")

    new_residuals: dict[str, list[str]] | None = None
    if args.baseline:
        baseline_path = os.path.abspath(args.baseline)
        if not os.path.isfile(baseline_path):
            print(f"error: baseline file not found: {baseline_path}", file=sys.stderr)
            return 2
        baseline = _load_baseline(baseline_path)
        new_residuals = _new_vs_baseline(residual_signatures, baseline)
        new_total = sum(len(v) for v in new_residuals.values())
        print(f"\nbaseline comparison: new signatures vs baseline = {new_total}")
        if new_total:
            for key in sorted(new_residuals):
                if new_residuals[key]:
                    print(f"  {key}: +{len(new_residuals[key])}")
                    for sig in new_residuals[key][: args.show]:
                        print(f"    {sig}")

    if args.baseline_out:
        out_path = os.path.abspath(args.baseline_out)
        # PROV-5: this file graded two Mids databases and named neither. It is the
        # third artefact of the shape PROV-3 found, and it goes through the same
        # primitive the other two now do.
        payload: dict = {
            "schema": "dsh9-enh-oracle-residual-baseline/1",
            "dataset": args.dataset,
            "oracle": {
                "enhdb": read_i12.provenance(
                    mhd, open(mhd, "rb").read(), len(enhancements), "enhancementCount"
                ),
                "i12": i12_provenance if args.value_diff else None,
            },
        }
        payload.update(residual_signatures)
        with open(out_path, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, indent=2, sort_keys=True)
            fh.write("\n")
        print(f"\nwrote residual-signature baseline: {out_path}")

    if args.triage_json:
        triage_path = os.path.abspath(args.triage_json)
        bucket_counts = Counter(row["bucket"] for row in extra_proc_classification)
        triage_payload: dict[str, object] = {
            "dataset": args.dataset,
            "enhdb_version": version,
            "summary": {
                "missing_sets": len(missing_sets),
                "extra_sets": len(extra_sets),
                "missing_proc": len(missing_proc),
                "extra_proc": len(extra_proc),
                "proc_aliases": len(proc_aliases),
                "explicit_proc_aliases": len(explicit_proc_aliases),
                "value_missing_stats": value_missing_stats,
                "value_extra_stats": value_extra_stats,
                "value_mismatches": value_mismatch_count,
                "proc_category_mismatches": proc_category_mismatch,
            },
            "residual_signatures": residual_signatures,
            "extra_proc_classification": {
                "summary": {k: bucket_counts[k] for k in sorted(bucket_counts)},
                "rows": extra_proc_classification,
            },
            "top": {
                "value_mismatch": sorted(
                    value_details["mismatch"],
                    key=lambda r: (-r["delta"], r["set"], r["pieces"], r["stat"]),
                )[: (args.triage_top or 25)],
                "missing_stats": Counter(row["stat"] for row in value_details["missing"]).most_common(
                    args.triage_top or 25
                ),
                "extra_stats": Counter(row["stat"] for row in value_details["extra"]).most_common(
                    args.triage_top or 25
                ),
            },
            "proc_alias_map": {
                f"{set_name}|{io_name}": mapped
                for (set_name, io_name), mapped in sorted(proc_aliases.items())
            },
            "proc_alias_map_explicit": {
                f"{set_name}|{io_name}": mapped
                for (set_name, io_name), mapped in sorted(explicit_proc_aliases.items())
            },
        }
        with open(triage_path, "w", encoding="utf-8") as fh:
            json.dump(triage_payload, fh, indent=2, sort_keys=True)
            fh.write("\n")
        print(f"\nwrote triage JSON: {triage_path}")

    should_fail = False
    if args.strict:
        # A stat name with no counterpart is a defect in this comparator, not a
        # residual, so a baseline must not forgive it -- that is how the
        # `defense_(area)` rename sat for five weeks looking like 115 findings.
        if args.value_diff and (vocab_oracle_only or vocab_repo_only):
            print(
                "STAT-VOCABULARY: the two sides name different stats "
                f"(oracle-only={vocab_oracle_only}, repo-only={vocab_repo_only})",
                file=sys.stderr,
            )
            should_fail = True
        if args.value_diff and unmapped_effects:
            print(
                f"UNMAPPED-EFFECTS: {sum(unmapped_effects.values())} oracle effect rows "
                f"in {len(unmapped_effects)} shapes have no stat name",
                file=sys.stderr,
            )
            should_fail = True
        if args.baseline:
            assert new_residuals is not None
            should_fail = should_fail or any(new_residuals.values())
        else:
            should_fail = should_fail or bool(
                missing_sets
                or missing_proc
                or (args.value_diff and (value_missing_stats or value_extra_stats or value_mismatch_count or proc_category_mismatch))
            )
    if should_fail:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
