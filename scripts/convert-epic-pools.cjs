/**
 * Convert Epic/Patron Pool Powers from raw Homecoming JSON data
 *
 * Reads raw JSON from raw_data_homecoming/powers/epic/ and generates
 * the EPIC_POOLS_RAW object for src/data/epic-pools-raw.ts.
 *
 * Preserves pool-level metadata (archetype, displayName, minLevel) from
 * the existing file, and reconverts power data using the same
 * extractEffects/extractDamage pipeline as archetype powers.
 *
 * Usage:
 *   node scripts/convert-epic-pools.cjs                  # Dry run (preview)
 *   node scripts/convert-epic-pools.cjs --apply          # Write epic-pools-raw.ts
 *   node scripts/convert-epic-pools.cjs --pool blaster_dark_mastery  # One pool only
 *   node scripts/convert-epic-pools.cjs --archetype blaster          # One archetype
 */

const fs = require('fs');
const path = require('path');
const {
  BOOST_TYPE_MAP,
  SET_CATEGORY_MAP,
  extractEffects,
  extractPowerSummon,
  extractDamage,
  extractFormVariants,
  extractGrantEdges,
  extractQuickSnipeData,
  extractSnipeBaseTiming,
  assignModes,
  resolveThunderspyMovementTargets,
  guardThunderspyOnesBuffs,
  guardThunderspyAppliedMez,
  collectTemplatesDeep,
  collectAtomTemplates,
  collectBaseTemplates,
  encodeAtomsForEmit,
  extractConditionalEffects,
  stampConditionalIds,
  detectStackingEffects,
  mergeStackingPatches,
  RAW_DATA_PATH,
  EFFECT_AREA_MAP,
  BIN_BOOST_MAP,
  collectProcRollSites,
  normalizeIconPath,
  TARGET_TYPE_MAP,
  getStrengthsDisallowedIndex,
  _readPowerFile,
} = require('./convert-powerset.cjs');

// Bin export writes epic pool powers under `<RAW_DATA_PATH>/epic/`.
// Old CoD2 layout had an extra `powers/` segment. Probe both.
const RAW_POWERS_PATH = (() => {
  const newLayout = path.join(RAW_DATA_PATH, 'epic');
  const oldLayout = path.join(RAW_DATA_PATH, 'powers', 'epic');
  if (fs.existsSync(newLayout)) return newLayout;
  return oldLayout;
})();
const { parseDatasetArg, dataPath, datasetPath } = require('./_dataset-paths.cjs');
const { displayText, helpText } = require('./_display-text.cjs');
const { gateTokens } = require('./_gate-tokens.cjs');
const { powerStats } = require('./_power-stats.cjs');
const datasetId = parseDatasetArg();

// All datasets write under `src/data/datasets/<id>/`.
const OUTPUT_PATH = datasetPath(datasetId, 'generated', 'epic-pools.ts');
const COMPOSED_PATH = datasetPath(datasetId, 'epic-pools-raw.ts');
const OVERRIDES_PATH = datasetPath(datasetId, 'overrides', 'epic-pools.ts');

// Parse CLI args
const args = process.argv.slice(2);
const applyChanges = args.includes('--apply');
const poolFilterIdx = args.indexOf('--pool');
const poolFilter = poolFilterIdx >= 0 ? args[poolFilterIdx + 1] : null;
const archFilterIdx = args.indexOf('--archetype');
const archFilter = archFilterIdx >= 0 ? args[archFilterIdx + 1] : null;

// ============================================
// LOAD EXISTING DATA (for preserving metadata)
// ============================================

function loadExistingPools() {
  // Try the layered generated/ output first; fall back to the old single-file
  // location so the first run after the layering switch still has access to
  // hand-tagged metadata (archetype, minLevel) that lived in epic-pools-raw.ts.
  for (const candidate of [OUTPUT_PATH, COMPOSED_PATH]) {
    try {
      const content = fs.readFileSync(candidate, 'utf-8');
      const match = content.match(/export\s+const\s+EPIC_POOLS_RAW\s*=\s*(\{[\s\S]*\})\s*(?:as\s+const)?\s*;?\s*$/);
      if (match) {
        return new Function('return ' + match[1])();
      }
    } catch (_) { /* try next */ }
  }
  return {};
}

