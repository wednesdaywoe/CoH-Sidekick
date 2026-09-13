#!/usr/bin/env python3
"""
DSH5 — oracle-backed differential harness (streams/DEDUCTIVE_SCHEMA_HARNESS.md).

The production successor to the DSH1 PoC (`diff_oracle.py`). Where the PoC diffed a
14-power cohort on a bridge-free table proxy, DSH5:

  1. sweeps the WHOLE HC set — joins every Mids power (oracle, via `read_i12`) to our
     parser export by `full_name`;
  2. keys effects by the **DSH4 canonical identity** (real `(effectType, subType,
     pvMode, resistible)` via the tested `atomic-effect.ts` bridge — the export side
     is canonicalized once by `emit_canonical.ts`, never re-ported into Python), with
     Mids' `Enhancement`/`et_modifies` pair re-spelled into that vocabulary (PROV-4);
  3. checks the structural invariants (set/count equality = the collapse catcher;
     multi-type completeness; PvE/PvP twin integrity; resistibility present+correct;
     table-name + aspect/attribType agreement as advisory tiers);
  4. runs a **tiered classifier** and writes typed rules to
     `oracle_divergence_rules.json` (STRUCTURAL/UNCLASSIFIED = triage-blocks ·
     NUMERIC_DRIFT = skew · BY_DESIGN/RELABEL/COSMETIC = suppressed);
  5. emits a coverage manifest (matched / oracle-only / export-only / redirect).

Trust boundary (plan doc): **Mids for topology, bins for names and numbers.** So the
GATING signal is deliberately narrow — the known-answer cohort must match exactly,
and no *new* UNCLASSIFIED divergence may appear beyond the committed baseline.
Table-name / numeric divergence against a ~5-week-stale, typo-carrying oracle is
advisory only (never fails) — anything else would be a false positive, the one thing
these guards must never produce.

Local only: `MidsReborn-master/` + `I12.mhd` are gitignored, so this does not run in
CI yet (that is DSH7). Python 3 stdlib + one `npx tsx` call for the export side.

Usage:
    python3 diff_harness.py                       # sweep, write rules, cohort gate
    python3 diff_harness.py --baseline oracle_divergence_rules.json   # regression gate
    python3 diff_harness.py --emit                # force-refresh the canonical export
    python3 diff_harness.py --top 30              # show the 30 largest residual classes
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from collections import Counter, defaultdict

import read_i12

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
CACHE_DIR = os.path.join(HERE, ".oracle_cache")
DEFAULT_CANONICAL = os.path.join(CACHE_DIR, "canonical_export.jsonl")
DEFAULT_RULES = os.path.join(HERE, "oracle_divergence_rules.json")

# --- Mids eEffectType -> our EffectType space (the ONLY Mids-vocab crosswalk on the
#     Python side; the attrib->type BRIDGE stays single-source in atomic-effect.ts). --
MIDS_ET = {
    "Damage": "Damage", "DamageBuff": "DamageBuff", "Enhancement": "Enhancement",
    "Heal": "Heal", "Absorb": "Absorb", "HitPoints": "MaxHP",
    "Defense": "Defense", "Resistance": "Resistance", "Elusivity": "Elusivity",
    "ToHit": "ToHit", "Accuracy": "Accuracy", "Mez": "Mez", "MezResist": "MezResist",
    "Endurance": "Endurance", "EnduranceDiscount": "EnduranceDiscount",
    "Recovery": "Recovery", "Regeneration": "Regeneration", "RechargeTime": "RechargeTime",
    "Range": "Range", "ThreatLevel": "ThreatLevel", "PerceptionRadius": "Perception",
    "StealthRadius": "Stealth", "StealthRadiusPlayer": "Stealth",
    "SpeedRunning": "Movement", "SpeedFlying": "Movement", "SpeedJumping": "Movement",
    "JumpHeight": "Movement", "Fly": "Movement", "MovementControl": "Movement",
    "MovementFriction": "Movement", "GrantPower": "GrantPower", "EntCreate": "EntCreate",
    "ExecutePower": "ExecutePower", "RechargePower": "RechargePower",
    "GlobalChanceMod": "GlobalChanceMod",
}

# effectTypes with a clean 1:1 Mids counterpart — the gating comparison set. Types
# outside this set (Meta/engine markers, the 79%-relabeled type/app_type edge cases)
# are counted in coverage but never gated, to stay false-positive-free.
COMPARABLE = {
    "Damage", "DamageBuff", "Enhancement", "Mez", "MezResist", "Defense", "Resistance",
    "ToHit", "Recovery", "Regeneration", "RechargeTime", "Endurance", "Heal", "Absorb",
    "Elusivity", "Movement", "Accuracy", "Range", "Perception",
    # Mids' catch-all for "resistance to a secondary-attribute debuff" (Acid Arrow's
    # -regen/-recovery/-endurance/etc.). Mids collapses the whole family to ResEffect
    # with a placeholder damage_type; our bridge preserves the affected attrib at
    # aspect=Res (the documented DSH4/DSH6 boundary). We canonicalize BOTH sides to
    # this bucket so the family compares by count instead of flooding UNCLASSIFIED.
    "ResEffect",
}

# effectTypes that carry their OWN aspect=Res semantics in Mids (Resistance/MezResist
# are distinct Mids types, not ResEffect) — excluded from the ResEffect fold below.
_RES_NATIVE = {"Defense", "Resistance", "Elusivity", "Mez", "MezResist"}

# The 2026-07-05 resistible/unresistable-twin collapse cohort — must match EXACTLY
# (DSH1 proved this). These are the harness's teeth.
TWIN_GATE = {
    "controller_buff.trick_arrow.flash_arrow",
    "controller_buff.trick_arrow.poison_gas_arrow",
}
# Broader known-answer cohort — must have zero UNCLASSIFIED residual (modeling
# classes allowed). A regression in the converter/parser breaks one of these.
COHORT = TWIN_GATE | {
    "arachnos_soldiers.arachnos_soldier.single_shot",
    "arachnos_soldiers.arachnos_soldier.pummel",
    "controller_buff.trick_arrow.acid_arrow",
    "blaster_support.energy_manipulation.build_up",
    "scrapper_melee.broad_sword.build_up",
}


def norm_sub(s):
    """Fold All/None/empty subtypes to '' (a naming artifact between the two models)."""
    return "" if s in (None, "", "All", "None") else s


def norm_table(t):
    """Normalize a modifier-table name for advisory agreement: lowercase, drop the
    pvp/pve context token and any trailing _level/level-range suffix. Preserves the
    base-family signal (Ranged_ vs Melee_, Damage vs Debuff) that table SELECTION
    turns on, while absorbing Mids' string skew."""
    t = (t or "").lower()
    for tok in ("_pvp", "pvp", "_pve", "pve"):
        t = t.replace(tok, "")
    parts = [p for p in t.replace("__", "_").split("_") if p and not p.isdigit()]
    return "_".join(parts)


