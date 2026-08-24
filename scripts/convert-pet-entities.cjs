/**
 * Convert Pet Entity & Power Data
 *
 * Reads entity files + their power files from raw data,
 * outputs a TypeScript data module with pet abilities for damage calculation.
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, dataPath, datasetPath } = require('./_dataset-paths.cjs');
const { isPvpOnlyGroup: isPvpOnlyByRequires } = require('./_pv-scope.cjs');
const { gateText } = require('./_gate-tokens.cjs');
// The atomizer: the same collectors + encoder the powerset converter runs, so a pet
// ability's atoms are minted by the identical code path (window_slots' pet-merge census
// grades the atom side against the PetEffect rows it already ships). Requiring the
// powerset converter is side-effect-free past its dataset reads (require.main-guarded
// main), and it shares this script's parseDatasetArg, so the dataset is the same.
const {
  _readPowerFile: readGatedPowerFile,
  collectAtomTemplates,
  encodeAtomsForEmit,
} = require('./convert-powerset.cjs');

const datasetId = parseDatasetArg();

// Read from the per-dataset bin-crawler export — the committed,
// manifest-guarded `exported_powers/` trees (HC at the root, Rebirth and
// Thunderspy in subdirectories). All are organized as
//   <root>/entities/*.json
//   <root>/<powerset_category>/<powerset>/<power>.json
// so we share the same `<root>` for both lookups. (HC previously read the
// gitignored `tools/bin-crawler/exported_powers/live/` local tree, which
// went stale against the committed export.)
const EXPORT_ROOTS = {
  homecoming: path.join(__dirname, '../exported_powers'),
  rebirth: path.join(__dirname, '../exported_powers/rebirth'),
  thunderspy: path.join(__dirname, '../exported_powers/thunderspy'),
  brainstorm: path.join(__dirname, '../exported_powers/brainstorm'),
};
const ROOT = EXPORT_ROOTS[datasetId];
if (!ROOT || !fs.existsSync(ROOT)) {
  throw new Error(`No bin-crawler export root for dataset '${datasetId}'. Looked at ${ROOT}`);
}
const ENTITIES_PATH = path.join(ROOT, 'entities');
const POWERS_PATH = ROOT;

// Absent is a real state (entity power references can point at powers the
// export doesn't carry); a corrupt file must surface, never read as "absent" —
// that silently omits the pet or its power from the committed output.
function readJsonFile(filePath) {
  let text;
  try {
    text = fs.readFileSync(filePath, 'utf-8');
  } catch (err) {
    if (err.code === 'ENOENT') return null;
    throw err;
  }
  try {
    return JSON.parse(text);
  } catch (err) {
    throw new Error(`Corrupt JSON in ${filePath}: ${err.message}`);
  }
}

// pet-entities was migrated into datasets/<id>/pet-entities.ts during the
// first wave of Stage A. Both HC and Rebirth write through datasetPath().
const OUTPUT_PATH = datasetPath(datasetId, 'pet-entities.ts');

// Sidecar JSON of entity_def → lifespan_seconds, consumed by convert-powerset.cjs
// to populate summon.duration when a summoning power's EntCreate AttribMod has
// Duration=0 (the lifespan lives on the pet's bundled Self_Destruct power instead).
// Kept alongside the script so build pipelines that run convert-pet-entities first
// can require() it directly without parsing the generated TS.
const SIDECAR_LIFESPANS_PATH = datasetPath(datasetId, 'pet-lifespans.json');

// Sidecar JSON of fully-qualified Self_Destruct power name → delay seconds.
// Used for pseudopet summons (PL_StaticObject, Vines pseudo-pets) whose
// `params.redirects` array names a `*.Self_Destruct` redirect power — those
// pseudopets aren't backed by a pet entity file, so the entity-keyed sidecar
// can't reach them. Built by scanning every `self_destruct.json` in the bin
// export, regardless of which category it lives in.
const SIDECAR_SELF_DESTRUCT_PATH = datasetPath(datasetId, 'self-destruct-delays.json');

// Sidecar JSON of entity name -> the fully-qualified powers that entity DECLARES
// (`defaults.power_full_names`, verbatim and in export order). Consumed by
// convert-powerset.cjs to answer the one question the summoning power cannot answer
// alone: an `EntCreate` that states no `entity_def` names redirect POWERS instead, and
// the only thing that says which entity those powers belong to is the entity's own
// declaration. Both halves are data, so the join needs no naming convention — which is
// the point, because turning `Villain_Pets.Traps_Poison_Trap.Self_Destruct` into
// `Pets_Traps_Poison_Trap` by string surgery would be inventing one (ENT-16).
//
// The whole index ships rather than the summons-that-need-it subset: which summons need
// it is a fact about the powers tree, and this script does not read the powers tree for
// entity purposes. A filtered index would go quietly wrong the cycle a new power starts
// using the form.
const SIDECAR_ENTITY_POWERS_PATH = datasetPath(datasetId, 'pet-entity-powers.json');

// Damage type attributes we care about
const DAMAGE_ATTRIBS = new Set([
  'smashing_dmg', 'lethal_dmg', 'fire_dmg', 'cold_dmg',
  'energy_dmg', 'negative_energy_dmg', 'toxic_dmg', 'psionic_dmg',
]);

// Thunderspy stores pet damage with a single generic `Damage` attrib on a
// `*_Damage` table (the element lives only in the shortHelp `DMG(...)`), and its
// AttribMod schema DROPS the aspect — so neither the specific-`*_Dmg`-attrib gate
// nor the `aspect === 'Absolute'` gate below can fire. Without a tspy branch every
// melee/attack pet extracts ZERO damage, and ~287 of 619 pets (the pure-attack ones
// like Howler Wolf, Demonlings, Knight Minion) carry no ability at all, so their
// summoning power surfaces only the pet NAME. This mirrors the player-power
// `applyThunderspyDamageType` handling in convert-powerset.cjs.
const _TSPY = datasetId === 'thunderspy';
const _DMG_TYPE_MAP = {
  smash: 'Smashing', smashing: 'Smashing', lethal: 'Lethal', fire: 'Fire',
  cold: 'Cold', energy: 'Energy', negative: 'Negative', 'negative energy': 'Negative',
  psionic: 'Psionic', toxic: 'Toxic', special: 'Special',
};
// Primary damage element from a shortHelp `DMG(...)` clause (e.g.
// "Melee, Light DMG(Lethal)" → "Lethal"); null when absent. Multi-type collapses
// to the primary element, matching the player-power path (element label only —
// the scale/table drive the actual damage math).
function _tspyDamageType(shortHelp) {
  if (!shortHelp) return null;
  // `DoT(...)` as well as `DMG(...)`: a pure damage-over-time pet attack carries
  // its element only in the DoT clause, which the DMG-only match left `Special`.
  const m = shortHelp.match(/D(?:MG|oT)\(([^)]+)\)/i);
  if (!m) return null;
  const first = m[1].split(/[/,]/)[0].trim().toLowerCase();
  return _DMG_TYPE_MAP[first] || null;
}

// Mez/control attributes
const MEZ_ATTRIBS = {
  'sleep': 'Sleep',
  'held': 'Hold',
  'stunned': 'Stun',
  'terrorized': 'Fear',
  'afraid': 'Fear',
  'confused': 'Confuse',
  'immobilized': 'Immobilize',
  'knockback': 'Knockback',
  'knockup': 'Knockup',
  'taunt': 'Taunt',
};

// Debuff attributes (negative effects on targets). The slow family is absent: what a
// movement or recharge attrib IS depends on its aspect and direction, not on its name,
// so `classifySlowFamily` below owns those five.
const DEBUFF_ATTRIBS = {
  'endurance': 'EndDrain',
  'recovery': 'RecoveryDebuff',
  'regeneration': 'RegenDebuff',
  'tohit': 'ToHitDebuff',
  'base_defense': 'DefenseDebuff',
};

/**
 * Which FACE of the stat its attrib names a template moves — the discriminator the attrib
 * name cannot carry (§2).
 *
 * A `Held` at aspect=Resistance is hold-duration RESISTANCE; a NEGATIVE `Held` at
 * aspect=Current is a protection MAGNITUDE (the game's own encoding for mez protection);
 * at aspect=Strength it modifies the target's hold strength. None of the three is a hold,
 * and this route published all three under the applied type until now — so a pet that
 * protects its summoner from knockback advertised knockback, and the mez merge's
 * strongest-instance contest was being entered by rows that apply no mez at all.
 *
 * Both twins already fork this way and this route was the odd one out:
 * `convert-powerset.cjs` sends a parent power's mez to `mezResistance` / `specialBuff` /
 * the mez by the same three aspects, and its `classifyPseudoPetEffect` — the inline
 * pseudo-pet route — emits `<Type>Protection` for the negative-Current face and
 * `<Debuff>Resist` for the resistance face. Those names are this suffix rule applied to
 * the applied type, so both are derived here rather than restated (ENT-9).
 *
 * Scoped to the families whose applied face is Current/Absolute. The slow family is
 * absent because `classifySlowFamily` owns its own aspect fork (Maximum is the movement
 * CAP, and Strength is how the game states a −Recharge), and the damage-type attribs
 * because aspect=Resistance is a resistance DEBUFF there and aspect=Strength a damage
 * buff — `extractDamageStrength` and the tspy resistance branch own those.
 */
const FACE_FORKED_ATTRIBS = new Set([
  ...Object.keys(MEZ_ATTRIBS), ...Object.keys(DEBUFF_ATTRIBS), 'heal_dmg',
]);

/** Mez/knock attribs, where a negative Current scale is protection rather than the effect. */
const PROTECTABLE_ATTRIBS = new Set(Object.keys(MEZ_ATTRIBS));

function templateFace(attribLower, template) {
  if (!FACE_FORKED_ATTRIBS.has(attribLower)) return 'applied';
  // A resistance face states its DIRECTION, and a negative one is a vulnerability — the target
  // resists the effect LESS. Named here rather than left to fall through, because falling
  // through reads it as the applied effect: Rebirth's Disruption Arrow lowers six mez
  // resistances on the foes under it, and 'applied' would publish those as six mezzes it never
  // lands (they published as +40% resistance instead, at `Math.abs(scale)`).
  //
  // The sign carries the direction only on a plain table — `isDebuffTable` exists because a
  // `*_Debuff_*` name carries it too, and then the sign means something else. No face-forked
  // attrib rides a debuff table anywhere in the pet corpus (the 3 negative rows that do are
  // `*_Dmg` attribs, which this fork does not claim), so that case is left undecided rather
  // than guessed: every vulnerability the corpus really ships is on a `*_Ones` table.
  if (template.aspect === 'Resistance') {
    const vulnerability = (template.scale || 0) < 0 && !isDebuffTable(template.table);
    return vulnerability ? 'vulnerability' : 'resistance';
  }
  if (template.aspect === 'Strength') return 'strength';
  if ((template.scale || 0) < 0 && PROTECTABLE_ATTRIBS.has(attribLower)) return 'protection';
  return 'applied';
}

/**
 * The type name a non-applied face publishes under, or `null` for a face with no name.
 *
 * The suffixes reproduce every name the inline route already emits — `EndDrainResist`,
 * `RecoveryDebuffResist`, `RegenDebuffResist`, `HoldProtection`, `KnockbackProtection`, … —
 * by construction, so the two routes agree without either listing the names.
 *
 * The STRENGTH face has no name because it has no meaning in this vocabulary: it modifies
 * how strongly the target applies the stat, which is neither an effect the pet delivers nor
 * a protection it grants (`convert-powerset.cjs` parks the parent-power form in
 * `specialBuff`, which no pet consumer reads). Measured before dropping: every such row in
 * all three forks is on a Lore boss's `Weakening_Shot`, whose entity is commandable and so
 * is never walked by either consumer — 2 rows per attrib per fork, 0 of them reachable.
 *
 * The VULNERABILITY face has none for the same reason, and this route was the odd one out in
 * lacking the fork: the inline twin requires `scale > 0` before it names a resistance and
 * `classifySlowFamily` says so out loud, while this one read the aspect alone and published
 * `Math.abs(scale)` — so a −40% mez resistance shipped as +40% resistance, the sign being the
 * whole difference between the two (25 rows, 12 of them on Rebirth's build-reachable
 * Disruption Arrow). Naming it would need a display key the PARENT route writes, and neither
 * route has one for a −resistance debuff, so it stays out until one exists (register ENT-12).
 */
function facedType(appliedType, face) {
  if (face === 'applied') return appliedType;
  if (face === 'resistance') return `${appliedType}Resist`;
  if (face === 'protection') return `${appliedType}Protection`;
  return null;
}

// The movement axis a speed attrib names. Spelled to match `MOVEMENT_TYPES` in
// convert-powerset.cjs, including the Thunderspy variants, so a pet row and a parent-power
// row land in the same axis of the same `slow` key rather than in two vocabularies.
const MOVEMENT_AXIS = {
  'runningspeed': 'runSpeed', 'speed_running': 'runSpeed', 'speedrunning': 'runSpeed',
  'runspeed': 'runSpeed',
  'flyingspeed': 'flySpeed', 'speed_flying': 'flySpeed', 'speedflying': 'flySpeed',
  'flyspeed': 'flySpeed',
  'jumpingspeed': 'jumpSpeed', 'speed_jumping': 'jumpSpeed', 'speedjumping': 'jumpSpeed',
  'jumpheight': 'jumpHeight',
};

