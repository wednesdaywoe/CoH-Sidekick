/**
 * Archetype-inherent conversion script.
 *
 * Usage: node scripts/convert-archetype-inherents.cjs --dataset=<id>
 *
 * ## The gap this closes
 *
 * A server can put an archetype's signature power in `Inherent.Inherent`
 * instead of in a powerset, auto-issue it, and gate it on the archetype's
 * class. Thunderspy does exactly that with the Stalker's **Hide** and
 * **Placate** — and then reuses the vacated powerset name slots for other
 * powers (`Stalker_Defense.Ninjitsu.Hide` holds Quick Recovery,
 * `Stalker_Melee.Ninja_Sword.Placate` holds The Lotus Drops).
 *
 * Nothing in the planner picked those up. `convert-powerset.cjs` only walks
 * the powerset categories, and the archetype-inherent list in
 * `datasets/homecoming/levels.ts` is hand-written — it covers the two
 * Kheldians and nobody else. So on Thunderspy both powers were reachable
 * from no screen at all: not in the powerset picker (the slots show the
 * repurposed powers under their own names), and not in the inherent list.
 *
 * ## The selection rule
 *
 * Derived, not a name list — a fork that moves another power the same way is
 * picked up with no edit here. A member of `<dataset>/inherent/inherent/` is
 * kept when ALL of:
 *
 *   1. `auto_issue` — the server hands it over; the player never picks it.
 *   2. `requires` gates on player archetype classes ONLY, per that dataset's
 *      own class catalogue (`derivePlayerArchetypes`). This is what keeps out
 *      the NPC classes and the dead legacy variants (`Class_BlasterOLD`'s four
 *      Defiance copies) that also sit in this directory.
 *   3. It is either slottable (`boosts_allowed` non-empty) or a `Toggle`.
 *
 *      An unslottable `Auto` in this directory is one of two things, and the
 *      planner already handles both. Either it is engine bookkeeping —
 *      `Domination_Meter`, `Rage_Dampen`, `Primal_Energy_Meter`,
 *      `Vigilance_PerTeamEndAdjustment` — which shares its archetype
 *      inherent's display name and is not a pick at all. Or it IS the
 *      archetype's headline inherent (Containment, Fury, Gauntlet, Scourge,
 *      …), which reaches the build through the archetype record's own
 *      hand-written `inherent:` field via `createArchetypeInherentPower`.
 *      Emitting those here would double every one of them.
 *
 *      A `Toggle` is neither. It is something the player switches on, so it
 *      has to be visible whether or not it takes slots. Across all three
 *      forks exactly one power qualifies: Thunderspy's Mastermind
 *      `Hold_Ground`, a 60ft toggle that immobilises your henchmen and gives
 *      them knockback protection — the "stay put" pet command. Supremacy
 *      already occupies the Mastermind's single `inherent:` field, so there is
 *      nowhere else for it to go.
 *
 *      `Click` is deliberately NOT in this clause. It isn't a clean signal:
 *      `Domination` is a Click and is already the Dominator's headline
 *      inherent, and `Vigilance_PerTeamEndAdjustment` is a Click on
 *      Thunderspy while being an Auto on Homecoming and Rebirth. Widening to
 *      Click doubles the first and admits the second.
 *   4. No powerset in this dataset already displays that name. Note the key:
 *      DISPLAY name, not internal name. Thunderspy's powerset layer does carry
 *      `internalName: "Hide"` — pointing at Quick Recovery — so an
 *      internal-name check reproduces the very collision this script exists to
 *      see past.
 *   5. It isn't already handed out by `GRANTED_POWER_GROUPS`. The Kheldian
 *      form attacks (`Bright_Nova_Blast`, `Black_Dwarf_Strike`, …) live in
 *      `Inherent.Inherent` too and reach the build through their form toggle;
 *      emitting them here would double every one of them.
 *
 * On the three shipped datasets this keeps 11 powers, all Thunderspy:
 * Stalker Hide + Placate, Mastermind Hold Ground, four Peacebringer travel
 * toggles, four Warshade teleports. Homecoming and Rebirth keep zero — they
 * grant these from powersets, so rule 4 rejects them, which is the answer that
 * makes this emit safe to merge into the shared hand-written list
 * unconditionally.
 *
 * ## Levels
 *
 * `available_level` is read straight through — it is the authority, not a
 * guess. Homecoming's export says 9 for Combat Flight and Shadow Recall (the
 * L10 unlocks) and 0 for Energy Flight and Shadow Step, which is exactly what
 * the hand-written Kheldian list encodes. Thunderspy's says 0 for all ten, so
 * on that fork they really are granted from level 1.
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, datasetPath } = require('./_dataset-paths.cjs');
const { derivePlayerArchetypes } = require('./_player-classes.cjs');
const { convertPower } = require('./convert-powerset.cjs');

const datasetId = parseDatasetArg();

const RAW_DATA_BASE = path.join(__dirname, '../exported_powers');
const RAW_DATA_PATH =
  datasetId === 'homecoming' && !fs.existsSync(path.join(RAW_DATA_BASE, datasetId))
    ? RAW_DATA_BASE
    : path.join(RAW_DATA_BASE, datasetId);

const INHERENT_DIR = path.join(RAW_DATA_PATH, 'inherent', 'inherent');
const TABLES_DIR = path.join(RAW_DATA_PATH, 'tables');
const OUTPUT_FILE = datasetPath(datasetId, 'generated', 'archetype-inherents.ts');

/**
 * Archetype ids this dataset actually registers, so a class stem that maps to
 * no archetype is a loud failure rather than a silently dropped power. Read
 * from the registry's own top-level keys — `archetypes.ts` is generated, so a
 * new archetype flows through without an edit here.
 */
