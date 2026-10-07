/**
 * Conditional-effect coverage — does every power whose exported effects carry a CLASSIFIABLE
 * state gate actually ship the `conditionalEffects` toggle for it, whichever converter
 * produced it?
 *
 * The sibling of `audit-form-coverage.cjs`, filed against the same defect class and the third
 * occurrence of it: a capability built in `convert-powerset.cjs` and never given to
 * `convert-pool-powers.cjs` / `convert-epic-pools.cjs`. It has now happened four times — the
 * six epic snipes (SNIPE-2), the pool/epic atom blackout (Plan B), Aid Other's fast form
 * (CHAIN-1), and the pool/epic conditional blackout this gate closes (COND-2). Each was
 * invisible to every existing gate for the same reason: the affected powers simply lacked a
 * field nothing asserted was required, and a partition-wide zero reads exactly like "this
 * partition has none."
 *
 * So the check runs OUTSIDE all three converters, joining the committed export (their shared
 * input) to the committed generated tree (their combined output) through `_export-join.cjs`
 * and the `planb-shadow-sweep` corpus walk — the sweep whose whole purpose is that a gate
 * cannot forget to look in a partition.
 *
 * The predicate is the converter's own `extractConditionalEffects`, CALLED rather than
 * transcribed: a copy here could drift from the emit it grades and report a clean sweep about
 * a rule the converter no longer follows.
 *
 * Three things are asserted, because no one of them is enough on its own:
 *
 *   1. **Coverage** — a power the predicate finds groups for ships a non-empty
 *      `conditionalEffects`. This is the partition-wide zero, caught directly.
 *   2. **Population** — the per-fork totals, pinned in BOTH directions. Shrinking means a
 *      converter quietly stopped emitting a family; growing means the corpus started
 *      declaring state nothing here has been read against. A coverage rule alone cannot see
 *      either, because it only ever looks at powers the predicate already flagged.
 *   3. **Injectivity of the id** — one id names one caster state, and one caster state is
 *      named by one id (COND-3). Both directions, because they are different defects and both
 *      ship a wrong toggle rather than a wrong number: an id holding two states is one switch
 *      for a bonus the game splits (the `>= 1` / `>= 5` collapse that closed COND-3), and one
 *      state holding two ids is two switches for a thing a build is either in or not, so
 *      turning on the obvious one understates. Neither is visible to a population count, which
 *      is happy either way.
 *
 * On id-set equality: it is REPORTED, not gated. The powerset converter appends entries the
 * raw effects do not carry (the Storm Cell, Oil Slick and Swap Ammo emits), so a read of the
 * export legitimately predicts a different id set there. The pool and epic converters append
 * nothing, so their sets are expected to match exactly and any divergence there is a
 * divergence.
 *
 * FOURTH, since COND-4 closed: **the archetype fork survives the emit**. A conditional group
 * whose templates are ARCHETYPE-FORKED used to project through `_bagTemplates` — whose only
 * filter is "drop a template carrying `_casterArchetypes`", because a shared bag has no slot for
 * a per-AT fork (AT-FORK-1) — and empty, taking the whole entry with it. It now projects with the
 * fork stripped and the ENTRY carrying it, so two things are asserted together: the group ships
 * at all, and the entry it ships names exactly the archetypes its group forks on. Both directions,
 * because they fail differently — an entry that lost the field is a Dominator bonus offered to
 * every class, and one that gained it is a control taken away from builds that have it.
 *
 * The exclusion this replaces is kept as an anti-revert floor. The predicate below runs the atom
 * emit first, as all three converters do, because `_casterArchetypes` is stamped by whichever
 * collector reaches a template first; reading the export WITHOUT that stamp used to predict groups
 * no converter could emit, and the difference between the two reads was exactly the forked family.
 * That difference must now be ZERO — the stamp no longer decides whether an entry exists, since
 * the group derives its own restriction from the effect tree. Reverting the fix makes it 4 again,
 * with this file's own diagnosis attached.
 *
 * Usage:
 *   node scripts/audit-conditional-coverage.cjs [--dataset <id>]... [--gate]
 *     --dataset <id>   repeatable; one of scripts/_dataset-paths.cjs ALL_DATASETS (default: all three)
 *     --gate           CI mode: one PASS/FAIL line per dataset, exit 1 on any failure
 */

require('tsx/cjs');
const path = require('path');
const { sweepDataset } = require('./planb-shadow-sweep.cjs');
const { joinerFor } = require('./_export-join.cjs');

