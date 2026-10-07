#!/usr/bin/env python3
"""def-vs-export differ — parser COMPLETENESS oracle against authored HC source.

Parses the private Homecoming `.powers` source defs (authored, pre-binary) and
compares them, per AttribMod, against our exported HC power JSON:

  (B) KEY CENSUS      — every field key the defs carry at Power/Effect/AttribMod
                        level, and whether our export has a counterpart. Keys with
                        no counterpart are candidate parser drops.
  (A) TYPING FIDELITY — for name-matched powers, does our export reproduce the
                        def's per-effect damage Attrib set? (recurses child_effects)

This measures COMPLETENESS (vs authored truth), which oracle_gate/totals_gate —
generated from our own export — structurally cannot. HC only: the defs are HC,
so this validates the HC parser end-to-end; for Thunderspy it validates the
shared SCHEMA, not the (rebalanced) values. See memory hc-source-def-oracle.

Reads the `raw defs/` snapshot from the reference tree beside the repo;
COH_RAW_DEFS relocates it. See tools/refdata.py.
"""
import os, sys, re, glob, json, collections

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(_ROOT, "tools"))
import refdata  # noqa: E402

# Resolved the way extraction-audit/audit.py resolves the same snapshot — one
# spelling, in refdata, rather than two that have to agree. The defs moved out of
# the tree on 2026-09-24: they are private HC source this project may not ship,
# and as a gitignored directory they resolved to nothing on every fresh checkout.
#
# Unchecked, because the SKIP below is this gate's deliberate answer to their
# absence — CI has never held them. The skip PRINTS, naming COH_RAW_DEFS; a silent
# one reads exactly like a pass, which is the failure this gate exists to catch,
# aimed at itself.
DEFS = str(refdata.raw_defs(required=False))
EXPROOT = os.environ.get("COH_EXPORT_ROOT", os.path.join(_ROOT, "exported_powers"))


class Block:
    __slots__ = ("scalars", "children")
    def __init__(self):
        self.scalars = []    # list[(key, [values])]
        self.children = []   # list[(name, Block)]
    def get1(self, key):
        for k, v in self.scalars:
            if k == key:
                return v
        return None
    def kids(self, name):
        return [b for n, b in self.children if n == name]


_tok_re = re.compile(r'"[^"]*"|\S+')
def _tokens(line):
    out = []
    for m in _tok_re.findall(line):
        if m.startswith('"') and m.endswith('"'):
            out.append(m[1:-1])
        else:
            for part in m.split(','):
                if part:
                    out.append(part)
    return out


def parse_powers(text):
    root = Block()
    stack = [root]
    pending = None  # key a following '{' turns into a child block
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("//"):
            continue
        if line == "{":
            b = Block()
            stack[-1].children.append((pending or "", b))
            stack.append(b)
            pending = None
        elif line == "}":
            if len(stack) > 1:
                stack.pop()
            pending = None
        elif line.endswith("{") and len(line) > 1:
            b = Block()
            stack[-1].children.append((line[:-1].strip(), b))
            stack.append(b)
            pending = None
        else:
            toks = _tokens(line)
            if not toks:
                continue
            stack[-1].scalars.append((toks[0], toks[1:]))
            pending = toks[0]
    return root


def load_defs():
    powers = {}
    for f in glob.glob(f"{DEFS}/**/*.powers", recursive=True):
        try:
            text = open(f, encoding="latin-1").read()
        except OSError:
            continue
        m = re.search(r'^\s*Power\s+(\S+)', text, re.M)
        if not m:
            continue
        root = parse_powers(text)
        pw = next((b for n, b in root.children if n.startswith("Power")), None)
        if pw is None and root.children:
            pw = root.children[0][1]
        if pw is not None:
            powers[m.group(1).lower()] = (m.group(1), pw, f)
    return powers


def census(powers):
    pk, ek, amk, amb, blk = (collections.Counter() for _ in range(5))
    for _full, pw, _f in powers.values():
        for k, _ in pw.scalars:
            pk[k] += 1
        for eff in pw.kids("Effect"):
            for k, _ in eff.scalars:
                ek[k] += 1
            for n, _ in eff.children:
                if n:
                    blk[n] += 1
            for am in eff.kids("AttribMod"):
                for k, _ in am.scalars:
                    amk[k] += 1
                for n, _ in am.children:
                    if n:
                        amb[n] += 1
    return pk, ek, amk, amb, blk