// A debuff states its direction in the SIGN or in the TABLE, and either alone is
// incomplete: a foe -ToHit is stored as a POSITIVE scale on a `*_Debuff_ToHit`
// table, and a -Recharge foe debuff rides a `*_Slow` table. This is the same test
// convert-powerset.cjs applies as `isDebuff = scale < 0 || table.includes('debuff')`,
// factored out here because both `extractBuffAura` and the attrib classification
// below ask it and a drift between them is a sign trap.
function isDebuffTable(table) {
  const lower = (table || '').toLowerCase();
  return lower.includes('debuff') || lower.endsWith('_slow');
}

function isDebuffDirection(template) {
  return (template.scale || 0) < 0 || isDebuffTable(template.table);
}

/**
 * Classify one movement-speed or recharge attrib, which the attrib NAME cannot do alone.
 *
 * The three faces of a slow are three different attributes and the game keeps them apart:
 * a Current/Strength-aspect speed debuff, a Maximum-aspect movement CAP debuff, and a
 * -Recharge. `convert-powerset.cjs` routes a parent power's rows to `slow[axis]`,
 * `movementCapDebuff[axis]` and `rechargeDebuff` for exactly that reason (ENT-5), and this
 * returns the same three so a pet row lands in the key its parent-power twin would.
 *
 * The other two directions are not debuffs at all and previously published as one:
 * aspect=Resistance is protection FROM being slowed (Lore Knives' Rallying Cry), and the
 * buff direction is a +Recharge or +Speed aura (Tech Lab's Haste, whose own tooltip reads
 * "+Recharge, +SPD"). Both showed under a `-Speed` label. Neither has a pet debuff slot,
 * and `extractBuffAura` owns whatever the buff direction is worth.
 *
 * The recharge half of the resistance face DOES have a name — the inline route's
 * `RechargeDebuffResist` — so it is published rather than dropped, which is what stops the
 * two routes disagreeing about the same template. The movement half still has none, as it
 * has none on the inline route either (ENT-9).
 *
 * Returns `null` when the attrib is not in this family or the row is not a debuff.
 */
function classifySlowFamily(attribLower, template) {
  const axis = MOVEMENT_AXIS[attribLower];
  const isRecharge = attribLower === 'rechargetime';
  if (!axis && !isRecharge) return null;
  if (template.aspect === 'Resistance') {
    // Positive only, as the inline route requires: the negative direction of a resistance
    // face is a vulnerability, which neither route has a name for.
    return isRecharge && (template.scale || 0) > 0 ? { type: 'RechargeDebuffResist' } : null;
  }
  if (!isDebuffDirection(template)) return null;
  if (isRecharge) return { type: 'RechargeDebuff' };
  if (template.aspect === 'Maximum') return { type: 'MovementCapDebuff', axis };
  return { type: 'Slow', axis };
}

/**
 * The identity a pet effect is deduplicated on: the §2 discriminators the emitted row
 * carries, not its type name.
 *
 * Keying on the type alone kept one effect per type per power and dropped the rest by
 * template ORDER, so Sentinel Whirlpool published its run-speed CAP kill as its `-Speed`
 * and lost the real 0.4 speed debuff, the 0.3 fly debuff and the -Recharge underneath
 * (ENT-8). Two rows are the same effect only when they agree on every axis that reaches
 * the wire; anything else is a distinct effect the merge is entitled to see.
 */
function effectIdentity(effect, template) {
  // Every field the row carries, not a chosen subset — `defenseTypes`, `resistanceTypes`
  // and `absorbAspect` discriminate as surely as the scale does, and a hand-listed key is
  // how a discriminator gets left out again. `ignoreStrength` is read from the template
  // because it is stamped onto the row after this runs.
  const fields = Object.keys(effect).sort().map((k) => `${k}=${JSON.stringify(effect[k])}`);
  if ((template.flags || []).includes('IgnoreStrength')) fields.push('ignoreStrength=true');
  return fields.join('|');
}

// Thunderspy pet debuff vocabulary. Its Parse6-derived schema names the APPLIED
// attrib directly (Slow, Debuff_Def, DeBuff_ToHit, Res_DMG, …) — NOT the HC
// position/resource attribs DEBUFF_ATTRIBS keys on — AND drops the per-template
// target. So the HC map never fires and every location/patch pseudo-pet
// (Freezing Rain, Sleet, Tar Patch, Caltrops, Ice Slick, Fallout, …) surfaced
// only its damage, dropping the -res / -def / -speed that IS the point of the
// power (the player power carries just Create_Entity; all the debuffs live on
// the summoned pet). Mez already works — tspy's Held/Stunned/Confused/… lower-
// case-match MEZ_ATTRIBS — so this covers only the debuffs. Mirrors the generic-
// `Damage` handling (extractDamage) and the player-power tspy path in
// convert-powerset.cjs (target-drop + sign-trap guards).
//
// Two families, discriminated the way the shipped player path does it (tspy
// drops the aspect, so name + table + sign are the only signals — GAME-DATA §3):
//  • Name-encoded foe debuffs — the attrib name itself carries the debuff, so
//    it is always foe-facing; surfaced at |scale| regardless of stored sign
//    (Caltrops stores Slow +0.8 but SpeedRunning -1.0 — both foe slows). Slow /
//    Speed* route to the app's single movement `Slow` (-Speed) bucket, matching
//    how convert-powerset.cjs classifies a mod on a `*_Slow` table.
//  • Sign-discriminated resource attribs — Res_DMG is +N for a pet SELF-buff
//    (survivability, e.g. blaster_time Res_DMG +2.0) and -N for a foe debuff
//    (Freezing Rain Res_DMG -1.0). Positive = self-buff → dropped (matches HC
//    dropping pet ResistAll self-buffs); negative = the foe debuff we surface.
//    Two of them are further gated to a REAL magnitude table: Thunderspy carries
//    Recovery and Endurance as bare MARKERS on a `*_Ones` placeholder table (the
//    actual -End rides the separate `EndDrain` attrib on a real `*_EndDrain`
//    table) whose scale is not a computable percentage — surfacing those printed
//    a meaningless ~100% (and mislabeled +Recovery ally-buffs like Adrenalin
//    Boost / Guardianship as "-Recovery").
//    That exclusion belongs to those two attribs, not to the `*_Ones` table: for
//    -Regeneration the `*_Ones` scale IS the magnitude, exactly as it is on the
//    Homecoming and Rebirth twins (Poison Gas -10 @Melee_Ones) and on the parent
//    route, where convert-powerset.cjs ships `regenDebuff` on `Ranged_Ones`. A
//    blanket table guard would have dropped every tspy pet -Regen, since all of
//    them ride `*_Ones` (ENT-7).
// The `debuff_*` keys are CATEGORY tokens — what a tspy effect element carries at its
// front. Since the parser began reading each AttribMod's own index array (TSPY-4) most
// of these arrive under their real HC attrib name instead (`Base_Defense`, `ToHit`, the
// movement stats), so both spellings must map: the token for the elements that still
// front one, the real name for the mods that now name themselves. Without the real
// names every location/patch pseudo-pet lost the -Def half again — Sleet's
// `Base_Defense 3.0 Melee_Debuff_Def` is byte-identical to its HC and Rebirth twins.
// The slow family is absent for the reason it is absent from DEBUFF_ATTRIBS: these keys
// fired on the attrib NAME alone, so a Resistance-aspect slow PROTECTION (Temporal
// Mending) published as a foe `-Speed`. `classifySlowFamily` runs for tspy too — the
// aspect and the table both survive its schema, and it states the same movement attribs
// Homecoming does. The fork's own aggregate `slow` attrib went with them: it appears on no
// pet power in the corpus, and `MOVEMENT_TYPES` has no entry for it either, so a parent
// power carrying one already emits nothing.
const _TSPY_DEBUFF_NAMED = {
  'debuff_def': 'DefenseDebuff', 'debuff_tohit': 'ToHitDebuff', 'debuff_dam': 'DamageDebuff',
  'base_defense': 'DefenseDebuff', 'tohit': 'ToHitDebuff',
};
const _TSPY_DEBUFF_SIGNED = {
  'res_dmg': 'ResistanceDebuff', 'recovery': 'RecoveryDebuff',
  'regeneration': 'RegenDebuff',
  'endurance': 'EndDrain', 'enddrain': 'EndDrain',
};

// The two `_TSPY_DEBUFF_SIGNED` attribs whose `*_Ones` rows are markers rather than
// magnitudes. Measured over the reachable pet corpus: `recovery` and `endurance` are
// the only signed attribs that ever ride a `*_Ones` table with a negative scale
// (65 and 43 templates), so naming them costs the other keys nothing.
const _TSPY_ONES_MARKER_ATTRIBS = new Set(['recovery', 'endurance']);

// ---------------------------------------------------------------------------
// Ally-buff-aura vocabulary (buff-pets: Force Field Generator, Barrier Reef,
// Triage Beacon, Tree of Life, …). These "floaty" pets exist only to project a
// PBAoE buff onto the caster/team. The buff lives on the pet's own toggle/Auto
// power (FFG's Dispersion_Bubble) or on a Redirect power the entity references
// (Marine's Wellspring_Barrier_Def) — either way it reaches extractEffects as a
// template with target != Self (an aura), positive scale, and a buff aspect.
//
// The historical extractEffects vocabulary was ENTIRELY foe-facing (mez / debuff
// / -res) plus ally Heal, so these Def/Res/Absorb auras were parsed from the bin
// and then silently dropped on conversion — the root cause of "buff-pet stats
// can't be seen in your totals." A pet's OWN self-buff (FFG's ResistAll) stays
// dropped because it targets Self and never reaches this vocabulary.
//
// Defense attribs arrive as bare position/type names (Ranged/Melee/Area +
// Smashing/Lethal/…) on a `*_Buff_Def` table; normalize each to the calc's
// def sub-type key (matches DEFENSE_POSITIONS + the damage-type keys in
// character-totals' GlobalBonuses: defMelee/defRanged/defAoE/defSmashing/…).
const BUFF_DEF_TYPE_MAP = {
  melee: 'melee', ranged: 'ranged', area: 'aoe', aoe: 'aoe',
  smashing: 'smashing', lethal: 'lethal', fire: 'fire', cold: 'cold',
  energy: 'energy', negative_energy: 'negative', negative: 'negative',
  psionic: 'psionic', toxic: 'toxic',
};

// The AGGREGATE defence attrib, which names no vector: the game raises defence against
// everything, and the twin publishes it as a scalar `effects.defenseBuff` beside its own
// positional map (`convert-powerset.cjs`'s BASE_DEFENSE special handling). It is absent from
// BUFF_DEF_TYPE_MAP because that map answers "which vector is this", and the answer here is
// "all of them" — so every branch consulting the map declined the row, and the name-keyed
// DEBUFF_ATTRIBS caught the ally-facing ones while the self-facing ones fell out entirely
// (ENT-18).
//
// Spelled out as every vector rather than left implicit. This route's consumer keys per
// vector — `buff_pets::aura_keys` maps `defenseTypes` through `defense_key` — so a row naming
// no type writes nothing at all, and an absent list would ship an inert row rather than an
// "all" one. Derived from the positional map's own values so the two cannot drift; it is the
// same eleven vectors `defense_key` resolves.
const AGGREGATE_DEFENSE_ATTRIBS = new Set(['base_defense', 'defense']);
const ALL_DEFENSE_TYPES = [...new Set(Object.values(BUFF_DEF_TYPE_MAP))];

// Resistance-buff attribs arrive as damage-type `*_Dmg` names on a resistance
// aspect; normalize to the calc's res sub-type key (resSmashing/resLethal/…).
const BUFF_RES_TYPE_MAP = {
  smashing_dmg: 'smashing', lethal_dmg: 'lethal', fire_dmg: 'fire', cold_dmg: 'cold',
  energy_dmg: 'energy', negative_energy_dmg: 'negative', psionic_dmg: 'psionic',
  toxic_dmg: 'toxic',
};

// Scalar ally-buff attribs (single-value stats). aspect=Current + positive scale.
// Every one of these attribs ALSO keys DEBUFF_ATTRIBS in its other direction (a
// negative `recovery` is a -Recovery debuff, a negative `regeneration` a -Regen,
// a negative `rechargetime` a Slow) — so the caller must skip the debuff path for
// an attrib consumed here as a positive buff, or a Triage-Beacon-style +Regen aura
// would emit both a RegenBuff and a phantom debuff.
const BUFF_SCALAR_ATTRIBS = {
  regeneration: 'RegenBuff',
  recovery: 'RecoveryBuff',
  tohit: 'ToHitBuff',
  rechargetime: 'RechargeBuff',
};