// Cross-dataset metadata fallback. Rebirth's epic-pool IDs lack archetype
// prefixes for hero ancillary pools (e.g. `arctic_mastery` instead of
// `tanker_arctic_mastery`), which leaves `inferArchetypeFromPoolId` returning
// "" — and once that empty value is written to the generated file, subsequent
// runs preserve it. HC's generated data has all 81 pools correctly tagged
// (tanker for arctic_mastery, blaster for cold_mastery, etc.) and the same
// pool IDs are used in both datasets. Use HC as a seed when the active
// dataset's existing pool either doesn't exist or has no archetype.
function loadFallbackPoolMetadata() {
  if (datasetId === 'homecoming') return {};
  try {
    const hcPath = datasetPath('homecoming', 'generated', 'epic-pools.ts');
    const content = fs.readFileSync(hcPath, 'utf-8');
    const match = content.match(/export\s+const\s+EPIC_POOLS_RAW\s*=\s*(\{[\s\S]*\})\s*(?:as\s+const)?\s*;?\s*$/);
    if (match) return new Function('return ' + match[1])();
  } catch (_) { /* HC file missing; nothing to fall back to */ }
  return {};
}

const FALLBACK_POOLS = loadFallbackPoolMetadata();

// ============================================
// CONVERT A SINGLE EPIC POWER
// ============================================

