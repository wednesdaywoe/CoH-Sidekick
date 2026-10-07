/**
 * Shared CLI/path helper for the conversion scripts in this folder.
 *
 * Usage:
 *
 *     const { parseDatasetArg, pipelinePath } = require('./_dataset-paths.cjs');
 *     const datasetId = parseDatasetArg();          // "homecoming" by default
 *     const atTablesOut = pipelinePath(datasetId, 'at-tables.json');
 *
 * The `--dataset <id>` flag is recognized in `process.argv`. If absent,
 * defaults to `'homecoming'` so existing muscle memory keeps working.
 *
 * `pipelinePath` is the only output address. This file carried two others until
 * 2026-09-25 — `datasetPath` for `src/data/datasets/<id>/` and `dataPath` for
 * `src/data/` — because each script wrote generated TypeScript there and files
 * migrated into the per-dataset folder one at a time. Both are gone with `src/`, and
 * the migration they were tracking is what finished.
 */

const path = require('path');

const REPO_ROOT = path.resolve(__dirname, '..');

/**
 * Every dataset the pipeline builds, in regen order. The single source — the roster
 * was copy-pasted across ~25 scripts, and a paste is how a new dataset gets converted
 * by some of them and silently skipped by the rest (the DSH8 Clarion-leak shape).
 *
 * `brainstorm` is Homecoming's open beta (the Brainstorm server), a first-class shipping
 * dataset rather than a ring Homecoming reads. It trails HC's content by one release
 * cycle in the other direction: it holds what live is ABOUT to get.
 *
 * Adding a name here does NOT finish the job. A site keyed BY dataset name — an
 * expected-count table, a per-fork floor — keeps its old keys and goes quiet on the
 * new one rather than failing. `scripts/audit-dataset-roster.cjs` is the census that
 * finds those.
 */
const ALL_DATASETS = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'];

const KNOWN_DATASETS = new Set(ALL_DATASETS);

/**
 * The fork subdirectories that sit INSIDE `exported_powers/`, for a walker that starts at the
 * root because Homecoming's export is flat there.
 *
 * Derived, not written out, and that is the point. Four sites each carried their own literal
 * `new Set(['rebirth', 'thunderspy'])` and every one of them predated `brainstorm`, so a
 * homecoming run descended into `exported_powers/brainstorm/` and counted the beta shard's
 * powers as Homecoming's own. Measured on 2026-09-26, before the fix:
 * `detect-caster-meters` reported 158 declined where the truth is 79 — exactly doubled — and
 * `detect-chance-mod-selectors` listed every entry twice under the SAME names, which is why
 * nobody noticed: 87% of the brainstorm export is byte-identical to Homecoming's, so the
 * contamination showed up as doubled counts rather than as unfamiliar rows.
 *
 * The warning above this — "adding a name here does NOT finish the job" — is exactly what went
 * wrong, and `audit-dataset-roster.cjs` could not catch it because these are COMPLEMENT sets.
 * The census looks for a roster that fails to name every dataset; a roster written inside-out
 * names none of them, and reported "partial rosters (0)". A derived set cannot drift.
 */
const NESTED_DATASET_DIRS = new Set(ALL_DATASETS.filter((d) => d !== 'homecoming'));

/**
 * Read `--dataset <id>` (or `--dataset=<id>`) from argv. Returns the id
 * or `'homecoming'` if no flag was provided.
 */
function parseDatasetArg(argv = process.argv) {
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--dataset' && i + 1 < argv.length) return validate(argv[i + 1]);
    if (a.startsWith('--dataset=')) return validate(a.slice('--dataset='.length));
  }
  return 'homecoming';
}

function validate(id) {
  if (!KNOWN_DATASETS.has(id)) {
    throw new Error(
      `Unknown dataset "${id}". Known: ${[...KNOWN_DATASETS].join(', ')}. ` +
      `Add it to KNOWN_DATASETS in scripts/_dataset-paths.cjs first.`,
    );
  }
  return id;
}

/**
 * Resolve a path under `pipeline/<id>/` — where a converter writes the JSON the
 * contract emitter reads.
 *
 * This is the address that replaced `datasetPath` (deleted 2026-09-25, with `src/`).
 * A converter used to write
 * `export const X: T = {...}` into `src/data/datasets/<id>/`, which put the TS
 * reference app in the middle of the path the shipped data travels: export →
 * converters → oracle → contract → app. The oracle is what the Rust engine is
 * GRADED against, so having the graded pipeline read it made "the oracle is frozen"
 * and "the pipeline still builds" the same sentence about the same file.
 *
 * A converter writing here emits one JSON object whose KEYS ARE THE EXPORT NAMES it
 * used to declare — `{ "BOOST_INDEX": ... }`, not a bare array — because that is what
 * `emit-contract.cjs` destructures. Same names, same values, no `export` keyword.
 *
 * `pipeline/` is gitignored and rebuilt rather than committed: `exported_powers/` IS
 * committed, so a clone with no game install regenerates the whole tree.
 */
function pipelinePath(datasetId, ...sub) {
  return path.join(REPO_ROOT, 'pipeline', datasetId, ...sub);
}

module.exports = {
  ALL_DATASETS,
  NESTED_DATASET_DIRS,
  parseDatasetArg,
  pipelinePath,
  REPO_ROOT,
};