// ---------------------------------------------------------------------------
// SELF-buff vocabulary — the pet's OWN defensive profile.
//
// A summon is a second character: the pet has its own class row and its own
// always-on powers, and those powers carry its resistance, defense, mez protection
// and mez resistance on `target: Self` templates. `isDebuffTemplate` rejects
// target=Self wholesale, so every one of them was parsed from the bin and then
// dropped — 352 of the entities a player power can summon have at least one. This
// is what "can I see my pet's stats" needs.
//
// Distinct from the faced vocabulary `templateFace`/`facedType` produce. Those come
// off rows the pet addresses to somebody else (`AnyAffected`), where the recipient —
// summoner or foe — is decided by the ABILITY's `targetsAffected` (ENT-12). These
// come off rows addressed to the pet itself, so there is no recipient question to
// ask: the stat is the pet's, full stop.
//
// CRITICAL — these MUST NOT reuse the `DefenseBuff`/`ResistanceBuff`/`Absorb` type
// names. Those are the ALLY-aura vocabulary below, and `buff_pets` folds them into
// the PLAYER's totals by exact type string. A pet's own +Res is not the player's
// +Res, and emitting it under the ally name would leak a pet stat onto the character
// sheet. Hence the `Self…` prefix, and a guard test that asserts the two
// vocabularies stay disjoint.
//
// The families below are the census of what actually appears on self templates
// across all 760 pet powers reachable from a player summon — not a guess.
const SELF_MEZ_ATTRIBS = {
  held: 'Hold', stunned: 'Stun', sleep: 'Sleep', confused: 'Confuse',
  terrorized: 'Fear', immobilized: 'Immobilize', placate: 'Placate',
  knockback: 'Knockback', knockup: 'Knockup', repel: 'Repel', taunt: 'Taunt',
};

// Powersets whose self-buffs are TRANSIENT and must not be read as pet stats.
// This is a documented client-bin gap, not a preference: `Mastermind_Pets.
// Materialization` is the shared spawn-in grace window every MM henchman gets, and
// its own help text reads "As hechmen are first summoned, they are invisible to
// enemies and hard to hit. This effect lasts for up to 15s after the henchman is
// summoned or until they engage in combat." NEITHER condition is in the binary —
// the templates say a flat +100% Defense to all 11 positions/types on a 20s
// duration with a 20s activate_period, structurally identical to a permanent
// refreshing aura (Phalanx Fighting, Cosmic Balance, and Ghosts' own Resistance all
// carry duration == period and ARE permanent, so no duration heuristic separates
// them). Reading it literally tells a Mastermind their Bruiser has 100% defense.
// The summon-time scoping and the combat suppression are server-side, the same
// class of gap as the unparsed `suppress_events` tail.
//
// Named exclusions are pinned by pet-self-buffs.test.ts so this list cannot grow
// silently — anything added here needs the same standard of evidence.
const SELF_BUFF_EXCLUDED_POWERSETS = new Set([
  'mastermind_pets.materialization',
]);

// `aspect=Resistance` on a NON-damage, non-mez attrib is resistance to that
// attribute being debuffed (slow resistance, recharge-debuff resistance,
// heal-debuff resistance), not damage resistance.
const SELF_DEBUFF_RES_ATTRIBS = {
  runningspeed: 'runSpeed', jumpingspeed: 'jumpSpeed', flyingspeed: 'flySpeed',
  rechargetime: 'recharge', heal_dmg: 'heal', hitpoints: 'maxHP',
  recovery: 'recovery', endurance: 'endurance', tohit: 'toHit',
};

/**
 * Classify a `target: Self` template as one of the pet's own defensive stats.
 * Returns an array of PetEffect objects (possibly empty — most self templates are
 * movement/marker/utility and belong to nobody).
 *
 * Sign is load-bearing and preserved, not absolute-valued:
 *  • `aspect=Resistance` + damage type, +scale → resistance; −scale → VULNERABILITY
 *    (Dark Servant carries a real −20% Energy resistance, and abs()ing it would
 *    turn a weakness into a strength).
 *  • `aspect=Current` + mez, −scale → mez PROTECTION at magnitude |scale × table|
 *    (the game stores protection as negative magnitude on the mez attrib).
 *    A POSITIVE Current mez is the opposite thing — the pet applying a mode to
 *    itself (`Fly`, `Untouchable` at +100) — and is deliberately not captured.
 *  • `aspect=Resistance` + mez, +scale → mez RESISTANCE (duration reduction).
 */
function extractSelfBuff(template) {
  const out = [];
  const scale = template.scale;
  if (typeof scale !== 'number' || scale === 0 || !template.table) return out;
  const aspect = template.aspect;
  const attribsLower = (template.attribs || []).map(a => a.toLowerCase());
  const table = template.table;

  if (aspect === 'Resistance') {
    // Damage resistance (signed — a negative is a real vulnerability).
    const resistanceTypes = [];
    for (const a of attribsLower) {
      const key = BUFF_RES_TYPE_MAP[a];
      if (key && !resistanceTypes.includes(key)) resistanceTypes.push(key);
    }
    if (resistanceTypes.length > 0) {
      out.push({ type: 'SelfResistance', scale, table, resistanceTypes });
    }
    // Mez resistance (shorter mez duration), distinct from mez protection.
    const mezTypes = [];
    for (const a of attribsLower) {
      const key = SELF_MEZ_ATTRIBS[a];
      if (key && !mezTypes.includes(key)) mezTypes.push(key);
    }
    if (mezTypes.length > 0 && scale > 0) {
      out.push({ type: 'SelfMezResistance', scale, table, mezTypes });
    }
    // Debuff resistance (slow / -recharge / -heal / -HP …).
    const debuffTypes = [];
    for (const a of attribsLower) {
      const key = SELF_DEBUFF_RES_ATTRIBS[a];
      if (key && !debuffTypes.includes(key)) debuffTypes.push(key);
    }
    if (debuffTypes.length > 0 && scale > 0) {
      out.push({ type: 'SelfDebuffResistance', scale, table, debuffTypes });
    }
    return out;
  }

  if (aspect === 'Current') {
    // Positional/typed defense on a Buff_Def table (or a bare Ones scalar —
    // Lore's Evasion is `Area 0.5 x Melee_Ones`). Guard against the debuff twin.
    const tableLower = table.toLowerCase();
    if (!tableLower.includes('debuff') && scale > 0) {
      const defenseTypes = [];
      for (const a of attribsLower) {
        const key = BUFF_DEF_TYPE_MAP[a];
        if (key && !defenseTypes.includes(key)) defenseTypes.push(key);
      }
      // The aggregate attrib names every vector — Moment of Glory's `Base_Defense 9.5` was
      // dropped whole here while its resistance half at the same scale published seven rows.
      if (attribsLower.some(a => AGGREGATE_DEFENSE_ATTRIBS.has(a))) {
        for (const ty of ALL_DEFENSE_TYPES) {
          if (!defenseTypes.includes(ty)) defenseTypes.push(ty);
        }
      }
      if (defenseTypes.length > 0) {
        out.push({ type: 'SelfDefense', scale, table, defenseTypes });
      }
    }
    // Mez protection: negative magnitude on the mez attrib.
    if (scale < 0) {
      const mezTypes = [];
      for (const a of attribsLower) {
        const key = SELF_MEZ_ATTRIBS[a];
        if (key && !mezTypes.includes(key)) mezTypes.push(key);
      }
      if (mezTypes.length > 0) {
        out.push({ type: 'SelfMezProtection', scale: Math.abs(scale), table, mezTypes });
      }
    }
  }

  return out;
}

/**
 * Every effect type this converter can emit, and the only names it may.
 *
 * A pet effect type is a string, so the Rust consumers can only route the ones they know
 * and drop the rest in silence — which is how six types shipped for months reaching no
 * consumer at all, and how a seventh (`RegenDebuff`) reached one only after ENT-7 went
 * looking. The set cannot grow silently now: an unlisted type throws at the push below,
 * where a new type is born, and `pet_effect_vocabulary.rs` grades the other direction —
 * every type in the shipped corpus must have a route or a stated reason for having none.
 *
 * Derived from the classification maps rather than hand-listed, so adding an attrib to
 * one of them cannot leave this behind. Only the names that are literals in their own
 * emitter are literals here (ENT-9).
 */
const EMITTED_EFFECT_TYPES = new Set([
  ...Object.values(MEZ_ATTRIBS),
  ...Object.values(DEBUFF_ATTRIBS),
  ...Object.values(_TSPY_DEBUFF_NAMED),
  ...Object.values(_TSPY_DEBUFF_SIGNED),
  ...Object.values(BUFF_SCALAR_ATTRIBS),
  // classifySlowFamily
  'Slow', 'MovementCapDebuff', 'RechargeDebuff', 'RechargeDebuffResist',
  // extractBuffAura's typed branches, extractDamageStrength, and the direct heal
  'DefenseBuff', 'ResistanceBuff', 'Absorb',
  'DamageBuff', 'DamageDebuff', 'EnduranceGain', 'Heal',
  // The non-applied faces, built from the same applied names `facedType` suffixes.
  ...Object.values(MEZ_ATTRIBS).flatMap((t) => [`${t}Protection`, `${t}Resist`]),
  ...Object.values(DEBUFF_ATTRIBS).map((t) => `${t}Resist`),
  'HealResist',
  // extractSelfBuff — the pet's own stat sheet, off its `target: Self` templates.
  'SelfResistance', 'SelfDefense', 'SelfMezProtection', 'SelfMezResistance',
  'SelfDebuffResistance',
]);

/**
 * Classify an ally-buff-aura template (Defense / Resistance / Absorb / +Regen /
 * +Recovery / +ToHit / +Recharge — the vocabulary of "floaty" buff-pets: Force
 * Field Generator, Barrier Reef, Triage Beacon, …). Returns `{ effects, consumed }`:
 * `effects` is an array of buff PetEffect objects (Defense/Resistance collapse
 * their many sub-type attribs into ONE effect carrying a `defenseTypes`/
 * `resistanceTypes` list — one aura buffs all of them at the same scale/table);
 * `consumed` is the set of lowercased attrib names turned into buffs, which the
 * caller uses to suppress the overlapping DEBUFF_ATTRIBS classification. Both are
 * empty for a non-buff template, so the caller falls through to mez/debuff/heal.
 *
 * Gating (all datasets — keyed on table/aspect/attrib, which survive the tspy
 * aspect-drop for the table-based branches):
 *  • target != Self is already guaranteed by isDebuffTemplate at the call site.
 *  • scale > 0 — a negative scale here is a foe DEBUFF (a -Defense on a Buff_Def
 *    table, a -ToHit, a Slow), handled by the DEBUFF_ATTRIBS path, not here.
 */
function extractBuffAura(template) {
  const out = [];
  const consumed = new Set();
  const scale = template.scale;
  if (!(typeof scale === 'number' && scale > 0) || !template.table) {
    return { effects: out, consumed };
  }
  const aspect = template.aspect;
  const tableLower = template.table.toLowerCase();
  const attribsLower = (template.attribs || []).map(a => a.toLowerCase());

  // Buff vs debuff is encoded in the TABLE name, not the sign: a foe -ToHit is
  // stored as a POSITIVE scale on a `*_Debuff_ToHit` table (verified: Liquefy
  // 2.856, Earthquake 1.0), and a -Recharge foe debuff rides a `*_Slow` table.
  // So an ally buff is a positive scale on a table that is neither.
  const isBuffTable = !isDebuffTable(template.table);

  // Defense buff: `*_Buff_Def` table (aspect Current). Collect every def sub-type.
  // The leading underscore + debuff guard keeps `*_Debuff_Def` (a foe -Def) out.
  //
  // The aggregate attrib rides whatever table its power uses rather than a `*_Buff_Def` one
  // (Seductive Song's is `Melee_Ones`), so it answers to the same "not a debuff table" rule
  // the self-buff branch already applies to Lore's Evasion. The positional path keeps its
  // tighter table gate: whether a positional attrib on a bare Ones table is an ally buff is a
  // separate question with its own population, and widening it here would answer it silently.
  if (isBuffTable) {
    const defenseTypes = [];
    if (/_buff_def$/.test(tableLower)) {
      for (const a of attribsLower) {
        const key = BUFF_DEF_TYPE_MAP[a];
        if (key && !defenseTypes.includes(key)) { defenseTypes.push(key); consumed.add(a); }
      }
    }
    for (const a of attribsLower) {
      if (!AGGREGATE_DEFENSE_ATTRIBS.has(a)) continue;
      for (const ty of ALL_DEFENSE_TYPES) {
        if (!defenseTypes.includes(ty)) defenseTypes.push(ty);
      }
      consumed.add(a);
    }
    if (defenseTypes.length > 0) {
      out.push({ type: 'DefenseBuff', scale, table: template.table, defenseTypes });
    }
  }

  // Resistance buff to allies: positive aspect=Resistance on damage-type attribs.
  // (A pet's own +res is target=Self and never reaches here.)
  if (aspect === 'Resistance') {
    const resistanceTypes = [];
    for (const a of attribsLower) {
      const key = BUFF_RES_TYPE_MAP[a];
      if (key && !resistanceTypes.includes(key)) { resistanceTypes.push(key); consumed.add(a); }
    }
    if (resistanceTypes.length > 0) {
      out.push({ type: 'ResistanceBuff', scale, table: template.table, resistanceTypes });
    }
  }

  // Absorb: `Absorb` attrib. aspect=Maximum → the game folds this to a flat
  // absorb amount off a Heal table (the MaxHP-fraction form is an Expression the
  // pet parser doesn't carry; convert-powerset treats non-Expression Maximum
  // absorb as flat too). Carry the aspect for provenance; the calc resolves it
  // as a flat {scale,table} absorb.
  if (attribsLower.includes('absorb')) {
    out.push({ type: 'Absorb', scale, table: template.table, absorbAspect: aspect });
    consumed.add('absorb');
  }

  // Scalar buffs (+Regen / +Recovery / +ToHit / +Recharge). aspect=Current, on a
  // buff (non-debuff, non-slow) table — the table gate is what separates an ally
  // +ToHit aura from the many foe -ToHit debuff pseudo-pets (Liquefy, Earthquake,
  // Seekers) that store their debuff as a positive scale on a `*_Debuff_ToHit`
  // table.
  if (aspect === 'Current' && isBuffTable) {
    for (const a of attribsLower) {
      const type = BUFF_SCALAR_ATTRIBS[a];
      if (type) {
        out.push({ type, scale, table: template.table });
        consumed.add(a);
      }
    }
  }

  return { effects: out, consumed };
}

