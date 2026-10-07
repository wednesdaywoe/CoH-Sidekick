'use strict';

/**
 * Variant modes — the mechanic that swaps one effect group of an attack for another as the
 * caster changes mode, written in the binary as a chance modifier on a TAG rather than as a
 * gate on the group.
 *
 * Dual Pistols' Swap Ammo is the whole population (COND-9). Every one of the set's attacks
 * carries four tagged secondary groups — `Lethal`, `ColdDamage`, `FireDamage`, `ToxicDamage`
 * — and the loaded one is chosen from outside the attack entirely:
 *
 *     Cryo_Ammunition   Set_Mode IceAmmo
 *                       Global_Chance_Mod +1.0  EffectFilter tags ColdDamage
 *                       Global_Chance_Mod -1.0  EffectFilter tags Lethal
 *
 * so the `ColdDamage` groups ship at `chance: 0` and the `Lethal` ones at `chance: 1`, and
 * nothing on the attack says why. Every other mode-gated group in the corpus states its own
 * `kSomeMode Source.Mode?` — Bio Armor's stances, the Kheldian forms, and in this very set
 * Suppressive_Fire's redirect table, which spells the standard-rounds branch
 * `kIceAmmo Source.Mode? kFireAmmo Source.Mode? || kToxicAmmo Source.Mode? || !`. The tag
 * mechanic is the same fact in a form no single group can see, which is exactly the case §4 of
 * the skill reserves for the converter: read the whole set, then hand every collector the gate
 * the mechanic implies.
 *
 * What that replaces is `extractDualPistolsAmmo`, which reshaped the BAG only, keyed on the
 * powerset name containing `dual_pistols`, and left the wire atoms ungated — the asymmetry
 * COND-9 records. Nothing here names a power, a set, an ammo or a mode: a selector is
 * recognised by its shape, and the modes and labels come out of the data it publishes.
 *
 * The shape, all four clauses required:
 *
 *   1. the power publishes exactly ONE mode (`Set_Mode`, `mode_name`) — a power publishing
 *      several names no single gate, so it is reported and skipped rather than half-read;
 *   2. it carries `Global_Chance_Mod` templates, the only shape that retunes another group's
 *      chance (their filter names arrive under `EffectFilter.tags` on Parse7 and under the
 *      `Power` arm's `power_names` on Parse6 — see `filterNames`);
 *   3. at least one tag is raised by `+1` or more — taking a never-firing group to certain,
 *      which is what "this variant is now loaded" looks like;
 *   4. at least one OTHER tag is cut by `-1` or more — the variant it displaces.
 *
 * Clause 4 is what keeps this off the 200-odd other `EffectFilter` carriers. Fiery Embrace,
 * the Staff Fighting forms, Fusion, Blood Thirst and the flight toggles all raise a tag
 * without displacing one: they ADD a rider to whatever attack is running rather than swapping
 * a variant, and gating their groups here would move numbers this mechanic has nothing to do
 * with. Measured over the three exports (re-measured 2026-08-18, after COND-11 put the Parse6
 * forks' tags on the wire) the four clauses select: Homecoming's twelve Dual Pistols ammo
 * toggles plus Light Form (whose filter is inert there — see the census's INERT_SELECTORS);
 * Rebirth's two Granite Armor copies (the authored `Granite_Armor_Mode` suspension of the
 * other Stone toggles, carriers live since the tag fix) and its Light Form (filter names tags
 * worn only in Temporary_Powers, outside this per-set scope); Thunderspy nothing.
 *
 * Clause 1 is what keeps the AMMO mechanic Homecoming-only. Rebirth and Thunderspy ship the
 * same toggles with the same thirteen filters and the same deltas, and no `Set_Mode` row on
 * any of them — only Swap Ammo itself publishes a mode there. So those forks have no caster
 * state to gate on, their ammo variants stay in base as the `chance: 0` rows the export makes
 * them, and that is recorded as COND-10 rather than papered over: the reshape this file
 * replaces was inventing three per-ammo toggles on both of them, off a string match on the
 * powerset name.
 *
 * The families this pass structurally CANNOT read — movers with no shared parent and no mode,
 * spread across powersets (the flight tags, the Kheldian shapeshift suppression) — are the
 * corpus-wide pass's job: `detect-chance-mod-selectors.cjs` writes the per-dataset map and
 * [`installGlobalVariantModes`] carries it here (COND-12). One tag is never claimed by both:
 * the detector declines anything a per-set selector loads or displaces.
 */