# The complete standard damage / positional type-sets. When an effect covers the
# WHOLE set, Mids collapses it to a single `damage_type=None` ("all") record, while
# our bin export lists every type explicitly (and the DSH4 bridge faithfully splits
# them per-type). To compare like-for-like, the harness folds a complete set → one
# 'All' record on BOTH sides (Mids' convention). The resistible/unresistable twin is
# folded independently, so it stays distinct — the whole point of the twin gate.
COMPLETE_DAMAGE = {"Smashing", "Lethal", "Fire", "Cold", "Energy", "Negative", "Toxic", "Psionic"}
COMPLETE_POSITION = {"Melee", "Ranged", "AoE"}


def oracle_subtype(et, dmg, mez):
    if et in ("Damage", "DamageBuff", "Defense", "Resistance", "Elusivity"):
        return dmg
    if et in ("Mez", "MezResist"):
        return mez
    return None


# --- PROV-4: Mids' Enhancement encoding -> the attribute it enhances ----------
# Mids spells a strength buff as `EffectType.Enhancement` and names WHICH attribute
# in a second field, `et_modifies`. Our export spells the attribute itself, because
# the bridge reads aspect=Str off the attrib (atomic-effect.ts) — so `Enhancement`
# survives on our side only for the two families that have no effectType of their
# own: mez strength (`Enhancement|Held`) and defense strength (`Enhancement|Melee`,
# `Base_Defense` -> `Enhancement|All`). Everything else keeps its attribute's type
# at Str: ToHit, Heal, Absorb, Endurance, Movement, RechargeTime, ...
#
# Unfolded, `et_modifies` was simply dropped and every Enhancement row in a power
# collapsed onto one key — Power Boost's SpeedRunning, SpeedFlying and Defense all
# read `Enhancement|`. So the fold reads et_modifies and re-spells the record in the
# export's vocabulary; the two cases where our side also says `Enhancement` are the
# two that need their subType steered off damage_type/mez_type instead.
_ENH_KEEPS_ENHANCEMENT = {
    "Mez": "mez_type",          # Enhancement|Held, Enhancement|Sleep, ...
    "Defense": "damage_type",   # Enhancement|Melee, Enhancement|Smashing, |All
}
# et_modifies=Damage is the one name MIDS_ET would round-trip wrong: enhancing the
# Damage attribute is a damage BUFF, which the bridge writes `<type>_Dmg`@Str ->
# DamageBuff — not `Damage`, the dealt-damage face.
_ENH_ET_OVERRIDE = {"Damage": "DamageBuff"}