function registeredArchetypeIds() {
  const src = fs.readFileSync(datasetPath(datasetId, 'archetypes.ts'), 'utf-8');
  const body = src.slice(src.indexOf('export const ARCHETYPES'));
  return new Set([...body.matchAll(/^ {2}'?([a-z0-9_-]+)'?: \{/gm)].map((m) => m[1]));
}

/**
 * Archetype id → the inherent NAME it declares, from the same registry source
 * `registeredArchetypeIds` reads. This is the join key for the headline emit
 * below: the registry names the inherent, the export ships the power.
 */
function declaredInherentNames() {
  const src = fs.readFileSync(datasetPath(datasetId, 'archetypes.ts'), 'utf-8');
  const body = src.slice(src.indexOf('export const ARCHETYPES'));
  const out = new Map();
  const re = /^ {2}'?([a-z0-9_-]+)'?: \{\n\s+name: '[^']+',[\s\S]*?inherent: \{\n\s+name: '([^']+)'/gm;
  for (const m of body.matchAll(re)) out.set(m[1], m[2]);
  return out;
}

/** Class stem (`arachnos_soldier`) → archetype id (`arachnos-soldier`). */
function archetypeIdForClass(stem, registered) {
  for (const candidate of [stem, stem.replace(/_/g, '-')]) {
    if (registered.has(candidate)) return candidate;
  }
  return undefined;
}

/** Every display name this dataset's powerset layer already shows. */
function powersetDisplayNames() {
  const root = datasetPath(datasetId, 'generated', 'powersets');
  const names = new Set();
  (function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (!entry.name.endsWith('.ts') || entry.name === 'index.ts') continue;
      const match = fs.readFileSync(full, 'utf-8').match(/"name":\s*"([^"]+)"/);
      if (match) names.add(match[1].toLowerCase());
    }
  })(root);
  return names;
}

/**
 * Every power name `GRANTED_POWER_GROUPS` already routes into a build, parent
 * and child alike. Datasets that re-export Homecoming's groups resolve to
 * Homecoming's — matching what the app actually loads.
 */
function grantedPowerNames(id = datasetId) {
  const src = fs.readFileSync(datasetPath(id, 'granted-powers.ts'), 'utf-8');
  if (id !== 'homecoming' && /from ['"][^'"]*homecoming/.test(src)) {
    return grantedPowerNames('homecoming');
  }
  const names = new Set();
  for (const group of src.matchAll(/grantedPowers:\s*\[([^\]]*)\]/g)) {
    for (const name of group[1].matchAll(/'([^']+)'/g)) names.add(name[1].toLowerCase());
  }
  for (const parent of src.matchAll(/parentPower:\s*'([^']+)'/g)) names.add(parent[1].toLowerCase());
  return names;
}

