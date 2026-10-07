/**
 * Suppress/cancel event-tail coverage — does every served Meta atom carry the event
 * tails its export template states, and does every eligible template's tail ship?
 *
 * The fields under audit are `suppressEvents`/`suppressSeconds`/`suppressAlways` and
 * `cancelEvents` (RB5-d): `suppressible` folds the suppress tail into one verdict,
 * and the per-cast walk needs the clock itself, so the tails now ship whole on Meta
 * atoms. Same defect class as `audit-grant-edges.cjs`, same cure: the check runs
 * OUTSIDE all converters, joining the committed export to the committed generated
 * tree. The expectation is re-derived from the RAW export json here, not through
 * `ingestTemplate`, so a stamp defect in the shared ingest can't grade itself.
 *
 * Three checks per dataset:
 *  - soundness: every shipped atom carrying a tail matches some raw Meta template's
 *    tail, deep-equal on (metaAttrib, events, seconds, always, cancels). Matched
 *    against the dataset-wide raw tuple set because a collector may pull a child
 *    power's templates onto its shell.
 *  - completeness: every served power whose own export file states an eligible Meta
 *    tail ships at least one atom with that exact tuple. A partition whose atoms
 *    lost the stamp goes silent here.
 *  - anchors + floors: the two carriers the per-cast walk reads are pinned by value
 *    (a Hide meter suppressing Attacked/Damaged at 8s, a Placate meter cancelling
 *    on Attacked/Damaged/MissionObjectClick), and the meter-carrier population is
 *    floored per fork so a scope collapse can't read as a clean sweep.
 *
 * Usage:
 *   node scripts/audit-meter-events.cjs [--dataset <id>]... [--gate]
 */

require('tsx/cjs');
const fs = require('fs');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { joinerFor, converterFor, indexBySegment } = require('./_export-join.cjs');
const { decodeAtoms, bridgeAttrib } = require('./_atomic-effect.ts');

const argv = process.argv.slice(2);
const GATE = argv.includes('--gate');
const DATASETS = (() => {
  const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
  return picked.length ? picked : require('./_dataset-paths.cjs').ALL_DATASETS;
})();

/** One comparable tail statement. Joined for ASKING (set membership), never re-split. */
function tupleOf(metaLower, suppressEvents, seconds, always, cancelEvents) {
  return [
    metaLower,
    (suppressEvents ?? []).join(','),
    seconds ?? '',
    always ?? '',
    (cancelEvents ?? []).join(','),
  ].join('|');
}

/** The tail statements of one raw template's Meta attribs, or nothing. */
function templateTuples(t) {
  const out = [];
  if (!t || !Array.isArray(t.attribs)) return out;
  const se = Array.isArray(t.suppress_events) ? t.suppress_events : [];
  const ce = Array.isArray(t.cancel_events) ? t.cancel_events : [];
  if (!se.length && !ce.length) return out;
  for (const attrib of t.attribs) {
    if (typeof attrib !== 'string') continue;
    if (bridgeAttrib(attrib, t.aspect, t.table).effectType !== 'Meta') continue;
    const seconds = se.length ? se[0].duration : undefined;
    const always = se.length ? se[0].always !== 0 : undefined;
    out.push(tupleOf(attrib.toLowerCase(), se.map((e) => e.event), seconds, always, ce));
  }
  return out;
}

/**
 * Every Meta-attrib event-tail statement in one raw export json, re-derived here from
 * the raw fields (the independence this audit exists for).
 *
 * Two walks, because the two checks own different scopes. The OWN walk covers the
 * power's own effect collections (`effects`, `activation_effects`, nested
 * `child_effects`) — what the power itself states, so what completeness may demand it
 * ship. The DEEP walk recurses everything else too (a `redirect` block embeds the
 * child power's whole json), feeding only the dataset-wide soundness set: a collector
 * may legitimately pull a child's templates onto the shell, but the shell owes the
 * child's tails nothing.
 */
function rawTuples(json) {
  const own = [];
  const visitGroup = (group) => {
    if (!group || typeof group !== 'object') return;
    for (const t of group.templates ?? []) own.push(...templateTuples(t));
    for (const child of group.child_effects ?? []) visitGroup(child);
  };
  for (const collection of [json.effects, json.activation_effects]) {
    for (const group of collection ?? []) visitGroup(group);
  }

  const deep = [];
  const visit = (node) => {
    if (Array.isArray(node)) {
      for (const v of node) visit(v);
      return;
    }
    if (!node || typeof node !== 'object') return;
    deep.push(...templateTuples(node));
    for (const v of Object.values(node)) {
      if (v && typeof v === 'object') visit(v);
    }
  };
  visit(json);
  return { own, deep };
}

/** The tail statements a served power ships, decoded off its wire atoms. */
function shippedTuples(power) {
  const out = [];
  for (const a of decodeAtoms(power.atoms)) {
    if (!a.suppressEvents && !a.cancelEvents) continue;
    out.push({
      atom: a,
      tuple: tupleOf(
        a.metaAttrib,
        a.suppressEvents,
        a.suppressSeconds,
        a.suppressAlways,
        a.cancelEvents
      ),
    });
  }
  return out;
}

/**
 * Every tail statement in the dataset's WHOLE export tree (fork-scoped), for the
 * soundness set. Wider than the served-power join on purpose: a shell's collector may
 * pull templates from a child file no served power resolves to (the Teleport family's
 * `designer_status` tails live in such children), and those shipped tails are sound.
 */
