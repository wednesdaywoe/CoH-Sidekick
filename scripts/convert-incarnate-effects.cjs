#!/usr/bin/env node
/**
 * convert-incarnate-effects.cjs
 *
 * Extracts incarnate effects from bin-crawler JSON and generates a TS
 * data file with correct values for all incarnate slots.
 *
 * Reads from: exported_powers[/<datasetId>]/incarnate/
 *   - alpha/ + alpha_silent/       → Enhancement bonuses + level shift
 *   - destiny/ + destiny_silent/   → Click buffs with diminishing durations + level shift
 *   - hybrid/ + hybrid_silent/     → Passive + front-loaded + per-target toggle effects
 *   - interface/ + interface_silent/ → Proc debuffs on enemies
 *
 * Outputs:
 *   pipeline/<id>/incarnate-effects.json
 *
 * Usage: node scripts/convert-incarnate-effects.cjs [--dataset <id>] [--dry-run]
 */

const fs = require('fs');
const path = require('path');

const { gateText } = require('./_gate-tokens.cjs');
const { parseDatasetArg, pipelinePath, REPO_ROOT } = require('./_dataset-paths.cjs');
const { isPvpOnlyGroup: isPvpOnlyByRequires } = require('./_pv-scope.cjs');
const datasetId = parseDatasetArg();

// Source: bin-crawler JSON export. HC keeps the legacy flat layout
// (`exported_powers/incarnate/`); other datasets are namespaced
// (`exported_powers/<id>/incarnate/`).
const RAW_DATA_BASE = path.join(__dirname, '..', 'exported_powers');
const RAW_BASE = (datasetId === 'homecoming' && !fs.existsSync(path.join(RAW_DATA_BASE, datasetId, 'incarnate')))
  ? path.join(RAW_DATA_BASE, 'incarnate')
  : path.join(RAW_DATA_BASE, datasetId, 'incarnate');

const OUTPUT_FILE = pipelinePath(datasetId, 'incarnate-effects.json');
const DRY_RUN = process.argv.includes('--dry-run');

// ============================================
// UTILITY FUNCTIONS
// ============================================

function readJson(filePath) {
  let text;
  try {
    text = fs.readFileSync(filePath, 'utf8');
  } catch (err) {
    // Absent is a real state — the HC-reference recovery probes ask for files
    // that legitimately don't exist. Anything else must surface.
    if (err.code === 'ENOENT') return null;
    throw err;
  }
  try {
    return JSON.parse(text);
  } catch (err) {
    throw new Error(`Corrupt JSON in ${filePath}: ${err.message}`, { cause: err });
  }
}

// Every judgement/lore/verdict/data main file in all three exports carries a
// non-zero recharge_time (censused 2026-07-20); a missing one is a broken
// export, not a cooldown to guess at.
function requireRechargeTime(data, sourceName) {
  if (!data.recharge_time) {
    throw new Error(`${sourceName}: missing recharge_time — refusing to fabricate a cooldown`);
  }
  return data.recharge_time;
}

/** Template table name across export shapes: the alpha-tree tables are objects
 *  ({ column_name }), the power trees carry plain strings. */
function templateTableName(t) {
  if (t.table && typeof t.table === 'object') return t.table.column_name || '';
  return typeof t.table === 'string' ? t.table : '';
}

// Every damage/DoT template in all three exports names its table (censused
// 2026-07-21, DOT-TABLE-1); a blank one is a misparse to surface, never a
// table to guess at.
function requireTemplateTable(t, sourceName) {
  const name = templateTableName(t);
  if (!name) {
    throw new Error(`${sourceName}: damage template carries no table name — refusing to guess one`);
  }
  return name;
}

/** Iterate templates depth-first through nested child_effects — HC hides its
 *  interface DoT templates one child level down (an incarnate-content-gated /
 *  default table pair), where a top-level walk never sees them. */
function* walkTemplates(effects) {
  for (const eff of effects || []) {
    yield* eff.templates || [];
    yield* walkTemplates(eff.child_effects);
  }
}

/** Thunderspy names every judgement/verdict damage attrib with its own generic
 *  `Tempdamage` enum entry; the damage TYPE lives only in the authored short
 *  help — "Extreme DMG (Cold)". Null when the help names none, which keeps the
 *  visible 'Unknown' marker rather than guessing a type. */
function damageTypeFromShortHelp(shortHelp) {
  const match = /DMG\s*\(([^)]+)\)/i.exec(shortHelp || '');
  return match ? match[1].trim() : null;
}

function readDir(dirPath) {
  try {
    // Sort so output order is deterministic across platforms (NTFS readdir is
    // alphabetical, ext4 is hash-order) — the regen-diff CI guard runs on Linux
    // and must byte-match output committed from Windows.
    return fs
      .readdirSync(dirPath)
      .filter(f => f.endsWith('.json') && f !== 'index.json')
      .sort();
  } catch {
    return [];
  }
}

/** T3+ alpha and destiny tiers grant +1 level shift, and since the
 *  special-attrib sub-index fix the export states it outright: Alpha T3+
 *  mains carry a `Combat_Mod_Shift` template directly; Destiny T3+ mains
 *  Grant_Power a `*_Silent.Level_Shift` power whose own template carries the
 *  shift (the same grant shape Lore uses). The magnitude is the template's
 *  scale, never assumed. A level-shift grant whose silent file can't produce
 *  a readable `Combat_Mod_Shift` is a misparse signal, not a zero.
 *  `resolveSilent` maps a silent filename base (e.g. "level_shift") to its
 *  parsed JSON, or null when the file doesn't exist. */
function extractLevelShift(data, resolveSilent, sourceName) {
  let shift = 0;
  for (const t of walkTemplates(data.effects || [])) {
    if ((t.attribs || []).includes('Combat_Mod_Shift')) {
      shift = Math.max(shift, t.scale || 0);
    }
  }
  if (shift > 0) return shift;

  for (const ref of extractGrantedPowers(data)) {
    if (ref.split('.').pop().toLowerCase() !== 'level_shift') continue;
    const silent = resolveSilent(silentRefToFilename(ref));
    if (!silent) {
      throw new Error(`${sourceName}: grants ${ref} but the silent file is missing — refusing to guess the level shift`);
    }
    let silentShift = 0;
    for (const t of walkTemplates(silent.effects || [])) {
      if ((t.attribs || []).includes('Combat_Mod_Shift')) {
        silentShift = Math.max(silentShift, t.scale || 0);
      }
    }
    if (silentShift === 0) {
      throw new Error(`${sourceName}: grants ${ref} but its silent file carries no Combat_Mod_Shift template — refusing to guess the level shift`);
    }
    shift = Math.max(shift, silentShift);
  }
  return shift;
}

/** True if a template represents a Grant_Power, across both Parse7 (HC) and
 *  Parse6 (Rebirth) export shapes. HC tags `attribs: ['Grant_Power']`;
 *  Parse6 currently decodes the same attrib as `Null` but still populates
 *  `params.type = 'Power'` with the power_names. Skip Revoke_Power explicitly. */
function isGrantPowerTemplate(t) {
  const attribs = t.attribs || [];
  if (attribs.includes('Revoke_Power')) return false;
  if (attribs.includes('Grant_Power')) return true;
  return !!(t.params && t.params.type === 'Power' && Array.isArray(t.params.power_names) && t.params.power_names.length);
}

/** Extract all "Incarnate.*Silent.*" references from a power's Grant_Power templates */
function extractGrantedPowers(data) {
  const grants = [];
  for (const eff of data.effects || []) {
    for (const t of eff.templates || []) {
      if (!isGrantPowerTemplate(t)) continue;
      // power_names can be at top level or nested inside params
      const pnames = [
        ...(t.power_names || []),
        ...((t.params && t.params.power_names) || []),
      ];
      for (const pn of pnames) {
        if (pn.toLowerCase().includes('silent')) {
          grants.push(pn);
        }
      }
    }
  }
  return [...new Set(grants)];
}

/** Convert "Incarnate.Alpha_Silent.Damage_Very_Rare" → "damage_very_rare" */
function silentRefToFilename(ref) {
  const parts = ref.split('.');
  const name = parts[parts.length - 1];
  return name.toLowerCase();
}

/** Parse7 explicit PvP mode (Parse6 may synthesize this from requires). */
function isExplicitPvpOnlyGroup(effectGroup) {
  return (effectGroup && effectGroup.is_pvp) === 'PVP_ONLY';
}

/**
 * The BARE trailing `enttype target> player eq` — the leaguemate-buff shape.
 *
 * Deliberately narrower than the group's structural PvP verdict
 * (`_pv-scope.cjs`): this identifies one authored spelling so the beneficial
 * leaguemate rows below can be kept, and widening it to every PvP-only group
 * would grant that exception to conjunctions that have not earned it.
 */
function isBareLeaguematePlayerRequires(req) {
  return /enttype\s+target>\s+player\s+eq\s*$/i.test(req || '');
}
function isPvpMapOnlyRequires(req) {
  return /\bisPVPMap\?(?!\s+!)/i.test(req || '');
}

/** Resistibility bit from template flags (absence of IgnoreResistance => resistible). */
function isTemplateResistible(t) {
  return !((t.flags || []).includes('IgnoreResistance'));
}

/** Normalize attrib names from raw data to our internal keys */
const ATTRIB_MAP = {
  // Damage types → resistance keys
  'Smashing_Dmg': 'resSmashing', 'Lethal_Dmg': 'resLethal',
  'Fire_Dmg': 'resFire', 'Cold_Dmg': 'resCold',
  'Energy_Dmg': 'resEnergy', 'Negative_Energy_Dmg': 'resNegative',
  'Psionic_Dmg': 'resPsionic', 'Toxic_Dmg': 'resToxic',
  // Defense position/type keys (aspect=Current)
  'Melee': 'defMelee', 'Ranged': 'defRanged', 'Area': 'defAoE',
  'Smashing': 'defSmashing', 'Lethal': 'defLethal',
  'Fire': 'defFire', 'Cold': 'defCold',
  'Energy': 'defEnergy', 'Negative_Energy': 'defNegative',
  'Psionic': 'defPsionic', 'Toxic': 'defToxic',
  // Mez types (protection, aspect=Current, negative scale)
  'Held': 'protHold', 'Stunned': 'protStun', 'Immobilized': 'protImmobilize',
  'Sleep': 'protSleep', 'Confused': 'protConfuse', 'Afraid': 'protFear',
  'Terrorized': 'protFear', 'Knocked': 'protKnockback',
  // Buffs
  'Regeneration': 'regeneration',
  'Recovery': 'recovery',
  'Endurance': 'endurance',
  'RechargeTime': 'recharge',
  'Taunt': 'taunt',
  'Level_Shift': 'levelShift',
};

