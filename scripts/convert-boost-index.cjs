/**
 * The boost index — every enhancement the game can name, keyed by the name the
 * game prints for it.
 *
 * A `/buildsave` from the game client writes a slotted enhancement as its
 * binary boost name (`Crafted_Bonesnap_A`, `Magic_Accuracy`,
 * `Synthetic_Hamidon_Damage_Accuracy`). Nothing in the contract carried that
 * spelling: `io-sets.json` names sets and pieces by display slug, and
 * `enhancements.json` names common IOs and origins by planner stat. Importing a
 * game build meant matching display names, which is how the beta importer ended
 * up with eight hand-maintained alias tables.
 *
 * This module emits the join instead — one entry per boost record, pointing at
 * the section that already describes it:
 *
 *   io-set      -> `io-sets.json` set id + piece number (+ whether attuned)
 *   common-io   -> the planner stat(s) and the crafted level
 *   origin      -> TO/DO/SO, the gating origins, and the planner stat(s)
 *   special     -> `enhancements.json` family tag + entry id
 *
 * Nothing is duplicated: an entry carries identity, never values.
 *
 * Every record lands in exactly one bucket or in `unclassified`, which is
 * emitted rather than dropped — a build naming one fails loud at import instead
 * of silently resolving to something plausible. The unclassified list is
 * printed on every run.
 *
 * Regenerate: node scripts/convert-boost-index.cjs --dataset <id>
 */

const fs = require('fs');
const path = require('path');
require('tsx/cjs');
const { parseDatasetArg, datasetPath } = require('./_dataset-paths.cjs');
const { BOOST_TYPE_STATS, aspectTokens, aspectStats } = require('./_boost-stats.cjs');

const datasetId = parseDatasetArg();

// HC ships at the legacy flat layout (`exported_powers/...`); other datasets
// are namespaced under `exported_powers/<id>/`.
const EXPORT_BASE = path.join(__dirname, '..', 'exported_powers');
const EXPORT_ROOT =
  datasetId === 'homecoming' && !fs.existsSync(path.join(EXPORT_BASE, datasetId, 'boostsets.json'))
    ? EXPORT_BASE
    : path.join(EXPORT_BASE, datasetId);

const OUTPUT_PATH = datasetPath(datasetId, 'generated', 'boost-index.ts');

// The `enhancements` section key -> the category tag a slotted special carries
// (`EnhancementKind::Special`, matched by `EnhancementCatalog::special_families`).
// The index speaks the tag because that is what a build stores.
const FAMILY_TAGS = {
  hamidon: 'hamidon',
  syntheticHamidon: 'synthetic-hamidon',
  titan: 'titan',
  hydra: 'hydra',
  dsync: 'd-sync',
  prestige: 'prestige',
};

const ORIGIN_NAMES = ['Magic', 'Mutation', 'Natural', 'Science', 'Technology'];
const ORIGIN_BY_SEGMENT = new Map(ORIGIN_NAMES.map((o) => [o.toLowerCase(), o]));

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf-8'));
}

/**
 * The io-sets set id for a binary boostset name — the same derivation
 * `extract-rebirth-io-sets-v2.py` keys its output on, and asserted against the
 * emitted sets below so the two cannot drift.
 */
function setIdOf(binaryName) {
  return binaryName.toLowerCase().replace(/-/g, '').replace(/__/g, '_');
}

/**
 * Whether a set piece is the attuned variant. Crafted and attuned records are
 * byte-identical apart from their names — the export states the distinction
 * nowhere else — so the name carries it. A Superior ATO spells its attuned
 * variant `Superior_Attuned_*` because the Superior set is its own set, and its
 * pieces are attuned like any other; an unrecognized prefix throws rather than
 * defaulting either way.
 */
function attunedOf(boostName, where) {
  if (boostName.startsWith('Superior_Attuned_')) return true;
  if (boostName.startsWith('Attuned_')) return true;
  if (boostName.startsWith('Crafted_')) return false;
  throw new Error(`${where}: set piece "${boostName}" carries no crafted/attuned prefix`);
}

