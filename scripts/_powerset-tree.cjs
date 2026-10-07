/**
 * The converted powerset tree, walked once.
 *
 * Four scripts read a dataset's powerset output and key it four different ways:
 * `convert-inherents.cjs` wants the display names, `audit-allowed-set-categories.cjs` wants a
 * power by its slug path, `validate-converter-output.cjs` indexes by archetype and internal
 * name, and `dsh6-collapse-detector.cjs` keys by the export file each power came from. Before
 * 2026-09-25 each had its own directory walk over `src/data/datasets/<id>/generated/powersets`,
 * which is four chances to disagree about which powers exist — the `planb-shadow-sweep.cjs`
 * lesson, which cost ~15% of the corpus the first time it was learned.
 *
 * So: one walk, here, over `pipeline/<id>/powersets/` — what `convert-powerset.cjs` writes.
 * A caller that wants a different key builds it from `ctx`; a caller cannot want a different
 * SET of powers, which is the part worth sharing.
 *
 * The JSON is one file per POWERSET where the TypeScript was one file per power, so a
 * per-file text regex is not a per-power answer any more. `ctx.json` is the power serialized
 * on its own for the few callers that legitimately match on the wire text.
 */

const fs = require('fs');
const path = require('path');
const { pipelinePath } = require('./_dataset-paths.cjs');

/** The powerset tree's root for one dataset. */
function powersetRoot(dataset) {
  return pipelinePath(dataset, 'powersets');
}

/**
 * Call `onPower(power, ctx)` for every power in one dataset's powerset tree.
 *
 * `ctx` carries what the callers key on:
 *   set        the powerset object the power came from
 *   relPath    `powersets/<archetype>/<type>/<slug>.json`, relative to `pipeline/<id>/`
 *   archetype  the first path segment, so same-named powers across archetypes do not collide
 *   exportDir  the export directory this powerset was converted from, or null — `setPath`
 *              (`Scrapper_Defense.Bio_Organic_Armor`) folded to the export's own spelling.
 *              This is the JSON spelling of the `Source:` header the TypeScript carried.
 *
 * Returns the number of powers visited, so a caller can fail loud on an empty read rather
 * than report a vacuous pass.
 */
function forEachPowersetPower(dataset, onPower) {
  const root = powersetRoot(dataset);
  let count = 0;
  if (!fs.existsSync(root)) return count;
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) { stack.push(full); continue; }
      if (!entry.name.endsWith('.json')) continue;
      let set;
      try {
        set = JSON.parse(fs.readFileSync(full, 'utf-8'));
      } catch {
        continue;
      }
      if (!Array.isArray(set.powers)) continue;
      const relPath = path.join('powersets', path.relative(root, full)).replace(/\\/g, '/');
      const archetype = relPath.match(/(?:^|\/)powersets\/([^/]+)\//)?.[1] || '';
      const exportDir = typeof set.setPath === 'string'
        ? set.setPath.toLowerCase().split('.').join('/')
        : null;
      for (const power of set.powers) {
        if (!power || typeof power !== 'object') continue;
        count += 1;
        onPower(power, { set, relPath, archetype, exportDir });
      }
    }
  }
  return count;
}

module.exports = { powersetRoot, forEachPowersetPower };