/**
 * Classify a damage-STRENGTH modifier (an ally +Damage buff, a foe -Damage debuff) or an
 * ally endurance grant. Same `{ effects, consumed }` contract as extractBuffAura.
 *
 * These sit outside every other vocabulary in this file, which is why a pet carrying only
 * them classified nothing at all (ENT-3): a `*_Dmg` attrib at aspect=Strength is not damage
 * (extractDamage wants aspect=Absolute), not a resistance (that is aspect=Resistance), and
 * DEBUFF_ATTRIBS holds no damage-type keys — so it fell through all three. It is the §2
 * discriminator rule biting the converter: the ASPECT is what makes those rows a damage
 * buff/debuff rather than damage, and there was no slot for the pair.
 *
 * Homecoming is the oracle for the shape. It routes the same three mechanics through the
 * PARENT power instead of a pet, and `convert-powerset.cjs` emits them as `damageBuff`,
 * `damageDebuff` and `enduranceGain` — so the pet forms carry the same names to the same
 * consumers rather than inventing a parallel vocabulary (Rebirth's Siphon Power keeps its
 * own parent `damageDebuff` and was losing only the pet's ally +damage half).
 *
 * The TABLE discriminates direction, not the sign — the rule extractBuffAura already
 * documents: a foe -Dam is stored POSITIVE on a `*_Debuff_Dam` table, an ally +Dam positive
 * on `*_Buff_Dmg`. Only on a neutral `*_Ones` table does the sign carry it. A `*_Dmg` at
 * aspect=Strength on any other table is a different mechanic (a Lore boss's `Ranged_Stun`
 * row is the live case) and classifies as neither — falling through is the correct answer
 * there, not a default.
 */
function extractDamageStrength(template) {
  const out = [];
  const consumed = new Set();
  const scale = template.scale;
  if (typeof scale !== 'number' || !template.table || template.target === 'Self') {
    return { effects: out, consumed };
  }
  const tableLower = template.table.toLowerCase();
  const attribsLower = (template.attribs || []).map(a => a.toLowerCase());

  if (template.aspect === 'Strength' && attribsLower.some(a => /_dmg$/.test(a))) {
    let type = null;
    if (/_debuff_dam$/.test(tableLower)) type = scale > 0 ? 'DamageDebuff' : null;
    else if (/_buff_dmg$/.test(tableLower)) type = scale > 0 ? 'DamageBuff' : null;
    else if (/_ones$/.test(tableLower)) {
      type = scale > 0 ? 'DamageBuff' : (scale < 0 ? 'DamageDebuff' : null);
    }
    if (type) {
      // One modifier moves every damage type it names at the same scale, so the per-type
      // attribs collapse to a single effect — as DefenseBuff/ResistanceBuff collapse theirs.
      out.push({ type, scale: Math.abs(scale), table: template.table });
      for (const a of attribsLower) if (/_dmg$/.test(a)) consumed.add(a);
    }
  }

  // Ally endurance grant. `endurance` is a DEBUFF_ATTRIBS key (an EndDrain) in the draining
  // direction only — both debuff paths already require a negative scale — so the granting
  // direction had nowhere to land. aspect=Absolute is an amount off the table.
  if (template.aspect === 'Absolute' && attribsLower.includes('endurance') && scale > 0) {
    out.push({ type: 'EnduranceGain', scale, table: template.table });
    consumed.add('endurance');
  }

  return { effects: out, consumed };
}

// Attrib cache values that indicate non-attack utility powers
const UTILITY_ATTRIBS = new Set([
  'fly', 'untouchable', 'translucency', 'stealth',
  'grant_power', 'revoke_power', 'set_mode', 'set_costume',
  'teleport', 'entcreate',
]);

// Power names that are always utility
const UTILITY_POWER_PATTERNS = [
  /^resistall$/i,
  /^invisible$/i,
  /^immobilize$/i,  // Self-immobilize for stationary pets
  /^fly$/i,
  /^hover$/i,
  /^phase$/i,
  /^stealth$/i,
  /^teleport$/i,
  /^grant_/i,
  /^set_mode/i,
];

/**
 * Check if a power is a utility/non-combat power we should skip
 */
function isUtilityPower(powerData) {
  const name = powerData.name.toLowerCase();

  // Check name patterns
  for (const pattern of UTILITY_POWER_PATTERNS) {
    if (pattern.test(name)) return true;
  }

  // If attrib_cache only has utility attribs, skip
  // But keep powers that have combat-relevant effects (damage, mez, debuffs)
  const attribCache = (powerData.attrib_cache || []).map(a => a.toLowerCase());
  if (attribCache.length > 0) {
    const hasCombatAttrib = attribCache.some(a =>
      DAMAGE_ATTRIBS.has(a) ||
      MEZ_ATTRIBS[a] !== undefined ||
      DEBUFF_ATTRIBS[a] !== undefined
    );
    if (!hasCombatAttrib && attribCache.every(a =>
      UTILITY_ATTRIBS.has(a) ||
      a === 'null' ||
      a.startsWith('resist') ||
      a.startsWith('defense') ||
      a === 'fly' ||
      a === 'translucency' ||
      a === 'stealth'
    )) {
      return true;
    }
  }

  // Self-targeting immobilize (stationary pets)
  if (name === 'immobilize' && powerData.target_type === 'Self') return true;

  return false;
}

// A PvE/PvP `enttype` pair is split into `enttype target> critter eq` (PvE) and
// `enttype target> player eq` (PvP) groups, BOTH tagged is_pvp='EITHER', so the
// PVP_ONLY flag never catches the PvP half — the requires clause does, and
// `_pv-scope.cjs` reads the parser's verdict on it. This converter previously
// matched the CoD2 *infix* spelling `target>enttype eq 'player'`, which the
// parser never emits, so the guard was dead: every summoned rain/storm pet kept
// BOTH halves of its damage pair. Blizzard read four sources (Lethal 0.05 + PvE
// Cold 0.05 + PvP Cold 0.01 + PvP Cold 0.09) instead of two; Ice Storm and Rain
// of Fire read three instead of one.

// Thunderspy spells the same split as an `isPVPMap?` check (negated for the PvE
// half), so read both idioms — reading only one leaves tspy pets double-counted.
function isPvpMapOnly(requiresExpression) {
  return /\bisPVPMap\?(?!\s+!)/i.test(requiresExpression || '');
}

/** True for any effect group that only applies on a PvP map / to a player
 *  target. The planner has no PvP mode, so the PvE twin is always preferred
 *  (GAME-DATA-PRINCIPLES §3). Dropping the whole group also drops its
 *  child_effects subtree, matching convert-powerset.cjs `collectTemplatesDeep`. */
function isPvpOnlyGroup(effectGroup) {
  if (!effectGroup) return false;
  if (effectGroup.is_pvp === 'PVP_ONLY') return true;
  if (isPvpOnlyByRequires(effectGroup)) return true;
  return isPvpMapOnly(gateText(effectGroup.requires_expression));
}

/**
 * True for a bonus-damage group gated on a target-state window the pet does not
 * always satisfy: `now <Token> target.TokenTime> - <N> <` ("the target was hit by
 * <Token> in the last N seconds").
 *
 * Singularity's Lift carries Gravity Control's Impact group (`GravityDistortion`,
 * 12s, scale 0.33 on top of the 1.32 base). This converter has no conditional-gate
 * filter at all, so it summed both and showed Lift as ALWAYS dealing 1.65 — a +25%
 * over-count, the exact mirror of the powerset converter dropping the same group
 * entirely (see `_isUntoggleableGate` in convert-powerset.cjs). A pet's powers carry
 * no user toggles, so the honest reading is base damage only.
 *
 * Deliberately narrow: this converter still lacks the general `_isConditionalGate`
 * pass that convert-powerset.cjs applies, so other gated pet bonuses remain summed.
 * Widening it needs its own audit of every pet damage number.
 */
function isTargetTokenWindowGroup(effectGroup) {
  const req = gateText(effectGroup?.requires_expression);
  return /\btarget\.TokenTime>/.test(req);
}

/**
 * The probability an effect group states, or `undefined` when it states none.
 *
 * **Zero is not a probability here.** `convert-powerset.cjs` settled this on the parent
 * route — "a chance:0 group is a mode-gate SENTINEL, not literal 0%" — and the export
 * bears it out: Thunderspy alone ships 2383 groups at `chance: 0.0`, among them Bullet
 * Rain's slow, which lands every time. Reading the sentinel as a number publishes
 * `chance: 0` on rows that always fire, and every consumer that weights by chance then
 * reports them as never happening: Howler Wolf's Vicious Bite lost its 0.42 Lethal
 * rider, and `granted::pseudo_pet_effects` drops a control row outright on `chance < 1`.
 *
 * Only a value strictly inside (0, 1) is a real roll. `1` and absence both mean the
 * group always applies, and are equally uninteresting to a display, so both answer
 * `undefined` — a stated `chance: 1` on a row would read as a fact somebody measured.
 *
 * Unlike the parent route this converter has no `gated` mark to move the sentinel onto,
 * so the gate itself is not recorded; the row is published ungated, which is what it
 * already was. Naming the sentinel is the fix available here.
 */
function groupProbability(chance) {
  if (typeof chance !== 'number') return undefined;
  return chance > 0 && chance < 1 ? chance : undefined;
}

/**
 * Check if an effect template is PvE-relevant damage
 */
function isPvEDamageTemplate(template, effectGroup) {
  const attribs = (template.attribs || []).map(a => a.toLowerCase());

  // Thunderspy: generic `Damage` attrib with the aspect dropped. Accept a
  // positive-scale `Damage` on a `*_Damage` table — this excludes the negative
  // `*_Ones` summon-shell / -res templates, the scale-0 strength meta-templates,
  // and the `CritActive` crit rider (not a `damage` attrib). Element is resolved
  // from the shortHelp at extract time.
  if (_TSPY && attribs.includes('damage')) {
    if (isPvpOnlyGroup(effectGroup)) return false;
    if (isTargetTokenWindowGroup(effectGroup)) return false;
    return /_damage$/i.test(template.table || '') && template.scale > 0;
  }

  // Must be a damage attribute
  if (!attribs.some(a => DAMAGE_ATTRIBS.has(a))) return false;

  // Must be absolute aspect (actual damage, not resistance/strength)
  if (template.aspect !== 'Absolute') return false;

  // Skip PvP-only effects and the PvP half of a PvE/PvP pair
  if (isPvpOnlyGroup(effectGroup)) return false;

  // Skip conditional bonus damage the pet doesn't always land (Singularity's
  // Lift + Gravity Distortion Impact).
  if (isTargetTokenWindowGroup(effectGroup)) return false;

  return true;
}

/**
 * Extract damage entries from a pet power's effects
 */
function extractDamage(powerData) {
  const damageEntries = [];
  // Thunderspy: element for the generic `Damage` attrib lives in the shortHelp.
  const tspyType = _TSPY ? (_tspyDamageType(powerData.display_short_help) || 'Special') : null;

  for (const effectGroup of (powerData.effects || [])) {
    // Skip PvP-only effect groups (and the `player eq` half of a PvE/PvP pair)
    if (isPvpOnlyGroup(effectGroup)) continue;

    for (const template of (effectGroup.templates || [])) {
      if (isPvEDamageTemplate(template, effectGroup)) {
        // A sub-1.0 group chance is a probabilistic hit, not a guaranteed one
        // (Trip Mine's third Fire template lands 50% of the time). `extractEffects`
        // has always carried this through; damage silently did not, so every such
        // entry was summed at face value — Trip Mine read ~14% high before the
        // fires-per-spawn bug even entered into it.
        const chance = groupProbability(effectGroup.chance);
        for (const attrib of template.attribs) {
          const attribLower = attrib.toLowerCase();
          if (DAMAGE_ATTRIBS.has(attribLower)) {
            // Convert attrib name to display type: "Energy_Dmg" -> "Energy"
            const damageType = attrib.replace(/_Dmg$/i, '').replace(/_/g, ' ');
            damageEntries.push({
              damageType,
              scale: template.scale,
              table: template.table || 'Melee_Damage',
              ...(chance !== undefined ? { chance } : {}),
            });
          } else if (_TSPY && attribLower === 'damage') {
            // Generic tspy Damage — element from the shortHelp (falls back to
            // Special when the tooltip has no DMG(...) clause).
            damageEntries.push({
              damageType: tspyType,
              scale: template.scale,
              table: template.table || 'Melee_Damage',
              ...(chance !== undefined ? { chance } : {}),
            });
          }
        }
      }
    }

    // Also check child effects (e.g., Containment bonus damage)
    // Skip these for base DPS - they are conditional
  }

  return damageEntries;
}