const argv = process.argv.slice(2);
const GATE = argv.includes('--gate');
const DATASETS = (() => {
  const picked = argv.flatMap((a, i) => (a === '--dataset' && argv[i + 1] ? [argv[i + 1]] : []));
  return picked.length ? picked : require('./_dataset-paths.cjs').ALL_DATASETS;
})();

/** Which partition a generated module belongs to — the axis the drift runs along. */
function partitionOf(relPath) {
  if (relPath.includes('/power-pools-raw')) return 'pool';
  if (relPath.includes('/epic-pools-raw')) return 'epic';
  return 'powerset';
}

const idsOf = (entries) =>
  new Set((entries || []).map((entry) => entry.id).filter((id) => typeof id === 'string'));

/**
 * The caster states a group's gates assert about the group's OWN path — the axis its id and its
 * ownership claim are both derived from, normalized so that two spellings of one state read as
 * one state (`4 >` and `5 >=` are both "five or more"; a bare presence test is "one or more").
 *
 * Restricted to `_powerName` for the reason `_ownershipClaim` restricts to it: a group's gate set
 * can mention a second power entirely (Savage Melee's Exhausted gates read Blood Frenzy's count),
 * and a constraint on a path the id does not name says nothing about whether the id is right.
 */
function ownPathStates(group) {
  if (!group._powerName) return [];
  const escaped = group._powerName.replace(/[.\\]/g, '\\$&');
  const reader = new RegExp(
    `${escaped}\\s+(?:source|target)\\.ownPower(Num)?\\?(?:\\s+(\\d+)\\s+(==|>=|>|<=|<))?`,
    'gi',
  );
  const states = new Set();
  for (const gate of group._gates || []) {
    reader.lastIndex = 0;
    let match;
    while ((match = reader.exec(gate))) {
      const [, numeric, literal, operator] = match;
      if (!numeric) { states.add('atleast 1'); continue; }
      // `ownPowerNum?` with no comparison is the count as a VALUE, not an assertion about it.
      if (literal === undefined) continue;
      const bound = Number(literal);
      const state = {
        '==': `exact ${bound}`,
        '>=': `atleast ${bound}`,
        '>': `atleast ${bound + 1}`,
        '<=': `atmost ${bound}`,
        '<': `atmost ${bound - 1}`,
      }[operator];
      if (state) states.add(state);
    }
  }
  return [...states];
}

