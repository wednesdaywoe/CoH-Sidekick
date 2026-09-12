/**
 * Convert the universal inherent powers — the ones every character is handed
 * regardless of archetype — from raw game JSON → a per-fork generated module.
 *
 * These are Brawl, Sprint, Rest, the free travel toggles (Ninja / Beast /
 * Athletic Run) and the prestige sprints — the last hand-authored powers in the
 * planner. `datasets/homecoming/levels.ts` spelled out their scales, tables and
 * endurance by hand, and because only Homecoming has a `levels.ts`, every fork
 * got Homecoming's copy. That cost four shipped wrong numbers, all of them
 * things the export states plainly (DATA-GAP-REGISTER INHERENT-4, INHERENT-5,
 * INHERENT-8).
 *
 * WHICH powers: the address list below, one entry per power the planner offers.
 * That list is curation, not derivation, and it is the only hand-held thing here
 * — no structural predicate separates "the sprint you slot" from the forty
 * vanity pets and costume toggles that share `Inherent.Inherent` and its
 * auto-issue flag. Everything the planner then SAYS about a power (its type,
 * endurance, atoms, slots, allowed enhancements, icon, help text) is read off
 * the record.
 *
 * WHICH fork: each entry is resolved against the fork being converted, and a
 * fork that holds none of an entry's addresses emits nothing for it. That is the
 * INHERENT-4 fix — Thunderspy has no Ninja Run, Beast Run or Athletic Run
 * anywhere in its export, and was being offered all three.
 *
 * Homecoming states its two P2W runs as an `Auto` granter whose only templates
 * are `Revoke_Power` on itself and `Grant_Power` aimed at the real toggle in the
 * `Prestige` category. `followGrant` walks that hop, so what gets emitted is the
 * toggle the character actually runs rather than the bookkeeping power that
 * hands it over. Rebirth authors the same power inline, with no hop.
 *
 * Usage:
 *   node scripts/convert-basic-inherents.cjs --dataset homecoming
 *   node scripts/convert-basic-inherents.cjs --dataset homecoming --dry-run
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
  assignModes,
  resolveThunderspyMovementTargets,
  guardThunderspyOnesBuffs,
  guardThunderspyAppliedMez,
  TARGET_TYPE_MAP,
  EFFECT_AREA_MAP,
  BOOST_TYPE_MAP,
  BIN_BOOST_MAP,
  RAW_DATA_PATH,
  extractGrantEdges,
  _readPowerFile,
} = require('./convert-powerset.cjs');
const { displayText, helpText } = require('./_display-text.cjs');
const { parseDatasetArg, datasetPath } = require('./_dataset-paths.cjs');
const { gateTokens } = require('./_gate-tokens.cjs');
const { powerStats } = require('./_power-stats.cjs');

const datasetId = parseDatasetArg();
const dryRun = process.argv.includes('--dry-run');

const OUTPUT_PATH = datasetPath(datasetId, 'generated', 'basic-inherents.ts');

/**
 * The powers the planner offers, by the address each fork files them under.
 *
 * `addresses` is tried in order and the first hit wins, so a fork that moved a
 * power between categories still resolves. The order matters in exactly one
 * place: Homecoming files Ninja and Beast Run in BOTH `Inherent.Inherent` (the
 * granter) and `Prestige.Prestige_Travel` (the toggle), and the granter is
 * listed first so `followGrant` does the walk and the emitted record carries the
 * granter's provenance. Athletic Run has no granter on any fork — the toggle is
 * handed over directly — so its only address is the Prestige one.
 *
 * `category` and `locked` are planner-side facts with no counterpart in the
 * game data: which picker tab the power renders under, and that the player
 * cannot un-grant it.
 *
 * Slot ceiling: a stated `max_boosts` is read literally, and export silence
 * decodes to six — not as a convention but as what the export means. The game's
 * parse table stamps 6 into the binary when the authored def says nothing
 * (powers_load.c `TOK_INT(BasePower, iMaxBoosts, 6)`), the exporter omits
 * exactly the value 6, and the engine enforces the stored value plus a
 * five-slot buy cap (`MAX_BOOSTS_BOUGHT`) that lands on the same six. So the
 * three travel toggles state `0` and mean it, and Sprint, Rest and the
 * prestige sprints are silent sixes. The hand table's four had no source and
 * is retired. Closed INHERENT-7 / MAXBOOST-1.
 */
