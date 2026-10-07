/**
 * DSH8 incarnate per-slot collapse detector.
 *
 * The sibling of [`dsh6-collapse-detector.cjs`](scripts/dsh6-collapse-detector.cjs)
 * for the SEPARATE incarnate converter (`scripts/convert-incarnate-effects.cjs`),
 * which independently reinvented the bag-of-slots model. Same shape as DSH6:
 *
 *   INPUT  = DSH4 bridge atoms from the source `exported_powers/<ds>/incarnate/<slot>/`
 *            (`ingestExportPower`, single-source — the attrib→(effectType,subType) map
 *            is never re-ported here), filtered to what SHOULD surface as a caster buff.
 *   OUTPUT = every effect identity reachable in the generated record for that power
 *            (`pipeline/<ds>/incarnate-effects.json`).
 *   GATE   = "class-present, sibling-missing": an input identity whose effectType IS
 *            represented somewhere in the output but whose specific subType is not —
 *            the high-confidence multi-type collapse (the Support Core defense drop).
 *            A wholly-absent effectType lands in a non-gating class-absent bucket.
 *
 * WHY THE BY-DESIGN DROPS DIFFER FROM DSH6:
 *   - aspect=Str `<type>_Dmg`/`Accuracy` ARE surfaced by incarnates (Hybrid/Destiny
 *     `damage`/`accuracy`), NOT the Power-Boost→Enhancement boundary DSH6 excludes.
 *     So DamageBuff + Accuracy are CHECKABLE here (folded to presence).
 *   - The genuine incarnate drops are Enhancement (aspect=Str mez / by-type strength),
 *     Heal / team-heal, GrantPower / pets / engine markers — none is a flat caster stat.
 *   - `enttype target> player eq` is a LEAGUEMATE buff (kept, routed by polarity), NOT a
 *     PvP variant to drop (that was the bridge-convergence fix). So NO isPvpVariant drop;
 *     only an explicit pvMode=PvP (Parse7) is dropped.
 *
 * SCOPE (v1): the two multi-type BUFF slots that feed caster totals — Hybrid
 * (`passive`/`frontLoaded`/`perTarget`) and Destiny (flat stat map). Alpha/Genesis are
 * single-aspect enhancement (aspect=Str, keyed by filename) — structurally no multi-type
 * buff map to collapse; Interface/Judgement/Lore are proc/nuke/pet shapes. Those are
 * tracked as coverage, not swept here (a presence check would be low-signal).
 *
 * Runs in CI (all inputs committed, unlike DSH5's gitignored `.mhd`).
 *
 * Usage:
 *   node scripts/dsh8-incarnate-collapse-detector.cjs                    # HC sweep, write worklist
 *   node scripts/dsh8-incarnate-collapse-detector.cjs --dataset rebirth  # rebirth / thunderspy
 *   node scripts/dsh8-incarnate-collapse-detector.cjs --top 30           # + print top groups
 *   node scripts/dsh8-incarnate-collapse-detector.cjs --power support --slot hybrid
 */

require('tsx/cjs');
const fs = require('fs');
const path = require('path');
const { ingestExportPower } = require('./_atomic-effect.ts');
const { gateText } = require('./_gate-tokens.cjs');

const REPO = path.resolve(__dirname, '..');
const argv = process.argv.slice(2);
const argVal = (f) => { const i = argv.indexOf(f); return i >= 0 ? argv[i + 1] : undefined; };
const DATASET = argVal('--dataset') || 'homecoming';
const TOP = argVal('--top') ? parseInt(argVal('--top'), 10) : 0;
const POWER_FILTER = argVal('--power');
const SLOT_FILTER = argVal('--slot');
const GATE = argv.includes('--gate'); // exit non-zero on any high-confidence collapse (CI)

// dataset → (export sub-root, generated file). HC uses the flat export layout.
const DS_EXPORT_ROOT = DATASET === 'homecoming'
  ? path.join(REPO, 'exported_powers/incarnate')
  : path.join(REPO, 'exported_powers', DATASET, 'incarnate');
