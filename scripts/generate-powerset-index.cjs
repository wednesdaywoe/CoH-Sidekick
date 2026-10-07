/**
 * Generate Powerset Index Script
 *
 * Aggregates every converted powerset shard into the JSON registry the contract is
 * emitted from.
 *
 * Usage: node scripts/generate-powerset-index.cjs [--dataset <id>]
 *
 * Writes `pipeline/<id>/powersets.json`.
 *
 * THE JSON IS THE POINT. `emit-pipeline-json.cjs` used to produce `powersets.json` by
 * `require()`ing a TypeScript index through `tsx`, which pulled ~2,900 TypeScript
 * modules into the path the shipped data travels — the last thing doing so, and the
 * reason that script and `_oracle-modules.cjs` could not be deleted. The shards it
 * aggregates here are written by `convert-powerset.cjs` from the values it already
 * holds; see the transcription note there.
 *
 * ONE WALK, TWO WRITES until 2026-09-25: this file also wrote
 * `src/data/datasets/<id>/powersets/index.ts`, the registry the TS app imported, and
 * the argument for writing both here was that neither copy should have its own idea of
 * which sets exist or in what order. The TypeScript went with the app, so there is one
 * write left and the argument is discharged rather than dropped.
 *
 * DISCOVERY MOVED WITH IT, and that is the substantive change. The walk enumerated the
 * COMPOSED TYPESCRIPT tree — any directory under `src/data/datasets/<id>/powersets/`
 * holding an `index.ts` — and then read the matching `pipeline/` shard. With the
 * TypeScript gone it walks the shard tree itself. Measured at the switch: the two
 * trees named the same 364 / 305 / 305 / 373 powersets, zero difference either
 * direction on all four forks.
 *
 * WHAT DISCOVERY MAY NOT BE. The export is the obvious authority and is the wrong one:
 * `convert-powerset.cjs` renames a powerset to its DISPLAY slug, so the export's
 * `gadgets`, `bio_organic_armor` and `martial_manipulation` are `devices`, `bio-armor`
 * and `martial-combat` downstream — 26 of Homecoming's 364, and 37 of Thunderspy's.
 * Enumerating from the export produces the right COUNT with 26 wrong names, which is
 * how that mistake would have shipped.
 *
 * WHAT REPLACES THE MISSING-SHARD ERROR. Reading the shards while a second tree said
 * which ones to expect meant an absent shard could be caught by name: a set in the
 * composed tree and not in `pipeline/` meant its converter had not run. Walking one
 * tree cannot catch that — an unconverted set is simply not there, and a contract
 * quietly missing a powerset is not a smaller contract. So the count is cross-checked
 * against the export per archetype/type bucket, which re-derives the expectation from
 * the source of truth on every run rather than freezing a number here, and is immune
 * to the renaming above because a rename does not change how many sets a bucket holds.
 * Measured at the switch: 26 buckets per fork, 0 mismatched on all four.
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath } = require('./_dataset-paths.cjs');
// CATEGORY_MAP is the single raw-category → archetype/type routing table; the
// per-powerset converter owns it and this file only iterates it, same as
// `convert-all-powersets.cjs` does.
const { RAW_DATA_PATH, CATEGORY_MAP } = require('./convert-powerset.cjs');

const datasetId = parseDatasetArg();
const JSON_OUTPUT_FILE = pipelinePath(datasetId, 'powersets.json');
const SHARD_ROOT = pipelinePath(datasetId, 'powersets');

// The three buckets a powerset can land in, in the order the registry visits them.
// Every value in CATEGORY_MAP is one of these.
const TYPES = ['primary', 'secondary', 'epic'];

/**
 * Every converted powerset, as `{ archetype, type, set }`.
 *
 * One shard per powerset at `<archetype>/<type>/<set>.json`, and the basename IS the
 * slug the composed TypeScript directory used to carry, which is what made this a
 * like-for-like swap.
 */
function getPowersets() {
  const powersets = [];
  if (!fs.existsSync(SHARD_ROOT)) {
    throw new Error(
      `no converted powersets at ${path.relative(process.cwd(), SHARD_ROOT)}\n`
      + `  Rebuild them with:\n`
      + `    node scripts/convert-all-powersets.cjs --dataset ${datasetId} --force`,
    );
  }
  for (const archetype of fs.readdirSync(SHARD_ROOT)) {
    if (!fs.statSync(path.join(SHARD_ROOT, archetype)).isDirectory()) continue;
    for (const type of TYPES) {
      const typePath = path.join(SHARD_ROOT, archetype, type);
      if (!fs.existsSync(typePath)) continue;
      for (const entry of fs.readdirSync(typePath)) {
        if (!entry.endsWith('.json')) continue;
        powersets.push({ archetype, type, set: entry.slice(0, -'.json'.length) });
      }
    }
  }
  return powersets;
}

