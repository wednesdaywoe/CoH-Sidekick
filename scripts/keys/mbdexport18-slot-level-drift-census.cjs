#!/usr/bin/env node
/**
 * MBDEXPORT-18's key: "a build exported here states the slot levels its author placed".
 *
 * The claim stops a session looking, because our own round trip agrees with
 * itself here — the reader and the writer share one solver, so the file we
 * write back is whatever that solver synthesised, not what the author placed.
 * Only Mids' own file states the latter, which is why this censuses the FILES
 * rather than the builds.
 *
 * The header this file carried until 2026-09-11 said the opposite — "the reader
 * retains slot levels verbatim" — and that was the row's original misdiagnosis.
 * The reader retained nothing: `SlotEntries[].Level` reached `midsMaxUsedLevel`,
 * which takes one max over the whole file, and was read nowhere else.
 *
 * Exits non-zero if the population moves, so a drop that comes from the corpus
 * changing rather than the defect closing is visible. The residual is sorted
 * into its causes, because three unrelated ones share this key's population and
 * the split is what a session picking the row up next actually needs.
 *
 * Canonical-only: reads fixtures/mids/.
 */
const fs = require('fs');
const path = require('path');

const ROOT = path.resolve(__dirname, '..', '..');
const OURS = path.join(ROOT, 'fixtures', 'mids', 'ours');
const EXPECTED_TOTAL = 0;
const EXPECTED_FILES = 0;
/**
 * The floor under the agreement, which is what stops a zero scored by an empty
 * corpus reading as a close. Set under the population the eight files agreed on
 * when the row closed (2026-09-12: 292), so it reds on a writer that has gone
 * back to synthesising and on a corpus that has lost files.
 */
const EXPECTED_AGREE_AT_LEAST = 280;

/**
 * What each row WAS, in the order they were adjudicated. All four are at zero
 * since 2026-09-12 and the causes are kept rather than deleted, because each is
 * a distinct way for this to come back and the classifier still sorts them.
 *
 * `expected` is a pin per cause, not decoration: the total held flat across the
 * import fix at a coincidence of arithmetic once already, and a per-cause count
 * is what tells a genuine close apart from one cause growing as another shrinks.
 */
const CAUSES = {
  inherentPick: {
    expected: 0,
    label: "the inherent pick level — closed by reading the fork's own available_level",
  },
  respecRows: {
    expected: 0,
    label: "Mids' respec rows at 47/49 — closed by the writer carrying them; MBDIMPORT-14 still owns the question",
  },
  overPlaced: {
    expected: 0,
    label: 'Rebirth placing more slots at 9/13/17/23 than our table grants — same carry, same open question',
  },
  formSubPower: {
    expected: 0,
    label: 'Kheldian form sub-powers — closed by seeding their file rows and letting them reach the solver',
  },
};

function twin(base) {
  for (const fork of ['homecoming', 'rebirth']) {
    const p = path.join(ROOT, 'fixtures', 'mids', fork, base);
    if (fs.existsSync(p)) return p;
  }
  return null;
}

/**
 * Which cause a divergent power belongs to.
 *
 * Read off the shape of the disagreement rather than off a power name — a name
 * in a conditional is the thing Rule 0 forbids, and the shapes are what the
 * causes actually differ in. A form sub-power is the one that needs a roster,
 * and `SubPowerEntries` does not carry it, so it is recognised by its signature
 * instead: every slot collapsed onto the power's own pick level, which is what
 * `addAutoGrantedPowers` stamps when a power never reaches the solver.
 */
function classify(mids, ours) {
  const differing = [];
  for (let i = 0; i < Math.max(mids.length, ours.length); i++) {
    if (mids[i] !== ours[i]) differing.push(i);
  }
  if (differing.length === 1 && differing[0] === 0) return 'inherentPick';
  if (ours.length > 1 && ours.every((lvl) => lvl === ours[0])) return 'formSubPower';
  const placed = differing.filter((i) => i > 0).map((i) => mids[i]);
  if (placed.some((lvl) => lvl === 47 || lvl === 49)) return 'respecRows';
  return 'overPlaced';
}

const rows = [];
const found = Object.fromEntries(Object.keys(CAUSES).map((k) => [k, []]));
let total = 0;
let agree = 0;
for (const base of fs.readdirSync(OURS).filter((f) => f.endsWith('.mbd')).sort()) {
  const mids = twin(base);
  if (!mids) continue;
  const ours = JSON.parse(fs.readFileSync(path.join(OURS, base), 'utf8'));
  const theirs = JSON.parse(fs.readFileSync(mids, 'utf8'));
  const byName = (d) => new Map(d.PowerEntries.map((e) => [e.PowerName, e]));
  const o = byName(ours);
  const m = byName(theirs);
  let n = 0;
  const examples = [];
  for (const [name, me] of m) {
    const oe = o.get(name);
    if (!oe) continue;
    const a = me.SlotEntries.map((s) => s.Level);
    const b = oe.SlotEntries.map((s) => s.Level);
    if (a.join() === b.join()) { agree += 1; continue; }
    n += 1;
    const cause = classify(a, b);
    found[cause].push(`${base.slice(0, 20)} ${name.split('.').pop()} mids[${a}] ours[${b}]`);
    if (examples.length < 2) examples.push(`${name.split('.').pop()} mids[${a}] ours[${b}]`);
  }
  if (n) rows.push({ base, n, examples });
  total += n;
}

console.log('MBDEXPORT-18: powers whose placed-slot LEVEL list differs from the file the build came from\n');
for (const { base, n, examples } of rows.sort((x, y) => y.n - x.n)) {
  console.log(`  ${String(n).padStart(3)}  ${base}`);
  for (const e of examples) console.log(`       ${e}`);
}
console.log(`\n  TOTAL ${total} powers differ; ${agree} agree, over 8 files`);

console.log('\nwhat is left, by cause (all four closed 2026-09-12):\n');
for (const [key, { expected, label }] of Object.entries(CAUSES)) {
  console.log(`  ${String(found[key].length).padStart(3)} (pinned ${expected})  ${label}`);
  for (const e of found[key].slice(0, 2)) console.log(`         ${e}`);
}

const broken = [];
if (total !== EXPECTED_TOTAL) broken.push(`total is ${total}, the row states ${EXPECTED_TOTAL}`);
if (rows.length !== EXPECTED_FILES) broken.push(`${rows.length} files affected, the row states ${EXPECTED_FILES}`);
if (agree < EXPECTED_AGREE_AT_LEAST) {
  broken.push(`only ${agree} powers agree, under the floor of ${EXPECTED_AGREE_AT_LEAST} — a zero scored on an empty corpus is not a close`);
}
for (const [key, { expected }] of Object.entries(CAUSES)) {
  if (found[key].length !== expected) {
    broken.push(`${key} is ${found[key].length}, the row states ${expected}`);
  }
}
if (broken.length) {
  console.log('\nBREAKS:');
  for (const b of broken) console.log(`  - ${b}`);
  console.log('  A fall here without a reader change means the CORPUS moved, not the defect.');
  process.exit(1);
}
console.log(`\nholds: every slot level the corpus states survives the round trip, on ${agree} powers.`);