def load_export_index():
    idx = {}
    for p in glob.glob(f"{EXPROOT}/**/*.json", recursive=True):
        if "/thunderspy/" in p or "/rebirth/" in p:
            continue
        if os.path.basename(p) in ("salvage.json", "_export_manifest.json"):
            continue
        try:
            d = json.load(open(p))
        except (OSError, json.JSONDecodeError):
            continue
        if isinstance(d, dict) and d.get("full_name"):
            idx[d["full_name"].lower()] = d
    return idx


def export_templates(d):
    out = []
    def walk(e):
        if not isinstance(e, dict):
            return
        out.extend(t for t in (e.get("templates") or []) if isinstance(t, dict))
        for c in e.get("child_effects") or []:
            walk(c)
    for e in d.get("effects") or []:
        walk(e)
    return out


DMG = {"ksmashing": "Smashing_Dmg", "klethal": "Lethal_Dmg", "kfire": "Fire_Dmg",
       "kcold": "Cold_Dmg", "kenergy": "Energy_Dmg", "knegative_energy": "Negative_Energy_Dmg",
       "knegative": "Negative_Energy_Dmg", "kpsionic": "Psionic_Dmg", "ktoxic": "Toxic_Dmg",
       "kspecial": "Special_Dmg", "kheal": "Heal_Dmg"}
DMG_EXPORT = set(DMG.values())

# Powers whose def states direct damage the EXPORT has none of, because the damage
# is dealt by a SEPARATE summoned power record (EntCreate pseudo-pet), not this
# power's own effect tree — the differ can't follow the grant chain. All six are the
# Fiery-Aura/Fire-Manipulation "Burn" fire-patch (a `Pets.*` entity carries the Fire
# ticks). NOT a parser drop; the datum lives on the pet record. A NEW power appearing
# here trips the gate → confirm it's the summon pattern (then add it), or it's a real
# regression. Keyed by full_name.
GATE_MISSING_ALLOWLIST = {
    "Blaster_Support.Fire_Manipulation.Burn": "Fire patch — damage on Pets.* pseudo-pet",
    "Brute_Defense.Fiery_Aura.Burn": "Fire patch — damage on Pets.* pseudo-pet",
    "Scrapper_Defense.Fiery_Aura.Burn": "Fire patch — damage on Pets.* pseudo-pet",
    "Sentinel_Defense.Fiery_Aura.Burn": "Fire patch — damage on Pets.* pseudo-pet",
    "Stalker_Defense.Fiery_Aura.Burn": "Fire patch — damage on Pets.* pseudo-pet",
    "Tanker_Defense.Fiery_Aura.Burn": "Fire patch — damage on Pets.* pseudo-pet",
}


def def_damage_types(pw, *, damage_dealt_only):
    """Damage-element set the def states for a power. With damage_dealt_only, restrict
    to AttribMods that actually DEAL damage (Aspect kAbs on a `*Damage*` table),
    excluding the kStr/kRes riders that merely reference a damage attrib (buff/resist)."""
    types = set()
    for eff in pw.kids("Effect"):
        for am in eff.kids("AttribMod"):
            if damage_dealt_only:
                aspect = (am.get1("Aspect") or [""])[0].lower()
                table = (am.get1("Table") or [""])[0]
                if aspect != "kabs" or "Damage" not in table:
                    continue
            types |= {DMG[a.lower()] for a in (am.get1("Attrib") or []) if a.lower() in DMG}
    return types


def export_damage_types(d):
    return {a for t in export_templates(d) for a in (t.get("attribs") or []) if a in DMG_EXPORT}


def classify_fidelity(powers, exp, *, damage_dealt_only):
    """Bucket each name-matched, def-damage-typed power into exact/richer/poorer/
    missing/conflict by comparing def vs export damage-type sets."""
    buckets = {k: [] for k in ("exact", "richer", "poorer", "missing", "conflict")}
    for full, pw, _f in powers.values():
        d = exp.get(full.lower())
        if not d:
            continue
        dt = def_damage_types(pw, damage_dealt_only=damage_dealt_only)
        if not dt:
            continue
        et = export_damage_types(d)
        if et == dt:
            buckets["exact"].append((full, dt, et))
        elif not et:
            buckets["missing"].append((full, dt, et))
        elif dt < et:
            buckets["richer"].append((full, dt, et))
        elif et < dt:
            buckets["poorer"].append((full, dt, et))
        else:
            buckets["conflict"].append((full, dt, et))
    return buckets

