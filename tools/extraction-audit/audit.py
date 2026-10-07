"""
Phase 1 extraction-completeness audit: diff the authoritative `.powers` raw defs
against our `exported_powers` JSON to find what the bin parser drops.

Per power it compares two things and aggregates across the whole set:
  1. ATTRIBS  — every attrib named in a `.powers` AttribMod should appear in some
     exported template. Attribs present in `.powers` but absent from our export are
     candidates the parser drops (the "unmapped exotic attrib" class).
  2. POWER FIELDS — every `.powers` power-level scalar should map to an export field
     (asset-reference fields are expected-skips and are reported separately).

Usage:
  py -3 tools/extraction-audit/audit.py [--limit N] [--power Energy_Torrent] [--json out.json]

Output: a ranked report (most-frequently-missing first) so we can prioritise parser
fixes by impact, not anecdote.

Caveat (2026-07-07): the `raw defs/` snapshot is OLDER than the live bins, so a
residual "missing" attrib can be genuine game drift, not a parser drop — HC has
reworked powers in place, keeping the internal name (see the slot-reuse rename
gotcha: Stalker Willpower "Resurgence" is now the +Regen passive "Up to the
Challenge"; Ninjitsu "Smoke_Flash" is now "Bo Ryaku"). Check the export's
display_name against the .powers one before treating a residual as a drop.
As of 2026-07-07 the report's attrib section is entirely this drift class plus
one unmapped kUnique3 on a silent temp power.
"""

from __future__ import annotations
import sys, json, argparse
from pathlib import Path
from collections import Counter

sys.path.insert(0, str(Path(__file__).resolve().parent))
from parse_powers import parse_powers_file, iter_attribmods  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]

sys.path.insert(0, str(ROOT / "tools"))
import refdata  # noqa: E402

# Out of the tree since 2026-09-24 — private HC source, and a gitignored directory
# meant a fresh checkout resolved it to nothing. Unchecked here because the SKIP at
# the top of the audit is the deliberate answer for a machine without them; it
# prints COH_RAW_DEFS rather than going quiet. See tools/refdata.py.
RAW_DEFS = refdata.raw_defs(required=False)
EXPORT = ROOT / "exported_powers"

# `.powers` power-level fields that are pure asset/presentation references — we
# deliberately don't import these (2026-06-04 rule: capture mechanics, skip asset
# refs). Reported separately, never counted as gaps.
ASSET_FIELDS = {
    "visualfx", "include", "iconname", "fxstart", "fxidle", "fxstop", "fxattack",
    "fxblock", "fxhit", "fxdeath", "fxstartfidget", "fxfidget", "deviceanim",
    "displayname", "displayhelp", "displayshorthelp", "displaypowerdefault",
    # presentation / targeting-UI strings — not mechanics
    "displaytargetshorthelp", "displaytargethelp", "displayattackerhit",
    "displayvictimhit", "displayattackerhitself", "displayfloat", "displaycaster",
    "highlightring", "highlighteval", "highlighticon",
}

# Normalised `.powers` field names we KNOW are captured in the export (possibly
# under a renamed key — e.g. TimeToActivate -> activation_time). Anything not here
# and not an asset ref surfaces as a genuine "field we don't capture" candidate.
CAPTURED_FIELDS = {
    "name", "type", "accuracy", "attacktypes", "entsaffected", "entsautohit",
    "target", "targetsecondary", "range", "rangesecondary", "endurancecost",
    "timetoactivate", "rechargetime", "effectarea", "radius", "arc",
    "boostsallowed", "maxtargetshit", "requires", "activaterequires",
    "targetrequires", "interrupttime", "activateperiod", "numallowed",
    "numcharges", "usagetime", "lifetime", "shortname", "castthrough",
    "targetsautohit", "targetsaffected",
    # BuyRequires is bin field 26 `buy_requires`, exported as `requires`
    # (verified 2026-07-07: Frag_Grenade's expression matches verbatim).
    "buyrequires",
    # TimeToRoot is Parse7 field 48b, exported as `time_to_root` (2026-07-07).
    "timetoroot",
    # The per-mez ToggleIgnore*/CastThrough* spellings land in the exported
    # `toggle_ignore` / `cast_through` lists (bin field 34 cast_flags).
    "toggleignorehold", "toggleignorestun", "toggleignoresleep",
    "castthroughhold", "castthroughstun", "castthroughsleep",
    "castthroughterrorize",
    # Power-level IgnoreStrength is propagated by the game's bin compiler onto
    # every effect template's flags (verified: Kuji-In Toh carries it on all 3
    # templates) — captured at template level, honored by the converter.
    "ignorestrength",
    # .powers `MaxTargetsExpr`/`ChainEff`/`ChainTarget` export as
    # max_targets_expression / chain_eff_expression / chain_target_expression
    # (emitted only when non-empty, which is exactly when .powers carries them).
    "maxtargetsexpr", "chaineff", "chaintarget",
}