const OFFERED = [
  {
    addresses: ['Inherent.Inherent.Brawl'],
    category: 'basic',
    locked: true,
  },
  {
    addresses: ['Inherent.Inherent.Sprint'],
    category: 'basic',
    locked: true,
  },
  {
    addresses: ['Inherent.Inherent.Rest'],
    category: 'basic',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.Prestige_Ninja_Run',
      'Prestige.Prestige_Travel.Prestige_Ninja_Run',
    ],
    category: 'basic',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.Prestige_Beast_Run',
      'Prestige.Prestige_Travel.Prestige_Beast_Run',
    ],
    category: 'basic',
    locked: true,
  },
  {
    addresses: ['Prestige.Prestige_Travel.Prestige_Athletic_Run'],
    category: 'basic',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.prestige_DVD_Glidep',
      'Prestige.Prestige_Sprints.prestige_DVD_Glidep',
    ],
    category: 'prestige',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.prestige_Gamestop_Sprintp',
      'Prestige.Prestige_Sprints.prestige_Gamestop_Sprintp',
    ],
    category: 'prestige',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.prestige_generic_Sprintp',
      'Prestige.Prestige_Sprints.prestige_generic_Sprintp',
    ],
    category: 'prestige',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.prestige_BestBuy_Sprintp',
      'Prestige.Prestige_Sprints.prestige_BestBuy_Sprintp',
    ],
    category: 'prestige',
    locked: true,
  },
  {
    addresses: [
      'Inherent.Inherent.prestige_EB_Sprintp',
      'Prestige.Prestige_Sprints.prestige_EB_Sprintp',
    ],
    category: 'prestige',
    locked: true,
  },
];

/**
 * Every power record in this fork's export, keyed by lowercased `full_name`.
 *
 * Built by walking the whole tree rather than the two directories the address
 * list happens to name today: a fork that files one of these somewhere else
 * still resolves, and a missing category shows up as a missing power rather than
 * a missing directory.
 */
function buildIndex() {
  const index = new Map();
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (!entry.name.endsWith('.json') || entry.name === 'index.json') continue;
      let json;
      try {
        // Through the gate-stamping reader (COND-12): the Stance inherent is a carrier.
        json = _readPowerFile(full);
      } catch {
        continue;
      }
      if (json && typeof json.full_name === 'string') {
        index.set(json.full_name.toLowerCase(), json);
      }
    }
  };
  for (const entry of fs.readdirSync(RAW_DATA_PATH, { withFileTypes: true })) {
    // The Homecoming export root holds the other two forks as subdirectories.
    if (!entry.isDirectory() || entry.name === 'rebirth' || entry.name === 'thunderspy') continue;
    walk(path.join(RAW_DATA_PATH, entry.name));
  }
  return index;
}

/** The power names a `Grant_Power` template hands over. */
function grantTargets(rawJson) {
  const targets = [];
  for (const group of rawJson.effects || []) {
    for (const template of group.templates || []) {
      if (!(template.attribs || []).includes('Grant_Power')) continue;
      for (const name of (template.params && template.params.power_names) || []) targets.push(name);
    }
  }
  return targets;
}

/**
 * Resolve a granter to the power it hands over.
 *
 * A granter is recognised by what it does, not by its name or category: every
 * one of its templates is `Grant_Power` or `Revoke_Power`, so it carries no
 * mechanic of its own and there is nothing to lose by walking past it. A record
 * that grants something AND states its own effects (Rebirth's Ninja Run grants a
 * `Ninja_Run` marker alongside its movement) is not a granter and is kept whole.
 *
 * Returns the granted record, or `null` when the record is not a granter.
 */