function datasetRawSet(dataset) {
  const cv = converterFor(dataset);
  const set = new Set();
  for (const paths of indexBySegment(cv.RAW_DATA_PATH).values()) {
    for (const p of paths) {
      let json;
      try {
        json = JSON.parse(fs.readFileSync(p, 'utf-8'));
      } catch {
        continue; // non-power json (indexes, boostsets); power files parse or fail the join below
      }
      for (const t of rawTuples(json).deep) set.add(t);
    }
  }
  return set;
}

function auditDataset(dataset) {
  const { join } = joinerFor(dataset);
  const unsound = [];
  const incomplete = [];
  const unjoined = [];
  const loadErrors = [];
  let swept = 0;
  let carriers = 0;
  let meterCarriers = 0;
  let hideAnchor = false;
  let placateAnchor = false;

  // Pass 1 — collect, per power: raw tuples from its own export file, shipped tuples.
  const powers = [];
  const datasetRaw = datasetRawSet(dataset);
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
      const { own } = rawTuples(json);
      powers.push({ power, relPath, raw: own, shipped: shippedTuples(power) });
    },
    { onLoadError: (rel, err) => loadErrors.push(`${rel}: ${err.message}`) }
  );

  // Pass 2 — grade. Soundness is dataset-wide (collectors cross files); completeness
  // is per power against its own file's top-level statements.
  for (const { power, relPath, raw, shipped } of powers) {
    const name = power.internalName || power.name;
    if (shipped.length) carriers += 1;
    if (shipped.some(({ atom }) => atom.metaAttrib === 'meter')) meterCarriers += 1;
    for (const { atom, tuple } of shipped) {
      if (!datasetRaw.has(tuple)) {
        unsound.push(`${name} [${relPath}]: shipped tail ${tuple} matches no raw template`);
      }
      if (
        atom.metaAttrib === 'meter' &&
        atom.suppressSeconds === 8 &&
        (atom.suppressEvents ?? []).includes('Attacked') &&
        (atom.suppressEvents ?? []).includes('Damaged')
      ) {
        hideAnchor = true;
      }
      if (
        atom.metaAttrib === 'meter' &&
        !atom.suppressEvents &&
        ['Attacked', 'Damaged', 'MissionObjectClick'].every((e) =>
          (atom.cancelEvents ?? []).includes(e)
        )
      ) {
        placateAnchor = true;
      }
    }
    const shippedSet = new Set(shipped.map((s) => s.tuple));
    for (const t of raw) {
      if (!shippedSet.has(t)) {
        incomplete.push(`${name} [${relPath}]: raw tail ${t} ships on no atom`);
      }
    }
  }

  return {
    dataset, swept, carriers, meterCarriers, hideAnchor, placateAnchor,
    unsound, incomplete, unjoined, loadErrors,
  };
}

/**
 * Non-vacuity floors, measured at first ship (2026-08-15): served-corpus Meta atoms
 * carrying an event tail, and the meter-attrib subset the per-cast walk reads. `>=` so
 * a patch adding a mechanic isn't a failure; a scope collapse is.
 */
const FLOORS = {
  homecoming: { carriers: 59, meters: 39 },
  rebirth: { carriers: 38, meters: 31 },
  thunderspy: { carriers: 18, meters: 12 },
  // Measured 2026-08-22 on this fork's own corpus: five tail carriers and one meter carrier
  // above Homecoming.
  brainstorm: { carriers: 64, meters: 40 },
};

let failed = false;
for (const dataset of DATASETS) {
  const r = auditDataset(dataset);
  // No `{ carriers: 1, meters: 1 }` default — see audit-form-coverage: that is not a floor.
  const floor = FLOORS[r.dataset];
  if (floor === undefined) {
    console.log(`FAIL ${r.dataset}: no measured floor — add one to FLOORS before sweeping it`);
    failed = true;
    continue;
  }
  const problems = [];
  if (r.unsound.length) problems.push(`${r.unsound.length} shipped tail(s) matching no raw template`);
  if (r.incomplete.length) problems.push(`${r.incomplete.length} raw tail(s) shipping on no atom`);
  if (r.loadErrors.length) problems.push(`${r.loadErrors.length} load error(s)`);
  if (r.unjoined.length) problems.push(`${r.unjoined.length} generated power(s) resolve to no export`);
  if (r.carriers < floor.carriers) {
    problems.push(`only ${r.carriers} tail carriers, below the measured ${floor.carriers}`);
  }
  if (r.meterCarriers < floor.meters) {
    problems.push(`only ${r.meterCarriers} meter carriers, below the measured ${floor.meters}`);
  }
  if (!r.hideAnchor) problems.push('no meter atom suppresses Attacked+Damaged at 8s (the Hide anchor)');
  if (!r.placateAnchor) {
    problems.push('no meter atom cancels on Attacked/Damaged/MissionObjectClick (the Placate anchor)');
  }

  if (problems.length) {
    failed = true;
    console.log(`FAIL ${r.dataset}: ${problems.join('; ')}`);
    for (const u of r.unsound.slice(0, 10)) console.log(`  unsound: ${u}`);
    for (const u of r.incomplete.slice(0, 10)) console.log(`  incomplete: ${u}`);
    for (const u of r.unjoined.slice(0, 10)) console.log(`  unjoined: ${u}`);
    for (const e of r.loadErrors.slice(0, 10)) console.log(`  load: ${e}`);
  } else {
    console.log(
      `PASS ${r.dataset}: ${r.carriers} tail carriers (${r.meterCarriers} meter) over ${r.swept} powers swept`
    );
  }
}

if (!GATE) {
  console.log('\n(run with --gate for CI semantics; this run always exits 0 without it)');
}
process.exit(GATE && failed ? 1 : 0);
