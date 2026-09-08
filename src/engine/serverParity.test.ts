/**
 * PROD5 — per-server engine-vs-legacy parity spot-check.
 *
 * The spike browser-verified only Homecoming. This exercises all three datasets through the
 * Rust engine and diffs its dashboard totals against the legacy TS calc on an IDENTICAL,
 * data-driven build (no fork proper nouns — the build is assembled by reading each dataset's
 * own registries, per Rule 0). A per-server row surfacing here means the engine and the old
 * calc disagree on that fork's data, which is exactly what PROD5 must catch before the two
 * new servers ship on the engine.
 *
 * The engine runs via the wasm-node target (the browser `engine.ts` uses fetch + `?url`, which
 * has no Node path); the reshape mirrors `engineCalculate` field-for-field so the comparison is
 * against what the browser would produce.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadDataset } from '@/data/dataset';
import { getArchetype, STANDARD_ARCHETYPE_IDS } from '@/data/archetypes';
import { getAllPowersets, getPowersetsForArchetype } from '@/data/powersets';
import {
  toHitBuffValue, damageBuffValue, resistanceBuffValue, defenseBuffValue,
  defenseBuffSuppressibleValue, regenBuffValue, recoveryBuffValue,
  absorbMaxHPFractionValue, baseAtoms,
} from '@/data/core/atom-query';
import { getAvailableGenericIOs, createGenericIOEnhancement } from '@/data/enhancement-registry';
import { withoutIllegalSlots } from '@/utils/build-enhancement-validation';
import { createEmptyBuild } from '@/types/build';
import { legacyCalculateCharacterTotals } from '@/utils/calculations/legacy-totals.oracle';
import { toCharacterStateJson, type AdapterCalcContext } from './characterStateAdapter';
import { mapStats, mapGlobal, type EngineTotals } from './engineTotalsMap';
import type { Build } from '@/types/build';
import type { Power, SelectedPower } from '@/types/power';

// wasm-node is a CommonJS module that instantiates the .wasm synchronously from its own dir.
const require = createRequire(import.meta.url);

const SERVERS = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'] as const;
type Server = (typeof SERVERS)[number];

// The engine artifacts are generated and gitignored — the web target, the Node target this test
// loads, and the bundles all come from `npm run build:engine`. A fresh checkout / CI without
// those cannot run this — skip loudly rather than error.
const NODE_ENGINE = join(__dirname, 'wasm-node', 'coh_wasm.cjs');
const BUNDLE_DIR = join(__dirname, '..', '..', 'public', 'engine', 'contract');
const artifactsReady =
  existsSync(NODE_ENGINE) && SERVERS.every((s) => existsSync(join(BUNDLE_DIR, `${s}.json.gz`)));

type EngineHandle = { recalculate: (json: string) => string };
const nodeEngine = artifactsReady
  ? (require('./wasm-node/coh_wasm.cjs') as { load_dataset: (bytes: Uint8Array) => EngineHandle })
  : null;

// One handle per dataset, loaded from the same gzip bundle engine.ts fetches in the browser.
const handles = new Map<Server, EngineHandle>();

/** Whether the ENGINE's own copy of a power authors any per-foe increment, read off its bundle
 *  bag exactly the way `forkHasPerFoeData` reads the legacy one (2026-08-19 oracle re-cut).
 *  The per-foe walk below discovers its corpus from the LEGACY dataset's increments, and the two
 *  converters disagree about a handful of powers — the legacy one invents an increment on rows
 *  the export states as flat Self buffs (a Defiance rider read as per-foe scaling), while the
 *  contract's `per_target` stamp is restricted to the export's real increments (PERFOE-1). A
 *  response comparison on such a power grades two different authored datasets, not the two
 *  calcs, so it is logged and skipped; a power BOTH datasets give an increment still fails hard
 *  on any response mismatch. */
const bundleEffectsCache = new Map<Server, Map<string, Record<string, unknown>>>();
function bundleEffects(server: Server): Map<string, Record<string, unknown>> {
  const cached = bundleEffectsCache.get(server);
  if (cached) return cached;
  const { gunzipSync } = require('node:zlib') as typeof import('node:zlib');
  const bundle = JSON.parse(
    gunzipSync(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))).toString('utf8'),
  ) as Record<string, Record<string, { id?: string; powers?: { internalName?: string; effects?: Record<string, unknown> }[] }>>;
  const out = new Map<string, Record<string, unknown>>();
  for (const section of ['powersets', 'power-pools', 'epic-pools']) {
    for (const [setKey, set] of Object.entries(bundle[section] ?? {})) {
      for (const power of set.powers ?? []) {
        if (power.internalName) out.set(`${set.id ?? setKey}\0${power.internalName}`, power.effects ?? {});
      }
    }
  }
  bundleEffectsCache.set(server, out);
  return out;
}