function followGrant(rawJson, index) {
  const attribs = [];
  for (const group of rawJson.effects || []) {
    for (const template of group.templates || []) attribs.push(...(template.attribs || []));
  }
  if (!attribs.length) return null;
  if (!attribs.every((a) => a === 'Grant_Power' || a === 'Revoke_Power')) return null;

  // The revoke aims at the granter itself; the grant is the one that leaves.
  const granted = grantTargets(rawJson).filter(
    (name) => name.toLowerCase() !== String(rawJson.full_name).toLowerCase(),
  );
  if (granted.length !== 1) {
    throw new Error(
      `${rawJson.full_name}: expected exactly one granted power, got ${JSON.stringify(granted)}`,
    );
  }
  const target = index.get(granted[0].toLowerCase());
  if (!target) {
    throw new Error(
      `${rawJson.full_name} grants ${granted[0]}, which is not in this fork's export. ` +
        'The grant target is the power the character actually runs — emitting the granter ' +
        'instead would ship a power with no mechanic (INHERENT-6).',
    );
  }
  return target;
}

/**
 * Convert one resolved record → the Power shape the planner consumes.
 *
 * Same encoding as every other converter (shared helpers from
 * convert-powerset.cjs), so these powers read exactly like a pool or powerset
 * pick once they arrive.
 */
