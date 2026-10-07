/**
 * The join a gate needs to grade a converter from OUTSIDE it: generated power → the exported
 * record it was converted from.
 *
 * A check that lives inside one converter can only ever grade that converter, which is the
 * blind spot the three-converters drift keeps walking into (SNIPE-2, Plan B, Aid Other,
 * COND-2 — each a capability built in `convert-powerset.cjs` and never given to the other
 * two). A check that joins the shared INPUT to the combined OUTPUT sees all three at once.
 *
 * Extracted from `audit-form-coverage.cjs`, the first gate of this shape, so a second one
 * (`audit-conditional-coverage.cjs`) does not re-derive the join and drift from it — the very
 * failure mode both gates exist to catch.
 */

require('tsx/cjs');
const fs = require('fs');
const path = require('path');

const REPO = path.resolve(__dirname, '..');

/** The fork subtrees that nest under the Homecoming export root. */
const FORK_SUBTREES = new Set(require('./_dataset-paths.cjs').ALL_DATASETS);

/**
 * The converter module, loaded per dataset — `RAW_DATA_PATH` and the redirect resolver are
 * bound to `--dataset` at require time, so each dataset needs its own module instance.
 */
function converterFor(dataset) {
  const modulePath = require.resolve('./convert-powerset.cjs');
  delete require.cache[modulePath];
  const saved = process.argv;
  process.argv = [saved[0], modulePath, '--dataset', dataset];
  try {
    return require(modulePath);
  } finally {
    process.argv = saved;
  }
}

/** An identity folded to the export's file-name alphabet: lower case, runs of other characters
 *  collapsed to one underscore, and no leading or trailing separator. */
const fileFold = (ident) =>
  ident.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');

/**
 * The export directory a converted powerset names as its input, or null.
 *
 * Two spellings, because the converters emit two formats. The TypeScript modules under
 * `src/data/datasets/<id>/generated/powersets/` state it in a `Source:` header comment,
 * one file per power. The JSON under `pipeline/<id>/powersets/` is one file per POWERSET
 * and cannot carry a comment, so it states the same thing as data: `setPath`, the export's
 * dotted path (`Scrapper_Defense.Bio_Organic_Armor`), which is the header's directory in
 * the game's own spelling.
 *
 * Anything else — the pool, epic, inherent and accolade files — returns null here and joins
 * on `fullName` instead, exactly as it did when those were TypeScript.
 */
function sourceDirOf(relPath) {
  const body = fs.readFileSync(path.join(REPO, relPath), 'utf-8');
  if (relPath.endsWith('.json')) {
    let parsed;
    try {
      parsed = JSON.parse(body);
    } catch {
      return null;
    }
    const setPath = parsed && typeof parsed.setPath === 'string' ? parsed.setPath : null;
    return setPath ? setPath.toLowerCase().split('.').join(path.sep) : null;
  }
  const match = body.slice(0, 2048).match(/^\s*\*\s*Source:\s*(\S+\.json)\s*$/m);
  return match ? path.dirname(match[1]) : null;
}

/** Every export file indexed by the last segment of its path, for the fallback join. */
function indexBySegment(root, forkSubtrees = FORK_SUBTREES) {
  const bySegment = new Map();
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        // A fork's export nests under the Homecoming root; its powers are not this
        // dataset's, and letting them in would make an unrelated fork's file the unique
        // answer for a name this dataset never emits.
        if (dir === root && forkSubtrees.has(entry.name)) continue;
        stack.push(p);
        continue;
      }
      if (!entry.name.endsWith('.json')) continue;
      const key = entry.name.slice(0, -'.json'.length);
      if (!bySegment.has(key)) bySegment.set(key, []);
      bySegment.get(key).push(p);
    }
  }
  return bySegment;
}

/**
 * The exported record a generated power was converted from.
 *
 * Three joins, because the three partitions carry different identity: a powerset module
 * names its source directory in its header and its powers keep `internalName`; pool and
 * epic powers ship PRE-transform and carry the dotted `fullName` instead (PROD6C-3h); the
 * inherent powerset carries neither, and falls back to a last-segment lookup that is used
 * only when it resolves to exactly one export.
 */
function resolveExport(cv, power, relPath, dirCache, bySegment) {
  const ident = power.internalName || power.name;
  if (!dirCache.has(relPath)) dirCache.set(relPath, sourceDirOf(relPath));
  const dir = dirCache.get(relPath);
  // The export's file names carry only `[a-z0-9_]`, while an identity can hold the
  // punctuation the game's display name had (`Combat_Training:_Defensive`) or a doubled
  // separator where it had two spaces (`Enervating__Field`). Try the identity verbatim
  // first — that is the exact join — then the folded spelling.
  for (const spelling of ident ? [ident.toLowerCase(), fileFold(ident)] : []) {
    if (!dir) break;
    const candidate = path.join(cv.RAW_DATA_PATH, dir, `${spelling}.json`);
    if (fs.existsSync(candidate)) return candidate;
  }
  if (power.fullName) {
    const candidate = cv.resolveRedirectPath(power.fullName);
    if (fs.existsSync(candidate)) return candidate;
  }
  const matches = ident ? bySegment.get(ident.toLowerCase()) : undefined;
  return matches && matches.length === 1 ? matches[0] : null;
}

/**
 * A dataset's join, ready to call: `{ cv, join(power, relPath) }` where `join` returns the
 * export path or null.
 */
function joinerFor(dataset) {
  const cv = converterFor(dataset);
  const bySegment = indexBySegment(cv.RAW_DATA_PATH);
  const dirCache = new Map();
  return {
    cv,
    join: (power, relPath) => resolveExport(cv, power, relPath, dirCache, bySegment),
  };
}

module.exports = {
  REPO,
  FORK_SUBTREES,
  converterFor,
  fileFold,
  sourceDirOf,
  indexBySegment,
  resolveExport,
  joinerFor,
};
