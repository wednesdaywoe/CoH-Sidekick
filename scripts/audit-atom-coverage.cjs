#!/usr/bin/env node
/**
 * audit-atom-coverage.cjs — how much of the game's effect vocabulary does the
 * test corpus actually touch?
 *
 * `coh_math` is 29,059 lines and the workspace contains zero `#[test]`. The
 * fixtures that would grade it are committed (`fixtures/totals/`, 143 builds;
 * `fixtures/oracle/`, 199,486 recorded answers) and nothing reads them. Before
 * writing more builds it is worth knowing which ones are worth writing, and that
 * needs a denominator nobody had measured: how many GENUINELY DIFFERENT atoms
 * exist.
 *
 * An atom (`AtomicEffect`) carries ~35 fields, so "every combination" is not a
 * target — almost all of them never occur. What matters is the combinations the
 * calculator BRANCHES on. This counts distinct values of:
 *
 *   effectType, aspect, toWho, attribType, subType   what it does, to whom
 *   stacking, pvMode                                 how it accumulates
 *   9 booleans                                       resistible, ignoreED, gated, ...
 *   12 presence bits                                 does it have a modifier table,
 *                                                    a tick period, a sub-1 chance,
 *                                                    a PPM, a gating expression,
 *                                                    a valuing expression, ...
 *
 * Measured 2026-09-26 over 153,297 atoms in 14,484 powers across four datasets:
 * 5,096 distinct shapes, 4,929 of them (96.7%) reachable from powers a character
 * can actually take. Coarser keys undercount badly — 32 by effect family alone,
 * 501 by the first row above — and the flags are exactly where calculator bugs
 * live, so the wide key is the honest one.
 *
 * The corpus covers 963 of 4,929 (19.5%), from only 169 distinct powers across
 * 512 picks: the existing builds were written to exercise calculations, not to
 * spread across the data.
 *
 *   node scripts/audit-atom-coverage.cjs              # report
 *   node scripts/audit-atom-coverage.cjs --gate       # ratchet, see below
 *   node scripts/audit-atom-coverage.cjs --next 20    # what to write next
 *   node scripts/audit-atom-coverage.cjs --write-baseline
 *
 * BREAKS THE CLAIM, two ways, and they mean opposite things:
 *
 *   covered < baseline   A fixture build lost powers, or a power lost atoms.
 *                        The corpus grades less than it did. Find out why.
 *   universe != baseline The DATA changed — a re-export landed, or the game
 *                        patched under one. Coverage is now measured against a
 *                        different denominator and the old number is not
 *                        comparable. Re-adjudicate, then --write-baseline.
 *
 * The second is deliberate and is what makes this a live instrument rather than
 * a frozen one. `prov8-shard-drift.cjs` says the game moved; this says what the
 * move did to the vocabulary the tests are supposed to cover. A gate that
 * silently re-based itself on new data would report a comfortable percentage
 * while the thing it measures had changed underneath — the failure this
 * repository keeps finding in its own guards.
 *
 * COVERAGE IS REACH, NOT CORRECTNESS. A build that touches an atom proves only
 * that the atom was touched. What makes the touch mean something is the recorded
 * answer it is compared against, and that lives in `fixtures/oracle/` and
 * `fixtures/totals/`, not here. This gate sizes the job; it does not do it.
 */

'use strict';

require('tsx/cjs');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { decodeAtoms } = require('./_atomic-effect.ts');
const { ALL_DATASETS } = require('./_dataset-paths.cjs');

const BASELINE_PATH = path.join(__dirname, 'audit-atom-coverage-baseline.json');

/** Build corpora that count as "covered". Add a directory here when its builds
 *  become something a test actually runs; a fixture nothing reads covers nothing,
 *  but these are the files the harness is being built against. */
const BUILD_CORPORA = ['fixtures/totals'];

const argv = process.argv.slice(2);
const argVal = (f) => { const i = argv.indexOf(f); return i >= 0 ? argv[i + 1] : undefined; };
const GATE = argv.includes('--gate');
const WRITE_BASELINE = argv.includes('--write-baseline');
const NEXT = argv.includes('--next') ? Number(argVal('--next') || 20) : 0;
const JSON_OUT = argVal('--json');