def enhanced_attrib(e):
    """(effectType, subType) an oracle `Enhancement` row enhances, in export vocab.
    Returns (None, reason) for an et_modifies with no counterpart in MIDS_ET — the
    caller counts those into the rules artifact rather than dropping them silently."""
    etm = e["et_modifies"]
    vector = _ENH_KEEPS_ENHANCEMENT.get(etm)
    if vector:
        return "Enhancement", norm_sub(e[vector])
    et = _ENH_ET_OVERRIDE.get(etm) or MIDS_ET.get(etm)
    if not et:
        return None, f"et_modifies={etm}"
    if et == "Movement":
        return et, ""
    return et, norm_sub(oracle_subtype(et, e["damage_type"], e["mez_type"]))


def rec_key(r):
    """INV1 structural key WITHOUT table/pv — table is INV5 (advisory), pv is folded so
    a PvE effect can't be silently clobbered by its PvP sibling. Movement subtypes are
    named differently between the models, so compare effectType only there."""
    k = r["et"] if r["et"] == "Movement" else f"{r['et']}|{r['sub']}"
    return (k, "R" if r["resist"] else "U")


def _fold_complete(recs):
    """Fold complete damage/position type-sets into a single 'All' (sub='') record,
    per (effectType, resistible) group — symmetric across oracle and export.

    Both sets fold, independently. One group can hold both vectors: PROV-4's fold
    puts defense strength in `Enhancement` alongside mez strength, and Power Boost
    carries all 8 damage types AND all 3 positions there. Folding only the first
    match left the positions unfolded on the side whose damage set was complete and
    the damage types unfolded on the side whose wasn't, so a one-type difference
    read as ten."""
    groups = defaultdict(list)
    for r in recs:
        groups[(r["et"], r["resist"])].append(r)
    out = []
    for (_et, _resist), rs in groups.items():
        by_sub = defaultdict(list)
        for r in rs:
            by_sub[r["sub"]].append(r)
        subs = {s for s in by_sub if s}
        for complete in (COMPLETE_DAMAGE, COMPLETE_POSITION):
            if not subs >= complete:
                continue
            k = min(len(by_sub[s]) for s in complete)  # number of complete copies
            # the representative donates the folded record's table/scale, so pick it
            # by name rather than by set-iteration order — the latter moved the
            # advisory INV5 count by ±1 between runs of an unchanged tree.
            rep = by_sub[min(complete)][0]
            out.extend({**rep, "sub": ""} for _ in range(k))
            for s in complete:
                by_sub[s] = by_sub[s][k:]              # remainders stay
        for lst in by_sub.values():
            out.extend(lst)
    return out


