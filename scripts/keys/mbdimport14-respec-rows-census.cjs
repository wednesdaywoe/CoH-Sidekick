#!/usr/bin/env node
/**
 * Key for DATA-GAP MBDIMPORT-14: Mids' rows at 47 and 49 are the respec table's, on every fork.
 *
 * Run from the repo root:  node scripts/keys/mbdimport14-respec-rows-census.cjs
 *
 * The row's door-closing claim is that 47/49 is a property of Mids' RESPEC table and of nothing
 * else — not of a fork's data, not of the game's schedule, not of a build author's editing. That
 * claim is what stops the next session going looking for a second schedule in the binary, which
 * is the search MBDIMPORT-11 already ran once and closed (`ParseSchedules` has one member). If it
 * rots, nothing surfaces it: no reader and no writer touches these rows, so no test reds.
 *
 * So the census reads the tables directly, all of them: Mids ships a level schedule per database
 * as two plain TSVs, `NLevels.mhd` (normal) and `RLevels.mhd` (respec), and the four we hold are
 * Generic, Homecoming, Rebirth and Thunderspy. Each is paired against our own export's schedule
 * for the same fork.
 *
 * What BREAKS the claim:
 *   - any NLevels granting a slot at 47 or 49 — then the rows are not respec-only;
 *   - any of our exports granting one — then the game's schedule has them and the suspect half
 *     of MBDIMPORT-11's reading is inverted;
 *   - an RLevels granting something other than 3 at either level — then the rows vary by fork and
 *     "Mids' convention" is the wrong description.
 * A missing database is a hard error, not a skip: a key that silently censuses three of four
 * forks is the soft default this repo keeps being bitten by.
 */
const fs = require('node:fs');
const path = require('node:path');
const { ALL_DATASETS } = require('../_dataset-paths.cjs');

const REPO = path.join(__dirname, '..', '..');
const DB_ROOT =
  process.env.MIDS_DB_ROOT ||
  path.join(process.env.MIDS_WINEPREFIX || path.join(require('node:os').homedir(), 'Games', 'mids-reborn'),
            'drive_c', 'MidsReborn', 'Databases');

/** Mids' level tables are TSV: Level, Power, Slots, Inspirations, Other. Latin-1, CRLF. */
function midsTable(file) {
  const rows = fs.readFileSync(file, 'latin1').split(/\r?\n/).slice(1);
  const slots = new Map();
  for (const line of rows) {
    if (!line.trim()) continue;
    const cell = line.split('\t');
    const level = Number(cell[0]);
    if (!Number.isFinite(level) || level === 0) continue;
    slots.set(level, cell[2] ? Number(cell[2]) : 0);
  }
  return slots;
}

/** Our own schedule, as the contract states it. */
function exportTable(fork) {
  const doc = JSON.parse(fs.readFileSync(path.join(REPO, 'contract', fork, 'leveling-schedule.json'), 'utf8'));
  const slots = new Map();
  for (const [level, count] of Object.entries(doc.slotGrants)) slots.set(Number(level), count);
  return { slots, total: doc.totalSlots };
}

const sum = (t) => [...t.values()].reduce((a, b) => a + b, 0);
const at = (t, level) => t.get(level) ?? 0;

// Mids' database name -> the fork our export calls it. Generic is Mids' pre-fork legacy table and
// has no counterpart of ours; it is censused because a claim about "every fork" that skips the
// one table nobody maintains is a claim about the maintained ones.
//
// Brainstorm is that case mirrored: our fork with no Mids table, because Mids ships no
// Brainstorm database (MBDEXPORT-2) and a Brainstorm build is authored in Mids' Homecoming one.
// It carries a row with a null `dir` rather than a paragraph after the loop, which is where it
// used to live. A fork handled outside the roster is a fork the roster audit cannot see, and
// the audit not seeing it is the whole failure this census's own header warns about.
const DATABASES = [
  { mids: 'Generic', dir: path.join(DB_ROOT, 'Generic'), fork: null },
  { mids: 'Homecoming', dir: path.join(DB_ROOT, 'Homecoming'), fork: 'homecoming' },
  { mids: 'Rebirth', dir: path.join(DB_ROOT, 'Rebirth'), fork: 'rebirth' },
  { mids: 'Thunderspy', dir: path.join(REPO, 'Thunderspy'), fork: 'thunderspy' },
  { mids: null, dir: null, fork: 'brainstorm' },
];

// The roster check the header promises. A fork with no row would be censused by nothing and
// the key would still print a table and exit 0 — three of four forks, reported as four.
const covered = new Set(DATABASES.map((db) => db.fork).filter(Boolean));
for (const fork of ALL_DATASETS) {
  if (!covered.has(fork)) {
    console.error(`no DATABASES row for ${fork} — this census would report ${covered.size} of `
      + `${ALL_DATASETS.length} forks and exit 0. Add its row before trusting the output.`);
    process.exit(2);
  }
}

const broken = [];
console.log('level 47 and 49, per Mids database and per export\n');
console.log('database      NLevels           RLevels           our export');
console.log('------------  ----------------  ----------------  ----------------');

for (const db of DATABASES) {
  let n = null;
  let r = null;
  if (db.dir) {
    const nFile = path.join(db.dir, 'NLevels.mhd');
    const rFile = path.join(db.dir, 'RLevels.mhd');
    for (const f of [nFile, rFile]) {
      if (!fs.existsSync(f)) {
        console.error(`\nMISSING ${f}\n  set MIDS_DB_ROOT, or MIDS_WINEPREFIX (currently ${DB_ROOT})`);
        process.exit(2);
      }
    }
    n = midsTable(nFile);
    r = midsTable(rFile);
  }
  const ours = db.fork ? exportTable(db.fork) : null;

  const cell = (t) => (t ? `${String(sum(t)).padStart(3)}  47:${at(t, 47)} 49:${at(t, 49)}` : '  —');
  console.log(
    (db.mids ?? `(${db.fork})`).padEnd(13),
    cell(n).padEnd(17),
    cell(r).padEnd(17),
    ours ? `${String(ours.total).padStart(3)}  47:${at(ours.slots, 47)} 49:${at(ours.slots, 49)}` : '   —',
  );

  if (n && (at(n, 47) || at(n, 49))) broken.push(`${db.mids}: NLevels grants at 47/49 — the rows are not respec-only`);
  if (r && (at(r, 47) !== 3 || at(r, 49) !== 3)) broken.push(`${db.mids}: RLevels grants ${at(r, 47)}/${at(r, 49)}, not 3/3 — the rows vary by fork`);
  if (ours && (at(ours.slots, 47) || at(ours.slots, 49))) {
    broken.push(`${db.fork}: our export grants at 47/49 — the game's schedule has them after all`);
  }
}

console.log('');
if (broken.length) {
  console.log('BREAKS the row:');
  for (const b of broken) console.log('  -', b);
  process.exit(1);
}
console.log('holds: every NLevels and every export grants 0 at both levels; every RLevels grants 3 at both.');
console.log('47/49 is Mids\' respec table and nothing else.');
