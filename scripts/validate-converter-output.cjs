/**
 * DSH3 (SC-2) — converter effect-collapse detector.
 *
 * Reads the committed parser export (`exported_powers/`, the converter INPUT) and
 * the committed generated data (the converter OUTPUT), and checks that compound
 * game effects survived conversion instead of collapsing into a single slot.
 *
 * The whole "collapse" bug family (PvE/PvP clobber, resistible/unresistable
 * pair-collapse, multi-type merge, duration-variant loss) shares ONE root cause:
 * convert-powerset models a power as a bag of ~90 named single-value slots
 * (last-write-wins), while the game/parser model it as a flat array of atomic
 * effect records.
 *
 * Two tiers:
 *
 *   GATE (hard fail) — RESISTIBLE-TWIN REGRESSION. HC splits many debuffs into a
 *   resistible + an IgnoreResistance template that are otherwise identical
 *   (attrib/aspect/table/scale/duration). Both apply in game; the bag-of-slots
 *   model would drop or double one. The converter detects these twins and stamps
 *   the survivor with resistible=false (convert-powerset.cjs:6305). This gate
 *   mirrors that EXACT detection on the input: if a power has a debuff twin, its
 *   generated output MUST carry the unresistable disposition in the atom model
 *   (a tuple with resistible=false at index 10) — else the fix regressed. This is
 *   the precise class fixed 2026-07-05; the gate keeps it fixed.
 *
 *   REPORT (informational, never fails) — COLLAPSE RISK. Per power, sibling
 *   templates that share (attribs, aspect) but differ in table/scale/pvMode/
 *   resistibility are effects the single-slot model cannot represent losslessly.
 *   A fully-general per-slot detector needs the converter's attrib→slot routing
 *   map (the output has already discarded the attrib identity) — that lands with
 *   the DSH4 schema + DSH6 converter rework. Until then this surfaces the family
 *   for triage without asserting every case is a bug.
 *
 * Usage:
 *   node scripts/validate-converter-output.cjs [--dataset <id>] [--gate] [--limit N]
 *     --dataset <id>   homecoming (default) | rebirth | thunderspy
 *     --gate           CI mode: suppress the report, print one PASS/FAIL line,
 *                      exit 1 on any gate failure
 *     --limit N        cap the risk-report rows printed (default 40)
 */

const fs = require('fs');
const path = require('path');
require('tsx/cjs');
const { ingestExportGroup } = require('./_atomic-effect.ts');
const { parseDatasetArg } = require('./_dataset-paths.cjs');
const { isPvpOnlyGroup: isPvpOnlyByRequires } = require('./_pv-scope.cjs');
const { gateText } = require('./_gate-tokens.cjs');

const args = process.argv.slice(2);
const GATE = args.includes('--gate');
const LIMIT = (() => {
  const i = args.indexOf('--limit');
  return i >= 0 && args[i + 1] ? parseInt(args[i + 1], 10) : 40;
})();
const datasetId = parseDatasetArg();

// ---- input (exported_powers) + output (generated) roots ----
const RAW_BASE = path.join(__dirname, '..', 'exported_powers');
const INPUT_ROOT =
  datasetId === 'homecoming' && !fs.existsSync(path.join(RAW_BASE, datasetId))
    ? RAW_BASE
    : path.join(RAW_BASE, datasetId);
// Spelled directly. This used to be `path.dirname(datasetPath(datasetId, 'at-tables.ts'))`
// — the `src/data/datasets/<id>/` folder, reached by naming a file inside it and stepping
// back up. That file was not written there any more, and naming it implied a dependency
// that never existed; `datasetPath` itself is gone now, with `src/`.
const { forEachPowersetPower } = require('./_powerset-tree.cjs');

const norm = (s) => (s || '').toLowerCase().replace(/[^a-z0-9]/g, '');

