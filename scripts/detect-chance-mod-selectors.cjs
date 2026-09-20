'use strict';

/**
 * Corpus-wide chance-mod selectors — the caster states the game publishes as a
 * `Global_Chance_Mod` tag with no `Set_Mode` naming them and no shared parent power
 * (DATA-GAP-REGISTER COND-12).
 *
 * The engine never gates a tagged group on a mode. `ChanceModAccumulate` sums, per tag, the
 * chance mods of everything the caster is running, and a group fires when its own chance plus
 * that sum reaches a roll — so "Fly is on" reaches Combat Jumping's ground package as a `-1.0`
 * on `FeetCantTouchTheGround` from a power in another pool entirely. COND-9's per-powerset pass
 * reads the same mechanic where a selector also publishes a mode (Swap Ammo), and borrows that
 * mode as the gate vocabulary; the families here publish no usable mode, so the gate has to be
 * minted, and the only honest token to mint it from is the tag itself: each mover's emitted
 * `setsModes` gains the tag, and the carriers' groups gain `k<Tag> Source.Mode?` (or its
 * negation), which the engine side already resolves from what the build is running
 * (`coh_math::gather::collect_source_modes`). No new vocabulary anywhere downstream.
 *
 * The shape, every clause measured over all three exports (the sweep this file writes):
 *
 *   1. a MOVER of tag T is a power whose `Global_Chance_Mod` moves T by a full point in a
 *      PvE-reachable group — below ±1 a mod retunes a probability rather than switching a
 *      variant, the same thresholds COND-9 reads;
 *   2. every mover of T is a **Toggle**. A Click's mod is a timed rider (Fiery Embrace, Fusion,
 *      Blood Thirst — the families COND-9 declines by design), and an Auto's is a constant that
 *      never switches (Domination_Rage's standing `-1`), so neither states a caster CHOICE the
 *      planner can offer. Read over the movers a build can HOLD, where those exist (COND-10):
 *      a power in a category no archetype picks and with no `auto_issue` grant is running for
 *      nobody, so its type states nothing about whether the tag switches. The Parse6 forks'
 *      ammo toggles are the measured case — Rebirth's three Dual Pistols toggles and
 *      Thunderspy's three `Inherent` ones are Toggles requiring their set's `Swap Ammo`, and
 *      the only Autos wearing the same tags are `Temporary_Powers` copies in no bundle on any
 *      fork (the family `convert-inherents.cjs` omits by design). Where a tag has no holdable
 *      mover at all the full list is read instead, so this can only ever RESCUE a family, never
 *      decline one: every clause-2 decline the sweep already recorded still declines;
 *   3. T moves in ONE direction. A tag both raised and cut is a swap-among-variants family
 *      (the Staff forms trading the three Perfections), which is COND-9's shape and is left to
 *      its per-powerset rule — measured, every such family's carriers already spell their own
 *      `ownPower?` gates;
 *   4. T is not claimed by a per-powerset COND-9 selector (`selectorOf`) — one mechanic, one
 *      owner, so Granite Armor's mode-published pair stays with the pass that reads its mode;
 *   5. T collides with no mode token the dataset publishes anywhere (`Set_Mode` names,
 *      `modes_suspended` / `modes_required` / `modes_disallowed`), because the minted gate
 *      would otherwise answer to the real mode's publishers too;
 *   6. some carrier is not the mover itself. A mover cutting only its own rows (Spirit Ward's
 *      `initAbsorb`, Geode's `GrantThermalBoost`) is first-application timing — the row fires
 *      before the power's own mod registers — not a cross-power caster state, and gating it on
 *      "the toggle is running" would delete a row the game grants on every cast;
 *   7. a stampable carrier exists: `chance: 0` groups wearing a raised T, `chance: 1` groups
 *      wearing a cut T, holding at least one template — a group with no templates ships no
 *      atoms, so there is nothing for a gate to move (the Stance inherent's `TravelStance`
 *      row is exactly that: the travel toggles raise the tag, and what it switches is a
 *      payload-less costume-FX stance the planner has no number for). A carrier whose own
 *      gate already reads a mode EVERY mover publishes is not stampable — the gate already
 *      states the mover condition (Sonic Melee's `SMAttune` rows all carry
 *      `kSonicAtuneSelf Source.Mode?`, published by Attune itself), and a tag that mixes
 *      stampable and already-stated carriers is declined outright rather than half-stamped
 *      (none does, and the decline is what keeps that measured). A `Boost` is not a carrier:
 *      an enhancement's rows reach a character through slotting, not through the caster
 *      running the power, and the crafted damage-resistance enhancements tag one group per
 *      damage type (`Smashing`, `Lethal`, `Fire`, …) — `pchName` used as a plain label, which
 *      collides by spelling with the `Lethal` the ammo toggles genuinely cut (COND-10). No gate
 *      is at stake: a `Boost` is never converted into a bundle power, so nothing would be
 *      stamped on one. What the exclusion protects is this clause — a tag whose only carriers
 *      were enhancement labels would satisfy "a stampable carrier exists" and select a family
 *      with nothing to stamp. Measured: no family this pass selects on any fork holds a `Boost`
 *      carrier, and `Lethal` on the two Parse6 forks is the population that would.
 *
 * The CLI writes the whole sweep — selected AND declined, with reasons — to
 * `src/data/datasets/<id>/chance-mod-selectors.json`. That artifact is both the converters'
 * input (loaded once per process, corpus scan amortised) and the recorded census the exit
 * condition requires: what the rule selects is a committed, diffable fact, not a comment.
 */

