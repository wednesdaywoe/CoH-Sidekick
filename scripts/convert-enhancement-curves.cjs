/**
 * SOURCE-1 SW3 — per-dataset generated enhancement-curves module.
 *
 * Emits `pipeline/<id>/enhancement-curves.json` from the committed binary
 * export, carrying everything the enhancement engine would otherwise
 * second-source from Mids/Wiki tables:
 *
 *   - ED tier thresholds + tier effectiveness   <- enhancement_curves.json dim_returns
 *   - boost-type -> ED schedule assignment       <- dim_returns boost_types (+ default)
 *   - per-boost-level enhancement strength curve <- tables/*.json Melee_Boosts_20/33/40/60
 *   - multi-aspect crafted-piece scale ladder    <- boosts/crafted_* fScale census
 *   - boost_effectiveness above/below/boosters   <- enhancement_curves.json
 *   - per-tier per-schedule origin values (SW8)  <- boosts/{generic,<origin>}_* fScale census
 *   - exemplar handicap curves (WS17)            <- exemplar_handicaps.json (exemplar_handicaps.bin)
 *
 * WHAT BACKS THESE DERIVATIONS, corrected 2026-09-27. This header used to cite
 * `scripts/derive-source1-constants.py` (SW1/SW2) as the proof. That script was
 * deleted the same day, because all three things it compared against are gone:
 * `src/data/enhancements.ts` and `src/utils/calculations/enhancement-values.ts`
 * went with the whole `src/` tree, and `crates/coh_math/src/enhancement.rs` no
 * longer holds one of the four constants the script grepped it for — its own
 * header now says "There are no local curve constants" (SW6). The proof was
 * one-time by its own docstring, it gated this wiring, the wiring landed, and
 * the hardcoded values it proved against were then removed BECAUSE it had. A
 * live emitter citing a proof that raises FileNotFoundError is the defect this
 * repository keeps finding in its own prose, so it is named rather than moved.
 *
 * What holds the numbers now, in the order a change would trip them:
 *   - this converter's own assertions. Every decode is checked — an unknown
 *     tier triple, a non-uniform handicap, a cross-AT table divergence, or an
 *     ambiguous census modal throws instead of shipping a soft-wrong module.
 *   - `npm run regen` + `git diff --exit-code` over the committed contract, so
 *     any movement in a derived number is a diff a person reads.
 *   - `crates/coh_math/tests/totals_baseline.rs`, which grades 20,163 numbers
 *     per fork end to end and reds on any of them moving.
 *
 * Regenerate: node scripts/convert-enhancement-curves.cjs --dataset <id>
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath, REPO_ROOT } = require('./_dataset-paths.cjs');
const { derivePlayerArchetypes } = require('./_player-classes.cjs');
const { authored, deriveSchedules } = require('./_dim-returns.cjs');

const datasetId = parseDatasetArg();

// HC ships at the legacy flat layout (`exported_powers/...`); other datasets
// are namespaced under `exported_powers/<id>/` (same convention as
// extract-at-tables.cjs).
const EXPORT_BASE = path.join(__dirname, '..', 'exported_powers');
const EXPORT_ROOT =
  datasetId === 'homecoming' && !fs.existsSync(path.join(EXPORT_BASE, datasetId, 'enhancement_curves.json'))
    ? EXPORT_BASE
    : path.join(EXPORT_BASE, datasetId);

const OUTPUT_PATH = pipelinePath(datasetId, 'enhancement-curves.json');

// The named class-modifier table each schedule's strength curve reads from;
// the suffix is the schedule's SO percentage (SW1 §2). The link is verified
// below against the crafted corpus: every single-table attrib family's pieces
// must sit on the table this map names for their schedule.
const TABLE_FOR_SCHEDULE = {
  A: 'Melee_Boosts_33',
  B: 'Melee_Boosts_20',
  C: 'Melee_Boosts_40',
  D: 'Melee_Boosts_60',
};

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, 'utf-8'));
}

// ---------------------------------------------------------------------------
// 1. dim_returns -> schedules, thresholds, tier effectiveness, boost-type map
//    (shared derivation: ./_dim-returns.cjs)
// 2. tables/*.json -> per-schedule strength curves (asserted AT-invariant)
// ---------------------------------------------------------------------------

// Invariance is asserted across PLAYER classes only: NPC classes (bosses,
// pets) genuinely carry different Boosts curves on Rebirth/Thunderspy, and
// only player classes slot enhancements.
function deriveStrengthCurves() {
  const tablesDir = path.join(EXPORT_ROOT, 'tables');
  const reference = {}; // table name -> { values, from }
  let classesChecked = 0;
  for (const archetype of derivePlayerArchetypes(tablesDir)) {
    const file = `${archetype}.json`;
    const filePath = path.join(tablesDir, file);
    const namedTables = readJson(filePath).named_tables;
    if (!namedTables || !namedTables[TABLE_FOR_SCHEDULE.A]) {
      throw new Error(`${file}: player class missing ${TABLE_FOR_SCHEDULE.A}`);
    }
    classesChecked += 1;
    for (const [letter, tableName] of Object.entries(TABLE_FOR_SCHEDULE)) {
      const meleeValues = namedTables[tableName];
      const rangedValues = namedTables[tableName.replace('Melee_', 'Ranged_')];
      if (!meleeValues) throw new Error(`${file}: missing ${tableName}`);
      const key = JSON.stringify(meleeValues);
      if (!reference[letter]) {
        reference[letter] = { values: meleeValues, key, from: file };
      } else if (reference[letter].key !== key) {
        throw new Error(`${tableName} differs between ${reference[letter].from} and ${file} — curve is not AT-invariant`);
      }
      if (rangedValues && JSON.stringify(rangedValues) !== key) {
        throw new Error(`${file}: ${tableName} != its Ranged_ twin — curve is not prefix-invariant`);
      }
    }
  }
  if (classesChecked === 0) throw new Error(`No class table in ${tablesDir} carries ${TABLE_FOR_SCHEDULE.A}`);
  console.log(`  strength curves AT-invariant across ${classesChecked} class tables`);
  const curves = {};
  for (const [letter, ref] of Object.entries(reference)) curves[letter] = ref.values;
  return curves;
}

// ---------------------------------------------------------------------------
// 3. boosts/crafted_* -> multi-aspect scale ladder + schedule->table proof
// ---------------------------------------------------------------------------

function isEnhancementTemplate(template) {
  // Thunderspy templates carry no flags list; where flags exist, non-Boost
  // segments are proc riders (SW1 §4 / SW2 census).
  if (template.aspect !== 'Strength') return false;
  if (template.flags !== null && template.flags !== undefined && !template.flags.includes('Boost')) return false;
  const table = template.table || '';
  return /_(20|33|40|60)$/.test(table);
}

function craftedCensus(attribFamily, defaultSchedule) {
  const boostsDir = path.join(EXPORT_ROOT, 'boosts');
  const scalesBySegmentCount = new Map(); // count -> Map(scale -> pieces)
  const tablesByFamily = new Map(); // schedule letter -> Set(table names)
  const knownTables = new Set(Object.values(TABLE_FOR_SCHEDULE));

  for (const dir of fs.readdirSync(boostsDir).sort()) {
    if (!dir.startsWith('crafted_')) continue;
    const file = path.join(boostsDir, dir, `${dir}.json`);
    if (!fs.existsSync(file)) continue;
    const piece = readJson(file);
    const templates = [];
    for (const effect of piece.effects || []) {
      for (const t of effect.templates || []) {
        if (isEnhancementTemplate(t)) templates.push(t);
      }
    }
    if (templates.length === 0) continue;

    for (const t of templates) {
      if (!knownTables.has(t.table)) {
        throw new Error(`${dir}: enhancement template on unknown table ${t.table}`);
      }
      // Schedule->table link evidence: a template whose attribs all belong to
      // one non-default dim_returns family pins that family's schedule to the
      // table it sits on. Families observed on more than one table (the
      // damage/resistance attrib-name collision — dim_returns carries no
      // aspect axis) are excluded from the assertion below.
      const letters = new Set(t.attribs.map((a) => attribFamily.get(a) ?? defaultSchedule));
      if (letters.size === 1) {
        const letter = letters.values().next().value;
        if (!tablesByFamily.has(letter)) tablesByFamily.set(letter, new Set());
        tablesByFamily.get(letter).add(t.table);
      }
    }

    // Multi-aspect ladder: the display name's slash-segment count is the
    // authored aspect count (the same signal the engine counts); the modal
    // scale per count is the standard ladder — runner-ups are the purple
    // x1.25 ladder and the Heal/Absorb segment collapse.
    const displayName = piece.display_name || '';
    if (!displayName.includes(':')) continue;
    const scales = new Set(templates.map((t) => Math.round(t.scale * 1e5) / 1e5));
    if (scales.size !== 1) continue;
    const count = Math.min(displayName.split(':')[1].split('/').length, 4);
    if (!scalesBySegmentCount.has(count)) scalesBySegmentCount.set(count, new Map());
    const dist = scalesBySegmentCount.get(count);
    const scale = scales.values().next().value;
    dist.set(scale, (dist.get(scale) || 0) + 1);
  }

  // Prove the TABLE_FOR_SCHEDULE link from the single-table families.
  const proven = new Set();
  for (const [letter, tables] of tablesByFamily) {
    if (tables.size !== 1) continue;
    const table = tables.values().next().value;
    if (table !== TABLE_FOR_SCHEDULE[letter]) {
      throw new Error(`Schedule ${letter} pieces sit on ${table}, but TABLE_FOR_SCHEDULE names ${TABLE_FOR_SCHEDULE[letter]}`);
    }
    proven.add(letter);
  }
  console.log(`  schedule->table link proven from single-table families: ${[...proven].sort().join(', ')}`);

  const multiAspectScale = [];
  for (const count of [1, 2, 3, 4]) {
    const dist = scalesBySegmentCount.get(count);
    if (!dist || dist.size === 0) throw new Error(`No crafted pieces with ${count} name segments — census cannot derive the ladder`);
    const ranked = [...dist.entries()].sort((a, b) => b[1] - a[1]);
    const [modal, modalCount] = ranked[0];
    if (ranked.length > 1 && ranked[1][1] >= modalCount) {
      throw new Error(`${count}-aspect scale is ambiguous: ${JSON.stringify(ranked.slice(0, 3))}`);
    }
    console.log(`  ${count}-aspect modal scale ${modal} (${modalCount} pieces; runner-up ${ranked[1] ? `${ranked[1][0]}x${ranked[1][1]}` : 'none'})`);
    multiAspectScale.push(modal);
  }
  if (multiAspectScale[0] !== 1) throw new Error(`Single-aspect modal scale is ${multiAspectScale[0]}, expected 1.0`);
  for (let i = 1; i < 4; i++) {
    if (multiAspectScale[i] >= multiAspectScale[i - 1]) {
      throw new Error(`Multi-aspect ladder not strictly decreasing: ${multiAspectScale}`);
    }
  }
  return multiAspectScale;
}

// ---------------------------------------------------------------------------
// 4. boosts/{generic,<origin>,<originA>_<originB>}_* -> origin-tier values (SW8)
// ---------------------------------------------------------------------------

// The per-tier origin families are named by origin, not tier (SW2): Generic_*
// = TO, <OriginA>_<OriginB>_* = DO, <Origin>_* = SO. Every family sits on
// Melee_Ones (flat 1.0), so the per-aspect value is the template fScale, and
// it depends only on the aspect's ED schedule — asserted below per cell.
const ORIGIN_NAMES = new Set(['magic', 'mutation', 'natural', 'science', 'technology']);

// Origin-piece aspect segment -> the dim_returns boost type governing its ED
// schedule (null = the default entry, the game's own miss path). Closed
// vocabulary: an unlisted segment is a new data variant and must break the
// build. `cone` is a Range boost (arc pieces carry the Range attrib).
const ORIGIN_SEGMENT_BOOST_TYPE = {
  accuracy: null,
  confuse: null,
  damage: null,
  defense_debuff: null,
  drain_endurance: null,
  endurance_discount: null,
  fear: null,
  fly: null,
  heal: null,
  hold: null,
  immobilize: null,
  intangible: null,
  jump: null,
  recharge: null,
  recovery: null,
  run: null,
  sleep: null,
  snare: null,
  stun: null,
  taunt: null,
  range: 'Range',
  defense_buff: 'Buff_Defense',
  tohit_buff: 'Buff_ToHit',
  tohit_debuff: 'Debuff_ToHit',
  interrupt: 'Interrupt',
  knockback: 'Knockback',
  res_damage: 'Res_Damage',
};

// Segments deliberately outside the tier grid:
//  - cone (arc enhancement): no engine aspect surfaces it (COMMON_IO_TYPES has
//    no Arc/Cone), and the game's own DO cone pieces are authored at TWO
//    scales (0.1 and 0.2 on HC/Rebirth) — it has no per-schedule value.
const EXCLUDED_ORIGIN_SEGMENTS = new Set(['cone']);

// The Going Rogue pre-order pieces live under the generic_ prefix but are
// prestige enhancements, not TO origin pieces — they belong to the
// special-enhancement registries, not the tier grid.
const PRESTIGE_PIECE_SEGMENTS = new Set([
  'clockwork_efficiency',
  'might_of_the_empire',
  'resistance_tactics',
  'syndicate_techniques',
  'will_of_the_seers',
]);

function originTierOf(dirName) {
  const parts = dirName.split('_');
  if (parts[0] === 'generic') return { tier: 'TO', segment: parts.slice(1).join('_') };
  if (!ORIGIN_NAMES.has(parts[0])) return null;
  if (parts.length > 1 && ORIGIN_NAMES.has(parts[1])) {
    return { tier: 'DO', segment: parts.slice(2).join('_') };
  }
  return { tier: 'SO', segment: parts.slice(1).join('_') };
}

function deriveOriginTiers(boostTypeSchedules, defaultSchedule) {
  const boostsDir = path.join(EXPORT_ROOT, 'boosts');
  const grid = { TO: {}, DO: {}, SO: {} }; // tier -> schedule -> { scale, from, pieces }

  for (const dir of fs.readdirSync(boostsDir).sort()) {
    const parsed = originTierOf(dir);
    if (!parsed) continue;
    const { tier, segment } = parsed;
    if (EXCLUDED_ORIGIN_SEGMENTS.has(segment)) continue;
    if (tier === 'TO' && PRESTIGE_PIECE_SEGMENTS.has(segment)) continue;
    const boostType = ORIGIN_SEGMENT_BOOST_TYPE[segment];
    if (boostType === undefined) {
      throw new Error(`${dir}: unknown origin aspect segment "${segment}" — extend ORIGIN_SEGMENT_BOOST_TYPE deliberately`);
    }
    const file = path.join(boostsDir, dir, `${dir}.json`);
    if (!fs.existsSync(file)) continue;
    const piece = readJson(file);

    const scales = new Set();
    for (const effect of piece.effects || []) {
      for (const t of effect.templates || []) {
        // Enhancement-magnitude templates only: Ones-table Strength rows that
        // aren't proc riders. Thunderspy carries no flags and pads with
        // attribs=[Ones] filler rows — both handled here (SW1 §4 census).
        if (t.aspect !== 'Strength') continue;
        if (t.flags !== null && t.flags !== undefined && !t.flags.includes('Boost')) continue;
        if (t.table !== 'Melee_Ones') continue;
        if (t.scale === null || t.scale === undefined) continue;
        if (JSON.stringify(t.attribs) === '["Ones"]') continue;
        scales.add(authored(t.scale));
      }
    }
    if (scales.size === 0) continue;
    if (scales.size > 1) {
      throw new Error(`${dir}: origin piece carries multiple enhancement scales ${[...scales].sort()}`);
    }
    const scale = scales.values().next().value;
    const schedule = boostType === null ? defaultSchedule : (boostTypeSchedules[boostType] ?? defaultSchedule);
    const cell = grid[tier][schedule];
    if (!cell) {
      grid[tier][schedule] = { scale, from: dir, pieces: 1 };
    } else if (cell.scale !== scale) {
      throw new Error(`Origin tier ${tier} schedule ${schedule}: ${dir} scale ${scale} != ${cell.from} scale ${cell.scale} — tier value is not schedule-uniform`);
    } else {
      cell.pieces += 1;
    }
  }

  const letters = ['A', 'B', 'C', 'D'];
  const originTiers = {};
  for (const tier of ['TO', 'DO', 'SO']) {
    originTiers[tier] = {};
    for (const letter of letters) {
      const cell = grid[tier][letter];
      if (!cell) throw new Error(`Origin tier ${tier} has no schedule-${letter} pieces — grid incomplete`);
      originTiers[tier][letter] = cell.scale;
    }
    console.log(`  origin tier ${tier}: ${letters.map((l) => `${l}=${originTiers[tier][l]}`).join(' ')} (${letters.reduce((n, l) => n + grid[tier][l].pieces, 0)} pieces)`);
  }
  return originTiers;
}

// ---------------------------------------------------------------------------
// Emit
// ---------------------------------------------------------------------------

/**
 * Exemplar handicap curves (boost.c boost_HandicapExemplar). The export keys
 * follow the ExemplarHandicaps struct fields; `handicaps` is the authored
 * `Weights` token (pfHandicaps), renamed back to `weights` for the module.
 * boost.c tolerates empty PreClamp/PostClamp (the clamp is skipped) but bails
 * out of scaling entirely on empty Limits/Weights — that would be a real data
 * change, so it fails loud here.
 */