/**
 * Slot ceiling, reading an explicit `max_boosts: 0` as the zero it is.
 *
 * `convert-powerset.cjs` computes `powerJson.max_boosts || 6`, which folds a
 * stated 0 into the 6-slot default because 0 is falsy. That is wrong for the
 * powers this script emits and wrong in a user-visible way: Thunderspy's
 * Placate states `max_boosts: 0` — it cannot be slotted — while its Hide omits
 * the field and takes the 6-slot default. The bug report that started this work
 * said exactly that ("Hide can be slotted, placate cannot"), so the export and
 * the player agree and only the `||` disagrees.
 *
 * The pool and epic-pool converters already read it correctly
 * (`max_boosts !== undefined && !== null ? max_boosts : 6`); this matches them.
 * The powerset converter's copy is left alone deliberately — 554 Thunderspy
 * powers state a 0 alongside a non-empty `boosts_allowed`, so correcting it
 * there is its own change with its own verification, not a side effect of this
 * one.
 */
function resolveMaxSlots(json, power) {
  if (!power.allowedEnhancements?.length) return 0;
  return json.max_boosts === undefined || json.max_boosts === null ? 6 : json.max_boosts;
}

/**
 * Archetypes whose declared inherent name reaches the right power through no
 * derivable rule. ONE entry, and it is the one canonical's `convert-inherents.cjs`
 * documents at length: the Brute declares "Fury", there is no `Fury` power, and the
 * mechanic is implemented by `Rage_Buff` — an Auto gated `@Class_Brute` whose
 * `display_name` IS "Fury", sitting among four siblings (`Rage`, `Rage_Dampen`,
 * `Rage_Proc_Grant`, `Rage_Strengthen`) that share that display name. No field
 * separates them on every fork: `Rage_Buff`'s damage atoms carry the
 * `kRage source> .02 *` magnitude expression on Homecoming, and Rebirth and
 * Thunderspy drop it.
 *
 * Canonical's table has THREE entries; this has one, because the two `Conditioning`
 * rows resolve here off the export's own `@Class_` gate rather than off a filename.
 * Both Arachnos archetypes declare that name, and `Spider_Conditioning` and
 * `Widow_Conditioning` each gate to exactly one of them.
 */
const HEADLINE_INHERENT_ALIASES = {
  brute: 'Rage_Buff',
};

/**
 * Archetype id → the `Inherent.Inherent` full name of its HEADLINE inherent —
 * Defiance, Fury, Dark Sustenance, the power the archetype is built around.
 *
 * Clause 3 of `selectPowers` rejects every one of these on purpose: they reach the
 * build through the archetype record's own `inherent:` field, so emitting them as
 * roster rows would double them. But the `.mbd` writer has to NAME this power for
 * another program, and the name it needs is the export's, not the
 * `Inherent.<Archetype>.<Name>` that `createArchetypeInherentPower` synthesises.
 * That is MBDEXPORT-20: the writer emitted no archetype inherent at all, on 8 of 8
 * corpus builds. So this emits the NAME and nothing else — no stats, no atoms, which
 * is what keeps it clear of PARTSTAT-2's territory. There is no number here to drift.
 *
 * The rule, in order:
 *   1. auto-issued, gated on player archetype classes only — clauses 1 and 2 above.
 *   2. `display_name` equals the name this archetype DECLARES, and the gate names
 *      this archetype. That alone is ambiguous for 15 of 60 archetype-fork pairs:
 *      every headline inherent sits beside its own meter/dampen/mode bookkeeping,
 *      which shares its display name.
 *   3. prefer the candidate whose internal name IS the declared name with spaces
 *      underscored. That settles 13 of the 15 — `Domination` over `Domination_Meter`
 *      and four more, `Vigilance` over `Vigilance_PerTeamEndAdjustment`,
 *      `Opportunity` over `Opportunity_Meter`.
 *   4. failing that, the alias above.
 *
 * Anything still ambiguous emits NOTHING, and the writer warns rather than guessing.
 * Exactly one archetype lands there — Thunderspy's Primalist, whose fork ships
 * `Primal_Energy_Meter` and `Primal_Energy_Dampen` and no canonical power. That is
 * INHERENT-10, and emitting either half would be the silent wrong answer the row
 * exists to keep visible.
 */