function auditDataset(dataset) {
  const { cv, join } = joinerFor(dataset);
  const unjoined = [];
  const loadErrors = [];
  const missing = [];
  const diverged = [];
  const atForked = [];
  const forked = [];
  const forkFaults = [];
  const predicted = { pool: 0, epic: 0, powerset: 0 };
  const shipped = { pool: 0, epic: 0, powerset: 0 };
  // path → id → Set(state), and path → state → Set(id). Two views of one claim; see below.
  const statesById = new Map();
  const idsByState = new Map();
  let swept = 0;

  sweepDataset(
    dataset,
    (power, relPath) => {
      swept += 1;
      const partition = partitionOf(relPath);
      const carried = idsOf(power.conditionalEffects);
      shipped[partition] += carried.size;

      const exportPath = join(power, relPath);
      if (!exportPath) {
        unjoined.push(`${power.internalName || power.name} [${relPath}]`);
        return;
      }
      // Through the converter's own reader, not a bare parse. `_readPowerFile` writes the
      // set-local variant-mode gate onto the effect groups before anyone sees them, and a
      // group whose gate arrives that way is one the converters emit and a bare parse cannot
      // predict — so this gate scored those as "no group here" and read a missing toggle as
      // an empty partition. Thunderspy's Quantum Acceleration is the instance: its
      // `flightactive` group was absent from the emit AND absent from the prediction, which
      // is the two-blind-sides shape a coverage gate exists to break.
      const read = () => cv._readPowerFile(exportPath);
      let json;
      let unstamped;
      try {
        json = read();
        unstamped = read();
      } catch (err) {
        loadErrors.push(`${relPath}: ${exportPath}: ${err.message}`);
        return;
      }
      if (!json.effects?.length) return;

      const source = path.relative(cv.RAW_DATA_PATH, exportPath);
      const name = power.internalName || power.name;

      // The converters' own order: atoms, then conditionals. See the header on why the
      // stamp has to be here for the prediction to be one a converter could satisfy.
      cv.collectAtomTemplates(json.effects || []);
      const groups = cv.extractConditionalEffects(json.effects, json) || [];
      const unforked = idsOf(cv.extractConditionalEffects(unstamped.effects, unstamped) || []);
      for (const id of unforked) {
        if (!groups.some((group) => group.id === id)) {
          atForked.push({ name, partition, source, id });
        }
      }

      // The id derivation's injectivity, read off the grouping rather than off the emitted
      // entries — the entry keeps its id but not the gates the id was derived from. The same
      // walk answers the archetype fork, for the same reason: the entry states the verdict, the
      // group states what the verdict was derived FROM, and only the pair grades the derivation.
      for (const [id, group] of cv.collectConditionalsGrouped(
        json.effects,
        json.powerset || json.full_name,
      )) {
        const restriction = group._casterArchetypes;
        const entry = groups.find((candidate) => candidate.id === id);
        const stated = entry && entry.casterArchetypes;
        if (restriction && restriction.length) {
          forked.push({ name, partition, source, id, want: restriction.join(',') });
          if (!entry) {
            forkFaults.push(`${name}/${id}: forked on ${restriction.join(',')}, ships no entry`);
          } else if (!stated || stated.join(',') !== restriction.join(',')) {
            forkFaults.push(
              `${name}/${id}: group forks on ${restriction.join(',')}, entry states ` +
                `${stated ? stated.join(',') : '(nothing)'}`,
            );
          }
        } else if (stated) {
          forkFaults.push(
            `${name}/${id}: entry states ${stated.join(',')} on a group that forks on nothing`,
          );
        }
        for (const state of ownPathStates(group)) {
          const path = group._powerName.toLowerCase();
          if (!statesById.has(path)) statesById.set(path, new Map());
          if (!idsByState.has(path)) idsByState.set(path, new Map());
          const byId = statesById.get(path);
          const byState = idsByState.get(path);
          if (!byId.has(id)) byId.set(id, new Set());
          if (!byState.has(state)) byState.set(state, new Set());
          byId.get(id).add(state);
          byState.get(state).add(id);
        }
      }

      if (!groups.length) return;
      predicted[partition] += groups.length;

      if (carried.size === 0) {
        missing.push({ name, partition, source, module: relPath, ids: [...idsOf(groups)] });
        return;
      }
      // Pool and epic reach the extractor with the export unmodified, so their id sets are
      // an equality. The powerset converter's pre-passes make its sets a report only.
      if (partition === 'powerset') return;
      const want = idsOf(groups);
      const lost = [...want].filter((id) => !carried.has(id));
      if (lost.length) {
        diverged.push({ name, partition, source, module: relPath, lost });
      }
    },
    { onLoadError: (rel, err) => loadErrors.push(`${rel}: ${err.message}`) },
  );

  const collapsed = [];
  for (const [path, byId] of statesById) {
    for (const [id, states] of byId) {
      if (states.size > 1) collapsed.push({ path, id, states: [...states].sort() });
    }
  }
  const split = [];
  for (const [path, byState] of idsByState) {
    for (const [state, ids] of byState) {
      if (ids.size > 1) split.push({ path, state, ids: [...ids].sort() });
    }
  }

  return {
    dataset, swept, unjoined, loadErrors, missing, diverged, atForked, forked, forkFaults,
    predicted, shipped, collapsed, split,
  };
}

/**
 * The population each fork ships, per partition, measured 2026-08-07 with COND-2 closed and
 * re-measured the same day when COND-3 split the collapsed threshold ids.
 *
 * Pinned in both directions and per partition, because a total alone hides the failure this
 * whole file is about: pool and epic going back to zero while the powerset partition's larger
 * number absorbs the change. `powerset` counts entries from every emit path (the extractor's
 * groups plus the Storm Cell, Oil Slick and Swap Ammo appends), which is why it exceeds what
 * a fresh read of the export predicts.
 *
 * COND-3's growth is four entries in Rebirth and four in Thunderspy, and all of it is Kinetic
 * Assault: Energetic Strike and Kinetic Shockwave each carried one `kinetic_assault_impulse`
 * holding both the any-stack and the five-stack bonus, and each now ships the two separately,
 * in both powerset copies of the set. Nothing changed in Homecoming, whose two respellings
 * (Leviathan Hunger, Savage Melee's Stalker Blood Frenzy) each held one state already.
 */