const fs = require('fs');
const path = require('path');
const { selectorOf, ENABLE_DELTA, DISABLE_DELTA } = require('./_variant-modes.cjs');
const { gateText } = require('./_gate-tokens.cjs');

/** The attrib whose params retune tagged groups, and the mode-publishing attrib. */
const FILTER_ATTRIB = 'global_chance_mod';
const MODE_ATTRIB = 'set_mode';

function walkGroups(effects, visit) {
  for (const effect of effects || []) {
    if (!effect) continue;
    visit(effect);
    walkGroups(effect.child_effects, visit);
  }
}

function attribOf(template) {
  const attribs = template.attribs || [];
  return attribs.length === 1 ? String(attribs[0]).toLowerCase() : null;
}

/** The filter names of a GCM row, whichever params arm the fork spells them in (COND-9). */
function filterNames(params) {
  if (!params) return [];
  if (params.type === 'EffectFilter') return params.tags || [];
  return params.power_names || [];
}

/** Every power-shaped JSON under `root`, skipping per-set `index.json` files. */
function* powerFiles(root, exclude = new Set()) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (exclude.has(entry.name)) continue;
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) yield* powerFiles(full);
    else if (entry.name.endsWith('.json') && entry.name !== 'index.json') yield full;
  }
}

/**
 * The sweep: read every power once, then judge every moved tag against the seven clauses.
 * Returns `{ selected, declined }`, both sorted by tag for a stable artifact.
 */