function selectHeadlineInherents() {
  const registered = registeredArchetypeIds();
  const playerClasses = new Set(derivePlayerArchetypes(TABLES_DIR));
  const declaredNames = declaredInherentNames();

  const candidates = new Map(); // archetype id → raw json[]
  for (const file of fs
    .readdirSync(INHERENT_DIR)
    .filter((f) => f.endsWith('.json') && f !== 'index.json')
    .sort()) {
    const json = JSON.parse(fs.readFileSync(path.join(INHERENT_DIR, file), 'utf-8'));
    if (!json.auto_issue) continue; // 1
    const classes = [...String(json.requires || '').matchAll(/@Class_([A-Za-z0-9_-]+)/g)].map(
      (m) => m[1].toLowerCase(),
    );
    if (!classes.length || !classes.every((c) => playerClasses.has(c))) continue; // 2
    for (const stem of classes) {
      const archetypeId = archetypeIdForClass(stem, registered);
      if (!archetypeId) continue; // selectPowers throws on this; it is not this emit's call
      if (json.display_name !== declaredNames.get(archetypeId)) continue; // 2
      if (!candidates.has(archetypeId)) candidates.set(archetypeId, []);
      candidates.get(archetypeId).push(json);
    }
  }

  const resolved = new Map();
  const ambiguous = [];
  for (const [archetypeId, rows] of candidates) {
    let pick = rows.length === 1 ? rows[0] : undefined;
    if (!pick) {
      const exact = String(declaredNames.get(archetypeId)).replace(/\s+/g, '_'); // 3
      const byName = rows.filter((r) => r.name === exact);
      pick = byName.length === 1 ? byName[0] : undefined;
    }
    if (!pick && HEADLINE_INHERENT_ALIASES[archetypeId]) {
      const aliased = rows.filter((r) => r.name === HEADLINE_INHERENT_ALIASES[archetypeId]); // 4
      pick = aliased.length === 1 ? aliased[0] : undefined;
    }
    if (!pick) {
      ambiguous.push(`${archetypeId}/${declaredNames.get(archetypeId)} → ${rows.map((r) => r.name).join(', ')}`);
      continue;
    }
    resolved.set(archetypeId, pick.full_name);
  }
  // Declared but never a candidate at all — the fork ships no power by that name.
  for (const [archetypeId, name] of declaredNames) {
    if (!candidates.has(archetypeId)) ambiguous.push(`${archetypeId}/${name} → (no power)`);
  }
  return { resolved, ambiguous };
}

function selectPowers() {
  if (!fs.existsSync(INHERENT_DIR)) {
    throw new Error(
      `No inherent export at ${INHERENT_DIR}. Re-run the bin-crawler for "${datasetId}" ` +
        'before this converter — an empty emit here would silently un-grant every power ' +
        'this file owns.',
    );
  }
  const registered = registeredArchetypeIds();
  const playerClasses = new Set(derivePlayerArchetypes(TABLES_DIR));
  const alreadyShown = powersetDisplayNames();
  const alreadyGranted = grantedPowerNames();

  const byArchetype = new Map();
  const files = fs
    .readdirSync(INHERENT_DIR)
    .filter((f) => f.endsWith('.json') && f !== 'index.json')
    .sort();

  for (const file of files) {
    const json = JSON.parse(fs.readFileSync(path.join(INHERENT_DIR, file), 'utf-8'));

    if (!json.auto_issue) continue; // 1
    const classes = [...String(json.requires || '').matchAll(/@Class_([A-Za-z0-9_-]+)/g)].map(
      (m) => m[1].toLowerCase(),
    );
    if (!classes.length || !classes.every((c) => playerClasses.has(c))) continue; // 2
    const slottable = Array.isArray(json.boosts_allowed) && json.boosts_allowed.length > 0;
    if (!slottable && json.type !== 'Toggle') continue; // 3
    if (alreadyShown.has(String(json.display_name).toLowerCase())) continue; // 4
    if (alreadyGranted.has(String(json.name).toLowerCase())) continue; // 5

    for (const stem of classes) {
      const archetypeId = archetypeIdForClass(stem, registered);
      if (!archetypeId) {
        throw new Error(
          `${file} gates on player class "${stem}", which matches no archetype in ` +
            `${datasetId}'s registry. Reconcile the class catalogue and the archetype ` +
            'registry before emitting — dropping it would hide the power silently.',
        );
      }
      const power = convertPower(json, json.available_level, archetypeId, 'inherent');
      // Auto-issued and un-removable, and grouped under the expanded
      // "<AT> Inherent" heading rather than the collapsed Basic one — an
      // archetype power the server hands you is not discoverable down there.
      power.isLocked = true;
      power.category = 'archetype';
      power.fullName = json.full_name;
      power.maxSlots = resolveMaxSlots(json, power);
      if (!byArchetype.has(archetypeId)) byArchetype.set(archetypeId, []);
      byArchetype.get(archetypeId).push(power);
    }
  }

  for (const powers of byArchetype.values()) {
    powers.sort((a, b) => a.available - b.available || a.internalName.localeCompare(b.internalName));
  }
  return byArchetype;
}