# Fields that exist in the `.powers` server source but are NOT serialized into
# the client powers.bin at all. These can never be parser-captured; if the
# planner needs one it must be sourced from the committed `raw defs/` oracle
# converter-side. GlobalStrengthsDisallowed is an HC addition with no i24
# parse-table entry at all.
#
# StrengthsDisallowed is NOT one of them — the 2026-07-07 byte-accounting that
# put it here was wrong, and the parser has read it out of the tail since
# 2026-07-21 (`_parse_power_tail`). Exempting a field the parser captures is
# how a regression in it would go unnoticed.
SERVER_ONLY_FIELDS = {
    "globalstrengthsdisallowed",
}


def norm_field(name: str) -> str:
    return name.lower().replace("_", "")


# `.powers` attrib names whose export spelling differs beyond suffix-stripping.
# Keys/values are post-norm_attrib (lowercase, no underscores/spaces, suffixes
# stripped). Verified against parser/_enums.py ATTRIB_NAME 2026-07-07.
ATTRIB_ALIASES = {
    "defense": "basedefense",          # kDefense ↔ Base_Defense
    "speedrunning": "runningspeed",    # kSpeedRunning ↔ RunningSpeed
    "speedflying": "flyingspeed",
    "speedjumping": "jumpingspeed",
    "entcreate": "createentity",       # kEntCreate ↔ Create_Entity
    "stealthradius": "stealthradiuspve",
    "stealthradiusplayer": "stealthradiuspvp",
    "aoe": "area",                     # kAOE_Attack ↔ Area (positional defense)
    "knock": "knockback",              # kKnock ↔ Knockback (verified: Shockwaves)
    "phase": "combatphase",            # kPhase ↔ Combat_Phase
    "nullbool": "null",                # kNullBool ↔ Null
    "rechargetime": "recharge",        # kRechargeTime ↔ Recharge
    "viewattrib": "viewattributes",    # kViewAttrib ↔ View_Attributes
}

# Attribs with zero build-math value: kNull/kNullBool are the engine's no-op
# placeholder (FX/icon carriers). Missing ones are reported separately, never
# as gaps.
MARKER_ATTRIBS = {"null"}