// The 13 core archetypes whose input category prefix maps 1:1 to a generated
// `/powersets/<archetype>/` tree, so an input power can be matched to its exact
// output. The twin GATE is restricted to these: a power's category is
// `<Archetype>_<Role>` (Defender_Buff, Dominator_Control, …), so the archetype is
// the first token. Everything else — pseudo-pets (Pets.*), cross-AT Pool/Epic/
// Inherent/Incarnate (single-object files), Mission_Maker, VEAT Teamwork/Widow —
// either has no per-AT output or an ambiguous name, so it is NOT gated (its debuff
// twins ride a different representation path, e.g. the summon subtree). This is
// what let the Dimension_Shift pseudo-pet false-positive slip in.
const GATED_ARCHETYPES = new Set([
  'blaster', 'brute', 'controller', 'corruptor', 'defender', 'dominator',
  'mastermind', 'scrapper', 'sentinel', 'stalker', 'tanker',
  'peacebringer', 'warshade',
]);
const archOfCategory = (fullName) => (fullName || '').split('.')[0].split('_')[0].toLowerCase();

// ---- twin detection — mirrors extractEffects (convert-powerset.cjs:6305) ----
const isTwinDebuff = (t) =>
  (t.scale || 0) < 0 || (t.table || '').toLowerCase().includes('debuff');
const twinSig = (t) =>
  `${(t.attribs || []).map((a) => (a || '').toLowerCase()).sort().join(',')}|` +
  `${(t.aspect || '').toLowerCase()}|${t.table}|` +
  `${(t.scale || 0).toFixed(4)}|${t.duration || ''}`;

/** Flatten every template in a power's effect tree, skipping PVP_ONLY groups
 * (collectTemplatesDeep drops the `player eq`/PvP group before extractEffects). */
function flattenTemplatesWithMeta(groups, out = []) {
  for (const g of groups || []) {
    const pvMode = g.is_pvp || 'EITHER';
    const requires = gateText(g.requires_expression);
    // Carried per row because the PvP verdict is the parser's structural read
    // of the expression, not something re-derivable from the string here.
    const pvpByRequires = isPvpOnlyByRequires(g);
    for (const t of g.templates || []) out.push({ t, pvMode, requires, pvpByRequires });
    if (g.child_effects) flattenTemplatesWithMeta(g.child_effects, out);
  }
  return out;
}

function isPvpMapOnly(requires) {
  return /\bisPVPMap\?(?!\s+!)/i.test(requires || '');
}

function isDebuffLike(scale, table) {
  return (scale || 0) < 0 || /debuff|slow/i.test(table || '');
}

function sc3Check(power) {
  let checked = 0;
  let missing = 0;
  let atoms;
  try {
    atoms = ingestExportPowerDeep(power);
  } catch {
    return { checked: 0, missing: 0 };
  }
  for (const a of atoms) {
    const damageOrDebuff = a.effectType === 'Damage' || isDebuffLike(a.scale, a.modifierTable);
    if (!damageOrDebuff) continue;
    checked++;
    if (typeof a.resistible !== 'boolean') missing++;
  }
  return { checked, missing };
}

function sc4Check(power) {
  const bySig = new Map();
  for (const row of flattenTemplatesWithMeta(power.effects)) {
    const t = row.t;
    if (!t.attribs || t.attribs.length === 0) continue;
    const sig =
      `${t.attribs.map((a) => (a || '').toLowerCase()).sort().join(',')}|` +
      `${(t.aspect || '').toLowerCase()}|${t.table || ''}|` +
      `${(t.scale || 0).toFixed(4)}|${t.duration || ''}|${t.target || ''}`;
    if (!bySig.has(sig)) bySig.set(sig, { pve: 0, pvp: 0, pvpDropped: 0, pvpKept: 0 });
    const b = bySig.get(sig);
    const pvpByMode = row.pvMode === 'PVP_ONLY';
    const pvpByReq = row.pvpByRequires || isPvpMapOnly(row.requires);
    const isPvpSibling = pvpByMode || pvpByReq;
    if (isPvpSibling) {
      b.pvp++;
      if (pvpByMode || pvpByReq) b.pvpDropped++;
      else b.pvpKept++;
    } else {
      b.pve++;
    }
  }

  let siblingPairs = 0;
  let accountedDrops = 0;
  let unaccounted = 0;
  for (const b of bySig.values()) {
    if (b.pve > 0 && b.pvp > 0) {
      siblingPairs++;
      accountedDrops += b.pvpDropped;
      // Any surviving PvP sibling in a PvE-kept family is unaccounted in this model.
      unaccounted += b.pvpKept;
    }
  }
  return { siblingPairs, accountedDrops, unaccounted };
}