/**
 * Check if a template applies a debuff (negative value on target)
 */
function isDebuffTemplate(template, effectGroup) {
  // Skip PvP-only effects and the PvP half of a PvE/PvP pair
  if (isPvpOnlyGroup(effectGroup)) return false;

  // Must target foes (not self buffs)
  if (template.target === 'Self') return false;

  return true;
}

/**
 * Extract non-damage effects (mez, debuffs) from a pet power
 */
function extractEffects(powerData) {
  const effects = [];
  const seen = new Set(); // Identities already held — see effectIdentity
  // Transient self-buffs that must not be read as permanent pet stats — see
  // SELF_BUFF_EXCLUDED_POWERSETS for why the binary can't tell us this itself.
  const selfBuffsSuppressed = SELF_BUFF_EXCLUDED_POWERSETS.has(
    String(powerData.powerset || '').toLowerCase(),
  );

  for (const effectGroup of (powerData.effects || [])) {
    if (isPvpOnlyGroup(effectGroup)) continue;

    const processTemplates = (templates, chance) => {
      for (const template of (templates || [])) {
        // Hold an effect unless one agreeing on every discriminator is already held. A
        // power states the same template several times — once per effect group, and again
        // through a child group — and those repeats are genuine duplicates; two rows
        // differing on an axis, an aspect, a scale or a table are not.
        const push = (effect) => {
          if (!EMITTED_EFFECT_TYPES.has(effect.type)) {
            throw new Error(
              `convert-pet-entities: '${effect.type}' is not a registered pet effect type. `
                + 'Add it to EMITTED_EFFECT_TYPES and give it a route in '
                + 'granted.rs PET_EFFECT_ROUTES, or the consumers will drop it in silence.',
            );
          }
          if (chance !== undefined) effect.chance = chance;
          const identity = effectIdentity(effect, template);
          if (seen.has(identity)) return;
          seen.add(identity);
          // A template the caster's Strength does not reach is not enhanceable by the
          // summoner's slotting, even though the pet inherits it through CopyBoosts.
          // Recorded per effect: this converter read no flags at all before, so an
          // unenhanceable pet debuff was indistinguishable from an enhanceable one and
          // 93-100% of the pet -Resistance corpus was the wrong kind (ENT-4).
          //
          // `granted::pseudo_pet_effects` merges the mark into the summoning power's
          // display bag, where a marked value resolves flat across all three tiers — the
          // same rule `convert-powerset.cjs` now stamps on a parent power, so neither
          // route can overstate what the other understates.
          if ((template.flags || []).includes('IgnoreStrength')) effect.ignoreStrength = true;
          effects.push(effect);
        };

        // The pet's OWN defensive stats (`target: Self`) — a separate vocabulary from
        // everything below, which is foe/ally-facing. `isDebuffTemplate` rejects
        // target=Self, so this branch takes the template instead of, not after, that
        // gate. `push`'s identity covers every field the row carries, so a pet whose
        // Resistance power lists S/L and F/C on separate templates keeps BOTH rows
        // (different scales) while a genuine repeat collapses.
        if (template.target === 'Self') {
          if (!selfBuffsSuppressed) {
            for (const self of extractSelfBuff(template)) push(self);
          }
          continue;
        }
        if (!isDebuffTemplate(template, effectGroup)) continue;

        // Ally-buff auras (Def/Res/Absorb/+Regen/…) — the whole point of a buff-pet.
        // Runs for all datasets (table/aspect/attrib based). Deduped per type so a
        // single power contributes at most one Defense + one Absorb + … effect.
        // `buffConsumed` names the attribs turned into positive buffs so the
        // debuff loop below skips them (a positive `recovery`/`rechargetime` must
        // not ALSO surface as a -Recovery/Slow debuff).
        const { effects: buffAuras, consumed: buffConsumed } = extractBuffAura(template);
        // Damage-strength modifiers and the endurance grant, which every branch below is
        // blind to. Runs for all datasets — aspect and table both survive the tspy schema.
        const { effects: dmgMods, consumed: dmgConsumed } = extractDamageStrength(template);
        for (const attrib of dmgConsumed) buffConsumed.add(attrib);

        for (const buff of [...buffAuras, ...dmgMods]) push(buff);

        for (const attrib of (template.attribs || [])) {
          const attribLower = attrib.toLowerCase();
          if (buffConsumed.has(attribLower)) continue;

          // Which FACE of the stat this row moves (see `templateFace`). The attrib names
          // the stat; the aspect and the sign name what is being done to it, and every
          // branch below reads the APPLIED face only. A resistance or protection row
          // publishes under its own name and is never also the effect it protects from —
          // ENT-3 closed this on `Heal_Dmg` alone and ENT-7 on `Regeneration` alone; the
          // guard is general now, so the next family cannot arrive without one.
          const face = templateFace(attribLower, template);
          if (face !== 'applied') {
            const appliedType = MEZ_ATTRIBS[attribLower] || DEBUFF_ATTRIBS[attribLower]
              || (attribLower === 'heal_dmg' ? 'Heal' : null);
            const type = facedType(appliedType, face);
            if (type) {
              const effect = { type };
              if (template.scale && template.table) {
                // Stored at |scale| like every other row here and on the inline route: a
                // protection magnitude is authored negative and is not a smaller number
                // than a weaker one.
                effect.scale = Math.abs(template.scale);
                effect.table = template.table;
              }
              push(effect);
            }
            continue;
          }

          // The direction half of the same rule, which the face fork cannot carry: a
          // POSITIVE `Regeneration` is the buff direction, which `extractBuffAura` owns.
          if (attribLower === 'regeneration' && !isDebuffDirection(template)) continue;

          // Check mez effects
          const mezType = MEZ_ATTRIBS[attribLower];
          if (mezType) {
            const effect = { type: mezType };
            if (template.magnitude && template.magnitude > 0) {
              effect.magnitude = template.magnitude;
            }
            if (template.scale && template.table) {
              effect.scale = template.scale;
              effect.table = template.table;
            }
            push(effect);
          }

          // The slow family — three different attributes the attrib name alone cannot
          // tell apart. Runs for every dataset: the aspect and the table survive the tspy
          // schema, and tspy states the same movement attribs Homecoming does.
          const slowFamily = classifySlowFamily(attribLower, template);
          if (slowFamily && Math.abs(template.scale || 0) >= 0.001) {
            const effect = { type: slowFamily.type };
            if (slowFamily.axis) effect.axis = slowFamily.axis;
            if (template.scale && template.table) {
              effect.scale = Math.abs(template.scale);
              effect.table = template.table;
            }
            push(effect);
          }

          // Check debuff effects. Thunderspy uses its own attrib vocabulary and
          // drops the target, so it takes a dedicated classification (name-encoded
          // debuffs at |scale|; resource debuffs only when negative — see the
          // _TSPY_DEBUFF_* maps) INSTEAD of the HC DEBUFF_ATTRIBS block, so a pet
          // self-buff (Res_DMG +N) can't leak in as a foe -Resistance.
          if (_TSPY) {
            let debuffType = _TSPY_DEBUFF_NAMED[attribLower];
            if (!debuffType) {
              const signed = _TSPY_DEBUFF_SIGNED[attribLower];
              // Resource attrib: only the draining/foe direction (negative scale)
              // is a debuff; the positive direction is a pet self-buff (dropped).
              // Recovery and Endurance are additionally gated to a real magnitude
              // table — their `*_Ones` rows are markers, not computable percents
              // (see the map). No other signed attrib carries that ambiguity.
              const isOnesMarker = _TSPY_ONES_MARKER_ATTRIBS.has(attribLower)
                && /_ones$/i.test(template.table || '');
              if (signed && template.scale < 0 && !isOnesMarker) debuffType = signed;
            }
            // Surfaced per-type resistance: the powers parser now expands a
            // `Res_DMG`-front template into its real `*_Dmg` index attribs and
            // synthesizes aspect='Resistance' (Mind Over Body, Freezing Rain, Tar
            // Patch, …). This replaces the bare `res_dmg`-front `_TSPY_DEBUFF_SIGNED`
            // path above, so recognize the new shape here with the SAME semantics:
            // negative = the foe -Resistance debuff we surface (once, deduped across
            // the per-type rows); positive = a pet self-buff, dropped. Gated to a
            // real magnitude table, mirroring the `*_Ones`-marker exclusion above.
            if (!debuffType && template.aspect === 'Resistance'
                && attribLower.endsWith('_dmg') && template.scale < 0
                && !/_ones$/i.test(template.table || '')) {
              debuffType = 'ResistanceDebuff';
            }
            if (debuffType) {
              const effect = { type: debuffType };
              if (template.scale && template.table) {
                effect.scale = Math.abs(template.scale);
                effect.table = template.table;
              }
              push(effect);
            }
            // tspy ally heal (support pseudo-pets: Triage Beacon, Lifegiving Spores,
            // Spirit Tree). Spelled `Heal_Dmg`, exactly as HC spells it — the duplicate
            // exists because the tspy branch `continue`s before the HC heal block below,
            // so that block is unreachable from here.
            //
            // The ASPECT discriminates, not the attrib name (GAME-DATA §2): the same
            // `Heal_Dmg` carries heal-debuff RESISTANCE at aspect=Resistance, which is not
            // a heal at all. Both amount forms are kept — Absolute is an HP amount off the
            // table, Current on a `*_Ones` table is a max-HP fraction (the rez full-heals).
            if (attribLower === 'heal_dmg' && template.aspect !== 'Resistance'
                && template.scale > 0) {
              const effect = { type: 'Heal' };
              if (template.scale && template.table) {
                effect.scale = Math.abs(template.scale);
                effect.table = template.table;
              }
              push(effect);
            }
            continue; // tspy handled; skip the HC debuff/heal blocks below
          }

          // Check debuff effects
          const debuffType = DEBUFF_ATTRIBS[attribLower];
          if (debuffType) {
            // For endurance drain, check the scale is negative (draining, not granting)
            if (attribLower === 'endurance' && template.scale >= 0) continue;

            const effect = { type: debuffType };
            if (template.scale && template.table) {
              effect.scale = Math.abs(template.scale);
              effect.table = template.table;
            }
            push(effect);
          }

          // Check healing effects (Heal_Dmg = direct ally heal). The aspect gate is the
          // one in the tspy branch above: heal-debuff resistance wears the same attrib.
          if (attribLower === 'heal_dmg' && template.aspect !== 'Resistance') {
            const effect = { type: 'Heal' };
            if (template.scale && template.table) {
              effect.scale = Math.abs(template.scale);
              effect.table = template.table;
            }
            push(effect);
          }
        }

      }
    };

    // Process main templates
    processTemplates(effectGroup.templates, groupProbability(effectGroup.chance));

    // Process child effects. The two rolls compose, but only the ones that ARE rolls:
    // a sentinel parent (`groupProbability` → undefined) leaves its child's own chance
    // standing rather than multiplying it to zero, which is what read every child of a
    // mode-gated group as never happening.
    for (const child of (effectGroup.child_effects || [])) {
      if (isPvpOnlyGroup(child)) continue;
      const outer = groupProbability(effectGroup.chance);
      const inner = groupProbability(child.chance);
      const combined = outer !== undefined && inner !== undefined
        ? outer * inner
        : (outer ?? inner);
      processTemplates(child.templates, combined);
    }
  }

  return effects;
}

/**
 * Read and process a pet power file
 */