/**
 * The origin family a non-set record belongs to, by the export's own namespace:
 * `Generic_*` = TO (all five origins), `<OriginA>_<OriginB>_*` = DO,
 * `<Origin>_*` = SO. `null` for a record in no origin family.
 */
function originFamilyOf(recordName) {
  const parts = recordName.toLowerCase().split('_');
  if (parts[0] === 'generic') return { tier: 'TO', origins: [...ORIGIN_NAMES] };
  const first = ORIGIN_BY_SEGMENT.get(parts[0]);
  if (!first) return null;
  const second = parts.length > 1 ? ORIGIN_BY_SEGMENT.get(parts[1]) : undefined;
  if (second) return { tier: 'DO', origins: [first, second] };
  return { tier: 'SO', origins: [first] };
}

/**
 * The crafted level a common IO's name pins, or `null` for the level-scaling
 * template (the record on the class table rather than a pinned Ones fraction).
 */
function craftedLevelOf(recordName) {
  const m = /_(\d+)$/.exec(recordName);
  return m ? Number(m[1]) : null;
}

function loadRecords() {
  const boostsDir = path.join(EXPORT_ROOT, 'boosts');
  const records = new Map(); // record name -> parsed power json
  const byLower = new Map(); // lower-cased name -> record name
  for (const dir of fs.readdirSync(boostsDir).sort()) {
    const file = path.join(boostsDir, dir, `${dir}.json`);
    if (!fs.existsSync(file)) throw new Error(`boosts/${dir}: no ${dir}.json`);
    const power = readJson(file);
    if (!power.name) throw new Error(`boosts/${dir}: record carries no name`);
    const collision = byLower.get(power.name.toLowerCase());
    if (collision) throw new Error(`boosts/${dir}: "${power.name}" collides with "${collision}" case-insensitively`);
    records.set(power.name, power);
    byLower.set(power.name.toLowerCase(), power.name);
  }
  return { records, byLower };
}

/**
 * record name -> { set, piece, attuned }, from the binary's own set membership.
 *
 * A boostset's reference is resolved against the record names case-insensitively:
 * the two spellings genuinely disagree in places (Rebirth's Exploit Weakness set
 * names `Crafted_Exploit_Weakness_C` while the record calls itself
 * `..._c`), and the game's own name lookups are case-insensitive. The record's
 * own name wins as the canonical spelling; the disagreements are reported.
 */
function setMembership(records, byLower) {
  const sets = readJson(path.join(EXPORT_ROOT, 'boostsets.json'));
  const claimed = new Map();
  const emptySets = [];
  const respelled = [];
  for (const set of sets) {
    const setId = setIdOf(set.name);
    if (!set.boostlists.length) {
      emptySets.push(setId);
      continue;
    }
    set.boostlists.forEach((boostlist, i) => {
      for (const full of boostlist.boosts) {
        const referenced = full.split('.').pop();
        const boostName = byLower.get(referenced.toLowerCase());
        if (!boostName) {
          throw new Error(`${setId} piece ${i + 1}: boostset names "${referenced}", which has no boost record`);
        }
        if (boostName !== referenced) respelled.push(`${referenced} -> ${boostName}`);
        const prior = claimed.get(boostName);
        if (prior) {
          throw new Error(
            `"${boostName}" is claimed by two pieces: ${prior.set}#${prior.piece} and ${setId}#${i + 1}`,
          );
        }
        claimed.set(boostName, { set: setId, piece: i + 1, attuned: attunedOf(boostName, setId) });
      }
    });
  }
  return { claimed, emptySets, respelled };
}

