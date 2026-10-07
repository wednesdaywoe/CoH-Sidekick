/**
 * Contract emitter — the JSON dataset contract the Rust side consumes.
 *
 * Emits the COMPOSED (post-override) surfaces, so Rust never ports withOverrides.
 *
 * Powers ship in the modern Power shape: a Power object with tuple `atoms` and a `stats` block.
 * The transitional `effects` bag is GONE from every partition as of 2026-09-03 — the powerset
 * tree lost it at atom1-13, and the pool and inherent converters, the last two writers, stopped
 * emitting it with the writer-side strip (REBUILD-PROGRESS; `scripts/keys/effects-bag-survivors.py`
 * is the standing check, and it counts converters that assign `power.effects` as well as bags
 * that survive).
 *
 * What has NOT been normalized is identity and the legacy execution KEYS: a pool or epic power
 * still carries its identity only in `fullName`, and `coh_data`'s `normalize_legacy_power` still
 * fills `enduranceCost` / `castTime` from the older spellings on the way in (PROD6C-3h). Both of
 * those arms are now no-ops in practice — every partition power carries the minted `stats` block
 * (`scripts/_power-stats.cjs`), graded against the raw export by
 * the raw export — but they are still what the loader runs, so do
 * not assume the two partitions are byte-identical in shape when reading them out of the bundle.
 *
 * Output (committed; regen-diff-guarded like generated/):
 *   contract/schema-version.json          {"schema":1, "atomTupleFields":[...]}
 *   contract/<dataset>/manifest.json      counts + shard index
 *   contract/<dataset>/powersets/<archetype>/<category>/<slug>.json
 *   contract/<dataset>/power-pools.json · epic-pools.json · at-tables.json
 *     · archetypes.json · archetype-stats.json · enhancement-curves.json
 *     · io-sets.json · pet-entities.json · incarnate.json · proc-data.json
 *     · enhancements.json · levels.json
 *   contract/<dataset>/bundle.json.gz     one gzip of everything — the runtime artifact
 *
 * Determinism (§8): no timestamps; JSON.stringify preserves the deterministic source
 * order; node's gzip writes mtime=0. `regen → emit → git diff --exit-code` is the guard.
 *
 * Usage: node scripts/emit-contract.cjs [--dataset homecoming]
 */

const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
// Every input is JSON now: `pipeline/*.json` for anything a converter derives, plus the two
// committed roots nothing derives — `hand-data/` and `mids-tables/`, read where they sit. This
// file used to `require('tsx/cjs')` and pull TypeScript out of `src/data`, which put the oracle
// inside the supply line that feeds the shipped app. Nothing reads `src/` any more.
const {
  REPO, DATASETS, loadComposed, datasetJson, sharedJson, handJson, midsJson,
} = require('./collect-composed-powers.cjs');
const { ATOM_TUPLE_FIELDS } = sharedJson('atom-tuple-fields');

const argv = process.argv.slice(2);
const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
const TARGETS = picked.length ? picked : DATASETS;

const CONTRACT_ROOT = path.join(REPO, 'contract');

// THE `dataOnly` DEEP-COPY STOOD HERE, and it was deleted on 2026-09-26. It walked every
// section before writing it and dropped function-valued and undefined-valued keys, turning
// undefined array elements into null. It was needed while this emitter `require()`d TypeScript
// modules out of `src/data` through `tsx`: a module export could be a getter or a function, and
// those had to come off before serialization. Every input is a `JSON.parse` now -- `datasetJson`,
// `handJson`, `sharedJson`, `midsJson` and the two `readFileSync` calls in the incarnate builders
// are the whole list -- and parsed JSON cannot hold either one. Instrumented over all four
// datasets before removal, it stripped 0 functions and 16 undefined values, and `JSON.stringify`
// drops exactly those 16 by itself: an undefined-valued key is omitted, an undefined array
// element becomes null. So it reimplemented the serializer's own rules one step early, over
// 22 MB per dataset. Recoverable from `git show 9ee639aae:scripts/emit-contract.cjs`.

function writeJson(rel, value) {
  const p = path.join(CONTRACT_ROOT, rel);
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, JSON.stringify(value, null, 1) + '\n');
  return value;
}