function emit(byArchetype, headline) {
  const entries = [...byArchetype.entries()].sort(([a], [b]) => a.localeCompare(b));
  const total = entries.reduce((n, [, powers]) => n + powers.length, 0);

  const body = entries
    .map(([archetypeId, powers]) => `  '${archetypeId}': ${JSON.stringify(powers, null, 2).replace(/\n/g, '\n  ')},`)
    .join('\n');

  const summary = entries.length
    ? entries.map(([id, powers]) => ` *   ${id}: ${powers.map((p) => p.name).join(', ')}`).join('\n')
    : ' *   (none — every archetype inherent on this server is already reachable)';

  const headlineEntries = [...headline.resolved.entries()].sort(([a], [b]) => a.localeCompare(b));
  const headlineBody = headlineEntries.map(([id, full]) => `  '${id}': '${full}',`).join('\n');
  const headlineSummary = [
    `${headlineEntries.length} archetype(s) named`,
    ...(headline.ambiguous.length
      ? [` *`, ` * Unresolved, and left unnamed on purpose — the writer warns rather than guessing:`,
         ...headline.ambiguous.map((a) => ` *   ${a}`)]
      : []),
  ].join('\n');

  const content = `/**
 * Archetype inherents — GENERATED LAYER
 * AUTO-GENERATED by \`node scripts/convert-archetype-inherents.cjs --dataset=${datasetId}\`.
 * Do not hand-edit.
 *
 * Auto-issued, archetype-gated powers that live in this server's
 * \`Inherent.Inherent\` set and are reachable from nowhere else — not from a
 * powerset, not from a granted-power group. See the converter's header for the
 * full selection rule and why each clause is there.
 *
 * ${total} power(s):
${summary}
 */

import type { InherentPowerDef } from '@/data/datasets/homecoming/levels';

export const GENERATED_ARCHETYPE_INHERENTS: Record<string, InherentPowerDef[]> = {
${body}
};

/**
 * Archetype id → the \`Inherent.Inherent\` full name of its HEADLINE inherent, for
 * the \`.mbd\` writer, which has to name this power for Mids (MBDEXPORT-20). The
 * NAME only — no stats and no atoms, so there is nothing here that can drift into a
 * wrong number. See the converter's \`selectHeadlineInherents\` for the rule.
 *
 * ${headlineSummary}
 */
export const HEADLINE_ARCHETYPE_INHERENTS: Record<string, string> = {
${headlineBody}
};
`;

  fs.mkdirSync(path.dirname(OUTPUT_FILE), { recursive: true });
  fs.writeFileSync(OUTPUT_FILE, content);
  console.log(`Wrote ${OUTPUT_FILE} — ${total} power(s) across ${entries.length} archetype(s)`);
  console.log(`  headline inherents named: ${headlineEntries.length}`);
  for (const a of headline.ambiguous) console.log(`  UNNAMED ${a}`);
  for (const [id, powers] of entries) {
    console.log(`  ${id}: ${powers.map((p) => `${p.name} (L${p.available + 1})`).join(', ')}`);
  }
}

if (require.main === module) {
  emit(selectPowers(), selectHeadlineInherents());
}

module.exports = { selectPowers, selectHeadlineInherents };
