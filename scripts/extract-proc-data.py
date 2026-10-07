"""Source the proc/global IO database from the committed export (was: from .pigg).

PHASE 1 (globals): resolve always-on GLOBAL IO effects from the export and validate
against the current hand PROC_DATABASE as oracle. The authoritative value for a global
lives in the granted `Set_Bonus.Global_Bonus.<Set>[_suffix]` power, NOT the boost
piece's `Null/Current` marker (Shield Wall marker=0.03 but real=0.05=5%). See the
memory `global-io-values-from-globalbonus-powers`.

PHASE 2 (this file): emit structured `ProcEffect[]` per global piece to
`pipeline/_shared/proc-globals.json`, keyed by the authored proc key, so a consumer reads
`.effects` instead of parsing the `mechanics` prose.

This read `homecoming`'s live `.pigg` archives until 2026-09-25, which is the whole
reason `regen-all.cjs` excluded it by name and the six generated modules sat frozen with
no regen step that could re-emit them. It reads `exported_powers/` now, like every other
converter, and is a regen step. Nothing moved: all six files come out byte-identical,
because the four things it took from the binary are all in the committed export already —
`boostsets.bin` is `boostsets.json` verbatim, `powers.bin`'s boost-piece and Set_Bonus
records are the `boosts/` and `set_bonus/` trees, `classes.bin`'s `Melee_ProcDamage` is in
`tables/`, and `clientmessages-en.bin` was read for exactly one field (`GroupName`) that
the export stores already resolved.

Usage:
    python3 scripts/extract-proc-data.py            # validate + emit
    python3 scripts/extract-proc-data.py --check    # validate only, no emit
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path
from functools import lru_cache

PROJECT_ROOT = Path(__file__).resolve().parent.parent
HAND_PROC_DATA = PROJECT_ROOT / 'hand-data' / 'proc-data.json'
OUT_DIR = PROJECT_ROOT / 'pipeline' / '_shared'
OUT_GLOBALS = OUT_DIR / 'proc-globals.json'
OUT_DAMAGE = OUT_DIR / 'proc-damage.json'
OUT_EFFECTS = OUT_DIR / 'proc-effects.json'
OUT_PPM = OUT_DIR / 'proc-ppm.json'
OUT_PERIOD = OUT_DIR / 'proc-activate-period.json'
OUT_BOOSTS = OUT_DIR / 'proc-boosts-allowed.json'

# ---------------------------------------------------------------------
# Committed-export loaders.
#
# `export_powers.py` parses boostsets.bin / powers.bin / classes.bin once per fork,
# resolves the message-store display strings, and commits the result under
# `exported_powers/`. These loaders read that JSON instead of re-parsing the archives,
# which is why this script no longer imports `bin_crawler` at all — it reads only
# committed files and can run on a machine with no game install.
#
# Each record class below is the SUBSET of its exported shape this script reads, not the
# whole thing. `extract-rebirth-io-sets-v2.py` takes the same approach against the same
# trees. The attribute names match the binary parser's records field for field, so every
# resolver between here and the emitters is untouched.
# ---------------------------------------------------------------------
EXPORT_ROOT = PROJECT_ROOT / 'exported_powers'


def _seconds(raw) -> float:
    """`"10.25 seconds"` -> `10.25`. `export_powers.format_duration` writes a duration as
    a string with the unit appended and full float precision (`f"{seconds} seconds"`), so
    nothing is lost — but it is a string, and every reader here wants the float the
    binary template carried."""
    if raw is None:
        return 0.0
    if isinstance(raw, (int, float)):
        return float(raw)
    m = re.match(r'\s*(-?[0-9.eE+-]+)', str(raw))
    return float(m.group(1)) if m else 0.0


@dataclass
class ExportTemplate:
    """One `effects[].templates[]` entry of an exported power."""
    attribs: list[str]
    aspect: str
    target: str
    table: str
    scale: float
    duration: float
    magnitude: float
    params: dict | None


@dataclass
class ExportEffectGroup:
    """One `effects[]` entry. `_filtered_own` clones these through `__dict__`, so this
    stays a plain dataclass (no `slots=True`)."""
    chance: float
    ppm: float
    tags: list[str]
    templates: list[ExportTemplate] = field(default_factory=list)


@dataclass
class ExportPower:
    """A `Boosts.*` / `Set_Bonus.*` power template out of `exported_powers/`."""
    full_name: str
    boosts_allowed: list[str]
    activate_period: float
    effects: list[ExportEffectGroup] = field(default_factory=list)


@dataclass
class ExportBoostList:
    boosts: list[str]


@dataclass
class ExportBoostSet:
    name: str
    group_name: str
    boostlists: list[ExportBoostList] = field(default_factory=list)


def load_boostsets(base: Path = EXPORT_ROOT) -> list[ExportBoostSet]:
    """The fork's boost sets. `group_name` arrives already resolved — the binary stores it
    as a message-store key (`"P2234567739"`) and the exporter resolves it, which is the one
    and only thing `clientmessages-en.bin` was ever opened for here."""
    return [
        ExportBoostSet(
            name=s['name'],
            group_name=s.get('group_name') or '',
            boostlists=[ExportBoostList(boosts=list(bl.get('boosts') or []))
                        for bl in (s.get('boostlists') or [])],
        )
        for s in json.loads((base / 'boostsets.json').read_text(encoding='utf-8'))
    ]


def load_boost_powers(base: Path = EXPORT_ROOT) -> list[ExportPower]:
    """Every `Boosts.*` and `Set_Bonus.*` power template, which is every power this script
    looks up: a set's pieces come off `boostlists[].boosts` and a piece's `Grant_Power`
    redirect targets a `Set_Bonus.Global_Bonus.*`. No player power is ever asked for."""
    out: list[ExportPower] = []
    for tree in ('boosts', 'set_bonus'):
        tree_dir = base / tree
        if not tree_dir.is_dir():
            continue
        for path in sorted(tree_dir.glob('*/*.json')):
            if path.name == 'index.json':
                continue
            d = json.loads(path.read_text(encoding='utf-8'))
            out.append(ExportPower(
                full_name=d['full_name'],
                boosts_allowed=list(d.get('boosts_allowed') or []),
                activate_period=float(d.get('activate_period') or 0.0),
                effects=[
                    ExportEffectGroup(
                        chance=float(eg.get('chance') or 0.0),
                        ppm=float(eg.get('ppm') or 0.0),
                        tags=list(eg.get('tags') or []),
                        templates=[
                            ExportTemplate(
                                attribs=list(tpl.get('attribs') or []),
                                aspect=tpl.get('aspect') or '',
                                target=tpl.get('target') or '',
                                table=tpl.get('table') or '',
                                scale=float(tpl.get('scale') or 0.0),
                                duration=_seconds(tpl.get('duration')),
                                magnitude=float(tpl.get('magnitude') or 0.0),
                                params=tpl.get('params') or None,
                            )
                            for tpl in (eg.get('templates') or [])
                        ],
                    )
                    for eg in (d.get('effects') or [])
                ],
            ))
    return out


def load_proc_damage_table(base: Path = EXPORT_ROOT) -> list[float]:
    """`Melee_ProcDamage`, the per-level table a damage proc's scale multiplies.

    This is a CLASS table and the classes do not agree: 62 of the export's 71 archetypes
    carry the player table (L1 -10, L50 -107.09) and nine carry harder ones for boss and
    elite ranks. The binary read took whichever class `classes.bin` happened to list
    first, which is not a rule anyone would choose on purpose. Take the value the majority
    of archetypes state instead, and stop if there is no unique majority rather than pick
    a class — a damage proc would then need one named, and that is a decision, not a
    tie-break.
    """
    seen: Counter[tuple[float, ...]] = Counter()
    tables: dict[tuple[float, ...], list[float]] = {}
    for path in sorted((base / 'tables').glob('*.json')):
        table = (json.loads(path.read_text(encoding='utf-8')).get('named_tables') or {}
                 ).get('Melee_ProcDamage')
        if table:
            seen[tuple(table)] += 1
            tables[tuple(table)] = table
    if not seen:
        raise SystemExit(f'no archetype in {base / "tables"} carries a Melee_ProcDamage table')
    ranked = seen.most_common(2)
    if len(ranked) > 1 and ranked[0][1] == ranked[1][1]:
        raise SystemExit(f'Melee_ProcDamage has no majority ({ranked[0][1]} archetypes each '
                         f'state two different tables) — name the class to read it from')
    return tables[ranked[0][0]]


# ---------------------------------------------------------------------
# Binary (attrib, aspect) -> structured ProcEffect {category, mult}.
# `category` matches ProcEffectCategory in proc-data.ts. `mult` converts the
# binary scale to the displayed value (×100 for %, ×10 for max HP, abs for KB).
# ---------------------------------------------------------------------
DMG_ATTRIBS = {'Smashing_Dmg', 'Lethal_Dmg', 'Fire_Dmg', 'Cold_Dmg',
               'Energy_Dmg', 'Negative_Energy_Dmg', 'Toxic_Dmg', 'Psionic_Dmg'}
DEF_ATTRIBS = {'Melee', 'Ranged', 'Area', 'Smashing', 'Lethal', 'Fire', 'Cold',
               'Energy', 'Negative_Energy', 'Psionic', 'Toxic', 'Base_Defense'}
MEZ_ATTRIBS = {'Held', 'Stunned', 'Sleep', 'Confused', 'Terrorized', 'Immobilized'}

# Marker/meta attribs that never carry a proc's real effect. Includes the
# labels the special-attrib sub-index fix split out of the old merged decode
# (Grant_Power→Revoke_Power; Create_Entity→Silent_Kill/Translucency/
# Clear_Damagers; Set_Mode→Set_Costume/XPDebtProtection) so the split
# doesn't leak markers past the skip.
MARKER_ATTRIBS = {'Null', 'Grant_Power', 'Revoke_Power', 'Create_Entity',
                  'Silent_Kill', 'Translucency', 'Clear_Damagers',
                  'Set_Mode', 'Set_Costume', 'XPDebtProtection'}

ATTRIB_ASPECT_TO_EFFECT = {
    ('RechargeTime', 'Strength'): ('Recharge', 100.0),
    ('RunningSpeed', 'Current'):  ('RunSpeed', 100.0),
    ('RunningSpeed', 'Strength'): ('RunSpeed', 100.0),
    ('ToHit', 'Current'):         ('ToHit', 100.0),
    ('ToHit', 'Strength'):        ('ToHit', 100.0),
    ('Recovery', 'Current'):      ('Recovery', 100.0),
    ('Endurance', 'Strength'):    ('Recovery', 100.0),
    ('Endurance', 'Current'):     ('Endurance', 100.0),
    ('Regeneration', 'Current'):  ('Regeneration', 100.0),
    ('HitPoints', 'Strength'):    ('Regeneration', 100.0),
    ('HitPoints', 'Maximum'):     ('MaxHP', 10.0),
    ('Heal_Dmg', 'Absolute'):     ('Heal', 100.0),
    ('Absorb', 'Maximum'):        ('Absorb', 100.0),
    ('Absorb', 'Current'):        ('Absorb', 100.0),
    ('PerceptionRadius', 'Current'): ('Perception', 100.0),
    ('Taunt', 'Resistance'):      ('Debuff', 100.0),
    ('RunningSpeed', 'Resistance'): ('SlowResistance', 100.0),
    ('FlyingSpeed', 'Resistance'):  ('SlowResistance', 100.0),
    ('RechargeTime', 'Resistance'): ('RechargeResistance', 100.0),
}

_BRIDGE_CLI = PROJECT_ROOT / 'scripts' / 'bridge-attrib-one.cjs'


@lru_cache(maxsize=1024)
def _bridge_attrib(attrib: str, aspect: str, table: str = '') -> dict:
    req = json.dumps({'attrib': attrib, 'aspect': aspect, 'table': table})
    out = subprocess.check_output(
        ['node', str(_BRIDGE_CLI), req],
        cwd=PROJECT_ROOT,
        text=True,
    )
    return json.loads(out)


def _proc_effect_from_bridge(attrib: str, aspect: str, table: str = '') -> tuple[str, float, str | None] | None:
    # Explicit overrides where proc categories differ from bridge effect types.
    if (attrib, aspect) == ('RunningSpeed', 'Resistance'):
        return ('SlowResistance', 100.0, None)
    if (attrib, aspect) == ('FlyingSpeed', 'Resistance'):
        return ('SlowResistance', 100.0, None)
    if (attrib, aspect) == ('RechargeTime', 'Resistance'):
        return ('RechargeResistance', 100.0, None)

    br = _bridge_attrib(attrib, aspect, table)
    et = br.get('effectType')
    # FINDING (2026-09-26). The bridge answers with a subType and this
    # function drops it. The return is a 3-tuple whose third slot IS the subtype slot,
    # and exactly one branch below fills it — the Resistance branch, which derives the
    # damage type by string-editing `attrib` (`attrib.replace('_Dmg', '')`) rather than
    # reading the value fetched here. So there are two ways to name a subtype and the
    # authoritative one is the unused one. Whether the two ever disagree is a question
    # for the oracle, not for a linter; left in place and named rather than deleted.
    sub = br.get('subType')  # noqa: F841

    if et == 'RechargeTime':
        return ('Recharge', 100.0, None)
    if et == 'ToHit':
        return ('ToHit', 100.0, None)
    if et == 'Recovery':
        return ('Recovery', 100.0, None)
    if et == 'Endurance':
        return ('Endurance', 100.0, None)
    if et == 'Regeneration':
        return ('Regeneration', 100.0, None)
    if et == 'MaxHP':
        return ('MaxHP', 10.0, None)
    if et == 'Absorb':
        return ('Absorb', 100.0, None)
    if et == 'Heal':
        return ('Heal', 100.0, None)
    if et == 'Perception':
        return ('Perception', 100.0, None)
    if et == 'Movement':
        # Each movement axis maps to its OWN proc stat. Collapsing them all onto
        # RunSpeed read Launch's +Jump Height as +Run Speed.
        #
        # Only the aspect=Current *buff* templates count. These globals pair the
        # buff with an aspect=Maximum template that raises the axis CAP (Launch's
        # +Max Jump Height = 10.0 x Melee_Ones = +1000%), which the planner models
        # as a power's movementCapBump, not as a buff — emitting it here would read
        # Launch as +1200% jump height. (The AT-table factor is applied by the
        # single-attrib caller, which is the only one that knows the table.)
        if aspect != 'Current':
            return None
        if attrib == 'RunningSpeed':
            return ('RunSpeed', 100.0, None)
        if attrib == 'JumpingSpeed':
            return ('JumpSpeed', 100.0, None)
        if attrib == 'FlyingSpeed':
            return ('FlySpeed', 100.0, None)
        if attrib == 'JumpHeight':
            return ('JumpHeight', 100.0, None)
        return None
    if et == 'Resistance' and attrib.endswith('_Dmg'):
        return ('Resistance', 100.0, attrib.replace('_Dmg', ''))

    # Preserve existing behavior if the bridge yields no handled mapping.
    base = ATTRIB_ASPECT_TO_EFFECT.get((attrib, aspect))
    if base:
        return (base[0], base[1], None)
    return None

# Boost-piece Null-marker `tags` -> the effect category it stands for. Used to
# pick the right Global_Bonus power per piece in multi-global sets (Steadfast
# Def vs KB, Shield Wall Res vs Teleport).
TAG_TO_CATEGORY = {
    'Defense': 'Defense', 'Knock': 'KnockbackProtection', 'Res': 'Resistance',
    'rechargetime': 'Recharge', 'Movement': 'RunSpeed', 'Endurance': 'Recovery',
    'Heal': 'Heal', 'ToHit': 'ToHit',
}

# Boost-set categories whose pieces are only slottable in a SUMMON power, so the
# game copies the boosts onto the summoned pet and the pet is what carries the proc.
# Such a proc's templates say `target: Self` — but "self" there is the PET, not the
# player, so every effect they resolve to has to be stamped `target: 'pets'` for the
# player-dashboard passes to skip it.
#
# The set's slot heading is the ONLY discriminator the binary offers: Soulbound
# Allegiance, Decimation and Gaussian's all grant the byte-identical
# `Set_Bonus.Global_Bonus.Boost_Up` power through a byte-identical `Grant_Power`
# template — nothing inside the proc piece distinguishes the pet-only Build Up from
# the two self ones.
#
# That heading is `GroupName`, not `Category`. `Category` is blank on every PvP,
# purple, event and ATO record (31 of Homecoming's 227), so keying on it dropped the
# stamp for Soulbound Allegiance — a purple set — and its pet Build Up leaked into
# the player's totals. BOOST-3 established the same field as the answer after the
# same blank bit a resist set's aspect label; this was one of the sites it named and
# did not move. `GroupName` is a message-store hash, hence the resolve in `main`.
PET_CARRIED_GROUPS = {
    'Pet Damage',                  # Soulbound Allegiance, Sovereign Right, Blood Mandate…
    'Recharge Intensive Pets',     # Call to Arms, Expedient Reinforcement
    'Mastermind Archetype Sets',   # the MM ATOs (henchman summons only)
}

# Globals the binary can't express as a plain (attrib, scale) — value comes from
# an HP-scaling expression or special mechanic. Hand-override to match the current
# planner behaviour (parity); revisit when the scaling model is improved.
SCALING_OVERRIDES = {
    # Reactive Defenses / Preventive Medicine: scaling +Res 3%–12.9% (planner
    # applies the 3% floor via this structured `scaling` effect).
    'reactivedefenses': [{'category': 'Resistance', 'value': 3.0, 'effectType': 'All', 'scaling': True}],
    'preventivemedicine': [{'category': 'Absorb', 'value': 20.0, 'scaling': True}],
    # Kheldian's Grace ATO global — the only SELF global among the ATO passive-
    # global 6th pieces (the rest buff pets, which the player calc skips). Its
    # Set_Bonus.Global_Bonus power resolves cleanly via structured_effects() but
    # the per-piece resolve_piece() walk misses it (ATO boost pieces don't carry
    # the Null/Grant_Power marker the walk expects), so hand-provide the SELF
    # Res(All) + Max HP. Values binary-sourced from Set_Bonus.Global_Bonus.
    # [Superior_]Kheldians_Grace (2026-06-18); the set's +Dmg-to-pets component is
    # omitted (pet target → calc-skipped; the binary's ×250 damage-proc heuristic
    # mislabels it anyway).
    'kheldiansgrace':         [{'category': 'Resistance', 'value': 3.5, 'effectType': 'All'},
                               {'category': 'MaxHP', 'value': 7.5}],
    'superiorkheldiansgrace': [{'category': 'Resistance', 'value': 5.0, 'effectType': 'All'},
                               {'category': 'MaxHP', 'value': 10.0}],
}


def _effect_type_for_defense(attribs: set[str]) -> str:
    return 'All' if DEF_ATTRIBS.issubset(attribs) or len(attribs & DEF_ATTRIBS) >= 8 else \
        '/'.join(sorted(attribs & DEF_ATTRIBS))


# Movement modifier tables for a PLAYER archetype are AT- and level-invariant
# constants (run 3.5, fly 1.365, jump 2.49, leap 27.8; the PET tables differ, but
# pets don't slot player IOs). The single-attrib mapping below turns a global's
# binary scale into a displayed value with a plain x100, which silently ASSUMES the
# template's table is Melee_Ones (value 1.0). That under-counts every movement global
# whose real table is a speed table: Thrust's "+Run Speed" is 0.1 x Melee_SpeedRunning
# = 35%, not the 10% a bare x100 yields. (Swift is the same 0.1 x Melee_SpeedRunning
# structure and reads 35% in game.) Multiplying by the table factor fixes those and is
# a no-op for a genuinely flat Melee_Ones global.
_MOVEMENT_TABLE_FACTOR = {
    'melee_speedrunning': 3.5,
    'melee_speedflying': 1.365,
    'melee_speedjumping': 2.49,
    'melee_leap': 27.8,
}


def _group_effects(eg) -> list[dict]:
    """Structured effects for ONE effect group (target/chance stamped by caller)."""
    out: list[dict] = []
    attset = set()
    # Table per (attrib, aspect, scale), captured alongside attset so the single-attrib
    # mapping below can resolve the real AT-table factor instead of assuming Melee_Ones.
    tbl_of: dict[tuple, str] = {}
    for t in eg.templates:
        for a in (t.attribs or []):
            key = (a, t.aspect, round(t.scale, 5))
            attset.add(key)
            tbl_of[key] = t.table or ''
    if not attset:
        return out
    attribs = {a for a, _, _ in attset}
    if DMG_ATTRIBS.issubset(attribs):
        asp = next(iter({asp for a, asp, _ in attset if a in DMG_ATTRIBS}))
        sc = next(iter({s for a, _, s in attset if a in DMG_ATTRIBS}))
        if asp == 'Resistance':
            out.append({'category': 'Resistance', 'value': round(abs(sc) * 100, 4), 'effectType': 'All'})
        elif asp == 'Strength':
            out.append({'category': 'Damage', 'value': round(abs(sc) * 250, 4), 'effectType': 'All'})
        return out
    if DEF_ATTRIBS & attribs:
        sc = next(iter({s for a, _, s in attset if a in DEF_ATTRIBS}))
        out.append({'category': 'Defense', 'value': round(abs(sc) * 100, 4),
                    'effectType': _effect_type_for_defense(attribs)})
        return out
    if MEZ_ATTRIBS.issubset(attribs):
        sc = next(iter({s for a, _, s in attset if a in MEZ_ATTRIBS}))
        out.append({'category': 'MezResist', 'value': round(abs(sc) * 100, 4), 'effectType': 'All'})
        return out
    if attribs & {'StealthRadius_PVE', 'StealthRadius_PVP'}:
        pve = next((s for a, _, s in attset if a == 'StealthRadius_PVE'), None)
        pvp = next((s for a, _, s in attset if a == 'StealthRadius_PVP'), None)
        ef = {'category': 'Stealth', 'value': round(pve if pve is not None else pvp, 4)}
        if pvp is not None:
            ef['valueMax'] = round(pvp, 4)
        out.append(ef)
        return out
    if attribs & {'Knockback', 'Knockup'}:
        sc = next(iter({s for a, _, s in attset if a in ('Knockback', 'Knockup')}))
        out.append({'category': 'KnockbackProtection', 'value': round(abs(sc), 4)})
        return out
    # single-attrib mapped effects — emit ALL distinct categories in the group
    # (Winter's Gift: SlowResistance AND RechargeResistance), not just the first.
    seen_cats: set[str] = set()
    for a, asp, sc in sorted(attset):
        mapped = _proc_effect_from_bridge(a, asp)
        if not mapped:
            continue
        cat, mult, eff_type = mapped
        if cat not in seen_cats:
            # Resolve the movement AT-table factor (a no-op for Melee_Ones and for
            # every non-speed table) so e.g. Thrust's +Run Speed lands at 35%, not 10%.
            tfac = _MOVEMENT_TABLE_FACTOR.get(
                tbl_of.get((a, asp, round(sc, 5)), '').lower(), 1.0)
            eff = {'category': cat, 'value': round(abs(sc) * mult * tfac, 4)}
            if eff_type:
                eff['effectType'] = eff_type
            out.append(eff)
            seen_cats.add(cat)
    if not out:
        out.append({'category': 'Special', 'raw': sorted(str(x) for x in attset)})
    return out


def structured_effects(power: ExportPower) -> list[dict]:
    """Map a power's templates -> ProcEffect dicts, stamping target/chance so the
    consumer can exclude pet/ally buffs (target!=Self) and chance-gated procs."""
    out: list[dict] = []
    for eg in power.effects:
        effs = _group_effects(eg)
        if not effs:
            continue
        target = next((t.target for t in eg.templates if t.target), 'Self')
        chance = round(eg.chance, 4)
        for ef in effs:
            if target and target != 'Self':
                ef['target'] = 'pets'
            if chance < 0.999:
                ef['chance'] = chance
        out.extend(effs)
    return out


def _buff_duration(gp: ExportPower) -> float | None:
    """The lifetime of a granted buff power's effect (max real-attrib template
    duration). A stacking-buff proc (Unrelenting Fury) grants a power whose +Regen
    lasts this long; the granting Grant_Power template's own duration is only the
    trigger dwell. Ignores marker templates (Grant_Power/Null/etc.)."""
    ds = [round(t.duration, 2) for eg in gp.effects for t in eg.templates
          if t.duration and t.attribs
          and t.attribs[0] not in MARKER_ATTRIBS]
    return max(ds) if ds else None


def _filtered_own(piece: ExportPower) -> ExportPower:
    """A view of the piece with only non-enhancement, non-marker templates."""
    groups = []
    for eg in piece.effects:
        # Exclude markers and ALL enhancement aspects (aspect=Strength, positive
        # scale) — a global's real effect uses Current/Absolute/Resistance/Maximum.
        # (Damage PROCS, which DO use Strength/Absolute damage, are a later phase.)
        keep = [t for t in eg.templates if t.attribs
                and t.attribs[0] not in MARKER_ATTRIBS
                and not (t.aspect == 'Strength' and t.scale > 0.001)]
        if keep:
            eg2 = type(eg).__new__(type(eg))
            eg2.__dict__.update(eg.__dict__)
            eg2.templates = keep
            groups.append(eg2)
    pv = type(piece).__new__(type(piece))
    pv.__dict__.update(piece.__dict__)
    pv.effects = groups
    return pv


def resolve_piece(piece: ExportPower, set_name: str, gb_index: dict[str, ExportPower]) -> tuple[list[dict], str]:
    """Resolve ONE global piece's effect. Tag-aware Global_Bonus selection for
    multi-global sets. Returns (effects, source)."""
    # collect markers (Null presence + tags) and explicit Grant_Power redirects
    grant_targets: list[str] = []
    marker_tags: list[str] = []
    has_null = False
    for eg in piece.effects:
        for t in eg.templates:
            if t.params and t.params.get('power_names'):
                grant_targets += [p for p in t.params['power_names'] if 'Bonus' in p]
            if t.attribs and t.attribs[0] == 'Null':
                has_null = True
                marker_tags += (eg.tags or [])
    # 1) explicit Grant_Power redirect
    for tgt in grant_targets:
        gp = gb_index.get(tgt) or gb_index.get(tgt.split('.')[-1])
        if gp:
            return structured_effects(gp), f'param->{tgt.split(".")[-1]}'
    # The piece's OWN non-enhancement effects (e.g. Impervious Skin's +Regen
    # rides alongside the Null marker that grants the mez-resist global).
    own = structured_effects(_filtered_own(piece))
    # 2) Null marker (even with empty tags) -> Global_Bonus by naming,
    #    disambiguated by the marker tag when present. Combine with own effects.
    if has_null:
        want = next((TAG_TO_CATEGORY[t] for t in marker_tags if t in TAG_TO_CATEGORY), None)
        cands = sorted((name for name in gb_index
                        if name == name.split('.')[-1]  # short keys only
                        and sid(name).startswith(sid(set_name)) and 'teleport' not in name.lower()),
                       key=len)
        chosen = None
        for c in cands:
            effs = structured_effects(gb_index[c])
            if effs and (want is None or effs[0]['category'] == want):
                chosen = (effs, c)
                break
        if chosen is None and cands:
            chosen = (structured_effects(gb_index[cands[0]]), cands[0])
        if chosen:
            gb_effs, c = chosen
            seen = {ef['category'] for ef in gb_effs}
            combined = gb_effs + [ef for ef in own if ef['category'] not in seen and ef['category'] != 'Special']
            return combined, f'name->{c}'
    # 3) own real-attrib templates (Kismet, Miracle, Numina, stealth, travel)
    return (own, 'own-templates') if own else ([], 'UNRESOLVED')


def resolve_set_global_effects(s, gb_index, pidx) -> dict[str, tuple[int, list[dict], str]]:
    """Resolve every global piece in a set -> {primary_category: (piece#, effects, src)}."""
    by_cat: dict[str, tuple[int, list[dict], str]] = {}
    for i, bl in enumerate(s.boostlists):
        pp = next((pidx[b] for b in bl.boosts if b in pidx), None)
        if not pp:
            continue
        effs, src = resolve_piece(pp, s.name, gb_index)
        # Register the piece under EACH of its effect categories so a hand entry
        # can find it by its primary category (e.g. Impervious Skin by Regeneration).
        for ef in (effs or []):
            by_cat.setdefault(ef['category'], (i + 1, effs, src))
    return by_cat


def resolve_damage_proc(s, pidx, l1: float, l50: float) -> list[dict] | None:
    """Find the set's Melee_ProcDamage damage template -> a Damage ProcEffect.

    Proc damage = scale × Melee_ProcDamage[level]; the displayed N–M range is the
    level-1..level-50 damage (|L1|≈10, |L50|≈107). Reproduces the hand 'Damage(Type
    N - M)' and corrects its inconsistencies (some ATO procs were entered flat).
    """
    def rnd(x):
        # 2 decimals — proc damage displays to 2dp (e.g. 71.75, not 72). The
        # parity test rounds to int when comparing against the hand 'N - M'
        # mechanics strings.
        return round(x, 2)
    for bl in s.boostlists:
        pp = next((pidx[b] for b in bl.boosts if b in pidx), None)
        if not pp:
            continue
        for eg in pp.effects:
            for t in eg.templates:
                if t.table == 'Melee_ProcDamage' and t.attribs and t.attribs[0].endswith('_Dmg'):
                    sc = abs(round(t.scale, 5))
                    dtype = t.attribs[0][:-4].replace('_', ' ')  # Negative_Energy_Dmg -> Negative Energy
                    return [{'category': 'Damage', 'value': rnd(sc * l1),
                             'valueMax': rnd(sc * l50), 'effectType': dtype}]
    return None


# Mez attrib -> display label (proc payload). magnitude = template.magnitude,
# duration = template.scale (mez procs encode duration-seconds in scale).
MEZ_LABEL = {'Held': 'Hold', 'Stunned': 'Stun', 'Sleep': 'Sleep', 'Confused': 'Confuse',
             'Terrorized': 'Fear', 'Immobilized': 'Immobilize', 'Placate': 'Placate'}


def resolve_proc_payload(piece: ExportPower, gb_index: dict[str, ExportPower]) -> list[dict]:
    """Map a non-global proc piece's PAYLOAD (the chance/ppm effect group) to
    structured ProcEffects: self-buffs (Endurance/Heal/Recovery/Regen/Absorb),
    foe debuffs (-Res/-ToHit/-Recharge -> Debuff), mez/knock (-> Control), and
    Build Up (Grant_Power -> Boost_Up). Skips enhancement aspects and damage
    (handled by resolve_damage_proc)."""
    out: list[dict] = []
    seen: set[str] = set()
    for eg in piece.effects:
        for t in eg.templates:
            if not t.attribs:
                continue
            a, asp, sc = t.attribs[0], t.aspect, round(t.scale, 5)
            # Grant_Power / Null redirects to a Global_Bonus power.
            if a in ('Grant_Power', 'Null') and t.params:
                names = t.params.get('power_names', [])
                # Build Up: Boost_Up grants the standard +100% Dam / +15% ToHit, 10s.
                if any('Boost_Up' in p for p in names):
                    d = round(t.duration, 2) or 10.0
                    out += [{'category': 'Damage', 'value': 100.0, 'effectType': 'All', 'duration': d},
                            {'category': 'ToHit', 'value': 15.0, 'duration': d}]
                    return out
                # Otherwise resolve the granted Global_Bonus (Force Feedback +Rech,
                # Unrelenting Fury +Regen, …). The buff's LIFETIME lives on the
                # granted power's own templates (e.g. UF's +15% Regen lasts 10.25s
                # and Stacks); the outer Grant_Power template's duration (~0.5s) is
                # just the trigger's dwell and must NOT be used as the buff duration
                # — a stacking-buff proc's steady-state contribution depends on the
                # real lifetime. Fall back to the trigger duration only when the
                # grant target carries none.
                for tgt in names:
                    gp = gb_index.get(tgt) or gb_index.get(tgt.split('.')[-1])
                    if not gp:
                        continue
                    gp_dur = _buff_duration(gp)
                    for ef in structured_effects(gp):
                        # Skip Special and Damage: a +Damage% buff-stack granted via
                        # this path (Ascendancy of the Dominator) is bespoke — the
                        # damage-proc ×250 heuristic and pet-target stamp don't apply.
                        # Clean grant-globals are Recharge/Defense/etc. (Force Feedback).
                        if ef['category'] in ('Special', 'Damage'):
                            continue
                        dur = gp_dur if gp_dur else (round(t.duration, 2) if t.duration else None)
                        if dur:
                            ef['duration'] = dur
                        ck = ef['category'] + str(ef.get('effectType', '')) + str(ef.get('value'))
                        if ck not in seen:
                            seen.add(ck)
                            out.append(ef)
                continue
            # skip markers, damage (3a), and enhancement aspects
            if a in MARKER_ATTRIBS or t.table == 'Melee_ProcDamage':
                continue
            if asp == 'Strength' and sc > 0.001 and not a.endswith('_Dmg'):
                continue
            dur = round(t.duration, 2) or None
            ef = None
            # Mez (magnitude + duration-in-scale)
            if a in MEZ_LABEL and asp == 'Current':
                ef = {'category': 'Control', 'value': round(t.magnitude, 3),
                      'effectType': MEZ_LABEL[a], 'duration': round(sc, 2) or dur}
            elif a in ('Knockback', 'Knockup') and asp == 'Current':
                kind = 'Knockdown' if abs(sc) < 1 else 'Knockback'
                ef = {'category': 'Control', 'value': abs(sc), 'effectType': kind}
            # Foe debuffs (negative scale)
            elif sc < 0 and a.endswith('_Dmg') and asp == 'Resistance':
                ef = {'category': 'Debuff', 'value': round(sc * 100, 4), 'effectType': 'Resistance', 'duration': dur}
            elif sc < 0 and a == 'ToHit':
                ef = {'category': 'Debuff', 'value': round(sc * 100, 4), 'effectType': 'ToHit', 'duration': dur}
            elif sc < 0 and a == 'RechargeTime':
                ef = {'category': 'Debuff', 'value': round(sc * 100, 4), 'effectType': 'Recharge', 'duration': dur}
            elif sc < 0 and a in ('Recovery', 'Endurance') and asp == 'Current':
                # The % end-drain is the Current-aspect template; skip the paired
                # Absolute/Melee_EndDrain template (a flat magnitude, not a %).
                ef = {'category': 'Debuff', 'value': round(sc * 100, 4), 'effectType': 'Recovery', 'duration': dur}
            # Self buffs
            elif a == 'Endurance' and asp == 'Current':
                ef = {'category': 'Endurance', 'value': round(sc * 100, 4)}
            elif a == 'Recovery' and asp == 'Current':
                ef = {'category': 'Recovery', 'value': round(sc * 100, 4)}
            elif a == 'Regeneration' and asp == 'Current':
                ef = {'category': 'Regeneration', 'value': round(sc * 100, 4)}
            elif a == 'Heal_Dmg':
                ef = {'category': 'Heal', 'value': round(sc * 10, 2)}  # ~% of HP (display; dashboard skips Heal)
            elif a == 'Absorb':
                ef = {'category': 'Absorb', 'value': round(sc * 100, 4)}
            if ef is None:
                continue
            # Foe-vs-self keys off the effect KIND, not the target field
            # ('AnyAffected' means the caster for beneficial procs, the enemy for
            # harmful ones). Debuff/Control are applied to the foe.
            if ef['category'] in ('Debuff', 'Control'):
                ef['target'] = 'foe'
            key = ef['category'] + str(ef.get('effectType', '')) + str(ef.get('value'))
            if key not in seen:
                seen.add(key)
                out.append(ef)
    return out


@lru_cache(maxsize=1)
def hand_entries() -> tuple[dict, ...]:
    """The authored proc table, in file order, one dict per entry with its key folded in.

    This was five regexes over `src/data/proc-data.ts`, each anchored on the entry's exact
    FIELD ORDER, so a field added, removed or reordered dropped rows in silence rather than
    failing — and a `check_hand_shape` pass existed to catch that by counting. The data is
    `hand-data/proc-data.json` as of 2026-09-25 and the readers below are filters over it,
    so there is nothing left to drop and nothing left to count. The underscore keys are the
    file's own prose (`_comment`, `_groups`, `_notes`); no proc key starts with one.

    One entry was reached by two of those five regexes and not the other three, an authored
    comment sitting between its `ioName` and its `ppm` where they expected the next field:
    `Superior Overpowering Presence: Recharge/Chance for Energy Font`, which is why
    `proc-activate-period` carried it and `proc-effects`, `proc-ppm` and `proc-boosts-allowed`
    did not. It was held out of the JSON readers for one commit so that its diff would be
    reviewable alone, and is in as of 2026-09-25.
    """
    data = json.loads(HAND_PROC_DATA.read_text(encoding='utf-8'))
    return tuple({'key': k, **v} for k, v in data.items() if not k.startswith('_'))


def parse_hand_damage() -> list[dict]:
    """The authored Damage entries (key + setName)."""
    return [{'key': e['key'], 'setName': e['setName']} for e in hand_entries()
            if re.match(r'Damage\s*\(', e['mechanics'])]


def infer_category(mech: str) -> str:
    """Infer a hand entry's PRIMARY effect category from its mechanics string."""
    m = mech.lower()
    if 'knock' in m and ('protection' in m or 'mag' in m):
        return 'KnockbackProtection'
    if 'resist(' in m and 'speed' in m:           # Winter's Gift slow/recharge resist
        return 'SlowResistance'
    if 'mez prot' in m:
        return 'Special'
    if 'defense' in m or '+def' in m:
        return 'Defense'
    if 'resistance' in m or '+res' in m:
        return 'Resistance'
    if 'maximum hit points' in m or 'max hp' in m:
        return 'MaxHP'
    if 'recharge' in m:
        return 'Recharge'
    if 'recovery' in m:
        return 'Recovery'
    if 'regeneration' in m:
        return 'Regeneration'
    if 'run speed' in m or 'runspeed' in m:
        return 'RunSpeed'
    if 'tohit' in m:
        return 'ToHit'
    if 'heal' in m or 'health' in m:
        return 'Heal'
    if 'absorb' in m or 'absorption' in m:
        return 'Absorb'
    if 'stealth' in m:
        return 'Stealth'
    if 'perception' in m:
        return 'Perception'
    return 'Special'


def parse_hand_other() -> list[dict]:
    """The authored non-global, non-damage Proc entries."""
    return [{'key': e['key'], 'setName': e['setName'], 'ioName': e['ioName'],
             'mechanics': e['mechanics']}
            for e in hand_entries()
            if e['type'] == 'Proc' and not re.match(r'Damage\s*\(', e['mechanics'])]


def infer_proc_category(mech: str) -> str:
    """Infer a non-global proc entry's primary structured category from mechanics."""
    m = mech.lower()
    if m.startswith('foe('):
        if '-resist' in m: return 'Debuff:Resistance'
        if '-tohit' in m or '-to hit' in m: return 'Debuff:ToHit'
        if '-rech' in m: return 'Debuff:Recharge'
        if '-recovery' in m or '-end' in m: return 'Debuff:Recovery'  # -Endurance is a Recovery debuff
        # Knockdown is the same family as Knockback but the binary mag (<1)
        # decides; check 'knockdown' FIRST since "Knockback Mag .67 = Knockdown"
        # contains both words.
        if 'knockdown' in m: return 'Control:Knockdown'
        if 'knockback' in m: return 'Control:Knockback'
        for word, label in (('disorient', 'Stun'), ('stun', 'Stun'), ('hold', 'Hold'),
                            ('immobiliz', 'Immobilize'), ('sleep', 'Sleep'), ('confus', 'Confuse'),
                            ('terror', 'Fear'), ('fear', 'Fear'), ('placate', 'Placate')):
            if word in m:
                return f'Control:{label}'
        return 'Control'
    if 'build up' in m or 'buildup' in m: return 'Damage'  # Build Up grants damage+tohit
    if 'endurance' in m: return 'Endurance'
    if 'absorb' in m or 'absorption' in m: return 'Absorb'
    if 'heal' in m: return 'Heal'
    if 'recovery' in m: return 'Recovery'
    if 'regener' in m: return 'Regeneration'
    return ''


# PROC_DATABASE is one cross-server table, but the bins this script reads are
# Homecoming's, so a fork-UNIQUE set (Witchcraft, Endless Nightmare) resolves to no
# binary set here. Those forks' own committed exports carry the same two structures —
# `boostsets.json` with the identical `boostlists[].boosts` shape, and a boost power
# per directory — so the fallback reads them rather than leaving the entry unsourced.
# Brainstorm joined the fallback pool 2026-08-22. Measured: all six generated files come out
# byte-identical, and `activate period` still reports 0 sets disagreeing — so this fork's boost
# tree agrees with the others everywhere the fallback reaches, and adds no conflict for
# `export_proc_boosts_by_set` to drop a set over. The roster is complete so a future divergence
# surfaces as a diff instead of never being asked about.
EXPORT_FORKS = {'homecoming': '', 'rebirth': 'rebirth', 'thunderspy': 'thunderspy',
                'brainstorm': 'brainstorm'}


def _set_keys(s: dict) -> set[str]:
    """An exported set's lookup slugs: its internal `name` and its shown `display_name`."""
    return {sid(n) for n in (s['name'], s.get('display_name')) if n}


@lru_cache(maxsize=1)
def export_periods_by_set() -> dict[str, set[float]]:
    """`{set name slug: {activate_period, …}}` over every fork's committed export."""
    out: dict[str, set[float]] = {}
    for sub in EXPORT_FORKS.values():
        base = EXPORT_ROOT / sub
        sets_file = base / 'boostsets.json'
        boosts_dir = base / 'boosts'
        if not sets_file.exists() or not boosts_dir.is_dir():
            continue
        periods: dict[str, float] = {}
        for entry in boosts_dir.iterdir():
            record = entry / f'{entry.name}.json'
            if not record.exists():
                continue
            power = json.loads(record.read_text(encoding='utf-8'))
            if 'full_name' in power and 'activate_period' in power:
                periods[power['full_name'].lower()] = float(power['activate_period'])
        for s in json.loads(sets_file.read_text(encoding='utf-8')):
            found = {round(periods[b.lower()], 4)
                     for bl in s.get('boostlists', []) for b in bl.get('boosts', [])
                     if b.lower() in periods}
            # A fork may file a set under an internal name and show another (Thunderspy's
            # `KB` is Subaluwa); the authored table names the shown one, so key both.
            if found:
                for key in _set_keys(s):
                    out.setdefault(key, set()).update(found)
    return out


@lru_cache(maxsize=1)
def export_proc_boosts_by_set() -> dict[str, list[str]]:
    """`{set name slug: the max-PPM piece's boosts_allowed}` over every fork's
    committed export. Forks that share a set author the same list; a set whose
    forks disagree is dropped (the caller reports it unresolved) rather than
    resolved by picking a fork."""
    picked: dict[str, tuple[float, tuple[str, ...]]] = {}
    conflicted: set[str] = set()
    for sub in EXPORT_FORKS.values():
        base = EXPORT_ROOT / sub
        sets_file = base / 'boostsets.json'
        boosts_dir = base / 'boosts'
        if not sets_file.exists() or not boosts_dir.is_dir():
            continue
        pieces: dict[str, tuple[float, tuple[str, ...]]] = {}
        for entry in boosts_dir.iterdir():
            record = entry / f'{entry.name}.json'
            if not record.exists():
                continue
            power = json.loads(record.read_text(encoding='utf-8'))
            if 'full_name' not in power:
                continue
            ppm = max((eg.get('ppm') or 0.0 for eg in power.get('effects', [])), default=0.0)
            pieces[power['full_name'].lower()] = (ppm, tuple(power.get('boosts_allowed') or ()))
        for s in json.loads(sets_file.read_text(encoding='utf-8')):
            best = max((pieces[b.lower()] for bl in s.get('boostlists', [])
                        for b in bl.get('boosts', []) if b.lower() in pieces),
                       key=lambda t: t[0], default=(0.0, ()))
            if best[0] <= 0 or not best[1]:
                continue
            for key in _set_keys(s):
                if key in picked and picked[key][1] != best[1]:
                    conflicted.add(key)
                elif key not in picked:
                    picked[key] = best
    return {k: list(v[1]) for k, v in picked.items() if k not in conflicted}


def parse_hand_entries() -> list[dict]:
    """Every authored entry, whatever its type (key + setName)."""
    return [{'key': e['key'], 'setName': e['setName']} for e in hand_entries()]


def parse_hand_ppm() -> list[dict]:
    """Every authored entry with a numeric PPM (key + setName + ppm). A Global and some
    Proc120s author `null`, which is not a PPM and never was read as one."""
    return [{'key': e['key'], 'setName': e['setName'], 'ppm': float(e['ppm'])}
            for e in hand_entries() if isinstance(e['ppm'], (int, float))]


def parse_hand_globals() -> list[dict]:
    """The authored Global / Proc120s entries (key + fields)."""
    return [{'key': e['key'], 'setName': e['setName'], 'ioName': e['ioName'],
             'ppm': 'null' if e['ppm'] is None else str(e['ppm']),
             'mechanics': e['mechanics'], 'type': e['type']}
            for e in hand_entries() if e['type'] in ('Global', 'Proc120s')]


# The authored fields, and the whole of them. A derived field appearing here would mean the
# merge `emit-contract.cjs` performs had been folded back into the authored base, which is
# the one thing the split exists to prevent — so it stops rather than emits.
AUTHORED_FIELDS = {'key', 'setName', 'ioName', 'ppm', 'mechanics', 'pvpNotes', 'type',
                   'levelRange', 'pool', 'unique'}
DERIVED_FIELDS = {'effects', 'activatePeriod', 'boostsAllowed'}


def check_hand_shape() -> None:
    entries = hand_entries()
    if not entries:
        raise SystemExit(f'{HAND_PROC_DATA} holds no proc entries')
    for e in entries:
        stray = set(e) - AUTHORED_FIELDS
        if stray & DERIVED_FIELDS:
            raise SystemExit(f'{e["key"]!r} carries the derived field(s) '
                             f'{sorted(stray & DERIVED_FIELDS)} — those are stitched on by '
                             f'emit-contract.cjs and must not be authored')
        if stray:
            raise SystemExit(f'{e["key"]!r} carries unknown field(s) {sorted(stray)}')
        missing = AUTHORED_FIELDS - set(e)
        if missing:
            raise SystemExit(f'{e["key"]!r} is missing {sorted(missing)}')
    always_on = sum(1 for e in entries if e['type'] in ('Global', 'Proc120s'))
    print(f'  shape ok: {len(entries)} entries, {always_on} always-on')


def sid(n: str) -> str:
    return re.sub(r'[^a-z0-9]', '', n.lower())


def _jsonable(value):
    """Normalise a float that is a whole number to an int.

    The six files were TypeScript and reached the contract through `JSON.stringify`, which
    writes the Number `5.0` as `5`. Python's `json` writes `5.0`. Normalising here is what
    keeps the emitted bytes the ones the contract already carries; the two `_emit_*` helpers
    that used to do this inline for PPM and ActivatePeriod are folded into it.
    """
    if isinstance(value, bool):
        return value
    if isinstance(value, float) and value == int(value):
        return int(value)
    if isinstance(value, dict):
        return {k: _jsonable(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_jsonable(v) for v in value]
    return value


# The ProcEffect field order the six TypeScript modules emitted, which is the order these
# fields land in every fork's contract. `emit-contract.cjs` writes the section with
# `JSON.stringify`, so a reordering here moves shipped bytes with no data change.
#
# The second element is how the TypeScript emitter decided whether to write the field at
# all, and the two rules are not interchangeable: `effectType`, `target` and `scaling` were
# written only when TRUTHY, so an empty string or a `False` was omitted, where a numeric
# field was written whenever it was not None (a `value` of 0 is a value). Anything not named
# here is dropped, which is what keeps the resolvers' internal `raw` key out.
EFFECT_FIELDS = (('category', 'truthy'), ('value', 'not-none'), ('valueMax', 'not-none'),
                 ('effectType', 'truthy'), ('duration', 'not-none'), ('target', 'truthy'),
                 ('chance', 'not-none'), ('scaling', 'truthy'))


def _emit_json(path: Path, const_name: str, table: dict) -> None:
    """One side table as `pipeline/_shared/<name>.json`.

    Written the way every other pipeline file is: `{"<CONST_NAME>": <table>}`, minified, no
    trailing newline, so `sharedJson(name).<CONST_NAME>` reads it. Keys sorted, as the
    TypeScript emitted them. `pipeline/` is gitignored machine output — the spec for each
    table is the comment on its call in `main`, where the producer is, and not a header in
    the output.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {const_name: {k: _jsonable(table[k]) for k in sorted(table)}}
    path.write_text(json.dumps(payload, separators=(',', ':'), ensure_ascii=False),
                    encoding='utf-8')
    print(f'Wrote {path}')


def _emit_effects_json(path: Path, const_name: str, generated: dict[str, list[dict]]) -> None:
    """An effects table, with each effect's fields in EFFECT_FIELDS order."""
    def keep(ef: dict, field: str, rule: str) -> bool:
        return bool(ef.get(field)) if rule == 'truthy' else ef.get(field) is not None

    ordered = {
        key: [{f: ef[f] for f, rule in EFFECT_FIELDS if keep(ef, f, rule)} for ef in effects]
        for key, effects in generated.items()
    }
    _emit_json(path, const_name, ordered)


def main() -> int:
    emit = '--check' not in sys.argv[1:]
    check_hand_shape()
    print(f'Loading the committed export from {EXPORT_ROOT}…')
    sets = load_boostsets()
    # `GroupName` arrives resolved (see load_boostsets); PET_CARRIED_GROUPS matches the
    # resolved heading, so nothing here reads a message-store key.
    powers = load_boost_powers()
    pidx = {p.full_name: p for p in powers}
    gb_index = {p.full_name: p for p in powers if p.full_name.startswith('Set_Bonus.Global_Bonus.')}
    for p in list(gb_index.values()):
        gb_index[p.full_name.split('.')[-1]] = p
    print(f'  {len(sets)} sets, {len(powers)} boost/set-bonus powers, '
          f'{len([k for k in gb_index if k.startswith("Set_Bonus")])} Global_Bonus powers')

    hand = parse_hand_globals()
    print(f'  {len(hand)} hand Global/Proc120s entries\n')

    # Hand setName -> the name the DATA uses. Most are binary MISSPELLINGS (HC's data
    # has the typo); aliasing lets the generator resolve the proc from the right set.
    # `superiorhaunting` is not a typo but a drift — Rebirth's export calls the set
    # `Superior_The_Haunting` where the hand table dropped the article.
    SET_ALIASES = {
        'numinasconvalescence': 'numinasconvalesence',
        'cacophony': 'cacophany',                       # binary typo
        'debilitativeaction': 'debiliativeaction',      # binary typo (missing 't')
        'ascendancyofthedominator': 'ascendencyofthedominator',          # a->e
        'superiorascendancyofthedominator': 'superiorascendencyofthedominator',
        'superiorhaunting': 'superiorthehaunting',
    }
    set_by_id = {sid(s.name): s for s in sets}
    for hk, bk in SET_ALIASES.items():
        if bk in set_by_id:
            set_by_id[hk] = set_by_id[bk]

    generated: dict[str, list[dict]] = {}
    # FINDING (2026-09-26). `n_match` and `n_diff` are initialised here,
    # never incremented anywhere, and absent from the summary line below, which reports
    # only `n_special` and `n_missing`. Read the loop and the reason is plain: it does not
    # compare. It walks the hand-written proc entries, GENERATES a replacement for each
    # from the binary, and prints it. The comparison that these two counters are named
    # for — how many of the hand-written entries the binary agrees with, and how many it
    # does not — was never written. The shape: a claim cited in a name with nothing behind it.
    # Left standing, because
    # the counters are the only surviving evidence of what this loop was meant to answer.
    rows, n_match, n_diff, n_special, n_missing = [], 0, 0, 0, 0  # noqa: F841
    for e in hand:
        set_key = sid(e['setName'])
        if set_key in SCALING_OVERRIDES:
            generated[e['key']] = SCALING_OVERRIDES[set_key]
            rows.append(f'  [override] {e["key"]}')
            continue
        s = set_by_id.get(set_key)
        if not s:
            rows.append(f'  [NO BINARY SET] {e["key"]}  ({e["mechanics"][:40]})')
            n_missing += 1
            continue
        by_cat = resolve_set_global_effects(s, gb_index, pidx)
        want = infer_category(e['mechanics'])
        pick = by_cat.get(want)
        if not pick:
            # fall back to the set's sole / first global effect
            pick = next(iter(by_cat.values()), None)
        if not pick or all(ef['category'] == 'Special' for ef in pick[1]):
            generated[e['key']] = pick[1] if pick else [{'category': 'Special'}]
            n_special += 1
            rows.append(f'  [special] {e["setName"]:30s} want={want} GEN={pick[1] if pick else "[]"}')
            continue
        pnum, effs, src = pick
        # strip helper keys from emitted effects
        clean = [{k: v for k, v in ef.items() if k != 'raw'} for ef in effs]
        generated[e['key']] = clean
        gen_str = '; '.join(f'{ef["category"]}({ef.get("effectType","")} {ef.get("value")})'.replace('( ', '(')
                            for ef in clean)
        rows.append(f'  {e["setName"]:30s} #{pnum} want={want:18s} GEN: {gen_str}')

    print('\n'.join(rows))
    print(f'\n=== globals: {len(generated)} entries; {n_special} special/no-op; {n_missing} missing set ===')

    # --- Damage procs (Phase 3): scale × Melee_ProcDamage[level] ------------
    procdmg = load_proc_damage_table()
    l1, l50 = abs(procdmg[0]), abs(procdmg[49])
    dmg_gen: dict[str, list[dict]] = {}
    dmg_unresolved: list[str] = []
    for e in parse_hand_damage():
        s = set_by_id.get(sid(e['setName']))
        effs = resolve_damage_proc(s, pidx, l1, l50) if s else None
        if effs:
            dmg_gen[e['key']] = effs
        else:
            dmg_unresolved.append(f'    {"NO-SET" if not s else "no-template"}: {e["key"]} (set={e["setName"]})')
    print(f'=== damage: {len(dmg_gen)} entries (L1={l1}, L50={round(l50, 2)}); '
          f'{len(dmg_unresolved)} unresolved (universal-damage/Rebirth sets) ===')
    if '--diag' in sys.argv[1:]:
        print('\n'.join(dmg_unresolved))

    # --- Other non-global procs (Phase 3b): debuff / mez / knock / self-buff ----
    eff_gen: dict[str, list[dict]] = {}
    eff_unresolved: list[str] = []
    for e in parse_hand_other():
        s = set_by_id.get(sid(e['setName']))
        if not s:
            eff_unresolved.append(f'    NO-SET: {e["key"]} (set={e["setName"]}) mech="{e["mechanics"][:50]}"')
            continue
        # collect payloads from every piece, indexed by their effects' categories
        by_key: dict[str, list[dict]] = {}
        for bl in s.boostlists:
            pp = next((pidx[b] for b in bl.boosts if b in pidx), None)
            if not pp:
                continue
            payload = resolve_proc_payload(pp, gb_index)
            for ef in payload:
                ck = ef['category'] + (':' + ef['effectType'] if ef.get('effectType') else '')
                by_key.setdefault(ck, payload)
                by_key.setdefault(ef['category'], payload)
        want = infer_proc_category(e['mechanics'])
        pick = by_key.get(want)
        # Category fallback for non-Control wants only; a mez/knock want must match
        # its exact type (don't grab a different Control payload — e.g. the generic
        # "Chance for Stun" tagged to Stupefy, whose only proc is Knockback).
        if not pick and want and not want.startswith('Control'):
            pick = by_key.get(want.split(':')[0])
        if not pick and not want:
            pick = next(iter(by_key.values()), None)  # no inference -> any payload
        if pick:
            # A piece from a pet set rides on the PET (see PET_CARRIED_GROUPS) —
            # its `target: Self` templates mean the pet. Copy rather than mutate:
            # `by_key` aliases one payload list under several keys.
            if s.group_name in PET_CARRIED_GROUPS:
                pick = [{**ef, 'target': 'pets'} for ef in pick]
            eff_gen[e['key']] = pick
        else:
            payload_keys = sorted(by_key.keys())
            eff_unresolved.append(
                f'    NO-MATCH: {e["key"]} (set={e["setName"]}) want={want!r} '
                f'payloads={payload_keys} mech="{e["mechanics"][:45]}"')
    print(f'=== other: {len(eff_gen)} entries; {len(eff_unresolved)} unresolved ===')
    if '--diag' in sys.argv[1:]:
        print('\n'.join(eff_unresolved))

    # --- PPM (P6): binary-source the per-proc PPM. PPM drives proc DPS + PPM
    # recovery; the hand values had drift (esp. Superior ATOs carrying the base
    # PPM). Each proc set has one proc piece, so its proc-group PPM is unambiguous. -
    ppm_gen: dict[str, float] = {}
    boosts_gen: dict[str, list[str]] = {}
    boosts_unresolved: list[str] = []
    export_boosts = export_proc_boosts_by_set()
    for e in parse_hand_ppm():
        set_key = sid(e['setName'])
        s = set_by_id.get(set_key)
        binppm = 0.0
        proc_piece = None
        for bl in (s.boostlists if s else []):
            pp = next((pidx[b] for b in bl.boosts if b in pidx), None)
            if not pp:
                continue
            for eg in pp.effects:
                if (eg.ppm or 0) > binppm:
                    binppm, proc_piece = eg.ppm, pp
        if binppm > 0:
            ppm_gen[e['key']] = round(binppm, 4)
        # The PIECE that carries the PPM group also carries the routing key: its own
        # BoostsAllowed, which is what CopyBoosts filters by when a kNone shell hands
        # its slotting to an executed child. Emitted verbatim, origins included —
        # the intersection ignores them for free (no power's list names an origin).
        piece_boosts = None
        if proc_piece is not None and proc_piece.boosts_allowed:
            piece_boosts = list(proc_piece.boosts_allowed)
        elif s is not None:
            # A chance-based proc (Superior Assassin's Mark) authors no PPM group to
            # name its piece, so ask the set as a group instead: pieces of one set
            # slot as a unit and author one BoostsAllowed between them (the four MM
            # pet sets that don't all carry a PPM piece, which the branch above
            # takes). A set whose pieces disagree AND names no PPM piece stays
            # unresolved rather than guessed.
            lists = {tuple(pidx[b].boosts_allowed or ())
                     for bl in s.boostlists for b in bl.boosts if b in pidx}
            lists.discard(())
            if lists and len({frozenset(t) for t in lists}) == 1:
                piece_boosts = list(min(lists))
        if piece_boosts is None:
            fb = export_boosts.get(set_key) or export_boosts.get(SET_ALIASES.get(set_key, ''))
            if fb:
                piece_boosts = list(fb)
        if piece_boosts is not None:
            boosts_gen[e['key']] = piece_boosts
        else:
            boosts_unresolved.append(f'    NO-PIECE: {e["key"]} (set={e["setName"]})')
    ppm_diffs = sum(1 for e in parse_hand_ppm()
                    if e['key'] in ppm_gen and abs(ppm_gen[e['key']] - e['ppm']) > 0.01)
    print(f'=== ppm: {len(ppm_gen)} entries; {ppm_diffs} corrections vs hand ===')
    print(f'=== boosts allowed: {len(boosts_gen)} entries; '
          f'{len(boosts_unresolved)} unresolved ===')
    print('\n'.join(boosts_unresolved))

    # --- ActivatePeriod (PPM-1): the piece's own fActivatePeriod ------------
    # `CalculateModChance` multiplies PPM by `ptemplate->ppowBase->fActivatePeriod`,
    # and a proc's templates are iterated straight off the boost, so the period read
    # is the ENHANCEMENT PIECE's — never the host toggle's. Emitted per entry so both
    # engines read it instead of standing a constant where the data belongs.
    # A set's pieces are asked as a group: they author one period between them, and a
    # set that ever disagrees is reported rather than resolved by picking a piece,
    # because then the piece identity is load-bearing and this resolution is too coarse.
    period_gen: dict[str, float] = {}
    period_split: list[str] = []
    period_unresolved: list[str] = []
    from_export = export_periods_by_set()
    for e in parse_hand_entries():
        set_key = sid(e['setName'])
        s = set_by_id.get(set_key)
        found = {round(pidx[b].activate_period, 4)
                 for bl in s.boostlists for b in bl.boosts if b in pidx} if s else set()
        if not found:
            found = from_export.get(set_key) or from_export.get(SET_ALIASES.get(set_key, '')) or set()
        if len(found) == 1:
            period_gen[e['key']] = next(iter(found))
        elif found:
            period_split.append(f'    SPLIT: {e["key"]} (set={e["setName"]}) periods={sorted(found)}')
        else:
            period_unresolved.append(f'    NO-SET: {e["key"]} (set={e["setName"]})')
    print(f'=== activate period: {len(period_gen)} entries; {len(period_split)} sets disagree; '
          f'{len(period_unresolved)} unresolved ===')
    print('\n'.join(period_split + period_unresolved))

    # NOTE (P5 — Rebirth): the Rebirth-UNIQUE proc sets (Guardian's Gift, Imperial
    # Might, The Haunting, Vampire's Bite, Return From The Grave, Inexhaustibility,
    # Superior Winter's Gift, …) are bespoke — Create_Entity summons, Set_Mode
    # globals, and Fear+Damage combos the generic resolver can't fully reproduce.
    # They are already binary-sourced in the curated proc-data.ts entries (values
    # pulled from the Rebirth bins by hand). Shared sets reuse the HC effects above
    # (PROC_DATABASE is one cross-server table). A generator pass yielded only
    # incomplete results, so it's intentionally omitted.

    # The six side tables `emit-contract.cjs` staples onto `hand-data/proc-data.json`, in the
    # order it applies them. The three effects tables each REPLACE an entry's whole `effects`
    # array, so a later one wins outright; `ppm` overwrites the authored value in place.
    if emit:
        # Always-on GLOBAL proc effects, resolved off the `Set_Bonus.Global_Bonus.*` power a
        # piece's marker or `Grant_Power` redirect names — not the piece's own Null/Current
        # marker, which understates them (Shield Wall marker 0.03, real 0.05).
        _emit_effects_json(OUT_GLOBALS, 'PROC_GLOBAL_EFFECTS', generated)
        # DAMAGE proc effects: scale × Melee_ProcDamage[level], with the displayed N–M range
        # being the level-1 and level-50 damage.
        _emit_effects_json(OUT_DAMAGE, 'PROC_DAMAGE_EFFECTS', dmg_gen)
        # Every other proc's PAYLOAD: self-buff, foe debuff, mez, knock, Build Up.
        _emit_effects_json(OUT_EFFECTS, 'PROC_OTHER_EFFECTS', eff_gen)
        # Per-proc PPM off the piece's own proc group, which overlays the authored value. The
        # authored numbers had drift, most of it Superior ATOs carrying the base PPM.
        _emit_json(OUT_PPM, 'PROC_PPM', ppm_gen)
        # The PIECE's own fActivatePeriod, read off the boost power that carries the templates
        # — never the host toggle's. `CalculateModChance` multiplies PPM by this term, and
        # `power_IncrementBoostTimers` re-arms on this interval, so it sets both the per-check
        # chance and the checks per minute.
        _emit_json(OUT_PERIOD, 'PROC_ACTIVATE_PERIOD', period_gen)
        # The PIECE's own BoostsAllowed, verbatim: its one real boost type plus the five
        # origins. This is the list `CopyBoosts` filters by when a `ProcAllowed kNone` shell
        # hands its slotting to an executed child — a proc rolls in the child whose own
        # BoostsAllowed intersects this. The origins ride along harmlessly, since no power's
        # list names one.
        _emit_json(OUT_BOOSTS, 'PROC_BOOSTS_ALLOWED', boosts_gen)
    return 0


if __name__ == '__main__':
    sys.exit(main())