const slugify = (s) => String(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');

// The seven incarnate slots, in the beta's INCARNATE_SLOT_ORDER (picker tab order).
const INCARNATE_SLOTS = ['alpha', 'judgement', 'interface', 'destiny', 'lore', 'hybrid', 'genesis'];

// Read the dataset's own `exported_powers/<ds>/incarnate/<slot>/index.json` files into
// the catalog section. Emission is raw index data only — tree/tier/branch structure is
// name-shape interpretation and lives in the Rust reader, one derivation site.
function buildIncarnateCatalog(dataset) {
  const exportBase = path.join(REPO, 'exported_powers');
  // HC ships at the legacy flat layout; other datasets are namespaced under
  // `exported_powers/<id>/` (same convention as convert-enhancement-curves.cjs).
  const exportRoot = dataset === 'homecoming' && !fs.existsSync(path.join(exportBase, dataset, 'incarnate'))
    ? exportBase
    : path.join(exportBase, dataset);

  const slots = [];
  for (const slot of INCARNATE_SLOTS) {
    const indexPath = path.join(exportRoot, 'incarnate', slot, 'index.json');
    // A slot the export doesn't carry is data (a fork without that slot), not an
    // error — the catalog mirrors the export; absence means the slot is never offered.
    if (!fs.existsSync(indexPath)) continue;
    const index = JSON.parse(fs.readFileSync(indexPath, 'utf8'));
    slots.push({
      id: slot,
      key: index.key,
      displayName: index.display_name,
      icon: index.icon,
      powers: index.powers.map((fullName, i) => ({
        fullName,
        internalName: fullName.split('.').pop(),
        displayName: index.power_display_names[i],
        shortHelp: (index.power_short_helps && index.power_short_helps[i]) || '',
      })),
    });
  }
  if (!slots.length) {
    throw new Error(`${dataset}: no incarnate slot indices found under ${exportRoot}/incarnate`);
  }
  return { slots };
}

// Which crafting family a recipe belongs to, read off its authored source file.
// Homecoming splits the three families across def files; the forks author only
// the current one. An unknown basename is a new family and must stop the emit.
const CRAFT_FAMILY_BY_SOURCE = {
  'INCARNATE.RECIPE': 'current',
  'INCARNATE_PVP.RECIPE': 'pvp',
  'INCARNATE_ALPHA.RECIPE': 'legacyShard',
};

// The incarnate crafting section: every craft recipe of the dataset (salvage +
// prerequisite POWERS consumed, ability granted) plus the salvage catalog those
// recipes reference, priced from the dataset's own Conversion-tab store rows.
// Read from `exported_powers[/<ds>]/incarnate-recipes.json` (baserecipes.bin via
// export_incarnate_recipes.py). Selection of WHICH recipe a planner surface
// shows (family, T4 pair variant) is a Rust-side decision — everything ships.
//
// Salvage identity/display/rarity comes from HC's salvage.json for every
// dataset: salvage.bin is HC-only and the S_* TOK_LINK ids are shared
// identities (the forks' recipes reference the same items, case-insensitively —
// the bins spell `S_FavoroftheWell` where the catalog has `S_FavorOfTheWell`).
// Buy prices are per-dataset: a Conversion row on a
// `Conversion|<currency>|<rarity>|Component` tab whose single input IS the
// currency the tab names prices the salvage its icon points at. Multi-input
// conversion routes (the forks' thread-ladder rare/very-rare rows) are not
// single prices and are deliberately not flattened into one.
function buildIncarnateCrafting(dataset) {
  const exportBase = path.join(REPO, 'exported_powers');
  const exportRoot = dataset === 'homecoming' ? exportBase : path.join(exportBase, dataset);
  const { recipes } = JSON.parse(
    fs.readFileSync(path.join(exportRoot, 'incarnate-recipes.json'), 'utf8'));
  const catalog = JSON.parse(
    fs.readFileSync(path.join(exportBase, 'salvage.json'), 'utf8')).salvage;
  const catalogByLowerName = new Map(catalog.map((s) => [s.name.toLowerCase(), s]));
  const catalogByIcon = new Map(catalog.map((s) => [s.icon, s]));

  // Only Homecoming's def layout names the families (three source files); the
  // forks merged everything into one file — and went further, authoring shard
  // legacy variants for every slot where HC only kept Alpha's. Fork stamping is
  // therefore two data-to-data joins against HC's own split, in order:
  //   1. the Homecoming NAME-twin's family (the shared authored identity —
  //      'Cardiac_Boost_RecipeAlpha' exists on all three, byte-equivalent);
  //   2. else, the shard-era SALVAGE BAND: the items HC's legacy family
  //      consumes and its current family never does. A fork-only recipe
  //      consuming any of them is the shard path ('Cryonic_Judgement_RecipeAlpha');
  //      one consuming none (Rebirth's Genesis trees) is current.
  // The Rust corpus gate cross-checks the result by asserting one current
  // recipe per non-Very-Rare reward — a misclassified legacy recipe lands as a
  // second `current` there and reds.
  const hcFamilyByName = new Map();
  const legacyBandSalvage = new Set();
  if (dataset !== 'homecoming') {
    const hc = JSON.parse(
      fs.readFileSync(path.join(exportBase, 'incarnate-recipes.json'), 'utf8')).recipes;
    const currentBand = new Set();
    for (const r of hc) {
      if (!r.incarnate_reward) continue;
      const family = CRAFT_FAMILY_BY_SOURCE[r.source_file.split('/').pop()];
      hcFamilyByName.set(r.name, family);
      for (const c of r.salvage) {
        (family === 'current' ? currentBand : legacyBandSalvage).add(c.name.toLowerCase());
      }
    }
    for (const id of currentBand) legacyBandSalvage.delete(id);
  }

  const usedSalvage = new Map();
  const resolveSalvage = (recipeName, ref) => {
    const hit = catalogByLowerName.get(ref.toLowerCase());
    if (!hit) {
      throw new Error(`${dataset}: recipe ${recipeName} references salvage ${ref} `
        + 'that salvage.bin does not carry');
    }
    usedSalvage.set(hit.name, hit);
    return hit;
  };

  const craftRecipes = recipes
    .filter((r) => r.incarnate_reward)
    .map((r) => {
      const basename = r.source_file.split('/').pop();
      const family = dataset === 'homecoming'
        ? CRAFT_FAMILY_BY_SOURCE[basename]
        : (hcFamilyByName.get(r.name)
          ?? (r.salvage.some((c) => legacyBandSalvage.has(c.name.toLowerCase()))
            ? 'legacyShard' : 'current'));
      if (!family) {
        throw new Error(`${dataset}: recipe ${r.name} comes from unmapped source `
          + `${r.source_file} — new crafting family?`);
      }
      return {
        name: r.name,
        family,
        rewardPower: r.incarnate_reward,
        tab: r.display_tab_name_resolved,
        salvage: r.salvage.map((c) => ({
          id: resolveSalvage(r.name, c.name).name,
          amount: c.amount,
        })),
        powerComponents: r.power_components.map((c) => ({
          fullName: c.name,
          amount: c.amount,
        })),
      };
    });
  if (!craftRecipes.length) {
    throw new Error(`${dataset}: incarnate-recipes.json holds no craft recipes`);
  }

  const buyBySalvage = new Map();
  for (const r of recipes) {
    if (r.incarnate_reward) continue;
    const segs = (r.display_tab_name_resolved || '').split('|');
    if (segs.length !== 4 || segs[0] !== 'Conversion' || segs[3] !== 'Component') continue;
    if (r.salvage.length !== 1) continue;
    const input = catalogByLowerName.get(r.salvage[0].name.toLowerCase());
    if (!input || input.display_name !== segs[1]) continue;
    const granted = catalogByIcon.get(r.icon);
    if (!granted) {
      throw new Error(`${dataset}: conversion recipe ${r.name} prices no `
        + `icon-resolvable salvage (icon ${r.icon})`);
    }
    const options = buyBySalvage.get(granted.name) ?? [];
    options.push({ currency: input.display_name, amount: r.salvage[0].amount });
    buyBySalvage.set(granted.name, options);
  }

  const salvage = [...usedSalvage.values()]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((s) => ({
      id: s.name,
      displayName: s.display_name,
      rarity: s.rarity,
      icon: s.icon,
      buy: (buyBySalvage.get(s.name) ?? [])
        .sort((a, b) => a.currency.localeCompare(b.currency) || a.amount - b.amount),
    }));

  return { recipes: craftRecipes, salvage };
}

/**
 * The four inherent Fitness powers — Swift, Hurdle, Health, Stamina.
 *
 * Authored gating (`available`, `maxSlots`, allowed enhancements, `isLocked`) comes from
 * `hand-data/levels.json`; the calc payload is spliced off the `fitness` pool, matched on
 * `internalName`. `levels.ts` did this with a spread, which is why the payload keys land LAST
 * in each power, and why `effects` is missing from the contract: the atomized pool powers carry
 * no such bag, so the key resolved to undefined and was stripped. `JSON.stringify` omits an
 * undefined-valued key, so it stays stripped with no help from the emitter. A hand-transcribed scale table is what dropped Health's `MezResist(Sleep)`
 * and had the planner reporting no Sleep resistance at all (INHERENT-1).
 *
 * `poolDataset` is THIS fork, which it was not until 2026-09-25: `levels.ts` imported
 * Homecoming's pool, being the only fork with a `levels.ts`, so every bundle shipped
 * Homecoming's four powers. Rebirth and Thunderspy tag these atoms with the scale table they
 * read (`SpeedRunning`, `Leap`, `Ones`) where Homecoming tags nothing, so the contract moved on
 * those two forks. No number moved: the scales already agreed, and the only atom tag anything
 * computes from is `Containment` (`coh_math/src/window_slots.rs`, plus damage grouping, which
 * needs a damage atom and these four have none).
 */
function fitnessInherents(authored, poolDataset) {
  const pool = new Map(
    datasetJson(poolDataset, 'power-pools-raw').POWER_POOLS_RAW.fitness.powers
      .map((power) => [power.name, power]),
  );
  return authored.map((power) => {
    const from = pool.get(power.internalName);
    if (!from) {
      throw new Error(
        `INHERENT_FITNESS_POWERS: no ${poolDataset} Fitness pool power named ${power.internalName}`,
      );
    }
    return {
      ...power,
      atoms: from.atoms,
      effects: from.effects,
      stats: from.stats,
      effectArea: from.effectArea,
    };
  });
}

/** An authored `hand-data/` table without its prose keys (`_comment`, `_groups`, `_notes`). */
function authoredTable(name) {
  return Object.fromEntries(
    Object.entries(handJson(name)).filter(([key]) => !key.startsWith('_')),
  );
}

/**
 * The contract's `proc-data` section: the authored proc table with eight side tables stapled
 * onto its entries.
 *
 * This was eight `for` loops at the foot of `src/data/proc-data.ts`, resolved when the module
 * was imported, and `emit-pipeline-json.cjs` `require()`d the module to get the result. It was
 * the LAST oracle read in the pipeline. Ported here 2026-09-25, loop for loop and in the same
 * order, which is what keeps the bytes identical — and the order is load-bearing twice over:
 *
 * - The three `effects` tables each REPLACE an entry's whole array, so a later one wins
 *   outright. Residual is last of the four, which is how a hand transcription overrides a
 *   derived one for the procs the generator cannot reach.
 * - `effects`, `activatePeriod` and `boostsAllowed` are absent from the authored entry, so each
 *   merge APPENDS its key the first time it writes one, and the append order is the field order
 *   that lands in the contract. `ppm` is authored, so it is overwritten in place and keeps its
 *   position. Swapping two merges here would move shipped bytes with no data change.
 *
 * The variable-control overlay must stay last: it is the only ADDITIVE one, patching fields onto
 * the effects the earlier merges left, and only onto the effects whose `category` matches.
 *
 * One shared table across all four forks, so it is read once, outside `emitDataset`. The
 * authored base is read from `hand-data/` directly; the six derived tables come out of
 * `pipeline/_shared/`, written by `scripts/extract-proc-data.py`.
 */
function procDatabase() {
  const db = authoredTable('proc-data');
  const derived = (name, konst) => sharedJson(name)[konst];

  for (const table of [derived('proc-globals', 'PROC_GLOBAL_EFFECTS'),
                       derived('proc-damage', 'PROC_DAMAGE_EFFECTS'),
                       derived('proc-effects', 'PROC_OTHER_EFFECTS')]) {
    for (const [key, effects] of Object.entries(table)) {
      if (db[key]) db[key].effects = effects;
    }
  }
  for (const [key, ppm] of Object.entries(derived('proc-ppm', 'PROC_PPM'))) {
    if (db[key]) db[key].ppm = ppm;
  }
  for (const [key, period] of Object.entries(derived('proc-activate-period', 'PROC_ACTIVATE_PERIOD'))) {
    if (db[key]) db[key].activatePeriod = period;
  }
  for (const [key, boosts] of Object.entries(derived('proc-boosts-allowed', 'PROC_BOOSTS_ALLOWED'))) {
    if (db[key]) db[key].boostsAllowed = boosts;
  }
  for (const [key, effects] of Object.entries(authoredTable('proc-residual-effects'))) {
    if (db[key]) db[key].effects = effects;
  }
  for (const [key, control] of Object.entries(authoredTable('proc-variable-controls'))) {
    const entry = db[key];
    if (!entry?.effects) continue;
    for (const eff of entry.effects) {
      if (eff.category !== control.category) continue;
      if (control.maxStacks !== undefined) eff.maxStacks = control.maxStacks;
      if (control.valueMax !== undefined) eff.valueMax = control.valueMax;
      if (control.scaleTable !== undefined) eff.scaleTable = control.scaleTable;
    }
  }
  return db;
}

const PROC_DATABASE = procDatabase();

function emitDataset(dataset) {
  const { powersets: composedPowersets, pools, epics } = loadComposed(dataset);

  // Merge the archetype-inherent powerset (Vigilance, Containment, …) extracted by
  // convert-inherents.cjs. Deliberately merged HERE, not into loadComposed's
  // MODULAR_POWERSETS: the contract bundle is Rust-consumed, and the archetype
  // inherents must reach the Rust calc (Pass 3 derives Vigilance from these atoms),
  // but they must NOT reach the TS oracle/totals fixtures — the beta computes
  // archetype-inherent damage from a combat option, never the inherent power's
  // presence, so adding it there would change nothing but broaden the diff. Keeping
  // it emitter-local means the fixture pipeline is untouched.
  // A missing or empty inherents module is a HARD failure, not a warning: the bundle
  // would ship without Vigilance/Fury data and Pass 3 would fail-loud on every
  // Defender/Brute build — a broken pipeline must stop here, at regen time.
  const powersets = { ...composedPowersets };
  const { INHERENT_POWERSET } = datasetJson(dataset, 'inherents');
  if (!INHERENT_POWERSET || !INHERENT_POWERSET.id) {
    throw new Error(`${dataset}: pipeline/${dataset}/inherents.json has no INHERENT_POWERSET with an id`);
  }
  powersets[INHERENT_POWERSET.id] = INHERENT_POWERSET;

  // Merge the Accolades powerset (Temporary_Powers.Accolades) the same emitter-local way as
  // the inherents above: it must reach the Rust-consumed contract (a selected accolade's
  // +MaxHP/+MaxEnd atoms are read from here), but NOT the TS oracle/totals fixtures — the TS
  // engine derives accolade toggles straight from the accolades section, so adding it to
  // MODULAR_POWERSETS would only broaden the diff. A missing module is a broken pipeline: stop.
  const { ACCOLADES_POWERSET } = datasetJson(dataset, 'accolades');
  if (!ACCOLADES_POWERSET || !ACCOLADES_POWERSET.id) {
    throw new Error(`${dataset}: pipeline/${dataset}/accolades.json has no ACCOLADES_POWERSET with an id`);
  }
  powersets[ACCOLADES_POWERSET.id] = ACCOLADES_POWERSET;

  // per-powerset shards
  let powerCount = 0;
  let atomCount = 0;
  const shardIndex = [];
  for (const [psId, ps] of Object.entries(powersets)) {
    const archetype = slugify(ps.archetype || psId.split('/')[0] || 'misc');
    const category = slugify(ps.category || 'other');
    const slug = slugify(psId.split('/').pop() || ps.name);
    const rel = path.join(dataset, 'powersets', archetype, category, `${slug}.json`);
    writeJson(rel, ps);
    shardIndex.push({ id: psId, path: `powersets/${archetype}/${category}/${slug}.json` });
    for (const p of ps.powers || []) {
      powerCount += 1;
      atomCount += Array.isArray(p.atoms) ? p.atoms.length : 0;
    }
  }

  const sections = {
    'power-pools': pools,
    'epic-pools': epics,
  };
  for (const agg of Object.values(sections)) {
    // Count pool/epic powers via the same Power-shape rule as the walker. This count is what
    // `coh_data`'s verify_counts reconciles against, so the rule here and `collect_into`'s
    // must be the same one: the bag came off both on 2026-09-03, and a node carrying a bag
    // and no atoms is an error on the Rust side rather than a power neither side counts.
    const stack = [agg];
    const seen = new Set();
    while (stack.length) {
      const n = stack.pop();
      if (!n || typeof n !== 'object' || seen.has(n)) continue;
      seen.add(n);
      if (typeof n.name === 'string' && Array.isArray(n.atoms)) {
        powerCount += 1;
        atomCount += n.atoms.length;
      }
      for (const v of Object.values(n)) if (v && typeof v === 'object') stack.push(v);
    }
  }

  const atTables = datasetJson(dataset, 'at-tables');
  sections['at-tables'] = { archetypes: atTables.AT_TABLES, pets: atTables.PET_TABLES };

  // Purple patch (combat level-difference scaling), straight off `hand-data/purple-patch.json`.
  // The app reaches these tables through lookup FUNCTIONS, so the dump used to sample them at
  // regen time; that sampling recovered the authored tables entry for entry, so the authored
  // file IS the section and the sampling is gone. One shared file — its `_comment` carries the
  // signed-`levelDiff` convention and what a fork that retunes the tables does. Pass 8 reads
  // the section back through coh_math lookups.
  const purplePatch = handJson('purple-patch');
  delete purplePatch._comment;
  sections['purple-patch'] = purplePatch;

  const ats = datasetJson(dataset, 'archetypes');
  sections['archetypes'] = {
    archetypes: ats.ARCHETYPES,
    epicArchetypeIds: ats.EPIC_ARCHETYPE_IDS || [],
    standardArchetypeIds: ats.STANDARD_ARCHETYPE_IDS || [],
  };
  sections['archetype-stats'] = datasetJson(dataset, 'archetype-stats').ARCHETYPE_BINARY_STATS;
  // Binary-derived enhancement curves (SOURCE-1 SW4): ED thresholds, schedule
  // assignment, per-boost-level strength, multi-aspect ladder, boost effectiveness.
  // Sourced from the generated module — the same bytes the TS engine consumes —
  // which is staleness-guarded against the export both
  // directions. A missing module or dataset mismatch is a broken pipeline: stop.
  const enhancementCurves = datasetJson(dataset, 'enhancement-curves').ENHANCEMENT_CURVES;
  if (!enhancementCurves || enhancementCurves.dataset !== dataset) {
    throw new Error(`${dataset}: pipeline/${dataset}/enhancement-curves.json missing or wrong dataset id`);
  }
  sections['enhancement-curves'] = enhancementCurves;
  sections['io-sets'] = datasetJson(dataset, 'io-sets-raw').IO_SETS_RAW;

  // The join from the name the game client prints for an enhancement to the
  // section that describes it (game-client import / export).
  const boostIndex = datasetJson(dataset, 'boost-index').BOOST_INDEX;
  if (!boostIndex || boostIndex.dataset !== dataset) {
    throw new Error(`${dataset}: pipeline/${dataset}/boost-index.json missing or wrong dataset id`);
  }
  sections['boost-index'] = boostIndex.entries;
  sections['pet-entities'] = {
    entities: datasetJson(dataset, 'pet-entities').PET_ENTITIES,
    lifespans: datasetJson(dataset, 'pet-lifespans'),
    selfDestructDelays: datasetJson(dataset, 'self-destruct-delays'),
  };
  // GENESIS-1 / INCARNATE-1: Genesis is a Rebirth-only slot. Homecoming and
  // Thunderspy export a byte-shaped but DORMANT genesis table (placeholder help,
  // reused Interface icons, no exemplar-grant linkage) the beta runtime never
  // serves — `genesisEffectsRegistry` = `_pick3(_genesisEmpty, REBIRTH_GENESIS,
  // _genesisEmpty)`. Mirror that dormancy in the Rust-consumed contract so a
  // genesis slot force-equipped off-Rebirth finds the same empty table the beta
  // does, not dormant data the runtime suppresses.
  const incarnate = datasetJson(dataset, 'incarnate-effects');
  if (dataset !== 'rebirth') incarnate.GENERATED_GENESIS_EFFECTS = {};
  sections['incarnate'] = incarnate;
  // Incarnate catalog — the per-slot pick lists the incarnate picker renders (slot
  // display name/icon + each power's identity/display/help), read from THIS dataset's
  // own binary-export slot indices rather than the beta's vendored HC-shaped copies
  // (`src/data/incarnate-indices/`, shared across datasets). The catalog carries every
  // slot the export does — all three forks ship a genesis powerset — and the reader
  // gates what is OFFERED on the matching effects table above being non-empty, which
  // is where the GENESIS-1 dormancy already lives (HC/Thunderspy genesis powers exist
  // but their effect tables are empty).
  sections['incarnate-catalog'] = buildIncarnateCatalog(dataset);
  // Incarnate crafting — the recipes behind the catalog's craft ladder (salvage
  // and prerequisite powers consumed per ability) plus the priced salvage
  // catalog. Binary-sourced from baserecipes.bin; see buildIncarnateCrafting.
  sections['incarnate-crafting'] = buildIncarnateCrafting(dataset);
  // Leveling schedule (WS17): the level-gated enhancement-slot and power-pick budget, derived
  // from schedules.bin (AssignableBoost / Power) — the sourced replacement for the
  // hand-authored SLOT_GRANTS/POWER_PICK_LEVELS in the `levels` section below. Same
  // generated-module pattern as enhancement-curves: the bytes the TS engine consumes. Read and
  // checked up here because `levels` takes seven of its fields; ASSIGNED below, in the place it
  // has always held, because `sections` insertion order is the bundle's and the manifest's.
  const levelingSchedule = datasetJson(dataset, 'leveling-schedule').LEVELING_SCHEDULE;
  if (!levelingSchedule || levelingSchedule.dataset !== dataset) {
    throw new Error(`${dataset}: pipeline/${dataset}/leveling-schedule.json missing or wrong dataset id`);
  }
  // The `levels` section, built here since 2026-09-25 — the last thing `levels.ts` was read
  // for. Keys are ALPHABETICAL because that is the order esbuild emitted the module's exports
  // in, so it is the order the contract's bytes are in. A comment here used to call it
  // levels.ts's declaration order; it never was.
  //
  // Three inputs, ALL of them this fork's own. The six constants with no upstream sit in the one
  // shared `hand-data/levels.json` — no fork authors its own. The two inherent lists come off
  // `basic-inherents.json`, split on the `category` the converter curates. The seven schedule
  // constants come off `leveling-schedule.json`.
  //
  // Eight of these keys were HOMECOMING's on every fork until 2026-09-25, because `levels.ts`
  // was Homecoming's and read Homecoming's imports: the seven schedule constants took the
  // SHARED schedule by design, leaving the per-fork values to getters no contract carried, and
  // `INHERENT_FITNESS_POWERS` took Homecoming's pool. Thunderspy's 71 slots, 5 pools and
  // level-1 pool unlock now reach the contract. Nothing regressed when they did not: Rust reads
  // the per-fork `leveling-schedule` section for all seven, and `LevelsSection` in
  // `coh_data/src/inherent_grants.rs` deserializes only the three power lists out of here.
  const handLevels = handJson('levels');
  const { BASIC_INHERENTS } = datasetJson(dataset, 'basic-inherents');
  sections['levels'] = {
    BASIC_INHERENT_POWERS: BASIC_INHERENTS.filter((p) => p.category === 'basic'),
    ENHANCEMENT_AVAILABILITY: handLevels.ENHANCEMENT_AVAILABILITY,
    EPIC_POOL_LEVEL: levelingSchedule.epicPoolLevel,
    EPIC_TIER_REQUIREMENTS: handLevels.EPIC_TIER_REQUIREMENTS,
    INCARNATE_LEVEL: handLevels.INCARNATE_LEVEL,
    INCARNATE_SLOTS: handLevels.INCARNATE_SLOTS,
    INHERENT_FITNESS_POWERS: fitnessInherents(handLevels.INHERENT_FITNESS_POWERS, dataset),
    MAX_LEVEL: handLevels.MAX_LEVEL,
    MAX_POWER_PICKS: levelingSchedule.maxPowerPicks,
    MAX_POWER_POOLS: levelingSchedule.maxPowerPools,
    MAX_SLOTS_PER_POWER: handLevels.MAX_SLOTS_PER_POWER,
    POOL_UNLOCK_LEVEL: levelingSchedule.poolUnlockLevel,
    POWER_PICK_LEVELS: Object.keys(levelingSchedule.powerPicks).map(Number).sort((a, b) => a - b),
    PRESTIGE_SPRINT_POWERS: BASIC_INHERENTS.filter((p) => p.category === 'prestige'),
    SLOT_GRANTS: levelingSchedule.slotGrants,
    TOTAL_SLOTS_AT_50: levelingSchedule.totalSlots,
  };
  sections['leveling-schedule'] = levelingSchedule;

  // shared (identical across datasets today; per-dataset in the contract so a future
  // per-server divergence needs no schema change)
  sections['proc-data'] = PROC_DATABASE;
  // Special-enhancement registries (SOURCE-1 item 9): per-dataset generated
  // from the boost-piece templates — the same bytes the TS engine consumes.
  const specials = datasetJson(dataset, 'special-enhancements').SPECIAL_ENHANCEMENTS;
  if (!specials || specials.dataset !== dataset) {
    throw new Error(`${dataset}: pipeline/${dataset}/special-enhancements.json missing or wrong dataset id`);
  }
  // The five character origins, straight off `hand-data/origins.json`. Authored, no
  // upstream, identical on every fork — so it is read from the committed root rather than
  // copied through `pipeline/` (see `handJson`).
  const origins = handJson('origins');
  sections['enhancements'] = {
    hamidon: specials.hamidon,
    syntheticHamidon: specials.syntheticHamidon,
    titan: specials.titan,
    hydra: specials.hydra,
    dsync: specials.dsync,
    prestige: specials.prestige,
    // Derived from the crafted boost family, not the frozen hand list — that list
    // was missing Intangible, so the picker offered 25 of the game's 26 common IOs.
    commonIoTypes: boostIndex.commonIoTypes,
    // Origin-tier values live in the enhancement-curves section (SW8 grid).
    origins,
  };

  // Mids Reborn's two namespaces, for the .mbd reader and writer (MBDEXPORT-1,
  // MBDIMPORT-2). Both are Mids' answer about Mids, which is the one question Rule 0
  // cannot send to the export: what MIDS calls a thing. They ride the contract because
  // the Rust reader has to share the tables the TS half already reads — an importer
  // growing its own copy is what shipped a build missing 13 of 63 enhancements.
  //
  // Read from the committed `mids-tables/` directly, through `midsJson`, the way authored
  // data is read through `handJson`. They were copied into `pipeline/` until 2026-09-25,
  // which put a file no build step can produce inside a directory a clean build deletes.
  const midsUids = midsJson(dataset, 'mids-uids').MIDS_UIDS;
  if (!midsUids || !midsUids.ioSetPieces) {
    throw new Error(`${dataset}: mids-tables/${dataset}/mids-uids.json missing MIDS_UIDS`);
  }
  sections['mids-uids'] = midsUids;

  // Mids' third namespace, and the only reader that needs it is the legacy `.mxd`
  // one: that format names an enhancement by Mids' SHORT CODE in its post half and by
  // an array INDEX in its compressed half, and neither is derivable from anything the
  // export owns. Separate from `mids-uids` above because the two are read a different
  // way — that one places a piece by the letter its UID ends in, this one by where it
  // sits in its set's member list — and a gate compares them.
  const midsEnhNames = midsJson(dataset, 'mids-enh-names').MIDS_ENH_NAMES;
  if (!midsEnhNames || !Array.isArray(midsEnhNames.enhancements)) {
    throw new Error(`${dataset}: mids-tables/${dataset}/mids-enh-names.json missing MIDS_ENH_NAMES`);
  }
  sections['mids-enh-names'] = midsEnhNames;

  const midsNames = datasetJson(dataset, 'mids-name-map');
  if (!midsNames.MIDS_NAME_MAP || !midsNames.MIDS_POWERSET_ALIAS) {
    throw new Error(`${dataset}: pipeline/${dataset}/mids-name-map.json missing MIDS_NAME_MAP/MIDS_POWERSET_ALIAS`);
  }
  // Both directions, because the two halves have to read one table or either can rot
  // the other — MBDEXPORT-4 was the import half reading Mids' grade names correctly
  // while the export half wrote ours, and nobody had run the two in sequence.
  sections['mids-names'] = {
    nameMap: midsNames.MIDS_NAME_MAP,
    powersetAlias: midsNames.MIDS_POWERSET_ALIAS,
    nameReverse: midsNames.MIDS_NAME_REVERSE,
    powersetPath: midsNames.MIDS_POWERSET_PATH,
  };

  for (const [name, value] of Object.entries(sections)) {
    writeJson(path.join(dataset, `${name}.json`), value);
  }

  const manifest = writeJson(path.join(dataset, 'manifest.json'), {
    dataset,
    schema: 1,
    counts: { powersets: Object.keys(powersets).length, powers: powerCount, atoms: atomCount },
    shards: shardIndex,
    sections: Object.keys(sections),
  });

  // the runtime bundle: everything in one gzip (one fetch, one parse)
  const bundle = {
    manifest,
    powersets: powersets,
    ...sections,
  };
  const gz = zlib.gzipSync(JSON.stringify(bundle), { level: 9 });
  // Pin the gzip header's OS byte (offset 9) so the bundle is byte-identical across
  // hosts. node's zlib stamps this from its build's OS_CODE — 0x03 (Unix) here, but
  // it varies by platform/zlib build, so a fresh CI host would emit a different byte
  // and `regen → git diff --exit-code` would trip on a purely cosmetic header delta
  // (the committed bundles are normalized to 0x13; gunzip ignores this byte entirely).
  // This replaces a fragile manual post-regen fixup with a deterministic stamp.
  const CANONICAL_GZIP_OS_BYTE = 0x13;
  gz[9] = CANONICAL_GZIP_OS_BYTE;
  fs.writeFileSync(path.join(CONTRACT_ROOT, dataset, 'bundle.json.gz'), gz);
  // The app crate ships the runtime bundle as an asset (embedded on desktop,
  // fetched on web). Copied here so a regen can never leave the app stale.
  const appAsset = path.join(REPO, 'crates/app/assets/contract', dataset);
  fs.mkdirSync(appAsset, { recursive: true });
  fs.writeFileSync(path.join(appAsset, 'bundle.json.gz'), gz);

  console.log(
    `${dataset}: ${manifest.counts.powersets} powersets, ${manifest.counts.powers} powers, ` +
    `${manifest.counts.atoms} atoms, bundle ${(gz.length / 1024 / 1024).toFixed(2)} MB gz`
  );
}

writeJson('schema-version.json', { schema: 1, atomTupleFields: ATOM_TUPLE_FIELDS });
for (const ds of TARGETS) emitDataset(ds);