const { composeGates } = require('./_gate-tokens.cjs');

/** The attrib whose `EffectFilter` params retune another group's chance. */
const FILTER_ATTRIB = 'global_chance_mod';

/** The `params` union arm that names tags rather than powers or entities. */
const FILTER_PARAM_TYPE = 'EffectFilter';

/** The attrib that publishes a caster mode; the mode is on the template's `mode_name`. */
const MODE_ATTRIB = 'set_mode';

/** A chance mod at or above this takes a `chance: 0` group to certain — the variant loads. */
const ENABLE_DELTA = 1.0;

/** A chance mod at or below this takes a `chance: 1` group to never — the variant unloads. */
const DISABLE_DELTA = -1.0;

/**
 * The `requires_default` verdict a positive mode gate carries — what the parser exports for every
 * authored one, and therefore what an injected one has to state to be indistinguishable from it.
 */
const DEFAULT_VERDICT_GATED = 'UNSATISFIED';

/** The mode-reader token, and the `k`-prefix its operand wears in a gate. */
const MODE_READER = 'Source.Mode?';
const MODE_PREFIX = 'k';

/** Every effect group in a tree, parents before children. */
function walkGroups(effects, visit) {
  for (const effect of effects || []) {
    if (!effect) continue;
    visit(effect);
    walkGroups(effect.child_effects, visit);
  }
}

/**
 * The effect-filter names a `Global_Chance_Mod` row retunes, whichever params arm the fork spells
 * them in.
 *
 * Parse7 carries them as `EffectFilter.tags`; Parse6 carries the SAME names — `ColdDamage`,
 * `HailofBulletsFire`, the lot, with the same thirteen deltas in the same order — under the
 * `Power` arm's `power_names`, because the older format had no dedicated filter arm. The attrib is
 * what settles the meaning of the list: `Global_Chance_Mod` retunes tagged effect groups, never a
 * power, so a name here is a filter name on either fork. Reading only Parse7's arm would make the
 * mechanic invisible on the other two rather than absent from them.
 */
function filterNames(params) {
  if (!params) return [];
  if (params.type === FILTER_PARAM_TYPE) return params.tags || [];
  return params.power_names || [];
}

/** A template's attrib, folded — a template states one per row here. */
function attribOf(template) {
  const attribs = template.attribs || [];
  return attribs.length === 1 ? String(attribs[0]).toLowerCase() : null;
}

/**
 * The one variant selector a power is, or `null` for the overwhelming majority that are not one.
 *
 * `problems` collects the powers that match clauses 2-4 but fail clause 1, so an ambiguous
 * publisher is REPORTED rather than dropped in silence: a mechanic this pass declines to read
 * leaves its groups ungated, which reads exactly like a set that has no variant modes.
 */
