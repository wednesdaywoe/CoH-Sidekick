/**
 * audit-allowed-set-categories.cjs
 *
 * Validates the `allowedSetCategories` field on every archetype power by:
 *   1. Comparing the composed (generated ⊕ override) value against
 *      `allowed_set_categories` from the bin export — the authoritative
 *      per-power list the game itself uses, reversed out of boostsets.bin's
 *      per-set power lists. Any diff means an override is masking the
 *      authoritative answer.
 *   2. Flagging powers where the exporter did NOT produce authoritative data
 *      (the generated layer fell back to inference). Usually indicates a
 *      power not present in any IO set's allowed-powers list, or a stale
 *      export predating the boostsets parser.
 *   3. Applying independent invariants to the composed value:
 *        - Damage boost → power has at least one damage-flavor category
 *        - Range ≥ 150 + Damage + Range boost → has Sniper Attacks
 *        - Melee-only → no Ranged/Sniper damage flavor
 *        - Ranged (Range boost) → no Melee/Melee AoE flavor
 *        - Heal boost → Healing (or Accurate Healing if Accuracy is also present)
 *        - Hold/Immobilize/etc. boosts → corresponding category
 *        - Damaging AT power → carries that AT's archetype sets category
 *
 * Scope: the 26 archetype categories in CATEGORY_MAP (not pool/epic/incarnate).
 *
 * Usage:
 *   node scripts/audit-allowed-set-categories.cjs              # summary + first N
 *   node scripts/audit-allowed-set-categories.cjs --all        # full listings
 *   node scripts/audit-allowed-set-categories.cjs --drift      # drift only
 *   node scripts/audit-allowed-set-categories.cjs --invariants # invariants only
 *   node scripts/audit-allowed-set-categories.cjs --fallback   # inference-fallback only
 */

const fs = require('fs');
const path = require('path');
const {
  inferAllowedSetCategories,
  inferEffectiveArea,
  CATEGORY_MAP,
  EFFECT_AREA_MAP,
  toKebabCase,
  RAW_DATA_PATH,
} = require('./convert-powerset.cjs');
const { parseDatasetArg } = require('./_dataset-paths.cjs');

// Dataset-aware roots. `--dataset <id>` (default homecoming) selects both the
// export input and the generated/override output trees. Previously these were
// hardcoded to the pre-migration HC layout (`exported_powers/`,
// `src/data/generated/powersets`), which no longer exists — so the audit
// silently skipped EVERY power on EVERY dataset ("no generated .ts") — a dead
// guardrail, which is how Thunderspy's Subaluwa set spent months handing "Melee
// Damage" to 1,387 ranged attacks off a mis-read category.
// RAW_DATA_PATH is imported from convert-powerset.cjs so the export
// root resolves identically to the converter (HC → `exported_powers/`, others →
// `exported_powers/<id>/`).
const datasetId = parseDatasetArg();
const RAW_DATA = RAW_DATA_PATH;
const { forEachPowersetPower } = require('./_powerset-tree.cjs');

const args = process.argv.slice(2);
const SHOW_ALL = args.includes('--all');
const ONLY_DRIFT = args.includes('--drift');
const ONLY_INVARIANTS = args.includes('--invariants');
const ONLY_FALLBACK = args.includes('--fallback');
// --gate: CI mode. Exits non-zero on two high-confidence classes: the
// "contradiction" invariants (a genuine ranged attack carrying single-target
// Melee, or vice versa — the signature of one category landing across the wrong
// pool), and an override masking the export's own authoritative list (BRAIN-2).
// Suppresses the noisy informational sections (un-overridden augmentation
// drift, inference fallback, heuristic missing-category checks) so CI logs show
// only the actionable failures.
const GATE = args.includes('--gate');
const LIMIT = SHOW_ALL || GATE ? Infinity : 30;

// ─── helpers ────────────────────────────────────────────────────────────────