function readExemplarHandicaps() {
  const curves = readJson(path.join(EXPORT_ROOT, 'exemplar_handicaps.json')).curves;
  const out = {
    limits: curves.limits,
    weights: curves.handicaps,
    preClamp: curves.pre_clamp,
    postClamp: curves.post_clamp,
  };
  for (const [name, values] of Object.entries(out)) {
    if (!Array.isArray(values) || !values.every((v) => Number.isFinite(v))) {
      throw new Error(`exemplar_handicaps ${name} is not a finite number array`);
    }
  }
  if (out.limits.length === 0 || out.weights.length === 0) {
    throw new Error('exemplar_handicaps limits/weights empty — the game would skip exemplar scaling entirely');
  }
  return out;
}

function main() {
  console.log(`Converting enhancement curves for ${datasetId}...`);
  const curves = readJson(path.join(EXPORT_ROOT, 'enhancement_curves.json'));
  const { edThresholds, boostTypeSchedules, defaultSchedule, tierEffectiveness, attribFamily } =
    deriveSchedules(curves);
  const strengthCurves = deriveStrengthCurves();
  const multiAspectScale = craftedCensus(attribFamily, defaultSchedule);
  const originTiers = deriveOriginTiers(boostTypeSchedules, defaultSchedule);
  const boostEffectiveness = curves.boost_effectiveness;
  const exemplarHandicaps = readExemplarHandicaps();

  // FIELD GLOSSARY — lifted verbatim from the doc comments on the `EnhancementCurvesData`
  // interface this converter used to emit alongside the values. The interface went with
  // the TypeScript; what it knew about each field did not.
  //
  //   schedules[letter].edThresholds       ED tier boundaries (t1, t2, t3) as pre-ED
  //                                        enhancement fractions.
  //   schedules[letter].sourceTable        Named class-modifier table the strength curve
  //                                        was read from.
  //   schedules[letter].strengthByBoostLevel
  //                                        Enhancement strength of one boost, indexed
  //                                        strengthByBoostLevel[boostLevel - 1].
  //   tierEffectiveness                    ED effectiveness of the value beyond each
  //                                        threshold (t1-t2, t2-t3, past t3).
  //   boostTypeSchedules                   dim_returns boost-type vocabulary -> its ED
  //                                        schedule.
  //   defaultSchedule                      Schedule for every boost type not listed in
  //                                        boostTypeSchedules (the dim_returns default).
  //   multiAspectScale                     Standard crafted-piece per-aspect scale,
  //                                        indexed by aspect count - 1 (1..4 aspects).
  //   originTiers                          TO/DO/SO enhancement fraction per ED schedule
  //                                        (the origin families sit on flat Ones tables).
  //   boostEffectiveness                   Relative-level attenuation (above/below) and
  //                                        +boost combine curves.
  //   exemplarHandicaps                    Exemplar magnitude-handicap curves
  //                                        (exemplar_handicaps.bin), applied by boost.c
  //                                        boost_HandicapExemplar IN THIS ORDER: clamp to
  //                                        preClamp, scale by weights[combat]/weights[
  //                                        trueLevel] when the magnitude reaches limits,
  //                                        then clamp to postClamp. Index = 1-based level
  //                                        - 1, top-clamped to the curve.
  //
  // The four schedule letters are A/B/C/D and the three origin tiers TO/DO/SO; both
  // vocabularies were TypeScript union types (`EnhancementSchedule`, `OriginTier`) that
  // the Rust reader in crates/coh_data/src/enhancement_curves.rs now owns.
  const letters = Object.keys(edThresholds).sort();
  const data = {
    dataset: datasetId,
    schedules: Object.fromEntries(letters.map((letter) => [letter, {
      edThresholds: edThresholds[letter],
      sourceTable: TABLE_FOR_SCHEDULE[letter],
      strengthByBoostLevel: strengthCurves[letter],
    }])),
    tierEffectiveness,
    // Sorted so a reader can find a boost type by eye; the vocabulary is ~30 entries
    // and nothing downstream reads it by position.
    boostTypeSchedules: Object.fromEntries(
      Object.keys(boostTypeSchedules).sort().map((bt) => [bt, boostTypeSchedules[bt]]),
    ),
    defaultSchedule,
    multiAspectScale,
    originTiers: Object.fromEntries(['TO', 'DO', 'SO'].map((tier) => [
      tier,
      Object.fromEntries(['A', 'B', 'C', 'D'].map((l) => [l, originTiers[tier][l]])),
    ])),
    // Named explicitly, not spread: `boost_effectiveness` is raw export JSON and may
    // carry fields the engine has no reader for. Three curves is what the contract
    // section is, and a fourth appearing silently is not a thing to ship.
    boostEffectiveness: {
      above: boostEffectiveness.above,
      below: boostEffectiveness.below,
      boosters: boostEffectiveness.boosters,
    },
    exemplarHandicaps,
  };

  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, JSON.stringify({ ENHANCEMENT_CURVES: data }), 'utf-8');
  console.log(`  wrote ${path.relative(REPO_ROOT, OUTPUT_PATH)} (schedules ${letters.join('')}, default ${defaultSchedule})`);
}

main();
