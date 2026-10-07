/**
 * The single shared walker over a dataset's COMPOSED surfaces — the post-override
 * Power objects the planner actually consumes. It was written so the contract emitter and
 * the oracle-fixture emitter could never disagree about which powers exist (the
 * planb-shadow-sweep lesson: two walkers = one silent coverage hole). Only the contract
 * emitter is left here -- `emit-contract.cjs` is the sole caller of `loadComposed` and
 * `forEachComposedPower` in this repository, nothing writes `fixtures/oracle/`, and those
 * records are frozen at what the last TS-era run answered. The one-walker rule still holds;
 * it just has one walker to hold over.
 *
 * Reads `pipeline/<id>/*.json`, which the converters write themselves. It used to
 * `require()` TypeScript out of `src/data` through `tsx`, which made the oracle a stage
 * in the path the shipped data travels rather than a reference held
 * beside it. Same values either way — the guard on that change was `contract/` coming out
 * byte-identical across all 1,450 files.
 *
 * Composed surfaces (NOT the raw generated/ tree, and NEVER src/data/index.ts —
 * the app barrel imports React components):
 *   - pipeline/<id>/powersets.json        MODULAR_POWERSETS (per-power withOverrides)
 *   - pipeline/<id>/power-pools-raw.json  POWER_POOLS_RAW  (applyAggregateOverrides)
 *   - pipeline/<id>/epic-pools-raw.json   EPIC_POOLS_RAW   (applyAggregateOverrides)
 *
 * `pipeline/` is gitignored and rebuilt rather than committed: `exported_powers/` IS
 * committed, so a clone with no game install regenerates all of it. A reader that reports
 * a file missing names the command that rebuilds that one.
 *
 * Two roots sit BESIDE it and are never copied into it, because nothing derives them and a
 * clean build must not be able to delete them: `hand-data/` (authored, via `handJson`) and
 * `mids-tables/` (vendored out of an installed Mids Reborn, via `midsJson`).
 */

const fs = require('fs');
const path = require('path');

const REPO = path.resolve(__dirname, '..');
const PIPELINE = path.join(REPO, 'pipeline');
const { ALL_DATASETS: DATASETS } = require('./_dataset-paths.cjs');

/**
 * Pipeline files whose producer is not a converter that `npm run regen` runs inside its
 * ordinary per-dataset loop, so the rebuild hint below would otherwise name the wrong command.
 *
 * The two Mids tables are deliberately NOT here, because they are not pipeline files at all
 * any more — `midsJson` reads them from the committed `mids-tables/` and carries its own
 * message about the Mids install.
 */
const REBUILT_BY = {
  'io-sets-raw': (ds) => `python3 scripts/extract-rebirth-io-sets-v2.py --dataset ${ds}`,
  // Two producers, in this order: the per-set converter writes the shards under
  // `pipeline/<id>/powersets/`, the index generator aggregates them into the registry.
  powersets: (ds) => `node scripts/convert-all-powersets.cjs --dataset ${ds} --force, `
    + `then node scripts/generate-powerset-index.cjs --dataset ${ds}`,
};

/**
 * One pipeline file, by the name the emitter knows it as (no extension).
 *
 * Stops rather than returning empty when the file is absent. An emitter that quietly
 * skipped a missing section would write a contract short of a section and say nothing,
 * and a bundle missing its `io-sets` is not a smaller bundle, it is a broken app.
 */
function datasetJson(dataset, name) {
  const full = path.join(PIPELINE, dataset, `${name}.json`);
  if (!fs.existsSync(full)) {
    const rebuild = REBUILT_BY[name]
      ? REBUILT_BY[name](dataset)
      : `npm run regen -- --dataset ${dataset}`;
    throw new Error(
      `pipeline input missing: ${path.relative(REPO, full)}\n`
      + `  Rebuild it with: ${rebuild}`,
    );
  }
  return JSON.parse(fs.readFileSync(full, 'utf8'));
}

/**
 * A file under `hand-data/` — COMMITTED authored input, not pipeline output.
 *
 * Read from its own root rather than copied into `pipeline/` first. `pipeline/` is
 * gitignored output a clean build deletes, and authored data with no upstream does not
 * belong in a directory named after output — the same argument that moved
 * `effect-registry.json` out of `contract/`. See `hand-data/README.md`.
 *
 * So absent here cannot mean "not built yet". It means the committed file is gone, and
 * only git has it back.
 */
function handJson(name) {
  const full = path.join(REPO, 'hand-data', `${name}.json`);
  if (!fs.existsSync(full)) {
    throw new Error(
      `authored input missing: ${path.relative(REPO, full)}\n`
      + '  Nothing derives this file. Restore it from git.',
    );
  }
  return JSON.parse(fs.readFileSync(full, 'utf8'));
}

/** A file under `pipeline/_shared/`, not tied to one dataset. */
function sharedJson(name) {
  const full = path.join(PIPELINE, '_shared', `${name}.json`);
  if (!fs.existsSync(full)) {
    throw new Error(
      `pipeline input missing: ${path.relative(REPO, full)}\n`
      + '  Rebuild it with: npm run regen',
    );
  }
  return JSON.parse(fs.readFileSync(full, 'utf8'));
}