//
// RE-MEASURED 2026-08-22 closing BRAIN-3. The three pre-existing forks had drifted from the
// 2026-08-07 figures on every axis, because nothing ran this file between those dates: it is
// reachable only as an npm script and was in neither regen-all nor CI. Every movement was
// attributed against the generated tree at COND-4's own commit before re-pinning, and all of it
// is downstream of work that closed AFTER the pin:
//
//   * the Swap Ammo re-key (`chemicalammunition`/`cryoammunition` out; `fireammo`/`iceammo`/
//     `toxicammo`/`lethalammo` on HC, `firedamage`/`colddamage`/`toxicdamage` on the Parse6
//     forks) — COND-8, COND-10 and COND-12. The family also GREW, because two ammo types
//     shipped no toggle at all before. Thunderspy's Reaction Time moves the other way and is
//     the same fix: its only entry was a `cryoammunition` COND-9 borrowed from Homecoming's
//     authored modes, onto a power whose own export states no ammo gate and, before COND-11,
//     stated no tags at all.
//   * the minted mode toggles — `flightactive`, `groupflying`, `shapeshiftactive`,
//     `graniteroot`/`granite_armor_mode`, `gravitydistortion` — COND-12's chance-mod selector
//     pass, matching its committed per-dataset sweep artifact fork for fork.
//   * the Parse6 forks' Domination bonus, COND-13 (see EXPECTED_FORKED below).
//   * `boxing` and `cross_punch` on Brawl, +2 to each fork that has them — BRAIN-3's own fix.
//
// No power lost its toggles to a cause its export does not state: measured directly, and the
// single power that went from some to none is the Thunderspy re-key named above.
const EXPECTED = {
  // 513 -> 539 with Issue 28 Page 4 going live (2026-10-06): Homecoming now equals the
  // brainstorm figure below, whose notes attribute the 26 (Sonic Aura, Light Affinity).
  //
  // 11/539 -> 10/535 on both Homecoming forks when the literal-`0` gate stopped minting a
  // toggle: Cross Punch's never-true group (pool) and Personal Force Field's `Display` group
  // on its four Force Field copies (powerset), each surfaced as a "Conditional" adjuster.
  homecoming: { pool: 10, epic: 7, powerset: 535 },
  rebirth: { pool: 16, epic: 2, powerset: 473 },
  thunderspy: { pool: 10, epic: 3, powerset: 618 },
  // Brainstorm, measured 2026-08-21 off the i28p4 open beta. Its pool and epic
  // populations equal Homecoming's; the powerset figure runs 18 ahead, which is Sonic
  // Aura and Light Affinity arriving with conditional toggles of their own. Pinned from
  // measurement rather than copied from Homecoming: a shared hand number is right on one
  // fork and unasked on the other, and this dataset exists precisely to differ.
  // Brainstorm was pinned from measurement 2026-08-21 off the i28p4 open beta rather than
  // copied from Homecoming, and its pool and epic figures held on re-measure — the powerset
  // line moves by Brawl's two and nothing else.
  //
  // 531 -> 539 on the i28p4 RC3 re-export (2026-09-19). Pool and epic held again; the eight
  // are Light Affinity's Lightfield and Spotlight, one conditional each across all four ATs.
  // Both carried no gate at all on 2026-08-20 (three effects, no requires), so this is a gate
  // arriving with Build 3/RC1's Radiance rework, not an existing one being re-read.
  //
  // The gate token is `kChain_Jolt_Mode`, which is upstream's, not ours: HC built Radiance's
  // gating on the existing Chain Jolt mode rather than minting one, and no Radiance token
  // exists anywhere in the export. It reaches the UI as the label "Chain Jolt Mode" on a Light
  // Affinity power, which looks like our bug and is not — `requires_expression` is a string
  // array resolved from the string table, never an index through the mode table, so the
  // 217 -> 219 mode renumber in the same patch cannot reach it.
  //
  // 11/539 -> 10/535 with Homecoming, above: the literal-`0` gate is no toggle.
  brainstorm: { pool: 10, epic: 7, powerset: 535 },
};

/**
 * The archetype-forked groups the emit DROPPED, pinned at zero per fork since COND-4 closed.
 *
 * This is the anti-revert floor described in the header. It measures the difference between a
 * read with the atom stamp live and one without, which used to be exactly the forked family —
 * so the fix moving it 4 → 0 on Homecoming is the visible change, and any revert brings the
 * four back under this name.
 */