function readJsonSafe(p) {
  try { return JSON.parse(fs.readFileSync(p, 'utf8')); } catch { return null; }
}

/**
 * Extract the `allowedSetCategories` array literal from a generated or
 * override .ts file. Returns the array if the field is present, null
 * otherwise. Callers interpret null differently for generated (strict-mode
 * convert omits the field when no sets are allowed → "no sets") vs override
 * ("override doesn't touch this field" → fall through to generated).
 */
function arraysEqual(a, b) {
  if (a.length !== b.length) return false;
  const sa = [...a].sort(), sb = [...b].sort();
  return sa.every((v, i) => v === sb[i]);
}

function diff(actual, expected) {
  const a = new Set(actual), e = new Set(expected);
  const missing = [...e].filter(x => !a.has(x));
  const extra = [...a].filter(x => !e.has(x));
  return { missing, extra };
}

// Replicate the AT-slug extraction convert-powerset.cjs uses at call time.
function resolveCategoryInfo(category) {
  const info = CATEGORY_MAP[category];
  if (info) return info;
  // Suffix-stripped aliases (_aux → core category) don't appear in
  // CATEGORY_MAP; skip those here — they feed into redirects, not picks.
  return null;
}

/**
 * The converted powers, keyed by the export record each was converted from — the directory
 * (`scrapper_defense/bio_organic_armor`) and the internal name (`Efficient_Adaptation`), both
 * of which the converted power carries verbatim. Keyed on the display-name slug instead, 394
 * of Homecoming's 3,359 powers miss: an internal name and a display name are not the same
 * string wherever the game renamed one (`WS_Wide_Area_Web_Grenade`).
 *
 * The overrides arm is gone with the 372 empty stubs it read: `extractAllowedCategories`
 * returned null for every one of them, so `composed` was always `generated` and
 * `overriddenCats` was always 0. Adding one back means restoring the lookup, not reviving
 * a layer. All 372 override files across the four forks are empty records, and a regen onto a
 * clean tree writes none of them: the layer has no subject left.
 */
const convertedByExport = (() => {
  const index = new Map();
  forEachPowersetPower(datasetId, (power, ctx) => {
    if (!ctx.exportDir || typeof power.internalName !== 'string') return;
    index.set(`${ctx.exportDir}|${power.internalName}`, power);
  });
  return index;
})();

/**
 * Recreate the exact call that convert-powerset.cjs makes so the audit is
 * apples-to-apples, including the redirect probe (Head Splitter is marked
 * SingleTarget in the main file but redirects to a Cone AoE) and the
 * location-teleport special case (Shield Charge, Lightning Rod).
 *
 * NOTHING CALLS THIS, and it is the only caller of `inferAllowedSetCategories` — the converter
 * function this audit imports at the top of the file. So the audit pulls in the converter's own
 * inference and never runs it, and the apples-to-apples comparison the doc above promises does
 * not happen. The gate this script fails CI on is the contradiction-invariant arm, which is a
 * different check and does run.
 *
 * Left in place rather than deleted: an audit that has lost its comparison is a bigger question
 * than an unused function, and deleting the function makes the loss invisible. Found by the lint
 * gate on its first run, 2026-09-26, and is still open.
 */
// eslint-disable-next-line no-unused-vars
function inferForPower(powerJson) {
  const redirectArea = inferEffectiveArea(powerJson);
  const hasTeleportAttrib = (powerJson.effects || []).some(eff =>
    (eff.templates || []).some(t => (t.attribs?.[0] || '').toLowerCase() === 'teleport')
  );
  const isLocationTeleport = (powerJson.effect_area === 'Location' && hasTeleportAttrib);
  const effectiveArea = redirectArea
    ?? (isLocationTeleport ? 'AoE' : (EFFECT_AREA_MAP[powerJson.effect_area] ?? powerJson.effect_area));
  const boosts = (redirectArea || isLocationTeleport)
    ? (powerJson.boosts_allowed || []).filter(b => b !== 'Range')
    : (powerJson.boosts_allowed || []);
  return inferAllowedSetCategories(
    boosts,
    effectiveArea,
    powerJson.range,
    powerJson.powerset || powerJson.full_name,
    hasTeleportAttrib,
  );
}