function convertBasicInherent(rawJson, entry, granter) {
  const power = {};

  power.name = displayText(rawJson.display_name) || rawJson.name;
  power.internalName = rawJson.name;
  power.fullName = rawJson.full_name;
  // The Auto record that hands this toggle over, when the fork uses one. Kept so
  // a reader can tell a walked grant from a power authored inline, which is the
  // whole of the Homecoming/Rebirth difference here.
  if (granter) power.grantedBy = granter.full_name;

  // Auto-granted and hidden from the picker: the game hands these over, it never
  // offers them. `-1` is the planner's marker for that, and it is a planner fact
  // — the records' own `available_level` is 0 or 3 and describes when the GAME
  // grants, which the planner (a level-50 tool) does not gate on.
  power.available = -1;
  // The level the game actually hands it over at, 1-based and separate from the
  // marker above because that one is spoken for. `character_GrantAutoIssuePowers`
  // gates on `available <= level`, so this record's own `available_level` IS the
  // grant level — Rest's 1 means level 2, which is what Mids' own files state and
  // what the planner stamped as 1 for as long as nothing carried the field
  // (MBDEXPORT-18). Read off the GRANTER for the same reason `autoIssue` is:
  // the toggle is not what the gate is evaluated on.
  power.grantedAtLevel = ((granter ?? rawJson).available_level ?? 0) + 1;
  // Read off the GRANTER where the fork uses one. The Homecoming toggle sitting
  // in `Prestige` is not itself auto-issued — the `Auto` record that hands it
  // over is — so reading the toggle's own flag would report a power the game
  // gives every character as one it offers to none.
  power.autoIssue = (granter ?? rawJson).auto_issue === true;
  power.free = (granter ?? rawJson).free === true;
  power.isLocked = entry.locked;
  power.category = entry.category;

  power.description = helpText(rawJson.display_help) || '';
  if (displayText(rawJson.display_short_help)) {
    power.shortHelp = rawJson.display_short_help.replace(/ /g, ' ');
  }
  power.icon = normalizeIconPath(rawJson.icon || '');
  power.powerType = rawJson.type || 'Toggle';

  // The same call every other Power-emitting converter makes, for the reason
  // `assignModes` states: a power's mode gating must not depend on which tree
  // converted it. This was the last tree without it. Read off the toggle rather
  // than the granter, because the granter only says who hands the power over,
  // while the toggle is what carries `modes_suspended` — the field saying a
  // Kheldian in Nova form is not getting their Sprint speed (INHERENT-9).
  assignModes(power, rawJson);

  if (rawJson.target_type) {
    const mapped = TARGET_TYPE_MAP[rawJson.target_type];
    if (mapped) power.targetType = mapped;
  }

  // The entitlement gate (`Preorder:BestBuy auth> VetSprints Owned? ||`, and the
  // Homecoming granters' bare `0`). Carried through unread: it asks about the
  // account, not the build, and the planner shows every character every one of
  // these. Kept so the question stays visible in the data.
  power.requires = gateTokens(rawJson.requires);

  // Literal read; silence is the exporter suppressing the parse-table default,
  // so it decodes to 6 (see the OFFERED comment — INHERENT-7 / MAXBOOST-1).
  power.maxSlots =
    rawJson.max_boosts === undefined || rawJson.max_boosts === null
      ? 6
      : rawJson.max_boosts;

  const enhancements = (rawJson.boosts_allowed || [])
    .map((b) => BOOST_TYPE_MAP[b] || BIN_BOOST_MAP[b])
    .filter(Boolean);
  power.allowedEnhancements = [...new Set(enhancements)].sort();

  // null and [] both mean the game lists this power in no set — the field stays
  // unset and only single IOs work. No inference fallback (SETCAT-1).
  if (Array.isArray(rawJson.allowed_set_categories) && rawJson.allowed_set_categories.length) {
    power.allowedSetCategories = [...rawJson.allowed_set_categories].sort();
  }

  // Shared mint, so this partition publishes the same object as an archetype power. It gains
  // the reach fields the hand-written block here omitted (range/radius/arc/maxTargets): Brawl
  // states a range and a target cap, and nothing was reading them off the record.
  power.stats = powerStats(rawJson);

  // Thunderspy movement target-trap: resolve empty movement-template targets
  // from targets_affected before any collector reads them. Load-bearing here —
  // Thunderspy's Sprint carries the whole travel band the other forks put on
  // Ninja Run.
  resolveThunderspyMovementTargets(rawJson);

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

  {
    const atomTemplates = [
      ...new Set([...allTemplates, ...collectAtomTemplates(rawJson.effects || [])]),
    ];
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
  // This converter shipped no conditionals at all until BRAIN-3 — the fifth instance of a
  // capability built in convert-powerset.cjs and never handed to a sibling. Brawl's entire
  // Fighting-pool synergy sat gated in the export with no toggle able to reach it: the
  // Boxing-or-Kick -recharge/-tohit pair and Cross Punch's -regen/-recovery, on Homecoming,
  // Rebirth and Brainstorm alike. Thunderspy's Brawl states neither.
  if (rawJson.effects?.length) {
    const conditional = extractConditionalEffects(rawJson.effects, rawJson);
    if (conditional) power.conditionalEffects = conditional;
  }

  // Caster-state writes (grant/revoke edges) — the stamp every partition gets, called
  // explicitly for the audit-grant-edges.cjs reason: an extractor reaches only the
  // converters that ask.
  const grantEdges = extractGrantEdges(rawJson);
  if (grantEdges) power.grantEdges = grantEdges;

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

// Serialize like the sibling converters: atoms one tuple per line, everything
// else pretty.
function serializeValue(val, indent) {
  if (val === null || val === undefined) return 'null';
  if (typeof val === 'number' || typeof val === 'boolean') return String(val);
  if (typeof val === 'string') return JSON.stringify(val);

  if (Array.isArray(val)) {
    if (val.length === 0) return '[]';
    const items = val.map((v) => `${' '.repeat(indent + 2)}${serializeValue(v, indent + 2)}`);
    return `[\n${items.join(',\n')}\n${' '.repeat(indent)}]`;
  }

  const keys = Object.keys(val);
  if (keys.length === 0) return '{}';
  const entries = keys.map((k) => {
    if (k === 'atoms' && Array.isArray(val[k]) && val[k].length) {
      const tuples = val[k]
        .map((t) => `${' '.repeat(indent + 4)}${JSON.stringify(t)}`)
        .join(',\n');
      return `${' '.repeat(indent + 2)}${JSON.stringify(k)}: [\n${tuples}\n${' '.repeat(indent + 2)}]`;
    }
    return `${' '.repeat(indent + 2)}${JSON.stringify(k)}: ${serializeValue(val[k], indent + 2)}`;
  });
  return `{\n${entries.join(',\n')}\n${' '.repeat(indent)}}`;
}

function main() {
  console.log(`=== CONVERT BASIC INHERENTS (dataset: ${datasetId})${dryRun ? ' [DRY RUN]' : ''} ===\n`);

  const index = buildIndex();
  if (!index.size) {
    throw new Error(`No power records under ${RAW_DATA_PATH} — nothing to resolve addresses against.`);
  }

  const powers = [];
  const absent = [];

  for (const entry of OFFERED) {
    const address = entry.addresses.find((a) => index.has(a.toLowerCase()));
    if (!address) {
      absent.push(entry.addresses[0]);
      continue;
    }
    const record = index.get(address.toLowerCase());
    const granted = followGrant(record, index);
    const power = convertBasicInherent(granted ?? record, entry, granted ? record : null);
    powers.push(power);
    const atoms = Array.isArray(power.atoms) ? power.atoms.length : 0;
    const via = granted ? ` via ${record.full_name}` : '';
    console.log(`  ${power.internalName.padEnd(28)} ${power.fullName}${via} atoms=${atoms}`);
  }

  // A fork that holds none of an entry's addresses gets nothing for it. Loud,
  // because the shape of this list IS the answer to "which powers does this fork
  // grant" and a silent shortfall reads the same as a converter that ran clean.
  if (absent.length) {
    console.log(`\n  Not in this fork's export (${absent.length}): ${absent.join(', ')}`);
  }

  const duplicates = powers
    .map((p) => p.internalName)
    .filter((name, i, all) => all.indexOf(name) !== i);
  if (duplicates.length) {
    throw new Error(
      `Duplicate internalName in ${datasetId}: ${duplicates.join(', ')}. ` +
        'internalName is the identity saved builds store, so two powers cannot share one.',
    );
  }

  const totalAtoms = powers.reduce((n, p) => n + (Array.isArray(p.atoms) ? p.atoms.length : 0), 0);
  let out = `/**\n`;
  out += ` * Universal inherent powers — AUTO-GENERATED, DO NOT EDIT.\n`;
  out += ` *\n`;
  out += ` * Sprint, Rest, the free travel toggles and the prestige sprints, read from\n`;
  out += ` * THIS fork's own export. A power missing here is one ${datasetId} does not\n`;
  out += ` * have; see scripts/convert-basic-inherents.cjs for how each is addressed.\n`;
  out += ` * Regenerate: node scripts/convert-basic-inherents.cjs --dataset ${datasetId}\n`;
  out += ` *\n`;
  out += ` * Powers: ${powers.length}, atoms: ${totalAtoms}\n`;
  out += ` */\n\n`;
  out += `import type { Power } from '@/types';\n\n`;
  out += `/** A universal inherent: an ordinary Power plus the three planner-side facts. */\n`;
  out += `export type BasicInherentDef = Power & {\n`;
  out += `  isLocked?: boolean;\n`;
  out += `  category?: 'basic' | 'prestige';\n`;
  out += `  /** 1-based level the game grants it at; 'available' is the picker's marker, not this. */\n`;
  out += `  grantedAtLevel?: number;\n`;
  out += `};\n\n`;
  out += `export const BASIC_INHERENTS: BasicInherentDef[] = ${serializeValue(powers, 0)};\n`;

  if (dryRun) {
    console.log(`\nWould write ${OUTPUT_PATH} (${powers.length} powers, ${totalAtoms} atoms)`);
    return;
  }
  fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
  fs.writeFileSync(OUTPUT_PATH, out);
  console.log(`\nWrote ${OUTPUT_PATH} (${powers.length} powers, ${totalAtoms} atoms)`);
}

main();