/** The bundle meta the engine's derived per-foe absorb gate reads (`effectArea`,
 *  `stats.maxTargets`), keyed like `bundleEffects`. The BPORT7 regen empties the bag this
 *  sits next to, but this meta survives it. */
const bundleMetaCache = new Map<Server, Map<string, { effectArea?: string; maxTargets?: number }>>();
function bundleMeta(server: Server): Map<string, { effectArea?: string; maxTargets?: number }> {
  const cached = bundleMetaCache.get(server);
  if (cached) return cached;
  const { gunzipSync } = require('node:zlib') as typeof import('node:zlib');
  const bundle = JSON.parse(
    gunzipSync(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))).toString('utf8'),
  ) as Record<string, Record<string, { id?: string; powers?: { internalName?: string; effectArea?: string; stats?: { maxTargets?: number } }[] }>>;
  const out = new Map<string, { effectArea?: string; maxTargets?: number }>();
  for (const section of ['powersets', 'power-pools', 'epic-pools']) {
    for (const [setKey, set] of Object.entries(bundle[section] ?? {})) {
      for (const power of set.powers ?? []) {
        if (power.internalName) {
          out.set(`${set.id ?? setKey}\0${power.internalName}`, { effectArea: power.effectArea, maxTargets: power.stats?.maxTargets });
        }
      }
    }
  }
  bundleMetaCache.set(server, out);
  return out;
}

/** Whether the engine's own bundle authors any per-foe increment, read off its wire atoms
 *  (slot 24 = per_target). Independent of the legacy dataset and its atom readers. */
const bundlePerFoeCache = new Map<Server, boolean>();
function bundleHasPerFoeAtom(server: Server): boolean {
  const cached = bundlePerFoeCache.get(server);
  if (cached !== undefined) return cached;
  const { gunzipSync } = require('node:zlib') as typeof import('node:zlib');
  const bundle = JSON.parse(
    gunzipSync(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))).toString('utf8'),
  ) as Record<string, Record<string, { powers?: { atoms?: unknown[][] }[] }>>;
  const out = ['powersets', 'power-pools', 'epic-pools'].some((section) =>
    Object.values(bundle[section] ?? {})
      .flatMap((set) => set.powers ?? [])
      .flatMap((power) => power.atoms ?? [])
      .some((atom) => Array.isArray(atom) && atom.length > 24 && atom[24] != null),
  );
  bundlePerFoeCache.set(server, out);
  return out;
}

/** The slot paths in an effects bag that carry a per-foe increment (`defenseBuff`,
 *  `movement.runSpeed`, …), sorted — one dataset's answer to "which of this power's effects
 *  grow per foe". */
function perTargetSlots(effects: Record<string, unknown> | undefined): string[] {
  const out: string[] = [];
  for (const [key, slot] of Object.entries(effects ?? {})) {
    if (!slot || typeof slot !== 'object') continue;
    const record = slot as Record<string, unknown>;
    if (record.perTarget || record.maxHPFractionPerTarget) out.push(key);
    for (const [subKey, entry] of Object.entries(record)) {
      if (entry && typeof entry === 'object' && (entry as { perTarget?: number }).perTarget) {
        out.push(`${key}.${subKey}`);
      }
    }
  }
  return out.sort();
}

/** `null` when the two datasets agree which of this power's slots grow per foe, or when the
 *  legacy bag is absent: bag-vs-bag is unrunnable post-strip, and by then the response
 *  comparison grades both sides atom-fed. Otherwise a description of the disagreement. */
function perTargetDrift(server: Server, powersetId: string, internalName: string, legacy: Power): string | null {
  const legacyEffects = (legacy as unknown as { effects?: Record<string, unknown> }).effects;
  if (legacyEffects == null) return null;
  const engineSlots = perTargetSlots(bundleEffects(server).get(`${powersetId}\0${internalName}`));
  const legacySlots = perTargetSlots(legacyEffects);
  if (engineSlots.join(',') === legacySlots.join(',')) return null;
  return `legacy grows [${legacySlots.join(', ')}] per foe, engine dataset grows [${engineSlots.join(', ') || 'none'}]`;
}