// `flymode` (kFly, the flight-mode grant → the bag's `fly` key) and `fly`
// (FlyingSpeed, the speed buff → `flySpeed`) are now distinct axes on the atom, so
// each maps to exactly ONE bag key. This used to read `fly → ['fly', 'flySpeed']`:
// the bridge collapsed both attribs onto subType `Fly`, so SC-5 had to accept either
// key as a source or it would false-flag. That fudge cost precision — a self-slow on
// one key was satisfied by an atom belonging to the other — and is no longer needed.
function movementSlotKeys(subType) {
  const s = String(subType || '').toLowerCase();
  if (s === 'run' || s === 'runspeed') return ['runSpeed'];
  if (s === 'flymode') return ['fly'];
  if (s === 'fly' || s === 'flyspeed') return ['flySpeed'];
  if (s === 'jumpheight') return ['jumpHeight'];
  if (s === 'jump' || s === 'jumpspeed') return ['jumpSpeed'];
  if (s === 'control' || s === 'movementcontrol') return ['movementControl'];
  if (s === 'friction' || s === 'movementfriction') return ['movementFriction'];
  return [];
}

function sourceSelfDebuffSlots(power) {
  const slots = new Set();
  let atoms;
  try {
    atoms = ingestExportPowerDeep(power);
  } catch {
    return slots;
  }
  for (const a of atoms) {
    if (a.toWho !== 'Self') continue;
    if (!isDebuffLike(a.scale, a.modifierTable)) continue;
    if (a.effectType === 'DamageBuff') slots.add('damageDebuff');
    else if (a.effectType === 'RechargeTime') slots.add('rechargeDebuff');
    else if (a.effectType === 'ToHit') slots.add('tohitDebuff');
    else if (a.effectType === 'Accuracy') slots.add('accuracyDebuff');
    else if (a.effectType === 'Range') slots.add('rangeDebuff');
    else if (a.effectType === 'Movement') {
      for (const mapped of movementSlotKeys(a.subType)) slots.add(`slow:${mapped}`);
    }
  }
  return slots;
}

function ingestExportPowerDeep(power) {
  const out = [];
  const walk = (groups) => {
    for (const g of groups || []) {
      out.push(...ingestExportGroup(g));
      if (g.child_effects) walk(g.child_effects);
    }
  };
  walk(power.effects || []);
  return out;
}

function outputSelfDebuffSlots(powerObj) {
  const slots = new Set();
  const e = powerObj && powerObj.effects && typeof powerObj.effects === 'object' ? powerObj.effects : null;
  if (!e) return slots;

  for (const k of ['damageDebuff', 'rechargeDebuff', 'tohitDebuff', 'accuracyDebuff', 'rangeDebuff']) {
    const v = e[k];
    if (v && typeof v === 'object' && v.toWho === 'Self') slots.add(k);
  }
  if (e.slow && typeof e.slow === 'object') {
    for (const [k, v] of Object.entries(e.slow)) {
      if (v && typeof v === 'object' && v.toWho === 'Self') slots.add(`slow:${k}`);
    }
  }
  return slots;
}



/** True if the power's input has a resistible+unresistable debuff twin.
 *
 * PvP variants are NOT twins — they are the same effect on a different map, and
 * the converter gates them rather than routing them to a base slot, so no
 * `unresistable` stamp is owed. Both idioms must be excluded: Homecoming splits
 * on the target's entity type, Thunderspy on `isPVPMap?`. Reading only the
 * first made every tspy power whose PvP half carries IgnoreResistance (Unyielding's
 * knockback/repel protection and 10 others) look like an unstamped twin. */
function hasResistibleTwin(power) {
  const templates = flattenTemplatesWithMeta(power.effects)
    .filter(({ pvMode, requires, pvpByRequires }) => pvMode !== 'PVP_ONLY'
      && !pvpByRequires && !isPvpMapOnly(requires))
    .map(({ t }) => t);
  const seen = {};
  for (const t of templates) {
    if (!t.attribs || t.attribs.length === 0 || !isTwinDebuff(t)) continue;
    const sig = twinSig(t);
    if (!seen[sig]) seen[sig] = { res: false, unres: false };
    if ((t.flags || []).includes('IgnoreResistance')) seen[sig].unres = true;
    else seen[sig].res = true;
  }
  return Object.values(seen).some((s) => s.res && s.unres);
}