/** record name -> { family, id }, inverted from the special registries. */
function specialMembership(records) {
  const { SPECIAL_ENHANCEMENTS } = require(datasetPath(datasetId, 'generated', 'special-enhancements.ts'));
  if (SPECIAL_ENHANCEMENTS.dataset !== datasetId) {
    throw new Error(`generated/special-enhancements.ts is for ${SPECIAL_ENHANCEMENTS.dataset}, not ${datasetId}`);
  }
  const claimed = new Map();
  for (const [key, registry] of Object.entries(SPECIAL_ENHANCEMENTS)) {
    if (key === 'dataset') continue;
    const family = FAMILY_TAGS[key];
    if (!family) throw new Error(`special family "${key}" has no category tag — extend FAMILY_TAGS`);
    for (const [id, def] of Object.entries(registry)) {
      if (!def.boost) throw new Error(`special ${key}.${id}: no source boost record`);
      if (!records.has(def.boost)) {
        throw new Error(`special ${key}.${id}: names boost "${def.boost}", which has no boost record`);
      }
      claimed.set(def.boost, { family, id });
    }
  }
  return claimed;
}

function classify(records, byLower) {
  const { claimed: setPieces, emptySets, respelled } = setMembership(records, byLower);
  const specials = specialMembership(records);
  const entries = new Map();
  const unclassified = [];
  const commonIoTokens = new Set();

  for (const [name, power] of records) {
    const piece = setPieces.get(name);
    if (piece) {
      entries.set(name, { kind: 'io-set', set: piece.set, piece: piece.piece, attuned: piece.attuned });
      continue;
    }
    const special = specials.get(name);
    if (special) {
      entries.set(name, { kind: 'special', family: special.family, id: special.id });
      continue;
    }
    // A record with no aspect token enhances nothing this vocabulary can name
    // (the standalone -Regen proc IOs); it is not an origin or common IO
    // however its name reads.
    if (aspectTokens(power.boosts_allowed).length === 0) {
      unclassified.push(name);
      continue;
    }
    const origin = originFamilyOf(name);
    if (origin) {
      entries.set(name, {
        kind: 'origin',
        tier: origin.tier,
        origins: origin.origins,
        stats: aspectStats(power.boosts_allowed, name),
      });
      continue;
    }
    if (name.startsWith('Crafted_')) {
      const tokens = aspectTokens(power.boosts_allowed);
      if (tokens.length !== 1) {
        throw new Error(`${name}: a common IO enhances one boost type, not ${tokens.length} (${tokens})`);
      }
      commonIoTokens.add(tokens[0]);
      entries.set(name, {
        kind: 'common-io',
        level: craftedLevelOf(name),
        stats: aspectStats(power.boosts_allowed, name),
      });
      continue;
    }
    unclassified.push(name);
  }

  for (const name of unclassified) entries.set(name, { kind: 'unclassified' });
  // The pickable common-IO types: one per boost type the crafted family carries. A token's
  // FIRST mapped stat is the pickable one — `Heal` enhances Healing and Absorb together, and
  // the picker offers that as one Healing IO, not two.
  const commonIoTypes = [...commonIoTokens].map((token) => BOOST_TYPE_STATS[token][0]).sort();
  return { entries, unclassified, emptySets, respelled, commonIoTypes };
}

/** Every io-set entry must name a piece the contract's io-sets section carries. */
function assertSetsResolve(entries) {
  const { IO_SETS_RAW } = require(datasetPath(datasetId, 'io-sets-raw.ts'));
  const missing = [];
  for (const [name, entry] of entries) {
    if (entry.kind !== 'io-set') continue;
    const set = IO_SETS_RAW[entry.set];
    if (!set) {
      missing.push(`${name}: no set "${entry.set}"`);
    } else if (!set.pieces.some((p) => p.num === entry.piece)) {
      missing.push(`${name}: set "${entry.set}" has no piece ${entry.piece}`);
    }
  }
  if (missing.length) {
    throw new Error(`boost index names ${missing.length} io-set piece(s) the set data lacks:\n  ${missing.join('\n  ')}`);
  }
}

function literal(entry) {
  const parts = [`kind: '${entry.kind}'`];
  if (entry.kind === 'io-set') {
    parts.push(`set: '${entry.set}'`, `piece: ${entry.piece}`, `attuned: ${entry.attuned}`);
  } else if (entry.kind === 'special') {
    parts.push(`family: '${entry.family}'`, `id: '${entry.id}'`);
  } else if (entry.kind === 'origin') {
    parts.push(
      `tier: '${entry.tier}'`,
      `origins: [${entry.origins.map((o) => `'${o}'`).join(', ')}]`,
      `stats: [${entry.stats.map((s) => `'${s}'`).join(', ')}]`,
    );
  } else if (entry.kind === 'common-io') {
    parts.push(`level: ${entry.level ?? 'null'}`, `stats: [${entry.stats.map((s) => `'${s}'`).join(', ')}]`);
  }
  return `{ ${parts.join(', ')} }`;
}