const EXPECTED_AT_FORKED = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };

/**
 * The conditional groups that fork on the caster's archetype, pinned per fork.
 *
 * Homecoming's four are the whole population: the Domination bonus on Cross Punch, Intimidate
 * and Invoke Panic — the tag-route groups, which carry `arch source> Class_Dominator eq` on the
 * pool copies where a Dominator's own set needs no such clause — and Mace Blast's crit branch
 * under its `arch source> Class_Stalker eq` parent. Rebirth and Thunderspy carry none: their
 * equivalents state the same fork as a gate, which the untoggleable rule rejects one step
 * earlier, so no group is ever built for it.
 *
 * Pinned in both directions, and separately from the entry populations above, because the two
 * answer different questions. The populations say how many toggles ship; this says how many of
 * them are one archetype's — and a fix that shipped all four as everyone's toggle would satisfy
 * the first count while getting the second exactly wrong.
 */
// RE-MEASURED 2026-08-22 closing BRAIN-3, and the paragraph above is the pin that rotted: the
// zero on both Parse6 forks was true when written and was retired eleven days later by COND-13.
// `_isUntoggleableGate` used to reject `arch source> Class_` on the clause's PRESENCE, which
// sank exactly the shape those forks state — the Dominator fork CHAINED with a real
// `kStealth source> 0.5 >` toggle. COND-13 narrowed the skip to strip-and-retest, so the four
// Rebirth and seven Thunderspy groups build now, and every one of them is that gate.
//
// Homecoming's fifth is the same gate on Flurry, and the same fix. Its four tag-route groups
// are unchanged. Nothing here is a new fork in the data: the export's requires-expressions are
// byte-identical across COND-11's re-export, which only added `tags`.
//
// The ids differ by fork ON PURPOSE and are not a COND-3 collapse: both Parse6 forks grant
// Dominators an `Inherent.Inherent.Domination` that writes Stealth, so the gate IS Domination
// there, while Homecoming moved that bar to `Meter` and grants no Stealth-writer, so the same
// gate is genuinely Stealthed. `detect-caster-meters.cjs` reads that per dataset (COND-13) and
// nothing here spells it. Homecoming's Ear Splitter re-keys `domination` to `stealthed` on the
// same reading, and carries no `Domination` tag to contradict it.
const EXPECTED_FORKED = { homecoming: 5, rebirth: 4, thunderspy: 7, brainstorm: 5 };

/**
 * How many generated powers may fail to resolve to an export. ZERO, for the reason
 * `audit-form-coverage.cjs` gives: a power this cannot reach is a power it cannot check,
 * which is the same silence the gate exists to break.
 */
const MAX_UNJOINED = 0;