function engineHandle(server: Server) {
  const cached = handles.get(server);
  if (cached) return cached;
  const handle = nodeEngine!.load_dataset(new Uint8Array(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))));
  handles.set(server, handle);
  return handle;
}

// The same defaults calculateCharacterTotals assembles when the hook passes no options.
const CTX: AdapterCalcContext = {
  exemplarMode: false,
  exemplarLevel: 50,
  incarnateActive: { alpha: false, destiny: false, hybrid: false, interface: false, judgement: false, lore: false, genesis: false },
  incarnateLevelShift: null,
  targetsHitValues: {},
  targetLevelOffset: 0,
  vigilanceTeamSize: 0,
  furyLevel: 75,
  combatMode: false,
  destinyTime: null,
  hybridTargetsHit: null,
  globalAdjusters: {},
  mechanicAdjusters: {},
  dominationActive: false,
  stalkerHidden: false,
  whatIfBuffs: {},
};

/** A power is pickable at 50 when it unlocks by level (available 0-indexed, < 50) and isn't
 *  a prerequisite-gated hidden entry (available < 0). */
function pickable(p: Power): boolean {
  return p.available >= 0 && p.available < 50;
}

/** Slot up to three distinct valid generic IOs (L50) so the enhancement-value + ED path fires;
 *  a power with no common-IO slot (auto/passive) stays unslotted. */
function slotsFor(power: Power): (SelectedPower['slots'][number])[] {
  const stats = getAvailableGenericIOs(power).slice(0, 3);
  return stats.map((stat) => createGenericIOEnhancement(stat, 50));
}

/** Build the richest data-driven build a fork offers without naming anything: the first
 *  standard archetype's first primary + secondary set, every level-≤50 power selected, toggles
 *  and autos active, each power slotted with valid generic IOs. Exercises the fork's base
 *  tables, active-power bonuses, purple patch, movement, and ED — the fork-specific surfaces. */
function buildFor(
  server: Server,
  atId: (typeof STANDARD_ARCHETYPE_IDS)[number] = STANDARD_ARCHETYPE_IDS[0],
  level = 50,
  slotted = true,
): Build {
  const build = createEmptyBuild(server);
  build.level = level;
  // The Archetype object carries no `id` (it lives in the registry key), so drive off the id.
  const at = getArchetype(atId);
  build.archetype = { id: atId, name: at?.name ?? atId, stats: null, inherent: null } as Build['archetype'];

  const setsFor = (category: string) =>
    getPowersetsForArchetype(atId).filter(
      (ps) => (ps.category ?? '').toLowerCase() === category && !ps.dormant,
    );

  const selectFrom = (powersetId: string, powers: Power[]): SelectedPower[] =>
    powers.filter(pickable).map((p) => ({
      ...p,
      powerSet: powersetId,
      level: p.available + 1,
      slots: slotted ? slotsFor(p) : [],
      isActive: p.powerType === 'Toggle' || p.powerType === 'Auto',
    }));

  const primary = setsFor('primary')[0];
  const secondary = setsFor('secondary')[0];
  if (primary) {
    build.primary = { id: primary.id!, name: primary.name, powers: selectFrom(primary.id!, primary.powers) };
  }
  if (secondary) {
    build.secondary = { id: secondary.id!, name: secondary.name, powers: selectFrom(secondary.id!, secondary.powers) };
  }
  return build;
}

function engineResult(server: Server, build: Build, ctx: AdapterCalcContext = CTX) {
  const json = engineHandle(server).recalculate(toCharacterStateJson(withoutIllegalSlots(build), ctx));
  const totals = JSON.parse(json) as EngineTotals;
  return { stats: mapStats(totals.stats, totals.bonuses), global: mapGlobal(totals.bonuses, totals.stats), errors: totals.bonuses.errors ?? [] };
}

// The engine computes in f32; the legacy calc in f64 JS numbers. Widening a summed f32 total back
// to f64 leaves error up to a few hundredths on values in the hundreds (e.g. recovery 71.19 reads
// as 71.1899995803833). A dashboard total apart by more than this is a real disagreement, not that.
const F32_TOLERANCE = 0.05;

/** The sub-50 level the PROD6B-2c check resolves at — any level whose AT-table rows differ from
 *  the level-50 ones; mid-range keeps the movement well clear of [`F32_TOLERANCE`]. */
const SWEEP_LEVEL = 25;

