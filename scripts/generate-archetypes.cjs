/**
 * Bootstrap hand-data/<id>/archetypes.json for a NEW fork.
 *
 * Not part of the regen. Nothing in regen-all.cjs calls this, and nothing should:
 * its output is authored data, and authored data is edited by a person after this
 * script guesses the first draft. Run it once when a fork is added, then read what
 * it wrote and fix it.
 *
 * WHAT IT DERIVES. The per-AT roster — which primary/secondary powersets each
 * archetype can pick — is the part that genuinely differs between servers, so it
 * comes from this fork's converted powerset tree (`pipeline/<id>/powersets.json`).
 *
 * WHAT IT COPIES. Everything else about an archetype — display name, side,
 * description, inherent text, and the five hand-curated scalars — is CoH-intrinsic
 * and reused verbatim from Homecoming (`hand-data/homecoming/archetypes.json`),
 * which is the single source of truth for it.
 *
 * WHAT IT LEAVES ALONE. The binary-sourced stats (HP and cap curves, threat, damage
 * cap, movement and attribute ceilings) are never written here. They live in
 * `pipeline/<id>/archetype-stats.json`, come from this fork's own classes.bin, and
 * are merged in at regen time by convert-archetype-registry.cjs.
 *
 * This script used to be a TypeScript SOURCE transformer: it read Homecoming's
 * archetypes.ts as text, brace-matched each block out of it, regex-replaced the
 * roster arrays and emitted a file containing `...ARCHETYPE_BINARY_STATS[at]` — a
 * spread that only resolved when the emitted TypeScript was imported. Once the
 * authored data became JSON and the merge moved into a converter, all of that
 * apparatus had nothing left to do.
 *
 * Standard ATs get derived rosters. Kheldians (single blast + single aura) and VEATs
 * (one base set plus branches) keep Homecoming's arrays verbatim — their rosters are
 * fixed and identically named across servers, and their sets are filed under the
 * `epic` category, where a primary/secondary lookup finds nothing. Thunderspy's
 * custom Primalist archetype comes from a bespoke template.
 *
 * Usage: node scripts/generate-archetypes.cjs --dataset thunderspy
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath, REPO_ROOT } = require('./_dataset-paths.cjs');

const datasetId = parseDatasetArg();
if (datasetId === 'homecoming') {
  throw new Error('Refusing to overwrite the hand-authored Homecoming archetypes.json (it is the metadata source).');
}

const handPath = (id) => path.join(REPO_ROOT, 'hand-data', id, 'archetypes.json');
const OUT = handPath(datasetId);

function readOrDie(p, hint) {
  if (!fs.existsSync(p)) throw new Error(`missing ${path.relative(REPO_ROOT, p)}\n  ${hint}`);
  return JSON.parse(fs.readFileSync(p, 'utf8'));
}

const HC = readOrDie(handPath('homecoming'), 'Homecoming is the metadata source; restore it from git.');

// Which archetypes to emit, in display order, taken from Homecoming rather than typed
// out here. A hardcoded roster is how the old version of this script silently refused to
// bootstrap Homecoming's own Sentinel: the name was not in the list, so no amount of
// stats or powersets on the fork could put it in the output. Anything this fork has and
// Homecoming does not is reported at the end instead of vanishing.
const STANDARD = HC.standardArchetypeIds.filter((at) => HC.archetypes[at]);
// The two classes that keep Homecoming's roster verbatim: Kheldians (one blast set plus
// one aura set) and VEATs (one base set plus branches). Their sets are filed under the
// `epic` category, so a primary/secondary lookup would return nothing for them.
const KHELDIAN = ['peacebringer', 'warshade'];
const VEAT = ['arachnos-soldier', 'arachnos-widow'];

// Which archetypes this fork's binary carries stats for. Stops rather than treating an
// absent file as an empty set: an empty set makes every archetype below `continue`, and
// the result is a valid registry with no archetypes in it — a file that would load fine
// and offer the player nothing to build.
const STATS_KEYS = new Set(Object.keys(readOrDie(
  pipelinePath(datasetId, 'archetype-stats.json'),
  `Run: node scripts/convert-archetypes.cjs --dataset ${datasetId}`,
).ARCHETYPE_BINARY_STATS));

const POWERSETS = readOrDie(
  pipelinePath(datasetId, 'powersets.json'),
  `Run: node scripts/convert-all-powersets.cjs --dataset ${datasetId}`,
).MODULAR_POWERSETS;

const hasStats = (at) => STATS_KEYS.has(at);
const hasSets = (at) => Object.values(POWERSETS).some((p) => p.archetype === at);

/** Derived set ids for one archetype and slot, sorted. */
const derivedSets = (at, category) => Object.values(POWERSETS)
  .filter((p) => p.archetype === at && p.category === category)
  .map((p) => p.id)
  .sort();

