/**
 * Form-detector coverage — does every power whose REDIRECT TABLE has an alternate-form
 * shape actually ship that form, whichever converter produced it?
 *
 * The defect class this exists for is not a wrong detector; it is a detector that a
 * converter never calls. Sidekick has three power converters (convert-powerset,
 * convert-pool-powers, convert-epic-pools) and each maintains its own emit block, so a
 * detector added to one is silently absent from the other two. That has now happened
 * three times, each time invisible to every existing gate because the affected powers
 * simply lacked a field nothing asserted was required:
 *
 *   - the six epic snipes, which shipped the redirect shell's fast cast against the slow
 *     branch's charged damage (SNIPE-2 — convert-epic-pools never called the extractor);
 *   - the pool/epic tier's missing atoms (Plan B — neither converter called
 *     `encodeAtomsForEmit`);
 *   - Aid Other, whose interruptible 3.93s cast becomes an uninterruptible 2.93s while the
 *     build holds Field Medic. Its redirect pair is the same shape as a snipe's and
 *     `findFastFormRedirect` has always matched it; convert-pool-powers simply never asked
 *     (CHAIN-1's first deferred bullet, measured 2026-08-07).
 *
 * So the check runs OUTSIDE all three converters, joining the committed export (their
 * shared input) to the committed generated tree (their combined output) through the shared
 * `planb-shadow-sweep` corpus walk — the sweep whose whole purpose is that a gate cannot
 * forget to look in a partition. The join itself lives in `_export-join.cjs`, shared with
 * `audit-conditional-coverage.cjs`, the fourth occurrence of the class (COND-2).
 *
 * The predicate is the converter's own `findFastFormRedirect`, called rather than
 * transcribed: a copy here could drift from the emit it grades and report a clean sweep
 * about a rule the converter no longer follows.
 *
 * One family is excluded, structurally: a redirect table with a `kMeter` branch is an
 * Assassin's Strike, whose mid-combat branch also drops the from-Hide interrupt and so
 * matches the same predicate. Its forms are modelled by `midCombatCast`, and the converter
 * refuses to claim it twice (`meterForms`); this refuses in the same terms, by the branch
 * condition rather than by a power name.
 *
 * Usage:
 *   node scripts/audit-form-coverage.cjs [--dataset <id>]... [--gate]
 *     --dataset <id>   repeatable; one of scripts/_dataset-paths.cjs ALL_DATASETS (default: all three)
 *     --gate           CI mode: one PASS/FAIL line per dataset, exit 1 on any failure
 */

require('tsx/cjs');
const fs = require('fs');
const path = require('path');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { joinerFor } = require('./_export-join.cjs');
const { gateText } = require('./_gate-tokens.cjs');

const argv = process.argv.slice(2);
const GATE = argv.includes('--gate');
const DATASETS = (() => {
  const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
  return picked.length ? picked : require('./_dataset-paths.cjs').ALL_DATASETS;
})();

/** A meter-branched (Assassin's Strike) table, whose forms are `midCombatCast`'s to model. */
const isMeterBranched = (json) =>
  (json.redirect || []).some((r) => gateText(r.condition_expression).includes('kMeter'));

function auditDataset(dataset) {
  const { cv, join } = joinerFor(dataset);
  const missing = [];
  const unjoined = [];
  const loadErrors = [];
  let swept = 0;
  let matched = 0;
  let meterFamily = 0;

  sweepDataset(
    dataset,
    (power, relPath) => {
      swept += 1;
      const exportPath = join(power, relPath);
      if (!exportPath) {
        unjoined.push(`${power.internalName || power.name} [${relPath}]`);
        return;
      }
      let json;
      try {
        json = JSON.parse(fs.readFileSync(exportPath, 'utf-8'));
      } catch (err) {
        loadErrors.push(`${relPath}: ${exportPath}: ${err.message}`);
        return;
      }
      if (!cv.findFastFormRedirect(json)) return;
      if (isMeterBranched(json)) {
        meterFamily += 1;
        return;
      }
      matched += 1;
      if (!power.quickSnipe) {
        missing.push({
          power: power.internalName || power.name,
          module: relPath,
          source: path.relative(cv.RAW_DATA_PATH, exportPath),
        });
      }
    },
    { onLoadError: (rel, err) => loadErrors.push(`${rel}: ${err.message}`) },
  );

  return { dataset, swept, unjoined, matched, meterFamily, missing, loadErrors };
}