function selectorOf(powerJson, problems) {
  const modes = new Set();
  const deltas = new Map();
  walkGroups(powerJson.effects, (effect) => {
    for (const template of effect.templates || []) {
      const attrib = attribOf(template);
      if (attrib === MODE_ATTRIB && template.mode_name) {
        modes.add(template.mode_name);
        continue;
      }
      if (attrib !== FILTER_ATTRIB) continue;
      const delta = template.scale;
      if (typeof delta !== 'number') continue;
      for (const tag of filterNames(template.params)) {
        // Keep the strongest claim per tag: a selector states one delta per tag in the
        // corpus, and taking the extreme keeps that true rather than depending on it.
        const seen = deltas.get(tag);
        if (seen === undefined || Math.abs(delta) > Math.abs(seen)) deltas.set(tag, delta);
      }
    }
  });

  const enables = [...deltas].filter(([, d]) => d >= ENABLE_DELTA).map(([tag]) => tag);
  const disables = [...deltas].filter(([, d]) => d <= DISABLE_DELTA).map(([tag]) => tag);
  if (!enables.length || !disables.length) return null;

  // A swapper publishing NO mode is a different mechanic, not an under-read variant: there is
  // no caster state to gate on, and the tags it trades belong to two riders of one power
  // (Meltdown's high/low heal, the Staff Fighting forms' three perfections). Those are recorded
  // in the register rather than reported here, because a line on every conversion would say
  // "something is broken" about data this pass correctly has nothing to say about.
  if (modes.size === 0) return null;
  if (modes.size !== 1) {
    problems.push(
      `${powerJson.full_name || powerJson.name}: swaps tags [${enables.join(', ')}] for `
      + `[${disables.join(', ')}] but publishes ${modes.size} modes `
      + `[${[...modes].join(', ')}] — no single mode names the gate, so its groups stay ungated`);
    return null;
  }

  return {
    power: powerJson.full_name || powerJson.name,
    label: powerJson.display_name || powerJson.name,
    mode: [...modes][0],
    enables: enables.sort(),
    disables: disables.sort(),
    // The tags this selector raises by less than a full point — a rider whose chance it RETUNES
    // rather than switches (see [`retuneOf`]) — and every tag it cuts at all, which is what rules
    // a retune out.
    retunes: [...deltas].filter(([, d]) => d > 0 && d < ENABLE_DELTA).sort(([a], [b]) => a.localeCompare(b)),
    cutDeltas: [...deltas].filter(([, d]) => d < 0).sort(([a], [b]) => a.localeCompare(b)),
    // Whose grant this selector rides — the dotted path its `requires` names. The DEFAULT
    // variant is that power's own state, so this is what finds it.
    requires: (powerJson.requires || []).map(String),
  };
}

/**
 * The variant every selector displaces — the form the caster is in until one loads — named by the
 * power the selectors are granted BY.
 *
 * Swap Ammo is an `Auto` the build picks, and holding it publishes `LethalAmmo`; each ammo toggle
 * `requires` that pick and displaces the `Lethal` tag. So the parent's own published mode is the
 * default form's name, and the id it folds to (`lethalammo`) is the one
 * `coh_data::caster_state` looks for when it labels a stance group's CLEARED state. Read from the
 * grant relation rather than from a table of names: the parent is the power every selector's
 * `requires` names, and it has to publish exactly one mode to name anything.
 *
 * `null` where the selectors name no common parent or the parent publishes no mode — then the
 * cleared state simply has no name, which is what `CLEARED_FALLBACK` is for.
 */
function defaultVariantOf(selectors, powerJsons) {
  if (!selectors.length) return null;
  const paths = selectors.map((s) => new Set(s.requires.map((r) => r.toLowerCase())));
  const shared = [...paths[0]].filter((path) => paths.every((set) => set.has(path)));
  if (shared.length !== 1) return null;
  const parent = powerJsons.find(
    (p) => (p.full_name || '').toLowerCase() === shared[0]);
  if (!parent) return null;
  const modes = new Set();
  walkGroups(parent.effects, (effect) => {
    for (const template of effect.templates || []) {
      if (attribOf(template) === MODE_ATTRIB && template.mode_name) modes.add(template.mode_name);
    }
  });
  if (modes.size !== 1) return null;
  const mode = [...modes][0];
  return { mode, id: mode.toLowerCase(), parent: parent.full_name };
}

/** The gate that holds while `mode` is live. */
function modeGate(mode) {
  return [`${MODE_PREFIX}${mode}`, MODE_READER];
}

/**
 * The gate that holds while NONE of `modes` is live — the default variant's own condition,
 * spelled the way the export spells it on Suppressive_Fire's redirect table: the modes ORed
 * together and the whole thing negated.
 *
 * Sorted by mode name because the order carries no meaning and a stable one keeps the emitted
 * bundle byte-comparable across runs (the set's file order is the filesystem's, not the data's).
 */
function noModeGate(modes) {
  const tokens = [];
  for (const [index, mode] of [...modes].sort().entries()) {
    tokens.push(...modeGate(mode));
    if (index > 0) tokens.push('||');
  }
  tokens.push('!');
  return tokens;
}

