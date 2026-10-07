/**
 * Grant-edge coverage — does every served power whose export carries a Self-targeted
 * `Grant_Power`/`Revoke_Power` edge actually ship `grantEdges`, whichever converter
 * produced it?
 *
 * Same defect class as `audit-form-coverage.cjs`, same cure: the check runs OUTSIDE all
 * three converters, joining the committed export (their shared input) to the committed
 * generated tree (their combined output). The predicate is the converter's own
 * `extractGrantEdges`, called rather than transcribed, and the comparison is
 * DEEP-EQUALITY of the whole stamp — a partition whose converter never calls the
 * extractor emits nothing and fails here, and a stamp that drifts from the extractor's
 * current output fails the same way.
 *
 * Two silences this gate turns into numbers:
 *  - unresolved grant targets: the extractor warns-and-skips a grant whose target record
 *    is missing from the export (an edge without its expiry would read "no limit").
 *    Pinned to ZERO — the exporter ships every referenced grant target, so a skip means
 *    the reference collector or the path resolution regressed.
 *  - the carrier population per fork, floored so a join or extractor collapse cannot
 *    read as a clean sweep.
 *
 * Usage:
 *   node scripts/audit-grant-edges.cjs [--dataset <id>]... [--gate]
 */

require('tsx/cjs');
const fs = require('fs');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { joinerFor } = require('./_export-join.cjs');

const argv = process.argv.slice(2);
const GATE = argv.includes('--gate');
const DATASETS = (() => {
  const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
  return picked.length ? picked : require('./_dataset-paths.cjs').ALL_DATASETS;
})();

/** Grant/revoke templates the extractor reads, counted straight off the export so an
 *  unresolved-target skip is visible as extracted-vs-eligible disagreement. */
function eligibleEdgeCount(cv, json) {
  let eligible = 0;
  const visit = (group) => {
    if (!group || typeof group !== 'object') return;
    if (group.is_pvp === 'PVP_ONLY') return;
    if ((group.chance ?? 1.0) === 0) return;
    for (const t of group.templates ?? []) {
      const attrib = (t.attribs && t.attribs[0]) || null;
      if (attrib !== 'Grant_Power' && attrib !== 'Revoke_Power') continue;
      if (t.target !== 'Self') continue;
      eligible += (t.params?.power_names ?? []).length;
    }
    for (const child of group.child_effects ?? []) visit(child);
  };
  for (const collection of [json.effects, json.activation_effects]) {
    for (const group of collection ?? []) visit(group);
  }
  return eligible;
}

function auditDataset(dataset) {
  const { cv, join } = joinerFor(dataset);
  const mismatched = [];
  const unresolved = [];
  const unjoined = [];
  const loadErrors = [];
  let swept = 0;
  let carriers = 0;
  let edges = 0;

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
      const expected = cv.extractGrantEdges(json);
      const shipped = power.grantEdges ?? null;
      if (JSON.stringify(expected) !== JSON.stringify(shipped)) {
        mismatched.push({
          power: power.internalName || power.name,
          module: relPath,
          expected: expected?.length ?? 0,
          shipped: shipped?.length ?? 0,
        });
      }
      if (expected) {
        carriers += 1;
        edges += expected.length;
        const eligible = eligibleEdgeCount(cv, json);
        if (eligible !== expected.length) {
          unresolved.push(
            `${power.internalName || power.name}: ${eligible - expected.length} edge(s) skipped `
            + '(grant target missing from the export)',
          );
        }
      }
    },
    { onLoadError: (rel, err) => loadErrors.push(`${rel}: ${err.message}`) },
  );

  return { dataset, swept, carriers, edges, mismatched, unresolved, unjoined, loadErrors };
}

/**
 * Non-vacuity floors, measured at first ship (2026-08-14): served-corpus grantEdges
 * carriers per fork (765 / 530 / 685 edges). `>=` so a game patch adding a mechanic is not
 * a failure; a collapse — a partition going silent, the extractor dropping a collection —
 * is. The first run of this gate caught exactly that: the inherents and accolades
 * partitions shipped nothing until their converters were given the call.
 */
const FLOORS = {
  homecoming: 405,
  rebirth: 281,
  thunderspy: 359,
  // Measured 2026-08-22: 406 carriers / 767 edges. One carrier above Homecoming, which is
  // the content this fork ships ahead of live rather than a join difference.
  brainstorm: 406,
};
const MAX_UNJOINED = 0;

let failed = false;
for (const dataset of DATASETS) {
  const r = auditDataset(dataset);
  // No `?? 1` default — see audit-form-coverage: a floor of one is not a floor.
  const floor = FLOORS[r.dataset];
  if (floor === undefined) {
    console.log(`FAIL ${r.dataset}: no measured floor — add one to FLOORS before sweeping it`);
    failed = true;
    continue;
  }
  const problems = [];
  if (r.mismatched.length) {
    problems.push(`${r.mismatched.length} power(s) whose shipped grantEdges differ from the extractor`);
  }
  if (r.unresolved.length) problems.push(`${r.unresolved.length} unresolved grant target(s)`);
  if (r.loadErrors.length) problems.push(`${r.loadErrors.length} load error(s)`);
  if (r.carriers < floor) {
    problems.push(`only ${r.carriers} grantEdges carriers found, below the measured ${floor}`);
  }
  if (r.unjoined.length > MAX_UNJOINED) {
    problems.push(`${r.unjoined.length} generated power(s) resolve to no export`);
  }

  if (problems.length) {
    failed = true;
    console.log(`FAIL ${r.dataset}: ${problems.join('; ')}`);
    for (const m of r.mismatched.slice(0, 10)) {
      console.log(`  drift: ${m.power} [${m.module}] expected ${m.expected} edge(s), shipped ${m.shipped}`);
    }
    for (const u of r.unresolved.slice(0, 10)) console.log(`  ${u}`);
    for (const u of r.unjoined.slice(0, 10)) console.log(`  unjoined: ${u}`);
    for (const e of r.loadErrors.slice(0, 10)) console.log(`  load: ${e}`);
  } else {
    console.log(
      `PASS ${r.dataset}: ${r.carriers} carriers / ${r.edges} edges over ${r.swept} powers swept`,
    );
  }
}

if (!GATE) {
  console.log('\n(run with --gate for CI semantics; this run always exits 0 without it)');
}
process.exit(GATE && failed ? 1 : 0);