// ─── invariants ─────────────────────────────────────────────────────────────

const DAMAGE_FLAVOR_CATEGORIES = new Set([
  'Melee Damage', 'Ranged Damage', 'Melee AoE Damage', 'Ranged AoE Damage',
  'Sniper Attacks', 'Pet Damage',
]);
const RANGED_DAMAGE_CATEGORIES = new Set([
  'Ranged Damage', 'Ranged AoE Damage', 'Sniper Attacks',
]);
// Below this range a single-target attack is melee/reach (which legitimately
// takes Melee Damage), not ranged. Ranged blasts are 40-100; snipes 150.
const RANGED_MIN_RANGE = 40;

const ARCHETYPE_DAMAGE_ATO = {
  blaster: 'Blaster Archetype Sets',
  brute: 'Brute Archetype Sets',
  corruptor: 'Corruptor Archetype Sets',
  defender: 'Defender Archetype Sets',
  scrapper: 'Scrapper Archetype Sets',
  sentinel: 'Sentinel Archetype Sets',
  stalker: 'Stalker Archetype Sets',
  tanker: 'Tanker Archetype Sets',
  'arachnos-soldier': 'Soldiers of Arachnos Archetype Sets',
  'arachnos-widow': 'Soldiers of Arachnos Archetype Sets',
  peacebringer: 'Kheldian Archetype Sets',
  warshade: 'Kheldian Archetype Sets',
};

const BOOST_REQUIRED_CATEGORY = {
  Heal: 'Healing',
  Healing: 'Healing',
  Hold: 'Holds',
  Stun: 'Stuns',
  Confuse: 'Confuse',
  Sleep: 'Sleep',
  Fear: 'Fear',
  Immobilize: 'Immobilize',
  Knockback: 'Knockback',
  Slow: 'Slow Movement',
  Taunt: 'Threat Duration',
  Debuff_Defense: 'Defense Debuff',
  Debuff_ToHit: 'To Hit Debuff',
  Buff_Defense: 'Defense Sets',
  Defense: 'Defense Sets',
  Res_Damage: 'Resist Damage',
  Resistance: 'Resist Damage',
  Buff_ToHit: 'To Hit Buff',
  ToHit: 'To Hit Buff',
};

