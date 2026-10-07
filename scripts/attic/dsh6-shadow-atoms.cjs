/**
 * DSH6 Phase 0a — shadow cross-check of the converter-local atom ingest.
 *
 * Validates `templatesToAtoms` (convert-powerset.cjs) — the PRODUCTION atom
 * encoder that will eventually feed the PowerEffects projection — against the
 * REFERENCE encoder `ingestExportGroup` (scripts/_atomic-effect.ts),
 * per power, across a dataset's committed `exported_powers/` tree.
 *
 * Invariant checked (the "structural cross-check" from the Phase 0 plan):
 * the converter path is  collectAllTemplates(effects) → templatesToAtoms  —
 * the collector only DROPS groups (PvP-only, enttype-player twins, dead-state
 * / conditional gates, containment/domination tags, empty chance=0 procs);
 * it never invents or rewrites a template, and `templatesToAtoms` shares the
 * per-template encoder (`ingestTemplate`) with the reference. Therefore the
 * converter's atom multiset must be a SUB-MULTISET of the reference atom
 * multiset (reference = every group, recursively including child_effects,
 * which `ingestExportPower` itself does not descend into). Any converter atom
 * whose identityKey is not available in the reference multiset = the two
 * encoders disagree about the same template ⇒ ingest skew ⇒ FAIL.
 *
 * This is a read-only harness — it never writes to generated/.
 *
 * Usage:
 *   node scripts/dsh6-shadow-atoms.cjs                        # HC
 *   node scripts/dsh6-shadow-atoms.cjs --dataset rebirth
 *   node scripts/dsh6-shadow-atoms.cjs --dataset thunderspy
 *   node scripts/dsh6-shadow-atoms.cjs --power Call_Depths    # filter + verbose
 */

require('tsx/cjs');
const fs = require('fs');
const path = require('path');
const {
  ingestExportGroup,
  identityKey,
} = require('../_atomic-effect.ts');
// NB: convert-powerset.cjs reads --dataset from argv at load, so this script's
// own --dataset flag configures it too.
const { collectAllTemplates, templatesToAtoms, RAW_DATA_PATH, CATEGORY_MAP } =
  require('./convert-powerset.cjs');

const argv = process.argv.slice(2);
const argVal = (f) => { const i = argv.indexOf(f); return i >= 0 ? argv[i + 1] : undefined; };
const POWER_FILTER = argVal('--power');

// Reference multiset: every group, recursively (child_effects carry their own
// full group context — is_pvp / chance / ppm / requires_expression).
function referenceAtoms(powerJson) {
  const atoms = [];
  const queue = [...(powerJson.effects || [])];
  while (queue.length > 0) {
    const g = queue.shift();
    atoms.push(...ingestExportGroup(g));
    if (g.child_effects) queue.push(...g.child_effects);
  }
  return atoms;
}

function toMultiset(atoms) {
  const m = new Map();
  for (const a of atoms) {
    const k = identityKey(a);
    m.set(k, (m.get(k) || 0) + 1);
  }
  return m;
}

const stats = {
  powers: 0,
  convAtoms: 0,
  refAtoms: 0,
  unmapped: 0,
  skewPowers: [],
};

function checkPower(powerJson, file) {
  if (POWER_FILTER && !(powerJson.name || '').includes(POWER_FILTER)
      && !file.includes(POWER_FILTER)) return;
  const conv = templatesToAtoms(collectAllTemplates(powerJson.effects || []));
  const ref = toMultiset(referenceAtoms(powerJson));
  stats.powers++;
  stats.convAtoms += conv.length;
  let refCount = 0; for (const n of ref.values()) refCount += n;
  stats.refAtoms += refCount;

  const missing = [];
  const seen = new Map();
  for (const a of conv) {
    if (a.effectType === 'Unmapped') stats.unmapped++;
    const k = identityKey(a);
    const used = (seen.get(k) || 0) + 1;
    seen.set(k, used);
    if (used > (ref.get(k) || 0)) missing.push(k);
  }
  if (missing.length > 0) {
    stats.skewPowers.push({ file, power: powerJson.name, missing });
  }
  if (POWER_FILTER) {
    console.log(`\n=== ${powerJson.name} (${file}) ===`);
    console.log(`converter atoms: ${conv.length}, reference atoms: ${refCount}`);
    for (const a of conv) console.log('  ' + identityKey(a));
  }
}

function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(p);
    else if (entry.name.endsWith('.json') && !entry.name.startsWith('_')
             && entry.name !== 'index.json') {
      let json;
      try { json = JSON.parse(fs.readFileSync(p, 'utf-8')); } catch (_) { continue; }
      if (json && json.effects) checkPower(json, path.relative(RAW_DATA_PATH, p));
    }
  }
}

for (const category of Object.keys(CATEGORY_MAP)) {
  const dir = path.join(RAW_DATA_PATH, category);
  if (fs.existsSync(dir)) walk(dir);
}

console.log(`\nDSH6 shadow atom cross-check (${RAW_DATA_PATH})`);
console.log(`  powers checked:        ${stats.powers}`);
console.log(`  converter atoms:       ${stats.convAtoms}`);
console.log(`  reference atoms:       ${stats.refAtoms} (superset — includes collector-dropped groups)`);
console.log(`  unmapped converter:    ${stats.unmapped} (${(100 * stats.unmapped / Math.max(1, stats.convAtoms)).toFixed(2)}%)`);
console.log(`  ingest-skew powers:    ${stats.skewPowers.length}`);

if (stats.skewPowers.length > 0) {
  console.log('\nSKEW (converter atom not found in reference multiset):');
  for (const s of stats.skewPowers.slice(0, 25)) {
    console.log(`  ${s.power}  [${s.file}]`);
    for (const k of s.missing.slice(0, 5)) console.log(`    - ${k}`);
  }
  process.exit(1);
}
console.log('OK — converter ingest is a sub-multiset of the reference encoding everywhere.');
