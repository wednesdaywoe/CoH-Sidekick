/**
 * Batch Powerset Conversion Script
 *
 * Converts ALL raw Homecoming power data to the new modular structure.
 * Usage: node scripts/convert-all-powersets.cjs [--force]
 *   --force  Reconvert even if output directory already exists
 */

const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath } = require('./_dataset-paths.cjs');

// CATEGORY_MAP is the single raw-category → archetype/type routing table; the
// per-powerset converter owns it and this orchestrator only iterates it. `toKebabCase`
// comes from the same place for the same reason — see `shardFor` below.
const { RAW_DATA_PATH, CATEGORY_MAP, toKebabCase } = require('./convert-powerset.cjs');

const datasetId = parseDatasetArg();
// Forward `--dataset <id>` to each child convert-powerset.cjs invocation
// so the per-powerset converter writes into the same dataset folder.
const datasetFlag = `--dataset ${datasetId}`;

const force = process.argv.includes('--force');


let converted = 0;
let failed = 0;
let skipped = 0;
const errors = [];

// Where the per-powerset converter's output lands: one JSON shard per powerset.
const SHARD_ROOT = pipelinePath(datasetId, 'powersets');

/**
 * This powerset's shard path — the file the child is about to write.
 *
 * It has to be READ out of the set's own `index.json` rather than derived, because the
 * slug is the set's DISPLAY name and the export's directory name is its internal one.
 * That is deliberate on the converter's side — the display slug is the powerset's `id`
 * and its file name in `contract/`, while the internal name ships as `setPath` — so this
 * is not a quirk to route around, it is the identity, and the only place that states it
 * is the converter. Hence: ask the same file the converter asks.
 *
 * Deriving it the cheap way is what this script did until 2026-09-25 — it spelled the
 * output directory `powerset.replace(/_/g, '-')`, a guess at the slug from the internal
 * name, which named a path the converter never wrote for the 26 of Homecoming's 364 sets
 * and 37 of Thunderspy's 305 where the two names differ (`gadgets` is `devices`,
 * `bio_organic_armor` is `bio-armor`, `martial_manipulation` is `martial-combat`). The
 * `[EXISTS]` check below therefore never fired for any of them and they were reconverted
 * on every run, and the staging that used to be here never staged them either. Both
 * failures were silent, and a wrong path only ever looks like a cache miss.
 */
function shardFor(category, info, powerset) {
  const indexPath = path.join(powersPath, category, powerset, 'index.json');
  const { display_name: displayName } = JSON.parse(fs.readFileSync(indexPath, 'utf-8'));
  return path.join(SHARD_ROOT, info.archetype, info.type, `${toKebabCase(displayName)}.json`);
}

/**
 * STAGING WENT WITH THE DIRECTORY OUTPUT (2026-09-25).
 *
 * Each powerset used to be a directory of ten-odd TypeScript modules, so the child
 * could die partway through and leave some of them missing — Homecoming's Sentinel
 * Willpower, REGEN-1, 2026-08-10. This script moved the previous directory aside before
 * each child and moved it back on failure, and reconciled any staging directory an
 * interrupted run had left behind, because that leftover was sometimes the only copy.
 *
 * A powerset is one JSON shard now, and `convert-powerset.cjs` writes it by renaming a
 * temp file into place, so a dead child leaves the previous shard whole. There is
 * nothing to stage, nothing to reconcile, and nothing to restore on failure.
 */

console.log(`=== Batch Powerset Conversion${force ? ' (FORCE)' : ''} ===\n`);

// Bin export writes categories at <RAW_DATA_PATH>/<category>/.
// Old CoD2 layout had an extra `powers/` segment. Probe both.
const powersPath = (() => {
  const newLayout = RAW_DATA_PATH;  // categories are direct children
  const oldLayout = path.join(RAW_DATA_PATH, 'powers');
  // Detect by checking for any known category directory
  for (const cat of Object.keys(CATEGORY_MAP)) {
    if (fs.existsSync(path.join(newLayout, cat))) return newLayout;
    if (fs.existsSync(path.join(oldLayout, cat))) return oldLayout;
  }
  return newLayout;
})();

for (const [category, info] of Object.entries(CATEGORY_MAP)) {
  const categoryPath = path.join(powersPath, category);

  if (!fs.existsSync(categoryPath)) {
    // console.log(`[SKIP] Category not found: ${category}`);
    continue;
  }

  const powersets = fs.readdirSync(categoryPath)
    .filter(item => {
      const itemPath = path.join(categoryPath, item);
      return fs.statSync(itemPath).isDirectory();
    });

  console.log(`\n--- ${category} (${powersets.length} powersets) ---`);

  for (const powerset of powersets) {
    // The file the child is about to write, spelled the way the child spells it.
    const shardPath = shardFor(category, info, powerset);
    const setSlug = path.basename(shardPath, '.json');

    // Check if already converted (skip unless --force)
    if (!force && fs.existsSync(shardPath)) {
      console.log(`  [EXISTS] ${info.archetype}/${setSlug}`);
      skipped++;
      continue;
    }

    try {
      console.log(`  [CONVERT] ${category}/${powerset} -> ${info.archetype}/${info.type}/${setSlug}`);
      execSync(`node scripts/convert-powerset.cjs ${category} ${powerset} ${datasetFlag}`, {
        // Child stdout is the per-power listing (thousands of lines); its stderr is the
        // converter's warning path — a record it read but could not use. Piping BOTH made every
        // such warning vanish from the batch run that regen actually uses.
        stdio: ['ignore', 'pipe', 'inherit'],
        timeout: 30000
      });
      converted++;
    } catch (err) {
      // Nothing to put back: the child writes its shard by rename, so a child that
      // failed — a throw, or the 30s timeout above under load — left the previous one
      // untouched. See the staging note above.
      const stderr = err.stderr ? err.stderr.toString().trim() : err.message;
      console.log(`  [ERROR] ${category}/${powerset}: ${stderr.split('\n')[0]}`);
      errors.push({ category, powerset, error: stderr });
      failed++;
    }
  }
}

console.log('\n=== Summary ===');
console.log(`Converted: ${converted}`);
console.log(`Skipped (already exists): ${skipped}`);
console.log(`Failed: ${failed}`);

if (errors.length > 0) {
  console.log('\n=== Errors ===');
  for (const e of errors) {
    console.log(`  ${e.category}/${e.powerset}: ${e.error.split('\n')[0]}`);
  }
}

// Reporting a failed child as success is how Homecoming's Sentinel Willpower left
// the tree with a green `npm run regen` (2026-08-10): ten files gone, the run's
// exit code 0, and nothing downstream noticing until an import failed hours later.
// The rename-into-place above means the previous shard survives, so this is no longer
// a data-loss alarm — but a regen that could not convert a powerset has not
// regenerated it, and the shard tree is now a mix of fresh and stale. That is exactly
// the staleness the regen-and-diff guard exists to catch, and it can only catch it
// if the run says so.
if (failed > 0) {
  console.error(
    `\nconvert-all-powersets: ${failed} powerset(s) failed to convert. Their previous output `
      + 'shard was left in place, so the tree is a MIX of freshly-converted and stale '
      + 'powersets.',
  );
  process.exit(1);
}