function checkInvariants(powerJson, archetypeId, composed) {
  const issues = [];
  const boosts = new Set(powerJson.boosts_allowed || []);
  const cats = new Set(composed);
  const area = EFFECT_AREA_MAP[powerJson.effect_area] ?? powerJson.effect_area;
  const range = powerJson.range || 0;
  const hasDamage = boosts.has('Damage');
  const hasRange = boosts.has('Range');

  // Leap / teleport / redirect-AoE attacks have a Range boost (for the leap
  // itself) but deliver damage as Melee AoE around the landing point.
  // Detect Teleport and Execute_Power attribs; when present, skip the
  // melee/ranged flavor-mismatch invariants.
  const hasTeleportAttrib = (powerJson.effects || []).some(eff =>
    (eff.templates || []).some(t => (t.attribs?.[0] || '').toLowerCase() === 'teleport')
  );
  const hasExecutePower = (powerJson.effects || []).some(eff =>
    (eff.templates || []).some(t => (t.attribs?.[0] || '').toLowerCase() === 'execute_power')
  );
  const hasCreateEntity = (powerJson.effects || []).some(eff =>
    (eff.templates || []).some(t => (t.attribs?.[0] || '').toLowerCase() === 'create_entity')
  );
  // Execute_Power redirects (Lightning Rod, Savage Leap) and location-teleport
  // AoEs (Shield Charge) deliver Melee AoE damage around a landing point while
  // the host power carries a Range boost for the travel. The top-level attrib
  // scan above misses redirect-delivered damage, so also consult the converter's
  // own redirect probe — inferEffectiveArea returns a Cone/AoE area for these.
  // Without this, the "ranged → no melee flavor" invariant false-positives on
  // every charge/leap attack across all datasets.
  const redirectArea = inferEffectiveArea(powerJson);
  const isRedirectAoE = redirectArea === 'AoE' || redirectArea === 'Cone';
  const isLeapOrTeleportAttack = hasTeleportAttrib || hasExecutePower || isRedirectAoE;
  // Pet summons / self-targeted placements (Auto Turret, Acid Mortar) have
  // range 0 and target_type Self, but the pet itself does ranged AoE. Skip
  // the melee/ranged flavor check on these — the category describes the
  // pet's output, not the placement power.
  const isPetOrSelfPlacement = hasCreateEntity || powerJson.target_type === 'Self';

  // Boost → category (exact pair). Accurate variants are context-dependent
  // and handled below.
  for (const b of boosts) {
    const req = BOOST_REQUIRED_CATEGORY[b];
    if (req && !cats.has(req)) {
      // Accuracy + debuff/heal powers may substitute the plain category with
      // the "Accurate" variant — allow that.
      if (b === 'Heal' || b === 'Healing') {
        if (boosts.has('Accuracy') && cats.has('Accurate Healing')) continue;
      }
      if (b === 'Debuff_Defense') {
        if (boosts.has('Accuracy') && cats.has('Accurate Defense Debuff')) continue;
      }
      if (b === 'Debuff_ToHit') {
        if (boosts.has('Accuracy') && cats.has('Accurate To-Hit Debuff')) continue;
      }
      issues.push(`boost ${b} → missing category "${req}"`);
    }
  }

  if (hasDamage) {
    // Must have at least one damage flavor (besides Universal)
    const hasFlavor = [...cats].some(c => DAMAGE_FLAVOR_CATEGORIES.has(c));
    if (!hasFlavor) issues.push('damage boost but no Melee/Ranged/AoE/Sniper flavor');

    // Universal Damage should always be present on damage powers
    if (!cats.has('Universal Damage Sets')) {
      issues.push('damage boost but missing "Universal Damage Sets"');
    }

    // Sniper gate: range >= 150 single-target with a Range boost (Assassin
    // Strikes have range 150 but no Range enhancement — they're melee,
    // not snipers).
    if (range >= 150 && hasRange && area === 'SingleTarget' && !cats.has('Sniper Attacks')) {
      issues.push(`range ${range} ST → missing "Sniper Attacks"`);
    }

    // Melee-only shouldn't carry Ranged flavor. Skip pet summons and
    // Execute_Power redirects (e.g. turrets that shoot ranged while the
    // host power is marked melee).
    if (!hasRange && range === 0 && !hasExecutePower && !isPetOrSelfPlacement) {
      for (const c of cats) {
        if (RANGED_DAMAGE_CATEGORIES.has(c)) issues.push(`melee power has ranged damage category "${c}"`);
      }
    }
    // A genuine single-target ranged attack must not carry the single-target
    // "Melee Damage" category — that pairing is impossible in-game, and it is what
    // a whole category landing on the wrong pool looks like from the power's side
    // (Thunderspy's Subaluwa, read as ECMelee over a 1,905-power universal pool,
    // handed "Melee Damage" to ~1387 ranged blasts). We check only the
    // SINGLE-TARGET flavor on SINGLE-TARGET
    // powers: "Melee AoE Damage" is legitimate on melee cones (Breath of Fire,
    // range 15) and location-teleport PBAoEs (Lightning Rod, Savage Leap) that
    // also carry a Range boost, so those must not trip this. Leap/Execute_Power
    // redirects are excluded outright.
    // range >= RANGED_MIN_RANGE distinguishes a true ranged attack (blasts are
    // 40-100, snipes 150) from short-range melee/reach attacks that also carry
    // a Range boost (Executioner's Shot range 10, Warshade Essence Drain 7,
    // Point Blank 9) — those legitimately take Melee Damage and must not flag.
    if (hasRange && range >= RANGED_MIN_RANGE && area === 'SingleTarget' && !isLeapOrTeleportAttack) {
      if (cats.has('Melee Damage')) {
        issues.push('single-target ranged power has "Melee Damage" (KB-pollution signature)');
      }
    }

    // AT damage → AT archetype sets
    const ato = ARCHETYPE_DAMAGE_ATO[archetypeId];
    if (ato && !cats.has(ato)) {
      issues.push(`${archetypeId} damage power missing "${ato}"`);
    }
  }

  return issues;
}