// The converted incarnate tables. Came from the generated TypeScript module until
// 2026-09-24; convert-incarnate-effects.cjs writes `pipeline/<id>/incarnate-effects.json`
// now, and the frozen TS copy would only tell this detector what was true last month.
const GEN_FILE = path.join(REPO, 'pipeline', DATASET, 'incarnate-effects.json');
// --worklist lets callers that only read stdout (the DSH8 vitest gate) redirect
// the side-effect file somewhere disposable instead of dirtying the committed one.
const OUT_PATH = argVal('--worklist') || path.join(REPO, 'scripts', 'dsh8-incarnate-collapse-worklist.json');

// ---------------------------------------------------------------------------
// Canonical subtype token (shared with DSH6): fold bridge subType names and
// converter slot keys onto one token so `Ranged`(bridge)==`defRanged`(slot), etc.
// ---------------------------------------------------------------------------
const CANON_SUB = {
  smashing: 'smashing', lethal: 'lethal', fire: 'fire', cold: 'cold', energy: 'energy',
  negative: 'negative', toxic: 'toxic', psionic: 'psionic',
  melee: 'melee', ranged: 'ranged', aoe: 'aoe', area: 'aoe',
  held: 'hold', hold: 'hold', stunned: 'stun', stun: 'stun', sleep: 'sleep',
  immobilized: 'immobilize', immobilize: 'immobilize', confused: 'confuse', confuse: 'confuse',
  terrorized: 'fear', afraid: 'fear', fear: 'fear',
  knockback: 'knockback', knockup: 'knockup', repel: 'repel',
  run: 'run', runspeed: 'run', fly: 'fly', flyspeed: 'fly', jump: 'jump', jumpspeed: 'jump',
  jumpheight: 'jumpheight', all: 'all',
};
const normSub = (s) => (s == null ? '' : (CANON_SUB[String(s).toLowerCase()] ?? String(s).toLowerCase()));

// ---------------------------------------------------------------------------
// OUTPUT-side slot table: converter statKey → { et, sub | fold | byType }.
//   fold:'all'   the key represents the WHOLE effectType (defenseAll, mezProtection,
//                statusResistance, damage, accuracy) — any input subType is covered.
//   sub:'x'      the key represents one canonical subType (defMelee → Defense|melee).
//   scalar keys  carry no subType.
// Built from the enumerated Hybrid + Destiny vocab (2026-07-06).
// ---------------------------------------------------------------------------
const OUT_TABLE = {
  // Hybrid + Destiny shared / scalar
  damage: { et: 'DamageBuff', fold: 'all' },
  accuracy: { et: 'Accuracy' },
  regeneration: { et: 'Regeneration' },
  recovery: { et: 'Recovery' },
  endurance: { et: 'Endurance' },
  recharge: { et: 'RechargeTime' },
  enduranceDiscount: { et: 'EnduranceDiscount' },
  maxHP: { et: 'MaxHP' },
  // A single `runSpeed` key represents the WHOLE movement buff: the Destiny calc
  // (character-totals.ts:3147-3157) fans it to run + fly + jumpHeight ("one buff over
  // all three axes" — Incandescence Radial's uniform +0.35 run/fly/jump is stored once).
  runSpeed: { et: 'Movement', foldSet: ['run', 'fly', 'jump', 'jumpheight'] },
  statusResistance: { et: 'MezResist', fold: 'all' },
  // defense: per-type (Hybrid) + fold-all (Destiny)
  defMelee: { et: 'Defense', sub: 'melee' }, defRanged: { et: 'Defense', sub: 'ranged' },
  defAoE: { et: 'Defense', sub: 'aoe' }, defSmashing: { et: 'Defense', sub: 'smashing' },
  defLethal: { et: 'Defense', sub: 'lethal' }, defFire: { et: 'Defense', sub: 'fire' },
  defCold: { et: 'Defense', sub: 'cold' }, defEnergy: { et: 'Defense', sub: 'energy' },
  defNegative: { et: 'Defense', sub: 'negative' }, defPsionic: { et: 'Defense', sub: 'psionic' },
  defToxic: { et: 'Defense', sub: 'toxic' }, defenseAll: { et: 'Defense', fold: 'all' },
  // resistance: per-type (Hybrid) + fold-all (Destiny)
  resSmashing: { et: 'Resistance', sub: 'smashing' }, resLethal: { et: 'Resistance', sub: 'lethal' },
  resFire: { et: 'Resistance', sub: 'fire' }, resCold: { et: 'Resistance', sub: 'cold' },
  resEnergy: { et: 'Resistance', sub: 'energy' }, resNegative: { et: 'Resistance', sub: 'negative' },
  resPsionic: { et: 'Resistance', sub: 'psionic' }, resToxic: { et: 'Resistance', sub: 'toxic' },
  resistanceAll: { et: 'Resistance', fold: 'all' },
  // mez protection: per-mez (Hybrid) + fold-all (Destiny) + KB
  protHold: { et: 'Mez', sub: 'hold' }, protStun: { et: 'Mez', sub: 'stun' },
  protSleep: { et: 'Mez', sub: 'sleep' }, protImmobilize: { et: 'Mez', sub: 'immobilize' },
  protConfuse: { et: 'Mez', sub: 'confuse' }, protFear: { et: 'Mez', sub: 'fear' },
  mezProtection: { et: 'Mez', fold: 'all' },
  kbProtection: { et: 'Mez', foldSet: ['knockback', 'knockup', 'repel'] },
};
// debuffResistance is a nested {child: value} map — each child names a resisted effectType.
const DEBUFFRES_CHILD_ET = {
  defense: 'Defense', movement: 'Movement', endurance: 'Endurance', recovery: 'Recovery',
  regeneration: 'Regeneration', tohit: 'ToHit', accuracy: 'Accuracy', recharge: 'RechargeTime',
  range: 'Range', perception: 'Perception',
};
// keys that carry no effect identity (meta / value-detail / summon).
const META_KEY = new Set([
  'levelShift', 'healScale', 'healTable', 'healReceived', 'duration', 'recharge_', 'tree',
  'maxTargets', 'stackCaps',
]);