function convertEpicPower(rawJson, rank, availableLevel) {
  const power = {};

  // Thunderspy movement target-trap: resolve empty movement-template targets from
  // targets_affected before any collector reads them (the five-converters drift — see
  // convert-pool-powers.cjs and [[three-power-converters]]).
  resolveThunderspyMovementTargets(rawJson);

  // Basic metadata. `internalName` is the export's own identity for the power, carried the
  // same way the archetype converter carries it (`internalName: powerJson.name`). Before
  // MBDIMPORT-17 these two emitters wrote only `fullName`, and the Rust loader DERIVED the
  // identity from its last segment — a derivation that agreed on all 1,846 pool and epic
  // powers across four forks, which is why nothing ever broke. Agreement is not the same as
  // reading it: the export owns this field, so it is read rather than reconstructed.
  power.name = displayText(rawJson.display_name) || rawJson.name;
  power.internalName = rawJson.name;
  power.fullName = rawJson.full_name;

  // StrengthsDisallowed from the bin export; GlobalStrengthsDisallowed from the
  // `raw defs/` oracle, the only source for it (HC only — see
  // getStrengthsDisallowedIndex in convert-powerset.cjs). Recharge carriers
  // here: Rune of Protection, Afterburner-class travel boosts.
  if (Array.isArray(rawJson.strengths_disallowed) && rawJson.strengths_disallowed.length) {
    power.strengthsDisallowed = rawJson.strengths_disallowed;
  }
  {
    const sd = rawJson.full_name
      && getStrengthsDisallowedIndex().get(rawJson.full_name.toLowerCase());
    if (sd && sd.global.length) power.globalStrengthsDisallowed = sd.global;
  }

  // ProcMainTargetOnly — procs roll single-target here regardless of radius.
  // See the field doc on `Power.procsOnlyOnMainTarget`.
  if (rawJson.procs_only_on_main_target) power.procsOnlyOnMainTarget = true;

  // ProcAllowed kNone — the game fires no proc in this power. Sparse-false, mirroring
  // the export: absent means procs fire. See the field doc on `Power.procsAllowed`.
  if (rawJson.procs_allowed === false) power.procsAllowed = false;
  // …unless an executed child rolls in its place. No epic power has one today;
  // the call is here so an epic that grows one is not a silent gap.
  const procRollSites = collectProcRollSites(rawJson);
  if (procRollSites) power.procRollSites = procRollSites;

  // EntsAffected — who this power's effects can land on, which is what an atom
  // targeting `AnyAffected` means by "the target". See the field doc on
  // `Power.targetsAffected` (DATA-GAP-REGISTER MEZRES-3).
  if (Array.isArray(rawJson.targets_affected) && rawJson.targets_affected.length) {
    power.targetsAffected = rawJson.targets_affected;
  }

  power.rank = rank;
  power.available = availableLevel;
  // See the field doc on the archetype converter's `autoIssue`/`free`: the game
  // grants rather than offers a power when AutoIssue passes together with
  // BuyRequires and available <= level (character_base.c:1952).
  power.autoIssue = rawJson.auto_issue === true;
  power.free = rawJson.free === true;

  // Description & help
  power.description = helpText(rawJson.display_help) || '';
  // An unresolved message-store key is not text — see `_display-text.cjs`.
  if (displayText(rawJson.display_short_help)) {
    power.shortHelp = rawJson.display_short_help.replace(/\u00a0/g, ' ');
  }
  power.icon = normalizeIconPath(rawJson.icon || '');
  // Map bin's "GlobalBoost" to the planner's "Global Enhancement" type.
  power.powerType = rawJson.type === 'GlobalBoost' ? 'Global Enhancement' : (rawJson.type || 'Click');

  // Target type — same normalization powerset converter applies. Required
  // by the dashboard's ally-only filter so ally-buff toggles in epic pools
  // (e.g. patron pet-buff utilities) don't apply to the caster's totals.
  if (rawJson.target_type) {
    const mapped = TARGET_TYPE_MAP[rawJson.target_type];
    if (mapped) power.targetType = mapped;
  }

  // Game modes — the same four arrays archetype and pool powers carry, stamped here
  // so a power's mode gating never depends on which converter tree produced it.
  assignModes(power, rawJson);

  // Requires
  if (gateTokens(rawJson.requires).length) {
    power.requires = gateTokens(rawJson.requires);
  } else {
    power.requires = [];
  }

  // Slots
  power.maxSlots = rawJson.max_boosts !== undefined && rawJson.max_boosts !== null
    ? rawJson.max_boosts : 6;

  // Allowed enhancements: bin export uses short names (BIN_BOOST_MAP);
  // CoD2 used long ones (BOOST_TYPE_MAP). Try both.
  const enhancements = (rawJson.boosts_allowed || [])
    .map(b => BOOST_TYPE_MAP[b] || BIN_BOOST_MAP[b])
    .filter(Boolean);
  power.allowedEnhancements = [...new Set(enhancements)].sort();

  // Allowed IO set categories: the exporter's `allowed_set_categories`, built
  // from boostsets.bin's per-set allowed_powers index, so it reflects what the
  // game actually permits (Pet Damage / Recharge Intensive Pets on summon
  // powers, To Hit Debuff on Darkest Night, etc.). null and [] both mean the
  // game lists this power in no set — the field stays unset. No inference
  // fallback: it invented slottability the game refuses (SETCAT-1 — Healing
  // sets in Rebirth's Hoarfrost).
  if (Array.isArray(rawJson.allowed_set_categories)
      && rawJson.allowed_set_categories.length > 0) {
    power.allowedSetCategories = [...rawJson.allowed_set_categories].sort();
  }

  // Execution stats and the two power-level display fields, in the shape an archetype power
  // publishes. The legacy copies under `effects` below stay until the bag's execution keys are
  // deleted (atom-migration, display item job 2).
  power.stats = powerStats(rawJson);
  if (rawJson.effect_area && rawJson.effect_area !== 'None') {
    power.effectArea = EFFECT_AREA_MAP[rawJson.effect_area] ?? rawJson.effect_area;
  }

  // Extract effects from raw JSON using recursive template collection.
  //
  // `collectBaseTemplates` (shared with convert-powerset.cjs) covers BOTH shapes: a
  // power's own `effects`, and — when those are empty — its redirect chain. This
  // converter used to inline `collectTemplatesDeep(rawJson.effects)` behind an
  // `if (rawJson.effects?.length)` guard, so the six redirect-only epic snipes (Psionic
  // Lance, LRM Rocket, Frozen Spear, Mace Beam, Zapp, Moonbeam) shipped with NO DAMAGE
  // AT ALL. Their powerset twins — e.g. Blaster Dark Blast's Moonbeam — are the same
  // redirect-only shape and resolve correctly, purely because convert-powerset.cjs
  // converts them. See collectBaseTemplates' docstring.
  const { templates: allTemplates } = collectBaseTemplates(rawJson);
  if (allTemplates.length > 0) {
    // Damage
    const damage = extractDamage(allTemplates);
    if (damage) {
      power.damage = damage;
    }

    // Per-target stacking (Soul Drain / Spirit Drain's +ToHit/+Damage per foe hit).
    // This converter never ran the detector, so the ENTIRE epic/patron tier shipped its
    // AoE self-buffs as flat values: epic Soul Drain was {scale:1}/{scale:4} where the
    // identical Dark Melee primary power is {1.2, perTarget:0.2}/{4.8, perTarget:0.8}.
    // At 8 foes that under-reported +2.6 ToHit as +1.0 and +10.4 damage as +4.0 — a real
    // planner bug for every Blaster/Controller/Corruptor taking Soul Drain via an epic
    // pool, invisible until the Plan B shadows widened to sweep this tree (2026-07-15).
    //
    // Must run BEFORE the atom emit below: `computeAoePerTargetPatches` stamps
    // `_perTargetIncrement` on the templates, and `encodeAtomsForEmit` copies it onto
    // the atom (see AtomicEffect.perTarget). Reversing the order silently drops the stamp.
    // detectStackingEffects stamps _perTargetIncrement on the templates (atom path);
    // the bag it once patched is gone (atom1-13).
    detectStackingEffects(rawJson);

    // The pets. This partition reached `extractSummon` through the bag and nothing else, so
    // retiring the bag took every epic and patron summon with it — 50 records per fork, the
    // whole patron tier (Summon Widow, Mu Guardian, the Coralax) plus the epic rains, showing
    // no pet and folding no aura (ENT-22). Same address as convert-powerset.cjs's, so one
    // reader answers for both partitions.
    const summon = extractPowerSummon(allTemplates, rawJson.name);
    if (summon) power.summon = summon;

  }

  // The fast (uninterruptible) redirect form, and the slow form's real timing. This converter
  // never called either extractor, so the six epic snipes shipped a MIX of the two forms: the
  // redirect shell's `activation_time` mirrors the fast anim, while `collectBaseTemplates`
  // resolves the slow branch's damage. Epic Moonbeam read 1.33s cast against the 4.5-scale
  // charged hit — a cast that belongs to neither form as displayed. Same fix as
  // convert-powerset.cjs, into this partition's bag keys (`normalize_legacy_power` renames
  // `activationTime` → `castTime` at load).
  const quickSnipe = extractQuickSnipeData(rawJson);
  if (quickSnipe) {
    power.quickSnipe = quickSnipe;
    const base = extractSnipeBaseTiming(rawJson);
    // Into `stats`. The legacy bag keys are gone (atom1-13). The mint above reads the
    // redirect SHELL's timing, which is the fast anim; the base is the slow form's.
    if (base?.castTime != null) {
      power.stats.castTime = base.castTime;
    }
    if (base?.interruptTime != null) {
      power.stats.interruptTime = base.interruptTime;
    }
    // A root time carried over from the (unread) shell would contradict the base cast.
    if (base?.timeToRoot != null) power.stats.timeToRoot = base.timeToRoot;
    else delete power.stats.timeToRoot;
  }

  // Alternate records the redirect table selects by an evaluated condition — the same transform
  // archetype and pool powers get. Rebirth's Attack Munitions is this partition's carrier: it
  // fires as a rifle, as pistols, or plain, by which Guardian assault picks the build holds.
  const formVariants = extractFormVariants(rawJson);
  if (formVariants) {
    power.formVariants = formVariants;
  }

  // Caster-state writes (grant/revoke edges) — same stamp as the other partitions; called
  // explicitly for the reason convert-pool-powers.cjs records at its own call.
  const grantEdges = extractGrantEdges(rawJson);
  if (grantEdges) {
    power.grantEdges = grantEdges;
  }

  // Plan B — emit the pre-projection atom list alongside the bag, exactly as
  // convert-powerset.cjs and convert-pool-powers.cjs do. Epic-pool powers went
  // without it until 2026-07-15, so every atom-native applier silently fell back to
  // The conditional→atom join, stamped ahead of the atom emit for the reason
  // convert-powerset.cjs gives at its own copy of this pass: `extractConditionalEffects`
  // stamps `_conditionalId` on the surviving groups' templates, and `encodeAtomsForEmit`
  // carries it onto the atom (AtomicEffect.conditionalId), so a stamp after the encode
  // reaches nothing. `stampOnly` skips the `_perTargetIncrement` patch that makes the real
  // extraction order-sensitive, and cannot change which groups survive.
  stampConditionalIds(rawJson.effects, rawJson);

  // the bag for the whole epic/patron tier — safe, but invisible, and a Phase 3
  // blocker. Guarded on `atomTemplates`, NOT on the bag's `allTemplates` — see the
  // same block in convert-powerset.cjs. The base-set hard invariant is asserted
  // inside `encodeAtomsForEmit`.
  {
    const atomTemplates = [...new Set([
      ...allTemplates,
      ...collectAtomTemplates(rawJson.effects || []),
    ])];
    if (atomTemplates.length > 0) {
      const atoms = encodeAtomsForEmit(atomTemplates, allTemplates, rawJson.name);
      if (atoms) power.atoms = atoms;
    }
  }

  // Conditional bonus effects (Mechanic Adjusters) — the same emit convert-powerset.cjs and
  // convert-pool-powers.cjs run, and for the same reason placed AFTER the atom block: the
  // extractor stamps `_perTargetIncrement` on the templates it patches and `encodeAtomsForEmit`
  // copies that stamp, so the order decides whose increment lands on the base atoms.
  //
  // This partition's carriers are the three Leviathan Mastery Spirit Sharks (whose Leviathan
  // Hunger gate is an ownership claim), EM Pulse's vs-Machines bonus and, on Thunderspy, Dark
  // Obliteration's hide bonus. See COND-2.
  if (rawJson.effects?.length) {
    const conditional = extractConditionalEffects(rawJson.effects, rawJson);
    if (conditional) power.conditionalEffects = conditional;
  }

  // Thunderspy: the two target-trap guards, which this partition had never run at all — the
  // widest half of TWIN-2. The trap belongs to the BINARY (fork 6 drops aspect and the
  // per-template target), so it reaches every converter that reads one, and an epic power is
  // no less exposed than a powerset one. Hibernate and Rise of the Phoenix shipped a recovered
  // `immobilize` on `["Self"]` powers because of it.
  if (datasetId === 'thunderspy') {
    // The guards read the effects PROJECTION to decide which slots are Thunderspy target-trap
    // artifacts, then stamp the atoms behind them. The bag is no longer emitted on the power,
    // so it is computed here for the guards alone and thrown away — dropping the computation
    // along with the emission is what silently retired both guards and let the trapped rows
    // reach the engine on the atoms (TSPY-10, and it re-opened TWIN-2's widening).
    const guardBag = extractEffects(allTemplates, undefined, rawJson.targets_affected);
    guardThunderspyOnesBuffs(power, rawJson.targets_affected, guardBag);
    guardThunderspyAppliedMez(power, rawJson.targets_affected, guardBag);
  }

  return power;
}