/**
 * The variant map a set of power files implies: which tag each mode loads, and which tags are
 * the default that the modes displace.
 *
 * Scoped to ONE powerset directory on purpose. The mode is published by a power the same set
 * hands out (each ammo toggle `requires` Swap Ammo), so the set is the whole scope the mechanic
 * needs, and a corpus-wide map would gate an NPC's copy of a tagged group on a mode no build
 * holding that power can publish — the 188 `ColdDamage` groups outside the four player sets are
 * mission-maker and pet records with no ammo toggle anywhere near them.
 */
function detectVariantModes(powerJsons) {
  const problems = [];
  const selectors = [];
  for (const powerJson of powerJsons) {
    const selector = selectorOf(powerJson, problems);
    if (selector) selectors.push(selector);
  }
  for (const problem of problems) console.warn(`[variant-modes] ${problem}`);

  // Several archetypes' copies of one ammo toggle are the same selector (same mode, same tags),
  // and inside a set there is exactly one copy of each. Key by mode so a set that somehow ships
  // two publishers of one mode states it once.
  const byMode = new Map();
  for (const selector of selectors) {
    const seen = byMode.get(selector.mode);
    if (!seen) {
      byMode.set(selector.mode, selector);
      continue;
    }
    if (seen.enables.join() !== selector.enables.join()) {
      console.warn(
        `[variant-modes] ${selector.power} and ${seen.power} both publish ${selector.mode} but `
        + `load different tags ([${selector.enables}] vs [${seen.enables}]) — keeping the first`);
    }
  }

  const enable = new Map();
  const disabledBy = new Map();
  for (const selector of byMode.values()) {
    for (const tag of selector.enables) {
      const seen = enable.get(tag);
      if (seen && seen.mode !== selector.mode) {
        console.warn(
          `[variant-modes] tag ${tag} is loaded by both ${seen.mode} and ${selector.mode} — `
          + 'two modes cannot name one variant, so the tag stays ungated');
        enable.set(tag, null);
        continue;
      }
      if (seen === null) continue;
      enable.set(tag, selector);
    }
    for (const tag of selector.disables) {
      if (!disabledBy.has(tag)) disabledBy.set(tag, new Set());
      disabledBy.get(tag).add(selector.mode);
    }
  }
  for (const [tag, selector] of [...enable]) if (!selector) enable.delete(tag);

  // A tag both loaded and displaced states two things at once; neither reading is safe, so it
  // is reported and left alone. No corpus tag does this at these thresholds.
  const defaults = new Map();
  for (const [tag, modes] of disabledBy) {
    if (enable.has(tag)) {
      console.warn(
        `[variant-modes] tag ${tag} is both loaded and displaced by variant modes — `
        + 'left ungated');
      enable.delete(tag);
      continue;
    }
    defaults.set(tag, noModeGate(modes));
  }

  const kept = [...byMode.values()];
  return {
    enable,
    defaults,
    retune: retuneMap(kept.map((selector) => ({
      mode: selector.mode, retunes: selector.retunes, cutDeltas: selector.cutDeltas,
    })), (tag) => enable.has(tag) || defaults.has(tag)),
    selectors: kept,
    defaultVariant: defaults.size ? defaultVariantOf(kept, powerJsons) : null,
  };
}

/**
 * The tags a variant mode RETUNES: raised by less than a full point, and by exactly one mode.
 *
 * A full-point raise switches a variant on; a smaller one sets the chance of a rider the variant
 * carries. Incendiary Ammo is the case — `Global_Chance_Mod +1.0` on `FireDamage` loads the fire
 * round, and `+0.8` on `FireDamageDoT` is the burn's per-tick chance, which the attack ships at
 * `tick_chance: 0` because it never burns with any other round loaded. Left unread, the burn
 * reached the planner gated on the fire round but inert under it.
 *
 * Each entry also carries the mode's own sub-point CUTS. A retune is only stamped where those cuts
 * have nothing to act on ([`applyVariantGates`]): Hail of Bullets wears `HailofBulletsLethal` at a
 * standing 0.6 per tick that every ammo cuts by 0.6 while raising its own type's ticks, and
 * stamping the raised side alone would count the lethal ticks and the fire ticks together. The
 * other ammos' cuts of a retuned tag are no-ops here — the carrier sits at zero, and a chance below
 * zero never fires either. A tag that [`isClaimed`] already gives a switching gate is the switch's.
 */