// ─── walk ───────────────────────────────────────────────────────────────────

const driftRows = []; // { category, powerset, power, missing, extra, hasOverride }
const invariantRows = []; // { category, powerset, power, issues }
const fallbackRows = []; // { category, powerset, power } — no authoritative data
const stats = {
  powersScanned: 0,
  skippedNoTs: 0,
  overriddenCats: 0,
  hadAuthoritative: 0,
  fellBackToInference: 0,
};

for (const category of fs.readdirSync(RAW_DATA).sort()) {
  const catInfo = resolveCategoryInfo(category);
  if (!catInfo) continue; // skip pool/epic/redirect/etc. for now
  const catDir = path.join(RAW_DATA, category);
  if (!fs.statSync(catDir).isDirectory()) continue;

  for (const powerset of fs.readdirSync(catDir).sort()) {
    const psDir = path.join(catDir, powerset);
    if (!fs.statSync(psDir).isDirectory()) continue;
    const indexJson = readJsonSafe(path.join(psDir, 'index.json'));
    if (!indexJson) continue;
    const psSlug = toKebabCase(indexJson.display_name);

    for (const file of fs.readdirSync(psDir)) {
      if (file === 'index.json' || !file.endsWith('.json')) continue;
      const pJson = readJsonSafe(path.join(psDir, file));
      if (!pJson) continue;
      const powerSlug = toKebabCase(pJson.name);

      const converted = convertedByExport.get(`${category}/${powerset}|${pJson.name}`);
      if (!converted) { stats.skippedNoTs++; continue; }
      // Strict-mode convert omits `allowedSetCategories` entirely when no
      // sets slot in the power — treat null (field absent) as [].
      const generated = Array.isArray(converted.allowedSetCategories)
        ? converted.allowedSetCategories : [];
      const composed = generated;
      const hasOverride = false;
      stats.powersScanned++;

      // Authoritative check — compare composed to `allowed_set_categories`
      // from the export (reversed from boostsets.bin). Field PRESENT (even
      // if empty) is authoritative: empty means "game says no sets here",
      // non-empty means "exactly these sets". Field ABSENT means an old
      // export predating boostsets parsing — fall-back to inference.
      const hasAuthField = Array.isArray(pJson.allowed_set_categories);
      const authoritative = hasAuthField
        ? [...pJson.allowed_set_categories].sort()
        : null;

      if (hasAuthField) {
        stats.hadAuthoritative++;
        if (!arraysEqual(authoritative, composed)) {
          const { missing, extra } = diff(composed, authoritative);
          driftRows.push({
            category, powerset: psSlug, power: powerSlug,
            missing, extra, hasOverride,
          });
        }
      } else {
        stats.fellBackToInference++;
        fallbackRows.push({ category, powerset: psSlug, power: powerSlug });
      }

      // INVARIANTS — physical sanity checks on the composed (final) value,
      // run for EVERY power regardless of whether it had authoritative data.
      // Authoritative data is reversed from boostsets.bin, and a category read
      // off the wrong field is authoritative-looking: Thunderspy's Subaluwa,
      // taken as ECMelee over a 1,905-power universal pool, handed "Melee Damage"
      // to ~1387 ranged attacks. The drift check can't catch that (composed ==
      // the bad authoritative value), so these invariants — e.g. "a ranged attack
      // must not carry a Melee flavor" — are the backstop. Previously gated to
      // inference-fallback powers only, which is precisely why it went uncaught.
      const issues = checkInvariants(pJson, catInfo.archetype, composed);
      if (issues.length > 0) {
        invariantRows.push({
          category, powerset: psSlug, power: powerSlug, issues,
          hadAuthoritative: hasAuthField,
        });
      }
    }
  }
}

