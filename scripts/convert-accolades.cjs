/**
 * Convert the Accolades powerset (`Temporary_Powers.Accolades`) → the composed
 * `ACCOLADES_POWERSET` module. The convert-side twin of convert-inherents.cjs.
 *
 * Accolades are ordinary auto-hit Self powers in the game data
 * (`exported_powers/<ds>/temporary_powers/accolades/`), each either a permanent
 * stat buff (The Atlas Medallion +5 Max End, Freedom Phalanx Reserve +10% Max HP, …)
 * or a click/travel/temp-weapon utility (Eye of the Magus, Long Range Teleport, …).
 * The stat passives are gated `activate_requires: "type char> hero eq"` (hero) /
 * "... villain eq" (villain) — the real hero↔villain split the beta's silo faked
 * with hand-authored `excludes` pairs (DATA-GAP ACCOLADE-1).
 *
 * Phase 1 extracted the whole powerset into `exported_powers/`; this Phase-2 step
 * turns the convert path's output into a Power[] module so a later phase can consume
 * them as auto-on/gated toggles and DELETE `src/data/accolades.ts`. Taken WHOLE —
 * every member index.json declares — because a "stat ones only" filter would
 * re-encode the curated proper-noun silo (Rule 0); downstream decides relevance.
 *
 * Enumeration is the powerset's own `index.json` `powers` list (game order), so a
 * fork that reorders or adds/drops members flows through for free. `activate_requires`
 * is carried here (the archetype-inherent converter and the main powerset converter
 * both drop it — no other power needs it) so the hero/villain gate survives into the
 * module instead of being re-read from the export downstream.
 *
 * Reuses convert-powerset.cjs's atom/effect helpers verbatim (same encoding as
 * powerset, pool, epic, and inherent powers).
 *
 * Usage:
 *   node scripts/convert-accolades.cjs --dataset homecoming            # write
 *   node scripts/convert-accolades.cjs --dataset homecoming --dry-run  # preview
 */

const fs = require('fs');
const path = require('path');
const {
  extractEffects,
  extractPowerSummon,
  extractDamage,
  normalizeIconPath,
  collectBaseTemplates,
  collectAtomTemplates,
  encodeAtomsForEmit,
  extractConditionalEffects,
  stampConditionalIds,
  resolveThunderspyMovementTargets,
  guardThunderspyOnesBuffs,
  guardThunderspyAppliedMez,
  TARGET_TYPE_MAP,
  EFFECT_AREA_MAP,
  BOOST_TYPE_MAP,
  BIN_BOOST_MAP,
  RAW_DATA_PATH,
  assignModes,
  extractGrantEdges,
} = require('./convert-powerset.cjs');
const { parseDatasetArg, pipelinePath } = require('./_dataset-paths.cjs');
const { helpText } = require('./_display-text.cjs');
const { gateTokens } = require('./_gate-tokens.cjs');
const { powerStats } = require('./_power-stats.cjs');

const datasetId = parseDatasetArg();
const dryRun = process.argv.includes('--dry-run');

// Raw accolade powers live under `<RAW_DATA_PATH>/temporary_powers/accolades/`.
const RAW_ACCOLADE_PATH = path.join(RAW_DATA_PATH, 'temporary_powers', 'accolades');
const OUTPUT_PATH = pipelinePath(datasetId, 'accolades.json');

/**
 * Convert one raw accolade power → the composed Power shape. Mirrors
 * convert-inherents.cjs `convertInherentPower`; accolades additionally carry
 * `activateRequires` (the hero/villain gate) and, like inherents, no rank/slot metadata.
 */