function main() {
  console.log(`Building boost index for ${datasetId}...`);
  const { records, byLower } = loadRecords();
  const { entries, unclassified, emptySets, respelled, commonIoTypes } = classify(records, byLower);
  assertSetsResolve(entries);

  const counts = {};
  for (const entry of entries.values()) counts[entry.kind] = (counts[entry.kind] ?? 0) + 1;

  const lines = [];
  lines.push('/**');
  lines.push(' * Boost index — AUTO-GENERATED, DO NOT EDIT.');
  lines.push(' *');
  lines.push(' * Every enhancement the game can name, keyed by the spelling the game client');
  lines.push(' * prints for it, pointing at the contract section that describes it.');
  lines.push(' * Built from boostsets.json + boosts/** by scripts/convert-boost-index.cjs.');
  lines.push(' *');
  lines.push(` * Regenerate: node scripts/convert-boost-index.cjs --dataset ${datasetId}`);
  lines.push(' */');
  lines.push('');
  lines.push('export interface BoostIndexEntry {');
  lines.push("  /** Which contract section describes this enhancement, or 'unclassified'. */");
  lines.push("  kind: 'io-set' | 'common-io' | 'origin' | 'special' | 'unclassified';");
  lines.push('  /** io-set: the `io-sets` set id and 1-based piece number, and whether attuned. */');
  lines.push('  set?: string;');
  lines.push('  piece?: number;');
  lines.push('  attuned?: boolean;');
  lines.push('  /** special: the family category tag a slotted piece carries, and the entry id. */');
  lines.push('  family?: string;');
  lines.push('  id?: string;');
  lines.push('  /** origin: the tier and the origins whose characters may slot it. */');
  lines.push("  tier?: 'TO' | 'DO' | 'SO';");
  lines.push('  origins?: string[];');
  lines.push('  /** common-io: the crafted level, or null for the level-scaling template. */');
  lines.push('  level?: number | null;');
  lines.push('  /** common-io and origin: the planner stats the record enhances. */');
  lines.push('  stats?: string[];');
  lines.push('}');
  lines.push('');
  lines.push('export interface BoostIndexData {');
  lines.push('  dataset: string;');
  lines.push('  entries: Record<string, BoostIndexEntry>;');
  lines.push('  /** The pickable common-IO types, one per boost type the crafted family carries. */');
  lines.push('  commonIoTypes: string[];');
  lines.push('}');
  lines.push('');
  lines.push('export const BOOST_INDEX: BoostIndexData = {');
  lines.push(`  dataset: '${datasetId}',`);
  lines.push('  entries: {');
  for (const name of [...entries.keys()].sort()) {
    lines.push(`    ${name}: ${literal(entries.get(name))},`);
  }
  lines.push('  },');
  lines.push(`  commonIoTypes: [${commonIoTypes.map((s) => `'${s}'`).join(', ')}],`);
  lines.push('};');
  lines.push('');

  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, lines.join('\n'), 'utf-8');
  console.log(
    `  wrote ${path.relative(path.join(__dirname, '..'), OUTPUT_PATH)} ` +
      `(${entries.size} records: ${Object.entries(counts).map(([k, n]) => `${k} ${n}`).join(', ')})`,
  );
  if (respelled.length) {
    console.log(`  ${respelled.length} boostset reference(s) disagree with the record's own casing: ${respelled.join(', ')}`);
  }
  if (emptySets.length) {
    console.log(`  ${emptySets.length} set(s) state no piece membership in the binary: ${emptySets.join(', ')}`);
  }
  if (unclassified.length) {
    console.log(`  ${unclassified.length} unclassified record(s), emitted so an import of one fails loud:`);
    console.log(`    ${unclassified.join(', ')}`);
  }
}

main();