// Thunderspy generic-attrib map (aspect=='' hybrid buffs).
//
// Thunderspy's incarnate parser collapses each multi-attrib buff to a single
// generic *front-category* token and DROPS the AttribMod aspect (all-zero bytes
// → aspect=''), where HC (Parse7) and Rebirth (Parse6) both preserve the real
// per-attrib rows with their aspect (see [[thunderspy-attrib-index-array]] §7).
// So the aspect branches above never fire and every tspy hybrid renders empty.
//
// This maps the three front-categories whose meaning is UNAMBIGUOUS — verified
// three independent ways for the Support Hybrid (the only slot the collapse
// affects): the parallel HC and Rebirth files parse these exact effects as
//   `*_Dmg`@Strength (damage), `Melee`/`Area`/…@Current (defense), `Accuracy`@Strength,
// at IDENTICAL scales (Core Genome 0.02 → Embodiment 0.06 player-eq), and the
// in-game help reads "+Damage, +Accuracy, +Defense(All), +Special".
//   `Defense` → `defenseAll` (the calc expands it to all 11 def types; at the top
//              tier this reproduces HC's explicit 11-key list exactly).
// Two tokens are DELIBERATELY excluded — they are the help's "+Special", i.e. the
// heal-strength + mez-strength rows (`Heal_Dmg`/`Held`/`Confused`/… and `Stunned`,
// all @Strength on HC/Rebirth) that BOTH sister servers drop (team buff-strength,
// no self stat): `Ones` (the generic catch-all covering heal + non-stun mez) and
// `Stunned`. The `Ones`@scale=1.0/dur=0 grant marker is also dropped (it maps to
// nothing here and carries no `power_names` for the passive-boost linkage).
const TSPY_GENERIC_HYBRID_MAP = {
  'Damage': 'damage',
  'Accuracy': 'accuracy',
  'Defense': 'defenseAll',
};

/**
 * Thunderspy Parse6 omits Grant_Power linkage for Hybrid passives (the main
 * power carries only an `Ones` marker with no power_names). Recover the boost
 * linkage from the parallel Homecoming hybrid power of the same id — the same
 * reference-recovery pattern as readAlphaReference — and resolve it
 * against tspy's OWN silent-file scales. Returns [] when no HC parallel exists.
 */
// Homecoming incarnate export root, used by every HC-reference recovery
// (HC keeps the legacy flat layout when no namespaced copy exists).
const HC_REFERENCE_BASE = fs.existsSync(path.join(RAW_DATA_BASE, 'homecoming', 'incarnate'))
  ? path.join(RAW_DATA_BASE, 'homecoming', 'incarnate')
  : path.join(RAW_DATA_BASE, 'incarnate');

const HC_HYBRID_REF_DIR = path.join(HC_REFERENCE_BASE, 'hybrid');

function inferHybridPassiveFromReference(powerId) {
  if (datasetId === 'homecoming') return [];
  const ref = readJson(path.join(HC_HYBRID_REF_DIR, `${powerId}.json`));
  if (!ref) return [];
  const refs = [];
  for (const eff of ref.effects || []) {
    for (const t of eff.templates || []) {
      if (!isGrantPowerTemplate(t)) continue;
      const pnames = [
        ...(t.power_names || []),
        ...((t.params && t.params.power_names) || []),
      ];
      for (const pn of pnames) {
        if (pn.toLowerCase().includes('boost')) refs.push(pn);
      }
    }
  }
  return [...new Set(refs)];
}

/**
 * Thunderspy Parse6 omits Grant_Power linkage for Support Hybrid passives
 * (the main power carries only an `Ones` marker with no power_names), so infer
 * the corresponding Hybrid_Silent support_boost file by tier from the power id.
 */
function inferTspySupportBoostSilent(powerId) {
  if (!powerId.startsWith('support_genome')) return null;
  if (powerId === 'support_genome') return 'support_boost_common';
  if (powerId === 'support_genome_2' || powerId === 'support_genome_3') return 'support_boost_uncommon';
  if (['support_genome_4', 'support_genome_5', 'support_genome_6', 'support_genome_7'].includes(powerId)) return 'support_boost_rare';
  if (powerId === 'support_genome_8' || powerId === 'support_genome_9') return 'support_boost_very_rare';
  return null;
}

// The six control (mez) attribs, used to discriminate Destiny effects:
//   - aspect=Current, negative scale → mez PROTECTION for the caster (Clarion).
//   - aspect=Resistance             → mez/status duration RESISTANCE (Clarion),
//     which is NOT generic debuff resistance (that mislabel showed Clarion as a
//     bogus "300% Debuff Resistance"). HC packs all six into one multi-attrib
//     template; Rebirth/Parse6 splits them into one template each.
const MEZ_ATTRIBS = new Set([
  'Held', 'Stunned', 'Immobilized', 'Sleep', 'Confused', 'Afraid', 'Terrorized',
]);
// Knockback/Knockup protection attribs (aspect=Current, negative). `Knocked`
// lives in ATTRIB_MAP → protKnockback and is routed to kbProtection below;
// `Knockup`/`Knockback` are the raw attribs the bin actually uses (Clarion).
const KB_ATTRIBS = new Set(['Knockback', 'Knockup', 'Knocked']);
// Movement attribs Incandescence Radial grants (one multi-attrib template);
// surfaced as a single run-speed buff rather than dropped.
const MOVEMENT_ATTRIBS = new Set(['RunningSpeed', 'FlyingSpeed', 'JumpHeight', 'JumpSpeed']);

/** Map alpha_silent filename bases to enhancement aspect keys */
const ALPHA_FILENAME_MAP = {
  'damage': 'damage',
  'damage_plus': 'damage',
  'accuracy': 'accuracy',
  'accuracy_plus': 'accuracy',
  'recharge': 'recharge',
  'recharge_plus': 'recharge',
  'endurance_reduction': 'enduranceReduction',
  'endurance_reduction_plus': 'enduranceReduction',
  'range': 'range',
  'heal': 'heal',
  'heal_plus': 'heal',
  'defense_buff': 'defense',
  'res_damage': 'resistance',
  'res_damage_plus': 'resistance',
  'tohit_buff': 'toHitBuff',
  'tohit_debuff': 'toHitDebuff',
  'defense_debuff': 'defenseDebuff',
  'hold': 'hold',
  'hold_plus': 'hold',
  'stun': 'stun',
  'immobilize': 'immobilize',
  'sleep': 'sleep',
  'fear': 'fear',
  'confuse': 'confuse',
  'knockback': 'knockback',
  'snare': 'slow',
  'taunt': 'taunt',
  'run': 'runSpeed',
  'fly': 'flySpeed',
  'jump': 'jumpSpeed',
  // `intangible_*` is an HC slot REUSE, not an Intangible boost: the file kept
  // the old name while its content became Absorb (display_name "Absorb Very
  // Rare", one `Absorb`@Strength template pair, and the granting alphas' help
  // reads "…and Absorb by 33%"). Verified identical on all three datasets.
  // Reading the filename literally emitted an `intangible` key that no
  // AlphaEffects field and no aspect map knew, so Cardiac Radial Paragon /
  // Resilient Radial Paragon / Resilient Total Radial Revamp silently lost
  // their whole Absorb boost. The attribs are the truth; ALPHA_ASPECT_ATTRIBS
  // below is the tripwire for the next rename.
  'intangible': 'absorb',
  'recovery': 'enduranceModification',
  'recovery_plus': 'enduranceModification',
};

/**
 * The attrib an alpha aspect key is expected to boost — the tripwire for the
 * slot-reuse class above. Alpha aspect keys come from the silent FILENAME (it
 * is the only thing that separates `damage` from `res_damage`, or `tohit_buff`
 * from `tohit_debuff` — both pairs share an attrib), so a repurposed slot
 * retargets or drops a boost with nothing to notice it. The template attribs
 * cannot lie, so cross-check them and say so when the two stop agreeing.
 *
 * One entry per aspect, listing every dataset spelling of the FIRST attrib
 * (HC/Rebirth/Thunderspy diverge on which sibling leads the multi-attrib
 * templates). Aspects whose silent file collapses to a bare `Ones` template
 * carry no attrib to check and are skipped.
 */
const ALPHA_ASPECT_ATTRIBS = {
  damage: ['Smashing_Dmg'],
  accuracy: ['Accuracy'],
  recharge: ['RechargeTime'],
  enduranceReduction: ['EnduranceDiscount'],
  enduranceModification: ['Endurance', 'Recovery'],
  range: ['Range'],
  heal: ['Heal_Dmg'],
  defense: ['Ranged', 'Base_Defense'],
  resistance: ['Smashing_Dmg'],
  toHitBuff: ['ToHit'],
  toHitDebuff: ['ToHit'],
  defenseDebuff: ['Ranged', 'Base_Defense'],
  hold: ['Held'],
  stun: ['Stunned'],
  immobilize: ['Immobilized'],
  sleep: ['Sleep'],
  fear: ['Afraid', 'Terrorized'],
  confuse: ['Confused'],
  knockback: ['Knockup', 'Knockback'],
  slow: ['RunningSpeed'],
  taunt: ['Taunt'],
  runSpeed: ['RunningSpeed'],
  flySpeed: ['FlyingSpeed'],
  jumpSpeed: ['JumpingSpeed'],
  absorb: ['Absorb'],
};

function round(n, decimals = 6) {
  return Math.round(n * Math.pow(10, decimals)) / Math.pow(10, decimals);
}

// Thunderspy Parse6 omits the Grant_Power linkage on Alpha main powers (each
// carries only a bare `Ones` marker with no power_names), exactly as it does for
// Support Hybrid passives (see inferTspySupportBoostSilent). Every Alpha ability
// is a standard CoH incarnate (Agility/Cardiac/…) shared with Homecoming, and
// every silent file HC references exists in the tspy export, so recover the
// linkage from the parallel HC alpha power of the same id and resolve it against
// tspy's OWN silent-file scales. The `Combat_Mod_Shift` template rides the same
// collapsed main (tspy's display_help still names the shift), so the reference
// power is also the level-shift source. Returns null (current behavior — the
// slot stays empty and a WARN is logged) if the HC reference is unavailable.
const HC_ALPHA_REF_DIR = path.join(HC_REFERENCE_BASE, 'alpha');
const HC_DESTINY_REF_DIR = path.join(HC_REFERENCE_BASE, 'destiny');
const HC_DESTINY_SILENT_REF_DIR = path.join(HC_REFERENCE_BASE, 'destiny_silent');

function readAlphaReference(powerId) {
  if (datasetId === 'homecoming') return null;
  return readJson(path.join(HC_ALPHA_REF_DIR, `${powerId}.json`));
}

// ============================================
// ALPHA EXTRACTION
// ============================================