function processPetPower(powerFilePath, powerData) {
  if (!powerData) {
    powerData = readJsonFile(powerFilePath);
    if (!powerData) return null;
  }

  // Skip utility powers
  if (isUtilityPower(powerData)) return null;

  // Extract damage and effects
  const damage = extractDamage(powerData);
  const effects = extractEffects(powerData);

  // A power with neither damage nor effects isn't useful - skip it
  if (damage.length === 0 && effects.length === 0) return null;

  const rechargeUnaffected = (powerData.strengths_disallowed || [])
    .some(s => s.toLowerCase() === 'rechargetime');

  // Atom side (job 4): mint this ability's atoms through the powerset converter's
  // collectors so the window_slots pet-merge census can grade the atom path against
  // the PetEffect rows above. Read through the gated reader — the powerset pipeline
  // applies applyVariantGates at load, and a pet ability's atoms must carry the same
  // gates or the census compares two different sources.
  //
  // A caller with no file to re-read mints no atoms. That is the classification tests,
  // which hand-build a powerData to grade one row's face and pass no path; the pipeline
  // itself always has one, since readJsonFile would have returned null otherwise. The
  // throw below is scoped to a path that WAS given and still failed, so the invariant
  // stays loud where it is real instead of firing on a caller that never had a file.
  let atoms;
  if (powerFilePath) {
    try {
      const gated = readGatedPowerFile(powerFilePath);
      const templates = collectAtomTemplates(gated.effects);
      atoms = encodeAtomsForEmit(templates, templates, powerData.name);
    } catch (err) {
      throw new Error(`[pet-atoms] ${powerData.name}: atom emission failed: ${err.message}`);
    }
  }

  return {
    name: powerData.name,
    displayName: powerData.display_name || powerData.name.replace(/_/g, ' '),
    type: powerData.type, // Click, Auto, Toggle
    damage,
    effects: effects.length > 0 ? effects : undefined,
    // The ability's atoms, minted by the powerset converter's collectors (job 4). The
    // EncodedAtom[] wire form is the same shape Power.atoms ships; the window_slots
    // pet-merge census runs these through bag_slots and grades them against `effects`.
    atoms,
    recharge: powerData.recharge_time || 0,
    castTime: powerData.activation_time || 0,
    activatePeriod: powerData.activate_period || undefined,
    effectArea: powerData.effect_area || 'SingleTarget',
    // The recipient every emitted row needs and none of them carries: see the field doc on
    // `PetAbility.targetsAffected`. Omitted when the export states nothing, mirroring the
    // power-converter twin, so absent stays distinguishable from an authored empty list.
    targetsAffected: Array.isArray(powerData.targets_affected) && powerData.targets_affected.length
      ? powerData.targets_affected
      : undefined,
    range: powerData.range > 0 ? powerData.range : undefined,
    radius: powerData.radius > 0 ? powerData.radius : undefined,
    maxTargets: powerData.max_targets_hit > 0 ? powerData.max_targets_hit : undefined,
    // bin-crawler currently exports attack_types as raw enum integers; the
    // PetAbility type expects string tags ("Lethal", "Area", "Incarnate", …).
    // Drop numeric entries until the enum mapping is added to export_powers.
    attackTypes: (() => {
      const at = powerData.attack_types;
      if (!at || at.length === 0) return undefined;
      const strings = at.filter(v => typeof v === 'string');
      return strings.length > 0 ? strings : undefined;
    })(),
    rechargeUnaffected: rechargeUnaffected || undefined,
  };
}

/**
 * Which player power turns a henchman's `_2` / `_3` powerset on, and what it
 * takes away when it does.
 *
 * A Mastermind upgrade is not additive. `Equip_Mercenary` grants
 * `Mastermind_Pets.Soldier_2.Equip` and in the same breath REVOKES
 * `Mastermind_Pets.Soldier.Resistance` — the upgraded henchman has one
 * resistance power, not two. Read as an append, the Skeletal Warrior ends up
 * swinging Hack and Slash once per tier (three times over at tier 3) and the
 * Howler Wolf keeps quoting its un-upgraded 7.5% instead of 12%.
 *
 * The revoke lives on the PLAYER power, so it has to be read from
 * `mastermind_summon/` and joined back onto the pet by target powerset. Keying
 * on the powerset (`mastermind_pets/soldier_2`) rather than on power names is
 * what makes the tier number data-derived: the `_2` / `_3` suffix on what a
 * power grants IS which tier it is, so nothing here depends on power naming or
 * on the level the upgrade unlocks at.
 *
 * Returns `{ granters, revokes }`, both keyed by target powerset:
 *   granters: 'mastermind_pets/soldier_2' -> Set('Equip_Mercenary')
 *   revokes:  'mastermind_pets/soldier'   -> Set('Resistance')
 *   (revokes are keyed under the granter too, so a tier only drops what ITS
 *    own granter revokes.)
 */
const GRANT_ATTRIBS = new Set(['Grant_Power', 'Grant_Boosted_Power']);
const UPGRADE_SOURCE_CATEGORY = 'mastermind_summon';

/** `Mastermind_Pets.Soldier_2.Equip` -> `{ set: 'mastermind_pets/soldier_2', power: 'Equip' }`. */
function splitPowerFullName(fullName) {
  const parts = String(fullName).split('.');
  if (parts.length < 3) return null;
  return {
    set: `${parts[0].toLowerCase()}/${parts[1].toLowerCase()}`,
    power: parts[parts.length - 1],
  };
}

function buildPetUpgradeMap() {
  const granters = new Map(); // targetSet -> Set(playerPowerName)
  const revokes = new Map();  // playerPowerName -> Map(targetSet -> Set(powerName))

  const categoryDir = path.join(POWERS_PATH, UPGRADE_SOURCE_CATEGORY);
  if (!fs.existsSync(categoryDir)) return { granters, revokes };

  for (const setName of fs.readdirSync(categoryDir)) {
    const setDir = path.join(categoryDir, setName);
    if (!fs.statSync(setDir).isDirectory()) continue;

    for (const file of fs.readdirSync(setDir)) {
      if (!file.endsWith('.json') || file === 'index.json') continue;
      const data = readJsonFile(path.join(setDir, file));
      if (!data || !data.name) continue;

      for (const group of data.effects || []) {
        for (const template of group.templates || []) {
          const attrib = (template.attribs || [])[0];
          const names = (template.params || {}).power_names || [];
          if (!names.length) continue;

          if (GRANT_ATTRIBS.has(attrib)) {
            // A grant conditional on owning ANOTHER power is that other power's
            // tier, not this one's. Thunderspy's Equip Mercenary grants the
            // tier-3 set when you also hold Tactical Upgrade; attributing that
            // to Equip would light tier 3 up for a build that never took the
            // second upgrade.
            const requires = `${gateText(template.jit_requires)} ${gateText(group.requires_expression)}`;
            if (/\bownPower\b/.test(requires)) continue;
            for (const name of names) {
              const target = splitPowerFullName(name);
              if (!target) continue;
              if (!granters.has(target.set)) granters.set(target.set, new Set());
              granters.get(target.set).add(data.name);
            }
          } else if (attrib === 'Revoke_Power') {
            for (const name of names) {
              const target = splitPowerFullName(name);
              if (!target) continue;
              if (!revokes.has(data.name)) revokes.set(data.name, new Map());
              const bySet = revokes.get(data.name);
              if (!bySet.has(target.set)) bySet.set(target.set, new Set());
              bySet.get(target.set).add(target.power);
            }
          }
        }
      }
    }
  }

  return { granters, revokes };
}

/** The export's powerset key for a pet powerset directory, e.g. `mastermind_pets/soldier_2`. */
function powersetKey(dirPath) {
  return path.relative(POWERS_PATH, dirPath).split(path.sep).join('/').toLowerCase();
}

/**
 * Scan a power directory and process all power files in it
 * Returns an array of PetAbility objects
 */
function processUpgradeDirectory(dirPath) {
  if (!fs.existsSync(dirPath)) return [];

  const abilities = [];
  const files = fs.readdirSync(dirPath).filter(f => f.endsWith('.json') && f !== 'index.json');

  for (const file of files) {
    const ability = processPetPower(path.join(dirPath, file));
    if (ability) {
      abilities.push(ability);
    }
  }

  return abilities;
}

// The HC export root physically contains the other datasets' export trees
// as subdirectories — never let a whole-tree walk cross into them.
const SIBLING_DATASET_DIRS = new Set(['rebirth', 'thunderspy']);

/** Walk a directory tree and collect every file whose basename matches. */
function findFilesRecursive(rootDir, basename) {
  const out = [];
  function walk(dir) {
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); }
    catch { return; }
    for (const e of entries) {
      const full = path.join(dir, e.name);
      if (e.isDirectory()) {
        if (dir === rootDir && SIBLING_DATASET_DIRS.has(e.name)) continue;
        walk(full);
      } else if (e.isFile() && e.name === basename) out.push(full);
    }
  }
  walk(rootDir);
  return out;
}

/**
 * Pull the pet's lifespan (seconds) out of its Self_Destruct power.
 *
 * Pet lifespans aren't stored on the entity record. They're encoded as a
 * Silent_Kill AttribMod inside each pet's bundled Self_Destruct Auto power:
 * the pet auto-fires Self_Destruct on spawn, and the Silent_Kill's `Delay`
 * field is when the despawn actually triggers.
 *
 * The HC (Parse7) export labels it `Silent_Kill` since the special-attrib
 * sub-index fix; the Rebirth (Parse6) export still decodes the shared index
 * 117 as `Create_Entity`, so we accept both and disambiguate by signature:
 * target=Self, stack=Stack, table='Melee_Ones', no EntCreate params.
 * Permanent pets (mastermind primaries that last until killed) either have
 * no Self_Destruct power at all or have one with delay=0. (Thunderspy
 * labels it `Ones` with empty target/stack — TSPY4, not handled here.)
 */
function extractLifespan(powerFilePath) {
  const data = readJsonFile(powerFilePath);
  if (!data) return null;
  for (const effectGroup of (data.effects || [])) {
    for (const t of (effectGroup.templates || [])) {
      const attribs = t.attribs || [];
      if (!attribs.includes('Silent_Kill') && !attribs.includes('Create_Entity')) continue;
      if (t.target !== 'Self') continue;
      if (t.stack !== 'Stack') continue;
      if (t.table !== 'Melee_Ones') continue;
      if (t.params) continue; // real Create_Entity has EntCreate params
      const delay = typeof t.delay === 'number' ? t.delay : 0;
      if (delay > 0) return delay;
    }
  }
  return null;
}

/**
 * The delay on a pet's bundled Self_Destruct `Silent_Kill`, or null when it has
 * none. Unlike `extractLifespan` this KEEPS a zero delay: an immediate self-kill
 * is exactly the case that matters for `oneShot` below, and it is the case
 * `extractLifespan` deliberately discards (0 means "no finite lifespan").
 *
 * Accepts the same Parse7 `Silent_Kill` / Parse6 `Create_Entity` aliasing as
 * `extractLifespan`, disambiguated the same way (target=Self, no EntCreate params).
 */
function extractSelfDestructDelay(powerFilePath) {
  const data = readJsonFile(powerFilePath);
  if (!data) return null;
  for (const effectGroup of (data.effects || [])) {
    for (const t of (effectGroup.templates || [])) {
      const attribs = t.attribs || [];
      if (!attribs.includes('Silent_Kill') && !attribs.includes('Create_Entity')) continue;
      if (t.target !== 'Self') continue;
      if (t.params) continue; // real Create_Entity has EntCreate params
      return typeof t.delay === 'number' ? t.delay : 0;
    }
  }
  return null;
}

/**
 * Does this pet detonate exactly once?
 *
 * Trip Mine, Time Bomb, Seeker Drone and High Explosives are bombs: they sit
 * armed for the whole summon window, fire once when triggered, and are destroyed
 * by their own bundled Self_Destruct. The damage layer otherwise models fires
 * -per-spawn as `summonDuration / (castTime + recharge)`, which for a Blaster's
 * Trip Mine (260s window, 20s attack recharge) says the mine detonates THIRTEEN
 * times. Only the Controller/Corruptor/Mastermind mine escaped, because its
 * shared entity carries a 1000s recharge that happens to round the same formula
 * down to one.
 *
 * The recharge is the wrong basis either way — a destroyed pet cannot recharge —
 * so mark the shape and let the damage layer cap it at one.
 *
 * Two conditions, both required, because neither alone is safe:
 *   • an IMMEDIATE self-targeted Silent_Kill (delay ≤ 1s), i.e. the pet dies with
 *     its detonation rather than living out a lifespan (Enflame's is 5s, Oil
 *     Slick's 15s — those really do tick for their whole delay); and
 *   • its whole offensive kit is ONE damaging Click and no damaging Auto/Toggle.
 *     Mastermind henchmen and VEAT pets also carry a delay-0 Self_Destruct — it
 *     is their *dismiss* power, not a detonation — but they field several attacks
 *     and an AI that keeps using them, so the ability-count test excludes them.
 *
 * Verified against the whole pet corpus: flags 18 entities — the trip mines, time
 * bombs, Seeker Drone flashpulses and High Explosives — and no repeat attacker
 * (arachnobots, widows, Mu pets, coral guardians, psionic nexus all excluded).
 */
function detectOneShot(abilities, selfDestructDelay) {
  if (selfDestructDelay === null || selfDestructDelay > 1) return false;
  const damaging = abilities.filter(a => a.damage.length > 0);
  if (damaging.length !== 1) return false;
  return damaging[0].type === 'Click';
}

/**
 * The entities one pet power creates IN PLACE — its `EntCreate` AttribMods at `target: Self`.
 *
 * A pet's payload can be one summon deeper: Poison Trap's pet carries only a Self_Destruct and
 * a self-resistance, and the choke/vomit/−Regen everyone associates with the power lives in the
 * `Pets_*_Poison_Gas` entity that Self_Destruct creates as the trap dies (ENT-3 step 4).
 *
 * `target` is the discriminator, and it is the §2 recipient axis doing its usual job. An
 * EntCreate at `Self` puts the new entity where the PET is, so it continues the same payload at
 * the same place — the gas cloud, Oil Slick's fire, the Voltaic Sentinel the Mu Guardian calls.
 * An EntCreate at `AnyAffected` puts one copy on EACH target the pet's power hit: Jolting Chain's
 * Jump1 spawns Jump2 on the foe it just zapped, which spawns Jump3 on the next one. Those are
 * per-target quantities, and folding one copy into a per-target-agnostic bag would state a
 * three-link chain's damage as if all of it landed on one foe. They are the larger population
 * (79 / 91 / 88 edges against 19 / 23 / 16 here), and they stay out until the `per_target` axis
 * can carry them.
 *
 * Names are emitted whether or not an entity record exists for them — the export carries only
 * player-facing pets, so a pet that creates an NPC entity (Fire Imps, Dust Devils) names one
 * that isn't there. Filtering it here would hide a real fact about the export's scope; no
 * player-summonable pet reaches one, which is what `pet_summon_chain.rs` asserts.
 */