/**
 * Group by archetype — and, because the registry is built by iterating this grouping,
 * THE ORDER THE REGISTRY'S KEYS COME OUT IN. Archetypes ascending, sets ascending
 * within each, so the walk order above cannot reach the output.
 */
function groupByArchetype(powersets) {
  const byArchetype = {};
  for (const ps of powersets) {
    if (!byArchetype[ps.archetype]) {
      byArchetype[ps.archetype] = [];
    }
    byArchetype[ps.archetype].push(ps);
  }
  for (const sets of Object.values(byArchetype)) {
    sets.sort((a, b) => a.set.localeCompare(b.set));
  }
  return Object.entries(byArchetype).sort();
}

/**
 * How many powersets the EXPORT puts in each archetype/type bucket.
 *
 * The category directories the bin export writes; the old CoD2 layout had an extra
 * `powers/` segment, so probe both exactly as `convert-all-powersets.cjs` does.
 */
function exportBucketCounts() {
  const powersPath = (() => {
    const oldLayout = path.join(RAW_DATA_PATH, 'powers');
    for (const cat of Object.keys(CATEGORY_MAP)) {
      if (fs.existsSync(path.join(RAW_DATA_PATH, cat))) return RAW_DATA_PATH;
      if (fs.existsSync(path.join(oldLayout, cat))) return oldLayout;
    }
    return RAW_DATA_PATH;
  })();
  const counts = {};
  for (const [category, info] of Object.entries(CATEGORY_MAP)) {
    const categoryPath = path.join(powersPath, category);
    if (!fs.existsSync(categoryPath)) continue;
    const n = fs.readdirSync(categoryPath)
      .filter((item) => fs.statSync(path.join(categoryPath, item)).isDirectory())
      .length;
    const key = `${info.archetype}/${info.type}`;
    counts[key] = (counts[key] || 0) + n;
  }
  return counts;
}

/**
 * Stop unless every bucket holds as many shards as the export holds powersets.
 *
 * This is what the per-name missing-shard error became; see the header. It names the
 * bucket and both numbers, because "one short" and "eleven short" are different
 * accidents and the count is the only thing this check knows.
 */
function assertNoBucketShortfall(powersets) {
  const want = exportBucketCounts();
  const got = {};
  for (const ps of powersets) {
    const key = `${ps.archetype}/${ps.type}`;
    got[key] = (got[key] || 0) + 1;
  }
  const bad = [...new Set([...Object.keys(want), ...Object.keys(got)])]
    .sort()
    .filter((key) => (want[key] || 0) !== (got[key] || 0))
    .map((key) => `    ${key}: export has ${want[key] || 0}, pipeline/ has ${got[key] || 0}`);
  if (bad.length) {
    throw new Error(
      `converted powerset count disagrees with the export in ${bad.length} bucket(s):\n`
      + `${bad.join('\n')}\n`
      + `  A bucket short means a powerset's converter did not run, and a contract\n`
      + `  quietly missing a powerset is not a smaller contract. Rebuild with:\n`
      + `    node scripts/convert-all-powersets.cjs --dataset ${datasetId} --force`,
    );
  }
}

/**
 * The registry, built over the shards — same array, same order, same `if (ps.id)`
 * skip the TypeScript index built at import time before it was deleted.
 */
function buildJsonRegistry(grouped) {
  const registry = {};
  for (const [, sets] of grouped) {
    for (const ps of sets) {
      const shard = path.join(SHARD_ROOT, ps.archetype, ps.type, `${ps.set}.json`);
      const powerset = JSON.parse(fs.readFileSync(shard, 'utf-8'));
      if (powerset.id) registry[powerset.id] = powerset;
    }
  }
  return registry;
}

// Main
const powersets = getPowersets();
console.log(`Found ${powersets.length} powersets`);

assertNoBucketShortfall(powersets);

const grouped = groupByArchetype(powersets);

// No pretty-printing: `emit-pipeline-json.cjs` wrote this file with a bare
// `JSON.stringify`, and byte-identity across the move is the guard.
fs.mkdirSync(path.dirname(JSON_OUTPUT_FILE), { recursive: true });
fs.writeFileSync(
  JSON_OUTPUT_FILE,
  JSON.stringify({ MODULAR_POWERSETS: buildJsonRegistry(grouped) }),
);
console.log(`Generated ${JSON_OUTPUT_FILE}`);