// ============================================
// POOL ID -> ARCHETYPE INFERENCE
// ============================================

// For pool IDs that aren't in the existing data, infer the archetype from the
// pool ID prefix (e.g. "blaster_dark_mastery" -> "blaster"). VEAT pools start
// with "veat_" and belong to the Arachnos archetypes. Shared/no-prefix pools
// (e.g. "cold_mastery") need an explicit archetype tag in the override layer.
const ARCHETYPE_PREFIXES = [
  'arachnos_soldier', 'arachnos_widow', 'peacebringer', 'warshade',
  'blaster', 'brute', 'controller', 'corruptor', 'defender', 'dominator',
  'mastermind', 'scrapper', 'sentinel', 'stalker', 'tanker',
  // Rebirth-only AT
  'guardian',
];

function inferArchetypeFromPoolId(poolId) {
  if (poolId.startsWith('veat_')) return 'arachnos_soldier';
  for (const at of ARCHETYPE_PREFIXES) {
    if (poolId.startsWith(at + '_')) return at;
  }
  return ''; // unknown — needs override
}

// Last-resort archetype detection for Rebirth-only pools that have no
// archetype prefix and no entry in HC's data (frost_mastery, martial_mastery,
// inferno_mastery, etc.). Reads the pool's power JSONs and looks for
// `@Class_<Name>` markers in `requires` — these are how the game gates a
// power to a specific archetype. When several classes appear (a defender/
// corruptor shared pool like frost_mastery), use the canonical hero side
// for consistency with HC's tagging convention.
const CLASS_NAME_TO_ARCHETYPE = {
  'Blaster': 'blaster', 'Brute': 'brute', 'Controller': 'controller',
  'Corruptor': 'corruptor', 'Defender': 'defender', 'Dominator': 'dominator',
  'Mastermind': 'mastermind', 'Scrapper': 'scrapper', 'Sentinel': 'sentinel',
  'Stalker': 'stalker', 'Tanker': 'tanker', 'Peacebringer': 'peacebringer',
  'Warshade': 'warshade', 'Arachnos_Soldier': 'arachnos_soldier',
  'Arachnos_Widow': 'arachnos_widow', 'Guardian': 'guardian',
};
const CLASS_PREFERENCE = [
  // When a pool is shared across alignments, prefer the hero-side AT — matches
  // HC's canonical tagging (cold_mastery → blaster, not corruptor).
  'defender', 'tanker', 'scrapper', 'controller', 'blaster',
  'corruptor', 'brute', 'stalker', 'dominator', 'mastermind',
  'sentinel', 'peacebringer', 'warshade',
  'arachnos_soldier', 'arachnos_widow', 'guardian',
];

