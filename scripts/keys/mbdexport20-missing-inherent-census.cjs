#!/usr/bin/env node
/**
 * MBDEXPORT-20's key: "the archetype inherent is never written". CLOSED 2026-09-11.
 *
 * Found on 2026-09-11 by MBDEXPORT-19's round-trip grade, not by looking for it. Dark
 * Sustenance carries a `VariableValue` Mids states and our reader captures, and it still could
 * not survive the trip — because the entry is not written AT ALL. The -19 census could not see
 * that: it diffs a field across entries BOTH files carry, so an entry we drop is skipped rather
 * than counted.
 *
 * Every archetype in the corpus has one of these and every one was missing. The other entries we
 * drop are accounted for and NOT counted here: `Reconstruction` is a name this fork rotated away
 * (MBDIMPORT-2, declined on the way in), the Mastermind `_H` rows are Mids' henchman shadow
 * entries, and `Special_Set_Bonuses` / `Double_Jump` / `Fast_Snipe` / `Shadow_Step` /
 * `Shadow_Recall` / `Afterburner` are Mids-only artifacts on the reader's silent-skip list.
 * This counts the archetype inherent alone, which is a power the planner models and displays.
 *
 * A CLOSURE PIN WITH TWO NUMBERS, for the reason MBDEXPORT-19's needed two. MISSING must stay 0,
 * and a key that measured only that would score a perfect 0 if the corpus emptied, if the writer
 * stopped writing the grid, or if this file's own roster went blank. WRITTEN is the population
 * that makes the 0 mean something: 8 entries our writer emits under the name MIDS states, one
 * per file, on 8 of 8.
 *
 * It grades against MIDS' name, never against our own — the defect's whole shape was that our
 * side had a name for this power (`Inherent.<Archetype>.<Name>`, synthesised) and it was not the
 * one Mids reads. Comparing our output to our own roster would have passed throughout.
 *
 * Canonical-only: reads fixtures/mids/.
 */
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..');
const EXPECTED_MISSING = 0;
const EXPECTED_WRITTEN = 8;
const EXPECTED_FILES = 8;

/** The archetype inherent each corpus build's class carries, by the name Mids files it under. */
const ARCHETYPE_INHERENT = {
  'blaster-assault-rifle-tactical-arrow-v3861.mbd': 'Defiance',
  'stalker-martial-arts-willpower-v37521.mbd': 'Assassination',
  'stalker-martial-arts-willpower-v3860.mbd': 'Assassination',
  'warshade-umbral-blast-umbral-aura-v3860.mbd': 'Dark_Sustenance',
  'warshade-umbral-blast-umbral-aura-slots-only-v3860.mbd': 'Dark_Sustenance',
  'guardian-dark-assault-atmospheric-composition-v3860.mbd': 'Resolve',
  'mastermind-mercenaries-trick-arrow-v3860.mbd': 'Supremacy',
  'veat-night-widow-teamwork-v3860.mbd': 'Widow_Conditioning',
};

function twin(base) {
  for (const fork of ['homecoming', 'rebirth']) {
    const p = path.join(ROOT, 'fixtures', 'mids', fork, base);
    if (fs.existsSync(p)) return p;
  }
  return null;
}

const OURS = path.join(ROOT, 'fixtures', 'mids', 'ours');
const missing = [];
const written = [];
const files = new Set();
for (const base of fs.readdirSync(OURS).filter((f) => f.endsWith('.mbd')).sort()) {
  const midsPath = twin(base);
  const inherent = ARCHETYPE_INHERENT[base];
  if (!midsPath || !inherent) continue;
  files.add(base);
  const ours = JSON.parse(fs.readFileSync(path.join(OURS, base), 'utf8'));
  const theirs = JSON.parse(fs.readFileSync(midsPath, 'utf8'));
  const name = `Inherent.Inherent.${inherent}`;
  const midsHas = theirs.PowerEntries.some((e) => e.PowerName === name);
  const oursHas = ours.PowerEntries.some((e) => e.PowerName === name);
  if (midsHas && !oursHas) missing.push({ base, inherent });
  if (midsHas && oursHas) written.push({ base, inherent });
}

console.log("MBDEXPORT-20: the archetype inherent, under the name Mids reads it\n");
for (const { base, inherent } of written) console.log(`  ${inherent.padEnd(20)} written to ours/${base}`);
for (const { base, inherent } of missing) console.log(`  MISSING ${inherent.padEnd(12)} from ours/${base}`);
console.log(`\n  MISSING ${missing.length}, WRITTEN ${written.length} over ${files.size} of ${EXPECTED_FILES} files`);

const broken = [];
if (missing.length !== EXPECTED_MISSING) broken.push(`${missing.length} missing, the row states ${EXPECTED_MISSING}`);
if (written.length !== EXPECTED_WRITTEN) broken.push(`${written.length} written, the row states ${EXPECTED_WRITTEN}`);
if (files.size !== EXPECTED_FILES) broken.push(`${files.size} files measured, the row states ${EXPECTED_FILES}`);
if (broken.length) {
  console.log('\nBREAKS:');
  for (const b of broken) console.log(`  - ${b}`);
  console.log('  WRITTEN falling is the row reopening. MISSING and WRITTEN falling together is the');
  console.log('  CORPUS moving, and the population this pin is measured over is gone.');
  process.exit(1);
}
console.log("\nholds: every corpus build's archetype inherent goes out under Mids' own name.");
