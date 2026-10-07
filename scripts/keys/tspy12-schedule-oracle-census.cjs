#!/usr/bin/env node
/**
 * Key for DATA-GAP TSPY-12: Mids' NLevels grades our slot schedule on two of the four forks.
 *
 * Run from the repo root:  node scripts/keys/tspy12-schedule-oracle-census.cjs
 *
 * MBDIMPORT-11 leaned on one oracle — "`NLevels.mhd` is byte-identical to our export's
 * schedule" — and it is, on the two forks it was read against. The row's door-closing claim is
 * that the sentence does NOT generalise: Mids' Thunderspy table is the legacy `Generic` table
 * with two cells hand-edited, so it is a second author's guess rather than a read of Thunderspy's
 * data, and our Thunderspy schedule is graded by nothing outside our own parser.
 *
 * That is exactly the claim that rots silently. Mids ships Thunderspy support separately; the day
 * that table becomes a real derivation it starts grading us, and nobody would notice, because no
 * test compares the two.
 *
 * All four forks are censused, because "two, not three" was itself a sentence about the roster
 * the day it was written. Brainstorm is the leg the count turns on: it has no Mids database and
 * cannot have one, so it is graded against Mids' HOMECOMING table, which is the table a
 * Brainstorm planner's own Mids file is authored in. That is a real oracle rather than a
 * stand-in only while the beta's schedule is still Homecoming's — it is, at all 50 levels — and
 * the day it stops being, the beta moved rather than the oracle, so the census says so and
 * does not break.
 *
 * What BREAKS the claim:
 *   - Homecoming or Rebirth diverging from ours — the oracle moved, and the row is describing the
 *     wrong fork;
 *   - Mids' Thunderspy NLevels ceasing to be Generic's table plus edits at 17 and 19 — it has
 *     been re-derived, and it can grade our Thunderspy read after all;
 *   - our Thunderspy schedule ceasing to be Homecoming's plus +1 at 9, 23, 29 and 43 — the
 *     population the row measures has moved.
 */
const fs = require('node:fs');
const path = require('node:path');
const { ALL_DATASETS } = require('../_dataset-paths.cjs');

const REPO = path.join(__dirname, '..', '..');
const DB_ROOT =
  process.env.MIDS_DB_ROOT ||
  path.join(process.env.MIDS_WINEPREFIX || path.join(require('node:os').homedir(), 'Games', 'mids-reborn'),
            'drive_c', 'MidsReborn', 'Databases');

function midsTable(file) {
  if (!fs.existsSync(file)) {
    console.error(`MISSING ${file}\n  set MIDS_DB_ROOT, or MIDS_WINEPREFIX (currently ${DB_ROOT})`);
    process.exit(2);
  }
  const slots = new Map();
  for (const line of fs.readFileSync(file, 'latin1').split(/\r?\n/).slice(1)) {
    if (!line.trim()) continue;
    const cell = line.split('\t');
    const level = Number(cell[0]);
    if (!Number.isFinite(level) || level === 0) continue;
    slots.set(level, cell[2] ? Number(cell[2]) : 0);
  }
  return slots;
}

function exportTable(fork) {
  const doc = JSON.parse(fs.readFileSync(path.join(REPO, 'contract', fork, 'leveling-schedule.json'), 'utf8'));
  const slots = new Map();
  for (const [level, count] of Object.entries(doc.slotGrants)) slots.set(Number(level), count);
  return slots;
}

const sum = (t) => [...t.values()].reduce((a, b) => a + b, 0);
const at = (t, l) => t.get(l) ?? 0;

/** Every level where two schedules disagree, as `L<level> a=x b=y`. */
function diff(a, b) {
  const out = [];
  for (let level = 1; level <= 50; level += 1) if (at(a, level) !== at(b, level)) out.push([level, at(a, level), at(b, level)]);
  return out;
}

/**
 * The Mids NLevels each fork is graded against, and whether Mids ships that fork a database of
 * its OWN. The second field is the break rule, not decoration: a disagreement is only evidence
 * the oracle moved where the oracle is Mids' own read of that fork's data.
 *
 * Mids has four databases and we ship four forks, and they are not the same four. Thunderspy's
 * is a third-party drop of Mids' Generic table with two cells edited. Brainstorm's is
 * Homecoming's, because Mids ships no Brainstorm database and a Brainstorm build is authored in
 * Mids' Homecoming one — `MIDS_DATABASE_FOR_DATASET` in src/utils/mids-import/mappers.ts is
 * where that routing lives, and this table is the level-schedule half of it.
 */
