/**
 * Convert archetype-inherent powers from raw game JSON → the composed `Inherent`
 * powerset the contract ships.
 *
 * The archetype inherents (Defender Vigilance, Controller Containment, Scrapper
 * Critical Hit, …) are ordinary auto-hit self powers in the game data
 * (`exported_powers/<ds>/inherent/inherent/<name>.json`), each gated
 * `requires: "$archetype @Class_<AT> =="`. Extraction previously DROPPED them —
 * the contract carried only the inherent's NAME string (in archetypes.json), not
 * its atoms — so the calc had to invent their values (the `crate::inherents`
 * stopgap for Vigilance/Fury; DATA-GAP INHERENT-2). This converter closes that gap:
 * it emits each archetype's declared inherent as a real Power (tuple `atoms` + the
 * transitional `effects` bag), so the Rust calc reads the mechanic from the data.
 *
 * WHICH powers: exactly the inherent each PLAYER archetype DECLARES
 * (`ARCHETYPES[at].inherent.name`), one per AT — NOT every `$archetype`-gated file
 * in the raw dir (that set also carries deprecated twins (`Defiance_old`,
 * `BlasterOLD`), non-player classes (`Rescued_Devoured`), and meter/dampen/buff
 * sub-powers). Keying off the archetype's declared name picks the canonical power
 * (Opportunity/Resolve over Vulnerability, Domination over Domination_Meter, …).
 * PLUS the pick-gated grant closure (see the main() comment): autoIssue members
 * whose gate reads the build's own picks, and the sub-powers those hand out —
 * the members the grant reconcile walks (AUTOISSUE-1's Thunderspy half).
 * PLUS archetype-gated grants — autoIssue members whose gate reads only the
 * build's ARCHETYPE (AUTOISSUE-2). A fork can move an archetype's signature
 * power out of its powerset into here and gate it `$archetype @Class_<AT> ==`;
 * Thunderspy does exactly that with the Stalker's Hide and Placate, the
 * Mastermind's Hold Ground and eight Kheldian travel powers.
 *
 * Reuses convert-powerset.cjs's atom/effect helpers verbatim (same encoding as
 * powerset + pool powers), mirroring convert-pool-powers.cjs's convertPoolPower.
 *
 * Usage:
 *   node scripts/convert-inherents.cjs --dataset homecoming            # write
 *   node scripts/convert-inherents.cjs --dataset homecoming --dry-run  # preview
 */

const fs = require('fs');
const path = require('path');
const {
  assignModes,
  extractEffects,
  extractDamage,
  extractGrantEdges,
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
  _readPowerFile,
} = require('./convert-powerset.cjs');
// No `_oracle-modules.cjs` import any more, and so no `tsx/cjs`: this converter reads
// nothing out of `src/` and runs on plain Node.
const { parseDatasetArg, pipelinePath, REPO_ROOT } = require('./_dataset-paths.cjs');
const { forEachPowersetPower, powersetRoot } = require('./_powerset-tree.cjs');
const { helpText } = require('./_display-text.cjs');
const { derivePlayerArchetypes } = require('./_player-classes.cjs');
const { gateTokens, gateText } = require('./_gate-tokens.cjs');
const { powerStats } = require('./_power-stats.cjs');

const datasetId = parseDatasetArg();
const dryRun = process.argv.includes('--dry-run');

// Raw archetype-inherent powers live under `<RAW_DATA_PATH>/inherent/inherent/`.
const RAW_INHERENT_PATH = path.join(RAW_DATA_PATH, 'inherent', 'inherent');
const OUTPUT_PATH = pipelinePath(datasetId, 'inherents.json');