def oracle_records(power, enh_fold=None):
    """Oracle effects -> canonical records. `enh_fold` (a Counter) receives the
    PROV-4 Enhancement census: where each `et_modifies` was routed, and which ones
    have no counterpart to route to."""
    recs = []
    for e in power["effects"]:
        if e["effect_type"] == "ResEffect":                 # Mids' resistance catch-all
            recs.append({"et": "ResEffect", "sub": "", "resist": e["resistible"],
                         "table": e["modifier_table"], "pv": e["pv_mode"], "aspect": e["aspect"],
                         "attrib_type": e["attrib_type"], "scale": e["scale"]})
            continue
        if e["effect_type"] == "Enhancement":               # PROV-4
            et, sub = enhanced_attrib(e)
            if enh_fold is not None:
                if et is None:
                    enh_fold[f"UNROUTED {sub}"] += 1       # sub carries the reason
                else:
                    comp = "" if et in COMPARABLE else " (not comparable)"
                    enh_fold[f"{e['et_modifies']} -> {et}{comp}"] += 1
        else:
            et = MIDS_ET.get(e["effect_type"])
            sub = None if et is None else (
                "" if et == "Movement"
                else norm_sub(oracle_subtype(et, e["damage_type"], e["mez_type"])))
        if not et or et not in COMPARABLE:
            continue
        recs.append({"et": et, "sub": sub, "resist": e["resistible"], "table": e["modifier_table"],
                     "pv": e["pv_mode"], "aspect": e["aspect"], "attrib_type": e["attrib_type"],
                     "scale": e["scale"]})
    return _fold_complete(recs)


def export_records(effects):
    recs = []
    for e in effects:
        et = e["effectType"]
        # aspect=Res on a non-Res-native attribute is Mids' ResEffect family (our
        # bridge keeps the affected attrib; Mids relabels to ResEffect). Fold to match.
        if e["aspect"] == "Res" and et not in _RES_NATIVE and et in (COMPARABLE | {"HealResistance"}):
            recs.append({"et": "ResEffect", "sub": "", "resist": e["resistible"],
                         "table": e["modifierTable"], "pv": e["pvMode"], "aspect": e["aspect"],
                         "attrib_type": e["attribType"], "scale": e["scale"]})
            continue
        if et not in COMPARABLE:
            continue
        sub = "" if et == "Movement" else norm_sub(e["subType"])
        recs.append({"et": et, "sub": sub, "resist": e["resistible"], "table": e["modifierTable"],
                     "pv": e["pvMode"], "aspect": e["aspect"], "attrib_type": e["attribType"],
                     "scale": e["scale"]})
    return _fold_complete(recs)


def details_of(recs):
    """flat records -> {inv1_key: [detail,...]} for multiset + classification."""
    out = defaultdict(list)
    for r in recs:
        out[rec_key(r)].append(r)
    return out


# --- tiered classifier --------------------------------------------------------
# tier -> gating: STRUCTURAL/UNCLASSIFIED gate; the rest are advisory.
TIER_BY_DESIGN = "BY_DESIGN"
TIER_RELABEL = "RELABEL"
TIER_UNCLASSIFIED = "UNCLASSIFIED"


def classify_power(only_o, only_e, od, ed, is_redirect):
    """Classify each INV1 residual for one power. only_o/only_e are Counters of
    (inv1_key, resist) -> count on each exclusive side; od/ed are the detail maps.
    Returns list of {tier, cls, side, key, resist, tables}."""
    out = []
    # a redirect-resolved shell: branch-inline ambiguity vs Mids -> advisory bucket,
    # never a defect (we picked the Always branch; Mids may inline a conditional one).
    redirect_tier = TIER_BY_DESIGN if is_redirect else None

    # TYPE_GRANULARITY: our single all-type marker ('' subtype) vs Mids per-type
    # expansion (or vice versa) for the SAME effectType. e.g. Base_Defense='All' vs
    # Mids Defense|Smashing.. ; all-damage DamageBuff stub vs per-type. Pair them so
    # neither half lands in UNCLASSIFIED.
    def et_of(key):
        return key[0].split("|")[0]

    o_ets_typed = {et_of(k) for k in only_o if "|" in k[0] and k[0].split("|")[1]}
    e_ets_bare = {et_of(k) for k in only_e if k[0].endswith("|")}
    o_ets_bare = {et_of(k) for k in only_o if k[0].endswith("|")}
    e_ets_typed = {et_of(k) for k in only_e if "|" in k[0] and k[0].split("|")[1]}
    granular_ets = (o_ets_typed & e_ets_bare) | (o_ets_bare & e_ets_typed)

    # INV4 (resistibility): a residual (effectType|subType) whose *base* string is
    # present on the OTHER side with the opposite resistibility bit — the two models
    # disagree only on resistible/unresistable. This is exactly the twin-collapse axis,
    # so it is a genuine candidate finding (kept in the triage worklist), distinguished
    # from a whole missing sibling. (Not gated wholesale — Mids self-buff records carry
    # an unresistable convention we cannot confidently call wrong against a stale DB.)
    o_base = {k[0] for k in od}
    e_base = {k[0] for k in ed}

    for side, only, detail, other_base in (
        ("oracle", only_o, od, e_base), ("export", only_e, ed, o_base)):
        # sorted, not set order: `examples` lands in a COMMITTED artefact that
        # verify-sync hashes across the two repos, so a re-run that reshuffles the
        # samples of an unchanged tree reads as drift.
        for (key, resist), n in sorted(only.items()):
            tables = sorted({d["table"] for d in detail.get((key, resist), [])})
            pvs = {d["pv"] for d in detail.get((key, resist), [])}
            et = et_of((key, resist))
            rec = {"side": side, "key": key, "resist": resist, "n": n, "tables": tables}
            if redirect_tier:
                out.append({**rec, "tier": redirect_tier, "cls": "REDIRECT"})
            elif any("inherent" in (t or "").lower() for t in tables):
                out.append({**rec, "tier": TIER_BY_DESIGN, "cls": "INHERENT_EXTRA"})
            elif et in granular_ets:
                out.append({**rec, "tier": TIER_RELABEL, "cls": "TYPE_GRANULARITY"})
            elif side == "oracle" and pvs and pvs <= {"PvP"}:
                # a PvP-only oracle record with no folded counterpart we model
                out.append({**rec, "tier": TIER_BY_DESIGN, "cls": "PVP_ONLY_ORACLE"})
            elif key in other_base:
                # same effect present on the other side, opposite resistibility bit
                out.append({**rec, "tier": TIER_UNCLASSIFIED, "cls": "RESISTIBILITY_FLIP"})
            else:
                out.append({**rec, "tier": TIER_UNCLASSIFIED, "cls": f"{side.upper()}_ONLY"})
    return out