function extractAlpha() {
  console.log('\n=== ALPHA ===');
  const mainDir = path.join(RAW_BASE, 'alpha');
  const silentDir = path.join(RAW_BASE, 'alpha_silent');
  const results = {};

  // Pre-load all silent files
  const silentCache = {};
  for (const f of readDir(silentDir)) {
    const data = readJson(path.join(silentDir, f));
    if (data) silentCache[f.replace('.json', '')] = data;
  }

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;

    const powerId = f.replace('.json', '');
    const displayName = data.display_name || powerId;
    let grantedRefs = extractGrantedPowers(data);
    let levelShiftSource = data;
    if (grantedRefs.length === 0) {
      // Parse6 (thunderspy) omits the silent-power linkage; recover it — and
      // the Combat_Mod_Shift template, which rides the same collapsed main —
      // from the parallel Homecoming alpha power (see readAlphaReference).
      const reference = readAlphaReference(powerId);
      if (reference) {
        grantedRefs = extractGrantedPowers(reference);
        levelShiftSource = reference;
      }
    }

    // T3+ alpha mains carry their shift as a Combat_Mod_Shift template; the
    // granted-silent path in extractLevelShift never fires here (alpha grants
    // are the enhancement boosts, none named Level_Shift).
    const levelShift = extractLevelShift(
      levelShiftSource,
      (name) => silentCache[name] || null,
      `alpha/${powerId}`,
    );

    // Resolve silent power references to get enhancement bonuses
    const enhancements = {};
    // Per-aspect portion of `enhancements` that bypasses ED (see the
    // BoostIgnoreDiminishing comment below). Always <= the aspect's total.
    const edBypass = {};
    for (const ref of grantedRefs) {
      const silentName = silentRefToFilename(ref);
      const silentData = silentCache[silentName];
      if (!silentData) {
        console.warn(`  WARN: Silent power not found: ${silentName} (from ${powerId})`);
        continue;
      }

      // Determine aspect key from filename (strip tier suffix)
      const fileBase = silentName
        .replace(/_common$/, '').replace(/_uncommon$/, '').replace(/_very_rare$/, '')
        .replace(/_rare$/, '').replace(/_half$/, '').replace(/_plus_very_rare$/, '')
        .replace(/_plus$/, '');
      const aspectKey = ALPHA_FILENAME_MAP[fileBase];
      if (!aspectKey) {
        console.warn(`  WARN: Unknown alpha silent file base: ${fileBase} (from ${silentName})`);
        continue;
      }

      // Sum all templates for this aspect (regular + ED-bypass portions).
      // HC/Rebirth silent files carry 2 templates per attrib — regular +
      // BoostIgnoreDiminishing — on the SAME attrib, and list sibling damage/
      // defense types at an identical scale, so we take the first unique attrib.
      // Thunderspy (Parse6) instead splits the ED-bypass onto a generic `Ones`
      // template (aspect=''), so fold every `Ones` template into the total too
      // (a no-op for HC/Rebirth, which have zero `Ones` templates here).
      //
      // The ED-bypass portion is authored per SILENT FILE, not per tier: the
      // BoostIgnoreDiminishing-flagged template (HC/Rebirth) or the `Ones`
      // template (Thunderspy) IS the portion added after Enhancement
      // Diversification, and secondary aspects of a boost can carry a
      // different split than the headline aspect (e.g. `_half` files are 1/2
      // even on a Very Rare boost). Summed separately so the calc reads the
      // split from data instead of a per-tier ratio.
      let totalScale = 0;
      let bypassScale = 0;
      let firstAttrib = null;
      for (const eff of silentData.effects || []) {
        // Alpha silent files are never PvP-gated (all groups export as EITHER);
        // skip any PVP_ONLY group defensively so a future twin can't inflate.
        if (eff.is_pvp === 'PVP_ONLY') continue;
        for (const t of eff.templates || []) {
          const attrib = (t.attribs || [])[0];
          if (!attrib || attrib === 'Set_Mode') continue;
          if (attrib === 'Ones') {
            totalScale += (t.scale || 0);
            bypassScale += (t.scale || 0);
            continue;
          }
          // Only process the first unique attrib (others are duplicates for different damage types)
          if (firstAttrib === null) firstAttrib = attrib;
          if (attrib !== firstAttrib) continue;
          totalScale += (t.scale || 0);
          if ((t.flags || []).includes('BoostIgnoreDiminishing')) {
            bypassScale += (t.scale || 0);
          }
        }
      }
      // Filename says one aspect, the templates say another → the silent slot
      // was repurposed (see ALPHA_ASPECT_ATTRIBS). Loud, not fatal: HC has done
      // this once already and the export must still produce a dataset.
      const expectedAttribs = ALPHA_ASPECT_ATTRIBS[aspectKey];
      if (expectedAttribs && firstAttrib && !expectedAttribs.includes(firstAttrib)) {
        console.warn(
          `  WARN: ${silentName} maps to aspect '${aspectKey}' but boosts '${firstAttrib}' ` +
            `(expected ${expectedAttribs.join('/')}) — alpha_silent slot repurposed?`,
        );
      }

      if (totalScale > 0) {
        if (!enhancements[aspectKey]) enhancements[aspectKey] = 0;
        enhancements[aspectKey] = round(enhancements[aspectKey] + totalScale);
      }
      if (bypassScale > 0) {
        if (!edBypass[aspectKey]) edBypass[aspectKey] = 0;
        edBypass[aspectKey] = round(edBypass[aspectKey] + bypassScale);
      }
    }

    results[powerId] = {
      displayName,
      levelShift,
      enhancements,
      edBypass,
    };

    const enhStr = Object.entries(enhancements)
      .map(([k, v]) => `${k}:${round(v * 100, 1)}%`)
      .join(', ');
    console.log(`  ${displayName}: lvlShift=${levelShift} enh=[${enhStr}]`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// DESTINY EXTRACTION
// ============================================

function extractDestiny() {
  console.log('\n=== DESTINY ===');
  const mainDir = path.join(RAW_BASE, 'destiny');
  const silentDir = path.join(RAW_BASE, 'destiny_silent');
  const results = {};

  // Pre-load silent files
  const silentCache = {};
  for (const f of readDir(silentDir)) {
    const data = readJson(path.join(silentDir, f));
    if (data) silentCache[f.replace('.json', '')] = data;
  }

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;

    const powerId = f.replace('.json', '');
    const displayName = data.display_name || powerId;

    // T3+ destinies Grant_Power a Destiny_Silent.Level_Shift whose template
    // carries the shift. Thunderspy mains omit Grant_Power linkage entirely
    // (bare front-collapsed shells), so — same HC-reference recovery as Alpha —
    // read the shift from the parallel Homecoming destiny power, resolved
    // against HC's own silent files (tspy's level_shift silent is itself a
    // collapsed `Ones` shell its own export can't decide from).
    let levelShift = extractLevelShift(
      data,
      (name) => silentCache[name] || null,
      `destiny/${powerId}`,
    );
    if (levelShift === 0 && datasetId !== 'homecoming' && extractGrantedPowers(data).length === 0) {
      const reference = readJson(path.join(HC_DESTINY_REF_DIR, `${powerId}.json`));
      if (reference) {
        levelShift = extractLevelShift(
          reference,
          (name) => readJson(path.join(HC_DESTINY_SILENT_REF_DIR, `${name}.json`)),
          `destiny/${powerId} (HC reference)`,
        );
      }
    }

    // Extract diminishing buff effects from main power
    // Destiny powers have multiple copies of the same effect at different durations
    // We want the peak (shortest duration) values
    const effects = {};
    // Rebirth's heal (Heal_Dmg/Absolute) is an instantaneous, table-scaled
    // click heal — it has no dashboard total and doesn't decay, so it is kept
    // aside (scale + table) and resolved to actual HP at display time via the
    // build's archetype (like Judgement damage), rather than folded into the
    // diminishing timeline.
    let healScale = 0;
    let healTable = '';
    for (const eff of data.effects || []) {
      const req = gateText(eff.requires_expression);
      const pvpByMode = isExplicitPvpOnlyGroup(eff);
      const pvpByReq = isPvpMapOnlyRequires(req) || isPvpOnlyByRequires(eff);
      for (const t of eff.templates || []) {
        const attrib = (t.attribs || [])[0];
        if (!attrib || attrib === 'Grant_Power' || attrib === 'Revoke_Power' || attrib === 'Set_Mode') continue;

        const aspect = t.aspect || '';
        const scale = t.scale || 0;
        const duration = t.duration || '';
        if (scale === 0) continue;

        // Targeted PvP awareness: drop explicit/synthetically-PvP rows by
        // default, with the same beneficial `player eq` leaguemate exception
        // used in Hybrid (Parse6 can synthesize those rows as PVP_ONLY).
        const isLeaguematePlayer = scale > 0 && isBareLeaguematePlayerRequires(req);
        if ((pvpByMode || pvpByReq) && !isLeaguematePlayer) continue;

        const resistible = isTemplateResistible(t);

        // Parse duration to seconds
        let durSec = 0;
        const durMatch = duration.match(/([\d.]+)\s*seconds?/i);
        if (durMatch) durSec = parseFloat(durMatch[1]);

        // Instantaneous click heal (Rebirth). Negative Absolute Heal_Dmg = a
        // heal; keep the strongest one (all tiers of a power share one heal).
        if (aspect === 'Absolute' && attrib === 'Heal_Dmg') {
          if (Math.abs(scale) > Math.abs(healScale)) {
            healScale = Math.abs(scale);
            healTable = (t.table && typeof t.table === 'object') ? (t.table.column_name || '') : (t.table || '');
          }
          continue;
        }

        // Determine the stat key
        let statKey = null;
        if (aspect === 'Resistance') {
          // `aspect: Resistance` reuses stat attribs for several distinct game
          // concepts — discriminate by ATTRIB, never a bare `_Dmg` suffix
          // (GAME-DATA §3):
          //   - `Heal_Dmg` → Res(Heal) = "Healing Received" (Incandescence),
          //     inverted below to a positive buff. MUST precede the `_Dmg`
          //     test (flattening it into resistanceAll was the "-res(all)" bug).
          //   - a true damage type (Smashing_Dmg …) → `resistanceAll`.
          //   - a mez attrib (Held, Confused …) → `statusResistance` (mez
          //     DURATION resistance, Clarion). Bucketing this into
          //     debuffResistance is what showed Clarion as a bogus 300% Debuff
          //     Resistance — same distinct-stat-collapsed-into-broader-bucket
          //     class as the KB/KU and Incandescence bugs.
          //   - everything else (defense positions, Repel, Taunt/Placate,
          //     ToHit, Recharge, movement, …) → `debuffResistance` (Ageless
          //     Radial grants 50% Debuff Resistance).
          if (attrib === 'Heal_Dmg') statKey = 'healReceived';
          else if (attrib.endsWith('_Dmg')) statKey = 'resistanceAll';
          else if (MEZ_ATTRIBS.has(attrib)) statKey = 'statusResistance';
          else statKey = 'debuffResistance';
        } else if (aspect === 'Maximum' && attrib === 'HitPoints') {
          // Max HP buff (Rebirth Core). The bin stores it negated like a heal;
          // invert to positive. Game convention: displayed % = scale × 10, and
          // the Destiny totals consumer multiplies the stored fraction by 100,
          // so store scale × 0.1 (scale 2 → 0.2 → +20% Max HP).
          statKey = 'maxHP';
        } else if (aspect === 'Current' && ATTRIB_MAP[attrib]) {
          const mapped = ATTRIB_MAP[attrib];
          if (mapped.startsWith('def')) {
            statKey = 'defenseAll';
          } else if (mapped.startsWith('prot')) {
            // Mez PROTECTION for the caster (Clarion) — previously skipped with
            // an incorrect "targets enemies" comment, which dropped Clarion's
            // entire identity. Knockback protection is its own total.
            statKey = (mapped === 'protKnockback') ? 'kbProtection' : 'mezProtection';
          } else {
            statKey = mapped;
          }
        } else if (aspect === 'Current' && KB_ATTRIBS.has(attrib)) {
          statKey = 'kbProtection'; // Knockup/Knockback (raw attribs, not in ATTRIB_MAP)
        } else if (aspect === 'Current' && MOVEMENT_ATTRIBS.has(attrib)) {
          statKey = 'runSpeed'; // Incandescence Radial travel buff
        } else if (aspect === 'Strength') {
          // Enhancement-style buffs (recharge, recovery, regen). Anything else
          // at aspect=Strength is a "+buff-strength" amplifier (Rebirth's
          // Clarion Radial buffs def/res/mez/heal STRENGTH across ~20 attribs)
          // with no player total — DROP it rather than leak lowercased junk
          // keys (`melee`, `tohit`, `absorb`, …) via the old catch-all.
          if (attrib === 'RechargeTime') statKey = 'recharge';
          else if (attrib === 'Recovery') statKey = 'recovery';
          else if (attrib === 'Regeneration') statKey = 'regeneration';
          else statKey = null;
        } else if (aspect === '' && datasetId === 'thunderspy') {
          // Parse6/Thunderspy omits aspect/type metadata for many Destiny rows.
          // Recover only the known caster-facing stats from attrib identity.
          if (attrib === 'RechargeTime') statKey = 'recharge';
          else if (attrib === 'Recovery') statKey = 'recovery';
          else if (attrib === 'Regeneration') statKey = 'regeneration';
          else if (attrib === 'Endurance') statKey = 'endurance';
          else if (MOVEMENT_ATTRIBS.has(attrib)) statKey = 'runSpeed';
          else statKey = null;
        } else if (attrib === 'Endurance' && aspect === 'Current') {
          statKey = 'endurance'; // Endurance refill
        }

        if (!statKey) {
          // console.warn(`  WARN: Unmapped destiny effect: ${attrib} aspect=${aspect} in ${powerId}`);
          continue;
        }

        // Track by stat+duration — we'll collapse and pick the peak later.
        //   - healReceived: Res(Heal) is stored negative → invert to positive.
        //   - maxHP: scale × 0.1 fraction (see above).
        //   - mez/KB protection: magnitude = |scale| (positive).
        //   - everything else: raw scale.
        let value;
        if (statKey === 'healReceived') value = -scale;
        else if (statKey === 'maxHP') value = Math.abs(scale) * 0.1;
        else if (statKey === 'mezProtection' || statKey === 'kbProtection') value = Math.abs(scale);
        else value = scale;
        const key = `${statKey}`;
        if (!effects[key]) effects[key] = [];
        effects[key].push({ value: round(value), duration: durSec, resistible });
      }
    }

    // For each stat, collapse duplicate same-duration tiers, then extract the
    // peak value and diminishing timeline.
    const peakEffects = {};
    const timeline = {};
    for (const [stat, entries] of Object.entries(effects)) {
      // A uniform multi-type buff (all 11 defense positions, all 6 mez types,
      // Ageless's ~15 debuff-resistance attribs) arrives as many identical
      // templates — one multi-attrib row in HC, one row per attrib in Parse6.
      // Summing them would N× the magnitude (Parse6 Clarion mez would read 6×).
      // Collapse each duration to its single strongest component so the timeline
      // sums real decay tiers, not duplicate attribs.
      // Resistible-aware: keep resistible/unresistable twins distinct.
      const byDurRes = new Map();
      for (const e of entries) {
        const durKey = `${e.duration}|${e.resistible ? 'R' : 'U'}`;
        const cur = byDurRes.get(durKey);
        if (cur === undefined || Math.abs(e.value) > Math.abs(cur)) {
          byDurRes.set(durKey, { duration: e.duration, value: e.value, resistible: e.resistible });
        }
      }
      const collapsed = Array.from(byDurRes.values())
        .sort((a, b) => a.duration - b.duration);
      // Peak = highest absolute value
      const peak = collapsed.reduce((best, e) => Math.abs(e.value) > Math.abs(best.value) ? e : best, collapsed[0]);
      peakEffects[stat] = round(peak.value);
      timeline[stat] = collapsed.map(e => ({ value: round(e.value), duration: e.duration }));
    }

    // Boost categories the power accepts — the game's own statement of which
    // Alpha enhancement aspects can enhance this Destiny buff (Res_Damage,
    // Buff_Defense, Heal, Recovery). Drives Alpha→Destiny enhancement in the
    // calc. Drop the universal Incarnate_Destiny slot marker (never mapped).
    const boostsAllowed = (data.boosts_allowed || []).filter((b) => b !== 'Incarnate_Destiny');

    results[powerId] = {
      displayName,
      levelShift,
      peakEffects,
      timeline,
      boostsAllowed,
      healScale,   // instantaneous click heal (0 = none); resolved to HP at display
      healTable,
    };

    const effectStr = Object.entries(peakEffects)
      .map(([k, v]) => `${k}:${round(v * 100, 1)}%`)
      .join(', ');
    const healStr = healScale ? ` heal=${healScale}×${healTable}` : '';
    console.log(`  ${displayName}: lvlShift=${levelShift} peak=[${effectStr}]${healStr}`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// HYBRID EXTRACTION
// ============================================

function extractHybrid() {
  console.log('\n=== HYBRID ===');
  const mainDir = path.join(RAW_BASE, 'hybrid');
  const silentDir = path.join(RAW_BASE, 'hybrid_silent');
  const results = {};

  // Pre-load all silent files
  const silentCache = {};
  for (const f of readDir(silentDir)) {
    const data = readJson(path.join(silentDir, f));
    if (data) silentCache[f.replace('.json', '')] = data;
  }

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;

    const powerId = f.replace('.json', '');
    const displayName = data.display_name || powerId;

    // Determine tree from filename
    let tree = 'unknown';
    if (powerId.startsWith('assault')) tree = 'assault';
    else if (powerId.startsWith('support')) tree = 'support';
    else if (powerId.startsWith('melee')) tree = 'melee';
    else if (powerId.startsWith('control')) tree = 'control';

    // The per-foe cap, read off the power rather than its tooltip. `max_targets_hit`
    // counts every entity the sphere can land on, and the caster is one of them
    // whenever `Self` is affected, so the FOE ceiling is one less: Melee Core
    // Embodiment's 10 is 9 enemies plus you. A hybrid that affects no `Foe` has no
    // per-foe layer to cap (Assault/Control are `Self` only; Support's 255 is the
    // uncapped leaguemate spread), which is why the discriminator is the target list
    // and not the number.
    //
    // Was a scrape of `display_help` for "maximum strength at N enemies" until
    // HYBRID-PT-1. Prose agrees with this read on all 9 melee tiers across all four
    // forks — that agreement is itself graded — but a
    // tooltip is not a field, and this cap now sets a slider's range.
    const affects = data.targets_affected || [];
    const maxTargets = affects.includes('Foe')
      ? Math.max(0, (data.max_targets_hit || 0) - (affects.includes('Self') ? 1 : 0))
      : 0;

    // Categorize effects by requires_expression
    const frontLoaded = {}; // Self-only (fixed baseline when toggle is on)
    const perTarget = {};   // Per-enemy stacking
    const grantedPassiveRefs = []; // Passive boost powers
    const grantedOtherRefs = []; // Other granted powers (procs, etc.)
    // Parse6 (Rebirth/Thunderspy) splits a uniform multi-attrib buff into one
    // group per attrib where HC packs them into a single template — both encode
    // ONE buff. Without dedup the split form SUMS: e.g. the eight `*_Dmg` groups
    // at +6% each collapse to the same `damage` stat and read a bogus +48%. Track
    // applied (bucket, statKey, scale) triples so an identical-scale sibling from
    // the split counts once, while a genuinely distinct buff (different scale)
    // still stacks. (HC is unaffected — its single template already deduped the
    // eight damage types to one `damage` via templateKeys.)
    const seenContribs = new Set();

    for (const eff of data.effects || []) {
      const req = gateText(eff.requires_expression);
      const pvpByMode = isExplicitPvpOnlyGroup(eff);
      const pvpByReq = isPvpMapOnlyRequires(req) || isPvpOnlyByRequires(eff);

      for (const t of eff.templates || []) {
        const attribs = t.attribs || [];
        const scale = t.scale || 0;
        const aspect = t.aspect || '';

        // Cross-format grant detection (Parse7 attribs vs Parse6 params).
        if (isGrantPowerTemplate(t)) {
          const pnames = [
            ...(t.power_names || []),
            ...((t.params && t.params.power_names) || []),
          ];
          for (const pn of pnames) {
            if (pn.toLowerCase().includes('boost')) {
              grantedPassiveRefs.push(pn);
            } else if (pn.toLowerCase().includes('silent')) {
              grantedOtherRefs.push(pn);
            }
          }
          continue;
        }

        if (scale === 0) continue;

        // Targeted PvP awareness: drop explicit PvP-only rows by default, but
        // keep the known leaguemate-buff shape (`enttype target> player eq` +
        // beneficial polarity) that Parse6 can synthesize as PVP_ONLY.
        const isLeaguematePlayer = scale > 0 && isBareLeaguematePlayerRequires(req);
        if ((pvpByMode || pvpByReq) && !isLeaguematePlayer) continue;

        // Collect the DISTINCT stat keys this template maps to. A uniform
        // multi-attrib buff — e.g. Support Hybrid's aspect=Strength row over all
        // eight `*_Dmg` types — is one buff and must count ONCE; summing per
        // attrib would read +48% damage for a +6% buff. Distinct positions
        // (Melee/Ranged/Area) map to distinct keys and all apply.
        const templateKeys = new Set();
        for (const attrib of attribs) {
          if (attrib === 'Revoke_Power' || attrib === 'Set_Mode') continue;
          let statKey = null;
          if (aspect === 'Resistance' && ATTRIB_MAP[attrib]) {
            statKey = ATTRIB_MAP[attrib]; // e.g., resSmashing
          } else if (aspect === 'Current') {
            if (ATTRIB_MAP[attrib]) statKey = ATTRIB_MAP[attrib];
          } else if (aspect === 'Strength') {
            // Enhancement-type buffs. Accept both 'Endurance' (legacy CoD2
            // name) and 'EnduranceDiscount' (current bin export name —
            // attrib index 92 vs Endurance's 22, they're distinct in the
            // game enum but Mids/CoD2 used the shorter name).
            if (attrib === 'Endurance' || attrib === 'EnduranceDiscount') statKey = 'enduranceDiscount';
            // Support Hybrid buffs the strength of your damage and accuracy —
            // previously dropped (only endurance was mapped at Strength), which
            // made the Support tree read nearly empty. The heal-strength and
            // mez-strength parts of the same row are team-oriented with no
            // standard self total, so they stay out.
            else if (attrib === 'Accuracy') statKey = 'accuracy';
            else if (attrib.endsWith('_Dmg') && attrib !== 'Heal_Dmg') statKey = 'damage';
          } else if (aspect === '' && TSPY_GENERIC_HYBRID_MAP[attrib]) {
            // Thunderspy dropped-aspect hybrid buff — map the unambiguous
            // generic front-category tokens (see TSPY_GENERIC_HYBRID_MAP). Only
            // fires for tspy: HC/Rebirth always carry a real aspect, and their
            // real attribs are never the bare `Damage`/`Defense` tokens.
            statKey = TSPY_GENERIC_HYBRID_MAP[attrib];
          } else if (aspect === '' && datasetId === 'thunderspy') {
            // Parse6/Thunderspy can also omit aspect on regular scalar rows
            // (e.g. Melee Genome +Regeneration); recover only known-safe stats.
            if (attrib === 'Regeneration') statKey = 'regeneration';
          }
          if (!statKey || statKey === 'taunt') continue; // taunt isn't a player stat
          templateKeys.add(statKey);
        }
        if (templateKeys.size === 0) continue;

        // Classify as front-loaded or per-target (per-effect requires, shared by
        // all attribs in the template). Two requires_expression dialects exist:
        //   - Legacy raw_data_homecoming: `Ne(target,source)` for per-target,
        //     `source>entref` for self.
        //   - bin-crawler export (RPN): `entref target> entref source> eq`
        //     for self (target equals source), same plus trailing ` !` for
        //     per-target (target NOT source).
        const isSelfRPN = /entref\s+target>\s+entref\s+source>\s+eq\s*$/.test(req);
        const isPerTargetRPN = /entref\s+target>\s+entref\s+source>\s+eq\s+!/.test(req);
        // Support Hybrid's *Core* line buffs "all leaguemates" (which includes the
        // caster) via an ENTITY-TYPE gate rather than an empty/self requires:
        //   `enttype target> player eq`  → the player-leaguemate value the CASTER
        //                                  receives (applies in PvE).
        //   `enttype target> critter eq` → the pet-doubled value ("This boost is
        //                                  doubled in strength for pets", per the
        //                                  in-game help); applies to pets, NOT the
        //                                  caster, so it stays out of caster totals.
        // Neither matched the self/per-target RPN forms above, so the whole Core
        // buff was routed nowhere (empty frontLoaded) while the Radial line — which
        // uses an empty requires — rendered. DSH8 fix: route the player-leaguemate
        // value to frontLoaded (the caster gets it, like an empty-req self buff).
        //
        // Two cross-dataset traps make the parser's `is_pvp` flag the WRONG key:
        //   1. CASE — HC/Thunderspy write `enttype`, Rebirth writes `Enttype`; the
        //      match must be case-insensitive (`/i`).
        //   2. SYNTHESIZED is_pvp — Rebirth/Thunderspy are Parse6, which has no
        //      explicit is_pvp field, so the bin parser SYNTHESIZES it from the
        //      requires target-type. A BARE `player eq` — which is exactly what
        //      this Hybrid buff carries — is structurally PvP-only, so the
        //      synthesis marks it PVP_ONLY. (Since MAPGATE-1 that verdict is a
        //      structural read, not a token scan, so a negated or disjunct
        //      clause no longer lands here — but the bare form still does.)
        //      That synthesis is harmless for FOE effects
        //      (the `critter eq` sibling carries the effect to the critter foe in
        //      PvE) but WRONG for this ally-BUFF, whose caster is a *player*. HC's
        //      explicit is_pvp=EITHER for the same buff is the ground truth.
        // The reliable discriminator is POLARITY, not is_pvp: a `player eq` group on
        // a BENEFICIAL buff (scale > 0 — and extractHybrid only ever maps beneficial
        // stat keys) is the leaguemate value and applies in PvE; a foe-debuff's
        // `player eq` PvP twin is harmful (negative) and never reaches this route.
        const isPerTarget = isPerTargetRPN || req.includes('Ne(target');
        const isSelfOnly = isSelfRPN || req.includes('source>entref') || req === '' || isLeaguematePlayer;
        const resistibleKey = isTemplateResistible(t) ? 'R' : 'U';

        for (const statKey of templateKeys) {
          // Resistible-aware dedup: Parse6 per-attrib split still dedups cleanly,
          // but a true resistible/unresistable twin is kept distinct.
          const contribKey = `${isPerTarget ? 'pt' : 'fl'}|${statKey}|${scale}|${resistibleKey}`;
          if (isPerTarget) {
            if (seenContribs.has(contribKey)) continue;
            seenContribs.add(contribKey);
            if (!perTarget[statKey]) perTarget[statKey] = 0;
            perTarget[statKey] = round(perTarget[statKey] + Math.abs(scale));
          } else if (isSelfOnly) {
            if (seenContribs.has(contribKey)) continue;
            seenContribs.add(contribKey);
            if (!frontLoaded[statKey]) frontLoaded[statKey] = 0;
            // Mez protection uses negative scales for protection magnitude
            const val = statKey.startsWith('prot') ? Math.abs(scale) : scale;
            frontLoaded[statKey] = round(frontLoaded[statKey] + val);
          }
        }
      }
    }

    // Resolve passive boosts from silent files
    const passive = {};
    if (datasetId === 'thunderspy' && grantedPassiveRefs.length === 0) {
      // Recover the omitted Parse6 linkage from the HC parallel (all trees);
      // fall back to the tier-derived support map when no parallel exists.
      const inferredRefs = inferHybridPassiveFromReference(powerId);
      if (inferredRefs.length > 0) {
        grantedPassiveRefs.push(...inferredRefs);
      } else {
        const inferred = inferTspySupportBoostSilent(powerId);
        if (inferred) grantedPassiveRefs.push(inferred);
      }
    }
    for (const ref of grantedPassiveRefs) {
      const silentName = silentRefToFilename(ref);
      const silentData = silentCache[silentName];
      if (!silentData) {
        console.warn(`  WARN: Silent boost not found: ${silentName} (from ${powerId})`);
        continue;
      }

      for (const eff of silentData.effects || []) {
        for (const t of eff.templates || []) {
          const attrib = (t.attribs || [])[0];
          const scale = t.scale || 0;
          const aspect = t.aspect || '';
          if (!attrib || scale === 0) continue;

          let statKey = null;
          if (attrib === 'Smashing_Dmg' && aspect === 'Strength') {
            statKey = 'damage'; // Assault passive +Dmg
          } else if (attrib === 'Regeneration' && aspect === 'Current') {
            statKey = 'regeneration'; // Melee passive +Regen
          } else if (attrib === 'EnduranceDiscount' && aspect === 'Strength') {
            // Hybrid Support silent boosts (support_boost_{common,uncommon,rare,very_rare})
            // store the +X% endurance discount as attrib 'EnduranceDiscount'
            // (index 92), not 'Endurance' (index 22). The earlier name was wrong
            // and made the +10% T4 Support passive vanish from `global.enduranceDiscount`.
            statKey = 'enduranceDiscount';
          } else if (datasetId === 'thunderspy' && attrib === 'Ones') {
            // Parse6/Thunderspy collapses these passives to a generic `Ones`
            // token; the silent-file family names the stat and the recovered
            // aspect (TSPY-3 typing — the fronts used to be empty, which an
            // older aspect === '' gate here matched and silently stopped
            // matching, HYBRID-1) discriminates: support/assault carry
            // Strength, control carries Resistance. Values are tspy's own
            // (it rebalances — control res is 0.1..0.4 vs HC's 0.1..0.25).
            if (silentName.startsWith('support_boost_') && aspect === 'Strength') {
              statKey = 'enduranceDiscount';
            } else if (silentName.startsWith('assault_boost_') && aspect === 'Strength') {
              statKey = 'damage';
            } else if (silentName.startsWith('control_boost_') && aspect === 'Resistance') {
              statKey = 'statusResistance';
            }
          } else if (aspect === 'Resistance') {
            // Control passive Status Resistance
            const mezTypes = ['Confused', 'Terrorized', 'Held', 'Immobilized', 'Stunned', 'Sleep', 'Afraid'];
            if (mezTypes.includes(attrib)) {
              statKey = 'statusResistance';
            }
          }

          if (statKey) {
            if (!passive[statKey]) passive[statKey] = 0;
            passive[statKey] = round(Math.max(passive[statKey], Math.abs(scale)));
          }
        }
      }
    }

    results[powerId] = {
      displayName,
      tree,
      maxTargets,
      passive,
      frontLoaded,
      perTarget,
      grantedOtherRefs,
      duration: 120,
      recharge: 120,
    };

    const passiveStr = Object.entries(passive).map(([k, v]) => `${k}:${round(v * 100, 1)}%`).join(', ');
    const fixedStr = Object.entries(frontLoaded).map(([k, v]) => `${k}:${round(v * 100, 1)}%`).join(', ');
    const ptStr = Object.entries(perTarget).map(([k, v]) => `${k}:${round(v * 100, 1)}%`).join(', ');
    console.log(`  ${displayName} (${tree}): maxTargets=${maxTargets}`);
    if (passiveStr) console.log(`    passive: ${passiveStr}`);
    if (fixedStr) console.log(`    front-loaded: ${fixedStr}`);
    if (ptStr) console.log(`    per-target: ${ptStr}`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// INTERFACE EXTRACTION
// ============================================

/** Map interface silent attrib names to debuff type display strings */
const INTERFACE_DEBUFF_MAP = {
  'ToHit': '-ToHit', 'Accuracy': '-ToHit',
  'Regeneration': '-Regen', 'Recovery': '-Recovery',
  'Base_Defense': '-Defense',
  'Smashing_Dmg': '-Damage', // aspect=Strength → damage debuff
  'RunningSpeed': '-Speed', 'JumpHeight': '-Speed', 'FlyingSpeed': '-Speed',
  'RechargeTime': '-Recharge',
  'Endurance': '-Endurance',
  'HitPoints': '-MaxHP',
};

/** Map attrib names to DoT type display */
const INTERFACE_DOT_MAP = {
  'Fire_Dmg': 'Fire', 'Cold_Dmg': 'Cold', 'Energy_Dmg': 'Energy',
  'Negative_Energy_Dmg': 'Negative', 'Toxic_Dmg': 'Toxic',
  'Psionic_Dmg': 'Psionic', 'Smashing_Dmg': 'Smashing', 'Lethal_Dmg': 'Lethal',
};

function extractInterface() {
  console.log('\n=== INTERFACE ===');
  const mainDir = path.join(RAW_BASE, 'interface');
  const silentDir = path.join(RAW_BASE, 'interface_silent');
  const results = {};

  // Pre-load silent files (normalize names: "to_hit_debuff" from "To_Hit_Debuff")
  const silentCache = {};
  for (const f of readDir(silentDir)) {
    const data = readJson(path.join(silentDir, f));
    if (data) silentCache[f.replace('.json', '')] = data;
  }

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;

    const powerId = f.replace('.json', '');
    const displayName = data.display_name || powerId;

    // Interface powers grant silent proc powers and set Global_Chance_Mod for proc rates.
    // The grants and chances pair up in order: first grant → first chance, etc.
    const grantedRefs = extractGrantedPowers(data);
    const chances = [];
    for (const t of walkTemplates(data.effects)) {
      if ((t.attribs || []).includes('Global_Chance_Mod')) {
        chances.push(t.scale || 0);
      }
    }

    // Resolve each granted proc from silent files
    let debuffType = null;
    let debuffMagnitude = 0;
    let debuffDuration = 0;
    let dotType = null;
    let dotDamage = 0;
    let dotDuration = 0;
    let dotTableName = '';
    let procChance = chances[0] || 0;

    for (let i = 0; i < grantedRefs.length; i++) {
      const silentName = silentRefToFilename(grantedRefs[i]);
      const silentData = silentCache[silentName];
      if (!silentData) continue;
      const chance = chances[i] || 0;

      for (const t of walkTemplates(silentData.effects)) {
        const attrib = (t.attribs || [])[0];
        const scale = t.scale || 0;
        const aspect = t.aspect || '';
        const duration = t.duration || '';
        if (!attrib || scale === 0) continue;

        const durMatch = duration.match(/([\d.]+)\s*seconds?/i);
        const durSec = durMatch ? parseFloat(durMatch[1]) : 0;

        // Classify as debuff or DoT
        if (aspect === 'Absolute' && attrib in INTERFACE_DOT_MAP) {
          // DoT damage
          if (!dotType || Math.abs(scale) > Math.abs(dotDamage)) {
            dotType = INTERFACE_DOT_MAP[attrib];
            dotDamage = round(Math.abs(scale));
            dotDuration = durSec;
            dotTableName = requireTemplateTable(t, `interface_silent/${silentName}`);
          }
        } else if (attrib in INTERFACE_DEBUFF_MAP) {
          // Debuff effect
          if (!debuffType || Math.abs(scale) > Math.abs(debuffMagnitude)) {
            debuffType = INTERFACE_DEBUFF_MAP[attrib];
            // Aspect=Strength means it's a strength debuff (percentage)
            // Aspect=Current means direct stat modification
            debuffMagnitude = round(Math.abs(scale));
            debuffDuration = durSec;
          }
        }

        // Use the highest proc chance
        if (chance > procChance) procChance = chance;
      }
    }

    results[powerId] = {
      displayName,
      debuffType,
      debuffMagnitude,
      debuffDuration,
      dotType,
      dotDamage,
      dotDuration,
      dotTableName,
      procChance: round(procChance),
    };

    const parts = [];
    if (debuffType) parts.push(`${debuffType} ${round(debuffMagnitude * 100, 1)}%`);
    if (dotType) parts.push(`DoT(${dotType}) scale=${dotDamage}`);
    console.log(`  ${displayName}: ${parts.join(', ') || 'procs only'} chance=${round(procChance * 100)}%`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// JUDGEMENT EXTRACTION
// ============================================

/** Map raw damage attrib names to display names */
const DAMAGE_TYPE_MAP = {
  'Smashing_Dmg': 'Smashing', 'Lethal_Dmg': 'Lethal', 'Fire_Dmg': 'Fire',
  'Cold_Dmg': 'Cold', 'Energy_Dmg': 'Energy', 'Negative_Energy_Dmg': 'Negative Energy',
  'Psionic_Dmg': 'Psionic', 'Toxic_Dmg': 'Toxic',
};

function extractJudgement() {
  console.log('\n=== JUDGEMENT ===');
  const mainDir = path.join(RAW_BASE, 'judgement');
  const results = {};

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;

    const powerId = f.replace('.json', '');
    const displayName = data.display_name || powerId;
    const helpText = data.display_help || '';

    // Extract area/targeting info from top-level
    const effectArea = data.effect_area || 'Unknown';
    const range = data.range || 0;
    const radius = data.radius || 0;
    const arc = data.arc || 0;
    const maxTargets = data.max_targets_hit || 0;
    const activationTime = data.activation_time || 0;
    const rechargeTime = requireRechargeTime(data, `judgement/${f}`);

    // Extract damage effects
    let primaryDamageType = null;
    let damageScale = 0;
    let tableName = '';
    const secondaryEffects = [];
    const shortHelpDamageType = damageTypeFromShortHelp(data.display_short_help);

    for (const t of walkTemplates(data.effects)) {
      const attrib = (t.attribs || [])[0];
      const scale = t.scale || 0;
      const duration = t.duration || '';
      const aspect = t.aspect || '';

      const damageType = attrib in DAMAGE_TYPE_MAP
        ? DAMAGE_TYPE_MAP[attrib]
        : (attrib === 'Tempdamage' ? shortHelpDamageType : null);
      if (damageType && aspect === 'Absolute') {
        const durMatch = duration.match(/([\d.]+)\s*seconds?/i);
        const durSec = durMatch ? parseFloat(durMatch[1]) : 0;

        if (durSec === 0 && scale > damageScale) {
          // Direct damage (no duration = instant)
          primaryDamageType = damageType;
          damageScale = scale;
          tableName = requireTemplateTable(t, `judgement/${f}`);
        } else if (durSec > 0) {
          // DoT
          secondaryEffects.push(`DoT(${damageType}) ${round(scale, 2)} scale/${round(durSec, 1)}s`);
        }
      } else if (attrib === 'Held' || attrib === 'Stunned' || attrib === 'Immobilized' ||
                 attrib === 'Knocked' || attrib === 'Sleep') {
        // Mez Magnitude rides the `magnitude` field; `scale` is the DURATION
        // multiplier (scale × table = seconds). Using scale here mislabeled every
        // incarnate hold (Cryonic/Ion "Held Mag 12" for a real Mag 4) across all
        // three datasets. `scale > 0` still gates out zero-scale metadata rows.
        if (scale > 0) {
          secondaryEffects.push(`${attrib} Mag ${round(t.magnitude || scale, 1)}`);
        }
      } else if ((attrib === 'JumpHeight' || attrib === 'RunningSpeed') && scale < 0) {
        secondaryEffects.push('Slow');
      }
    }

    if (!tableName) {
      throw new Error(`judgement/${f}: no direct-damage template found — cannot derive the damage table`);
    }

    // Determine effect area type from help text for better display
    let areaType = effectArea;
    if (helpText.includes('Cone')) areaType = 'Cone';
    else if (helpText.includes('Chain')) areaType = 'Chain';
    else if (helpText.includes('PBAoE')) areaType = 'PBAoE';
    else if (helpText.includes('Targeted') && helpText.includes('AoE')) areaType = 'Targeted AoE';
    else if (helpText.includes('AoE')) areaType = 'AoE';

    results[powerId] = {
      displayName,
      damageType: primaryDamageType || 'Unknown',
      effectArea: areaType,
      range: round(range, 1),
      radius: round(radius, 1),
      arc: round(arc, 1),
      maxTargets,
      activationTime: round(activationTime, 2),
      rechargeTime: round(rechargeTime, 1),
      damageScale: round(damageScale, 2),
      tableName,
      secondaryEffects: [...new Set(secondaryEffects)],
    };

    console.log(`  ${displayName}: ${primaryDamageType} scale=${damageScale} ${areaType} r=${radius} arc=${arc}`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// LORE EXTRACTION
// ============================================

/** Map entity def names to display-friendly pet types */
function petTypeFromEntityDef(entityDef) {
  const lower = entityDef.toLowerCase();
  if (lower.includes('boss')) return 'Boss';
  if (lower.includes('support')) return 'Support';
  if (lower.includes('_lt')) return 'Lieutenant';
  return 'Pet';
}

/** Extract faction name from power ID (e.g., 'arachnos_core_superior_ally' → 'Arachnos') */
function factionFromPowerId(powerId) {
  const factionMap = {
    arachnos: 'Arachnos', banished: 'Banished Pantheon', carnival: 'Carnival of Shadows',
    cimeroran: 'Cimeroran', clockwork: 'Clockwork', demons: 'Demons',
    elementals: 'Elementals', idf: 'IDF', knives: 'Knives of Artemis',
    lights: 'Lights', longbow: 'Longbow', nemesis: 'Nemesis',
    phantoms: 'Phantoms', rikti: 'Rikti', rularuu: 'Rularuu',
    seers: 'Seers', talons: 'Talons of Vengeance', tsoo: 'Tsoo',
    vanguard: 'Vanguard', warworks: 'War Works', drones: 'Drones',
  };
  for (const [key, name] of Object.entries(factionMap)) {
    if (powerId.includes(key)) return name;
  }
  return powerId.split('_')[0].charAt(0).toUpperCase() + powerId.split('_')[0].slice(1);
}

function extractLore() {
  console.log('\n=== LORE ===');
  const mainDir = path.join(RAW_BASE, 'lore');
  const results = {};

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;

    const powerId = f.replace('.json', '');
    const displayName = data.display_name || powerId;
    const rechargeTime = requireRechargeTime(data, `lore/${f}`);

    // Extract pets from Create_Entity templates and check for level shift grants
    const pets = [];
    let duration = 300;
    let levelShift = 0;
    for (const eff of data.effects || []) {
      for (const t of eff.templates || []) {
        const attrib = (t.attribs || [])[0];
        if (attrib === 'Create_Entity') {
          const entityDef = (t.params || {}).entity_def || '';
          if (entityDef) {
            pets.push(petTypeFromEntityDef(entityDef));
          }
          // Extract duration
          const durMatch = (t.duration || '').match(/([\d.]+)\s*seconds?/i);
          if (durMatch) duration = parseFloat(durMatch[1]);
        }
        // Check for level shift grant (via Lore_Silent.Level_Shift).
        // Cross-format detection: Parse7 tags Grant_Power, Parse6 leaves
        // attribs as Null but populates params.power_names.
        if (isGrantPowerTemplate(t)) {
          const pnames = [
            ...(t.power_names || []),
            ...((t.params && t.params.power_names) || []),
          ];
          if (pnames.some(p => p.includes('Level_Shift'))) {
            levelShift = 1;
          }
        }
      }
    }

    const faction = factionFromPowerId(powerId);

    results[powerId] = {
      displayName,
      faction,
      pets: pets.length > 0 ? pets : ['Unknown'],
      duration,
      rechargeTime: round(rechargeTime, 1),
      levelShift,
    };

    console.log(`  ${displayName}: ${faction} pets=[${pets.join(', ')}] ${duration}s/${rechargeTime}s`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// GENESIS EXEMPLAR POWERS (below level 45)
// ============================================
// Each Genesis ability grants a usable power when exemplared below 45. These
// live as self-contained main files under exported_powers/<id>/incarnate/{data,
// fate,socket,verdict}/. We parse them into a typed `exemplarEffect` discriminated
// by tree. Standalone parsers (reusing the shared maps/helpers) keep the existing
// slot extractors byte-identical — no drift to Alpha/Destiny/etc.

/** "Incarnate.Verdict.Ion_Total_Core_Verdict" -> { tree:'verdict', file:'.../verdict/ion_total_core_verdict.json' } */
function genesisExemplarPath(ref) {
  const parts = (ref || '').split('.');
  const tree = (parts[1] || '').toLowerCase();
  const name = (parts[2] || '').toLowerCase();
  return { tree, file: path.join(RAW_BASE, tree, name + '.json') };
}

/** Verdict: an AoE damage attack (mirrors Judgement). */
function parseExemplarAttack(data) {
  const helpText = data.display_help || '';
  const sourceName = data.full_name || 'verdict exemplar';
  let damageType = null, damageScale = 0, tableName = '';
  const secondaryEffects = [];
  const shortHelpDamageType = damageTypeFromShortHelp(data.display_short_help);
  for (const t of walkTemplates(data.effects)) {
    const attrib = (t.attribs || [])[0];
    const scale = t.scale || 0;
    const aspect = t.aspect || '';
    const templateDamageType = attrib in DAMAGE_TYPE_MAP
      ? DAMAGE_TYPE_MAP[attrib]
      : (attrib === 'Tempdamage' ? shortHelpDamageType : null);
    if (templateDamageType && aspect === 'Absolute') {
      const dm = (t.duration || '').match(/([\d.]+)\s*seconds?/i);
      const durSec = dm ? parseFloat(dm[1]) : 0;
      if (durSec === 0 && scale > damageScale) {
        damageType = templateDamageType; damageScale = scale;
        tableName = requireTemplateTable(t, sourceName);
      }
      else if (durSec > 0) secondaryEffects.push(`DoT(${templateDamageType}) ${round(scale, 2)} scale/${round(durSec, 1)}s`);
    } else if (['Held', 'Stunned', 'Immobilized', 'Knocked', 'Sleep'].includes(attrib) && scale > 0) {
      // Mag rides `magnitude`; `scale` is the duration multiplier (see parseJudgement).
      secondaryEffects.push(`${attrib} Mag ${round(t.magnitude || scale, 1)}`);
    }
  }
  if (!tableName) {
    throw new Error(`${sourceName}: no direct-damage template found — cannot derive the damage table`);
  }
  let area = data.effect_area || 'Unknown';
  if (helpText.includes('Cone')) area = 'Cone';
  else if (helpText.includes('PBAoE')) area = 'PBAoE';
  else if (helpText.includes('Targeted') && helpText.includes('AoE')) area = 'Targeted AoE';
  else if (helpText.includes('AoE')) area = 'AoE';
  return {
    damageType: damageType || 'Unknown', effectArea: area,
    range: round(data.range || 0, 1), radius: round(data.radius || 0, 1), arc: round(data.arc || 0, 1),
    maxTargets: data.max_targets_hit || 0, activationTime: round(data.activation_time || 0, 2),
    recharge: round(requireRechargeTime(data, sourceName), 1), damageScale: round(damageScale, 2),
    tableName, secondaryEffects: [...new Set(secondaryEffects)],
  };
}

/** Socket: a proc/debuff applied to your attacks (mirrors Interface). */
function parseExemplarProc(data) {
  const label = data.display_short_help || data.display_name || '';
  let magnitude = 0, duration = 0, dotType = null, dotDamage = 0, dotTableName = '';
  for (const t of walkTemplates(data.effects)) {
    const attrib = (t.attribs || [])[0];
    const scale = t.scale || 0;
    const aspect = t.aspect || '';
    if (!attrib || scale === 0) continue;
    const dm = (t.duration || '').match(/([\d.]+)\s*seconds?/i);
    const durSec = dm ? parseFloat(dm[1]) : 0;
    if (aspect === 'Absolute' && attrib in INTERFACE_DOT_MAP) {
      if (Math.abs(scale) > Math.abs(dotDamage)) {
        dotType = INTERFACE_DOT_MAP[attrib]; dotDamage = round(Math.abs(scale));
        dotTableName = requireTemplateTable(t, data.full_name || 'socket exemplar');
        duration = durSec;
      }
    } else if (Math.abs(scale) > Math.abs(magnitude)) {
      magnitude = round(Math.abs(scale));
      if (durSec) duration = durSec;
    }
  }
  const out = { label, duration: round(duration, 2), procPeriod: round(data.activate_period || 0, 1) };
  if (magnitude > 0) { out.debuffType = label; out.debuffMagnitude = magnitude; }
  if (dotType) { out.dotType = dotType; out.dotDamage = dotDamage; out.dotTableName = dotTableName; }
  return out;
}

/** Data: a pet summon (mirrors Lore). */
function parseExemplarSummon(data) {
  const pets = [];
  let duration = 150;
  for (const eff of data.effects || []) {
    for (const t of eff.templates || []) {
      if ((t.attribs || [])[0] === 'Create_Entity') {
        const ed = (t.params || {}).entity_def || '';
        if (ed) pets.push(petTypeFromEntityDef(ed));
        const dm = (t.duration || '').match(/([\d.]+)\s*seconds?/i);
        if (dm) duration = parseFloat(dm[1]);
      }
    }
  }
  return {
    faction: factionFromPowerId((data.name || '').toLowerCase()),
    pets: pets.length ? pets : ['Pet'],
    duration: round(duration, 1), recharge: round(requireRechargeTime(data, data.full_name || 'data exemplar'), 1),
  };
}

/** Fate: a PBAoE ally buff (+End/+Recharge; Clarion adds mez protection). */
function parseExemplarBuff(data) {
  const stats = {};
  let mezProtection = 0;
  for (const eff of data.effects || []) {
    for (const t of eff.templates || []) {
      const attrib = (t.attribs || [])[0];
      const aspect = t.aspect || '';
      const scale = t.scale || 0;
      if (!attrib) continue;
      if (attrib === 'RechargeTime' && aspect === 'Strength') stats.recharge = Math.max(stats.recharge || 0, scale);
      else if (attrib === 'Recovery' && aspect === 'Current') stats.recovery = Math.max(stats.recovery || 0, scale);
      else if (attrib === 'Endurance' && aspect === 'Current' && scale > 0) stats.endurance = Math.max(stats.endurance || 0, scale);
      else if (aspect === 'Current' && scale < 0 && ['Held', 'Sleep', 'Stunned', 'Immobilized', 'Terrorized', 'Confused'].includes(attrib)) {
        mezProtection = Math.max(mezProtection, Math.abs(scale));
      }
    }
  }
  for (const k of Object.keys(stats)) stats[k] = round(stats[k]);
  const out = { stats, radius: round(data.radius || 0, 1), recharge: round(data.recharge_time || 0, 1) };
  if (mezProtection > 0) out.mezProtection = round(mezProtection, 1);
  return out;
}

/** Resolve a Genesis exemplar-power ref into a typed effect (by tree). */
function resolveGenesisExemplar(ref) {
  if (!ref) return null;
  const { tree, file } = genesisExemplarPath(ref);
  const data = readJson(file);
  if (!data) { console.warn(`  WARN: Genesis exemplar JSON missing: ${file}`); return null; }
  switch (tree) {
    case 'verdict': return { kind: 'attack', ...parseExemplarAttack(data) };
    case 'socket': return { kind: 'proc', ...parseExemplarProc(data) };
    case 'data': return { kind: 'summon', ...parseExemplarSummon(data) };
    case 'fate': return { kind: 'buff', ...parseExemplarBuff(data) };
    default: return null;
  }
}

// ============================================
// GENESIS EXTRACTION (all datasets; only Rebirth is released)
// ============================================
// All three exports carry a genesis dir, so this extracts everywhere — but only
// Rebirth authored real powers; HC and tspy carry placeholder help/icons and no
// exemplar linkage. The app serves Genesis for Rebirth alone (GENESIS-1). Each
// ability GRANTS two things: a
// Genesis_Silent "boost" power that buffs its partner incarnate slot (the
// "level 45+" effect, active when incarnates aren't exemplar-suppressed), and
// an exemplar-only power usable only below level 45 (the "44-" effect).
//
// Parse6 decodes Grant_Power as `Null` but still fills params.power_names, so we
// follow the silent chain (genesis -> Genesis_Silent.{slot}_Boost ->
// ..._Data_Boost) to the real effect templates — the same resolution the other
// slots use. The 4 trees map to partner slots: Data->Lore, Fate->Destiny,
// Socket->Interface, Verdict->Judgement.

const GENESIS_TREE_TO_SLOT = {
  data: 'lore', fate: 'destiny', socket: 'interface', verdict: 'judgement',
};

/** Map a silent-boost effect template to the Genesis 45+ stat key, or null. */
function genesisStatKey(t) {
  const attrib = (t.attribs || [])[0];
  if (!attrib) return null;
  const aspect = t.aspect;
  const table = (t.table || '').toLowerCase();
  if (aspect === 'Strength') {
    // Damage buff (Data/Verdict, and part of Fate's broad buff).
    if (attrib.endsWith('_Dmg') && attrib !== 'Heal_Dmg') return 'damage';
    if (attrib === 'Heal_Dmg') return 'strengthHeal';
    // Fate's "+X% Destiny effects" — uniform strength to def/res/mez/endmod.
    if (['Melee', 'Ranged', 'Area'].includes(attrib)) return 'strengthDefense';
    if (['Smashing', 'Lethal', 'Fire', 'Cold', 'Energy', 'Negative_Energy', 'Psionic', 'Toxic'].includes(attrib)) return 'strengthResistance';
    if (['Held', 'Stunned', 'Immobilized', 'Sleep', 'Confused', 'Afraid', 'Terrorized', 'Placate', 'Taunt'].includes(attrib)) return 'strengthMez';
    if (attrib === 'Endurance') return 'strengthEndMod';
    if (attrib === 'HitPoints') return 'strengthMaxHP';
    return null;
  }
  if (aspect === 'Maximum') {
    // Flat HP (Data, table Melee_Ones) vs % HP (Socket, table Melee_HealSelf).
    if (attrib === 'HitPoints') return table === 'melee_ones' ? 'maxHP' : 'maxHPPct';
    if (attrib === 'Endurance') return 'maxEndurance';
  }
  return null;
}

/** Recursively resolve a Genesis_Silent chain, recording each real (non-grant)
 *  template's stat into `effects`. Attribs that share a key+scale (all 8 damage
 *  types, all mez types) are recorded once — never summed. */
function accumulateGenesisSilent(silentName, cache, seen, effects) {
  if (seen.has(silentName)) return;
  seen.add(silentName);
  const data = cache[silentName];
  if (!data) return;
  for (const eff of data.effects || []) {
    for (const t of eff.templates || []) {
      if (isGrantPowerTemplate(t)) continue;
      const key = genesisStatKey(t);
      if (!key) continue;
      const scale = t.scale || 0;
      if (effects[key] === undefined || Math.abs(scale) > Math.abs(effects[key])) {
        effects[key] = round(scale);
      }
    }
  }
  for (const ref of extractGrantedPowers(data)) {
    accumulateGenesisSilent(silentRefToFilename(ref), cache, seen, effects);
  }
}

/** Collapse the raw per-attrib scales into the canonical Genesis shape: a
 *  single `tierPercent` amplification (0.025 | 0.05 | 0.075 | 0.1) plus, for
 *  Data, the flat Max HP buff its Lore pets get. Targeting is fixed per tree
 *  (see GENESIS_TREE_TO_SLOT) — the percent isn't a flat global player buff:
 *    data    → amplifies your Lore pets' damage (+ flat pet HP)
 *    verdict → amplifies your Judgement attack's damage
 *    fate    → amplifies your Destiny slot ability's effects (uniform strength)
 *    socket  → amplifies your Interface procs AND your player Max HP / Max End
 *  The bin stores each tree's percent in a different raw unit (damage/strength
 *  decimals are already the fraction; Socket's maxHPPct scale is 10x the
 *  fraction), so normalize them all to the same 0.025..0.1 tier value here.
 *  The wiki confirms Socket's player Max HP / Max End scale identically to the
 *  tier percent (T4 = +10% each), so the consumer derives both from tierPercent. */
function deriveGenesisTier(tree, raw) {
  switch (tree) {
    case 'data':
      return { tierPercent: raw.damage || 0, loreMaxHP: raw.maxHP || 0 };
    case 'verdict':
      return { tierPercent: raw.damage || 0 };
    case 'fate':
      return { tierPercent: raw.strengthDefense || raw.strengthResistance
        || raw.strengthMez || raw.strengthMaxHP || raw.strengthHeal
        || raw.strengthEndMod || 0 };
    case 'socket':
      return { tierPercent: round((raw.maxHPPct || 0) * 0.1) };
    default:
      return { tierPercent: 0 };
  }
}

/** The 44- exemplar-only power a Genesis ability grants (lives in its tree
 *  category, e.g. Incarnate.Data.Longbow_Core_Superior_Data). */
function extractGenesisExemplarPower(data, tree) {
  for (const eff of data.effects || []) {
    for (const t of eff.templates || []) {
      if (!isGrantPowerTemplate(t)) continue;
      const pnames = [...(t.power_names || []), ...((t.params && t.params.power_names) || [])];
      for (const pn of pnames) {
        const parts = pn.split('.');
        if (parts.length === 3 && parts[1].toLowerCase() === tree.toLowerCase()) return pn;
      }
    }
  }
  return '';
}

function extractGenesis() {
  console.log('\n=== GENESIS ===');
  const mainDir = path.join(RAW_BASE, 'genesis');
  const silentDir = path.join(RAW_BASE, 'genesis_silent');
  const results = {};

  const silentCache = {};
  for (const f of readDir(silentDir)) {
    const data = readJson(path.join(silentDir, f));
    if (data) silentCache[f.replace('.json', '')] = data;
  }

  for (const f of readDir(mainDir)) {
    const data = readJson(path.join(mainDir, f));
    if (!data) continue;
    const powerId = f.replace('.json', '');
    const tree = powerId.split('_')[0];
    const displayName = data.display_name || powerId;

    const effects = {};
    const seen = new Set();
    for (const ref of extractGrantedPowers(data)) {
      accumulateGenesisSilent(silentRefToFilename(ref), silentCache, seen, effects);
    }

    const exemplarRef = extractGenesisExemplarPower(data, tree);
    results[powerId] = {
      displayName,
      tree,
      enhancesSlot: GENESIS_TREE_TO_SLOT[tree] || '',
      ...deriveGenesisTier(tree, effects),
      exemplarPower: exemplarRef,
      exemplarEffect: resolveGenesisExemplar(exemplarRef),
    };

    const r = results[powerId];
    const extra = r.loreMaxHP ? `, loreMaxHP:${r.loreMaxHP}` : '';
    console.log(`  ${displayName}: →${r.enhancesSlot} +${(r.tierPercent * 100).toFixed(1)}%${extra}`);
  }

  console.log(`  Total: ${Object.keys(results).length} powers`);
  return results;
}

// ============================================
// TYPESCRIPT CODE GENERATION
// ============================================

/**
 * Build the incarnate tables.
 *
 * WAS `generateTypeScript`, which hand-formatted these same values as a TypeScript
 * module. Every doc comment that module carried is kept here, beside the table it
 * describes — those comments are the only written record of what each table means and
 * why it is shaped the way it is.
 *
 * TWO INTERFACES WENT WITH THE TYPESCRIPT and are recorded here instead:
 *   GeneratedDestinyTimelineTier  { value: number; duration: number }
 *   GeneratedHybridEffects        { tree, passive, frontLoaded, perTarget,
 *                                   maxTargets, duration, recharge }
 * The per-entry `// <display name>` comments went too. The keys are the game's own
 * power ids and every display name is still in the export this converter reads.
 */
function buildData(alpha, destiny, hybrid, iface, judgement, lore, genesis) {
  const data = {};

  // ---- ALPHA ----
  // Enhancement bonuses (decimal values, e.g. 0.33 = 33%).
  // Values are the total of regular + ED-bypass portions.
  data.GENERATED_ALPHA_EFFECTS = {};
  for (const [id, d] of Object.entries(alpha)) {
    const obj = {};
    if (d.levelShift) obj.levelShift = d.levelShift;
    Object.assign(obj, d.enhancements);
    data.GENERATED_ALPHA_EFFECTS[id] = obj;
  }

  // Per-aspect portion of each GENERATED_ALPHA_EFFECTS bonus that bypasses
  // Enhancement Diversification — the BoostIgnoreDiminishing-flagged template
  // (HC/Rebirth) or the generic `Ones` template (Thunderspy) in the silent
  // grant power. Authored per silent FILE, so secondary aspects can carry a
  // different split than the headline aspect. Absent aspect = no bypass
  // (the whole bonus is subject to ED).
  data.GENERATED_ALPHA_ED_BYPASS = {};
  for (const [id, d] of Object.entries(alpha)) {
    if (!d.edBypass || Object.keys(d.edBypass).length === 0) continue;
    data.GENERATED_ALPHA_ED_BYPASS[id] = d.edBypass;
  }

  // ---- DESTINY ----
  // Peak stat bonuses (decimals). These diminish over the 120s duration.
  //
  // Values are numeric percents/mags, EXCEPT `healTable` (a string): Rebirth's
  // click heal is stored as `healScale` (raw scale) + `healTable` and resolved
  // to actual HP against the build's archetype at display time.
  data.GENERATED_DESTINY_EFFECTS = {};
  for (const [id, d] of Object.entries(destiny)) {
    const obj = {};
    if (d.levelShift) obj.levelShift = d.levelShift;
    Object.assign(obj, d.peakEffects);
    if (d.healScale) { obj.healScale = round(d.healScale); obj.healTable = d.healTable; }
    data.GENERATED_DESTINY_EFFECTS[id] = obj;
  }

  // Diminishing-buff timeline: every stat's decay tiers as {value, duration}.
  // Destiny buffs (Barrier, Ageless, …) apply several independent, overlapping
  // buffs at once; the effective value at time t is the SUM of tiers whose
  // duration is still > t. GENERATED_DESTINY_EFFECTS above stores only the
  // single strongest tier (legacy "peak") — the timeline lets the calc resolve
  // the true additive peak and the decayed value at any point (Mids-style).
  // Only powers with at least one multi-tier stat are emitted (single-tier
  // stats are already fully described by GENERATED_DESTINY_EFFECTS).
  data.GENERATED_DESTINY_TIMELINE = {};
  for (const [id, d] of Object.entries(destiny)) {
    const timeline = d.timeline || {};
    const hasDecay = Object.values(timeline).some((tiers) => tiers.length > 1);
    if (!hasDecay) continue;
    data.GENERATED_DESTINY_TIMELINE[id] = timeline;
  }

  // Boost categories each Destiny power accepts (Res_Damage, Buff_Defense,
  // Heal, Recovery). The calc gates Alpha→Destiny enhancement on these — an
  // Alpha aspect only enhances a Destiny stat when the game says the power
  // accepts that boost type. Only powers with at least one enhanceable
  // category are emitted.
  data.GENERATED_DESTINY_BOOSTS = {};
  for (const [id, d] of Object.entries(destiny)) {
    const ba = d.boostsAllowed || [];
    if (ba.length === 0) continue;
    data.GENERATED_DESTINY_BOOSTS[id] = ba;
  }

  // ---- HYBRID ----
  // Three-layer model: passive (always-on), frontLoaded (toggle baseline), perTarget (per enemy).
  data.GENERATED_HYBRID_EFFECTS = {};
  for (const [id, d] of Object.entries(hybrid)) {
    data.GENERATED_HYBRID_EFFECTS[id] = {
      tree: d.tree,
      passive: d.passive,
      frontLoaded: d.frontLoaded,
      perTarget: d.perTarget,
      maxTargets: d.maxTargets,
      duration: d.duration,
      recharge: d.recharge,
    };
  }

  // ---- INTERFACE ----
  // Proc-based debuffs applied to enemies. For display, not dashboard stats.
  data.GENERATED_INTERFACE_EFFECTS = {};
  for (const [id, d] of Object.entries(iface)) {
    data.GENERATED_INTERFACE_EFFECTS[id] = {
      debuffType: d.debuffType,
      debuffMagnitude: d.debuffMagnitude,
      debuffDuration: d.debuffDuration,
      dotType: d.dotType,
      dotDamage: d.dotDamage,
      dotDuration: d.dotDuration,
      dotTableName: d.dotTableName,
      procChance: d.procChance,
    };
  }

  // ---- JUDGEMENT ----
  // Click attack powers. For display, not dashboard stats.
  // Fields: damageType, effectArea, range, radius, arc, maxTargets,
  // activationTime, rechargeTime, damageScale, tableName, secondaryEffects[].
  //
  // `displayName` is dropped. The text emitter dropped it with a regex over the
  // serialized entry (`.replace(/"displayName":"[^"]*",?/, '')`); here it is simply
  // not copied, which is the same result without depending on how JSON spells it.
  data.GENERATED_JUDGEMENT_EFFECTS = {};
  for (const [id, d] of Object.entries(judgement)) {
    const entry = {};
    for (const [k, v] of Object.entries(d)) {
      if (k !== 'displayName') entry[k] = v;
    }
    data.GENERATED_JUDGEMENT_EFFECTS[id] = entry;
  }

  // ---- LORE ----
  // Pet summoning powers. For display, not dashboard stats.
  data.GENERATED_LORE_EFFECTS = {};
  for (const [id, d] of Object.entries(lore)) {
    data.GENERATED_LORE_EFFECTS[id] = {
      faction: d.faction,
      pets: d.pets,
      duration: d.duration,
      rechargeTime: d.rechargeTime,
      levelShift: d.levelShift || 0,
    };
  }

  // ---- GENESIS ----
  // Emitted for every dataset whose export carries the slot (all three do). Only
  // Rebirth's table is served by the app; HC/tspy emit dormant placeholders that
  // the incarnate layer ignores (GENESIS-1). `effects` are the level-45+
  // partner-slot buffs; `exemplarPower` is the level-44- exemplar-only grant.
  //
  // Each ability AMPLIFIES a partner slot at level 45+ (and grants an
  // exemplar-only power below 45). `tierPercent` (0.025|0.05|0.075|0.1)
  // is the amplification; what it targets is fixed per tree:
  //   data    → your Lore pets' damage (+ flat `loreMaxHP`)
  //   verdict → your Judgement attack's damage
  //   fate    → your Destiny slot ability's effects
  //   socket  → your Interface procs AND your player Max HP / Max End
  // It is NOT a flat global player buff — see the incarnate layer.
  //
  // Entry fields: displayName, tree, enhancesSlot, tierPercent, loreMaxHP?,
  // exemplarPower, exemplarEffect? ({ kind, … } — the authoritative shape was
  // `GenesisExemplarEffect` in the app's incarnate-effects.ts).
  //
  // The whole table is omitted when the dataset's export carries no Genesis slot,
  // exactly as the text emitter omitted the export.
  if (genesis && Object.keys(genesis).length > 0) {
    data.GENERATED_GENESIS_EFFECTS = {};
    for (const [id, d] of Object.entries(genesis)) {
      const entry = {
        displayName: d.displayName,
        tree: d.tree,
        enhancesSlot: d.enhancesSlot,
        tierPercent: d.tierPercent,
      };
      if (d.loreMaxHP) entry.loreMaxHP = d.loreMaxHP;
      entry.exemplarPower = d.exemplarPower;
      if (d.exemplarEffect) entry.exemplarEffect = d.exemplarEffect;
      data.GENERATED_GENESIS_EFFECTS[id] = entry;
    }
  }

  return data;
}

// ============================================
// MAIN
// ============================================

function main() {
  console.log('Extracting incarnate effects from raw data...');
  console.log(`Source: ${RAW_BASE}`);

  if (!fs.existsSync(RAW_BASE)) {
    console.error(`ERROR: Raw data directory not found: ${RAW_BASE}`);
    process.exit(1);
  }

  const alpha = extractAlpha();
  const destiny = extractDestiny();
  const hybrid = extractHybrid();
  const iface = extractInterface();
  const judgement = extractJudgement();
  const lore = extractLore();
  const genesis = extractGenesis();

  // Tables written in alphabetical order. Not decoration: the TypeScript module this
  // converter used to emit was read back through a transpiler, which alphabetizes a
  // module's exports, so alphabetical is what every already-shipped copy of this section
  // is in. Fixing the order here keeps `contract/` byte-identical across the format
  // change, which is the only evidence that the change carried no value with it.
  const built = buildData(alpha, destiny, hybrid, iface, judgement, lore, genesis);
  const output = JSON.stringify(Object.fromEntries(
    Object.keys(built).sort().map((k) => [k, built[k]]),
  ));

  if (DRY_RUN) {
    console.log('\n=== DRY RUN — would write to:', OUTPUT_FILE);
    console.log(`=== Output size: ${output.length} characters`);
  } else {
    fs.mkdirSync(path.dirname(OUTPUT_FILE), { recursive: true });
    fs.writeFileSync(OUTPUT_FILE, output, 'utf8');
    console.log(`\nWrote ${output.length} bytes to ${path.relative(REPO_ROOT, OUTPUT_FILE)}`);
  }

  // Summary
  console.log('\n=== SUMMARY ===');
  console.log(`  Alpha:     ${Object.keys(alpha).length} powers`);
  console.log(`  Destiny:   ${Object.keys(destiny).length} powers`);
  console.log(`  Hybrid:    ${Object.keys(hybrid).length} powers`);
  console.log(`  Interface: ${Object.keys(iface).length} powers`);
  console.log(`  Judgement: ${Object.keys(judgement).length} powers`);
  console.log(`  Lore:      ${Object.keys(lore).length} powers`);
  console.log(`  Genesis:   ${Object.keys(genesis).length} powers`);
}

main();