/** Partitions a character can take powers from. `other` is NPC/pet/critter data:
 *  real, exported, and unreachable from a build file, so counting it in the
 *  denominator would make full coverage permanently impossible. */
const PLAYER = new Set(['powerset', 'pool', 'epic', 'incarnate']);

function sourceOf(rel) {
  if (rel.includes(`${path.sep}powersets${path.sep}`)) return 'powerset';
  if (rel.endsWith('power-pools-raw.json')) return 'pool';
  if (rel.endsWith('epic-pools-raw.json')) return 'epic';
  if (rel.endsWith('incarnate-effects.json')) return 'incarnate';
  return 'other';
}

const has = (v) => (v === undefined || v === null ? '' : '1');

/** The branch key. Values, not just presence, for the five that name the effect;
 *  presence only for the rest, because a modifier table's NAME is data while its
 *  existence is a code path. */
function shapeOf(a) {
  return [
    a.effectType, a.aspect, a.toWho, a.attribType, a.subType,
    a.stacking, a.pvMode,
    a.resistible, a.ignoreStrength, a.buffable, a.ignoreED, a.ignoreScaling,
    a.gated, a.suppressible, a.notOnCaster, a.cancelOnMiss,
    has(a.modifierTable), has(a.ticks), has(a.applicationPeriod),
    (a.baseProbability !== undefined && a.baseProbability !== null && a.baseProbability < 1) ? 'p' : '',
    has(a.procsPerMinute), has(a.requiresExpression), has(a.magnitudeExpression),
    has(a.specialCase), has(a.requiredEvents), has(a.perTarget),
    has(a.stackKey), has(a.tags),
  ].join('|');
}

/* ---- the universe: every shape a build could reach ---------------------- */

const universe = new Set();
const byPower = new Map();      // "dataset:InternalName" -> Set(shape)
let atomsScanned = 0;
let powersWithAtoms = 0;

for (const dataset of ALL_DATASETS) {
  sweepDataset(dataset, (power, rel) => {
    if (!PLAYER.has(sourceOf(rel))) return;
    const atoms = Array.isArray(power.atoms) ? decodeAtoms(power.atoms) : [];
    if (!atoms.length) return;
    powersWithAtoms += 1;
    const key = `${dataset}:${power.internalName || power.name}`;
    let set = byPower.get(key);
    if (!set) { set = new Set(); byPower.set(key, set); }
    for (const a of atoms) {
      atomsScanned += 1;
      const s = shapeOf(a);
      set.add(s);
      universe.add(s);
    }
  });
}

/* ---- the corpus: every shape the committed builds touch ----------------- */

const covered = new Set();
const pickedPowers = new Set();
let buildCount = 0;
let unmatchedPicks = 0;

/** A power pick names a dataset and an internal name; the same name can sit in
 *  more than one powerset, so this unions every match. That OVERSTATES coverage
 *  slightly, in the safe direction for a ratchet: it can only make the measured
 *  number higher, never hide a regression. */
function takeBuild(build) {
  if (!build || !build.dataset) return;
  buildCount += 1;
  for (const slot of ['primary', 'secondary', 'pools', 'epic', 'incarnate']) {
    for (const set of [].concat(build[slot] || [])) {
      for (const p of (set && set.powers) || []) {
        const key = `${build.dataset}:${p.internal_name}`;
        pickedPowers.add(key);
        const shapes = byPower.get(key);
        if (!shapes) { unmatchedPicks += 1; continue; }
        for (const s of shapes) covered.add(s);
      }
    }
  }
}

for (const dir of BUILD_CORPORA) {
  const abs = path.join(ROOT, dir);
  if (!fs.existsSync(abs)) continue;
  const walk = (d) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) { walk(p); continue; }
      if (!p.endsWith('.jsonl')) continue;
      for (const line of fs.readFileSync(p, 'utf8').split('\n')) {
        if (!line.trim()) continue;
        let rec;
        try { rec = JSON.parse(line); } catch { continue; }
        takeBuild(rec.build || rec);
      }
    }
  };
  walk(abs);
}