// ─── report ─────────────────────────────────────────────────────────────────

function printRows(title, rows, formatRow) {
  console.log(`\n=== ${title} (${rows.length}) ===`);
  const shown = rows.slice(0, LIMIT);
  for (const r of shown) console.log(formatRow(r));
  if (rows.length > shown.length) {
    console.log(`... ${rows.length - shown.length} more (run with --all to see)`);
  }
}

console.log(`Scanned ${stats.powersScanned} archetype powers ` +
            `(${stats.hadAuthoritative} with authoritative data, ` +
            `${stats.fellBackToInference} fell back to inference, ` +
            `${stats.overriddenCats} with allowedSetCategories override, ` +
            `${stats.skippedNoTs} skipped — not in the converted tree).`);

// Contradiction invariants = the high-confidence, broad-pollution signature a
// malformed boostset produces (a real ranged attack carrying single-target
// Melee, or a melee attack carrying a ranged flavor). These are the only class
// the CI gate fails on: they can't be false-positived by the heuristic
// "missing category" checks, and they're exactly what the Thunderspy "KB" set
// tripped across ~1387 powers.
const CONTRADICTION_MARKERS = ['KB-pollution signature', 'melee power has ranged'];

// Individually-reviewed exceptions, keyed `<dataset>:<category>/<powerset>/<power>`.
// The gate exists to catch a malformed set poisoning HUNDREDS of powers; a lone
// reviewed anomaly is baselined here so it neither blocks CI nor masks a future
// flood (a real polluter re-adds hundreds, dwarfing this list). Revisit each
// when that dataset's bins are re-extractable.
const CONTRADICTION_ALLOWLIST = new Set([
  // Rebirth Warshade Gravimetric Snare (range-80 immobilize) is listed under a
  // single-target Melee set in Rebirth's boostsets.bin. Isolated (1 of ~2800
  // Rebirth powers), so not the broad-polluter pattern; Rebirth bins aren't
  // on-hand here to root-cause. Baselined pending re-extraction.
  'rebirth:warshade_offensive/umbral-blast/gravimetric-snare',
]);

// BRAIN-2 gate — an override whose `allowedSetCategories` contradicts the export's
// own `allowed_set_categories` is a hand list outranking the authoritative one.
// Separated from the un-overridden drift bucket because the two have different
// causes: an un-overridden diff on Thunderspy is usually the converter's
// deliberate per-AT augmentation (expected, hence informational), while an
// overridden one is `withOverrides` replacing the whole array with a copy that
// the export has since moved past. That is how Brainstorm inherited Homecoming's
// verbatim Caltrops copy — inert on the fork it was authored for, and on
// Brainstorm it took the Knockback the newer export grants back off the power.
//
// What this cannot see: the 8 powers per dataset whose export carries no
// `allowed_set_categories` field at all (Kheldian/Widow/Bane passives —
// Quantum Flight, Fate Sealed, Mental Training and the rest). Their generated
// layer omits the field, the override supplies it, and there is no
// authoritative value to grade it against. They are counted in the scan line's
// "fell back to inference".
const staleOverrideRows = driftRows.filter(r => r.hasOverride);

