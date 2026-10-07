/**
 * Extract AT Modifier Tables from the class export
 *
 * Reads one JSON file per character class and writes the dataset's
 * `pipeline/<id>/at-tables.json`: the `named_tables` a scaled effect resolves against, for the
 * player archetypes (`AT_TABLES`) and, separately, for the pet classes
 * (`PET_TABLES`) — a summon is a second character and resolves against its OWN
 * class row.
 *
 * A class file carries more than tables, and the rest is emitted alongside
 * rather than discarded: each archetype's RechargeTime `ClampStrength` interval
 * (`rechargeBounds`), and each pet class's own character stats (`attribs` —
 * hit points and the caps the summon lives against).
 *
 * Both rosters are derived from the export, never hand-listed; see
 * `_player-classes.cjs` for which signal names each one.
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath } = require('./_dataset-paths.cjs');

const datasetId = parseDatasetArg();

// Source: per-AT JSON files produced by `tools/bin-crawler/bin_crawler/
// export_classes.py`. HC ships at the legacy flat layout
// (`exported_powers/tables/`); other datasets are namespaced
// (`exported_powers/<id>/tables/`).
const RAW_DATA_BASE = path.join(__dirname, '..', 'exported_powers');
const RAW_DATA_PATH = (datasetId === 'homecoming' && !fs.existsSync(path.join(RAW_DATA_BASE, datasetId, 'tables')))
  ? path.join(RAW_DATA_BASE, 'tables')
  : path.join(RAW_DATA_BASE, datasetId, 'tables');

// at-tables.ts has migrated into `src/data/datasets/<id>/` — write there
// directly so we don't clobber the runtime facade at `src/data/at-tables.ts`.
const OUTPUT_PATH = pipelinePath(datasetId, 'at-tables.json');

// The pet entities live beside the tables, one directory up: HC's flat layout
// puts them at `exported_powers/entities/`, the namespaced ones at
// `exported_powers/<id>/entities/`.
const ENTITIES_PATH = path.join(RAW_DATA_PATH, '..', 'entities');

const { derivePlayerArchetypes, derivePetClasses } = require('./_player-classes.cjs');

const PLAYER_ARCHETYPES = derivePlayerArchetypes(RAW_DATA_PATH);
const PET_CLASSES = derivePetClasses(ENTITIES_PATH, RAW_DATA_PATH);

/**
 * The pet class's own character stats — the numbers that make a summon a second
 * character rather than an effect. `hit_points`, `hp_cap` and `absorb_cap` are
 * per-level arrays (index = level − 1); the caps and clamps are scalars.
 * Returns null when the export doesn't carry the block, so a consumer can show
 * nothing rather than invent a hit point total.
 */
function petAttribs(data, petClass) {
  const a = data.attribs;
  if (!a || typeof a !== 'object') {
    console.warn(`  ${petClass}: no attribs block — pet stats will be unavailable`);
    return null;
  }
  const levelArray = (key) => {
    const v = a[key];
    if (!Array.isArray(v) || v.length === 0) return undefined;
    if (!v.every((n) => typeof n === 'number' && Number.isFinite(n))) return undefined;
    return v;
  };
  const scalar = (key) => (typeof a[key] === 'number' && Number.isFinite(a[key]) ? a[key] : undefined);
  const movement = (key) => {
    const m = a[key];
    if (!m || typeof m !== 'object') return undefined;
    const out = {};
    for (const axis of ['run_speed', 'fly_speed', 'jump_speed', 'jump_height']) {
      const v = m[axis];
      if (typeof v === 'number' && Number.isFinite(v)) out[axis] = v;
      else if (Array.isArray(v) && v.every((n) => typeof n === 'number' && Number.isFinite(n))) out[axis] = v;
    }
    return Object.keys(out).length > 0 ? out : undefined;
  };

  const hitPoints = levelArray('hit_points');
  if (!hitPoints) {
    console.warn(`  ${petClass}: no usable hit_points array — pet stats will be unavailable`);
    return null;
  }
  return {
    hitPoints,
    hpCap: levelArray('hp_cap'),
    absorbCap: levelArray('absorb_cap'),
    resistanceCap: scalar('resistance_cap'),
    damageCap: scalar('damage_cap'),
    baseThreat: scalar('base_threat'),
    rechargeFloor: scalar('recharge_floor'),
    rechargeCap: scalar('recharge_cap'),
    enduranceFloor: scalar('endurance_floor'),
    enduranceCap: scalar('endurance_cap'),
    movementBase: movement('movement_base'),
    movementCap: movement('movement_cap'),
  };
}

