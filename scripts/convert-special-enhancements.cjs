/**
 * SOURCE-1 item 9 — per-dataset special-enhancement registries.
 *
 * Emits `src/data/datasets/<id>/generated/special-enhancements.ts` from the
 * committed binary export's boost-piece templates, replacing the hand
 * Hamidon/Synthetic/Titan/Hydra/D-Sync/prestige registries in
 * `src/data/enhancements.ts`.
 *
 * Derivation per family (hamidon/titan/hydra/dsync):
 *  - A piece's ASPECTS are its `boosts_allowed` list minus the `Hamidon`
 *    slotting marker and the origin gates — the binary's own statement of what
 *    the piece enhances (Thunderspy authors sparse templates but full
 *    boosts_allowed, so this is the one signal complete on every dataset).
 *  - A piece's VALUES come from the family's per-schedule ladder: every
 *    special family sits on flat Ones tables, so its value depends only on
 *    the aspect's ED schedule. The ladder is solved from the family's own
 *    template fScales by fixpoint (uniform-schedule pieces pin cells; mixed
 *    pieces with one unknown cell resolve it) and then every template scale
 *    is validated against it — an inconsistent family throws.
 *    Homecoming carries all four families at the Hamidon rate (A 33.33 /
 *    B 20); Rebirth and Thunderspy keep Titan/Hydra at the legacy 25/15.
 *  - Prestige (Going Rogue pre-order, generic_-prefixed) has no
 *    schedule-keyed ladder semantics: its aspects and values are read from
 *    its typed enhancement templates directly (Damage/Recharge 16.66).
 *
 * The trailing "(Acc/Dam)"-style display-name suffix HC appends is stripped;
 * entry ids reproduce the legacy registry ids exactly (icons and saved-build
 * slots reference them) via family slug rules + a small compat override map.
 *
 * Regenerate: node scripts/convert-special-enhancements.cjs --dataset <id>
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath } = require('./_dataset-paths.cjs');
const { authored, deriveSchedules } = require('./_dim-returns.cjs');
const { BOOST_TYPE_STATS, aspectTokens } = require('./_boost-stats.cjs');

const datasetId = parseDatasetArg();

const EXPORT_BASE = path.join(__dirname, '..', 'exported_powers');
const EXPORT_ROOT =
  datasetId === 'homecoming' && !fs.existsSync(path.join(EXPORT_BASE, datasetId, 'enhancement_curves.json'))
    ? EXPORT_BASE
    : path.join(EXPORT_BASE, datasetId);

const OUTPUT_PATH = pipelinePath(datasetId, 'special-enhancements.json');

// `synthetic_hamidon` sits beside `hamidon` rather than folding into it: the
// eleven Synthetic pieces carry templates byte-identical to their Hamidon
// counterparts, but they are their own purchasable enhancements with their own
// display names, and a build that slots one must round-trip as the piece the
// player owns.
const FAMILIES = ['hamidon', 'synthetic_hamidon', 'titan', 'hydra', 'dsync'];

// Emission order and the TS/contract key each family is written under.
const FAMILY_KEYS = {
  hamidon: 'hamidon',
  synthetic_hamidon: 'syntheticHamidon',
  titan: 'titan',
  hydra: 'hydra',
  dsync: 'dsync',
  prestige: 'prestige',
};

// The Going Rogue pre-order pieces, generic_-prefixed in the export.
const PRESTIGE_PIECES = [
  'generic_clockwork_efficiency',
  'generic_might_of_the_empire',
  'generic_resistance_tactics',
  'generic_syndicate_techniques',
  'generic_will_of_the_seers',
];

// Legacy registry-id compat: slug(display name) -> the id icons and saved
// builds reference. `peridont` preserves the pre-binary hand typo the icon
// filename (TNPeridont.png) and existing build slots were keyed on.
const ID_OVERRIDES = {
  deltaparticle: 'delta',
  quarkparticle: 'quark',
  peridot: 'peridont',
};

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf-8'));
}

/** Display name minus the "(Acc/Dam)"-style suffix HC appends. */
function entryName(displayName) {
  return displayName.replace(/\s*\([^)]*\)\s*$/, '').trim();
}