const NLEVELS_FOR = {
  homecoming: { file: path.join(DB_ROOT, 'Homecoming', 'NLevels.mhd'), own: true, note: '' },
  rebirth: { file: path.join(DB_ROOT, 'Rebirth', 'NLevels.mhd'), own: true, note: '' },
  thunderspy: {
    file: path.join(REPO, 'Thunderspy', 'NLevels.mhd'),
    own: false,
    note: 'third-party drop',
  },
  brainstorm: {
    file: path.join(DB_ROOT, 'Homecoming', 'NLevels.mhd'),
    own: false,
    note: "Mids' Homecoming table",
  },
};

const midsN = { generic: midsTable(path.join(DB_ROOT, 'Generic', 'NLevels.mhd')) };
const ours = {};
for (const fork of ALL_DATASETS) {
  const entry = NLEVELS_FOR[fork];
  if (!entry) {
    console.error(`no NLEVELS_FOR row for ${fork} — this census would report `
      + `${Object.keys(NLEVELS_FOR).length} of ${ALL_DATASETS.length} forks and exit 0.`);
    process.exit(2);
  }
  midsN[fork] = midsTable(entry.file);
  ours[fork] = exportTable(fork);
}

const broken = [];
const show = (d) => (d.length ? d.map(([l, a, b]) => `L${l} ${a}/${b}`).join('  ') : 'identical');

console.log('our export vs Mids NLevels, per fork  (ours/mids at each disagreeing level)\n');
for (const fork of ALL_DATASETS) {
  const d = diff(ours[fork], midsN[fork]);
  const { own, note } = NLEVELS_FOR[fork];
  console.log(`${fork.padEnd(12)} ours=${String(sum(ours[fork])).padStart(3)}  mids=${String(sum(midsN[fork])).padStart(3)}  ${show(d)}${note ? `  (${note})` : ''}`);
  // `own` carries the whole break rule. Where Mids has not read this fork's data itself, a
  // disagreement is what the row is ABOUT, so reporting it is the finding and breaking on it
  // would make the key red on the state it exists to describe.
  if (own && d.length) broken.push(`${fork}: the oracle no longer agrees with our export`);
  if (fork === 'brainstorm' && d.length) {
    console.log(`             ^ the beta's schedule has left Homecoming's. Not a break — a Mids `
      + `Homecoming table grades Brainstorm only while the two agree, and this says they no longer do.`);
  }
}

console.log('\nwhat Mids\' Thunderspy table descends from  (generic/thunderspy)');
const descent = diff(midsN.generic, midsN.thunderspy);
console.log(`  vs Mids Generic     ${show(descent)}`);
console.log(`  vs Mids Homecoming  ${show(diff(midsN.homecoming, midsN.thunderspy))}`);
if (descent.length !== 2 || descent[0][0] !== 17 || descent[1][0] !== 19) {
  broken.push("Mids' Thunderspy NLevels is no longer Generic plus edits at 17 and 19 — it may be a real read now");
}

console.log('\nour Thunderspy schedule vs our Homecoming one  (hc/ts)');
const oursTs = diff(ours.homecoming, ours.thunderspy);
console.log(`  ${show(oursTs)}`);
const expected = [9, 23, 29, 43];
if (oursTs.length !== expected.length || oursTs.some(([l, a, b], i) => l !== expected[i] || b - a !== 1)) {
  broken.push('our Thunderspy schedule is no longer Homecoming +1 at 9/23/29/43 — the row\'s population moved');
}

console.log('');
if (broken.length) {
  console.log('BREAKS the row:');
  for (const b of broken) console.log('  -', b);
  process.exit(1);
}
console.log("holds: the oracle is exact on Homecoming and Rebirth, and on Thunderspy it is Mids'");
console.log('legacy Generic table with two cells moved — it grades our Thunderspy read with nothing.');
console.log("Brainstorm is graded by Homecoming's table, which is the one its builds are authored");
console.log('in, and the two schedules still agree at every level.');