const archetypes = {};
const emitted = [];

// Standard ATs — Homecoming's authored block with this fork's own rosters.
for (const at of STANDARD) {
  if (!hasStats(at) || !hasSets(at) || !HC.archetypes[at]) continue;
  archetypes[at] = {
    ...HC.archetypes[at],
    primarySets: derivedSets(at, 'primary'),
    secondarySets: derivedSets(at, 'secondary'),
  };
  emitted.push(at);
}

// Kheldians + VEATs — Homecoming's rosters and branches verbatim, except for any
// fork-specific branch whose sets exist here and which Homecoming does not list.
const epicSetExists = (id) => Object.values(POWERSETS).some((p) => p.id === id && p.category === 'epic');
for (const at of [...KHELDIAN, ...VEAT]) {
  if (!hasStats(at) || !hasSets(at) || !HC.archetypes[at]) continue;
  const block = { ...HC.archetypes[at] };
  // Thunderspy adds a third Widow branch, Tarantula, alongside Night Widow and
  // Fortunata. Homecoming has no such branch, so add it when its two sets are present.
  if (datasetId === 'thunderspy' && at === 'arachnos-widow'
      && epicSetExists('arachnos-widow/tarantula-training')
      && epicSetExists('arachnos-widow/tarantula-teamwork')) {
    block.branches = {
      tarantula: {
        name: 'Tarantula',
        primarySet: 'arachnos-widow/tarantula-training',
        secondarySet: 'arachnos-widow/tarantula-teamwork',
      },
      ...block.branches,
    };
  }
  archetypes[at] = block;
  emitted.push(at);
}

// Thunderspy Primalist — bespoke, with no Homecoming template to copy. A Kheldian-style
// form-shifter: one primary (Feral Might) and one secondary (Primal Gifts). Forms and
// per-attack lifesteal redirects are modeled separately (see the kheldian-* files).
if (datasetId === 'thunderspy' && hasStats('primalist') && hasSets('primalist')) {
  archetypes.primalist = {
    name: 'Primalist',
    side: 'villain',
    description: 'Savage shapeshifter that channels Primal Energy, switching between human (Primal), Hunter, and Prowler forms to reshape its attacks. A Thunderspy original archetype.',
    inherent: {
      name: 'Primal Energy',
      description: "Many of the Primalist's attacks grant Primal Energy, lost slowly over time. It is spent to execute devastating attacks, debuffs, and potent healing — some powers require 10 Primal Energy, others scale with the amount you have.",
    },
    stats: {
      baseEndurance: 100,
      baseRecovery: 1.67,
      damageModifier: { melee: 0.95, ranged: 0.5, aoe: 0.8 },
      buffDebuffModifier: 1.0,
      defenseCap: 0.45,
    },
    primarySets: derivedSets('primalist', 'primary'),
    secondarySets: derivedSets('primalist', 'secondary'),
  };
  emitted.push('primalist');
}

const epicIds = [...KHELDIAN, ...VEAT].filter((at) => emitted.includes(at));
const standardIds = emitted.filter((at) => !epicIds.includes(at));

const sortedById = {};
for (const id of Object.keys(archetypes).sort()) sortedById[id] = archetypes[id];

fs.mkdirSync(path.dirname(OUT), { recursive: true });
fs.writeFileSync(OUT, JSON.stringify({
  _comment: HC._comment,
  archetypeOrder: emitted,
  archetypes: sortedById,
  epicArchetypeIds: epicIds,
  standardArchetypeIds: standardIds,
}, null, 2) + '\n');

console.log(`Wrote ${path.relative(REPO_ROOT, OUT)}`);
console.log(`  ${emitted.length} archetypes: ${emitted.join(', ')}`);
for (const at of emitted) {
  console.log(`  ${at}: ${archetypes[at].primarySets.length} primary / ${archetypes[at].secondarySets.length} secondary`);
}
// An archetype this fork carries that Homecoming has no template for cannot be copied —
// Rebirth's Guardian is the standing example. Naming it is the whole value here; the old
// script simply left it out, and the omission looked exactly like a clean run.
const unTemplated = [...STATS_KEYS]
  .filter((at) => hasSets(at) && !emitted.includes(at))
  .sort();
if (unTemplated.length) {
  console.log(`\n${unTemplated.length} archetype(s) this fork has that Homecoming cannot supply metadata for:`);
  for (const at of unTemplated) console.log(`  ${at} — write its block by hand (name, side, description, inherent, the five scalars)`);
}

console.log('\nThis is a FIRST DRAFT of authored data. Read it and correct it before committing.');