# Powers whose def-vs-export attrib mismatch is confirmed GAME DRIFT, not a parser
# drop: the private `.powers` snapshot predates live-bin reworks that changed or
# renamed the power in place (the module caveat). Keyed by full_name, with evidence.
# A NEW power surfacing outside this list trips the gate for human review; an entry
# that stops mismatching is reported as stale. Re-verify with `audit.py --power <n>`
# and compare the export `display_name` against the def's.
DRIFT_ALLOWLIST = {
    # HC reworked Willpower Resurgence per-AT: the single DisplayName ref
    # (P2219956264) now resolves to "Resurgence" (Brute/Scrapper/Sentinel/Tanker)
    # and "Up to the Challenge" (Stalker) — proof the snapshot predates the rework.
    # The missing smashing/lethal/tohit are the old Aspect-kStr damage-buff/debuff +
    # ToHit riders the reworked power no longer carries (a parser drop would lose the
    # whole 8-element aligned AttribMod, not two of eight). Verified 2026-07-19.
    "Brute_Defense.Willpower.Resurgence": "Willpower rework drift — dmg-buff/ToHit riders removed live",
    "Scrapper_Defense.Willpower.Resurgence": "Willpower rework drift — dmg-buff/ToHit riders removed live",
    "Sentinel_Defense.Willpower.Resurgence": "Willpower rework drift — dmg-buff/ToHit riders removed live",
    "Tanker_Defense.Willpower.Resurgence": "Willpower rework drift — dmg-buff/ToHit riders removed live",
    "Stalker_Defense.Willpower.Resurgence": "Willpower rework drift — slot is now the 'Up to the Challenge' passive",
    # Ninjitsu Smoke_Flash is renamed "Bo Ryaku" live; `placate` is the old power's.
    "Stalker_Defense.Ninjitsu.Smoke_Flash": "slot-rename drift — now 'Bo Ryaku' (placate belongs to the old power)",
    # Issue 28 Page 4 (live 2026-10-06), each read off the patch notes:
    # Rage 2.0 "no longer crash[es]" — the -Def/-End crash templates are gone.
    "Brute_Melee.Super_Strength.Rage": "I28P4 Rage rework — crash (-Def/-End) removed",
    "Tanker_Melee.Super_Strength.Rage": "I28P4 Rage rework — crash (-Def/-End) removed",
    # "Remove the endurance cost per target hit".
    "Defender_Buff.Sonic_Debuff.Sonic_Repulsion": "I28P4 — per-target endurance cost removed",
    "Mastermind_Buff.Sonic_Resonance.Sonic_Repulsion": "I28P4 — per-target endurance cost removed",
    # Debilitating Web Grenade: the slows now spread to an AoE, so they moved
    # into Villain_Pets.Web_Grenade.Web_Grenade, fired by Execute_Power, which
    # collect_export_attribs does not follow.
    "Defender_Buff.Traps.Web_Grenade": "I28P4 — slows moved behind Execute_Power (AoE)",
    "Mastermind_Buff.Traps.Web_Grenade": "I28P4 — slows moved behind Execute_Power (AoE)",
    # MM Detonator is now an AoE upgrade arming every henchman, not a GrantPower.
    "Mastermind_Buff.Traps.Detonator": "I28P4 — reworked into a henchman-wide upgrade",
    # "Player versions of Caltrops and Thorntrops now apply Avoid (on live they apply Afraid)".
    "Mastermind_Pets.Ninja_Pets.Jounins_Caltrops": "I28P4 — Caltrops apply Avoid instead of Afraid",
}


def norm_attrib(name: str) -> str:
    """Normalise a `.powers` (kEnergy) or export (Energy_Dmg) attrib to a common key."""
    s = name.strip()
    if s.startswith("k") and len(s) > 1 and s[1].isupper():
        s = s[1:]
    s = s.lower().replace("_", "").replace(" ", "")
    for suf in ("dmg", "attack", "boost"):
        if s.endswith(suf) and len(s) > len(suf):
            s = s[: -len(suf)]
    # kSmashingElude etc. ↔ Smashing_Elusivity (kAOEElude ↔ Area_Elusivity via
    # the aoe alias applied to the stem)
    if s.endswith("elude"):
        s = s[: -len("elude")]
        s = ATTRIB_ALIASES.get(s, s) + "elusivity"
    return ATTRIB_ALIASES.get(s, s)


def powers_path_to_export(fullname: str) -> Path:
    # Blaster_Ranged.Energy_Blast.Energy_Torrent -> exported_powers/blaster_ranged/energy_blast/energy_torrent.json
    parts = fullname.split(".")
    return EXPORT.joinpath(*[p.lower() for p in parts]).with_suffix(".json")


def collect_export_attribs(power_json: dict, _seen: set[str] | None = None) -> set[str]:
    """All attribs reachable from a power's export: effects, activation_effects,
    child_effects, AND redirect targets. Self-rez powers (Resurgence, Rise of the
    Phoenix) serialize ZERO effect templates in the client bin — their real
    effects live entirely behind `redirect` entries into the Redirects.* category
    (verified 2026-07-07). Not following redirects made that whole class look
    like dropped attribs."""
    out: set[str] = set()
    seen = _seen if _seen is not None else set()

    def walk(groups):
        for g in groups or []:
            if not isinstance(g, dict):
                continue
            for t in g.get("templates", []) or []:
                if isinstance(t, dict):
                    for a in t.get("attribs", []) or []:
                        if a:
                            out.add(norm_attrib(str(a)))
            walk(g.get("child_effects"))

    walk(power_json.get("effects"))
    walk(power_json.get("activation_effects"))

    for rd in power_json.get("redirect") or []:
        target = rd.get("name") if isinstance(rd, dict) else None
        if not target or target.lower() in seen:
            continue
        seen.add(target.lower())
        rp = powers_path_to_export(target)
        if rp.exists():
            try:
                rj = json.loads(rp.read_text(encoding="utf-8"))
                if isinstance(rj, list):
                    rj = rj[0] if rj else {}
                out |= collect_export_attribs(rj, seen)
            except Exception:  # noqa: BLE001
                pass
    return out