/**
 * The class's RechargeTime `ClampStrength` interval — the floor and cap the
 * server bounds NET recharge strength to, exported per class in `attribs`
 * (`{floor: 0.25, cap: 5}` on every player class: −75% debuff floor, +400% cap).
 *
 * Unlike most clamps this one is REACHABLE — a perma build stacking Hasten, set
 * bonuses and Ageless lives against it — so the perma tracker needs it both to
 * stop selling recharge past the ceiling and to decide whether a power's cycle
 * can ever fit inside its own window.
 *
 * Returns null when the export doesn't carry the pair, so a consumer can stand
 * aside rather than invent a ceiling. That is the failure mode a hardcoded 5
 * used to hide: it was ALSO wrong about what the number meant, reading the ×5.0
 * net strength as a +500% bonus.
 */
function rechargeBounds(data, at) {
  const attribs = data.attribs;
  if (!attribs || typeof attribs !== 'object') return null;
  const floor = attribs.recharge_floor;
  const cap = attribs.recharge_cap;
  if (typeof floor !== 'number' || typeof cap !== 'number'
      || !Number.isFinite(floor) || !Number.isFinite(cap) || cap <= 0) {
    console.warn(`  ${at}: no usable recharge clamp bounds in attribs — perma reachability will stand aside`);
    return null;
  }
  return { floor, cap };
}

// Principled filter: include every binary named table exposed on player AT/pet
// class exports. This avoids hand-maintained allowlists that silently miss real
// tables until a power references one in a fatal slot.
function normalizeNamedTableEntry(tableName, tableValues) {
  if (typeof tableName !== 'string' || tableName.length === 0) return null;
  if (!Array.isArray(tableValues) || tableValues.length === 0) return null;
  if (!tableValues.every((v) => typeof v === 'number' && Number.isFinite(v))) return null;
  return {
    key: tableName.toLowerCase(),
    values: tableValues,
  };
}

function extractTables() {
  const allTables = {};

  for (const at of PLAYER_ARCHETYPES) {
    const filePath = path.join(RAW_DATA_PATH, `${at}.json`);

    if (!fs.existsSync(filePath)) {
      console.warn(`Warning: ${at}.json not found`);
      continue;
    }

    console.log(`Processing ${at}...`);
    const data = JSON.parse(fs.readFileSync(filePath, 'utf-8'));

    const atKey = at.replace(/_/g, '-'); // arachnos_soldier -> arachnos-soldier
    allTables[atKey] = {
      primaryCategory: data.primary_category,
      secondaryCategory: data.secondary_category,
      rechargeBounds: rechargeBounds(data, at),
      tables: {}
    };

    if (data.named_tables && typeof data.named_tables === 'object') {
      for (const [tableName, tableValues] of Object.entries(data.named_tables)) {
        const normalized = normalizeNamedTableEntry(tableName, tableValues);
        if (!normalized) continue;
        allTables[atKey].tables[normalized.key] = normalized.values;
      }
    }
  }

  return allTables;
}

function extractPetTables() {
  const petTables = {};

  // Every entry has a table file: `derivePetClasses` refuses to return a class
  // whose file is absent, so there is no missing-file case to skip here.
  for (const petClass of PET_CLASSES) {
    const filePath = path.join(RAW_DATA_PATH, `${petClass}.json`);

    console.log(`Processing pet class ${petClass}...`);
    const data = JSON.parse(fs.readFileSync(filePath, 'utf-8'));

    petTables[petClass] = {
      villainRank: typeof data.villain_rank === 'number' ? data.villain_rank : undefined,
      attribs: petAttribs(data, petClass),
      tables: {},
    };

    if (data.named_tables && typeof data.named_tables === 'object') {
      for (const [tableName, tableValues] of Object.entries(data.named_tables)) {
        const normalized = normalizeNamedTableEntry(tableName, tableValues);
        if (!normalized) continue;
        petTables[petClass].tables[normalized.key] = normalized.values;
      }
    }

    // A class an entity NAMES but that carries no usable table resolves nothing
    // for that pet, so dropping it here would hand the consumer a silent miss
    // instead of a gap it can see (ENT-10).
    if (Object.keys(petTables[petClass].tables).length === 0) {
      throw new Error(
        `extract-at-tables: pet class '${petClass}' is named by a pet entity but its class file `
          + 'carries no usable named tables — every scaled effect on that pet would resolve '
          + 'against nothing.',
      );
    }
  }

  return petTables;
}