/** Per-power collapse-risk groups: ≥2 siblings sharing (attribs, aspect) but
 * differing in table/scale/pvMode/resistibility. Damage rides a multi-entry
 * array (not collapsed), so exclude pure damage attribs from the risk signal. */
function collapseRiskGroups(power) {
  const families = {};
  for (const g of power.effects || []) {
    const pv = g.is_pvp || 'EITHER';
    for (const t of g.templates || []) {
      if (!t.attribs || t.attribs.length === 0) continue;
      const famKey = `${t.attribs.map((a) => (a || '').toLowerCase()).sort().join(',')}|${(t.aspect || '').toLowerCase()}`;
      const identity = `${t.table}|${(t.scale || 0).toFixed(4)}|${pv}|${(t.flags || []).includes('IgnoreResistance') ? 'unres' : 'res'}`;
      (families[famKey] ||= new Set()).add(identity);
    }
  }
  return Object.entries(families)
    .filter(([, ids]) => ids.size >= 2)
    .map(([fam, ids]) => ({ family: fam, distinct: ids.size }));
}

// ---- index the converted output: internalName -> { hasUnresistable, relPath } ----
// Keyed by `<archetype>|<normInternalName>` so same-named powers across ATs
// (Dimension_Shift exists for both controller and dominator) don't collide.
//
// `hasUnresistable` matches the wire text of ONE power. It used to test the whole generated
// module, which held one power; the converted JSON holds a whole powerset, so the same regex
// over the file would answer for a power's neighbours as readily as for the power.
const outputIndex = new Map();
forEachPowersetPower(datasetId, (power, ctx) => {
  if (typeof power.internalName !== 'string') return;
  outputIndex.set(`${ctx.archetype}|${norm(power.internalName)}`, {
    // Atom model encodes unresistable disposition positionally: resistible=false at tuple index 10,
    // immediately followed by "Replace" at index 11. The old bag string "unresistable" is gone.
    hasUnresistable: /"Any",false,"Replace"/.test(JSON.stringify(power)),
    rel: ctx.relPath,
    power,
    exportDir: ctx.exportDir,
  });
});

// ---- iterate the input powers ----
function inputPowerFiles(root, out = []) {
  if (!fs.existsSync(root)) return out;
  for (const e of fs.readdirSync(root, { withFileTypes: true })) {
    const p = path.join(root, e.name);
    if (e.isDirectory()) inputPowerFiles(p, out);
    else if (e.name.endsWith('.json') && e.name !== 'index.json') out.push(p);
  }
  return out;
}

const twinRegressions = [];
const unmatchedTwins = [];
const riskRows = [];
const sc5Leaks = [];
let inputCount = 0;
let sc3Checked = 0;
let sc3Missing = 0;
let sc4SiblingPairs = 0;
let sc4AccountedDrops = 0;
let sc4Unaccounted = 0;

for (const f of inputPowerFiles(INPUT_ROOT)) {
  let power;
  try {
    power = JSON.parse(fs.readFileSync(f, 'utf8'));
  } catch {
    continue;
  }
  if (!Array.isArray(power.effects)) continue;
  inputCount++;

  const arch = archOfCategory(power.full_name);
  const inputRel = path.relative(INPUT_ROOT, f).replace(/\\/g, '/');
  const out = outputIndex.get(`${arch}|${norm(power.name)}`);

  const sc3 = sc3Check(power);
  sc3Checked += sc3.checked;
  sc3Missing += sc3.missing;

  const sc4 = sc4Check(power);
  sc4SiblingPairs += sc4.siblingPairs;
  sc4AccountedDrops += sc4.accountedDrops;
  sc4Unaccounted += sc4.unaccounted;

  // GATE — only for the core-AT categories with an unambiguous per-AT output.
  // Pseudo-pets / pools / mission-maker are not gated (their twins live on a
  // different path); count them as skipped rather than flagging blindly.
  if (hasResistibleTwin(power)) {
    if (!GATED_ARCHETYPES.has(arch) || !out) unmatchedTwins.push(power.full_name || power.name);
    else if (!out.hasUnresistable) twinRegressions.push({ name: power.full_name || power.name, rel: out.rel });
  }

  const risk = collapseRiskGroups(power);
  if (risk.length && out) {
    riskRows.push({ name: power.full_name || power.name, groups: risk.length, rel: out.rel });
  }

  if (out && out.exportDir === path.posix.dirname(inputRel)) {
    const generatedPower = out.power;
    if (generatedPower) {
      const srcSlots = sourceSelfDebuffSlots(power);
      const outSlots = outputSelfDebuffSlots(generatedPower);
      for (const slot of outSlots) {
        if (!srcSlots.has(slot)) {
          sc5Leaks.push({ name: power.full_name || power.name, rel: out.rel, slot });
        }
      }
    }
  }
}

