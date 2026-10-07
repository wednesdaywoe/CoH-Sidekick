#!/usr/bin/env node
/**
 * MBDEXPORT-19's key, now a CLOSURE pin: "every slider Mids states, we state back".
 *
 * It opened the other way round — the writer stated `VariableValue: 0` as a literal in four
 * places, so ten sliders over five files died on the way out. Both of our own sides agreed,
 * because our reader treats an absent slider as 0 too, which is why this censuses Mids' files
 * against ours rather than a round trip.
 *
 * Closed 2026-09-11, and the two assertions below are what hold it closed:
 *
 *   TOTAL must stay 0        — no slider Mids states differs from the one we state back.
 *   SURVIVORS must stay 10   — the population that proves it. Without this, deleting the
 *                              feature scores a perfect 0, since a field neither side writes
 *                              also never differs. A zero here means the literal came back.
 *
 * Canonical-only: reads fixtures/mids/.
 */
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..');
const OURS = path.join(ROOT, 'fixtures', 'mids', 'ours');
const EXPECTED_TOTAL = 0;
const EXPECTED_SURVIVORS = 10;

function twin(base) {
  for (const fork of ['homecoming', 'rebirth']) {
    const p = path.join(ROOT, 'fixtures', 'mids', fork, base);
    if (fs.existsSync(p)) return p;
  }
  return null;
}

const rows = [];
let total = 0;
let oursNonZero = 0;
for (const base of fs.readdirSync(OURS).filter((f) => f.endsWith('.mbd')).sort()) {
  const mids = twin(base);
  if (!mids) continue;
  const ours = JSON.parse(fs.readFileSync(path.join(OURS, base), 'utf8'));
  const theirs = JSON.parse(fs.readFileSync(mids, 'utf8'));
  const o = new Map(ours.PowerEntries.map((e) => [e.PowerName, e]));
  const hits = [];
  for (const me of theirs.PowerEntries) {
    const oe = o.get(me.PowerName);
    if (!oe) continue;
    if ((oe.VariableValue ?? 0) !== 0) oursNonZero += 1;
    if ((me.VariableValue ?? 0) !== (oe.VariableValue ?? 0)) {
      hits.push(`${me.PowerName.split('.').pop()} mids=${me.VariableValue} ours=${oe.VariableValue}`);
    }
  }
  if (hits.length) rows.push({ base, hits });
  total += hits.length;
}

console.log('MBDEXPORT-19: powers whose VariableValue slider does not survive the round trip\n');
for (const { base, hits } of rows.sort((x, y) => y.hits.length - x.hits.length)) {
  console.log(`  ${String(hits.length).padStart(2)}  ${base}`);
  for (const h of hits) console.log(`        ${h}`);
}
console.log(`\n  TOTAL ${total} powers over ${rows.length} files`);
console.log(`  SURVIVORS ${oursNonZero} entries carry a non-zero slider out of our writer`);

const broken = [];
if (total !== EXPECTED_TOTAL) {
  broken.push(`total is ${total}, the row states ${EXPECTED_TOTAL} — a slider stopped surviving`);
}
if (oursNonZero !== EXPECTED_SURVIVORS) {
  broken.push(`our writer states ${oursNonZero} non-zero sliders, the row states ` +
              `${EXPECTED_SURVIVORS} — at 0 the literal is back and the total above is ` +
              'passing for the wrong reason; any other number means the CORPUS moved');
}
if (broken.length) {
  console.log('\nBREAKS:');
  for (const b of broken) console.log(`  - ${b}`);
  process.exit(1);
}
console.log('\nholds: all 10 sliders Mids states survive the round trip.');