function createsInPlace(powerData) {
  const out = [];
  for (const effectGroup of (powerData.effects || [])) {
    for (const t of (effectGroup.templates || [])) {
      const params = t.params || {};
      if (params.type !== 'EntCreate' || !params.entity_def) continue;
      if (t.target !== 'Self') continue;
      if (!out.includes(params.entity_def)) out.push(params.entity_def);
    }
  }
  return out;
}

/**
 * Read an entity file and extract its powers
 */
function processEntity(entityFilePath, upgradeMap) {
  const entityData = readJsonFile(entityFilePath);
  if (!entityData) return null;

  const defaults = entityData.defaults || {};
  const powerFullNames = defaults.power_full_names || [];
  const displayNames = defaults.power_display_names || [];

  // Get display name from levels
  let displayName = entityData.name.replace(/^(Pets_|MastermindPets_|IncarnatePets_)/i, '').replace(/_/g, ' ');
  if (entityData.levels?.length > 0 && entityData.levels[0].display_names?.length > 0) {
    displayName = entityData.levels[0].display_names[0];
  }

  // Process each power and track powerset paths for upgrade tier scanning
  const abilities = [];
  const powersetPaths = new Set(); // Track unique powerset directories
  let lifespan = null;
  let selfDestructDelay = null;
  const createsEntities = [];

  for (let i = 0; i < powerFullNames.length; i++) {
    const fullName = powerFullNames[i]; // e.g., "Pets.Tornado.Tornado_Attack"
    const parts = fullName.split('.');
    if (parts.length < 3) continue;

    const category = parts[0].toLowerCase(); // "pets"
    const powerset = parts[1].toLowerCase(); // "tornado"
    const power = parts[2].toLowerCase();    // "tornado_attack"

    // Track powerset directory path for upgrade scanning
    powersetPaths.add(path.join(POWERS_PATH, category, powerset));

    // Build the file path
    const powerFilePath = path.join(POWERS_PATH, category, powerset, `${power}.json`);

    if (!fs.existsSync(powerFilePath)) {
      // Try without underscores
      const altPath = path.join(POWERS_PATH, category, powerset, `${power.replace(/ /g, '_')}.json`);
      if (!fs.existsSync(altPath)) continue;
    }

    // Pet lifespan: harvested from the bundled Self_Destruct power's
    // Silent_Kill delay. Recorded once per entity. Pets without a finite
    // lifespan (Mastermind primaries that die only to enemy damage) either
    // have no Self_Destruct or its delay is 0 — leave `lifespan` null.
    if (power === 'self_destruct' && lifespan === null) {
      lifespan = extractLifespan(powerFilePath);
    }
    // Same power, different question: `oneShot` needs the raw delay including
    // zero, which `extractLifespan` throws away. Matched on the power NAME
    // ending in self_destruct so the Dominator/Epic mines' `TripMine_SelfDestruct`
    // is seen too (they prefix the powerset onto every power name).
    if (/self_?destruct$/.test(power) && selfDestructDelay === null) {
      selfDestructDelay = extractSelfDestructDelay(powerFilePath);
    }

    const powerData = readJsonFile(powerFilePath);
    if (!powerData) continue;

    // Read BEFORE the ability filter, not after it: every in-place summon this corpus has
    // rides on a power the vocabulary rejects — Self_Destruct is a utility name, Oil Slick's
    // Res_Target and Generate_Target carry no combat attrib — so scanning only the powers that
    // became abilities would find none of them.
    for (const child of createsInPlace(powerData)) {
      if (!createsEntities.includes(child)) createsEntities.push(child);
    }

    const ability = processPetPower(powerFilePath, powerData);
    if (ability) {
      abilities.push(ability);
    }
  }

  // Scan for upgrade tier directories (_2 and _3), and resolve what turns each
  // one on and what it takes away — see buildPetUpgradeMap.
  // Every powerset this pet draws powers from — base and both tiers. A tier-3
  // upgrade routinely revokes a tier-2 power, so the tier sets belong here too.
  const ownSets = new Set();
  for (const psPath of powersetPaths) {
    ownSets.add(powersetKey(psPath));
    for (const tier of [2, 3]) ownSets.add(powersetKey(`${psPath}_${tier}`));
  }
  const upgradeTiers = [];
  for (const psPath of powersetPaths) {
    for (const tier of [2, 3]) {
      const tierDir = `${psPath}_${tier}`;
      const abilities = processUpgradeDirectory(tierDir);
      if (abilities.length === 0) continue;

      const tierKey = powersetKey(tierDir);
      const grantedBy = [...((upgradeMap && upgradeMap.granters.get(tierKey)) || [])].sort();

      // Only what THIS tier's granter revokes, and only against powersets this
      // pet actually owns — an upgrade power revokes across every henchman it
      // touches, and the Soldier must not lose the Medic's Brawl.
      const revoked = new Set();
      for (const granter of grantedBy) {
        const bySet = (upgradeMap && upgradeMap.revokes.get(granter)) || new Map();
        for (const [targetSet, powers] of bySet) {
          if (!ownSets.has(targetSet)) continue;
          for (const power of powers) revoked.add(power);
        }
      }

      upgradeTiers.push({
        tier,
        abilities,
        grantedBy: grantedBy.length > 0 ? grantedBy : undefined,
        revokes: revoked.size > 0 ? [...revoked].sort() : undefined,
      });
    }
  }

  // The class the pet's own magnitudes resolve against (ENT-10), so a default here would be a
  // guess shipped as a number rather than an absence anyone could see. Every entity in all
  // three forks states one, so this throws rather than falling back.
  if (!defaults.character_class_name) {
    throw new Error(
      `convert-pet-entities: entity '${entityData.name}' states no character_class_name — its `
        + 'magnitudes have no class to resolve against.',
    );
  }

  return {
    name: entityData.name,
    displayName,
    characterClass: defaults.character_class_name,
    commandable: entityData.commandable_pet === 1,
    copyCreatorMods: entityData.copy_creator_mods === true,
    abilities,
    lifespan: lifespan ?? undefined,
    oneShot: detectOneShot(abilities, selfDestructDelay) || undefined,
    createsEntities: createsEntities.length > 0 ? createsEntities : undefined,
    upgradeTiers: upgradeTiers.length > 0 ? upgradeTiers : undefined,
    // Verbatim, not the classified `abilities` above: the sidecar's whole job is to match
    // a summon's redirect list against what the entity declares, and `abilities` has
    // already dropped every power this script's effect vocabulary did not recognize.
    // Matching against a filtered list would fail on exactly the pets whose powers are
    // least understood. Not emitted into pet-entities.ts — `generateTypeScript` names the
    // fields it writes, so this stays a build-time fact.
    declaredPowers: powerFullNames,
  };
}

/**
 * A committed artifact must be a function of the export, not of the order the filesystem
 * happened to hand back its files.
 *
 * Both sidecars below were built by inserting into an object as a directory walk reached each
 * file, so their key order was `readdir`'s. That is stable on one machine and not across two:
 * regenerating `self-destruct-delays.json` with no change to the export moved two Defender
 * entries to the end of the file, which reads in a diff exactly like data moving. Sorting makes
 * the diff say only what actually changed.
 */
function sortedByKey(obj) {
  return Object.fromEntries(Object.keys(obj).sort().map((k) => [k, obj[k]]));
}

/**
 * Main execution
 */
function main() {
  console.log('Converting pet entity data...\n');

  const entities = {};
  let totalAbilities = 0;
  let noAbilities = 0;

  // Process all pet entity files
  const entityFiles = fs.readdirSync(ENTITIES_PATH)
    .filter(f => f.endsWith('.json') && (
      f.startsWith('pets_') ||
      f.startsWith('mastermindpets_') ||
      f.startsWith('incarnatepets_')
    ))
    .sort();

  console.log(`Found ${entityFiles.length} pet entity files\n`);

  const upgradeMap = buildPetUpgradeMap();
  console.log(
    `Upgrade grants: ${upgradeMap.granters.size} pet powersets granted by ` +
    `${new Set([...upgradeMap.granters.values()].flatMap((s) => [...s])).size} player powers, ` +
    `${upgradeMap.revokes.size} of which revoke\n`
  );

  for (const file of entityFiles) {
    const filePath = path.join(ENTITIES_PATH, file);
    const entity = processEntity(filePath, upgradeMap);

    if (!entity) continue;

    // Emitted whether or not any of its powers classified. Class, commandability and
    // lifespan are facts independent of this script's effect vocabulary, and a summoning
    // power names its pet by NAME: withholding the record makes that name unresolvable,
    // which both Rust consumers (`granted::pseudo_pet_effects`, `buff_pets::each_folded_row`)
    // answer by skipping in silence — the power then shows and contributes nothing (ENT-3).
    // So absence from this table means "the export has no such entity" and nothing else,
    // which is the invariant `summon_resolution.rs` asserts.
    if (entity.abilities.length === 0) noAbilities++;

    entities[entity.name] = entity;
    totalAbilities += entity.abilities.length;
  }

  console.log(`\nProcessed ${Object.keys(entities).length} entities with ${totalAbilities} abilities`);
  console.log(`${noAbilities} of them carry no classified ability\n`);

  // Generate TypeScript
  const tsContent = generateTypeScript(entities);
  fs.writeFileSync(OUTPUT_PATH, tsContent);
  console.log(`Wrote ${OUTPUT_PATH}`);

  // Emit sidecar JSON of pet lifespans for convert-powerset to consume.
  // Only entities with a real positive lifespan land here; permanent pets are absent.
  const lifespans = {};
  for (const [name, entity] of Object.entries(entities)) {
    if (typeof entity.lifespan === 'number' && entity.lifespan > 0) {
      lifespans[name] = entity.lifespan;
    }
  }
  fs.writeFileSync(SIDECAR_LIFESPANS_PATH, JSON.stringify(sortedByKey(lifespans), null, 2) + '\n');
  console.log(`Wrote ${SIDECAR_LIFESPANS_PATH} (${Object.keys(lifespans).length} entries)`);

  // Build the Self_Destruct delay map by walking every category for
  // `self_destruct.json` files. The pseudopet pathway (PL_StaticObject,
  // Vines) routes through `params.redirects` rather than the entity record,
  // so convert-powerset needs to resolve a dotted redirect name (e.g.
  // `Redirects.Gravity_Control.Self_Destruct`) to its delay independently
  // of the pet entity table.
  const selfDestructDelays = {};
  const allSelfDestructFiles = findFilesRecursive(POWERS_PATH, 'self_destruct.json');
  for (const filePath of allSelfDestructFiles) {
    const delay = extractLifespan(filePath);
    if (delay === null) continue;
    const data = readJsonFile(filePath);
    const fullName = data && data.full_name;
    if (fullName) selfDestructDelays[fullName] = delay;
  }
  fs.writeFileSync(SIDECAR_SELF_DESTRUCT_PATH, JSON.stringify(sortedByKey(selfDestructDelays), null, 2) + '\n');
  console.log(`Wrote ${SIDECAR_SELF_DESTRUCT_PATH} (${Object.keys(selfDestructDelays).length} entries)`);

  // Entity -> declared powers, sorted by entity name so the committed file is a function of
  // the export alone and not of readdir order. Entities that declare nothing are omitted:
  // they can never be the answer to a redirect-list lookup, and an empty list would match
  // an empty redirect list, which is a summon that names no powers at all.
  const entityPowers = {};
  for (const name of Object.keys(entities).sort()) {
    const declared = entities[name].declaredPowers || [];
    if (declared.length > 0) entityPowers[name] = declared;
  }
  fs.writeFileSync(SIDECAR_ENTITY_POWERS_PATH, JSON.stringify(entityPowers, null, 2) + '\n');
  console.log(`Wrote ${SIDECAR_ENTITY_POWERS_PATH} (${Object.keys(entityPowers).length} entries)`);

  // Print summary for our 3 target entities
  const targets = ['Pets_Tornado', 'Pets_LightningStorm', 'Pets_Gremlin_Controller'];
  console.log('\nTarget entities:');
  for (const name of targets) {
    const entity = entities[name];
    if (entity) {
      console.log(`  ${name}: ${entity.abilities.length} abilities (class: ${entity.characterClass}, copyMods: ${entity.copyCreatorMods})`);
      for (const ability of entity.abilities) {
        const dmgStr = ability.damage.length > 0
          ? ability.damage.map(d => `${d.damageType} s${d.scale}@${d.table}`).join(', ')
          : 'no damage';
        console.log(`    - ${ability.displayName} (${ability.type}): ${dmgStr} | recharge=${ability.recharge}s cast=${ability.castTime}s`);
      }
    } else {
      console.log(`  ${name}: NOT FOUND`);
    }
  }
}

/**
 * One ability literal, at `indent`. Both places an ability is written — an entity's own
 * abilities and a Mastermind upgrade tier's — go through here, because they held two copies
 * of one field list and a field added to either alone would reach only half the corpus.
 */