const contradictionRows = invariantRows.filter(r =>
  r.issues.some(i => CONTRADICTION_MARKERS.some(m => i.includes(m))) &&
  !CONTRADICTION_ALLOWLIST.has(`${datasetId}:${r.category}/${r.powerset}/${r.power}`));

if (!GATE) {
  if (!ONLY_INVARIANTS && !ONLY_FALLBACK) {
    // Drift = composed differs from authoritative. Split by whether an override
    // is masking it (likely stale) vs. the generated layer just needs regen.
    // NB for datasets whose converter augments per-power categories (Thunderspy
    // adds its AT ATOs + Universal Damage that the bin omits), most "extra"
    // drift is expected, not a bug — hence drift is informational, not gated.
    const bugs = driftRows.filter(r => !r.hasOverride);
    const overridden = driftRows.filter(r => r.hasOverride);

    printRows('DRIFT — generated disagrees with authoritative (regen candidates)', bugs, r =>
      `  ${r.category}/${r.powerset}/${r.power}` +
      (r.missing.length ? `\n    + add:    ${r.missing.join(', ')}` : '') +
      (r.extra.length   ? `\n    - remove: ${r.extra.join(', ')}` : '')
    );
    printRows('DRIFT — override masks authoritative value (likely stale)', overridden, r =>
      `  ${r.category}/${r.powerset}/${r.power}` +
      (r.missing.length ? `\n    + add:    ${r.missing.join(', ')}` : '') +
      (r.extra.length   ? `\n    - remove: ${r.extra.join(', ')}` : '')
    );
  }

  if (!ONLY_DRIFT && !ONLY_INVARIANTS) {
    printRows('INFERENCE FALLBACK — power not present in any IO set', fallbackRows, r =>
      `  ${r.category}/${r.powerset}/${r.power}`
    );
  }

  if (!ONLY_DRIFT && !ONLY_FALLBACK) {
    printRows('INVARIANT VIOLATIONS — composed value fails a sanity check', invariantRows, r =>
      `  ${r.category}/${r.powerset}/${r.power}\n    - ${r.issues.join('\n    - ')}`
    );
  }
}

// The gate fails on contradiction invariants only. Drift and heuristic
// missing-category invariants are informational (printed above in non-gate
// mode). Staleness of generated files is separately covered by the
// regen-and-diff CI guard.
printRows(
  'GATE — malformed-boostset contradictions (fails CI)',
  contradictionRows,
  r => `  ${r.category}/${r.powerset}/${r.power}\n    - ` +
       r.issues.filter(i => CONTRADICTION_MARKERS.some(m => i.includes(m))).join('\n    - '),
);
printRows(
  'GATE — override masks the authoritative category list (fails CI)',
  staleOverrideRows,
  r => `  ${r.category}/${r.powerset}/${r.power}` +
       (r.missing.length ? `\n    + export has, override drops: ${r.missing.join(', ')}` : '') +
       (r.extra.length   ? `\n    - override adds, export lacks: ${r.extra.join(', ')}` : ''),
);
if (contradictionRows.length === 0 && staleOverrideRows.length === 0) {
  console.log('\nGATE PASS — no malformed-boostset category contradictions, ' +
              'no override masking the authoritative list.');
} else {
  if (contradictionRows.length > 0) {
    console.log(`\nGATE FAIL — ${contradictionRows.length} contradiction(s). A boostset's ` +
                `category is reaching a pool it does not belong to (see _resolve_category ` +
                `in _boostsets.py).`);
  }
  if (staleOverrideRows.length > 0) {
    console.log(`\nGATE FAIL — ${staleOverrideRows.length} override(s) masking the export's ` +
                `own allowed_set_categories. Empty the override's array rather than ` +
                `re-stating the new value; the generated layer already carries it.`);
  }
}

process.exit(contradictionRows.length > 0 || staleOverrideRows.length > 0 ? 1 : 0);