// ---------------------------------------------------------------------------
// Collect represented identities `et|sub` (sub '' for folded/scalar) from a
// generated record. `all`-folds are recorded as `et|all`; the matcher treats
// `et|all` as covering any subType of that effectType.
// ---------------------------------------------------------------------------
function collectFromBag(bag, ids, classes) {
  if (!bag || typeof bag !== 'object') return;
  for (const [key, val] of Object.entries(bag)) {
    if (key === 'debuffResistance' && val && typeof val === 'object') {
      for (const child of Object.keys(val)) {
        const et = DEBUFFRES_CHILD_ET[child.toLowerCase()];
        if (et) { classes.add(et); ids.add(`${et}|`); }
      }
      continue;
    }
    if (META_KEY.has(key)) continue;
    const slot = OUT_TABLE[key];
    if (!slot) continue;
    // ignore empty/zero scalar values (an emitted-but-0 key is not a represented buff)
    if (typeof val === 'number' && val === 0) continue;
    classes.add(slot.et);
    if (slot.fold === 'all') { ids.add(`${slot.et}|all`); ids.add(`${slot.et}|`); }
    else if (slot.foldSet) { for (const s of slot.foldSet) ids.add(`${slot.et}|${s}`); ids.add(`${slot.et}|`); }
    else if (slot.sub) { ids.add(`${slot.et}|${slot.sub}`); ids.add(`${slot.et}|`); }
    else ids.add(`${slot.et}|`);
  }
}

function collectRepresented(rec, slot) {
  const ids = new Set();
  const classes = new Set();
  if (!rec || typeof rec !== 'object') return { ids, classes };
  if (slot === 'hybrid') {
    for (const bag of ['passive', 'frontLoaded', 'perTarget']) collectFromBag(rec[bag], ids, classes);
  } else { // destiny flat map
    collectFromBag(rec, ids, classes);
  }
  return { ids, classes };
}

// ---------------------------------------------------------------------------
// INPUT side: source atoms → checkable surviving buff identities.
// ---------------------------------------------------------------------------
// effectTypes surfaced as a flat caster buff. Folded (presence only, subType ''):
const FOLD_ET = new Set(['DamageBuff', 'Accuracy', 'Regeneration', 'Recovery', 'Endurance',
  'RechargeTime', 'EnduranceDiscount', 'MaxHP', 'MezResist']);
// per-subType (multi-type explosion — where a sibling can collapse):
const BYTYPE_ET = new Set(['Defense', 'Resistance', 'Mez', 'Movement']);
// The six core mez attribs the converter maps to statusResistance at aspect=Res
// (everything else — Repel/Taunt/Placate — projects to scalar debuffResistance).
const CORE_MEZ_ATTRIBS = new Set(['Held', 'Stunned', 'Immobilized', 'Sleep',
  'Confused', 'Afraid', 'Terrorized']);