function entryId(family, name) {
  let slug = name;
  if (family === 'hamidon' || family === 'synthetic_hamidon' || family === 'hydra') {
    slug = slug.replace(/\s+Exposure$/, '');
  }
  if (family === 'titan') slug = slug.replace(/^Titan\s+/, '').replace(/\s+Shard$/, '');
  if (family === 'dsync') slug = slug.replace(/^D-Sync\s+/, '');
  slug = slug.toLowerCase().replace(/[^a-z0-9]+/g, '');
  return ID_OVERRIDES[slug] ?? slug;
}

/** Enhancement-magnitude template scales on a piece: typed rows and the
 * Thunderspy attribs=[Ones] filler rows, both on Ones tables and not proc
 * riders. */
function templateScales(piece) {
  const typed = new Set();
  const ones = new Set();
  for (const effect of piece.effects || []) {
    for (const t of effect.templates || []) {
      if (t.aspect !== 'Strength') continue;
      if (t.flags !== null && t.flags !== undefined && !t.flags.includes('Boost')) continue;
      if (t.table !== 'Melee_Ones') continue;
      if (t.scale === null || t.scale === undefined) continue;
      if (JSON.stringify(t.attribs) === '["Ones"]') ones.add(authored(t.scale));
      else typed.add(authored(t.scale));
    }
  }
  return { typed: [...typed], ones: [...ones] };
}

function loadFamily(family, scheduleOf) {
  const boostsDir = path.join(EXPORT_ROOT, 'boosts');
  const pieces = [];
  for (const dir of fs.readdirSync(boostsDir).sort()) {
    if (!dir.startsWith(`${family}_`)) continue;
    const file = path.join(boostsDir, dir, `${dir}.json`);
    if (!fs.existsSync(file)) continue;
    const piece = readJson(file);
    const tokens = aspectTokens(piece.boosts_allowed);
    if (tokens.length === 0) throw new Error(`${dir}: no aspect tokens in boosts_allowed`);
    for (const token of tokens) {
      if (!BOOST_TYPE_STATS[token]) {
        throw new Error(
          `${dir}: unknown boosts_allowed token "${token}" — extend BOOST_TYPE_STATS in scripts/_boost-stats.cjs deliberately`,
        );
      }
    }
    const { typed, ones } = templateScales(piece);
    pieces.push({
      dir,
      boost: piece.name,
      name: entryName(piece.display_name || ''),
      tokens,
      schedules: [...new Set(tokens.map(scheduleOf))],
      scales: [...new Set([...typed, ...ones])],
    });
  }
  return pieces;
}

/** Solve the family's schedule -> fScale ladder from its pieces by fixpoint,
 * then validate every template scale against it. */
function familyLadder(family, pieces) {
  const cells = {}; // schedule -> { scale, from }
  let assigned = true;
  while (assigned) {
    assigned = false;
    for (const p of pieces) {
      const knownScales = new Set(p.schedules.filter((s) => cells[s]).map((s) => cells[s].scale));
      const unmatchedScales = p.scales.filter((sc) => !knownScales.has(sc));
      const unknownSchedules = p.schedules.filter((s) => !cells[s]);
      if (unmatchedScales.length === 1 && unknownSchedules.length === 1) {
        cells[unknownSchedules[0]] = { scale: unmatchedScales[0], from: p.dir };
        assigned = true;
      }
    }
  }
  for (const p of pieces) {
    const allowed = new Set(p.schedules.map((s) => cells[s]?.scale));
    for (const sc of p.scales) {
      if (!allowed.has(sc)) {
        throw new Error(
          `${family}: ${p.dir} template scale ${sc} matches no ladder cell for its schedules [${p.schedules}] (ladder ${JSON.stringify(cells)})`
        );
      }
    }
  }
  return cells;
}

function deriveFamily(family, scheduleOf) {
  const pieces = loadFamily(family, scheduleOf);
  if (pieces.length === 0) return {}; // family absent on this dataset (forks carry no D-Sync)
  const ladder = familyLadder(family, pieces);
  const registry = {};
  for (const p of pieces) {
    const id = entryId(family, p.name);
    if (registry[id]) throw new Error(`${family}: id collision "${id}" (${p.dir})`);
    const aspects = [];
    for (const token of p.tokens) {
      const cell = ladder[scheduleOf(token)];
      if (!cell) throw new Error(`${family}: ${p.dir} token ${token} has no ladder cell`);
      const value = Math.round(cell.scale * 100 * 100) / 100;
      for (const stat of BOOST_TYPE_STATS[token]) aspects.push({ stat, value });
    }
    registry[id] = { name: p.name, boost: p.boost, aspects };
  }
  return registry;
}