// GlobalBonuses fields `mapGlobal` deliberately hardcodes to 0 — they have no engine source and
// feed no dashboard total (see engineTotalsMap.ts). The legacy calc still fills them, so a
// difference here is expected and off-scope for parity.
//
// `toggleEndCost`/`netEndPerSec` were here too, and should never have been: they feed the
// dashboard's END COST and NET END, so excusing them hid a live 0 on every build from the
// engine swap (2026-07-23) until the engine grew them. They are graded now — this list is for
// fields with no consumer, not for fields with no implementation.
// `debuffResistAccuracy`/`debuffResistRange`: engine-only fields (DEBUFFRES-1). The legacy calc's
// `debuffResMapping` names neither key, so it reads 0 wherever the export states one — Light
// Affinity's accuracy, Regeneration's Revive/Dismiss Pain's range. Nothing to compare, so they sit
// here rather than under ADJUDICATED; the engine side is graded in the rebuild by
// `route_sweep::accuracy_and_range_debuff_resistance_reach_their_totals`.
const UNMAPPED = new Set([
  'threatLevel', 'protRepel', 'enduranceDiscount',
  'debuffResistAccuracy', 'debuffResistRange',
]);

// Stats (lowercased key) where the engine intentionally supersedes the legacy calc — the engine
// reads a faithful effect the beta's older converter drops. These are validated, not defects:
//   - homecoming `recharge`: Beta_Decay's per-target self-recharge (+12.5%). The engine models the
//     caster buff (its `scale` base); the legacy calc omits it entirely.
//   - thunderspy `maxhp`: Metabolic_Aura's `maxHPBuff{scale:0.333, Melee_HealSelf}` (+3.33). The
//     rebuild export carries it; the beta `effects` for the same toggle has no maxHP key at all.
// (See PROD5 in docs/SIDEKICK-PRODUCTIONIZE-PLAN.md for the adjudication.)
//
// `absorb` is deliberately NOT listed. The engine clamps it to the archetype's per-level ceiling
// and the legacy calc has no ceiling at all, so the two part company the moment the cap binds —
// but no single-powerset fixture here comes close to it (the biggest is roughly a third of the
// smallest ceiling), so the comparison stays honest. A fixture that stacks absorb hard enough to
// bind would surface here, and the engine value is the right one.
const ADJUDICATED: Record<Server, Set<string>> = {
  homecoming: new Set(['recharge']),
  rebirth: new Set(),
  thunderspy: new Set(['maxhp']),
  // Brainstorm needs no exemption: measured 2026-08-22, engine and legacy agree on every
  // graded stat. Its generated tree came through the current converter, which carries the
  // per-target self-recharge homecoming's older tree drops.
  brainstorm: new Set(),
};

function diff(server: Server, a: Record<string, number>, b: Record<string, number>): string[] {
  const out: string[] = [];
  for (const key of Object.keys(a)) {
    if (UNMAPPED.has(key)) continue;
    if (ADJUDICATED[server].has(key.toLowerCase())) continue;
    const av = a[key] ?? 0;
    const bv = b[key] ?? 0;
    if (Math.abs(av - bv) > F32_TOLERANCE) out.push(`${key}: engine ${av} vs legacy ${bv}`);
  }
  return out;
}

const suite = artifactsReady ? describe : describe.skip;
if (!artifactsReady) {
  // eslint-disable-next-line no-console
  console.warn('[PROD5 parity] skipped — engine artifacts absent; run `npm run build:engine` and build the Node target (see file header).');
}