const STD_TYPE = new Set(['smashing', 'lethal', 'fire', 'cold', 'energy', 'negative', 'toxic', 'psionic', 'melee', 'ranged', 'aoe']);
const MEZ_SUB = new Set(['hold', 'stun', 'sleep', 'immobilize', 'confuse', 'fear']);
const MOVE_AXIS = new Set(['run', 'fly', 'jump', 'jumpheight']);

function checkableSub(et, sub) {
  if (et === 'Defense' || et === 'Resistance') return STD_TYPE.has(sub) || sub === 'all';
  if (et === 'Mez') return MEZ_SUB.has(sub);            // protection mezzes only
  if (et === 'Movement') return MOVE_AXIS.has(sub);
  return true; // folded effectTypes have no subType to check
}

function inputIdentities(sourceJson, slot) {
  let atoms;
  try { atoms = ingestExportPower(sourceJson); } catch { return []; }
  const out = [];
  for (const a of atoms) {
    if (a.pvMode === 'PvP') continue;             // explicit PvP-only group (Parse7)
    // PvP-map-only gate (`isPVPMap?` without negation) — the converter's
    // isPvpMapOnlyRequires drop. Parse6/tspy can't mark these pvMode=PvP (its
    // synthesized flag reads only the enttype target-type), so the gate rides
    // requires_expression: tspy Clarion's 6-mez status-resistance twin carries
    // it where HC marks the same rows is_pvp=PVP_ONLY.
    if (/\bisPVPMap\?(?!\s+!)/i.test(gateText(a.requiresExpression))) continue;
    if (!a.scale) continue;                       // scale-0 marker
    if (a.attribType === 'Expression') continue;  // engine phantom / cap
    const et = a.effectType;
    const isFold = FOLD_ET.has(et);
    const isByType = BYTYPE_ET.has(et);
    if (!isFold && !isByType) continue;           // Enhancement/Heal/Damage/GrantPower/… = by-design drop
    // Ageless-style ArchVillain_Res templates are broad debuff-resistance bundles
    // (mixed attrib families under one Resistance-aspect table) that the converter
    // intentionally projects to `debuffResistance`, not `resistanceAll`. The bridge
    // surfaces their damage-type rows as `Resistance|*`; treat that family as a
    // by-design projection drop in this detector to avoid non-actionable noise.
    if (et === 'Resistance' && /archvillain_res/i.test(a.modifierTable || '')) continue;
    // aspect=Res on a BUFF effectType is resistance-to-that-debuff (routed to
    // debuffResistance), not a +buff — drop it (Ageless Radial's AV-blanket -recovery/
    // -regen/-recharge resistance). Keep aspect=Res only where it IS the signal:
    // Resistance (damage-res) and MezResist (mez-duration-res). Mirrors DSH6.
    if (a.aspect === 'Res' && et !== 'Resistance' && et !== 'MezResist') continue;
    // Non-core-mez MezResist (Clarion's Repel/Taunt/Placate resistance) is
    // projected to the scalar `debuffResistance` by design (the converter's
    // Resistance-aspect else-branch; Clarion debuffResistance 0.7 is pinned by
    // incarnate-effects-completeness) — only the six core mez map to
    // statusResistance, so only those are checkable here.
    if (et === 'MezResist' && !CORE_MEZ_ATTRIBS.has(a.sourceAttrib)) continue;
    // Slot-aware Strength semantics (mirror converter behavior):
    //   - Destiny surfaces only Recharge/Recovery/Regeneration at aspect=Str.
    //     Other Strength rows (e.g. Clarion's secondary-effect amplifiers such as
    //     RunningSpeed/FlyingSpeed/Endurance) are by-design drops from caster totals.
    //   - Hybrid surfaces DamageBuff/Accuracy/EnduranceDiscount at aspect=Str.
    if (a.aspect === 'Str') {
      if (slot === 'destiny' && !['RechargeTime', 'Recovery', 'Regeneration'].includes(et)) continue;
      if (slot === 'hybrid' && !['DamageBuff', 'Accuracy', 'EnduranceDiscount'].includes(et)) continue;
    }
    // polarity: incarnate buff slots are beneficial. A buff is scale>0; mez PROTECTION
    // is aspect=Cur negative-scale (the converter Math.abs's it). MezResist is aspect=Res.
    let beneficial;
    // `Unspecified` (a template that states no aspect — every Thunderspy one) counts
    // alongside `Cur` here: the ingest used to default it to `Cur`, and this test means
    // "not the Res face", not "literally stated Current". Excluding it would silently
    // drop Thunderspy's mez protection from the detector's view.
    if (et === 'Mez') beneficial = (a.aspect === 'Cur' || a.aspect === 'Unspecified') && a.scale < 0;   // protection, not applying a foe mez
    else if (et === 'MezResist') beneficial = a.scale > 0;
    else beneficial = a.scale > 0;
    if (!beneficial) continue;
    const sub = isByType ? normSub(a.subType) : '';
    if (isByType && !checkableSub(et, sub)) continue; // exotic type / KB-as-mez / unrouted
    out.push({ et, sub, sourceAttrib: a.sourceAttrib, table: a.modifierTable, scale: a.scale, aspect: a.aspect });
  }
  return out;
}