// AT id → raw file stem, for the archetypes whose declared `inherent.name` does
// NOT match its raw filename. Data-sourced exceptions, kept minimal:
//   arachnos-soldier / -widow both declare "Conditioning" but the raw powers are
//   AT-specific (`spider_conditioning` / `widow_conditioning`).
//   brute "Fury": the archetype declares inherent name "Fury", but there is no
//   fury.json — the Brute damage mechanic is implemented by `rage_buff.json`, an
//   `Auto` power gated `$archetype @Class_Brute ==` whose `display_name` IS "Fury".
//   Its eight damage-type atoms carry `magnitude_expression: "kRage source> .02 *"`
//   (2% damage-Strength per Rage point), so extracting it lets the calc DERIVE Fury
//   from the data (INHERENT-2 Fury follow-on) instead of a hardcoded constant. NB the
//   Rebirth/Thunderspy exports DROP that expression (their `Rage_Buff` damage atoms
//   arrive value-less), so Fury is not derivable there — a recorded data gap the calc
//   surfaces loud, never a silent fallback.
const RAW_FILE_ALIASES = {
  'arachnos-soldier': 'spider_conditioning',
  'arachnos-widow': 'widow_conditioning',
  brute: 'rage_buff',
};

// Archetypes whose inherent is deliberately NOT extracted. Now empty — Fury, the last
// holdout, is extracted via the `rage_buff` alias above once the `magnitudeExpression`
// atom field landed. The Set stays as the extension point for any future inherent whose
// raw source cannot yet round-trip.
const DEFERRED_ARCHETYPES = new Set();

/** Map an archetype id + declared inherent name to its raw file stem. */
function rawFileStem(archetypeId, inherentName) {
  if (RAW_FILE_ALIASES[archetypeId]) return RAW_FILE_ALIASES[archetypeId];
  return inherentName.toLowerCase().replace(/\s+/g, '_');
}

/**
 * Convert one raw inherent power → the composed Power shape. Mirrors
 * convert-pool-powers.cjs `convertPoolPower` (the atom/effect half is identical);
 * inherents carry no rank/slot metadata, so those fields are dropped.
 */