// ---- output ----
riskRows.sort((a, b) => b.groups - a.groups);

if (!GATE) {
  console.log(`\n=== DSH3 converter collapse detector — dataset: ${datasetId} ===`);
  console.log(`Scanned ${inputCount} input powers; indexed ${outputIndex.size} generated outputs.\n`);

  console.log(`=== GATE: resistible-twin regressions (${twinRegressions.length}) ===`);
  if (twinRegressions.length === 0) {
    console.log('  none — every input debuff twin carries its unresistable disposition in the atom output.');
  } else {
    for (const r of twinRegressions) console.log(`  COLLAPSE  ${r.name}  (${r.rel})`);
  }
  if (unmatchedTwins.length) {
    console.log(`\n  (${unmatchedTwins.length} twin-bearing input powers had no matched output — not player-relevant / filtered; not gated.)`);
  }

  console.log(`\n=== SC-3: explicit resistible disposition (${sc3Missing}) ===`);
  console.log(`  checked ${sc3Checked} damage/debuff atoms; missing resistible flag: ${sc3Missing}`);

  console.log(`\n=== SC-4: PvP sibling accounting (${sc4Unaccounted}) ===`);
  console.log(`  sibling families: ${sc4SiblingPairs}; dropped-by-design accounted: ${sc4AccountedDrops}; unaccounted: ${sc4Unaccounted}`);

  console.log(`\n=== SC-5: sign/target routing leaks (${sc5Leaks.length}) ===`);
  if (sc5Leaks.length === 0) {
    console.log('  none — no foe-directed debuff was surfaced as caster self-penalty.');
  } else {
    for (const r of sc5Leaks.slice(0, 40)) {
      console.log(`  LEAK  ${r.name}  slot=${r.slot}  (${r.rel})`);
    }
    if (sc5Leaks.length > 40) console.log(`  … ${sc5Leaks.length - 40} more.`);
  }

  console.log(`\n=== REPORT: collapse-risk groups (informational, top ${LIMIT} of ${riskRows.length}) ===`);
  for (const r of riskRows.slice(0, LIMIT)) {
    console.log(`  ${String(r.groups).padStart(3)} group(s)  ${r.name}  (${r.rel})`);
  }
  if (riskRows.length > LIMIT) console.log(`  … ${riskRows.length - LIMIT} more.`);
  console.log('');
}

const failures = [];
if (twinRegressions.length > 0) failures.push(`${twinRegressions.length} resistible-twin regression(s)`);
if (sc3Missing > 0) failures.push(`${sc3Missing} SC-3 missing-resistible atom(s)`);
if (sc4Unaccounted > 0) failures.push(`${sc4Unaccounted} SC-4 unaccounted PvP sibling(s)`);
if (sc5Leaks.length > 0) failures.push(`${sc5Leaks.length} SC-5 routing leak(s)`);

if (failures.length === 0) {
  console.log(
    `GATE PASS — DSH3 + SC-3/4/5 clean (${datasetId}). ` +
    `[SC3 checked ${sc3Checked}; SC4 sibling families ${sc4SiblingPairs}; SC5 leaks 0]`,
  );
} else {
  console.log(`GATE FAIL — ${datasetId}: ${failures.join('; ')}.`);
}
process.exit(failures.length > 0 ? 1 : 0);
