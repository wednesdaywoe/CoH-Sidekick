#!/usr/bin/env node
// Guard for the Thunderspy damage-type SET recovery (chip source). The tspy damage
// atoms are `Unmapped`, so the planner cannot derive their type from the atoms; the
// power-level set is recovered from `attack_types` (÷4 → ATTRIB_NAME index) ∪ the
// shortHelp `DMG()`/`DoT()` clauses (see convert-powerset.cjs::damageTypeSetFromPower).
//
// Three assertions, chosen because the raw-export atom set is an UNRELIABLE oracle for
// this (pseudopet/redirect crossing per METHOD-2, plus real damage the game advertises
// but does not atomize as an Absolute template — Thunder Strike's Smashing). So we do
// NOT assert "recovery ⊆ atoms". Instead:
//   1. The JS ÷4 decode map equals the parser's ATTRIB_NAME[29..36] (no drift — this is
//      the one place binary-index knowledge is duplicated into JS, so it self-checks).
//   2. Corpus: every recovered type is a valid damage element (no garbage from a map or
//      regex regression) — across ALL datasets.
//   3. Golden cases: exact recovery on hand-verified powers (single, DoT-secondary,
//      multi-component, and the Fiery-Embrace-rider power whose chance-0 Fire is
//      correctly NOT advertised by the shortHelp, so recovery must exclude it).
//
// Run: node scripts/audit-damage-type-recovery.cjs  (exit 1 on any failure)

const fs = require('fs');
const path = require('path');
const { damageTypeSetFromPower, DAMAGE_ATTRIB_INDEX } = require('./convert-powerset.cjs');

const EXPORT_ROOT = path.join(__dirname, '..', 'exported_powers');
const VALID_ELEMENTS = new Set([
  'Smashing', 'Lethal', 'Fire', 'Cold', 'Energy', 'Negative', 'Psionic', 'Toxic',
  'Radiation', 'Electrical', 'Sonic', 'Quantum',
]);
const SIBLING_DATASETS = new Set(['thunderspy', 'rebirth']);

const failures = [];
function check(cond, msg) { if (!cond) failures.push(msg); }

// ---- 1. ÷4 map == ATTRIB_NAME[29..36] (Rule-0 drift check) ----
(() => {
  const src = fs.readFileSync(
    path.join(__dirname, '..', 'tools', 'bin-crawler', 'bin_crawler', 'parser', '_enums.py'),
    'utf8',
  );
  const body = src.match(/\nATTRIB_NAME: dict\[int, str\] = \{([\s\S]*?)\n\}/);
  check(!!body, 'could not locate ATTRIB_NAME in _enums.py');
  const attrib = {};
  for (const m of (body ? body[1] : '').matchAll(/(\d+):\s*"([^"]+)"/g)) attrib[+m[1]] = m[2];
  // The parser spells them `<Type>` / `Negative_Energy`; the ÷4 map canonicalizes to
  // the SubType wire names. Only the element identity must line up 1:1.
  const PARSER_TO_WIRE = {
    Smashing: 'Smashing', Lethal: 'Lethal', Fire: 'Fire', Cold: 'Cold', Energy: 'Energy',
    Negative_Energy: 'Negative', Psionic: 'Psionic', Toxic: 'Toxic',
  };
  for (const [idx, wire] of Object.entries(DAMAGE_ATTRIB_INDEX)) {
    const parserName = attrib[+idx];
    check(
      PARSER_TO_WIRE[parserName] === wire,
      `÷4 map drift: index ${idx} → "${wire}" but ATTRIB_NAME[${idx}] = "${parserName}"`,
    );
  }
})();

// ---- 2. corpus: no garbage type, any dataset ----
function walk(dir, out, crossSiblings) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (!crossSiblings && SIBLING_DATASETS.has(e.name)) continue;
    const f = path.join(dir, e.name);
    if (e.isDirectory()) walk(f, out, crossSiblings);
    else if (e.name.endsWith('.json') && e.name !== 'index.json') out.push(f);
  }
}
const allFiles = [];
walk(EXPORT_ROOT, allFiles, true); // include tspy + rebirth — the map/regex must be clean everywhere
let recovered = 0;
for (const f of allFiles) {
  let d;
  try { d = JSON.parse(fs.readFileSync(f)); } catch { continue; }
  const set = damageTypeSetFromPower(d);
  if (set.length) recovered++;
  for (const t of set) {
    check(VALID_ELEMENTS.has(t), `garbage recovered type "${t}" in ${path.relative(EXPORT_ROOT, f)}`);
  }
}

// ---- 3. golden cases (hand-verified) ----
function findPower(basename, datasetSubdir) {
  const root = datasetSubdir ? path.join(EXPORT_ROOT, datasetSubdir) : EXPORT_ROOT;
  const out = [];
  walk(root, out, !!datasetSubdir);
  const hit = out.find((f) => path.basename(f) === basename && !(!datasetSubdir && (f.includes(`${path.sep}thunderspy${path.sep}`) || f.includes(`${path.sep}rebirth${path.sep}`))));
  return hit ? JSON.parse(fs.readFileSync(hit)) : null;
}
const GOLDEN = [
  // [basename, datasetSubdir(null=HC), expected sorted set, note]
  ['gloom.json', null, ['Negative'], 'single element via attack_types + DoT'],
  ['blazing_arrow.json', null, ['Fire', 'Lethal'], 'DoT secondary recovered (was dropped by DMG-only)'],
  ['beheader.json', null, ['Lethal'], 'Fiery-Embrace chance-0 Fire NOT advertised → excluded'],
  ['blaze.json', 'thunderspy', ['Fire'], 'tspy: attack_types ÷4 + shortHelp'],
];
for (const [base, ds, expected, note] of GOLDEN) {
  const d = findPower(base, ds);
  if (!d) { failures.push(`golden: power ${base} (${ds || 'HC'}) not found`); continue; }
  const got = damageTypeSetFromPower(d);
  check(
    JSON.stringify(got) === JSON.stringify(expected),
    `golden: ${base} (${ds || 'HC'}) → [${got}] expected [${expected}] — ${note}`,
  );
}

if (failures.length) {
  console.error(`FAIL — damage-type recovery guard (${failures.length}):`);
  for (const m of failures) console.error('  · ' + m);
  process.exit(1);
}
console.log(`OK — damage-type recovery: ÷4 map matches ATTRIB_NAME; ${recovered} powers recovered corpus-wide, 0 garbage types; ${GOLDEN.length} golden cases exact.`);