function abilityLines(ability, indent) {
  const lines = [`${indent}{`];
  const field = (name, value) => lines.push(`${indent}  ${name}: ${value},`);
  field('name', JSON.stringify(ability.name));
  field('displayName', JSON.stringify(ability.displayName));
  field('type', JSON.stringify(ability.type));
  field('damage', JSON.stringify(ability.damage));
  if (ability.effects) field('effects', JSON.stringify(ability.effects));
  if (ability.atoms) field('atoms', JSON.stringify(ability.atoms));
  field('recharge', ability.recharge);
  field('castTime', ability.castTime);
  if (ability.activatePeriod) field('activatePeriod', ability.activatePeriod);
  field('effectArea', JSON.stringify(ability.effectArea));
  if (ability.targetsAffected) field('targetsAffected', JSON.stringify(ability.targetsAffected));
  if (ability.range) field('range', ability.range);
  if (ability.radius) field('radius', ability.radius);
  if (ability.maxTargets) field('maxTargets', ability.maxTargets);
  if (ability.attackTypes) field('attackTypes', JSON.stringify(ability.attackTypes));
  if (ability.rechargeUnaffected) field('rechargeUnaffected', true);
  lines.push(`${indent}},`);
  return lines;
}

function generateTypeScript(entities) {
  const lines = [];

  lines.push(`/**`);
  lines.push(` * Pet Entity Data`);
  lines.push(` * Auto-generated from Homecoming raw data`);
  lines.push(` *`);
  lines.push(` * Contains pet abilities for damage calculation.`);
  lines.push(` * Use with PET_TABLES from at-tables.ts for damage lookups.`);
  lines.push(` */`);
  lines.push(``);
  lines.push(`export interface PetDamageEntry {`);
  lines.push(`  damageType: string;`);
  lines.push(`  scale: number;`);
  lines.push(`  table: string;`);
  lines.push(`  /** Sub-1.0 hit chance from the effect group (Trip Mine's third Fire`);
  lines.push(`   *  template lands 50% of the time). Absent = guaranteed. Damage layers`);
  lines.push(`   *  must weight by this; summing at face value overstates the power. */`);
  lines.push(`  chance?: number;`);
  lines.push(`}`);
  lines.push(``);
  lines.push(`export interface PetEffect {`);
  lines.push(`  type: string;`);
  lines.push(`  magnitude?: number;`);
  lines.push(`  chance?: number;`);
  lines.push(`  scale?: number;`);
  lines.push(`  table?: string;`);
  lines.push(`  /** The movement axis a Slow / MovementCapDebuff row applies to, spelled the`);
  lines.push(`   *  way a parent power's \`slow[axis]\` spells it. A power states several axes`);
  lines.push(`   *  at different scales, so the merge holds one value per axis rather than`);
  lines.push(`   *  one per key. Absent on every other type. */`);
  lines.push(`  axis?: string;`);
  lines.push(`  /** Ally-buff auras (buff-pets like Force Field Generator / Barrier Reef).`);
  lines.push(`   *  A DefenseBuff/ResistanceBuff aura buffs every listed sub-type at the`);
  lines.push(`   *  same scale/table; absorbAspect distinguishes MaxHP-fraction (Maximum)`);
  lines.push(`   *  from flat (Absolute) absorb. Folded into character totals when the`);
  lines.push(`   *  summon's buff-pet toggle is enabled. */`);
  lines.push(`  defenseTypes?: string[];`);
  lines.push(`  resistanceTypes?: string[];`);
  lines.push(`  absorbAspect?: string;`);
  lines.push(`  /** The pet's OWN defensive profile (\`target: Self\` templates) —`);
  lines.push(`   *  \`SelfResistance\` / \`SelfDefense\` / \`SelfMezProtection\` /`);
  lines.push(`   *  \`SelfMezResistance\` / \`SelfDebuffResistance\`. Deliberately NOT the`);
  lines.push(`   *  ally-aura type names above: those fold into the PLAYER's totals, and a`);
  lines.push(`   *  pet's own resistance is not the player's. \`scale\` stays SIGNED on`);
  lines.push(`   *  SelfResistance (a negative is a real vulnerability). */`);
  lines.push(`  mezTypes?: string[];`);
  lines.push(`  debuffTypes?: string[];`);
  lines.push(`  /** The source template carried IgnoreStrength: the summoner's slotting does`);
  lines.push(`   *  not reach this effect even though the pet inherits it via CopyBoosts. */`);
  lines.push(`  ignoreStrength?: boolean;`);
  lines.push(`}`);
  lines.push(``);
  lines.push(`export interface PetAbility {`);
  lines.push(`  name: string;`);
  lines.push(`  displayName: string;`);
  lines.push(`  type: 'Click' | 'Auto' | 'Toggle';`);
  lines.push(`  damage: PetDamageEntry[];`);
  lines.push(`  effects?: PetEffect[];`);
  lines.push(`  /** The ability's atoms, minted by the powerset converter's collectors (job 4).`);
  lines.push(`   *  Same EncodedAtom[] wire form as Power.atoms; the window_slots pet-merge`);
  lines.push(`   *  census runs these through bag_slots and grades them against effects. */`);
  lines.push(`  atoms?: unknown[];`);
  lines.push(`  recharge: number;`);
  lines.push(`  castTime: number;`);
  lines.push(`  activatePeriod?: number;`);
  lines.push(`  effectArea: string;`);
  lines.push(`  /** EntsAffected — which entity categories this ability's effects can land on.`);
  lines.push(`   *  Every effect row above is authored \`target: AnyAffected\`, and that word names`);
  lines.push(`   *  whoever the ability affects, so this is the only field that separates a`);
  lines.push(`   *  protection the pet grants its SUMMONER (Force Field Generator's Dispersion`);
  lines.push(`   *  Bubble, \`['Friend','Self']\`) from one it inflicts on the foe it just held`);
  lines.push(`   *  (Singularity's Gravity Distortion, \`['Foe']\`) — identical type names, identical`);
  lines.push(`   *  scales. The pet is the caster here, so the polarity inverts against a player`);
  lines.push(`   *  power: \`Self\` is the pet alone and the summoner arrives as \`Friend\`,`);
  lines.push(`   *  \`MyOwner\` or \`Teammate\` (register ENT-12; the power-converter twin is`);
  lines.push(`   *  MEZRES-3). Omitted when the export states nothing, so absent stays`);
  lines.push(`   *  distinguishable from an authored empty list. */`);
  lines.push(`  targetsAffected?: string[];`);
  lines.push(`  range?: number;`);
  lines.push(`  radius?: number;`);
  lines.push(`  maxTargets?: number;`);
  lines.push(`  attackTypes?: string[];`);
  lines.push(`  rechargeUnaffected?: boolean;`);
  lines.push(`}`);
  lines.push(``);
  lines.push(`export interface PetUpgradeTier {`);
  lines.push(`  tier: number;`);
  lines.push(`  abilities: PetAbility[];`);
  lines.push(`  /** The player power(s) that turn this tier on, by internal name`);
  lines.push(`   *  (\`Equip_Mercenary\`, \`Tactical_Upgrade\`). Derived from which powerset the`);
  lines.push(`   *  grant targets — the \`_2\`/\`_3\` suffix IS the tier — so it holds whatever a`);
  lines.push(`   *  server names its upgrades. Absent when the export carries no resolved`);
  lines.push(`   *  grant targets (Thunderspy), in which case a consumer cannot tell from the`);
  lines.push(`   *  build alone whether the tier is active. */`);
  lines.push(`  grantedBy?: string[];`);
  lines.push(`  /** Abilities this tier TAKES AWAY, by name. An upgrade replaces rather than`);
  lines.push(`   *  adds: Equip Mercenary revokes the Soldier's base Resistance as it grants`);
  lines.push(`   *  Equip, and Enchant Undead revokes the Skeletal Warrior's base Hack and`);
  lines.push(`   *  Slash as it grants its own. Appending a tier without applying these`);
  lines.push(`   *  double-counts the attacks and leaves the stale passive on top. */`);
  lines.push(`  revokes?: string[];`);
  lines.push(`}`);
  lines.push(``);
  lines.push(`export interface PetEntity {`);
  lines.push(`  name: string;`);
  lines.push(`  displayName: string;`);
  lines.push(`  characterClass: string;`);
  lines.push(`  commandable: boolean;`);
  lines.push(`  copyCreatorMods: boolean;`);
  lines.push(`  abilities: PetAbility[];`);
  lines.push(`  /** Pet lifespan in seconds (from bundled Self_Destruct power's Silent_Kill delay).`);
  lines.push(`   *  Omitted for permanent pets (mastermind primaries, etc.) that despawn only`);
  lines.push(`   *  when killed or unsummoned. Used by convert-powerset to populate`);
  lines.push(`   *  \`summon.duration\` for summoning powers whose EntCreate Duration is 0. */`);
  lines.push(`  lifespan?: number;`);
  lines.push(`  /** This pet detonates ONCE: it is destroyed by its own bundled`);
  lines.push(`   *  Self_Destruct the moment it fires (trip mines, time bombs, seeker`);
  lines.push(`   *  drones, high explosives). Its attack's recharge is therefore not a`);
  lines.push(`   *  repeat cadence — damage layers must cap fires-per-spawn at 1 rather`);
  lines.push(`   *  than dividing the summon window by the cycle time. */`);
  lines.push(`  oneShot?: boolean;`);
  lines.push(`  /** Entity defs this pet's own powers create IN PLACE (an \`EntCreate\` at`);
  lines.push(`   *  \`target: Self\`), in walk order. A pet's payload can be one summon deeper —`);
  lines.push(`   *  Poison Trap's gas cloud, Oil Slick's fire, the Mu Guardian's Voltaic Sentinel —`);
  lines.push(`   *  so the consumers that merge a pseudo-pet's kit into its summoning power follow`);
  lines.push(`   *  this chain. Each name is a key into this same record and keeps its OWN class row,`);
  lines.push(`   *  commandability and lifespan, which is why the link travels instead of the child's`);
  lines.push(`   *  abilities being folded into \`abilities\` here: pet damage resolves against`);
  lines.push(`   *  \`characterClass\`, and Oil Slick's burn is a different class from its oil.`);
  lines.push(`   *  A name with no record means the export doesn't carry that entity (NPC-only);`);
  lines.push(`   *  no player-summonable pet reaches one. */`);
  lines.push(`  createsEntities?: string[];`);
  lines.push(`  upgradeTiers?: PetUpgradeTier[];`);
  lines.push(`}`);
  lines.push(``);
  lines.push(`export const PET_ENTITIES: Record<string, PetEntity> = {`);

  for (const [name, entity] of Object.entries(entities)) {
    lines.push(`  ${JSON.stringify(name)}: {`);
    lines.push(`    name: ${JSON.stringify(entity.name)},`);
    lines.push(`    displayName: ${JSON.stringify(entity.displayName)},`);
    lines.push(`    characterClass: ${JSON.stringify(entity.characterClass)},`);
    lines.push(`    commandable: ${entity.commandable},`);
    lines.push(`    copyCreatorMods: ${entity.copyCreatorMods},`);
    if (typeof entity.lifespan === 'number' && entity.lifespan > 0) {
      lines.push(`    lifespan: ${entity.lifespan},`);
    }
    if (entity.oneShot) lines.push(`    oneShot: true,`);
    if (entity.createsEntities) {
      lines.push(`    createsEntities: ${JSON.stringify(entity.createsEntities)},`);
    }
    lines.push(`    abilities: [`);

    for (const ability of entity.abilities) {
      lines.push(...abilityLines(ability, '      '));
    }

    lines.push(`    ],`);

    // Upgrade tiers (for Mastermind pets)
    if (entity.upgradeTiers) {
      lines.push(`    upgradeTiers: [`);
      for (const tier of entity.upgradeTiers) {
        lines.push(`      {`);
        lines.push(`        tier: ${tier.tier},`);
        if (tier.grantedBy) lines.push(`        grantedBy: ${JSON.stringify(tier.grantedBy)},`);
        if (tier.revokes) lines.push(`        revokes: ${JSON.stringify(tier.revokes)},`);
        lines.push(`        abilities: [`);
        for (const ability of tier.abilities) {
          lines.push(...abilityLines(ability, '          '));
        }
        lines.push(`        ],`);
        lines.push(`      },`);
      }
      lines.push(`    ],`);
    }

    lines.push(`  },`);
  }

  lines.push(`};`);
  lines.push(``);

  return lines.join('\n');
}

// The classification is exported so a test can feed it a template the shipped corpus never
// states. Both guards on `Regeneration` — the aspect fork and the debuff-direction fork — select
// nothing in any fork today, so a corpus-driven gate would go green with either one deleted; the
// only way to grade them is to build the violating case (see pet-regen-classification.test.ts).
// `--dataset thunderspy` selects the tspy branch, exactly as a regen run does.
// `processPetPower` is exported for the recipient half of the gate: every shipped ability states a
// `targetsAffected`, so the corpus cannot grade what an export that states none produces, and only
// the record builder can be asked directly.
// `extractSelfBuff` and `extractBuffAura` are exported for the disjointness gate: the pet's own
// stat sheet and the ally-aura vocabulary must never share a type name, and only asking the two
// classifiers directly can prove it (pet-self-buffs.test.ts).
module.exports = {
  extractEffects,
  processPetPower,
  extractSelfBuff,
  extractBuffAura,
  SELF_BUFF_EXCLUDED_POWERSETS,
};

if (require.main === module) {
  main();
}