function retuneMap(movers, isClaimed) {
  const retune = new Map();
  for (const mover of movers) {
    for (const [tag, delta] of mover.retunes) {
      if (isClaimed(tag)) continue;
      const seen = retune.get(tag);
      if (seen === undefined) {
        retune.set(tag, { mode: mover.mode, delta, cuts: partialCuts(movers, mover.mode) });
      } else if (seen && (seen.mode !== mover.mode || seen.delta !== delta)) {
        retune.set(tag, null);
      }
    }
  }
  for (const [tag, entry] of [...retune]) if (!entry) retune.delete(tag);
  return retune;
}

/** Every sub-point cut the movers of `mode` make, sorted for a stable artifact. */
function partialCuts(movers, mode) {
  const cuts = new Set();
  for (const mover of movers) {
    if (mover.mode !== mode) continue;
    for (const [tag, delta] of mover.cutDeltas || []) if (delta > DISABLE_DELTA) cuts.add(tag);
  }
  return [...cuts].sort();
}

/** The active map, installed per powerset. Absent means no set-local variant mechanic. */
let ACTIVE = null;

/**
 * The corpus-wide map, installed once per process from the dataset's
 * `chance-mod-selectors.json` (COND-12). Same shape as the per-set map, different scope and
 * different mode provenance: these families publish no `Set_Mode`, so the gate token is MINTED
 * from the tag itself and the movers' emitted `setsModes` gain it (`globalMoverModes`). The
 * per-set map wins a tag lookup, and the detector keeps the two disjoint (a COND-9-claimed tag
 * is declined corpus-wide), so the preference is an ordering, not an adjudication.
 */
let GLOBAL = null;

/** Install the map for the set being converted (or clear it with `null`). */
function installVariantModes(map) {
  ACTIVE = map && (map.enable.size || map.defaults.size || map.retune?.size) ? map : null;
}

/** The installed map, for a caller that wants to report what was detected. */
function activeVariantModes() {
  return ACTIVE;
}

/**
 * Install the corpus-wide chance-mod selector families for this dataset — the `selected` list
 * of `detect-chance-mod-selectors.cjs`'s sweep artifact. Raised tags become the enable side
 * (their carriers' `chance: 0` groups are stamped `k<Tag> Source.Mode?` and promoted, exactly
 * as a COND-9 mode gate is); cut tags become the defaults side (their carriers' `chance: 1`
 * atoms gain the negation, and stay in the bag as base). Sticky for the process lifetime,
 * because the scope is the corpus, not the set being converted.
 */
function installGlobalVariantModes(selectedFamilies, retunes = []) {
  const enable = new Map();
  const defaults = new Map();
  const moverModes = new Map();
  for (const family of selectedFamilies || []) {
    if (family.dir === 'raise') {
      enable.set(family.tag, { power: null, label: splitModeToken(family.tag), mode: family.tag });
    } else {
      defaults.set(family.tag, noModeGate([family.tag]));
    }
    for (const mover of family.movers || []) {
      const key = mover.toLowerCase();
      if (!moverModes.has(key)) moverModes.set(key, []);
      moverModes.get(key).push(family.tag);
    }
  }
  for (const tags of moverModes.values()) tags.sort();
  // The detector has already applied [`retuneMap`]'s rules corpus-wide; each entry names the
  // minted mode of the family its movers raise.
  const retune = new Map((retunes || []).map((entry) => [
    entry.tag, { mode: entry.mode, delta: entry.delta, cuts: entry.cuts || [] },
  ]));
  GLOBAL = enable.size || defaults.size || retune.size ? { enable, defaults, moverModes, retune } : null;
}

/**
 * The minted mode tokens `power` publishes because it MOVES a selected family's tag — what
 * `assignModes` appends to the emitted `setsModes`, so the engine's ordinary mode resolution
 * (`coh_math::gather::collect_source_modes`) satisfies the stamped gates from what the build
 * is actually running. Empty for every power that moves nothing.
 */
function globalMoverModes(fullName) {
  return GLOBAL?.moverModes.get(String(fullName || '').toLowerCase()) || [];
}