/* ---- report ------------------------------------------------------------- */

const pct = (n, d) => (d ? (100 * n / d).toFixed(1) : '0.0');
console.log(`atoms scanned      ${atomsScanned} in ${powersWithAtoms} player-reachable powers`);
console.log(`distinct shapes    ${universe.size}`);
console.log(`build corpus       ${buildCount} builds, ${pickedPowers.size} distinct powers` +
            (unmatchedPicks ? `, ${unmatchedPicks} picks matched no exported power` : ''));
console.log(`shapes covered     ${covered.size} / ${universe.size}  (${pct(covered.size, universe.size)}%)`);
console.log(`shapes uncovered   ${universe.size - covered.size}`);

if (NEXT) {
  // Greedy: repeatedly take the power adding the most shapes nobody has yet.
  // This is the write-next list, in value order; the tail is long and flat, so
  // the point of printing it is knowing where to STOP, not finishing it.
  const need = new Set([...universe].filter((s) => !covered.has(s)));
  const entries = [...byPower.entries()];
  console.log(`\nhighest-value powers not yet in the corpus:`);
  for (let i = 0; i < NEXT && need.size; i += 1) {
    let best = null; let gain = 0;
    for (const [k, s] of entries) {
      if (pickedPowers.has(k)) continue;
      let g = 0;
      for (const x of s) if (need.has(x)) g += 1;
      if (g > gain) { gain = g; best = k; }
    }
    if (!best) break;
    console.log(`  +${String(gain).padStart(4)}  ${best}`);
    for (const x of byPower.get(best)) need.delete(x);
    pickedPowers.add(best);
  }
  console.log(`  ${need.size} shapes would still be uncovered after these`);
}

const report = { universe: universe.size, covered: covered.size, builds: buildCount, atoms: atomsScanned };
if (JSON_OUT) fs.writeFileSync(JSON_OUT, JSON.stringify(report, null, 1));

if (WRITE_BASELINE) {
  fs.writeFileSync(BASELINE_PATH, JSON.stringify({
    _note: 'Frozen atom-coverage baseline for audit-atom-coverage.cjs --gate. `universe` is the '
         + 'number of distinct atom branch-shapes reachable from player-takeable powers across all '
         + 'datasets; it changes ONLY when the exported data changes, so a mismatch means a '
         + 're-export or a game patch landed and the coverage figure is measured against a '
         + 'different denominator. `covered` is how many of them the committed build corpus '
         + 'touches; it must never go down. Refresh with --write-baseline after adjudicating.',
    universe: universe.size,
    covered: covered.size,
  }, null, 1) + '\n');
  console.log(`\nbaseline -> ${path.relative(process.cwd(), BASELINE_PATH)}`);
}

if (GATE && !WRITE_BASELINE) {
  if (!fs.existsSync(BASELINE_PATH)) {
    console.error(`\nGATE FAIL: no baseline at ${path.relative(ROOT, BASELINE_PATH)}. Run --write-baseline.`);
    process.exit(1);
  }
  const base = JSON.parse(fs.readFileSync(BASELINE_PATH, 'utf8'));
  const problems = [];
  if (universe.size !== base.universe) {
    problems.push(`the atom vocabulary CHANGED: ${base.universe} shapes recorded, ${universe.size} now. `
      + `The exported data moved under this gate — a re-export landed, or the game patched. `
      + `Coverage is no longer measured against the same denominator. Adjudicate, then --write-baseline.`);
  }
  if (covered.size < base.covered) {
    problems.push(`coverage REGRESSED: ${base.covered} shapes covered at baseline, ${covered.size} now. `
      + `A fixture build lost powers, or a power lost atoms.`);
  }
  if (problems.length) {
    console.error('\nGATE FAIL');
    for (const p of problems) console.error(`  !! ${p}`);
    process.exit(1);
  }
  console.log(`\nGATE PASS — ${covered.size}/${universe.size} shapes, baseline held`);
}