function convertInherentPower(rawJson) {
  const power = {};

  power.name = rawJson.display_name || rawJson.name;
  // `internalName` is the resolution identity the build's SelectedPower and the
  // Rust gather/Pass-3 lookup key on (`Vigilance`, `Critical_Hit`, …).
  power.internalName = rawJson.name;
  power.fullName = rawJson.full_name;
  power.available = typeof rawJson.available_level === 'number' ? rawJson.available_level : 0;
  // See the field doc on the archetype converter's `autoIssue`/`free`: the game
  // grants rather than offers a power when AutoIssue passes together with
  // BuyRequires and available <= level (character_base.c:1952).
  power.autoIssue = rawJson.auto_issue === true;
  power.free = rawJson.free === true;

  power.description = helpText(rawJson.display_help) || '';
  if (rawJson.display_short_help) {
    power.shortHelp = rawJson.display_short_help.replace(/\u00a0/g, ' ');
  }
  power.icon = normalizeIconPath(rawJson.icon || '');
  power.powerType = rawJson.type || 'Auto';
  // The same call the pool and epic converters make, for the reason `assignModes` states: a
  // power's mode gating must not depend on which tree converted it. This set was the one that
  // never made it, so its members published no mode at all — and Thunderspy authors its Swap
  // Ammo family HERE rather than in Dual Pistols, which meant the fork's ammo toggles could
  // not publish the minted tokens their own carriers were gated on (COND-10).
  assignModes(power, rawJson);

  if (rawJson.target_type) {
    const mapped = TARGET_TYPE_MAP[rawJson.target_type];
    if (mapped) power.targetType = mapped;
  }

  // The `$archetype @Class_<AT> ==` gate — carried through so the app/calc can see
  // WHICH archetype owns the inherent (Pass 3 gates on the AT to match the beta).
  power.requires = gateTokens(rawJson.requires);

  const enhancements = (rawJson.boosts_allowed || [])
    .map((b) => BOOST_TYPE_MAP[b] || BIN_BOOST_MAP[b])
    .filter(Boolean);
  power.allowedEnhancements = [...new Set(enhancements)].sort();

  // Thunderspy movement-template target-trap (no-op for non-movement inherents).
  resolveThunderspyMovementTargets(rawJson);

  // Execution stats and the two power-level display fields, in the shape an archetype power
  // publishes. The legacy copies under `effects` below stay until the bag's execution keys are
  // deleted (atom-migration, display item job 2).
  power.stats = powerStats(rawJson);
  if (rawJson.effect_area && rawJson.effect_area !== 'None') {
    power.effectArea = EFFECT_AREA_MAP[rawJson.effect_area] ?? rawJson.effect_area;
  }

  const effects = {};
  if (rawJson.accuracy) effects.accuracy = rawJson.accuracy;
  if (rawJson.recharge_time) effects.recharge = rawJson.recharge_time;
  if (rawJson.endurance_cost) effects.endurance = rawJson.endurance_cost;
  if (rawJson.activation_time) effects.activationTime = rawJson.activation_time;
  if (rawJson.activate_period) effects.activatePeriod = rawJson.activate_period;
  if (rawJson.effect_area && rawJson.effect_area !== 'None') {
    effects.effectArea = EFFECT_AREA_MAP[rawJson.effect_area] ?? rawJson.effect_area;
  }

  // Extract effects + atoms exactly as convert-pool-powers.cjs does (shared helpers).
  // `collectBaseTemplates` covers a power's own effects AND its redirect chain.
  const { templates: allTemplates } = collectBaseTemplates(rawJson);
  if (allTemplates.length > 0) {
    const damage = extractDamage(allTemplates);
    if (damage) {
      power.damage = damage;
      effects.damage = damage;
    }
    const extracted = extractEffects(allTemplates, rawJson.name, rawJson.targets_affected);
    for (const [key, value] of Object.entries(extracted)) effects[key] = value;
  }

  // The conditional→atom join, stamped ahead of the atom emit for the reason
  // convert-powerset.cjs gives at its own copy: `extractConditionalEffects` writes
  // `_conditionalId` onto the surviving groups' templates and `encodeAtomsForEmit` carries it
  // onto the atom, so a stamp placed after the encode reaches nothing. `stampOnly` skips the
  // `_perTargetIncrement` patch and cannot change which groups survive.
  stampConditionalIds(rawJson.effects, rawJson);

  // Plan B atom list: union of the bag's templates with the gated groups
  // `collectAtomTemplates` adds back (stamped `gated: true` by encodeAtomsForEmit).
  // Vigilance's three `0.0 source.TeamSize> N >` team-size steps arrive here.
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

  // The bag is NOT emitted — see the same note in convert-pool-powers.cjs. `effects` stays a
  // local for the two Thunderspy guards and for lifting `summon` out; `power.stats` and the
  // atoms carry everything it held. REBUILD-PROGRESS `effects` bag removal, writer side.

  // The pets, at the address every partition now answers on. A reader that has to ask
  // `power.summon ?? power.effects.summon` is two rules, and which one answers would depend on
  // which partition the power came from, which is the shape ENT-22 was. One writer, one address.
  if (effects.summon) power.summon = effects.summon;

  // Conditional bonus effects (Mechanic Adjusters) — the positive state gates the base
  // collector filters out, surfaced as toggles. Called AFTER the atom emit, because
  // `extractConditionalEffects` stamps `_perTargetIncrement` on the templates it patches and
  // `encodeAtomsForEmit` copies that stamp onto the atom; running it first would put a
  // conditional group's per-foe increment on the base atoms.
  //
  // Shipped none until BRAIN-3, alongside the basic-inherent and accolade trees. Nothing in
  // this partition carries a classifiable gate today, so this call changes no output — it
  // makes the zero a measured one rather than the absence of a capability, which is the
  // distinction audit-conditional-coverage.cjs exists to hold.
  if (rawJson.effects?.length) {
    const conditional = extractConditionalEffects(rawJson.effects, rawJson);
    if (conditional) power.conditionalEffects = conditional;
  }

  // Caster-state writes (grant/revoke edges) — the same stamp every other partition gets,
  // called explicitly for the audit-form-coverage reason: a shared extractor reaches only
  // the converters that ASK (audit-grant-edges.cjs found this partition silent on its
  // first run — Gauntlet, Defiance, the Bio adaptations, the Perfection forms).
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
    // Passed explicitly now that the bag is not on the power: both default their third argument
    // to `power.effects`, and a guard handed `undefined` returns without stamping
    // `notOnCaster` onto the atoms, which is the half that outlives the bag.
    guardThunderspyOnesBuffs(power, rawJson.targets_affected, effects);
    guardThunderspyAppliedMez(power, rawJson.targets_affected, effects);
  }

  return power;
}