function inferArchetypeFromPoolPowers(poolPath) {
  let files;
  try {
    files = fs.readdirSync(poolPath).filter(f => f.endsWith('.json') && f !== 'index.json');
  } catch (_) { return ''; }
  const found = new Set();
  for (const f of files) {
    let data;
    try { data = JSON.parse(fs.readFileSync(path.join(poolPath, f), 'utf-8')); }
    catch (_) { continue; }
    for (const field of ['requires', 'activate_requires', 'target_requires']) {
      const req = data[field] || '';
      const re = /@Class_([A-Za-z_]+)/g;
      let m;
      while ((m = re.exec(req)) !== null) {
        const at = CLASS_NAME_TO_ARCHETYPE[m[1]];
        if (at) found.add(at);
      }
    }
  }
  for (const at of CLASS_PREFERENCE) {
    if (found.has(at)) return at;
  }
  return '';
}

// ============================================
// CONVERT A SINGLE EPIC POOL
// ============================================

function convertEpicPool(poolId, existingPool) {
  const poolPath = path.join(RAW_POWERS_PATH, poolId);
  if (!fs.existsSync(poolPath)) {
    console.error(`  Raw pool directory not found: ${poolPath}`);
    return null;
  }

  // Load pool index
  const indexPath = path.join(poolPath, 'index.json');
  if (!fs.existsSync(indexPath)) {
    console.error(`  Pool index not found: ${indexPath}`);
    return null;
  }

  const poolIndex = JSON.parse(fs.readFileSync(indexPath, 'utf-8'));

  // Compatibility: bin export uses `powers` + `help`; CoD2 used
  // `power_names` + `display_help`. Normalize so the rest of this
  // function can use the CoD2 names.
  if (!poolIndex.power_names && poolIndex.powers) poolIndex.power_names = poolIndex.powers;
  if (!poolIndex.display_help && poolIndex.help) poolIndex.display_help = poolIndex.help;
  if (!poolIndex.display_short_help && poolIndex.short_help) poolIndex.display_short_help = poolIndex.short_help;

  // Build pool metadata — preserve archetype/minLevel from existing data,
  // use raw data for display info. Fallback chain when archetype is empty:
  //   1. Existing pool's tag (HC was hand-curated long ago).
  //   2. Pool ID prefix (e.g. `blaster_dark_mastery` → blaster).
  //   3. HC's generated data — same pool IDs, hand-curated tags
  //      (covers `arctic_mastery`, `cold_mastery`, etc. for Rebirth).
  //   4. Scan the pool's power requires for @Class_X — covers
  //      Rebirth-only pools without HC counterparts (frost_mastery,
  //      martial_mastery, inferno_mastery).
  const fallbackArchetype = (existingPool && existingPool.archetype)
    || inferArchetypeFromPoolId(poolId)
    || (FALLBACK_POOLS[poolId] && FALLBACK_POOLS[poolId].archetype)
    || inferArchetypeFromPoolPowers(poolPath)
    || '';
  // The set's binary name — the only spelling a `requires` set path uses.
  if (!poolIndex.key) {
    throw new Error(`${indexPath}: missing key (the binary set path)`);
  }

  const pool = {
    id: poolId,
    setPath: poolIndex.key,
    buyRequires: poolIndex.buy_requires || [],
    buyRequiresFailed: poolIndex.buy_requires_failed || '',
    specializeAt: poolIndex.specialize_at || 0,
    specializeRequires: poolIndex.specialize_requires || [],
    name: poolIndex.display_name || (existingPool ? existingPool.name : poolId),
    displayName: poolIndex.display_name || (existingPool ? existingPool.displayName : poolId),
    archetype: fallbackArchetype,
    description: helpText(poolIndex.display_help) || (existingPool ? existingPool.description : ''),
    icon: poolIndex.icon || (existingPool ? existingPool.icon : ''),
    requires: poolIndex.requires || [],
    minLevel: existingPool ? existingPool.minLevel : 35,
    powers: [],
  };

  // Get power order from index. The .pigg bin-export `powers` array is already
  // in native game-pick order (the old CoD2 source was alphabetical, hence the
  // legacy sort below). Sorting by available_level is now a stabilizer that
  // keeps the displayed pick order matching the in-game leveling sequence — it
  // only ever reorders a pool whose native order is not already level-ascending.
  const powerNames = poolIndex.power_names || [];
  const availableLevels = poolIndex.available_level || [];

  const collected = [];
  for (let i = 0; i < powerNames.length; i++) {
    const fullName = powerNames[i];
    const parts = fullName.split('.');
    const powerFile = parts[parts.length - 1].toLowerCase() + '.json';
    const filePath = path.join(poolPath, powerFile);

    if (!fs.existsSync(filePath)) {
      console.warn(`  Power file not found: ${powerFile} (from ${fullName})`);
      continue;
    }

    // Through the gate-stamping reader (COND-12), same reason as the pool converter.
    const rawJson = _readPowerFile(filePath);
    if (availableLevels[i] === undefined) {
      throw new Error(
        `No available_level entry for ${fullName} in ${poolPath} index.json — `
        + `malformed export, refusing to fabricate an unlock level`);
    }
    const availableLevel = availableLevels[i];
    collected.push({ rawJson, availableLevel, originalIndex: i });
  }

  collected.sort((a, b) => {
    if (a.availableLevel !== b.availableLevel) return a.availableLevel - b.availableLevel;
    return a.originalIndex - b.originalIndex;
  });

  collected.forEach((entry, idx) => {
    const power = convertEpicPower(entry.rawJson, idx + 1, entry.availableLevel);
    pool.powers.push(power);
  });

  return pool;
}