suite('PROD5 — engine vs legacy dashboard parity, per server', () => {
  beforeAll(() => {
    // Warm each engine handle once so a load failure surfaces here, not mid-assert.
    for (const s of SERVERS) engineHandle(s);
  });

  it.each(SERVERS)('%s: engine loads, errors clean, and matches the legacy calc', async (server) => {
    await loadDataset(server); // activates this dataset for the legacy TS calc
    const build = buildFor(server);

    // Guard the fixture itself: a build that selected nothing would trivially "match" at zero.
    const powerCount = build.primary.powers.length + build.secondary.powers.length;
    expect(powerCount, `${server}: fixture selected no powers`).toBeGreaterThan(0);

    const legacy = legacyCalculateCharacterTotals(build, false, undefined, {});
    const engine = engineResult(server, build);

    // BPORT13 floor: the oracle's ACTIVE-POWERS PASS has to have contributed. The fixture
    // guard above says the build selected powers; it says nothing about whether the oracle
    // then read them, and that is the failure this file actually had. BPORT7 emptied
    // `power.effects`, an `if (!power.effects) continue;` at the head of that pass turned from
    // a skip for atom-less legacy powers into a skip for every power on every fork, and the
    // oracle stopped grading anything. It went red only because the engine did not — had both
    // sides gone quiet together this would have reported a clean parity between two empty
    // results. A comparison needs a floor under each arm, not just under its input.
    //
    // "Non-zero totals" is too weak a floor to catch it: set bonuses, accolades and incarnates
    // are computed in other passes and keep the result non-zero with every power skipped. So
    // the floor is differential — the same build with its powers removed must produce a
    // DIFFERENT answer. That is the pass contributing, stated so it cannot be satisfied by any
    // other one.
    const stripped = buildFor(server);
    stripped.primary = { ...stripped.primary, powers: [] };
    stripped.secondary = { ...stripped.secondary, powers: [] };
    stripped.pools = [];
    stripped.epicPool = null;
    const legacyStripped = legacyCalculateCharacterTotals(stripped, false, undefined, {});
    const contributed = Object.entries({ ...legacy.stats, ...legacy.globalBonuses })
      .filter(([key]) => !UNMAPPED.has(key) && !ADJUDICATED[server].has(key.toLowerCase()))
      .filter(([key, v]) => {
        const w = ({ ...legacyStripped.stats, ...legacyStripped.globalBonuses } as Record<string, unknown>)[key];
        return typeof v === 'number' && typeof w === 'number' && Math.abs(v - w) > F32_TOLERANCE;
      });
    expect(
      contributed.length,
      `${server}: the legacy oracle's active-powers pass contributed nothing — it is not grading the engine`,
    ).toBeGreaterThan(0);

    // What this floor does NOT catch, stated because a floor whose reach is assumed is worse
    // than none. It was written for the BPORT7 regression and it does not detect it: with the
    // guard reinstated, `contributed` is still non-empty, because part of the pass runs ABOVE
    // the point the guard stood at — enhancement folding, toggle end costs, per-power set-bonus
    // collection — and that half keeps contributing while every buff arm below is skipped.
    // Measured, not assumed: reinstating the `continue` reds 11 of these 12 tests and this
    // assertion is not one of them.
    //
    // So the diff below remains the guard that catches a starved oracle, and this floor covers
    // only the stronger failure of the pass going wholly silent. A floor scoped to the buff
    // arms alone would be the right shape and needs a fixture that resolves inside the oracle's
    // own power lookup rather than the engine's — filed rather than guessed at.

    // Hard invariants: the per-serverId load path returned real, sane numbers.
    expect(engine.errors, `${server}: engine returned errors`).toEqual([]);
    for (const [key, value] of Object.entries({ ...engine.stats, ...engine.global })) {
      expect(Number.isFinite(value), `${server}: engine ${key} not finite (${value})`).toBe(true);
    }

    const statDeltas = diff(server, engine.stats as unknown as Record<string, number>, legacy.stats as unknown as Record<string, number>);
    const globalDeltas = diff(server, engine.global as unknown as Record<string, number>, legacy.globalBonuses as unknown as Record<string, number>);

    if (statDeltas.length || globalDeltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD5 parity] ${server} (${powerCount} powers)\n  stats:\n    ${statDeltas.join('\n    ')}\n  global:\n    ${globalDeltas.join('\n    ')}`);
    }
    expect({ statDeltas, globalDeltas }).toEqual({ statDeltas: [], globalDeltas: [] });
  }, 120000); // dataset load (large TS modules) + a full legacy calc; matches the suite's
  // --testTimeout so a bare `npx vitest run <file>` gets the same budget CI does. It read 30000
  // when the only thing to beat was vitest's 5s default, and once npm test started passing
  // 120000 that number stopped being a floor and became a ceiling below it — this test then
  // failed on a GitHub-hosted runner while passing everywhere faster.

  // PROD6B-2c. Mez protection used to read its `Res_Boolean` table at a fixed level 50 on both
  // sides, on the claim that protection doesn't scale while leveling. It does — the tables vary
  // by level and the game resolves them at the caster's combat level (`attribmod.c` `mod_Fill`;
  // no code branch anywhere special-cases `Res_Boolean`) — so a sub-50 build read roughly double
  // its real protection. The fixture above builds at 50, where a pinned read and a build-level
  // read agree, which is why no gate here could see it.
  //
  // This re-runs the parity diff at a sub-50 level and holds the property the pin violated: the
  // protection totals must MOVE with the build level. The archetype is chosen by measurement
  // rather than named — the first standard one whose fixture build carries any protection at all,
  // since attack-oriented archetypes carry none and would make the assertion vacuous.
  it.each(SERVERS)('%s: mez protection resolves at the build level, not a pinned one', async (server) => {
    await loadDataset(server);

    const protectionTotal = (global: Record<string, number>) =>
      Object.entries(global)
        .filter(([key]) => key.startsWith('prot') && !UNMAPPED.has(key))
        .reduce((sum, [, value]) => sum + value, 0);

    const globalsAt = (atId: (typeof STANDARD_ARCHETYPE_IDS)[number], level: number) => {
      const build = buildFor(server, atId, level, false);
      return {
        engine: engineResult(server, build).global as unknown as Record<string, number>,
        legacy: legacyCalculateCharacterTotals(build, false, undefined, {})
          .globalBonuses as unknown as Record<string, number>,
      };
    };

    // Chosen by measurement, not named: most archetypes' fixtures are attack sets carrying no
    // protection at all, which would make the movement assertion vacuous.
    const atId =
      STANDARD_ARCHETYPE_IDS.find((id) => protectionTotal(globalsAt(id, 50).engine) > 0) ??
      STANDARD_ARCHETYPE_IDS[0];

    const high = globalsAt(atId, 50);
    const low = globalsAt(atId, SWEEP_LEVEL);
    const highProtection = protectionTotal(high.engine);
    const lowProtection = protectionTotal(low.engine);

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6B-2c] ${server}: ${atId} protection ${highProtection.toFixed(2)} @50 → ${lowProtection.toFixed(2)} @${SWEEP_LEVEL}`,
    );

    // This fixture is a SECOND archetype (and unslotted), so it exercises data the level-50 test
    // above never reaches — it is what surfaced the per-foe stacking defect PROD6B-2d fixed. Both
    // levels must now agree with the legacy calc outright; a level-50 baseline is deliberately
    // NOT subtracted here, since that is exactly the shape that would let a fresh divergence hide.
    const deltas = [...diff(server, high.engine, high.legacy), ...diff(server, low.engine, low.legacy)];

    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6B-2c] ${server} deltas at levels 50/${SWEEP_LEVEL}\n    ${deltas.join('\n    ')}`);
    }
    expect(deltas).toEqual([]);

    // A fork whose fixture carries no protection can't demonstrate the movement — say so rather
    // than pass quietly.
    if (highProtection === 0) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6B-2c] ${server}: no standard archetype's fixture carries mez protection — movement unverified on this fork`);
      return;
    }
    expect(lowProtection, `${server}: protection did not scale with the build level`).toBeLessThan(highProtection - F32_TOLERANCE);
  }, 60000);

  // PROD6B-2d. A per-foe aura (Invincibility's +Def, Evolving Armor's +Res, Against All Odds'
  // +Damage) reaches the caster only THROUGH a target — its `EntsAffected` lists foes alone, so
  // with nobody in radius no effect block runs. The engine skipped the stacking wrap on the
  // defense and resistance families and read an absent count as the one-target value, so it
  // credited a phantom first foe and then never moved off it, whatever the slider said.
  //
  // Every fixture above passes `targetsHitValues: {}`, which is the identity on both calcs for
  // every OTHER family — that is why the defect survived. This drives the count instead, and
  // grades the RESPONSE (totals at N minus totals at 0) rather than the absolute totals, so a
  // constant fixture difference cancels and only slider handling is under test.
  //
  // Widened by PROD6C-3j, which the walk as first written was structurally blind to: it carried
  // no absorb reader, and it filtered to Toggle/Auto while every power whose absorb grows per
  // foe (Parasitic Aura / Leech) is a Click. Both halves of that blindness are lifted here.
  // BPORT7 A3 re-keyed absorb discovery from the legacy bag (the BPORT7 regen empties it) to
  // the Absorb atom's `perTarget` stamp, and excluded the per-foe absorb channel itself as the
  // named ABSORB-4 residual: the atom-fed oracle reads absorb flat (no reader returns
  // `perTarget`) while the engine grows it per foe, so key-comparing absorb grades two
  // channels, not two calcs. The exclusion mirrors the engine's
  // `per_target: derived.or(atom_per_target)`: the atom stamp, or the derived gate (frac probe
  // answers, bundle effectArea AoE/Cone, 1 < stats.maxTargets < 255). Excluded powers are
  // logged unverified, so the residual is watched, not hidden.
  it.each(SERVERS)('%s: per-foe buffs track the targets-hit count identically on both calcs', async (server) => {
    await loadDataset(server);

    const READERS = [toHitBuffValue, damageBuffValue, resistanceBuffValue, defenseBuffValue, defenseBuffSuppressibleValue, regenBuffValue, recoveryBuffValue];
    // BPORT7 A3: the absorb arm used to read the legacy bag (`effects.absorb.perTarget`),
    // which the BPORT7 regen empties. It now reads the Absorb atom's `perTarget` stamp, the
    // data the engine's `per_target` field is built from, so discovery survives the regen.
    // Measured a no-op on the examined set: every bag-discovered power is already found by
    // the readers arm (Recovery/Regeneration stamps), 74/69/71/74 pre- and post-rekey.
    const carriesAbsorbPerFoe = (power: Power) =>
      baseAtoms(power).some((a) => a.effectType === 'Absorb' && a.perTarget);
    const carriesPerTarget = (power: Power) => {
      if (carriesAbsorbPerFoe(power)) return true;
      return READERS.some((read) => {
        const value = read(power as never) as unknown;
        if (!value || typeof value !== 'object') return false;
        const entries = typeof (value as { scale?: number }).scale === 'number'
          ? [value as { perTarget?: number }]
          : Object.values(value as Record<string, { perTarget?: number }>);
        return entries.some((entry) => entry && !!entry.perTarget);
      });
    };

    // Powerset keys are `<archetypeId>/<setId>`, so the prefix arrives as a bare string; this is
    // the one place it re-enters the typed registry.
    const archetypeOf = (id: string) => getArchetype(id as (typeof STANDARD_ARCHETYPE_IDS)[number]);

    /** A build carrying exactly one power, forced active and unslotted. */
    const solo = (atId: string, powersetId: string, power: Power): Build => {
      const build = createEmptyBuild(server);
      build.level = 50;
      const at = archetypeOf(atId);
      build.archetype = { id: atId, name: at?.name ?? atId, stats: null, inherent: null } as Build['archetype'];
      build.primary = {
        id: powersetId,
        name: powersetId,
        powers: [{ ...power, powerSet: powersetId, level: Math.max(1, power.available + 1), slots: [], isActive: true } as SelectedPower],
      };
      return build;
    };

    const totalsAt = (build: Build, power: Power, targets: number | null) => {
      const targetsHitValues = targets === null ? {} : { [power.internalName]: targets };
      const ctx = { ...CTX, targetsHitValues };
      const engine = engineResult(server, build, ctx);
      const legacy = legacyCalculateCharacterTotals(build, false, undefined, { targetsHitValues });
      return {
        engine: { ...engine.stats, ...engine.global } as unknown as Record<string, number>,
        legacy: { ...legacy.stats, ...legacy.globalBonuses } as unknown as Record<string, number>,
      };
    };

    const mismatches: string[] = [];
    const adjudicated: string[] = [];
    const unverified: string[] = [];
    let examined = 0;
    let responsive = 0;

    for (const [powersetKey, powerset] of Object.entries(getAllPowersets()) as [string, { id?: string; powers?: Power[] }][]) {
      const atId = powersetKey.split('/')[0];
      if (!archetypeOf(atId)) continue;
      for (const power of powerset.powers ?? []) {
        // Click powers join Toggle/Auto here (PROD6C-3j): `solo` forces the power active, which
        // is what puts a click's buff in the totals, and the per-foe absorbs live only on clicks.
        if (power.powerType !== 'Toggle' && power.powerType !== 'Auto' && power.powerType !== 'Click') continue;
        if (!carriesPerTarget(power)) continue;
        // The corpus above is discovered from the LEGACY dataset's increments; a power whose
        // datasets DISAGREE about which slots grow per foe is dataset drift, not a slider
        // defect — the legacy converter invented an increment on rows the export states flat
        // (a Defiance rider read as per-foe damage), while the contract's `per_target` stamp
        // is restricted to the export's real increments (PERFOE-1). Logged and skipped whole:
        // one drifted slot pollutes the power's response totals. Excluded from `examined` so
        // the anti-vacuity counters keep counting only powers both sides can actually grade.
        const drift = perTargetDrift(server, powerset.id ?? powersetKey, power.internalName, power);
        if (drift) {
          adjudicated.push(`${powersetKey}/${power.name}: ${drift} — dataset drift, response comparison skipped`);
          continue;
        }
        // BPORT7 A3: the per-foe absorb channel is the named ABSORB-4 residual. The atom-fed
        // oracle reads absorb flat while the engine grows it per foe (atom stamp or derived
        // bundle gate), so the absorb key compares two channels, not two calcs. Drop it and
        // log the power unverified; the exclusion mirrors the engine's own gate, so it stays
        // live across the BPORT7 regen.
        const meta = bundleMeta(server).get(`${powerset.id ?? powersetKey}\0${power.internalName}`);
        const absorbUnverified =
          carriesAbsorbPerFoe(power) ||
          (absorbMaxHPFractionValue(power) != null &&
            (meta?.effectArea === 'AoE' || meta?.effectArea === 'Cone') &&
            (meta?.maxTargets ?? 0) > 1 && (meta?.maxTargets ?? 0) !== 255);
        if (absorbUnverified) {
          unverified.push(`${powersetKey}/${power.name}: absorb key excluded, per-foe channel ungraded (ABSORB-4 residual)`);
        }
        examined++;

        const build = solo(atId, powerset.id ?? powersetKey, power);
        const zero = totalsAt(build, power, 0);
        for (const targets of [null, 1, 5] as const) {
          const at = totalsAt(build, power, targets);
          // The RESPONSE to the count, not the totals themselves.
          const keys = Object.keys(at.engine).filter((key) => !UNMAPPED.has(key) && !(absorbUnverified && key === 'absorb'));
          const engineMoved = keys.filter((key) => Math.abs((at.engine[key] ?? 0) - (zero.engine[key] ?? 0)) > F32_TOLERANCE);
          const legacyMoved = keys.filter((key) => Math.abs((at.legacy[key] ?? 0) - (zero.legacy[key] ?? 0)) > F32_TOLERANCE);
          if (targets !== null && engineMoved.length) responsive++;
          for (const key of new Set([...engineMoved, ...legacyMoved])) {
            const engineDelta = (at.engine[key] ?? 0) - (zero.engine[key] ?? 0);
            const legacyDelta = (at.legacy[key] ?? 0) - (zero.legacy[key] ?? 0);
            if (Math.abs(engineDelta - legacyDelta) > F32_TOLERANCE) {
              mismatches.push(`${powersetKey}/${power.name} N=${targets ?? 'absent'} ${key}: engine ${engineDelta.toFixed(3)} vs legacy ${legacyDelta.toFixed(3)}`);
            }
          }
        }
      }
    }

    // Whether this fork carries per-foe data at all, read off the ENGINE's own bundle atoms
    // (wire slot 24 = per_target) — an oracle independent of the legacy dataset and its
    // readers, so a reader that silently stopped matching cannot also silence the guard
    // below. The bag this used to be read off leaves at BPORT8; the stamps survive both
    // regens (577/501/615/577 stamped atoms across the four forks, measured pre-regen).
    const forkHasPerFoeData = bundleHasPerFoeAtom(server);

    // eslint-disable-next-line no-console
    console.warn(`[PROD6B-2d] ${server}: ${examined} per-foe powers, ${responsive} slider responses observed`);
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6B-2d] ${server} dataset drift (accepted, ${adjudicated.length}):\n    ${adjudicated.join('\n    ')}`);
    }
    if (unverified.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6B-2d] ${server} per-foe absorb (ABSORB-4 residual, ${unverified.length}):\n    ${unverified.join('\n    ')}`);
    }

    if (mismatches.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6B-2d] ${server}\n    ${mismatches.slice(0, 20).join('\n    ')}${mismatches.length > 20 ? `\n    …+${mismatches.length - 20} more` : ''}`);
    }
    expect(mismatches).toEqual([]);

    // Report rather than assert when the fork's engine bundle authors no per_target stamp:
    // the absence might be an honest rebalance, not a converter drop, and no authored def
    // settles it.
    if (!forkHasPerFoeData) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6B-2d] ${server}: fork exports no per-foe increment on any power — slider handling unverified here`);
      return;
    }
    // Anti-vacuous: where the data DOES carry per-foe values, the walk must find powers and
    // observe them actually respond, or a reader break would read as a clean pass.
    expect(examined, `${server}: fork carries per-foe data but the walk found no toggle/auto power`).toBeGreaterThan(0);
    expect(responsive, `${server}: no power responded to the targets-hit count`).toBeGreaterThan(0);
  }, 180000);
});