# def AttribMod key -> export template field (empty string = genuinely no counterpart)
AM_MAP = {"Attrib": "attribs", "Aspect": "aspect", "Target": "target", "Table": "table",
          "Scale": "scale", "Duration": "duration", "Magnitude": "magnitude", "Type": "type",
          "ApplicationType": "application_type", "StackType": "stack", "Stack": "stack",
          "CasterStackType": "caster_stack", "StackKey": "stack_key", "StackLimit": "stack_limit",
          "Flags": "flags_raw", "Delay": "delay", "Period": "application_period",
          "TickChance": "tick_chance", "TickAdditive": "tick_mag_additive",
          "TickMultiplier": "tick_mag_multiplier", "MagnitudeExpr": "magnitude_expression",
          "DurationExpr": "duration_expression", "Requires": "jit_requires",
          "CancelEvents": "cancel_events", "Suppress": "suppress_events", "Params": "params",
          "Messages": "(ui strings)", "FX": "(visual)",
          "DelayedRequires": "", "RequiredEvent": "required_events"}


def cmd_report(powers, exp):
    print(f"parsed {len(powers)} power defs from {DEFS}")
    pk, ek, amk, amb, blk = census(powers)
    print("\n==== (B) DEF AttribMod key census ====")
    for k, c in amk.most_common():
        m = AM_MAP.get(k, "")
        print(f"  {k:18} {c:6}  {m or '<-- NO export counterpart'}")
    print("  child blocks:", dict(amb.most_common()))
    print("\n  Effect-level keys:", dict(ek.most_common()))
    print("  Effect/Power child blocks:", dict(blk.most_common()))

    print("\n==== (A) damage-typing fidelity (recurses child_effects) ====")
    print(f"  export index: {len(exp)} powers")
    # Informational view keeps the loose filter (any attrib naming a dmg element);
    # the GATE uses the sharp damage-dealt-only filter.
    for label, dealt in (("all attribs naming a dmg element", False),
                         ("damage DEALT only (Aspect kAbs + *Damage* table)", True)):
        b = classify_fidelity(powers, exp, damage_dealt_only=dealt)
        total = sum(len(v) for v in b.values())
        print(f"\n  [{label}]  matched def-damage-typed powers: {total}")
        for k in ("exact", "richer", "poorer", "missing", "conflict"):
            print(f"    {k:8}: {len(b[k])}")
        for tag in ("poorer", "conflict", "missing"):
            for full, dt, et in b[tag][:12]:
                print(f"    {tag.upper()} {full}  def={sorted(dt)} export={sorted(et)}")


def cmd_gate(powers, exp):
    """Fail-loud gate on HC damage-typing FAITHFULNESS vs authored source defs.
    Invariant: the parser must never DROP or MISTYPE a damage the power directly
    deals. Fails on any `poorer` (dropped) / `conflict` (mistyped), and on any
    `missing` not in the summon-pattern allowlist. `richer` (export adds types from
    child/summoned/proc effects) is not a drop → allowed."""
    if not powers:
        print(f"SKIP — def-fidelity gate: no source defs at {DEFS} "
              f"(set COH_RAW_DEFS). Gate enforces only where the private HC defs are present.")
        return 0
    b = classify_fidelity(powers, exp, damage_dealt_only=True)
    failures = []
    for full, dt, et in b["poorer"]:
        failures.append(f"DROPPED  {full}: def deals {sorted(dt)}, export has {sorted(et)}")
    for full, dt, et in b["conflict"]:
        failures.append(f"MISTYPED {full}: def deals {sorted(dt)}, export has {sorted(et)}")
    for full, dt, _et in b["missing"]:
        if full not in GATE_MISSING_ALLOWLIST:
            failures.append(f"MISSING  {full}: def deals {sorted(dt)}, export has none "
                            f"(if summon-pattern, add to GATE_MISSING_ALLOWLIST with a reason)")
    stale = [k for k in GATE_MISSING_ALLOWLIST
             if k not in {f for f, _, _ in b["missing"]}]
    matched = sum(len(v) for v in b.values())
    if failures:
        print(f"FAIL — def-fidelity gate ({len(failures)}):")
        for m in failures:
            print("  · " + m)
        return 1
    note = f" (stale allowlist entries no longer missing: {stale})" if stale else ""
    print(f"OK — def-fidelity: {matched} HC damage-dealing powers vs authored defs; "
          f"0 dropped, 0 mistyped; {len(b['missing'])} summon-pattern allowlisted, "
          f"{len(b['richer'])} export-richer.{note}")
    return 0


def main():
    import argparse
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--gate", action="store_true",
                    help="fail-loud gate: exit 1 on any dropped/mistyped damage type")
    args = ap.parse_args()
    powers = load_defs()
    exp = load_export_index()
    raise SystemExit(cmd_gate(powers, exp) if args.gate else cmd_report(powers, exp))


if __name__ == "__main__":
    main()