// represented? honor the `et|all` fold (defenseAll/resistanceAll/mezProtection/damage cover any sub).
function isRepresented(et, sub, ids) {
  if (ids.has(`${et}|${sub}`)) return true;
  if (ids.has(`${et}|all`)) return true;     // fold-all output covers any subType
  if (!sub && ids.has(`${et}|`)) return true; // folded input matches any output of that ET
  return false;
}

// ---------------------------------------------------------------------------
function main() {
  if (!fs.existsSync(GEN_FILE)) {
    console.error(`no converted incarnate tables: ${GEN_FILE}\n`
      + `  Run: node scripts/convert-incarnate-effects.cjs --dataset ${DATASET}`);
    process.exit(2);
  }
  const gen = JSON.parse(fs.readFileSync(GEN_FILE, 'utf8'));
  const SLOT_EXPORT = {
    hybrid: gen.GENERATED_HYBRID_EFFECTS || {},
    destiny: gen.GENERATED_DESTINY_EFFECTS || {},
  };
  const slots = SLOT_FILTER ? [SLOT_FILTER] : ['hybrid', 'destiny'];

  const collapses = [];    // class-present, sibling-missing (HIGH confidence)
  const classAbsent = [];  // whole effectType absent (non-gating)
  const cov = { powersTotal: 0, powersChecked: 0, genMissing: 0, atomsChecked: 0 };

  for (const slot of slots) {
    const dir = path.join(DS_EXPORT_ROOT, slot);
    if (!fs.existsSync(dir)) continue;
    const genMap = SLOT_EXPORT[slot];
    for (const f of fs.readdirSync(dir).sort()) {
      if (!f.endsWith('.json') || f === 'index.json') continue;
      const key = f.slice(0, -5);
      if (POWER_FILTER && !key.toLowerCase().includes(POWER_FILTER.toLowerCase())) continue;
      cov.powersTotal++;
      const rec = genMap[key];
      if (rec === undefined) { cov.genMissing++; continue; } // power not in generated (filtered tier / redirect)
      let sourceJson;
      try { sourceJson = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf-8')); } catch { continue; }
      cov.powersChecked++;

      const { ids, classes } = collectRepresented(rec, slot);
      const seen = new Set();
      for (const inp of inputIdentities(sourceJson, slot)) {
        const id = `${inp.et}|${inp.sub}`;
        if (seen.has(id)) continue;
        seen.add(id);
        cov.atomsChecked++;
        if (isRepresented(inp.et, inp.sub, ids)) continue;
        const rc = { dataset: DATASET, slot, power: key, et: inp.et, sub: inp.sub,
                     sourceAttrib: inp.sourceAttrib, table: inp.table, scale: inp.scale, aspect: inp.aspect };
        if (classes.has(inp.et)) collapses.push(rc); // class-present, sibling-missing = HIGH confidence
        else classAbsent.push(rc);                   // whole effectType absent = ambiguous
      }
    }
  }

  // group by (et, sub)
  const group = (arr, cap) => {
    const m = new Map();
    for (const c of arr) {
      const k = `${c.et}|${c.sub}`;
      if (!m.has(k)) m.set(k, { et: c.et, sub: c.sub, count: 0, powers: [] });
      const g = m.get(k); g.count++; if (g.powers.length < cap) g.powers.push(`${c.slot}:${c.power}`);
    }
    return [...m.values()].sort((a, b) => b.count - a.count);
  };
  const collapseGroups = group(collapses, 12);
  const absentGroups = group(classAbsent, 8);

  const result = {
    schema: 'dsh8-incarnate-collapse-worklist/1',
    dataset: DATASET,
    scope: {
      slots: 'Hybrid (passive/frontLoaded/perTarget) + Destiny (flat stat map) — the ' +
        'multi-type buff slots feeding caster totals. Alpha/Genesis (single-aspect ' +
        'enhancement) + Interface/Judgement/Lore (proc/nuke/pet) are structurally ' +
        'collapse-free here and tracked as coverage, not swept.',
      gate: 'class-present, sibling-missing: an input effectType represented somewhere in ' +
        'the output but a specific subType absent (the multi-type explosion). DamageBuff/' +
        'Accuracy/scalars are folded to presence; Defense/Resistance/Mez-protection/Movement ' +
        'are checked per-subType. Fold-all output keys (defenseAll/resistanceAll/mezProtection/' +
        'damage) cover any subType.',
      byDesignDrops: 'Enhancement (aspect=Str mez/by-type strength), Heal/team-heal, Damage ' +
        '(direct — Judgement/Interface), non-routed Strength amplifiers in slot context ' +
        '(Destiny keeps only recharge/recovery/regeneration at Str; Hybrid keeps only ' +
        'damage/accuracy/enduranceDiscount), GrantPower/pets/engine markers, scale-0, ' +
        'Expression, explicit pvMode=PvP, PvP-map-only gates (`isPVPMap?` requires — the ' +
        'Parse6/tspy form of the same twin), non-core-mez MezResist (Repel/Taunt/Placate — ' +
        'projected to scalar `debuffResistance`), and Ageless-style `*_ArchVillain_Res` bundles ' +
        '(projected to `debuffResistance`, not `resistanceAll`). NO isPvpVariant drop — ' +
        '`player eq` is the leaguemate buff (kept, routed by polarity), the ' +
        'bridge-convergence invariant.',
    },
    coverage: cov,
    summary: {
      highConfidenceCollapses: collapses.length,
      distinctCollapseGroups: collapseGroups.length,
      classAbsent: classAbsent.length,
    },
    collapseGroups,
    classAbsentGroups: absentGroups,
    collapses,
  };
  fs.writeFileSync(OUT_PATH, JSON.stringify(result, null, 2));

  console.log(`\nDSH8 incarnate collapse detector — ${DATASET}`);
  console.log(`  powers: ${cov.powersChecked}/${cov.powersTotal} checked (gen-missing ${cov.genMissing})`);
  console.log(`  atoms checked: ${cov.atomsChecked}`);
  console.log(`  HIGH-confidence collapses: ${collapses.length} (${collapseGroups.length} groups)`);
  console.log(`  class-absent (non-gating): ${classAbsent.length}`);
  console.log(`  worklist → ${path.relative(REPO, OUT_PATH)}`);
  if (TOP) {
    console.log(`\n  top ${TOP} collapse groups (effectType|subType  ×count  e.g. powers):`);
    for (const g of collapseGroups.slice(0, TOP)) {
      console.log(`   ${String(g.count).padStart(4)}  ${g.et}|${g.sub}   ${g.powers.slice(0, 6).join(', ')}`);
    }
    console.log(`\n  top ${TOP} class-absent groups:`);
    for (const g of absentGroups.slice(0, TOP)) {
      console.log(`   ${String(g.count).padStart(4)}  ${g.et}|${g.sub}   ${g.powers.slice(0, 6).join(', ')}`);
    }
  }

  if (GATE) {
    if (collapses.length > 0) {
      console.error(`GATE FAIL (${DATASET}) — ${collapses.length} high-confidence incarnate collapse(s): ` +
        collapseGroups.slice(0, 8).map((g) => `${g.et}|${g.sub}×${g.count}`).join(', '));
      process.exit(1);
    }
    console.log(`GATE PASS — no high-confidence incarnate collapses (${DATASET}).`);
  }
}

main();