let failed = false;
for (const dataset of DATASETS) {
  const r = auditDataset(dataset);
  const want = EXPECTED[r.dataset];
  const problems = [];

  if (r.missing.length) {
    problems.push(
      `${r.missing.length} power(s) carry a classifiable gate and ship no conditionalEffects`,
    );
  }
  if (r.diverged.length) {
    problems.push(`${r.diverged.length} pool/epic power(s) dropped a predicted toggle`);
  }
  if (r.collapsed.length) {
    problems.push(`${r.collapsed.length} id(s) carry two different caster states (COND-3)`);
  }
  if (r.split.length) {
    problems.push(`${r.split.length} caster state(s) reached by two different ids (COND-3)`);
  }
  if (r.loadErrors.length) problems.push(`${r.loadErrors.length} load error(s)`);
  if (r.unjoined.length > MAX_UNJOINED) {
    problems.push(
      `${r.unjoined.length}/${r.swept} generated powers did not resolve to an export, so they ` +
        `were not checked at all: ${r.unjoined.slice(0, 5).join(', ')}` +
        (r.unjoined.length > 5 ? ', …' : ''),
    );
  }
  // Pool and epic reach the extractor with the export unmodified, so what a read of the export
  // PREDICTS there and what the converters SHIP is one number, and the header says so. Asserted
  // because the coverage rule alone is one-directional: it fires when a power ships nothing
  // where a group was predicted, and is therefore vacuous exactly when the prediction collapses.
  // A prediction read through the wrong door does collapse — this file used to bare-parse the
  // export while every converter reads it through `_readPowerFile`, so any group whose gate is
  // written by the variant-mode pass was invisible on BOTH sides and the sweep stayed green
  // (BRAIN-3). The equality is the tripwire the missing-toggle count cannot be.
  for (const partition of ['pool', 'epic']) {
    if (r.predicted[partition] !== r.shipped[partition]) {
      problems.push(
        `${partition}: ${r.shipped[partition]} entries shipped but ${r.predicted[partition]} ` +
          `predicted from the export — the two must agree in this partition`,
      );
    }
  }
  if (want) {
    for (const partition of ['pool', 'epic', 'powerset']) {
      if (r.shipped[partition] !== want[partition]) {
        problems.push(
          `${partition}: ${r.shipped[partition]} conditional entries, pinned at ${want[partition]}`,
        );
      }
    }
  } else {
    problems.push(`no pinned population for ${r.dataset}`);
  }
  const wantDropped = EXPECTED_AT_FORKED[r.dataset];
  if (wantDropped === undefined) {
    problems.push(`no pinned archetype-forked drop count for ${r.dataset}`);
  } else if (r.atForked.length !== wantDropped) {
    problems.push(
      `${r.atForked.length} archetype-forked group(s) dropped by the emit, pinned at ` +
        `${wantDropped} (COND-4)`,
    );
  }
  const wantForked = EXPECTED_FORKED[r.dataset];
  if (wantForked === undefined) {
    problems.push(`no pinned archetype-forked population for ${r.dataset}`);
  } else if (r.forked.length !== wantForked) {
    problems.push(
      `${r.forked.length} archetype-forked group(s), pinned at ${wantForked} (COND-4)`,
    );
  }
  if (r.forkFaults.length) {
    problems.push(`${r.forkFaults.length} entr(ies) disagree with their group's fork (COND-4)`);
  }

  if (!GATE) {
    console.log(
      `\n${r.dataset}: ${r.swept} generated powers, ${r.unjoined.length} unjoined\n` +
        `  shipped   pool ${r.shipped.pool}  epic ${r.shipped.epic}  powerset ${r.shipped.powerset}\n` +
        `  predicted pool ${r.predicted.pool}  epic ${r.predicted.epic}  powerset ${r.predicted.powerset}\n` +
        `  archetype-forked: ${r.forked.length} group(s), ${r.atForked.length} dropped by the emit`,
    );
    for (const u of r.unjoined) console.log(`  UNJOINED  ${u}`);
    for (const m of r.missing) {
      console.log(`  MISSING  [${m.partition}] ${m.name}  want ${m.ids.join(', ')}  [${m.source}]`);
    }
    for (const d of r.diverged) {
      console.log(`  DROPPED  [${d.partition}] ${d.name}  lost ${d.lost.join(', ')}  [${d.source}]`);
    }
    for (const f of r.forked) {
      console.log(`  AT-FORKED  [${f.partition}] ${f.name}  ${f.id}  ${f.want}  [${f.source}]`);
    }
    for (const f of r.atForked) {
      console.log(`  DROPPED BY THE EMIT (COND-4)  [${f.partition}] ${f.name}  ${f.id}  [${f.source}]`);
    }
    for (const f of r.forkFaults) console.log(`  FORK MISMATCH  ${f}`);
    for (const c of r.collapsed) {
      console.log(`  COLLAPSED  ${c.id}  holds ${c.states.join(' + ')}  on ${c.path}`);
    }
    for (const s of r.split) {
      console.log(`  SPLIT  ${s.state} on ${s.path}  reached by ${s.ids.join(' + ')}`);
    }
    for (const e of r.loadErrors) console.log(`  LOAD ERROR  ${e}`);
  }

  if (problems.length) {
    failed = true;
    console.log(`FAIL ${r.dataset}: ${problems.join('; ')}`);
    for (const m of r.missing) console.log(`     ${m.partition}/${m.name} [${m.source}]`);
    for (const d of r.diverged) console.log(`     ${d.partition}/${d.name} lost ${d.lost.join(', ')}`);
    for (const f of r.forkFaults) console.log(`     ${f}`);
    for (const c of r.collapsed) console.log(`     ${c.id} holds ${c.states.join(' + ')} on ${c.path}`);
    for (const s of r.split) console.log(`     ${s.state} on ${s.path} reached by ${s.ids.join(' + ')}`);
  } else {
    console.log(
      `PASS ${r.dataset}: pool ${r.shipped.pool}, epic ${r.shipped.epic}, ` +
        `powerset ${r.shipped.powerset} conditional entries, all classifiable gates served`,
    );
  }
}
process.exit(failed ? 1 : 0);