// ============================================
// SERIALIZE OUTPUT
// ============================================

function serializeValue(val, indent) {
  if (val === null || val === undefined) return 'null';
  if (typeof val === 'number') return String(val);
  if (typeof val === 'boolean') return String(val);
  if (typeof val === 'string') return JSON.stringify(val);

  if (Array.isArray(val)) {
    if (val.length === 0) return '[]';
    const items = val.map(v => {
      if (typeof v === 'string') return `${' '.repeat(indent + 2)}${JSON.stringify(v)}`;
      return `${' '.repeat(indent + 2)}${serializeValue(v, indent + 2)}`;
    });
    return `[\n${items.join(',\n')}\n${' '.repeat(indent)}]`;
  }

  if (typeof val === 'object') {
    const keys = Object.keys(val);
    if (keys.length === 0) return '{}';
    const entries = keys.map(k => {
      // `atoms` is the Plan B wire list — one positional tuple per atom, rendered one
      // per line. The generic array branch above would explode every scalar onto its
      // own line, inflating the compact encoding ~10× on disk. Mirrors `serializePower`
      // in convert-powerset.cjs.
      if (k === 'atoms' && Array.isArray(val[k]) && val[k].length) {
        const tuples = val[k]
          .map(t => `${' '.repeat(indent + 4)}${JSON.stringify(t)}`)
          .join(',\n');
        return `${' '.repeat(indent + 2)}${JSON.stringify(k)}: [\n${tuples}\n${' '.repeat(indent + 2)}]`;
      }
      const v = serializeValue(val[k], indent + 2);
      return `${' '.repeat(indent + 2)}${JSON.stringify(k)}: ${v}`;
    });
    return `{\n${entries.join(',\n')}\n${' '.repeat(indent)}}`;
  }

  return String(val);
}