/**
 * The display label of a corpus-wide family's conditional, by the id the classifier derives
 * from its minted gate (`kFlightActive Source.Mode?` → `flightactive`) — or `null` for every
 * other conditional. The tag's own camel-case reading is the label, because the generic
 * powerset-prefix refinement would strip `Flight` from `Flight Active` on a Flight-pool power.
 */
function globalConditionalLabel(id) {
  if (!GLOBAL || !id) return null;
  for (const tag of [...GLOBAL.enable.keys(), ...GLOBAL.defaults.keys()]) {
    if (tag.toLowerCase() === String(id).toLowerCase()) return splitModeToken(tag);
  }
  return null;
}

/**
 * Write the implied gate onto every group in `effects` that a variant mode LOADS, in place.
 *
 * Only the loaded side is written into the tree, and only where the group's own chance is zero:
 * that is the group the export leaves inert and unexplained, and once it carries
 * `kIceAmmo Source.Mode?` every collector already knows what to do with it — the bag's
 * `collectAllTemplates` skips a positive mode gate, the conditional collector folds the mode
 * into an `iceammo` entry, and `encodeAtomsForEmit` stamps `gated: true` because the template is
 * no longer in the bag's set. The chance moves with the gate for the same reason the gate is
 * written at all: while the mode is live the game's chance for that group is `0 + delta`, and a
 * promoted atom still reading zero would be re-admitted inert.
 *
 * The DEFAULT side is not written here. `collectAllTemplates` drops any group whose gate merely
 * mentions `Source.Mode?`, so spelling the standard-variant condition into the tree would strip
 * the base debuff out of the bag — the very divergence COND-9 exists to close. It is stamped on
 * the atoms instead, where the reader that honours a gate can see it ([`defaultVariantGate`]).
 *
 * Idempotent: a tree walked by several collectors is only stamped once.
 */
function applyVariantGates(effects, inheritedMode = null) {
  if (!ACTIVE && !GLOBAL) return;
  const outer = BLOCKED_RETUNES;
  if (outer === null) BLOCKED_RETUNES = blockedRetuneModes(effects);
  try {
    for (const effect of effects || []) {
      if (!effect) continue;
      const mode = _gateOne(effect, inheritedMode);
      applyVariantGates(effect.child_effects, mode);
    }
  } finally {
    if (outer === null) BLOCKED_RETUNES = null;
  }
}

/** The retuning modes this power's tree rules out, set for the length of one outermost walk. */
let BLOCKED_RETUNES = null;

/**
 * The modes whose retunes cannot be stamped on THIS power: a mode that also cuts, by less than a
 * full point, a row here that would otherwise fire. That cut is a second half of the same swap with
 * no gate of its own, so stamping the raised half alone would count both ([`retuneMap`]).
 */
function blockedRetuneModes(effects) {
  const entries = [
    ...(ACTIVE?.retune ? ACTIVE.retune.values() : []),
    ...(GLOBAL?.retune ? GLOBAL.retune.values() : []),
  ];
  const blocked = new Set();
  if (!entries.length) return blocked;
  const live = new Set();
  walkGroups(effects, (effect) => {
    if (effect.chance === 0) return;
    const fires = (effect.templates || []).some((t) => t.tick_chance !== 0);
    if (!fires) return;
    for (const tag of effect.tags || []) live.add(tag);
  });
  for (const entry of entries) {
    if ((entry.cuts || []).some((tag) => live.has(tag))) blocked.add(entry.mode);
  }
  return blocked;
}

/**
 * Gate one group, returning the variant mode in force for its children.
 *
 * A group INSIDE a gated subtree is stamped with the same mode even though its own chance is not
 * zero, and that is not redundancy: the conditional collector reads each group's own gate, and an
 * ungated child under a gated parent has its templates dropped by both sides — Suppressive Fire's
 * Cryo branch is exactly that shape (a `chance: 0` `ColdDamage` parent over a `chance: 1` child
 * holding the 4-second hold), and it lost the hold entirely until the mode travelled down. The
 * gate is the parent's own, so stamping it states what the nesting already meant, and
 * `composeGates` drops the repeat where a walk composes both.
 */