function convertAccoladePower(rawJson) {
  const power = {};

  power.name = rawJson.display_name || rawJson.name;
  // `internalName` is the resolution identity the build's SelectedPower and any
  // downstream lookup key on (`The_Atlas_Medallion`, `Freedom_Phalanx_Reserve`, …).
  power.internalName = rawJson.name;
  power.fullName = rawJson.full_name;
  power.available = typeof rawJson.available_level === 'number' ? rawJson.available_level : 0;
  // See the field doc on the archetype converter's `autoIssue`/`free`: the game grants
  // rather than offers a power when AutoIssue passes together with BuyRequires and
  // available <= level (character_base.c:1952). Accolades carry neither, but the engine's
  // grant marker has to be readable on every power in every powerset it walks — an absent
  // field would read as "not granted" without ever saying the export stopped stating it.
  power.autoIssue = rawJson.auto_issue === true;
  power.free = rawJson.free === true;

  power.description = helpText(rawJson.display_help) || '';
  if (rawJson.display_short_help) {
    power.shortHelp = rawJson.display_short_help.replace(/\u00A0/g, ' ');
  }
  power.icon = normalizeIconPath(rawJson.icon || '');
  power.powerType = rawJson.type || 'Auto';

  // The same call the pool, epic and inherent converters make, for the reason `assignModes`
  // states: a power's mode gating must not depend on which tree converted it. Accolades were
  // the fourth tree and the one still missing it, so the Labyrinth of Fog pair — the only
  // accolades the game gates with `modes_required` — published no mode at all and read as
  // permanent buffs.
  assignModes(power, rawJson);

  if (rawJson.target_type) {
    const mapped = TARGET_TYPE_MAP[rawJson.target_type];
    if (mapped) power.targetType = mapped;
  }

  // Access gate (empty for accolades in the current corpus) and the hero/villain
  // ACTIVATION gate. The main converter drops `activate_requires`, but for accolades
  // it IS the hero↔villain pairing rule, so it is carried through here.
  if (gateTokens(rawJson.requires).length) power.requires = rawJson.requires;
  if (gateTokens(rawJson.activate_requires).length) {
    power.activateRequires = rawJson.activate_requires;
  }

  const enhancements = (rawJson.boosts_allowed || [])
    .map((b) => BOOST_TYPE_MAP[b] || BIN_BOOST_MAP[b])
    .filter(Boolean);
  power.allowedEnhancements = [...new Set(enhancements)].sort();

  // Thunderspy movement-template target-trap (no-op for non-movement powers).
  resolveThunderspyMovementTargets(rawJson);

  // Execution stats and the two power-level display fields, in the shape an archetype power
  // publishes. The legacy `effects` bag copies are gone (atom1-13).
  power.stats = powerStats(rawJson);
  if (rawJson.effect_area && rawJson.effect_area !== 'None') {
    power.effectArea = EFFECT_AREA_MAP[rawJson.effect_area] ?? rawJson.effect_area;
  }

  // `collectBaseTemplates` covers a power's own effects AND its redirect chain; the
  // templates feed the atom list below. The legacy `effects` bag is gone (atom1-13).
  const { templates: allTemplates } = collectBaseTemplates(rawJson);
  if (allTemplates.length > 0) {
    const damage = extractDamage(allTemplates);
    if (damage) {
      power.damage = damage;
    }

    // The pets. This partition reached `extractSummon` through the bag it no longer emits, so
    // the strip took its summons silently — the Accolade markers (Mark of Recall's beacon, the
    // Portable Workbench) among them (ENT-22). Same address as every other partition's.
    const summon = extractPowerSummon(allTemplates, rawJson.name);
    if (summon) power.summon = summon;
  }

  // The conditional→atom join, stamped ahead of the atom emit for the reason
  // convert-powerset.cjs gives at its own copy: `extractConditionalEffects` writes
  // `_conditionalId` onto the surviving groups' templates and `encodeAtomsForEmit` carries it
  // onto the atom, so a stamp placed after the encode reaches nothing. `stampOnly` skips the
  // `_perTargetIncrement` patch and cannot change which groups survive.
  stampConditionalIds(rawJson.effects, rawJson);

  // Plan B atom list: union of the bag's templates with the gated groups
  // `collectAtomTemplates` adds back (stamped `gated: true` by encodeAtomsForEmit).
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

  // Conditional bonus effects (Mechanic Adjusters) — the positive state gates the base
  // collector filters out, surfaced as toggles. Called AFTER the atom emit, because
  // `extractConditionalEffects` stamps `_perTargetIncrement` on the templates it patches and
  // `encodeAtomsForEmit` copies that stamp onto the atom; running it first would put a
  // conditional group's per-foe increment on the base atoms.
  //
  // Shipped none until BRAIN-3, alongside the inherent and basic-inherent trees. Nothing in
  // this partition carries a classifiable gate today, so this call changes no output — it
  // makes the zero a measured one rather than the absence of a capability, which is the
  // distinction audit-conditional-coverage.cjs exists to hold.
  if (rawJson.effects?.length) {
    const conditional = extractConditionalEffects(rawJson.effects, rawJson);
    if (conditional) power.conditionalEffects = conditional;
  }

  // Caster-state writes (grant/revoke edges) — the stamp every partition gets, called
  // explicitly for the audit-grant-edges.cjs reason: an extractor reaches only the
  // converters that ask.
  const grantEdges = extractGrantEdges(rawJson);
  if (grantEdges) power.grantEdges = grantEdges;

  // EntsAffected — who this power's effects can land on, which is what an atom
  // targeting `AnyAffected` means by "the target". See the field doc on
  // `Power.targetsAffected` (DATA-GAP-REGISTER MEZRES-3).
  if (Array.isArray(rawJson.targets_affected) && rawJson.targets_affected.length) {
    power.targetsAffected = rawJson.targets_affected;
  }

  if (datasetId === 'thunderspy') {
    // Both Thunderspy target-trap guards, because the trap is a property of the BINARY and
    // not of which converter read it: the aspect and per-template target the recovered slot
    // needs are missing from every fork-6 power alike. The applied-mez half was called only by
    // convert-powerset.cjs, so Rest, Hibernate and Rise of the Phoenix shipped 15 foe-control
    // slots on self-only powers (TWIN-2).
    // The guards read the effects PROJECTION to decide which slots are Thunderspy target-trap
    // artifacts, then stamp the atoms behind them. The bag is no longer emitted on the power,
    // so it is computed here for the guards alone and thrown away — dropping the computation
    // along with the emission is what silently retired both guards and let the trapped rows
    // reach the engine on the atoms (TSPY-10, and it re-opened TWIN-2's widening).
    const guardBag = extractEffects(allTemplates, rawJson.name, rawJson.targets_affected);
    guardThunderspyOnesBuffs(power, rawJson.targets_affected, guardBag);
    guardThunderspyAppliedMez(power, rawJson.targets_affected, guardBag);
  }

  return power;
}

