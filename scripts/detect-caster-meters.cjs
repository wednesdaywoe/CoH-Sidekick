'use strict';

/**
 * Per-archetype caster meters — the attributes a dataset reuses as one class's
 * resource bar (DATA-GAP-REGISTER COND-13).
 *
 * The game overloads attribute slots. `Stealth` is hide for most of the roster,
 * and on both Parse6 forks it is also the Dominator's Domination bar, because
 * `Inherent.Inherent.Domination` writes it. A gate reading `kStealth source>`
 * therefore means two different things depending on who cast the power, and the
 * converter used to tell them apart by testing whether the powerset path started
 * with `dominator_` — a proper noun in a conditional, and a wrong one for the
 * pool powers that carry the same bonus from `Pool.Fighting`.
 *
 * What the data states instead: a power that every member of one archetype is
 * GIVEN, and that writes the attribute, is that archetype's meter for it. Both
 * halves are needed. Without the grant the power is something a build might not
 * hold, so its attribute says nothing about the class; without the archetype
 * restriction the attribute is the roster's, which is what hide is.
 *
 * Measured, this selects exactly one family per fork and none on Homecoming:
 * HC moved the bar to `Meter` (`Domination_Meter` writes it) and left the older
 * `kStealth` spelling on one pool power, so on HC a Dominator's kStealth really
 * is hide and the honest label is Stealthed. That asymmetry is the point of
 * reading it per dataset rather than naming the mechanic in code.
 *
 * The clauses, each recorded on the declined side when it turns a power away:
 *
 *   1. `auto_issue` — the grant. A power a build picks is a choice, not a class
 *      property, so its attribute cannot stand for the class.
 *   2. `requires` names archetypes, in the `$archetype @Class_X ==` form the
 *      export writes (`||`-chained for a shared meter). A power granted to
 *      everyone states nothing to disambiguate.
 *   3. It writes at least one attribute to Self. A meter you cannot fill is not
 *      a meter, and a target-side write is a debuff on someone else.
 *   4. Some gate in the corpus reads the attribute as `k<Attrib> source>`. With
 *      no reader there is nothing for the name to disambiguate, and selecting it
 *      anyway would let a future gate inherit a name nothing measured.
 *   5. ONE power claims the attribute. Where several do, the attribute is a
 *      shared engine slot rather than one class's bar and no single name is the
 *      honest one for a gate that reads it. This is what turns away the bulk of
 *      the candidates on every dataset: `Rage` is claimed by seven inherents on
 *      Homecoming (Domination's four rows plus Rage's three), and the mez slots
 *      by every inherent that grants status protection. Being claimed twice is
 *      also a real answer for the family this exists for, since a second
 *      Stealth-writing inherent would mean the fork's gate is ambiguous too.
 *
 * `categories` is the other half the converter needs. A gate inside the class's
 * own powersets doesn't restate the archetype — only a Dominator holds
 * `Dominator_Control` — so the powerset category stands in for the fork. Only
 * categories used SOLELY by the meter's archetypes qualify; `Pool` is every
 * class's, which is why the pool powers must carry the fork on the gate itself
 * and do.
 */

const fs = require('node:fs');
const path = require('node:path');

const { derivePlayerArchetypes } = require('./_player-classes.cjs');

// `$archetype @Class_X ==` / `!=`, the RPN the export writes an archetype
// restriction in. Read as a flat token list because the corpus chains several
// with `||` for a meter more than one class holds.
function requiredArchetypes(requires) {
  if (!Array.isArray(requires) || !requires.length) return [];
  const named = new Set();
  for (let i = 0; i + 2 < requires.length; i++) {
    if (requires[i] !== '$archetype') continue;
    if (requires[i + 2] !== '==') continue;
    const token = requires[i + 1];
    if (typeof token === 'string' && token.startsWith('@Class_')) named.add(token.slice(1));
  }
  return [...named];
}

// Attributes a power's templates write to the caster, over the whole group tree.
function selfWrittenAttribs(groups, out = new Set()) {
  if (!Array.isArray(groups)) return out;
  for (const group of groups) {
    if (!group || typeof group !== 'object') continue;
    for (const template of group.templates || []) {
      if (!template || template.target !== 'Self') continue;
      for (const attrib of template.attribs || []) out.add(attrib);
    }
    selfWrittenAttribs(group.child_effects, out);
  }
  return out;
}

// Gate tokens of the form `k<Attrib> source>`, over the whole group tree. The
// attribute name is returned unprefixed so it compares against a template's.
function gateReadAttribs(groups, out = new Set()) {
  if (!Array.isArray(groups)) return out;
  for (const group of groups) {
    if (!group || typeof group !== 'object') continue;
    const tokens = group.requires_expression;
    if (Array.isArray(tokens)) {
      for (let i = 0; i + 1 < tokens.length; i++) {
        const attrib = typeof tokens[i] === 'string' && tokens[i].match(/^k([A-Za-z][A-Za-z0-9_]*)$/);
        if (attrib && /^source(\.owner)?>$/i.test(String(tokens[i + 1]))) out.add(attrib[1]);
      }
    }
    gateReadAttribs(group.child_effects, out);
  }
  return out;
}

