/**
 * Assemble the archetype registry → pipeline/<id>/archetypes.json.
 *
 * Two halves meet here, and the split is the point.
 *
 * AUTHORED — `hand-data/<id>/archetypes.json`: display name, side, description,
 * inherent blurb, the five hand-curated scalars (baseEndurance, baseRecovery,
 * damageModifier, buffDebuffModifier, defenseCap), the primary/secondary roster and
 * the VEAT branches. None of it derives from anything; if it is lost, only git has it.
 *
 * DERIVED — `pipeline/<id>/archetype-stats.json`, written by convert-archetypes.cjs
 * out of classes.bin: the HP and HP-cap curves, resistance cap, threat, damage cap,
 * the absorb ceiling, the recharge/endurance clamp bounds, and the movement and
 * attribute ceilings. Every regen re-reads the binary for these, which is the whole
 * reason convert-archetypes.cjs exists — a hand-typed HP table used to diverge from
 * the live game in silence.
 *
 * This script is what used to be a spread. `archetypes.ts` wrote
 * `stats: { ...ARCHETYPE_BINARY_STATS[at], baseEndurance: 100, ... }` and that only
 * resolved when the TypeScript was imported, which is why the row outlived every other
 * entry in emit-pipeline-json.cjs's PER_DATASET list. The merge is the same merge; it
 * just happens here, over JSON, at regen time. Authored fields are appended after the
 * binary ones exactly as the spread ordered them — and no authored field shadows a
 * binary one, in any archetype, on any fork.
 *
 * Runs after convert-archetypes.cjs and fails loud if its output is missing: an absent
 * stats file treated as an empty one yields a registry whose archetypes have no HP,
 * which loads fine and offers the player nothing.
 *
 * Usage: node scripts/convert-archetype-registry.cjs [--dataset <id>]
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath, REPO_ROOT } = require('./_dataset-paths.cjs');

const datasetId = parseDatasetArg();

const HAND_PATH = path.join(REPO_ROOT, 'hand-data', datasetId, 'archetypes.json');
const STATS_PATH = pipelinePath(datasetId, 'archetype-stats.json');
const OUTPUT_PATH = pipelinePath(datasetId, 'archetypes.json');

// The order the spread produced: binary keys first, in binary order, then these five.
const AUTHORED_STATS = ['baseEndurance', 'baseRecovery', 'damageModifier', 'buffDebuffModifier', 'defenseCap'];
// The order archetypes.ts wrote each block's fields in.
const AT_FIELDS = ['name', 'side', 'description', 'inherent', 'stats', 'primarySets', 'secondarySets', 'branches'];

function readJson(p, what) {
  if (!fs.existsSync(p)) {
    throw new Error(
      `${what} missing for ${datasetId}:\n  ${path.relative(REPO_ROOT, p)} does not exist.\n`
      + (p === STATS_PATH ? `  Run: node scripts/convert-archetypes.cjs --dataset ${datasetId}\n` : ''),
    );
  }
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

const hand = readJson(HAND_PATH, 'authored archetype data');
const stats = readJson(STATS_PATH, 'binary archetype stats').ARCHETYPE_BINARY_STATS;

const ARCHETYPES = {};
for (const id of hand.archetypeOrder) {
  const authored = hand.archetypes[id];
  if (!authored) throw new Error(`archetypeOrder names ${id}, which hand-data/${datasetId}/archetypes.json does not define`);
  const binary = stats[id];
  // Not a skip. An archetype the binary has no row for would ship with no HP curve,
  // no caps and no damage ceiling — a build the calc would happily compute nonsense for.
  if (!binary) throw new Error(`no binary stats for ${datasetId}/${id} in ${path.relative(REPO_ROOT, STATS_PATH)}`);

  const merged = { ...binary };
  for (const k of AUTHORED_STATS) {
    if (!(k in authored.stats)) continue;
    // The spread let an authored key overwrite a binary one silently. None does today,
    // on any fork, and a new one would be a hand-typed value quietly beating the game's.
    if (k in binary) throw new Error(`${datasetId}/${id}: authored stat '${k}' would shadow the binary value`);
    merged[k] = authored.stats[k];
  }

  const out = {};
  for (const f of AT_FIELDS) {
    if (f === 'stats') out.stats = merged;
    else if (f in authored) out[f] = authored[f];
  }
  ARCHETYPES[id] = out;
}

fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
fs.writeFileSync(OUTPUT_PATH, JSON.stringify({
  ARCHETYPES,
  EPIC_ARCHETYPE_IDS: hand.epicArchetypeIds,
  STANDARD_ARCHETYPE_IDS: hand.standardArchetypeIds,
}));
console.log(`[convert-archetype-registry] ${datasetId}: ${Object.keys(ARCHETYPES).length} archetypes → ${path.relative(REPO_ROOT, OUTPUT_PATH)}`);