/**
 * Non-vacuity floors. A join that stops resolving reports "0 matched, 0 missing" and reads
 * as a clean sweep — the failure mode this whole file is about, one level up. So the run
 * fails unless it actually resolved most of the corpus AND found the alternate-form
 * population each dataset is known to carry.
 *
 * The numbers are measured, not round: Homecoming's 47 snipes plus Aid Other, Rebirth's 47
 * plus Aid Other, Thunderspy's 8 plus Aid Other. `>=` so a new fast-form power is not a
 * failure; a collapse is.
 */
const FLOORS = {
  homecoming: 48,
  rebirth: 48,
  thunderspy: 9,
  // Measured 2026-08-22: 48, Homecoming's 47 snipes plus Aid Other. Equal to HC's number
  // and arrived at separately — this fork's own run, on 3,982 powers rather than 3,901.
  brainstorm: 48,
};

/**
 * How many generated powers may fail to resolve to an export. ZERO — every one of the 3,890 /
 * 3,299 / 3,278 joins today, and a power this cannot reach is a power it cannot check, which is
 * the same silence the gate exists to break. A future identity spelling the fold does not handle
 * should fail loudly here and be fixed in `_export-join.cjs`'s `fileFold`, not absorbed by a
 * tolerance.
 */
const MAX_UNJOINED = 0;

let failed = false;
for (const dataset of DATASETS) {
  const r = auditDataset(dataset);
  // No `?? 1` default. A floor of one passes any dataset that resolves a single power, so an
  // unlisted dataset used to be swept and graded against nothing — the vacuous pass this
  // whole file exists to prevent, one level up. An unmeasured dataset is a hard stop.
  const floor = FLOORS[r.dataset];
  if (floor === undefined) {
    console.log(`FAIL ${r.dataset}: no measured floor — add one to FLOORS before sweeping it`);
    failed = true;
    continue;
  }
  const problems = [];
  if (r.missing.length) {
    problems.push(`${r.missing.length} power(s) match the fast-form shape and ship no form`);
  }
  if (r.loadErrors.length) problems.push(`${r.loadErrors.length} load error(s)`);
  if (r.matched < floor) {
    problems.push(`only ${r.matched} fast-form powers found, below the measured ${floor}`);
  }
  if (r.unjoined.length > MAX_UNJOINED) {
    problems.push(
      `${r.unjoined.length}/${r.swept} generated powers did not resolve to an export, so they ` +
        `were not checked at all: ${r.unjoined.slice(0, 5).join(', ')}` +
        (r.unjoined.length > 5 ? ', …' : ''),
    );
  }

  if (!GATE) {
    console.log(
      `\n${r.dataset}: ${r.swept} generated powers, ${r.unjoined.length} unjoined, ` +
        `${r.matched} fast-form shaped (${r.meterFamily} meter-branched, excluded)`,
    );
    for (const u of r.unjoined) console.log(`  UNJOINED  ${u}`);
    for (const m of r.missing) {
      console.log(`  MISSING quickSnipe  ${m.power}  [${m.source}]  ← ${m.module}`);
    }
    for (const e of r.loadErrors) console.log(`  LOAD ERROR  ${e}`);
  }
  if (problems.length) {
    failed = true;
    console.log(`FAIL ${r.dataset}: ${problems.join('; ')}`);
    for (const m of r.missing) console.log(`     ${m.power} [${m.source}]`);
  } else {
    console.log(`PASS ${r.dataset}: ${r.matched} fast-form powers, all served`);
  }
}
process.exit(failed ? 1 : 0);