function detectChanceModSelectors(rawRoot, exclude = new Set()) {
  const byDir = new Map();
  const powers = [];

  // Clause 2's holdable set. An archetype names the categories it picks from, and the
  // top directory of a power file IS its category, so a power outside every named
  // category and without an `auto_issue` grant reaches no build through either channel
  // the planner ships. Deliberately generous: powers that arrive by slotting or a
  // redirect (`Incarnate`, `Set_Bonus`, `Redirects`, `Pets`) count as holdable here, so
  // their clause-2 verdicts and the reasons recorded with them stay exactly as measured.
  //
  // The class tables live in `tables/`, and this read used to scan the export
  // ROOT instead. That worked only by accident: three of the four trees carry
  // fossil copies of `tables/` at their root, left there before the
  // subdirectory existed. Brainstorm was exported after, has no fossils, and so
  // built this set from nothing — 0 archetypes instead of 71, every power
  // non-holdable, and the clause-2 verdicts it shipped were measured against an
  // empty category set. Read the tree the classes exporter actually writes, as
  // the sibling detector already does (detect-caster-meters.cjs).
  const atCategories = new Set();
  const tablesDir = path.join(rawRoot, 'tables');
  for (const entry of fs.readdirSync(tablesDir, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith('.json')) continue;
    let archetype;
    try {
      archetype = JSON.parse(fs.readFileSync(path.join(tablesDir, entry.name), 'utf-8'));
    } catch {
      continue;
    }
    if (!archetype || !archetype.primary_category) continue;
    for (const key of ['primary_category', 'secondary_category', 'pool_category', 'epic_pool_category']) {
      if (archetype[key]) atCategories.add(String(archetype[key]).toLowerCase());
    }
  }

  const holdableOf = new Map();
  for (const file of powerFiles(rawRoot, exclude)) {
    let json;
    try {
      json = JSON.parse(fs.readFileSync(file, 'utf-8'));
    } catch {
      continue;
    }
    if (!json || !Array.isArray(json.effects)) continue;
    const category = path.relative(rawRoot, file).split(path.sep)[0].toLowerCase();
    holdableOf.set(json, atCategories.has(category) || json.auto_issue === true);
    powers.push(json);
    const dir = path.dirname(file);
    if (!byDir.has(dir)) byDir.set(dir, []);
    byDir.get(dir).push(json);
  }

  // Clause 4's claim set: every tag a per-powerset COND-9 selector loads or displaces.
  const cond9Claimed = new Set();
  for (const jsons of byDir.values()) {
    const swallowed = [];
    for (const json of jsons) {
      const selector = selectorOf(json, swallowed);
      if (selector) for (const tag of [...selector.enables, ...selector.disables]) cond9Claimed.add(tag);
    }
  }

  const movers = new Map(); // tag -> [{power, type, dir, modes}]
  const modeTokens = new Set(); // clause 5's namespace
  for (const json of powers) {
    const name = json.full_name || json.name;
    const type = String(json.type || '?');
    const deltas = new Map();
    const published = [];
    walkGroups(json.effects, (effect) => {
      for (const template of effect.templates || []) {
        const attrib = attribOf(template);
        if (attrib === MODE_ATTRIB && template.mode_name) {
          published.push(template.mode_name);
          modeTokens.add(template.mode_name);
        }
        if (attrib !== FILTER_ATTRIB || typeof template.scale !== 'number') continue;
        if (effect.is_pvp === 'PVP_ONLY') continue; // PvE planner default, as assignModes reads modes
        for (const tag of filterNames(template.params)) {
          const seen = deltas.get(tag);
          if (seen === undefined || Math.abs(template.scale) > Math.abs(seen)) deltas.set(tag, template.scale);
        }
      }
    });
    for (const key of ['modes_suspended', 'modes_required', 'modes_disallowed']) {
      for (const mode of json[key] || []) modeTokens.add(mode);
    }
    for (const [tag, delta] of deltas) {
      if (delta < ENABLE_DELTA && delta > DISABLE_DELTA) continue;
      if (!movers.has(tag)) movers.set(tag, []);
      movers.get(tag).push({
        power: name,
        type,
        holdable: holdableOf.get(json) === true,
        dir: delta >= ENABLE_DELTA ? 'raise' : 'cut',
        modes: published,
      });
    }
  }

  const selected = [];
  const declined = [];
  for (const [tag, moverList] of movers) {
    const directions = new Set(moverList.map((m) => m.dir));
    const direction = directions.size === 1 ? [...directions][0] : null;
    const entry = {
      tag,
      dir: [...directions].sort().join('+'),
      movers: moverList.map((m) => m.power).sort(),
    };
    const decline = (reason) => declined.push({ ...entry, reason });
    if (cond9Claimed.has(tag)) {
      decline('claimed by a per-powerset COND-9 selector');
      continue;
    }
    // Judged over the movers a build can hold; where none is holdable the whole list is
    // read, so a tag can never be declined for want of a mover it was already declined on.
    const holdable = moverList.filter((m) => m.holdable);
    const judged = holdable.length ? holdable : moverList;
    if (!judged.every((m) => m.type === 'Toggle')) {
      decline(`mover types [${[...new Set(judged.map((m) => m.type))].sort().join(', ')}] — not all Toggle`);
      continue;
    }
    if (!direction) {
      decline('moved in both directions — a swap-among-variants family (COND-9\'s shape)');
      continue;
    }
    if (modeTokens.has(tag)) {
      decline('collides with a published mode token');
      continue;
    }

    // Clauses 6 and 7: the carriers.
    const sharedModes = moverList
      .map((m) => new Set(m.modes))
      .reduce((acc, set) => new Set([...acc].filter((mode) => set.has(mode))));
    const moverNames = new Set(entry.movers.map((n) => n.toLowerCase()));
    const wantChance = direction === 'raise' ? 0 : 1;
    const carriers = new Map(); // power -> row count
    let alreadyStated = 0;
    let payloadless = 0;
    for (const json of powers) {
      // An enhancement is not a carrier: its rows reach a character by being slotted, not by
      // the caster running it, and it labels its own groups with the damage-type vocabulary
      // (`Lethal`, `Fire`, …) that a real mover's tag collides with by spelling. Nothing is
      // stamped on a Boost either way — this keeps clause 7 from counting a label as a
      // carrier and selecting a family with nothing to gate (COND-10).
      if (String(json.type || '') === 'Boost') continue;
      const name = json.full_name || json.name;
      walkGroups(json.effects, (effect) => {
        if (!(effect.tags || []).includes(tag)) return;
        const chance = effect.chance == null && wantChance === 1 ? 1 : effect.chance;
        if (chance !== wantChance) return;
        // No atoms anywhere under it — nothing for a gate to move. Deep, because a switching
        // parent may hold its payload in children (Suppressive Fire's Cryo branch shape);
        // `Null` templates are the costume-FX no-ops the ingest drops, so they are not
        // payload (the Stance inherent's `TravelStance` subtree is entirely made of them).
        let payload = 0;
        walkGroups([effect], (sub) => {
          payload += (sub.templates || []).filter(
            (t) => (t.attribs || []).some((a) => String(a).toLowerCase() !== 'null')).length;
        });
        if (!payload) {
          payloadless += 1;
          return;
        }
        const gate = gateText(effect.requires_expression);
        if ([...sharedModes].some((mode) => gate.includes(`k${mode}`))) {
          alreadyStated += 1;
          return;
        }
        carriers.set(name, (carriers.get(name) || 0) + 1);
      });
    }
    entry.carriers = [...carriers.keys()].sort();
    entry.rows = [...carriers.values()].reduce((a, b) => a + b, 0);
    if (alreadyStated && entry.rows) {
      decline(`mixed carriers — ${alreadyStated} rows already state a shared mover mode, ${entry.rows} do not`);
      continue;
    }
    if (alreadyStated) {
      decline(`every carrier row already states a shared mover mode (${alreadyStated} rows)`);
      continue;
    }
    if (!entry.rows) {
      // A carrier population that is all costume-FX no-ops IS a family shape worth
      // recording (the Stance inherent); a moved tag nothing wears at the switching
      // chance is not — COND-9's own detector treats that silence the same way.
      if (payloadless) decline(`every carrier row is payload-less FX (${payloadless} rows)`);
      continue;
    }
    if (entry.carriers.every((carrier) => moverNames.has(carrier.toLowerCase()))) {
      decline('self-referential — every carrier is a mover of the same tag (first-application timing)');
      continue;
    }
    selected.push(entry);
  }
  selected.sort((a, b) => a.tag.localeCompare(b.tag));
  declined.sort((a, b) => a.tag.localeCompare(b.tag));
  return { selected, declined };
}

module.exports = { detectChanceModSelectors };

if (require.main === module) {
  const { parseDatasetArg, datasetPath } = require('./_dataset-paths.cjs');
  const datasetId = parseDatasetArg();
  const base = path.join(__dirname, '../exported_powers');
  const isLegacyHc = datasetId === 'homecoming' && !fs.existsSync(path.join(base, datasetId));
  const rawRoot = isLegacyHc ? base : path.join(base, datasetId);
  const exclude = isLegacyHc ? new Set(['rebirth', 'thunderspy']) : new Set();
  const sweep = detectChanceModSelectors(rawRoot, exclude);
  const outPath = datasetPath(datasetId, 'chance-mod-selectors.json');
  fs.writeFileSync(outPath, `${JSON.stringify(sweep, null, 1)}\n`);
  console.log(
    `[chance-mod-selectors] ${datasetId}: ${sweep.selected.length} families selected `
    + `(${sweep.selected.map((s) => s.tag).join(', ')}), ${sweep.declined.length} declined → ${outPath}`);
}