def load_canonical(path, want_emit):
    if want_emit or not os.path.isfile(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        print(f"[diff_harness] emitting canonical export -> {os.path.relpath(path, REPO)}",
              file=sys.stderr)
        r = subprocess.run(
            ["npx", "tsx", os.path.join(HERE, "emit_canonical.ts"), path],
            cwd=REPO, capture_output=True, text=True,
        )
        sys.stderr.write(r.stderr)
        if r.returncode != 0 or not os.path.isfile(path):
            raise SystemExit("error: emit_canonical.ts failed; run it manually "
                             "(npx tsx tools/mids-oracle/emit_canonical.ts <out>)")
    idx = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if not line.strip():
                continue
            p = json.loads(line)
            idx[p["full_name"].lower()] = p
    return idx


def main(argv=None):
    ap = argparse.ArgumentParser(description="DSH5 oracle-backed differential harness")
    ap.add_argument("--mhd", default=read_i12.DEFAULT_MHD)
    ap.add_argument("--canonical", default=DEFAULT_CANONICAL,
                    help="export-side canonical JSONL (auto-emitted if missing)")
    ap.add_argument("--emit", action="store_true", help="force-refresh the canonical export")
    ap.add_argument("--rules-out", default=DEFAULT_RULES)
    ap.add_argument("--baseline", default=None,
                    help="prior rules file; fail on any UNCLASSIFIED signature not in it")
    ap.add_argument("--top", type=int, default=20, help="show N largest residual classes")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args(argv)

    # oracle (Mids) — every power in the HC main DB, keyed by full_name.
    with open(os.path.abspath(args.mhd), "rb") as fh:
        oracle_buf = fh.read()
    powers, oracle_power_count = read_i12.read_powers(oracle_buf)
    oracle = {p["full_name"].lower(): p for p in powers}
    export = load_canonical(args.canonical, args.emit)

    matched = sorted(set(oracle) & set(export))
    oracle_only = sorted(set(oracle) - set(export))
    export_only = sorted(set(export) - set(oracle))

    tier_counts = Counter()
    class_counts = Counter()
    class_examples = defaultdict(list)
    unclassified = set()          # regression-gating signatures: "power|side|key|resist"
    cohort_status = {}            # per-cohort-power result
    inv5_matched = inv5_table_agree = 0   # advisory table-agreement stat
    multiplicity_keys = 0         # advisory: same key present both sides, different count
    powers_with_residual = 0
    enh_fold = Counter()          # PROV-4: where each Mids et_modifies was routed

    for fn in matched:
        od = details_of(oracle_records(oracle[fn], enh_fold))
        ed = details_of(export_records(export[fn]["effects"]))
        is_redirect = bool(export[fn].get("redirect"))
        o_keys, e_keys = set(od), set(ed)
        shared = o_keys & e_keys
        # INV1 is a *set* (presence) diff, NOT a multiset-count diff. The collapse family
        # drops a whole distinct sibling KEY (a resistibility/type/mez variant) — it never
        # merely reduces duplicate copies of an identical key. Count differences on a
        # SHARED key come from Mids enumerating conditional/DoT/combo scale-tiers as
        # separate records (scale is skew-distrusted) — advisory MULTIPLICITY, never gated.
        only_o = {k: 1 for k in o_keys - e_keys}
        only_e = {k: 1 for k in e_keys - o_keys}
        multiplicity_keys += sum(1 for k in shared if len(od[k]) != len(ed[k]))

        # INV5 (advisory): for keys present on BOTH sides, do the normalized table
        # name-sets agree? A stat, never a gate (Mids strings carry typos).
        for key in shared:
            inv5_matched += 1
            ot = {norm_table(d["table"]) for d in od[key]}
            et = {norm_table(d["table"]) for d in ed[key]}
            if ot & et:
                inv5_table_agree += 1

        if not only_o and not only_e:
            if fn in COHORT:
                cohort_status[fn] = {"status": "EXACT", "unclassified": 0}
            continue
        powers_with_residual += 1
        divs = classify_power(only_o, only_e, od, ed, is_redirect)
        n_unclassified = 0
        for d in divs:
            tier_counts[d["tier"]] += d["n"]
            cls = d["cls"]
            class_counts[cls] += d["n"]
            if len(class_examples[cls]) < 6:
                class_examples[cls].append(
                    f"{fn} [{d['side']}] {d['key']}({d['resist']}) tables={d['tables'][:2]}")
            if d["tier"] == TIER_UNCLASSIFIED:
                n_unclassified += 1
                unclassified.add(f"{fn}|{d['side']}|{d['key']}|{d['resist']}")
        if fn in COHORT:
            cohort_status[fn] = {"status": "RESIDUAL", "unclassified": n_unclassified,
                                 "twin_exact": (fn not in TWIN_GATE)}

    # --- cohort gate --------------------------------------------------------
    gate_fail = []
    for fn in sorted(COHORT):
        st = cohort_status.get(fn)
        if st is None:
            if fn not in export or fn not in oracle:
                gate_fail.append(f"{fn} (missing from {'oracle' if fn not in oracle else 'export'})")
            continue
        if fn in TWIN_GATE and st["status"] != "EXACT":
            gate_fail.append(f"{fn} (twin must match EXACTLY, got {st['status']})")
        elif st.get("unclassified", 0) > 0:
            gate_fail.append(f"{fn} ({st['unclassified']} UNCLASSIFIED residual)")

    # --- regression gate against a committed baseline -----------------------
    regression_new = []
    if args.baseline and os.path.isfile(args.baseline):
        with open(args.baseline, encoding="utf-8") as fh:
            base = set(json.load(fh).get("unclassified_signatures", []))
        regression_new = sorted(unclassified - base)

    # --- write the typed rules / baseline artifact --------------------------
    total_comparable = inv5_matched + sum(tier_counts.values())
    rules = {
        "schema": "dsh5-oracle-divergence-rules/1",
        # PROV-3: this was the relative path and nothing else, into a gitignored tree
        # that no longer held the file. `read_i12.provenance` is the same primitive the
        # name bridge stamps, so the two artefacts can be told apart — or told to be the
        # same database — without either of them being rerun.
        "oracle": read_i12.provenance(args.mhd, oracle_buf, oracle_power_count),
        "note": "Mids = structural oracle only (~5wk rebalance-stale, typo-carrying "
                "strings). STRUCTURAL/UNCLASSIFIED gate; NUMERIC_DRIFT + table/aspect "
                "skew are advisory. Regen locally; the .mhd is gitignored.",
        "coverage": {
            "matched": len(matched),
            "oracle_only": len(oracle_only),
            "export_only": len(export_only),
            "non_hc_skipped": "rebirth+thunderspy excluded on the export side (Mids "
                              "has no answer); tracked by emit_canonical, not joined",
            "powers_with_residual": powers_with_residual,
        },
        "invariants": {
            "inv1_key_set_equality": "gating (distinct (effectType,subType,resistible) key set; "
                                     "pv folded, complete-type-sets + ResEffect canonicalized)",
            "inv_multiplicity": {
                "shared_keys_with_count_diff": multiplicity_keys,
                "tier": "advisory (Mids enumerates conditional/DoT scale-tiers; scale distrusted)",
            },
            "inv5_table_name_agreement": {
                "matched_keys": inv5_matched,
                "agree_after_norm": inv5_table_agree,
                "pct": round(100 * inv5_table_agree / inv5_matched, 2) if inv5_matched else None,
                "tier": "advisory (Mids string skew)",
            },
        },
        # PROV-4: Mids' `Enhancement` + `et_modifies` re-spelled in the export's
        # vocabulary. "(not comparable)" = routed correctly to a type COMPARABLE does
        # not gate (EnduranceDiscount, MaxHP, Stealth) — the same exclusion a plain
        # oracle record of that type already gets. "UNROUTED" = no MIDS_ET counterpart.
        "enhancement_fold": dict(sorted(enh_fold.items(), key=lambda kv: (-kv[1], kv[0]))),
        "summary_by_tier": dict(tier_counts),
        "summary_by_class": dict(class_counts.most_common()),
        "examples": {k: class_examples[k] for k in class_counts},
        "unclassified_signatures": sorted(unclassified),
        "cohort_gate": {
            "cohort": sorted(COHORT), "twin_gate": sorted(TWIN_GATE),
            "status": "FAIL" if gate_fail else "PASS", "failures": gate_fail,
        },
    }
    with open(args.rules_out, "w", encoding="utf-8") as fh:
        json.dump(rules, fh, indent=2)
        fh.write("\n")

    # --- report -------------------------------------------------------------
    print(f"=== DSH5 differential harness — HC sweep ===")
    print(f"  coverage: matched={len(matched)}  oracle-only={len(oracle_only)}  "
          f"export-only={len(export_only)}")
    print(f"  powers with structural residual: {powers_with_residual} / {len(matched)}")
    print(f"  MULTIPLICITY (advisory, shared key different count): {multiplicity_keys}")
    print(f"  INV5 table-name agreement (advisory): "
          f"{inv5_table_agree}/{inv5_matched} "
          f"({100 * inv5_table_agree / inv5_matched:.1f}%)" if inv5_matched else "  INV5: n/a")
    print(f"\n  divergence by tier:")
    for t in ("UNCLASSIFIED", "BY_DESIGN", "RELABEL"):
        print(f"    {t:14} {tier_counts.get(t, 0)}")
    print(f"\n  top {args.top} residual classes (tier in brackets):")
    tier_of = {"INHERENT_EXTRA": "BY_DESIGN", "TYPE_GRANULARITY": "RELABEL",
               "PVP_ONLY_ORACLE": "BY_DESIGN", "REDIRECT": "BY_DESIGN"}
    for cls, n in class_counts.most_common(args.top):
        tier = tier_of.get(cls, "UNCLASSIFIED")
        print(f"    {n:6}  [{tier:11}] {cls}")
        if args.verbose:
            for ex in class_examples[cls][:3]:
                print(f"              e.g. {ex}")

    print(f"\n  rules/baseline written -> {os.path.relpath(args.rules_out, REPO)}")
    print(f"  cohort gate: {'FAIL' if gate_fail else 'PASS'} "
          f"({len(COHORT)} powers, {len(TWIN_GATE)} twin-exact)")
    for f in gate_fail:
        print(f"      FAIL: {f}")

    exit_code = 0
    if gate_fail:
        print("\nCOHORT-GATE FAIL — a known-answer power regressed (collapse or reader drift).")
        exit_code = 1
    if args.baseline:
        if regression_new:
            print(f"\nREGRESSION: {len(regression_new)} new UNCLASSIFIED signature(s) vs baseline:")
            for s in regression_new[:25]:
                print(f"      + {s}")
            exit_code = 1
        else:
            print("\nREGRESSION-GATE: PASS — no new UNCLASSIFIED divergence vs baseline.")
    elif not gate_fail:
        print("\nFirst-run baseline established (no --baseline given). The "
              f"{len(unclassified)} UNCLASSIFIED signatures are the DSH6 triage worklist; "
              "commit the rules file to gate regressions.")
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