/**
 * One of the two vendored Mids name tables under `mids-tables/<id>/` — COMMITTED input,
 * read from its own root the way `handJson` reads authored data.
 *
 * THESE TWO ARE NOT LIKE ANY OTHER CONTRACT INPUT. Every converter re-derives its JSON from the
 * committed `exported_powers/`, so a clone with no game install rebuilds it. These are read out
 * of an INSTALLED Mids Reborn, which CI does not have and a clone will not have, and
 * `exported_powers/` cannot stand in: Mids' enhancement UIDs and short codes are facts about
 * MIDS, not about the game, and cannot be derived from a display name at all
 * (`tools/mids-oracle/emit_mids_uids.py` argues that at length). So the dependency is real and
 * permanent, and the answer is to vendor the tables and name the dependency —
 * `mids-tables/README.md` — rather than let them look like every other derived file.
 *
 * `emit-pipeline-json.cjs` copied them into `pipeline/` until 2026-09-25, for no reason but
 * giving the emitter one input root. That is the mistake `handJson` already refused: `pipeline/`
 * is gitignored output a clean build deletes, and a file no build step can produce does not
 * belong in it. So absent here cannot mean "not built yet" either. It means the committed file
 * is gone.
 */
function midsJson(dataset, name) {
  const full = path.join(REPO, 'mids-tables', dataset, `${name}.json`);
  if (!fs.existsSync(full)) {
    const script = name === 'mids-uids' ? 'emit_mids_uids.py' : 'emit_mids_enh_names.py';
    throw new Error(
      `missing vendored Mids table: ${path.relative(REPO, full)}\n`
      + '  This is committed input — restore it from git. Regenerating it needs an\n'
      + `  installed Mids Reborn: python3 tools/mids-oracle/${script} --dataset ${dataset}`,
    );
  }
  return JSON.parse(fs.readFileSync(full, 'utf8'));
}

/**
 * A Power-shaped node: has a name and an atoms array.
 *
 * The bag was the other arm until 2026-09-03. It came off in step with `emit-contract.cjs`,
 * `planb-shadow-sweep.cjs` and `coh_data`'s `collect_into`, because a walker that disagrees
 * with the emitter about which powers exist is the planb-shadow-sweep lesson itself. The
 * loud half lives in `collect_into`: a node with a bag and no atoms is an error there, not a
 * skip, so a power this predicate newly drops reds the load instead of vanishing.
 */
function isPower(node) {
  if (!node || typeof node !== 'object' || typeof node.name !== 'string') return false;
  return Array.isArray(node.atoms);
}

/** Recursively collect Power objects from an aggregate (pool/epic trees nest). */
function collectPowers(node, out = [], seen = new Set()) {
  if (!node || typeof node !== 'object' || seen.has(node)) return out;
  seen.add(node);
  if (Array.isArray(node)) {
    for (const v of node) collectPowers(v, out, seen);
    return out;
  }
  if (isPower(node)) out.push(node);
  for (const v of Object.values(node)) collectPowers(v, out, seen);
  return out;
}

/**
 * Load one dataset's composed surfaces.
 * Returns { powersets, pools, epics } where powersets is the MODULAR_POWERSETS
 * registry (Record<string, Powerset>) and pools/epics are the raw aggregates.
 */
function loadComposed(dataset) {
  const { MODULAR_POWERSETS } = datasetJson(dataset, 'powersets');
  const { POWER_POOLS_RAW } = datasetJson(dataset, 'power-pools-raw');
  const { EPIC_POOLS_RAW } = datasetJson(dataset, 'epic-pools-raw');
  return { powersets: MODULAR_POWERSETS, pools: POWER_POOLS_RAW, epics: EPIC_POOLS_RAW };
}

/**
 * Iterate every composed power in a dataset.
 * cb(power, meta) with meta = { source: 'powerset'|'pool'|'epic', container, key }
 *   - container: powerset id (e.g. "blaster/fire-blast") or pool/epic aggregate path
 *   - key: stable fixture key `<source>/<container>/<internalName-or-name>`
 */
function forEachComposedPower(dataset, cb) {
  const { powersets, pools, epics } = loadComposed(dataset);
  const counts = { powerset: 0, pool: 0, epic: 0 };
  const occurrences = new Map();
  const emit = (power, source, container) => {
    counts[source] += 1;
    const base = keyFor(source, container, power);
    const occ = occurrences.get(base) ?? 0;
    occurrences.set(base, occ + 1);
    cb(power, { source, container, key: `${base}@${occ}` });
  };

  for (const [psId, ps] of Object.entries(powersets)) {
    for (const power of ps.powers || []) emit(power, 'powerset', ps.id || psId);
  }
  // JSON-clone the aggregates: the serialized contract materializes any shared
  // object references as duplicate copies, and the Rust side walks that serialized
  // form — walking the live modules here (where a shared ref is one identity)
  // could disagree with it. Same data, same multiplicity, by construction.
  for (const [source, aggregate] of [['pool', pools], ['epic', epics]]) {
    for (const power of collectPowers(JSON.parse(JSON.stringify(aggregate)))) {
      emit(power, source, source);
    }
  }
  return counts;
}

/** Stable fixture key; `@<occurrence>` disambiguates same-name twins (epic copies
 *  of one power across archetypes). The Rust gate derives the identical key while
 *  iterating the bundle in the same partition order.
 *
 *  Pool and epic powers reach here in the converter's legacy shape, carrying their
 *  identity only in `fullName` — the same derivation `transformPoolPower` does for the
 *  runtime, and that `coh_data` now does at bundle load. Keying on the display `name`
 *  instead would address 57 powers per fork by the wrong identity, since the game renamed
 *  them and kept the original internal name (Swift/Quick, Hover/Combat_Flight). */
function keyFor(source, container, power) {
  const identity = power.internalName || power.fullName?.split('.').pop() || power.name;
  return `${source}/${container}/${identity.replace(/\s+/g, '_')}`;
}

module.exports = {
  REPO, DATASETS, loadComposed, forEachComposedPower, collectPowers, datasetJson, sharedJson,
  handJson, midsJson,
};