def audit_power(powers_file: Path):
    """Return (fullname, missing_attribs, entity_attribs, missing_fields, asset_fields, server_only_fields) or None."""
    try:
        pw = parse_powers_file(powers_file)
    except Exception as e:  # noqa: BLE001
        return ("PARSE_ERROR:" + powers_file.name, {str(e)}, set(), set(), set(), set())
    fullname = pw.get("fullname") or powers_file.stem
    export_path = powers_path_to_export(fullname)
    if not export_path.exists():
        return None  # no matching export (e.g. category we don't export) — skip
    try:
        ej = json.loads(export_path.read_text(encoding="utf-8"))
    except Exception:  # noqa: BLE001
        return None
    if isinstance(ej, list):
        ej = ej[0] if ej else {}

    # --- attribs ---
    powers_attribs: set[str] = set()
    for _ei, am in iter_attribmods(pw):
        a = am.get("Attrib")
        raw_vals = a if isinstance(a, list) else [a]
        for item in raw_vals:
            if not item:
                continue
            # A single Attrib line may list several space-separated attribs
            # (e.g. `Attrib kSmashing kLethal kFire ...`).
            for tok in str(item).split():
                powers_attribs.add(norm_attrib(tok))
    export_attribs = collect_export_attribs(ej)
    missing_attribs = {a for a in powers_attribs
                       if a and a not in export_attribs and a not in MARKER_ATTRIBS}
    # Entity-summon powers (Burn, Voltaic Sentinel, MM pets…): the bin moves the
    # damage/mez onto the created entity's own powers (a different export tree,
    # handled by convert-pet-entities). Their "missing" attribs are expected —
    # bucket separately so they don't read as parser drops.
    summons_entity = "createentity" in export_attribs
    if summons_entity and missing_attribs:
        entity_attribs = missing_attribs
        missing_attribs = set()
    else:
        entity_attribs = set()

    # --- power-level fields ---
    export_field_norms = {norm_field(k) for k in ej.keys()}
    missing_fields: set[str] = set()
    asset_fields: set[str] = set()
    server_only_fields: set[str] = set()
    for k, v in pw.items():
        if k in ("fullname",):
            continue
        if isinstance(v, list) and v and isinstance(v[0], dict):
            continue  # block (Effect/AttribMod/etc.), handled via attribs
        nf = norm_field(k)
        if nf in ASSET_FIELDS:
            asset_fields.add(k)
        elif nf in SERVER_ONLY_FIELDS:
            server_only_fields.add(k)
        elif nf in CAPTURED_FIELDS or nf in export_field_norms:
            continue  # captured (possibly under a renamed key)
        else:
            missing_fields.add(k)
    return (fullname, missing_attribs, entity_attribs, missing_fields, asset_fields, server_only_fields)