function* powerFiles(root, exclude) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) {
      if (exclude.has(entry.name)) continue;
      yield* powerFiles(full, exclude);
    } else if (entry.name.endsWith('.json')) {
      yield full;
    }
  }
}

// Title-case-ish label from an attribute or power name: `Domination` stays,
// `Assassins_Focus` becomes `Assassins Focus`.
function labelOf(name) {
  return name.replace(/_/g, ' ').trim();
}

function detectCasterMeters(rawRoot, exclude) {
  const archetypes = [];
  const tablesDir = path.join(rawRoot, 'tables');
  for (const stem of derivePlayerArchetypes(tablesDir)) {
    const record = JSON.parse(fs.readFileSync(path.join(tablesDir, `${stem}.json`), 'utf-8'));
    archetypes.push({
      token: record.name,
      categories: [record.primary_category, record.secondary_category,
        record.pool_category, record.epic_pool_category]
        .filter((c) => typeof c === 'string' && c.length)
        .map((c) => c.toLowerCase()),
    });
  }
  // Which archetypes each powerset category belongs to — the inverse of the
  // above, so a category can be tested for being one class's alone.
  const categoryOwners = new Map();
  for (const at of archetypes) {
    for (const category of at.categories) {
      if (!categoryOwners.has(category)) categoryOwners.set(category, new Set());
      categoryOwners.get(category).add(at.token);
    }
  }
  const roster = new Set(archetypes.map((at) => at.token));

  const readAttribs = new Set();
  const candidates = [];
  for (const file of powerFiles(rawRoot, exclude)) {
    let power;
    try {
      power = JSON.parse(fs.readFileSync(file, 'utf-8'));
    } catch {
      continue;
    }
    if (!power || typeof power !== 'object' || !Array.isArray(power.effects)) continue;
    gateReadAttribs(power.effects, readAttribs);
    if (!power.auto_issue) continue;
    const named = requiredArchetypes(power.requires).filter((token) => roster.has(token));
    if (!named.length) continue;
    for (const attrib of selfWrittenAttribs(power.effects)) {
      candidates.push({ attrib, power: power.full_name, name: power.name, archetypes: named.sort() });
    }
  }

  const selected = [];
  const declined = [];
  const claimants = new Map();
  for (const candidate of candidates) {
    if (!claimants.has(candidate.attrib)) claimants.set(candidate.attrib, new Set());
    claimants.get(candidate.attrib).add(candidate.power);
  }
  for (const candidate of candidates.sort((a, b) =>
    `${a.attrib}${a.power}`.localeCompare(`${b.attrib}${b.power}`))) {
    const sharing = claimants.get(candidate.attrib);
    if (sharing.size > 1) {
      declined.push({ ...candidate,
        reason: `${sharing.size} powers claim ${candidate.attrib} (${[...sharing].sort().join(', ')})` });
      continue;
    }
    if (!readAttribs.has(candidate.attrib)) {
      declined.push({ ...candidate, reason: `no gate anywhere reads k${candidate.attrib} source>` });
      continue;
    }
    const categories = [...categoryOwners.entries()]
      .filter(([, owners]) => owners.size
        && [...owners].every((token) => candidate.archetypes.includes(token)))
      .map(([category]) => category)
      .sort();
    selected.push({
      attrib: candidate.attrib,
      id: candidate.name.toLowerCase(),
      label: `${labelOf(candidate.name)} Active`,
      power: candidate.power,
      archetypes: candidate.archetypes,
      categories,
    });
  }
  return { selected, declined };
}

module.exports = { detectCasterMeters };

if (require.main === module) {
  const { parseDatasetArg, pipelinePath, NESTED_DATASET_DIRS } = require('./_dataset-paths.cjs');
  const datasetId = parseDatasetArg();
  const base = path.join(__dirname, '../exported_powers');
  const isLegacyHc = datasetId === 'homecoming' && !fs.existsSync(path.join(base, datasetId));
  const rawRoot = isLegacyHc ? base : path.join(base, datasetId);
  // Derived, not a literal. This read `new Set(['rebirth', 'thunderspy'])` until 2026-09-26 and
  // so omitted `brainstorm`, which made a homecoming run walk into `exported_powers/brainstorm/`.
  const exclude = isLegacyHc ? NESTED_DATASET_DIRS : new Set();
  const sweep = detectCasterMeters(rawRoot, exclude);
  const outPath = pipelinePath(datasetId, 'caster-meters.json');
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, `${JSON.stringify(sweep, null, 1)}\n`);
  console.log(
    `[caster-meters] ${datasetId}: ${sweep.selected.length} selected `
    + `(${sweep.selected.map((s) => `${s.attrib}→${s.id}`).join(', ') || 'none'}), `
    + `${sweep.declined.length} declined → ${outPath}`);
}