function _gateOne(effect, inheritedMode) {
  const retuned = _retuneOne(effect, inheritedMode);
  if (retuned !== undefined) return retuned;
  // The variant whose filter NAMES this group's tag, when one does. A group inside a gated
  // subtree that no filter names (an ammo branch's Sentinel crit rider) still belongs to that
  // variant, but its chance is not the filter's to move.
  const loaded = (effect.tags || [])
    .map((tag) => ACTIVE?.enable.get(tag) ?? GLOBAL?.enable.get(tag))
    .filter(Boolean);
  if (loaded.length > 1) {
    console.warn(
      `[variant-modes] a group wears tags of ${loaded.length} variants `
      + `(${loaded.map((s) => s.mode).join(', ')}) — left ungated`);
    effect._variantGate = null;
    return null;
  }
  const selector = loaded[0];
  // Only a group the filter takes from never-fires to certain is a variant's own switch. One
  // already at `chance: 1` wearing the same tag is that variant's INNER group (Suppressive Fire's
  // Cryo hold), reached by inheritance rather than by its own chance.
  const switchesHere = selector && effect.chance === 0;
  const mode = switchesHere ? selector.mode : inheritedMode;
  if (!mode) return null;
  if (effect._variantGate !== undefined) return effect._variantGate || null;

  effect._variantGate = mode;
  effect.requires_expression = composeGates(effect.requires_expression, modeGate(mode));
  // The verdict `_gate-default.cjs` routes on, stated for a gate the parser never saw because it
  // is not in the binary. `UNSATISFIED` is what that parser exports for every one of the 279
  // authored `k<Mode> Source.Mode?` groups: no mode is live in the default situation. Composing it
  // onto a group that carried its own INDETERMINATE gate (an ammo variant's crit rider) is the
  // same answer — a conjunction with a definitely-false clause is definitely false.
  //
  // An archetype FORK is the one verdict left standing (COND-12; no COND-9 tag pairs with a
  // non-empty fork). `requires_default: INDETERMINATE_ARCHETYPE` plus `requires_archetypes` is
  // the only carrier of the class list, and overwriting it would flatten a fork the atoms still
  // need — Rebirth's flight rows are the same row written once per side of the Kheldian split,
  // and the engine resolves that per build (`caster_archetypes`, AT-FORK-1). Leaving the verdict
  // alone does not leak the group into base: every base collector skips a gate that mentions
  // `Source.Mode?` before it consults the verdict, which is the same routing every authored
  // mode-gated fork already takes.
  if (!(effect.requires_archetypes || []).length) {
    effect.requires_default = DEFAULT_VERDICT_GATED;
  }
  // While the mode is live the filter's delta is in force, so the group the filter names fires:
  // `0 + 1`. Left alone where the filter names nothing, and where the group already fires.
  if (selector && effect.chance === 0) {
    effect.chance = Math.min(1, effect.chance + ENABLE_DELTA);
    // An INSTANT template's tick chance at zero under a zero group is the same chance written a
    // second time — Parse6 has one field and the parser emits it into both slots (the Rebirth
    // and Thunderspy ammo rounds) — so it moves with the group. Left at zero, the loaded round
    // read as dormant: the reader treats a zero on either copy as never firing.
    for (const template of effect.templates || []) {
      if (!((template.application_period || 0) > 0) && template.tick_chance === 0) {
        template.tick_chance = effect.chance;
      }
    }
  }
  return mode;
}

/**
 * Retune one group a variant mode raises by less than a full point ([`retuneMap`]), returning the
 * mode in force for its children — or `undefined` where the group is not a retune carrier, so
 * [`_gateOne`] reads it as before.
 *
 * Only a chance the export leaves at ZERO is moved, because that is the rider which fires under
 * the mode and never otherwise: `0 + delta` while the mode runs, so the group is gated on the mode
 * and the zero becomes the delta. Which chance is the zero depends on the group. A ticking
 * template at `tick_chance: 0` under a group that already fires is a per-tick roll the filter
 * sets (Homecoming's burn); a group at `chance: 0` is the application roll; where both are zero
 * and every template ticks, the fork has one field written twice (Parse6), and both copies move
 * so they still read as one roll.
 */
