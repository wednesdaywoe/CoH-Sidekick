/*
 * Archetype / pet class-modifier table lookup.
 *
 * WHY THIS FILE EXISTS. `extract-at-tables.cjs` used to emit these two functions as
 * TypeScript source text, once per dataset, into `src/data/datasets/<id>/at-tables.ts`
 * beside the tables themselves. The converter now writes the tables as data
 * (`pipeline/<id>/at-tables.json`) and does not write code at all, so the lookup lives
 * here instead — in the pipeline, which is what needs it.
 *
 * The engine that ships owns its own copy: `crates/coh_data/src/at_tables.rs` implements
 * exactly these rules, and the contract carries the tables. This copy exists for the
 * scripts in this directory that ask a table a question directly.
 *
 * A miss returns `undefined` rather than a number. That is deliberate and load-bearing:
 * the caller can tell "this class has no such table" from "this table says zero", and
 * the zero is a real answer for many rows.
 */

const fs = require('fs');
const { pipelinePath } = require('./_dataset-paths.cjs');

/** Load a dataset's converted tables. Throws rather than returning empty ones. */
function loadAtTables(datasetId) {
  const file = pipelinePath(datasetId, 'at-tables.json');
  if (!fs.existsSync(file)) {
    throw new Error(
      `no converted class tables for ${datasetId}: ${file} does not exist.\n`
      + `  Run: node scripts/extract-at-tables.cjs --dataset ${datasetId}`,
    );
  }
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

/**
 * Get a table value for a specific archetype and level.
 *
 * `atTables` is the AT_TABLES record; level is 1-based.
 */
function getTableValue(atTables, archetype, tableName, level) {
  const at = atTables[archetype];
  if (!at) return undefined;

  const key = tableName.toLowerCase();
  let table = at.tables[key];

  // Power data uses suffixed names (e.g., "Ranged_HealSelf") that map to
  // base table names (e.g., "ranged_heal"). Strip common suffixes to match.
  if (!table) {
    const stripped = key.replace(/self$|other$|target$/, '');
    table = at.tables[stripped];
  }

  // Alias temp/incarnate damage tables to base damage tables.
  if (!table) {
    const aliased = key
      .replace('_tempdamage', '_damage')
      .replace('_incarnateprocdamage', '_damage');
    if (aliased !== key) table = at.tables[aliased];
  }

  // Alias the game's "_Dam" damage-table spelling to the extracted "_dmg" key.
  // Powers reference e.g. "Ranged_Debuff_Dam" but the AT tables are keyed
  // "ranged_debuff_dmg"; without this the lookup misses and the display falls
  // back to a generic half-rate (damage debuffs rendered at half — e.g. Ice
  // Arrow -10% instead of -20%).
  if (!table) {
    const aliased = key.replace(/_dam$/, '_dmg');
    if (aliased !== key) table = at.tables[aliased];
  }

  if (!table) return undefined;

  // Level 1 = index 0; clamp to table length (HC has 105 values, Rebirth has
  // 50 — different versions cap at different levels).
  const index = Math.max(0, Math.min(table.length - 1, level - 1));
  return table[index];
}

// Two one-line wrappers went with the TypeScript and are NOT reproduced here, because
// nothing in this directory calls them and the engine has both: `calculateEffectValue`
// (scale x table value) and `calculateIncarnateDamage` (the same, absolute, with `-`
// rewritten to `_` in the table name first).

/** Get a table value for a specific pet class and level. */
function getPetTableValue(petTables, petClass, tableName, level) {
  const pet = petTables[petClass];
  if (!pet) return undefined;

  const key = tableName.toLowerCase();
  let table = pet.tables[key];

  // Strip common suffixes like getTableValue does.
  if (!table) {
    const stripped = key.replace(/self$|other$|target$/, '');
    table = pet.tables[stripped];
  }

  if (!table) return undefined;

  const index = Math.max(0, Math.min(table.length - 1, level - 1));
  return table[index];
}

module.exports = { loadAtTables, getTableValue, getPetTableValue };