// `Temporary_Powers.Accolades.The_Atlas_Medallion` → `the_atlas_medallion` (the file stem
// the exporter writes). Lower-cased last segment, hyphens preserved (`Crey_CBX-9_Pistol`).
function fileStem(fullName) {
  return fullName.split('.').pop().toLowerCase();
}

function main() {
  console.log(`=== CONVERT ACCOLADES (dataset: ${datasetId})${dryRun ? ' [DRY RUN]' : ''} ===\n`);

  const indexFile = path.join(RAW_ACCOLADE_PATH, 'index.json');
  if (!fs.existsSync(indexFile)) {
    console.warn(`No accolades index at ${indexFile} — skipping.`);
    return;
  }

  const index = JSON.parse(fs.readFileSync(indexFile, 'utf-8'));
  const members = Array.isArray(index.powers) ? index.powers : [];

  const powers = [];
  const missing = [];
  for (const fullName of members) {
    const file = path.join(RAW_ACCOLADE_PATH, `${fileStem(fullName)}.json`);
    if (!fs.existsSync(file)) {
      // A powerset member index.json declares but the export never wrote is a real
      // extraction gap — surface it loud, never drop it silently.
      missing.push(fullName);
      continue;
    }
    const rawJson = JSON.parse(fs.readFileSync(file, 'utf-8'));
    const power = convertAccoladePower(rawJson);
    powers.push(power);
    const atomCount = Array.isArray(power.atoms) ? power.atoms.length : 0;
    console.log(`  ${power.internalName.padEnd(28)} atoms=${atomCount}`);
  }

  if (missing.length) {
    console.warn(`\n  WARNING: ${missing.length} declared member(s) had no export file: ${missing.join(', ')}`);
  }

  if (!index.key) {
    throw new Error(`${indexFile}: missing key (the binary set path)`);
  }

  const powerset = {
    id: 'Accolades',
    setPath: index.key,
    name: index.display_name || 'Accolades',
    archetype: 'accolade',
    category: 'accolade',
    powers,
  };

  const totalAtoms = powers.reduce((n, p) => n + (Array.isArray(p.atoms) ? p.atoms.length : 0), 0);
  // The Temporary_Powers.Accolades members, read out of
  // exported_powers/<ds>/temporary_powers/accolades/ as ordinary auto-on/gated Powers.
  // `ACCOLADES_POWERSET` is the name the contract emitter destructures.
  const out = JSON.stringify({ ACCOLADES_POWERSET: powerset });

  if (dryRun) {
    console.log(`\nWould write ${OUTPUT_PATH} (${powers.length} powers, ${totalAtoms} atoms)`);
    return;
  }
  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, out);
  console.log(`\nWrote ${OUTPUT_PATH} (${powers.length} powers, ${totalAtoms} atoms)`);
}

main();