def run_gate() -> int:
    """Fail-loud gate on parser COMPLETENESS: no power may DROP an attrib its authored
    `.powers` def declares. Reuses audit_power (redirect-following, summon-bucketing,
    alias-normalized), so a real per-power attrib drop fails the build — the class the
    self-generated regression fixtures structurally cannot catch. STRUCTURAL: checks
    attrib PRESENCE, never a value, so it stays green across balance patches (which
    change numbers, not which attrib a power modifies). HC only (the defs are HC);
    self-SKIPs when the private defs are absent, so it enforces on the runner that has
    them and no-ops elsewhere. Confirmed game-drift mismatches are DRIFT_ALLOWLIST'd."""
    if not RAW_DEFS.exists():
        print(f"SKIP — attrib-completeness gate: no source defs at {RAW_DEFS} "
              f"(set COH_RAW_DEFS). "
              f"Enforces only where the private HC defs are present.")
        return 0
    mismatching: dict[str, list[str]] = {}
    scanned = 0
    for f in sorted(RAW_DEFS.rglob("*.powers")):
        res = audit_power(f)
        if res is None:
            continue
        fullname, miss_a, _ent, _mf, _af, _sf = res
        scanned += 1
        if miss_a:
            mismatching[fullname] = sorted(miss_a)
    dropped = [(fn, m) for fn, m in mismatching.items() if fn not in DRIFT_ALLOWLIST]
    stale = [k for k in DRIFT_ALLOWLIST if k not in mismatching]
    if dropped:
        print(f"FAIL — attrib-completeness gate ({len(dropped)} power(s) dropped a def attrib):")
        for fn, m in sorted(dropped):
            print(f"  · DROPPED {fn}: def declares {m}, absent from export "
                  f"(if confirmed game drift, add to DRIFT_ALLOWLIST with evidence)")
        return 1
    note = f"  (stale allowlist entries no longer mismatching: {stale})" if stale else ""
    print(f"OK — attrib-completeness: {scanned} HC powers vs authored defs; "
          f"0 dropped; {len(DRIFT_ALLOWLIST)} drift-allowlisted.{note}")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--power", type=str, default=None, help="filter to powers whose file stem contains this")
    ap.add_argument("--json", type=str, default=None)
    ap.add_argument("--gate", action="store_true",
                    help="fail-loud gate: exit 1 if any power drops a def-declared attrib")
    args = ap.parse_args()
    if args.gate:
        raise SystemExit(run_gate())

    files = sorted(RAW_DEFS.rglob("*.powers"))
    if args.power:
        files = [f for f in files if args.power.lower() in f.stem.lower()]
    if args.limit:
        files = files[: args.limit]

    attrib_counter: Counter = Counter()
    field_counter: Counter = Counter()
    asset_counter: Counter = Counter()
    server_only_counter: Counter = Counter()
    entity_attrib_counter: Counter = Counter()
    attrib_examples: dict[str, str] = {}
    field_examples: dict[str, str] = {}
    audited = 0
    no_export = 0

    for f in files:
        res = audit_power(f)
        if res is None:
            no_export += 1
            continue
        fullname, miss_a, ent_a, miss_f, asset_f, server_f = res
        audited += 1
        for a in miss_a:
            attrib_counter[a] += 1
            attrib_examples.setdefault(a, fullname)
        for fld in miss_f:
            field_counter[fld] += 1
            field_examples.setdefault(fld, fullname)
        for af in asset_f:
            asset_counter[af] += 1
        for sf in server_f:
            server_only_counter[sf] += 1
        for ea in ent_a:
            entity_attrib_counter[ea] += 1

    print("\n=== EXTRACTION AUDIT (Phase 1: .powers vs exported_powers) ===")
    print(f"raw .powers files: {len(files)}   audited (have export): {audited}   no-export (skipped): {no_export}\n")

    print("--- ATTRIBS in .powers but MISSING from our export (top 40 by power count) ---")
    for a, c in attrib_counter.most_common(40):
        print(f"  {c:5d}  {a:24s}  e.g. {attrib_examples[a]}")
    if not attrib_counter:
        print("  (none — every .powers attrib appears in our export)")

    print("\n--- POWER-LEVEL FIELDS in .powers but MISSING from our export (top 40) ---")
    for fld, c in field_counter.most_common(40):
        print(f"  {c:5d}  {fld:24s}  e.g. {field_examples[fld]}")
    if not field_counter:
        print("  (none)")

    print("\n--- ATTRIBS likely living on a SUMMONED ENTITY's powers (power exports")
    print("    Create_Entity; convert-pet-entities handles the entity tree) ---")
    for ea, c in entity_attrib_counter.most_common(15):
        print(f"  {c:5d}  {ea}")

    print("\n--- SERVER-ONLY fields (not serialized in the client bin — converter must")
    print("    source these from `raw defs/` if needed; see audit.py SERVER_ONLY_FIELDS) ---")
    for sf, c in server_only_counter.most_common():
        print(f"  {c:5d}  {sf}")

    print("\n--- ASSET-REF fields (intentionally skipped, for reference) ---")
    for af, c in asset_counter.most_common(15):
        print(f"  {c:5d}  {af}")

    if args.json:
        Path(args.json).write_text(json.dumps({
            "audited": audited, "no_export": no_export,
            "missing_attribs": attrib_counter.most_common(),
            "missing_fields": field_counter.most_common(),
            "server_only_fields": server_only_counter.most_common(),
            "entity_attribs": entity_attrib_counter.most_common(),
            "asset_fields": asset_counter.most_common(),
        }, indent=2), encoding="utf-8")
        print(f"\nwrote {args.json}")


if __name__ == "__main__":
    main()