// THE TABLE RECORDS, FIELD BY FIELD.
//
// Reproduced verbatim from the TypeScript interfaces this converter used to emit above
// the tables. The interfaces went with the TypeScript; what they say did not survive in
// any value, and `crates/coh_data/src/at_tables.rs` is what reads these fields now.
//
// export interface ATTableData {
//   primaryCategory: string;
//   secondaryCategory: string;
//   /** RechargeTime ClampStrength interval — the bounds on NET recharge
//    *  strength (floor 0.25 = the −75% debuff floor, cap 5 = +400%). Absent
//    *  when the export didn't carry it; consumers stand aside rather than
//    *  invent a ceiling. */
//   rechargeBounds?: { floor: number; cap: number };
//   tables: Record<string, number[]>;
// }
//
// /** A pet class's own character stats — see PetTableData. */
// export interface PetClassAttribs {
//   /** Base max HP per level (index = level − 1). */
//   hitPoints: number[];
//   hpCap?: number[];
//   absorbCap?: number[];
//   /** Damage-resistance ceiling as a fraction (0.9 = 90%). */
//   resistanceCap?: number;
//   /** Damage strength ceiling as a multiplier (4 = +300%). */
//   damageCap?: number;
//   baseThreat?: number;
//   rechargeFloor?: number;
//   rechargeCap?: number;
//   enduranceFloor?: number;
//   enduranceCap?: number;
//   movementBase?: PetMovementAttrib;
//   movementCap?: PetMovementAttrib;
// }
//
// /** Scalar for a base, per-level array for a cap. */
// export interface PetMovementAttrib {
//   run_speed?: number | number[];
//   fly_speed?: number | number[];
//   jump_speed?: number | number[];
//   jump_height?: number | number[];
// }
//
// export interface PetTableData {
//   tables: Record<string, number[]>;
//   /** The class's villain-rank enum. Player classes are 0; the classes the
//    *  game treats as summons are 10 — including `henchman_boss`, so this does
//    *  NOT separate a henchman minion from a henchman boss. */
//   villainRank?: number;
//   /** The class's own character stats — hit points and the caps a summon
//    *  lives against. A summon is a second character, so these resolve
//    *  against the PET's class row, never the caster's archetype.
//    *  Absent when the export didn't carry an attribs block. */
//   attribs?: PetClassAttribs;
// }
//
//
// THE FOUR LOOKUP FUNCTIONS THIS CONVERTER USED TO EMIT — getTableValue,
// calculateEffectValue, calculateIncarnateDamage, getPetTableValue — were code, not data,
// and a converter has no business writing code. They are in two places now: the engine
// (`crates/coh_data/src/at_tables.rs`, which is what ships) and `scripts/_at-table-lookup.cjs`
// for the scripts here that ask a table a question directly.

/** Both tables as data, with exactly the fields the text emitter wrote. */
function buildData(tables, petTables) {
  const AT_TABLES = {};
  for (const [atKey, atData] of Object.entries(tables)) {
    const record = {
      primaryCategory: atData.primaryCategory,
      secondaryCategory: atData.secondaryCategory,
    };
    if (atData.rechargeBounds) {
      record.rechargeBounds = { floor: atData.rechargeBounds.floor, cap: atData.rechargeBounds.cap };
    }
    record.tables = atData.tables;
    AT_TABLES[atKey] = record;
  }

  const out = { AT_TABLES };

  // PET_TABLES is omitted entirely when the dataset's export carries no pet classes,
  // exactly as the text emitter omitted the export. The contract section reads it by
  // name, so an omitted table and an empty one are not the same statement.
  if (petTables && Object.keys(petTables).length > 0) {
    const PET_TABLES = {};
    for (const [petClass, petData] of Object.entries(petTables)) {
      const record = {};
      if (typeof petData.villainRank === 'number') record.villainRank = petData.villainRank;
      if (petData.attribs) {
        const attribs = {};
        for (const [key, value] of Object.entries(petData.attribs)) {
          if (value === undefined) continue;
          attribs[key] = value;
        }
        record.attribs = attribs;
      }
      record.tables = petData.tables;
      PET_TABLES[petClass] = record;
    }
    out.PET_TABLES = PET_TABLES;
  }

  return out;
}

// Run extraction
console.log('Extracting AT modifier tables...\n');
const tables = extractTables();

console.log('\nExtracting pet class tables...\n');
const petTables = extractPetTables();

fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
fs.writeFileSync(OUTPUT_PATH, JSON.stringify(buildData(tables, petTables)));
console.log(`\nWrote ${OUTPUT_PATH}`);

// Print summary
console.log('\nPlayer Archetypes:');
for (const [at, data] of Object.entries(tables)) {
  const tableCount = Object.keys(data.tables).length;
  console.log(`  ${at}: ${tableCount} tables`);
}

console.log('\nPet Classes:');
for (const [petClass, data] of Object.entries(petTables)) {
  const tableCount = Object.keys(data.tables).length;
  console.log(`  ${petClass}: ${tableCount} tables`);
}