function _retuneOne(effect, inheritedMode) {
  const hits = (effect.tags || [])
    .map((tag) => ACTIVE?.retune?.get(tag) ?? GLOBAL?.retune?.get(tag))
    .filter(Boolean);
  if (hits.length !== 1) return undefined;
  const { mode, delta } = hits[0];
  if (BLOCKED_RETUNES?.has(mode)) return undefined;
  if (inheritedMode && inheritedMode !== mode) return undefined;
  const templates = effect.templates || [];
  const ticking = templates.filter((t) => (t.application_period || 0) > 0);
  const tickZero = ticking.filter((t) => t.tick_chance === 0);
  const groupZero = effect.chance === 0;
  if (!groupZero && !tickZero.length) return undefined;
  if (effect._variantGate !== undefined) return effect._variantGate || null;

  effect._variantGate = mode;
  effect.requires_expression = composeGates(effect.requires_expression, modeGate(mode));
  if (!(effect.requires_archetypes || []).length) {
    effect.requires_default = DEFAULT_VERDICT_GATED;
  }
  const sameField = groupZero && templates.length > 0
    && tickZero.length === templates.length;
  for (const template of tickZero) template.tick_chance = delta;
  if (groupZero && (sameField || !tickZero.length)) effect.chance = delta;
  return mode;
}

/**
 * The gate to compose onto an atom of a DEFAULT-variant group — the standard rounds a Dual
 * Pistols attack fires until an ammo is loaded — or `null` for every other atom.
 *
 * Stamped on the atom rather than written into the group for the reason in
 * [`applyVariantGates`], and stamped at all because the alternative is silence: the group's
 * `chance: 1` says "always" and only the ammo toggles' `-1.0` filter says otherwise, so a
 * reader that honours gates (the damage projection) would keep counting standard rounds' lethal
 * hit and its `-Def` with Cryo Ammo loaded.
 */
function defaultVariantGate(tags, chance) {
  if ((!ACTIVE && !GLOBAL) || chance !== 1) return null;
  const gates = (tags || [])
    .map((tag) => ACTIVE?.defaults.get(tag) ?? GLOBAL?.defaults.get(tag))
    .filter(Boolean);
  if (!gates.length) return null;
  return gates.length === 1 ? gates[0] : composeGates(...gates);
}

/**
 * The `conditionalEffects` entry that NAMES the default variant, for a power that carries one —
 * or `null` where the set has no variant mechanic, no named parent, or this power no default row.
 *
 * Payload-less on purpose, and that is the whole statement: the default variant's effects are the
 * power's BASE (standard rounds are what a Dual Pistols attack fires until an ammo is loaded), so
 * an entry carrying them again would double them. What the entry is for is the NAME —
 * `coh_data::caster_state` labels a stance group's cleared state from the conditional whose id
 * folds to a mode the parent publishes, and without one a build with no ammo loaded reads "None".
 * Its `defaultActive: true` says the same thing to the display resolver: this is the form you are
 * in until you choose otherwise.
 */
function defaultVariantConditional(templates) {
  if (!ACTIVE?.defaultVariant) return null;
  const carries = (templates || []).some((t) => defaultVariantGate(t._groupTags, t._groupChance));
  if (!carries) return null;
  return {
    id: ACTIVE.defaultVariant.id,
    label: splitModeToken(ACTIVE.defaultVariant.mode),
    scope: 'global',
    defaultActive: true,
  };
}

/**
 * A mode token as a label — the same reading `coh_data::caster_state::ModeToggle` gives one and
 * the same one the conditional classifier gives `kIceAmmo` ("Ice Ammo"), so the default variant is
 * named in the same voice as the alternates beside it.
 */
function splitModeToken(mode) {
  return mode
    .replace(/_/g, ' ')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .trim();
}

module.exports = {
  detectVariantModes,
  defaultVariantConditional,
  installVariantModes,
  installGlobalVariantModes,
  globalMoverModes,
  globalConditionalLabel,
  activeVariantModes,
  applyVariantGates,
  defaultVariantGate,
  // Exported for the guards: the shape rules are the claim, so a test states them directly.
  selectorOf,
  retuneMap,
  noModeGate,
  modeGate,
  ENABLE_DELTA,
  DISABLE_DELTA,
};