// ============================================
// MAIN
// ============================================

function main() {
  console.log(`=== CONVERT EPIC POOLS ${applyChanges ? '(APPLYING)' : '(DRY RUN)'} ===\n`);

  const existingPools = loadExistingPools();
  const existingIds = Object.keys(existingPools);

  // Discover pools on disk too — bin export may have added new pools that
  // aren't yet in the committed file. Union with existing IDs so the no-filter
  // run picks up everything.
  let discoveredIds = [];
  try {
    discoveredIds = fs.readdirSync(RAW_POWERS_PATH).filter((entry) => {
      const indexFile = path.join(RAW_POWERS_PATH, entry, 'index.json');
      return fs.existsSync(indexFile);
    });
  } catch (_) {
    // RAW_POWERS_PATH missing — fall through to existingIds
  }
  const allKnownIds = [...new Set([...existingIds, ...discoveredIds])].sort();

  // Determine which pools to convert
  let poolIds;
  if (poolFilter) {
    poolIds = [poolFilter];
  } else if (archFilter) {
    poolIds = allKnownIds.filter(id => {
      const at = existingPools[id]?.archetype
        || inferArchetypeFromPoolId(id)
        || (FALLBACK_POOLS[id] && FALLBACK_POOLS[id].archetype)
        || '';
      return at === archFilter;
    });
    console.log(`Filtering to archetype "${archFilter}": ${poolIds.length} pools\n`);
  } else {
    poolIds = allKnownIds;
  }

  const allPools = {};

  for (const poolId of poolIds) {
    console.log(`Converting ${poolId}...`);
    const existing = existingPools[poolId];
    const pool = convertEpicPool(poolId, existing);

    if (pool) {
      allPools[poolId] = pool;
      console.log(`  ${pool.powers.length} powers converted`);

      // Compare with existing
      if (existing) {
        const existingNames = existing.powers.map(p => p.name);
        const newNames = pool.powers.map(p => p.name);
        const added = newNames.filter(n => !existingNames.includes(n));
        const removed = existingNames.filter(n => !newNames.includes(n));
        if (added.length) console.log(`  Added: ${added.join(', ')}`);
        if (removed.length) console.log(`  Removed: ${removed.join(', ')}`);
      }
    }
  }

  // If filtering, merge with existing (unconverted pools keep existing data)
  if (poolFilter || archFilter) {
    for (const [id, pool] of Object.entries(existingPools)) {
      if (!allPools[id]) allPools[id] = pool;
    }
  }

  // Generate output
  const totalPowers = Object.values(allPools).reduce((sum, p) => sum + p.powers.length, 0);
  const byArchetype = {};
  for (const pool of Object.values(allPools)) {
    byArchetype[pool.archetype] = (byArchetype[pool.archetype] || 0) + 1;
  }

  let output = `/**\n`;
  output += ` * Raw Epic/Patron Pool data — GENERATED LAYER\n`;
  output += ` * Auto-generated from raw Homecoming JSON data.\n`;
  output += ` *\n`;
  output += ` * DO NOT EDIT THIS FILE DIRECTLY. Overrides live in\n`;
  output += ` * src/data/overrides/epic-pools.ts and are merged by the\n`;
  output += ` * composed facade at src/data/epic-pools-raw.ts.\n`;
  output += ` * Run: node scripts/convert-epic-pools.cjs --apply\n`;
  output += ` *\n`;
  output += ` * Total pools: ${Object.keys(allPools).length}\n`;
  output += ` * Total powers: ${totalPowers}\n`;
  output += ` * Archetypes: ${JSON.stringify(byArchetype)}\n`;
  output += ` */\n\n`;
  output += `export const EPIC_POOLS_RAW = ${serializeValue(allPools, 0)};\n`;

  if (applyChanges) {
    fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
    fs.mkdirSync(path.dirname(OVERRIDES_PATH), { recursive: true });
    fs.writeFileSync(OUTPUT_PATH, output);
    console.log(`\nWritten to ${OUTPUT_PATH}`);
    console.log(`${Object.keys(allPools).length} pools, ${totalPowers} powers`);

    // Scaffold overrides + composed if missing (atomic: both or neither).
    if (!fs.existsSync(OVERRIDES_PATH) && !fs.existsSync(COMPOSED_PATH)) {
      fs.writeFileSync(OVERRIDES_PATH, `/**
 * Epic/Patron Pool overrides — hand-written deltas keyed by a power's
 * \`fullName\`. The composed facade (src/data/epic-pools-raw.ts) merges
 * each entry into its matching generated power via \`withOverrides\`.
 *
 * Empty record means no overrides. Add entries here when the stale
 * CoD2 raw extraction disagrees with current HC game values. See
 * src/data/README.md for the layering convention.
 */
import type { Power } from '@/types';

export const EPIC_POOL_OVERRIDES: Record<string, Partial<Power>> = {};
`);
      // `_layer` lives at `src/data/_layer.ts`. HC's composed file is a
      // sibling so `./_layer` works; other datasets sit at
      // `src/data/datasets/<id>/` and need the absolute alias.
      const layerImport = '@/data/_layer';
      fs.writeFileSync(COMPOSED_PATH, `/**
 * Epic/Patron Pool data — COMPOSED FACADE
 *
 * Merges the auto-generated extraction with hand-written per-power
 * overrides. The planner imports from here; see src/data/README.md.
 *
 * To re-generate the base data:
 *   node scripts/convert-epic-pools.cjs --apply
 */
import { applyAggregateOverrides } from '${layerImport}';
import { EPIC_POOLS_RAW as base } from './generated/epic-pools';
import { EPIC_POOL_OVERRIDES } from './overrides/epic-pools';

export const EPIC_POOLS_RAW = applyAggregateOverrides(base, EPIC_POOL_OVERRIDES);
`);
      console.log(`  Scaffolded composed facade: ${COMPOSED_PATH}`);
      console.log(`  Scaffolded overrides stub:  ${OVERRIDES_PATH}`);
    }
  } else {
    console.log(`\nWould write ${Object.keys(allPools).length} pools, ${totalPowers} powers`);
    console.log(`Run with --apply to write changes`);
  }
}

main();
