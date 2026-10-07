/**
 * The class-table files the planner consumes, shared by the converters that
 * read `exported_powers/<id>/tables/`. The export carries every server class
 * (NPC bosses, pets, monuments…); this module names the player-facing subset.
 *
 * `derivePlayerArchetypes` reads that subset straight from the export's own
 * membership signal instead of a hand-maintained allowlist — so a new
 * archetype flows in, and a fixed NPC-misclassification flows out, with no
 * edit here (resolved DATA-GAP-REGISTER CLASSES-2, 2026-07-21: the WS8
 * sequential parse restored the header fields this reads).
 *
 * `derivePetClasses` reads the pet subset from the DEMAND side: the classes the
 * exported pet entities actually name. No supply-side field marks "player-
 * relevant pet" — every pet class carries the same NPC-rank / empty-restriction
 * signal as a random enemy boss — but an entity states the class its own
 * magnitudes resolve against (`uiPowerInfo.c` `power_AddPetEffects` reads
 * `pDef->characterClassName`), so the set of classes a table consumer can ever
 * ask for is exactly the set the entities name.
 */

const fs = require('node:fs');
const path = require('node:path');

// The entity-export filename prefixes `convert-pet-entities.cjs` reads. Kept in
// step with that converter: a prefix it takes and this does not is a pet whose
// class would have no tables.
const PET_ENTITY_PREFIXES = ['pets_', 'mastermindpets_', 'incarnatepets_'];

// A class file is player-facing when the export's own membership signal says
// so. That signal differs by dataset SCHEMA, not by name (CLASSES-1/WS8):
// Homecoming deleted i24's origin-restriction fields and added VillainRank
// (0 for players, 1-11 for NPC ranks), so HC keys on `villain_rank === 0`; the
// forks kept i24's restriction gating, where every playable class carries
// Hero/Villain/Kheldian/Arachnos vocab and NPC classes carry none, so they key
// on a non-empty `special_restrictions`. Each field is present on exactly the
// dataset that owns it, so the presence check picks the right one per record.
function isPlayerClass(record) {
  if ('villain_rank' in record) return record.villain_rank === 0;
  return Array.isArray(record.special_restrictions) && record.special_restrictions.length > 0;
}

// Derive the player-archetype file stems from a dataset's exported `tables/`
// dir. Sorted for deterministic output (the generated maps are keyed by
// archetype, so entry order is cosmetic). Reproduces the former hand list
// exactly on all three datasets.
function derivePlayerArchetypes(tablesDir) {
  const stems = fs
    .readdirSync(tablesDir)
    .filter((file) => file.endsWith('.json') && !file.startsWith('_'))
    .map((file) => file.slice(0, -'.json'.length))
    .filter((stem) =>
      isPlayerClass(JSON.parse(fs.readFileSync(path.join(tablesDir, `${stem}.json`), 'utf-8'))),
    )
    .sort();

  // Fail loud on a desynced discriminator: a parser regression that dropped
  // both membership fields would silently empty this list, which then silently
  // drops every archetype's modifier tables downstream (the FLAGS-2 failure
  // mode — a self-consistent-but-wrong export). Every real dataset carries ~15
  // player archetypes; the precise per-dataset count is pinned by the coverage
  // and staleness tests.
  if (stems.length < 10) {
    throw new Error(
      `derivePlayerArchetypes: only ${stems.length} player class(es) found in ${tablesDir} — `
        + 'expected ~15. The villain_rank / special_restrictions membership signal likely '
        + 'desynced (schema drift).',
    );
  }
  return stems;
}

// The same player subset in the `Class_*` spelling an effect gate uses, read off
// each record's own `name` rather than transformed from its file stem — the
// parser resolves an archetype fork against exactly this roster
// (`export_powers._player_class_names`), so a spelling this side invented could
// silently fail to match a `requires_archetypes` entry and read as "not in the
// roster", which is the answer that decides whether a bag slot is stated at all.
function derivePlayerClassTokens(tablesDir) {
  return derivePlayerArchetypes(tablesDir)
    .map((stem) => JSON.parse(fs.readFileSync(path.join(tablesDir, `${stem}.json`), 'utf-8')).name)
    .sort();
}

// Derive the pet class table stems a dataset needs: every `character_class_name`
// its exported pet entities state, restricted to the ones that actually have a
// class table file. Sorted for deterministic output.
//
// The hand list this replaced named Homecoming's spellings only, so both forks
// lost the tables for their henchman classes (`minion_henchman`, `boss_henchman`,
// `lt_henchman`) and their praetorian grunts — a pet whose class has no tables
// resolves nothing, which is how Thunderspy's Rally The Militia reached the merge
// with no table to resolve against (ENT-10).
function derivePetClasses(entitiesDir, tablesDir) {
  if (!fs.existsSync(entitiesDir)) {
    throw new Error(`derivePetClasses: no entities dir at ${entitiesDir}`);
  }
  const demanded = new Set();
  for (const file of fs.readdirSync(entitiesDir)) {
    if (!file.endsWith('.json')) continue;
    if (!PET_ENTITY_PREFIXES.some((prefix) => file.startsWith(prefix))) continue;
    const entity = JSON.parse(fs.readFileSync(path.join(entitiesDir, file), 'utf-8'));
    const className = entity.defaults && entity.defaults.character_class_name;
    if (className) demanded.add(className);
  }

  // A demanded class with no table file is a gap the caller must see, not one to
  // pass over: its pet's every scaled effect resolves against nothing.
  const missing = [...demanded].filter(
    (className) => !fs.existsSync(path.join(tablesDir, `${className}.json`)),
  );
  if (missing.length > 0) {
    throw new Error(
      `derivePetClasses: ${missing.length} pet class(es) named by entities have no table file in `
        + `${tablesDir}: ${missing.sort().join(', ')}`,
    );
  }

  // Fail loud on a desynced read, the same way `derivePlayerArchetypes` does: an
  // entity-schema change that dropped `character_class_name` would empty this and
  // silently take every pet's tables with it. Every real dataset names ~15.
  if (demanded.size < 5) {
    throw new Error(
      `derivePetClasses: only ${demanded.size} pet class(es) named by entities in ${entitiesDir} — `
        + 'expected ~15. The `defaults.character_class_name` read likely desynced (schema drift).',
    );
  }
  return [...demanded].sort();
}

module.exports = { derivePlayerArchetypes, derivePlayerClassTokens, derivePetClasses };