function main() {
  console.log(`=== CONVERT INHERENTS (dataset: ${datasetId})${dryRun ? ' [DRY RUN]' : ''} ===\n`);

  if (!fs.existsSync(RAW_INHERENT_PATH)) {
    console.warn(`No raw inherent dir at ${RAW_INHERENT_PATH} — skipping.`);
    return;
  }

  // This was the last oracle read in this file, and it is now gone. It waited on a cycle:
  // `pipeline/<id>/archetypes.json` used to be written by `emit-pipeline-json.cjs`, which runs
  // LAST in the regen, so reading it here would have read the previous run's file. That is no
  // longer true — `convert-archetype-registry.cjs` writes it, several steps ahead of this one.
  //
  // Read in REGISTRY order, not the sorted order `hand-data/<id>/archetypes.json` stores, because
  // the order archetypes are walked in here is the order their powers land in the output array.
  const ARCHETYPES_PATH = pipelinePath(datasetId, 'archetypes.json');
  if (!fs.existsSync(ARCHETYPES_PATH)) {
    throw new Error(
      `no archetype registry for ${datasetId}:\n  ${path.relative(REPO_ROOT, ARCHETYPES_PATH)} does not exist.\n`
      + `  Run: node scripts/convert-archetype-registry.cjs --dataset ${datasetId}`,
    );
  }
  const ARCHETYPES = JSON.parse(fs.readFileSync(ARCHETYPES_PATH, 'utf8')).ARCHETYPES;
  const powers = [];
  const skipped = [];

  for (const [archetypeId, at] of Object.entries(ARCHETYPES)) {
    const inherentName = at.inherent && at.inherent.name;
    if (!inherentName) continue;
    if (DEFERRED_ARCHETYPES.has(archetypeId)) {
      skipped.push(`${archetypeId}/${inherentName} (deferred)`);
      continue;
    }
    const stem = rawFileStem(archetypeId, inherentName);
    const file = path.join(RAW_INHERENT_PATH, `${stem}.json`);
    if (!fs.existsSync(file)) {
      skipped.push(`${archetypeId}/${inherentName} (no ${stem}.json)`);
      continue;
    }
    // Through the gate-stamping reader (COND-12): the inherent flight movers live here.
    const rawJson = _readPowerFile(file);
    const power = convertInherentPower(rawJson);
    powers.push(power);
    const atomCount = Array.isArray(power.atoms) ? power.atoms.length : 0;
    console.log(`  ${archetypeId.padEnd(18)} ${power.internalName.padEnd(20)} atoms=${atomCount}`);
  }

  // Pick-gated grants (DATA-GAP-REGISTER AUTOISSUE-1, the traversal half).
  // `character_GrantAutoIssuePowers` walks Inherent.Inherent like any owned set, so a
  // member whose gate reads the BUILD'S OWN PICKS is a grant the planner's reconcile
  // must reach — Thunderspy authors all twenty Kheldian form attacks this way, plus
  // the Swap Ammo / Staff Mastery / Evolution stance parents and the Bane/Widow
  // Placates. Two structural rules, no power names:
  //   (a) an autoIssue member whose gate names a power OUTSIDE this powerset — a
  //       pick-dependent grant. Members whose only dotted references stay inside
  //       the set are the account-product idiom (prestige pets, booster packs:
  //       `productOwned?`/`auth>` beside a self-reference) — account state, not
  //       build state, and the temp-power family the planner deliberately omits.
  //   (b) an autoIssue member whose gate names an already-selected member — the
  //       sub-powers a stance parent from (a) hands out (ammo types, staff forms,
  //       adaptations). Grown to a fixpoint so a parent's chain arrives whole.
  //   (c) an autoIssue member whose gate names only the build's ARCHETYPE
  //       (AUTOISSUE-2) — see `archetypeGranted` below for the four terms and
  //       why each is there.
  const emitted = new Set(powers.map((p) => p.internalName.toLowerCase()));
  const setId = 'Inherent';
  const powerTokens = (requires) =>
    gateTokens(requires)
      .filter((t) => t.includes('.') && !t.startsWith('@') && !t.startsWith('$'));
  const ownSetToken = (token) => {
    const segs = token.split('.');
    const setSeg = segs.length >= 3 ? segs[segs.length - 2] : segs[0];
    return setSeg.toLowerCase() === setId.toLowerCase();
  };
  const rawMembers = fs
    .readdirSync(RAW_INHERENT_PATH)
    .filter((f) => f.endsWith('.json'))
    .sort()
    .map((f) => _readPowerFile(path.join(RAW_INHERENT_PATH, f)));

  // Temporary_Powers is the other set every character owns, so the same walk issues
  // its auto members too. The exporter brings over exactly the ones gated on holding
  // a player power (Personal Force Field's passive, `Personal_Force_Field_Auto`), and
  // only rule (a) applies to them: the rest of that directory is Grant_Power hosts the
  // powers that grant them already resolve. They join this set because it is the one
  // the grant reconcile walks; the gate, not the set, is what decides who holds them.
  // A `GlobalBoost` is skipped: it is no power a build holds or switches on, and the
  // corpus's one (Leviathan Force) carries no effects.
  const RAW_TEMPORARY_PATH = path.join(RAW_DATA_PATH, 'temporary_powers', 'temporary_powers');
  const temporaryMembers = fs.existsSync(RAW_TEMPORARY_PATH)
    ? fs
        .readdirSync(RAW_TEMPORARY_PATH)
        .filter((f) => f.endsWith('.json') && f !== 'index.json')
        .sort()
        .map((f) => _readPowerFile(path.join(RAW_TEMPORARY_PATH, f)))
        .filter((raw) => raw.auto_issue && raw.type !== 'GlobalBoost')
        .filter((raw) => powerTokens(raw.requires).some((t) => !ownSetToken(t)))
    : [];

  // Rule (c), AUTOISSUE-2. Rules (a) and (b) both key on a dotted POWER token in
  // the gate; `$archetype @Class_Stalker ==` names none, so a power a fork moved
  // out of its powerset into here and gated on the archetype alone matched
  // neither and was dropped. On Thunderspy that is the Stalker's Hide and
  // Placate, the Mastermind's Hold Ground and eight Kheldian travel powers —
  // reachable from no screen at all, because the powerset they left kept their
  // internal name for a DIFFERENT power.
  //
  // Four terms, all structural — no power names, so a fork that moves another
  // power the same way is picked up with no edit here:
  //
  //      Mutation-tested 2026-08-12: dropping term 2 loses Hold Ground, and
  //      keying term 3 on internal names instead of display names loses Hide,
  //      Placate and Group Energy Flight. Term 1 changes nothing on today's
  //      corpus — terms 2 and 3 already exclude everything it would — so it is
  //      defensive, not load-bearing, and should not be read as proven.
  //
  //   1. the gate names at least one archetype class and NOTHING BUT player
  //      archetype classes, per this dataset's own class catalogue. Keeps out
  //      the NPC classes (`Rescued_Devoured`, `Hyper-Advanced_Clockwork`) and
  //      the dead legacy variants (`Class_BlasterOLD`'s Defiance copies) that
  //      share this directory.
  //   2. slottable, or a `Toggle`. An unslottable `Auto` here is either engine
  //      bookkeeping (`Domination_Meter`, `Rage_Dampen`,
  //      `Vigilance_PerTeamEndAdjustment`) or the archetype's headline
  //      inherent — and the headline one is already emitted above, by name,
  //      from `ARCHETYPES[at].inherent`. A `Toggle` is neither: it is something
  //      the player switches on, so it has to arrive whether or not it slots.
  //      Thunderspy's `Hold_Ground` is the only one across all three forks.
  //
  //      That either/or presumes the by-name emission FIRED, and for one
  //      archetype it does not: Thunderspy's Primalist declares `Primal Energy`
  //      and the set holds no `primal_energy.json`, only `Primal_Energy_Meter`
  //      and `Primal_Energy_Dampen` (INHERENT-10). Term 2 still drops both, and
  //      still correctly — they are the bookkeeping half, not a headline wearing
  //      its clothes. The Primalist's resource is granted and spent by its own
  //      powersets (47 `Rage` atoms across Feral Might and Primal Gifts: +0.1..
  //      +0.3 `Current` on the attacks, -100 `Absolute` on the spenders), the
  //      meter only reads it back (`Meter` attrib, `kRage source> .01 *`,
  //      IgnoreStrength+IgnoreResistance, reaching no total) and the dampen only
  //      decays it. So the ground is the powers' own shape, not the presumption
  //      above — and `unbackedArchetypes` below states the consequence rather
  //      than leaving it as the residue of a missing filename.
  //   3. no powerset in this dataset already DISPLAYS that name. Display name,
  //      not internal name, and the distinction is the whole ballgame: twelve
  //      Thunderspy Stalker secondaries carry `internalName: "Hide"`, each
  //      showing a different power (Quick Recovery, Umbral Fade, EMP,
  //      Hibernate, …), while NO powerset on that fork displays the name
  //      "Hide". An internal-name check reproduces the very collision this
  //      rule exists to see past.
  //   4. not already emitted — the shared guard at the top of the loop.
  //
  // Homecoming and Rebirth admit zero under this rule: they still grant these
  // from powersets, so term 3 rejects them. That is what makes the rule safe to
  // run unconditionally rather than behind a fork flag.
  const playerClasses = new Set(derivePlayerArchetypes(path.join(RAW_DATA_PATH, 'tables')));
  const powersetDisplayNames = (() => {
    const names = new Set();
    forEachPowersetPower(datasetId, (power) => {
      if (typeof power.name === 'string') names.add(power.name.toLowerCase());
    });
    if (!names.size) {
      throw new Error(
        `No converted powersets under ${powersetRoot(datasetId)}. Rule (c) reads them to tell a ` +
          'moved power from one the powerset layer still shows, so an empty read would emit ' +
          'duplicates of every archetype-gated member. Run convert-all-powersets.cjs first.',
      );
    }
    return names;
  })();
  const archetypeGranted = (rawJson) => {
    const classes = [...gateText(rawJson.requires).matchAll(/@Class_([A-Za-z0-9_-]+)/g)].map(
      (m) => m[1].toLowerCase()
    );
    if (!classes.length || !classes.every((c) => playerClasses.has(c))) return false; // 1
    const slottable = Array.isArray(rawJson.boosts_allowed) && rawJson.boosts_allowed.length > 0;
    if (!slottable && rawJson.type !== 'Toggle') return false; // 2
    return !powersetDisplayNames.has(String(rawJson.display_name).toLowerCase()); // 3
  };

  // Slot ceiling, reading an explicit `max_boosts: 0` as the zero it is rather than
  // folding it into the 6-slot default the way a `||` would.
  //
  // The read is literal for every member (MAXBOOST-1). The binary has no absent state:
  // the game's parse table stamps 6 when the authored def says nothing (powers_load.c
  // `TOK_INT(BasePower, iMaxBoosts, 6)`), our exporter omits exactly the value 6, and the
  // engine enforces the stored value as-is (character_BuyBoostSlot) — so export silence
  // decodes to 6 and a stated 0 means the power takes no slots. Thunderspy's Kheldian
  // form attacks state 0 where the other forks' binaries state 6; that split is authored
  // (the same Thunderspy set states 6 on Hide, Sprint and the Kheldian travel powers),
  // and it matches the bug report that started this work ("Hide can be slotted, placate
  // cannot").
  const grantedMaxSlots = (rawJson) => {
    if (!Array.isArray(rawJson.boosts_allowed) || !rawJson.boosts_allowed.length) return 0;
    return rawJson.max_boosts === undefined || rawJson.max_boosts === null
      ? 6
      : rawJson.max_boosts;
  };
  for (const rawJson of temporaryMembers) {
    if (emitted.has((rawJson.name || '').toLowerCase())) continue;
    const power = convertInherentPower(rawJson);
    power.maxSlots = grantedMaxSlots(rawJson);
    powers.push(power);
    emitted.add(power.internalName.toLowerCase());
    const atomCount = Array.isArray(power.atoms) ? power.atoms.length : 0;
    console.log(`  ${'owner-issued'.padEnd(18)} ${power.internalName.padEnd(20)} atoms=${atomCount}`);
  }
  let grew = true;
  while (grew) {
    grew = false;
    for (const rawJson of rawMembers) {
      if (!rawJson.auto_issue || emitted.has((rawJson.name || '').toLowerCase())) continue;
      const tokens = powerTokens(rawJson.requires);
      const pickGated = tokens.some((t) => !ownSetToken(t));
      const handedByEmitted = tokens.some(
        (t) => ownSetToken(t) && emitted.has(t.split('.').pop().toLowerCase())
      );
      // (c) is evaluated last and only when (a)/(b) miss, so it can never
      // loosen them — a member they already claim keeps its existing kind.
      const archetypeGated = !pickGated && !handedByEmitted && archetypeGranted(rawJson);
      if (!pickGated && !handedByEmitted && !archetypeGated) continue;
      const power = convertInherentPower(rawJson);
      power.maxSlots = grantedMaxSlots(rawJson);
      powers.push(power);
      emitted.add(power.internalName.toLowerCase());
      grew = true;
      const atomCount = Array.isArray(power.atoms) ? power.atoms.length : 0;
      const kind = pickGated ? 'pick-gated' : handedByEmitted ? 'handed-by-parent' : 'archetype-gated';
      console.log(`  ${kind.padEnd(18)} ${power.internalName.padEnd(20)} atoms=${atomCount}`);
    }
  }

  // De-duplicate by internalName: SoA + Widow both declare "Conditioning" but
  // resolve to different files, so no collision; a genuine duplicate would mean a
  // raw-data surprise worth surfacing rather than silently keeping one.
  const seen = new Set();
  for (const p of powers) {
    if (seen.has(p.internalName)) {
      console.warn(`  WARNING: duplicate internalName ${p.internalName} — keeping both.`);
    }
    seen.add(p.internalName);
  }

  if (skipped.length) console.log(`\n  Skipped: ${skipped.join(', ')}`);

  // Which archetypes this fork declares an inherent for and backs with no power
  // (INHERENT-10). Derived from what was just emitted, by re-asking the question the
  // RUNTIME asks — `getArchetypeInherentPowerFor`'s name-then-`@Class_`-gate resolve —
  // rather than from the stem lookup that failed at the top. Two reasons that matters:
  // the runtime's answer is the one every consumer sees, and a power that arrives by
  // another door (rule (c)) is a backing the stem miss cannot see.
  //
  // Emitted rather than logged because an absence nobody states is indistinguishable
  // from a lookup that quietly missed, which is the whole of Rule 1. The card, the .mbd
  // writer and the naming sweep can all now tell "this fork backs this declaration with
  // nothing" from "the join failed", and a fork that later ships the power empties the
  // list instead of silently agreeing with it.
  const classGateOf = (power) => {
    const gate = new Set();
    for (const m of (power.requires || []).join(' ').matchAll(/@Class_([A-Za-z_]+)/g)) {
      gate.add(m[1].toLowerCase());
    }
    return gate;
  };
  const unbackedArchetypes = [];
  for (const [archetypeId, at] of Object.entries(ARCHETYPES)) {
    const declaredName = at.inherent && at.inherent.name;
    if (!declaredName) continue;
    const named = powers.filter((p) => p.name === declaredName);
    const wanted = archetypeId.toLowerCase().replace(/-/g, '_');
    const resolved = named.length <= 1 ? named : named.filter((p) => classGateOf(p).has(wanted));
    if (resolved.length !== 1) unbackedArchetypes.push({ archetypeId, declaredName });
  }
  if (unbackedArchetypes.length) {
    console.log(`\n  Declared, backed by no power: ${unbackedArchetypes
      .map((u) => `${u.archetypeId}/${u.declaredName}`)
      .join(', ')}`);
  }

  // The set's own binary path, read from the set the members come out of rather than
  // spelled here — `Inherent.Inherent` is what every gate naming this set writes.
  const setIndexPath = path.join(RAW_INHERENT_PATH, 'index.json');
  const setIndex = JSON.parse(fs.readFileSync(setIndexPath, 'utf-8'));
  if (!setIndex.key) {
    throw new Error(`${setIndexPath}: missing key (the binary set path)`);
  }

  const powerset = {
    id: 'Inherent',
    setPath: setIndex.key,
    name: 'Inherent',
    archetype: 'inherent',
    category: 'inherent',
    powers,
  };

  const totalAtoms = powers.reduce((n, p) => n + (Array.isArray(p.atoms) ? p.atoms.length : 0), 0);
  /*
   * THE TWO KEYS THIS WRITES — `pipeline/<id>/inherents.json`.
   *
   * `INHERENT_POWERSET`: each player archetype's declared inherent
   * (ARCHETYPES[at].inherent.name), read out of exported_powers/<ds>/inherent/inherent/
   * as an ordinary Power. `emit-contract.cjs` merges it into the contract's powersets
   * under the id "Inherent".
   *
   * `ARCHETYPES_WITHOUT_INHERENT_POWER`: the archetypes this fork DECLARES an inherent for
   * and ships no power behind (INHERENT-10). Empty on a fork where every declaration
   * resolves. It is the same resolve `getArchetypeInherentPowerFor` runs, precomputed — so
   * it is NOT independent evidence of anything, and cannot disagree with the runtime. What
   * it buys is a BOUNDED list: a second archetype falling into this state changes this
   * value, and the test that names its members reds. Whether the state is correct for a
   * given archetype is a verdict, not a derivation — the Primalist's is guarded by the
   * emptiness of its class gate.
   */
  const out = JSON.stringify({
    INHERENT_POWERSET: powerset,
    ARCHETYPES_WITHOUT_INHERENT_POWER: unbackedArchetypes,
  });

  if (dryRun) {
    console.log(`\nWould write ${OUTPUT_PATH} (${powers.length} powers, ${totalAtoms} atoms)`);
    return;
  }
  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, out);
  console.log(`\nWrote ${OUTPUT_PATH} (${powers.length} powers, ${totalAtoms} atoms)`);
}

main();