function derivePrestige() {
  const boostsDir = path.join(EXPORT_ROOT, 'boosts');
  const registry = {};
  for (const dir of PRESTIGE_PIECES) {
    const file = path.join(boostsDir, dir, `${dir}.json`);
    if (!fs.existsSync(file)) continue;
    const piece = readJson(file);
    // Prestige display names carry a ": Damage/Recharge/Chance for X" aspect
    // suffix; the entry name is the part before it.
    const name = entryName((piece.display_name || '').split(':')[0]);
    const byStat = new Map();
    for (const effect of piece.effects || []) {
      for (const t of effect.templates || []) {
        if (t.aspect !== 'Strength') continue;
        if (t.flags !== null && t.flags !== undefined && !t.flags.includes('Boost')) continue;
        if (t.table !== 'Melee_Ones') continue;
        if (t.scale === null || t.scale === undefined) continue;
        const attribs = t.attribs || [];
        if (attribs.length === 1 && attribs[0] === 'Ones') continue;
        // Prestige carries exactly two enhancement aspects; the recharge row
        // is a single attrib, everything else is the damage-type block.
        const isRecharge = attribs.length === 1 && (attribs[0] === 'RechargeTime' || attribs[0] === 'Recharge');
        const isDamage = attribs.length > 0 && attribs.every((a) => a.endsWith('_Dmg') || a === 'Damage');
        if (!isRecharge && !isDamage) {
          throw new Error(`${dir}: unrecognized prestige enhancement attribs ${JSON.stringify(attribs)}`);
        }
        const stat = isRecharge ? 'Recharge' : 'Damage';
        const value = Math.round(authored(t.scale) * 100 * 100) / 100;
        if (byStat.has(stat) && byStat.get(stat) !== value) {
          throw new Error(`${dir}: ${stat} templates carry two scales`);
        }
        byStat.set(stat, value);
      }
    }
    if (byStat.size === 0) throw new Error(`${dir}: no prestige enhancement templates`);
    const id = name.toLowerCase().replace(/[^a-z0-9]+/g, '_');
    registry[id] = {
      name,
      boost: piece.name,
      aspects: [...byStat.entries()].map(([stat, value]) => ({ stat, value })),
    };
  }
  return registry;
}


function main() {
  console.log(`Converting special enhancements for ${datasetId}...`);
  const curves = readJson(path.join(EXPORT_ROOT, 'enhancement_curves.json'));
  const { boostTypeSchedules, defaultSchedule } = deriveSchedules(curves);
  const scheduleOf = (token) => boostTypeSchedules[token] ?? defaultSchedule;

  const registries = {};
  for (const family of FAMILIES) {
    registries[family] = deriveFamily(family, scheduleOf);
  }
  registries.prestige = derivePrestige();

  /*
   * THE SHAPE THIS WRITES — `pipeline/<id>/special-enhancements.json`, one object under
   * the key `SPECIAL_ENHANCEMENTS`. Derived from the committed binary export (the
   * boosts/{hamidon,synthetic_hamidon,titan,hydra,dsync,generic prestige}_* piece
   * templates plus boosts_allowed) — SOURCE-1 item 9.
   *
   * `dataset`, then one registry per family under the camel-cased names in FAMILY_KEYS:
   * hamidon, syntheticHamidon, titan, hydra, dsync, prestige. `dsync` is EMPTY on the
   * forks, which carry no D-Sync pieces — an empty registry, not a missing key.
   *
   * Each entry is `{ name, boost, aspects: [{ stat, value }] }`, where `boost` is the
   * binary boost record it came from (`Hamidon_Damage_Accuracy`) — the spelling the game
   * client prints for a slotted piece — and `value` is the schedule-resolved percentage
   * off that family's ladder.
   */
  const out = JSON.stringify({
    SPECIAL_ENHANCEMENTS: {
      dataset: datasetId,
      ...Object.fromEntries(Object.keys(FAMILY_KEYS).map((f) => [FAMILY_KEYS[f], registries[f]])),
    },
  });

  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, out);
  const counts = Object.keys(FAMILY_KEYS)
    .map((k) => `${FAMILY_KEYS[k]} ${Object.keys(registries[k]).length}`)
    .join(', ');
  console.log(`  wrote ${path.relative(path.join(__dirname, '..'), OUTPUT_PATH)} (${counts})`);
}

main();
