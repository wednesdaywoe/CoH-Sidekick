/**
 * PROD6B-1/6B-2 — per-power projection fidelity gate.
 *
 * The engine's new `power_projection` block must equal the beta's own per-power calculators
 * (`calculatePowerEnhancementBonuses` + `calcThreeTier` + `calculateArcanaTime` +
 * `calculatePermaInfo`) that PROD6C–E will retire. This drives all three forks through the Rust
 * engine (wasm-node) on the SAME data-driven build the PROD5 dashboard gate uses (no fork proper
 * nouns — Rule 0), then diffs the engine's resolved execution three-tiers + ArcanaTime + perma
 * against the beta calculators, power by power. A row here means the engine and the TS calc
 * disagree on a fork's per-power math — exactly what must not ship before 6C swaps the surfaces.
 *
 * PROD6B-2 adds the granted buff/debuff magnitudes to the same diff. Their beta reference is
 * `resolvePowerMagnitudes` — the pure function extracted from `RegistryEffectsDisplay`, which
 * that component now calls (decision 2026-07-24, user-chosen: extract the real resolver rather
 * than reimplement it in the test, so the gate grades the code path the UI actually runs).
 *
 * PROD6B-2c adds the level sweep. Every fixture here builds at level 50, so a magnitude resolved
 * at a pinned 50 and one resolved at the build level looked identical to all of the above — the
 * blindness that let the pin through 6B-2. The last test re-runs the magnitude diff at a level
 * where the two answers differ.
 *
 * The engine runs via wasm-node (the browser `engine.ts` has no Node path), same as
 * `serverParity.test.ts`. The fixture is deliberately parallel to that gate but kept local so the
 * committed dashboard gate stays untouched.
 */

import { describe, it, expect, beforeAll } from 'vitest';
import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { join } from 'node:path';
import { loadDataset } from '@/data/dataset';
import { getArchetype, STANDARD_ARCHETYPE_IDS } from '@/data/archetypes';
import { getPowersetsForArchetype } from '@/data/powersets';
import { getAllPowerPools } from '@/data/power-pools';
import { getAllEpicPools } from '@/data/epic-pools';
import { getAvailableGenericIOs, createGenericIOEnhancement } from '@/data/enhancement-registry';
import { getIOSet } from '@/data/io-sets';
import { withoutIllegalSlots } from '@/utils/build-enhancement-validation';
import { createEmptyBuild } from '@/types/build';
import { applyExemplarScaling, calculatePowerEnhancementBonuses, combineWithAlphaED, normalizeAspectName, type EnhancementBonuses } from '@/utils/calculations/enhancement-values';
import { getAlphaEnhancementBonuses, getAlphaEdBypassBonuses } from '@/utils/calculations/character-totals';
import { areIncarnatesSuppressed, INCARNATE_MIN_LEVEL } from '@/utils/calculations/effective-level';
import { getIncarnateTrees } from '@/data/incarnates';
import { calculatePermaInfo, type PermaInfo } from '@/utils/calculations/perma';
import { getRechargeBounds } from '@/data/at-tables';
import { atomsOf, baseAtoms, mezSlotValue } from '@/data/core/atom-query';
import { landsOnCaster } from '@/data/core/atomic-effect';
import { calculateArcanaTime } from '@/utils/calculations/damage';
import { calcThreeTier, convertGlobalBonusesToAspects, withStrengthBonuses, type ThreeTierValues } from '@/components/info/powerDisplayUtils';
import { resolvePowerMagnitudes, type ResolvedMagnitude } from '@/components/info/resolvePowerMagnitudes';
import { magnitudesFromProjection } from '@/components/info/magnitudesFromProjection';
import { powerLevelDuration, type BagDurations } from '@/components/info/effectRowDuration';
import { EFFECT_REGISTRY } from '@/data/core/effect-registry';
import { buildDisplayEffects, getStackingInfo, withPseudoPetEffects, withTargetsHit } from '@/components/info/buildDisplayEffects';
import { resolveEffectivePower, effectiveGlobalAdjusters, isCasterHidden, currentToHitFraction, type EffectivePowerState } from '@/components/info/resolveEffectivePower';
import { selectableModes } from '@/utils/mode-suppression';
import { toCharacterStateJson, type AdapterCalcContext } from './characterStateAdapter';
import { BASE_ROW_OVER_ADDITIVE_FILL } from './powerProjectionFillPin';
import { mapGlobal, mapOnePowerProjection, mapPowerProjection, projectionKey, type EnginePowerProjection, type EngineTotals, type GrantedMagnitude, type PowerProjection } from './engineTotalsMap';
import type { Build } from '@/types/build';
import type { Power, PowerEffects, SelectedPower } from '@/types/power';
import type { SelectedIncarnatePower } from '@/types/incarnate';

const require = createRequire(import.meta.url);

const SERVERS = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'] as const;
type Server = (typeof SERVERS)[number];

const NODE_ENGINE = join(__dirname, 'wasm-node', 'coh_wasm.cjs');
const BUNDLE_DIR = join(__dirname, '..', '..', 'public', 'engine', 'contract');
const artifactsReady =
  existsSync(NODE_ENGINE) && SERVERS.every((s) => existsSync(join(BUNDLE_DIR, `${s}.json.gz`)));

type EngineHandle = {
  recalculate: (json: string) => string;
  project_power: (json: string, powerSet: string, internalName: string, targetsHit?: number) => string;
};
const nodeEngine = artifactsReady
  ? (require('./wasm-node/coh_wasm.cjs') as { load_dataset: (bytes: Uint8Array) => EngineHandle })
  : null;

const handles = new Map<Server, EngineHandle>();
function engineHandle(server: Server) {
  const cached = handles.get(server);
  if (cached) return cached;
  const handle = nodeEngine!.load_dataset(new Uint8Array(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))));
  handles.set(server, handle);
  return handle;
}

// The same defaults the totals hook assembles with no options — no incarnates, even level.
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

function pickable(p: Power): boolean {
  return p.available >= 0 && p.available < 50;
}

function slotsFor(power: Power): SelectedPower['slots'] {
  const stats = getAvailableGenericIOs(power).slice(0, 3);
  return stats.map((stat) => createGenericIOEnhancement(stat, 50));
}

/** The richest data-driven build a fork offers, assembled with no proper nouns (mirrors the PROD5
 *  gate): first standard AT's first primary + secondary, every ≤50 power, three generic IOs each.
 *
 *  `level` drives the AT-table reads on both sides. `slotted: false` is for the sub-50 sweep: the
 *  fixture's IOs are level-50 ones, which `withoutIllegalSlots` strips out of the ENGINE's input at
 *  a lower build level while the beta reference still reads them off the build — an asymmetry in
 *  the fixture, not in the math under test. Unslotted, both sides see the same slots. */
function buildFor(
  server: Server,
  atId: (typeof STANDARD_ARCHETYPE_IDS)[number] = STANDARD_ARCHETYPE_IDS[0],
  level = 50,
  slotted = true,
  chooseSet: (sets: { powers: Power[] }[]) => number = () => 0,
): Build {
  const build = createEmptyBuild(server);
  build.level = level;
  const at = getArchetype(atId);
  build.archetype = { id: atId, name: at?.name ?? atId, stats: null, inherent: null } as Build['archetype'];

  const setsFor = (category: string) =>
    getPowersetsForArchetype(atId).filter((ps) => (ps.category ?? '').toLowerCase() === category && !ps.dormant);
  const selectFrom = (powersetId: string, powers: Power[]): SelectedPower[] =>
    powers.filter(pickable).map((p) => ({
      ...p,
      powerSet: powersetId,
      level: p.available + 1,
      slots: slotted ? slotsFor(p) : [],
      isActive: p.powerType === 'Toggle' || p.powerType === 'Auto',
    }));

  // Which of the archetype's sets to build from. The default first-set pick is the 6B fixture
  // every sweep here shares; a caller that needs a corpus carrying some specific transform picks
  // by measuring the sets' own data instead (PROD6C-3k).
  const primarySets = setsFor('primary');
  const secondarySets = setsFor('secondary');
  const primary = primarySets[chooseSet(primarySets)];
  const secondary = secondarySets[chooseSet(secondarySets)];
  if (primary) build.primary = { id: primary.id!, name: primary.name, powers: selectFrom(primary.id!, primary.powers) };
  if (secondary) build.secondary = { id: secondary.id!, name: secondary.name, powers: selectFrom(secondary.id!, secondary.powers) };
  return build;
}

/** Does the effective-power resolution have anything to do on this power — a quick form, a
 *  mid-combat cast, or a conditional contribution (PROD6C-3k)? Read off the power's own fields so
 *  the fixture discovers its corpus rather than naming a set. */
function carriesTransform(power: Power): boolean {
  const p = power as unknown as { quickSnipe?: unknown; midCombatCast?: unknown; conditionalEffects?: unknown[] };
  return p.quickSnipe != null || p.midCombatCast != null || (p.conditionalEffects?.length ?? 0) > 0;
}

/** Does this power's fast-snipe form read the CASTER's own ToHit (SNIPE-2), the way canonical's
 *  `effective::fast_form_reads_caster_to_hit` reads it — structurally (the pair of tokens),
 *  not by matching the fork. Rebirth/Thunderspy's `cur.kToHit source> .97 >=` is a state the
 *  `combatMode` toggle alone can never move; only a build whose ToHit crosses 97% can, which
 *  this file's fixtures don't attempt to construct. Homecoming's `kEngaged Source.Mode? …`
 *  reads no such pair, so the toggle IS its reach signal. Used to keep the anti-vacuity
 *  `castMoved` check from asserting a toggle can move a state it structurally cannot. */
function quickSnipeReadsCasterToHit(power: Power): boolean {
  // The wire carries the condition as a token array (COND-8) — read it as one.
  const tokens = (power as unknown as { quickSnipe?: { condition?: string[] } }).quickSnipe?.condition ?? [];
  return tokens.some((t, i) => t.toLowerCase() === 'cur.ktohit' && tokens[i + 1]?.toLowerCase() === 'source>');
}

/** Does this power summon an entity? Read off `power.summon`, the whole-power verdict the
 *  converter stamps (ENT-22) — never re-derived from `EntCreate` atoms, which state the
 *  entity's own lifespan and not whether the power has one. */
function carriesSummon(power: Power): boolean {
  return (power as unknown as { summon?: unknown }).summon != null;
}

/** Does this power redirect on a caster mode (PROD6C-3l)? Read off the power's own table so the
 *  fixture discovers which archetypes have modes instead of naming any. */
function carriesModeVariant(power: Power): boolean {
  return Object.keys((power as unknown as { modeVariants?: Record<string, unknown> }).modeVariants ?? {}).length > 0;
}

/** Does this power carry a +Strength self-buff at all? Read off the bag key the Strength pass
 *  consumes on both sides, so the fixture below discovers its own carriers from the fork's data
 *  rather than naming any (Rule 0). */
/** Does this power grant +Strength — the Power Boost family the PROD6C fold reads?
 *
 *  Was `effects.specialBuff != null`. STRIP-1 emptied `effects`, so that read returned false for
 *  every power on every fork, `strengthCandidates` found none, and the body below returned at its
 *  first line — green, having graded nothing. It never went red, which makes it the worse half of
 *  the same defect the 36 loud failures came from (PROD6B-BETA-PARITY class 1).
 *
 *  The atom source: `specialBuff` is the bag slot `aspect: Str` atoms route to, minus the
 *  `DamageBuff` family, which routes to `damageBuff` instead. This is deliberately wider than that
 *  routing, because it only has to NOMINATE candidates — the caller then runs each through the
 *  engine and keeps the first whose `strength*` lands in a family the fold actually reads, which
 *  is the step written precisely so this predicate need not replicate the routing. */
function grantsStrength(power: Power): boolean {
  return baseAtoms(power).some((a) => a.aspect === 'Str' && landsOnCaster(a));
}

/** Candidate builds whose ACTIVE powers grant +Strength (the Power Boost family), for the
 *  PROD6C fold. `buildFor` activates only toggles and autos, and these are clicks, so no fixture
 *  above can produce a non-zero `strength*` — which is exactly what made the fold invisible to
 *  this gate.
 *
 *  Discovered rather than named (Rule 0): every archetype powerset carrying a `specialBuff`
 *  power becomes a candidate, with all its pickable powers selected and its carriers switched
 *  on. Only some Strength aspects reach a display row — the caller runs each candidate through
 *  the engine and keeps the first whose `strength*` lands in a family the fold reads, rather
 *  than this replicating the bag's aspect routing to guess. */
function strengthCandidates(server: Server): { build: Build; atId: string; carriers: string[] }[] {
  const out: { build: Build; atId: string; carriers: string[] }[] = [];
  for (const atId of STANDARD_ARCHETYPE_IDS) {
    for (const set of getPowersetsForArchetype(atId)) {
      if (set.dormant || !set.id) continue;
      const pickables = (set.powers ?? []).filter(pickable);
      const carriers = pickables.filter(grantsStrength);
      if (carriers.length === 0) continue;

      const build = createEmptyBuild(server);
      build.level = 50;
      const at = getArchetype(atId);
      build.archetype = { id: atId, name: at?.name ?? atId, stats: null, inherent: null } as Build['archetype'];
      const selected: SelectedPower[] = pickables.map((p) => ({
        ...p,
        powerSet: set.id!,
        level: p.available + 1,
        slots: slotsFor(p),
        isActive: grantsStrength(p) || p.powerType === 'Toggle' || p.powerType === 'Auto',
      }));
      const placed = { id: set.id, name: set.name, powers: selected };
      if ((set.category ?? '').toLowerCase() === 'secondary') build.secondary = placed;
      else build.primary = placed;
      // `buildStore`'s toggle handler keeps `activeModes` in sync with every active power's own
      // `setsModes` (a click like Power Boost sets `kBoostPower` the moment it's switched on —
      // see `mode-suppression.ts`'s `computeModeSuppression`). This fixture builds the Build by
      // hand rather than through the store, so it has to do that same sync itself, or a carrier
      // that ALSO sets a mode (Power Boost redirects Stun via `kBoostPower Source.Mode?`) would
      // look inactive to every mode/form-variant read below.
      build.activeModes = [...new Set(selected.filter((p) => p.isActive).flatMap((p) => p.setsModes ?? []))];
      out.push({ build, atId, carriers: carriers.map((c) => c.internalName) });
    }
  }
  return out;
}

/** An equipped Alpha and everything both sides need to agree about it: the build selection the
 *  adapter forwards, the beta's aspect-keyed bonuses, and the per-aspect slice of them that
 *  bypasses ED — both read from the export through the beta's own accessors. */
interface AlphaFixture {
  selection: SelectedIncarnatePower;
  bonuses: EnhancementBonuses;
  bypass: EnhancementBonuses;
  aspects: string[];
}

/** Every Alpha a fork offers, richest first — discovered from the fork's own incarnate data
 *  (Rule 0: no power is named here). "Richest" is the most non-zero enhancement aspects, so the ED
 *  split is exercised across as many schedules as the fork has; ties break on the power's full name
 *  so the fixture is deterministic.
 *
 *  `levelShift` is deliberately not counted: it is not an enhancement aspect and
 *  `getAlphaEnhancementBonuses` maps it nowhere. */
function alphaFixturesFor(): AlphaFixture[] {
  const out: AlphaFixture[] = [];
  for (const tree of getIncarnateTrees('alpha')) {
    for (const power of tree.powers) {
      const incarnates = { alpha: { powerId: power.fullName } } as unknown as Parameters<typeof getAlphaEnhancementBonuses>[0];
      const active = { alpha: true } as Parameters<typeof getAlphaEnhancementBonuses>[1];
      const bonuses = getAlphaEnhancementBonuses(incarnates, active);
      const bypass = getAlphaEdBypassBonuses(incarnates, active);
      const aspects = Object.entries(bonuses)
        .filter(([, value]) => value !== undefined && value !== 0)
        .map(([aspect]) => aspect)
        .sort();
      if (aspects.length === 0) continue;

      out.push({
        selection: {
          slotId: 'alpha',
          powerId: power.fullName,
          powerName: power.fullName,
          displayName: power.displayName,
          icon: power.icon,
          tier: power.tier,
          treeId: tree.id,
          treeName: tree.name,
        },
        bonuses,
        bypass,
        aspects,
      });
    }
  }
  return out.sort(
    (a, b) => b.aspects.length - a.aspects.length || a.selection.powerId.localeCompare(b.selection.powerId),
  );
}

/** The exemplar bonus cap both sides now read — `exemplar_handicaps.bin`'s `preClamp`, indexed
 *  `curve[level - 1]` the way `boost.c`'s `boost_HandicapExemplar` indexes it. Taken from the
 *  engine bundle and compared against what `applyExemplarScaling` produces (a raw bonus of 1.0 at
 *  IO level 50 can only come back as the cap), so a beta that stopped reading the curve fails
 *  before the parity rows below can pass by agreeing on the wrong number. */
function assertSharedExemplarCap(server: Server, exemplarLevel: number): number {
  const bundle = JSON.parse(
    gunzipSync(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))).toString('utf8'),
  ) as { 'enhancement-curves'?: { exemplarHandicaps?: { preClamp?: number[] } } };
  const curve = bundle['enhancement-curves']?.exemplarHandicaps?.preClamp ?? [];
  const index = Math.min(Math.max(exemplarLevel, 1), curve.length) - 1;
  const exported = curve[index];
  expect(exported, `${server}: the engine bundle carries no exemplar preClamp curve`).toBeDefined();
  expect(
    applyExemplarScaling(1.0, 50, exemplarLevel),
    `${server}: the beta's exemplar cap at ${exemplarLevel} is not the exported preClamp — it has stopped reading exemplar_handicaps.bin`,
  ).toBeCloseTo(exported, 6);
  return exported;
}

/** The power the ENGINE actually reads, keyed the way the engine addresses it — the aggregate's
 *  `id` plus the power's own `internalName`, or for the pool/epic partitions (which carry
 *  neither) the `fullName`-derived identity `coh_data` normalizes to at load.
 *
 *  This is the evidence side of the PROD6C-3h adjudications. The two repos' converters have
 *  drifted in BOTH directions ([[prod6b2a-absorb-stdresult]]), so a row the beta resolves and the
 *  engine does not — or one whose two sides read different authored VALUES — is only acceptable
 *  when the engine's own copy of the power proves it. Reading the bundle proves that rather than
 *  assuming it. */
type BundlePower = { effects?: Record<string, unknown>; strengthsDisallowed?: string[]; globalStrengthsDisallowed?: string[] };
const bundlePartitionCache = new Map<Server, Map<string, BundlePower>>();
function bundlePowers(server: Server): Map<string, BundlePower> {
  const cached = bundlePartitionCache.get(server);
  if (cached) return cached;
  const bundle = JSON.parse(
    gunzipSync(readFileSync(join(BUNDLE_DIR, `${server}.json.gz`))).toString('utf8'),
  ) as Record<string, Record<string, { id?: string; powers?: (BundlePower & { name?: string; internalName?: string; fullName?: string })[] }>>;
  const out = new Map<string, BundlePower>();
  for (const section of ['powersets', 'power-pools', 'epic-pools']) {
    for (const [setKey, set] of Object.entries(bundle[section] ?? {})) {
      for (const power of set.powers ?? []) {
        const source = power.internalName ?? power.fullName?.split('.').pop() ?? power.name ?? '';
        out.set(projectionKey(set.id ?? setKey, source.replace(/\s+/g, '_')), power);
      }
    }
  }
  bundlePartitionCache.set(server, out);
  return out;
}

/** `CTX` with the PROD6C-1d inputs turned on. Everything else stays at the totals hook's own
 *  defaults so a delta can only come from the input under test. */
function ctxWith(overrides: Partial<AdapterCalcContext>): AdapterCalcContext {
  return { ...CTX, ...overrides };
}

function engineTotals(server: Server, build: Build, ctx: AdapterCalcContext = CTX): EngineTotals {
  const json = engineHandle(server).recalculate(toCharacterStateJson(withoutIllegalSlots(build), ctx));
  return JSON.parse(json) as EngineTotals;
}

/** Base execution stat the engine's / RegistryEffectsDisplay's way: `stats.<key>` when truthy,
 *  else `effects.<effectsKey>`. */
function truthyStat(power: SelectedPower, statKey: string, effectsKey: string): number | null {
  const stats = (power as unknown as { stats?: Record<string, number> }).stats;
  const effects = (power as unknown as { effects?: Record<string, number> }).effects;
  const s = stats?.[statKey];
  if (s) return s;
  const e = effects?.[effectsKey];
  return e || null;
}

/** Does a build-wide +Range reach this power? The server copies the character's whole Strength
 *  set onto every power and then zeroes only what `StrengthsDisallowed` names, so the gate is the
 *  disallow list — NOT `boostsAllowed`, which governs slotting alone (Snow Storm takes no Range
 *  IO yet a global still extends it). `GlobalStrengthsDisallowed` is the HC narrower form that
 *  blocks the global while leaving slotting alone. Mirrors `projection.rs`
 *  `reachable_global_range`. */
function rangeEnhanceable(power: SelectedPower): boolean {
  const disallowed = power as unknown as {
    strengthsDisallowed?: string[];
    globalStrengthsDisallowed?: string[];
  };
  return !(
    disallowed.strengthsDisallowed?.includes('Range')
    || disallowed.globalStrengthsDisallowed?.includes('Range')
  );
}

function baseEnduranceCost(power: SelectedPower): number | null {
  const stats = (power as unknown as { stats?: Record<string, number> }).stats;
  const effects = (power as unknown as { effects?: Record<string, number> }).effects;
  const end = stats?.endurance;
  if (end) {
    if ((power.powerType ?? '').toLowerCase() === 'toggle') {
      const period = stats?.activatePeriod ?? 0.5;
      return end / period;
    }
    return end;
  }
  return effects?.enduranceCost || null;
}

// f32 engine vs f64 JS — a per-power value apart by more than this is a real disagreement.
const TOLERANCE = 0.05;

/** The same skew as a RELATIVE bound, for the rows whose magnitude makes an absolute one
 *  meaningless. The engine's enhancement fraction is an f32 and the beta's an f64, which measures
 *  as a constant ~1.4e-4 relative gap on every "enhanced"/"final" tier — invisible at 0.05 absolute
 *  on a 12-second recharge, over it on a 400-point heal (measured on the SAME power: its untouched
 *  `recharge` and `enduranceCost` rows carry the identical relative gap, and every `base` tier,
 *  which no enhancement multiplies, agrees exactly). Both bounds apply, so a small value is still
 *  held to the tight absolute one. */
const RELATIVE_TOLERANCE = 1e-3;

/** Are these two the same number, allowing for the f32/f64 skew above? */
function withinTolerance(engine: number, beta: number): boolean {
  const gap = Math.abs(engine - beta);
  return gap <= TOLERANCE || gap <= Math.abs(beta) * RELATIVE_TOLERANCE;
}

/** The sub-50 level the PROD6B-2c sweep resolves at. Any level where the AT tables differ from
 *  their level-50 row works; mid-range keeps the movement well clear of [`TOLERANCE`]. */
const SWEEP_LEVEL = 25;

function tierDelta(label: string, engine: ThreeTierValues | null, beta: ThreeTierValues | null): string[] {
  if (engine === null && beta === null) return [];
  if (engine === null || beta === null) return [`${label}: engine ${engine ? 'set' : 'null'} vs beta ${beta ? 'set' : 'null'}`];
  const out: string[] = [];
  for (const k of ['base', 'enhanced', 'final'] as const) {
    if (!withinTolerance(engine[k], beta[k])) out.push(`${label}.${k}: engine ${engine[k]} vs beta ${beta[k]}`);
  }
  return out;
}

// A perma delta split into real disagreements and adjudicated engine-supersedes-beta ones. The
// engine computes perma ONLY from a real exported recharge + duration, so "engine set, beta null"
// always means the engine read a buff/heal/summon duration the fork's beta converter dropped (the
// PROD5 pattern — the export is truth). That is logged, not failed. The reverse (engine null while
// the beta has a perma) would mean the engine dropped a duration the beta carries — a real defect,
// so it stays a hard delta.
/** The `strengthsDisallowed` entry that locks a power's recharge. Schema vocabulary (the export's
 *  own strength name), not a game proper noun. */
const RECHARGE_STRENGTH = 'RechargeTime';

/** Evidence that a perma disagreement is the engine reading an export field the beta ignores,
 *  rather than engine perma math being wrong. Supplied only where the caller has the engine's own
 *  copy of the power (PROD6C-3h); absent, every field stays a hard delta.
 *
 *  Two shapes, each pinned to a specific field set so an unrelated perma bug still fails:
 *   - the power's `strengthsDisallowed` / `globalStrengthsDisallowed` lists RechargeTime, so the
 *     engine correctly holds `totalRecharge` at 0 while the beta applies the build's global; the
 *     three fields that derive from it move together;
 *   - the two datasets resolved different durations, which moves `duration` and the
 *     `rechargeNeeded` computed from it. */
function permaEvidence(power: BundlePower | undefined, betaDuration: number): { rechargeLocked: boolean; durationDiffers: boolean } {
  const disallowed = [...(power?.strengthsDisallowed ?? []), ...(power?.globalStrengthsDisallowed ?? [])];
  const engineDuration = (power?.effects as { buffDuration?: number; effectDuration?: number } | undefined);
  const resolved = engineDuration?.buffDuration ?? engineDuration?.effectDuration;
  return {
    rechargeLocked: disallowed.includes(RECHARGE_STRENGTH),
    durationDiffers: resolved != null && Math.abs(resolved - betaDuration) > TOLERANCE,
  };
}

/**
 * True when the beta's copy of the power carries no atom array. The caster-side
 * window (`selfStateWindow`) uses the atoms' `toWho` to veto a bag duration that
 * belongs to the TARGET — the bag's buff slots carry no target of their own — so
 * with no atoms the beta has no evidence to run that veto with and keeps the bag's
 * answer, by design (absence is not a negative).
 *
 * Measured 2026-07-29: pool and epic powers carry atoms on NO dataset (0/71 and
 * 0/405 on Homecoming, 0/77 and 0/375 Rebirth, 0/82 and 0/330 Thunderspy) —
 * `convert-pool-powers.cjs` / `convert-epic-pools.cjs` are shortened pipelines
 * that never emit them, while the engine reads the same powers from `contract/`
 * WITH atoms. Two powers currently differ because of it, both a foe-side
 * `mezResistance` window the engine removes: Rebirth Long Range Teleport and
 * Thunderspy Teleport Foe. With `specialBuff` in SELF_BUFF_KEYS (2026-08-11),
 * Homecoming's epic Possess joined the class at the null boundary — its
 * `specialBuff` 30s is the foe's -Special debuff clock, so the engine computes
 * no perma at all while the atom-less beta copy keeps the bag window.
 */
function betaCannotRunTheWindowVeto(power: Power | SelectedPower | undefined): boolean {
  return power !== undefined && atomsOf(power).length === 0;
}

/**
 * The mirror-blind class, and the window each of its powers now resolves.
 *
 * These eight rows were `engine null vs beta set` when the roster was stamped on 2026-09-04: the
 * beta read the caster's window off the atoms while `coh_math`'s converter MIRROR had no slot for
 * the atom carrying it — Thunderspy Cross Punch and Swirl on an IgnoreStrength self-buff, the EM
 * Pulse copies on a self-directed `Recovery −1` outside the mirror's four penalty slots. The
 * roster called it vendor lag and predicted it would empty at the next engine vendor; PERMA-3 and
 * PERMA-4 landed, BPORT8 re-vendored, and on 2026-09-07 every row resolves NON-null and equal on
 * both sides at the duration below.
 *
 * So the class is recorded the other way up. An empty roster of an ADJUDICATED absence cannot tell
 * "the defect left" from "the arm stopped firing" — the two read identically, and the second is
 * silent. A window pinned by value can only be wrong loudly: a power the sweep stops reaching
 * drops its key, and a re-blinded mirror puts `null` where a number is pinned. The adjudication
 * that used to absorb these rows is gone with them, so a mirror that loses a slot again is a hard
 * delta rather than an accepted one.
 */
const MIRROR_RECOVERED_WINDOWS: Record<string, Record<string, number>> = {
  homecoming: { 'epic/electrical_mastery/EM_Pulse': 15 },
  rebirth: {
    'epic/charge_mastery/EM_Pulse': 15,
    'epic/electrical_mastery/EM_Pulse': 15,
  },
  thunderspy: {
    'epic/charge_mastery/EM_Pulse': 15,
    'epic/electrical_mastery/EM_Pulse': 15,
    'pool/fighting/Cross_Punch': 6,
    'pool/fighting/Swirl': 6,
  },
  brainstorm: { 'epic/electrical_mastery/EM_Pulse': 15 },
};

/**
 * The keys where the beta's resolver answered from the summoned pet's underlay while the engine
 * answered from the power's own atom, and the value each side recovered (2 dp).
 *
 * The five `Power_Surge.hold` rows were the last "value" family left in the magnitudes sweep, and
 * were not a value dispute: the engine's `Hold Prot` 17.3 is the stalker's own `Held −50 @Cur`
 * protection through `Melee_Res_Boolean`, the beta's `Hold` 22.35s is the crash pet's EM Pulse
 * `Held +15 Duration` on foes through the pet class's `Ranged_Immobilize`. On the two forks that
 * summon the pet separately the pseudo-pet merge lays the second under the first, and STRIP-1
 * removed the first from the beta's bag, so the underlay was what the resolver saw. Homecoming
 * and brainstorm inline the pet's templates into the power itself, so nothing is merged and the
 * key is unwitnessed there — an empty roster on those forks is a reached population of zero
 * merges, not an arm that stopped firing, and a pet the converter starts summoning separately
 * on either fork lands here as a missing key rather than silently.
 *
 * Pinned by VALUE, the class 4 shape: a power the sweep stops reaching drops its tag, an own row
 * the engine loses puts the pet's number where 17.3 is pinned, and a pet chain that resolves
 * differently moves the underlay.
 */
const OWN_ROW_OVER_PET_UNDERLAY: Record<string, Record<string, { own: number; underlay: number }>> = {
  rebirth: { 'stalker/Power_Surge.hold': { own: 17.3, underlay: 22.35 } },
  thunderspy: { 'stalker/Power_Surge.hold': { own: 17.3, underlay: 22.35 } },
};

function permaDelta(
  engine: PermaInfo | null,
  beta: PermaInfo | null,
  evidence?: BundlePower | undefined,
  betaPower?: Power | SelectedPower,
): { real: string[]; adjudicated: string[] } {
  if (engine === null && beta === null) return { real: [], adjudicated: [] };
  if (engine !== null && beta === null) {
    return { real: [], adjudicated: [`perma: engine computes (recharge ${engine.baseRecharge}s / dur ${engine.duration}s) but beta dropped the duration → null`] };
  }
  if (engine === null && beta !== null && betaCannotRunTheWindowVeto(betaPower)) {
    // The window-veto blindness (below) at the null boundary: with no atoms beta-side a
    // foe-owned bag duration stands as the whole window, so the beta computes a perma the
    // engine — atoms in hand — removes entirely (epic Possess's specialBuff 30s is the
    // foe's -Special debuff clock, not the caster's).
    return { real: [], adjudicated: [`perma: beta ships no atoms so the window veto cannot run; its ${beta.duration}s bag window stands while the engine removes the window → engine null`] };
  }
  if (engine === null && beta !== null && evidence !== undefined) {
    // Dataset drift at the null boundary (2026-08-19 oracle re-cut): the legacy dataset
    // authors a duration the engine's copy of the power does not, so the beta computes a
    // window from data the engine never sees. The stalker openers are the corpus population —
    // the legacy converter hung a `buffDuration` on the stealth-SUPPRESSION row
    // (`StealthRadius × -1`), a clock the export states for hiding, not for any buff.
    // The reverse (a duration BOTH datasets author that only the beta windows) stays hard.
    const authored = evidence.effects as { buffDuration?: number; effectDuration?: number } | undefined;
    if ((authored?.buffDuration ?? authored?.effectDuration) == null) {
      return { real: [], adjudicated: [`perma: beta computes a ${beta.duration}s window from a duration the engine's dataset does not author → engine null`] };
    }
  }
  if (engine === null || beta === null) return { real: [`perma: engine ${engine ? 'set' : 'null'} vs beta ${beta ? 'set' : 'null'}`], adjudicated: [] };
  const real: string[] = [];
  const adjudicated: string[] = [];
  const { rechargeLocked, durationDiffers } = evidence !== undefined
    ? permaEvidence(evidence, beta.duration)
    : { rechargeLocked: false, durationDiffers: false };
  // Fields each piece of evidence explains. Anything outside these sets is still a real delta
  // even on a power the evidence covers.
  const lockedFields = new Set<keyof PermaInfo>(['totalRecharge', 'effectiveRecharge', 'permaPercent']);
  const durationFields = new Set<keyof PermaInfo>(['duration', 'rechargeNeeded', 'permaPercent']);
  // Deliberately the same field set as `durationFields` and no wider: a missing atom
  // array can only move the WINDOW and what derives from it. `isPerma` stays a hard
  // delta below regardless — a flipped verdict is worth failing over even here.
  const windowBlind = betaCannotRunTheWindowVeto(betaPower);

  const numeric: (keyof PermaInfo)[] = ['baseRecharge', 'duration', 'effectiveRecharge', 'rechargeNeeded', 'totalRecharge', 'permaPercent'];
  for (const k of numeric) {
    if (Math.abs((engine[k] as number) - (beta[k] as number)) <= TOLERANCE) continue;
    if (rechargeLocked && lockedFields.has(k)) {
      adjudicated.push(`perma.${k}: engine ${engine[k]} vs beta ${beta[k]} — the export disallows ${RECHARGE_STRENGTH} on this power and the beta applies the global anyway`);
    } else if (durationDiffers && durationFields.has(k)) {
      adjudicated.push(`perma.${k}: engine ${engine[k]} vs beta ${beta[k]} — the two datasets resolved different durations (engine ${engine.duration}s, beta ${beta.duration}s)`);
    } else if (windowBlind && durationFields.has(k)) {
      adjudicated.push(`perma.${k}: engine ${engine[k]} vs beta ${beta[k]} — this power carries no atoms beta-side, so the caster-window veto cannot run and the bag's duration stands`);
    } else {
      real.push(`perma.${k}: engine ${engine[k]} vs beta ${beta[k]}`);
    }
  }
  if (engine.isPerma !== beta.isPerma) real.push(`perma.isPerma: engine ${engine.isPerma} vs beta ${beta.isPerma}`);
  return { real, adjudicated };
}

/**
 * This power's post-ED enhancement bonuses the beta's way. With no Alpha equipped that is
 * `calculatePowerEnhancementBonuses`; with one it is `combineWithAlphaED`, which folds Alpha's
 * ED-subject slice into the raw IO sum, applies ED once, then adds the bypass slice on top.
 *
 * `allowedEnhancements` is passed on BOTH paths, gating Alpha to the aspects the power accepts —
 * the rule `combineWithAlphaED` documents as the game's own (an Alpha's +Damage does not boost a
 * power that only takes EndRdx and Recharge) and the rule the engine's `power_enhancement`
 * applies. The beta's four DISPLAY call sites omitted it, so Alpha leaked onto every aspect
 * there; PROD6C-3f fixed them to pass it, which is what lets this reference grade the path the
 * surfaces actually run.
 */
function betaEnhancement(power: SelectedPower, build: Build, state: ReferenceState): EnhancementBonuses {
  if (state.enhancementOverride) return state.enhancementOverride;
  const identity = { name: power.name, slots: power.slots, allowedEnhancements: power.allowedEnhancements };
  if (state.alpha) {
    return combineWithAlphaED(
      identity,
      build.level,
      getIOSet,
      state.alpha.bonuses,
      state.alpha.bypass,
      state.exemplarLevel,
    );
  }
  return calculatePowerEnhancementBonuses(identity, build.level, getIOSet, state.exemplarLevel);
}

/** The two inputs PROD6C-1d found ungraded: an equipped Alpha (which splits through ED) and an
 *  exemplared build (which rescales non-attuned IOs and, below 45, suppresses incarnates
 *  outright). Absent = the 6B fixture's own state, no Alpha and no exemplar. */
interface ReferenceState {
  alpha?: AlphaFixture;
  /** The exemplared-to level, or undefined for an unexemplared build. */
  exemplarLevel?: number;
  /** This power's stacking-slider setting (PROD6C-3b), which the display bag reads before the
   *  resolver ever sees it. Absent = the untouched slider every other fixture here runs. */
  targetsHit?: number;
  /** Post-ED bonuses to use verbatim instead of computing them. The exemplar test re-runs the whole
   *  reference with the beta's exemplar CAP swapped for the exported one, which is how it proves a
   *  delta is that cap and nothing else. */
  enhancementOverride?: EnhancementBonuses;
  /** The combat/toggle state the effective-power resolution reads (PROD6C-3k). Absent = `CTX`,
   *  the same all-off state every other fixture drives the engine with. */
  ctx?: AdapterCalcContext;
}

/** The beta reference for one power — the calculators + resolver PROD6C–E will retire. Both halves
 *  come from one call so the enhancement bonuses they read are the same object. */
function betaReference(
  power: SelectedPower,
  build: Build,
  archetypeId: string,
  rawGlobal: ReturnType<typeof mapGlobal>,
  state: ReferenceState = {},
): { projection: Omit<PowerProjection, 'grantedMagnitudes' | 'damage'>; magnitudes: Map<string, ResolvedMagnitude>; bag: Record<string, unknown> } {
  const enh = betaEnhancement(power, build, state);
  const global = convertGlobalBonusesToAspects(rawGlobal);

  // Every execution value describes the power the surface SHOWS (PROD6C-3k): a snipe read in
  // combat mode reports its fast cast, not its slow one. Only `perma` below stays on the base
  // power, which is where both beta surfaces read it.
  const shown = shownPower(power, build, archetypeId, rawGlobal, state.ctx) as unknown as SelectedPower;
  const rechargeBase = truthyStat(shown, 'recharge', 'recharge');
  const endBase = baseEnduranceCost(shown);
  const accBase = truthyStat(shown, 'accuracy', 'accuracy');
  const castBase = truthyStat(shown, 'castTime', 'castTime');
  const rangeBase = truthyStat(shown, 'range', 'range');

  return {
    projection: {
      recharge: rechargeBase !== null ? calcThreeTier('recharge', rechargeBase, enh, global) : null,
      enduranceCost: endBase !== null ? calcThreeTier('endurance', endBase, enh, global) : null,
      accuracy: accBase !== null ? calcThreeTier('accuracy', accBase, enh, global) : null,
      castTime: castBase !== null ? calcThreeTier('castTime', castBase, enh, global) : null,
      arcanaTime: castBase !== null ? calculateArcanaTime(castBase) : null,
      // A global +Range is blocked only by the power's own `StrengthsDisallowed` (engine
      // `reachable_global_range`; `boostsAllowed` governs slotting, not strength). The corpus
      // carries no set bonuses, so the global is zero and gated and ungated agree here — the rule
      // itself is graded on real bundle data by the engine's `global_range_gate`.
      range:
        rangeBase !== null
          ? rangeEnhanceable(shown)
            ? calcThreeTier('range', rangeBase, enh, global)
            : calcThreeTier('range', rangeBase, enh, {})
          : null,
      // The archetype's RechargeTime clamp bounds, as the engine reads them: the
      // +400% cap is reachable, so recharge past it must stop buying a faster
      // cycle and stop closing the gap to perma on both sides.
      perma: calculatePermaInfo(
        power,
        enh,
        (rawGlobal.recharge ?? 0) / 100,
        getRechargeBounds(archetypeId),
      ),
      // PROD6D — the fractions every tier above was built from, now carried on the projection so
      // a re-slotting surface reads them instead of running this same calculator beside it.
      enhancementBonuses: enh,
    },
    // The magnitude rows read the +Strength-augmented globals — the surfaces' own input
    // (PROD6C). The execution half above deliberately does not: Strength carries no
    // recharge / endurance / accuracy / range / cast aspect.
    magnitudes: betaMagnitudes(power, build, archetypeId, enh, withStrengthBonuses(global, rawGlobal), state.targetsHit, shown as unknown as Power),
    // The bag those magnitudes resolved from, returned so every adjudication below reads the SAME
    // bag the reference did rather than rebuilding one from the authored power.
    bag: displayBag(power, state.targetsHit ?? 0, shown as unknown as Power),
  };
}

/** The state the effective-power resolution reads, assembled from a ctx exactly as the adapter
 *  assembles the engine's (PROD6C-3k) — the stance overlay included — so a delta can never come
 *  from the two sides being handed different toggle state. */
function shownState(
  build: Build,
  archetypeId: string,
  rawGlobal: ReturnType<typeof mapGlobal>,
  ctx: AdapterCalcContext,
): EffectivePowerState {
  return {
    combatMode: ctx.combatMode,
    hidden: isCasterHidden(build, ctx.stalkerHidden),
    globalAdjusters: effectiveGlobalAdjusters(build, ctx.globalAdjusters),
    mechanicAdjusters: ctx.mechanicAdjusters,
    atInherentState: { dominationActive: ctx.dominationActive },
    // Read off the build, exactly as the adapter reads it for the engine — the mode redirect is
    // build state, not ctx state (PROD6C-3l).
    activeModes: build.activeModes,
    build,
    currentToHit: currentToHitFraction(archetypeId, rawGlobal),
  };
}

/** The power the surfaces actually SHOW for this selection: the snipe's fast form, the mid-combat
 *  cast, the active mode-gated contributions merged in (PROD6C-3k). Not the identity even under the
 *  all-toggles-off `CTX` — a conditional whose own `defaultActive` is true merges with no toggle
 *  touched, which is why every bag below goes through this rather than the authored power. */
function shownPower(
  power: Power | SelectedPower,
  build: Build,
  archetypeId: string,
  rawGlobal: ReturnType<typeof mapGlobal>,
  ctx: AdapterCalcContext = CTX,
): Power {
  return resolveEffectivePower(power as Power, shownState(build, archetypeId, rawGlobal, ctx)).power;
}

/** The bag a surface resolves for one power: the built bag, the power's own slider, then the
 *  pseudo-pet merge (PROD6C-3c). This is the EVIDENCE bag every adjudication below reads, so it
 *  has to be the merged one — a summon power's pet-derived rows are in neither side's authored
 *  bag, and grading them against the unmerged build would adjudicate every one of them away as
 *  converter drift. The same reason `shown` belongs here: a conditional-added row is in neither
 *  AUTHORED bag either.
 *
 *  `shown` is the effective power the bag is BUILT from; `power` stays the base one, because the
 *  stacking slider is offered for the power the build holds and scales the merged bag with it —
 *  the asymmetry both the InfoPanel and the engine's `display_effects` carry. */
function displayBag(
  power: Power | SelectedPower,
  targetsHit = 0,
  shown: Power = power as Power,
): Record<string, unknown> {
  const built = buildDisplayEffects(shown);
  return withPseudoPetEffects(
    shown,
    withTargetsHit(power as Power, built, targetsHit),
  ) as unknown as Record<string, unknown>;
}

/** The bag keys the pseudo-pet merge ADDS to a power (PROD6C-3c) — the rows a summon power
 *  carries only through its non-commandable pet, which its own bag has no key for. */
function pseudoPetKeys(power: Power | SelectedPower, shown: Power = power as Power): Set<string> {
  const built = buildDisplayEffects(shown) as unknown as Record<string, unknown>;
  return new Set(Object.keys(displayBag(power, 0, shown)).filter((key) => !(key in built)));
}

/** The beta reference magnitudes for one power — the resolver `RegistryEffectsDisplay` calls. */
function betaMagnitudes(
  power: SelectedPower,
  build: Build,
  archetypeId: string,
  enh: Record<string, number | undefined>,
  global: Record<string, number | undefined>,
  targetsHit?: number,
  shown: Power = power as Power,
): Map<string, ResolvedMagnitude> {
  const rows = resolvePowerMagnitudes({
    // The bag the SURFACES resolve, not the authored one (PROD6C-3a): the merged stats, the
    // heal extracted from the damage array, a summon's duration as the buff duration, and the
    // flattened movement object. Before this the gate handed both sides the authored bag, so
    // every one of those transforms went ungraded.
    // …at the power's own slider setting, which the surfaces apply to that bag before resolving
    // it (PROD6C-3b) — zero/absent is the identity, so every other fixture here is unaffected —
    // and with the pseudo-pet debuffs merged under it (PROD6C-3c).
    effects: displayBag(power, targetsHit ?? 0, shown) as unknown as PowerEffects,
    archetypeId,
    level: build.level,
    enhancementBonuses: enh,
    globalBonuses: global,
    // 1.0 on both sides, and now unconditionally so: the beta's Defender/Controller-primary
    // support rule keyed on archetype NAMES (Rule 0), was measured to match no row that
    // actually reaches a table-less fallback, and was deleted in PROD6B-2b. The precondition
    // that keeps it dead is held down by supportModifierReach.test.ts.
    buffDebuffMod: 1.0,
  });
  return new Map(rows.map((row) => [row.rowKey, row]));
}

/** The bag's untyped-duration slot: a duration with no attrib behind it, which a converter emits
 *  when it read a template's duration but could not type the effect itself. It is the one beta-only
 *  row that can be adjudicated rather than failed — see `magnitudeDeltas`. Effect keys are schema
 *  vocabulary, not game proper nouns, so naming it here does not breach Rule 0. */
const UNTYPED_DURATION_EFFECT_KEY = 'effectDuration';

/** Effect keys the engine SYNTHESIZES when it normalizes a legacy pool/epic power
 *  (`coh_data`'s `normalize_legacy_power`), rather than reading from the converter.
 *
 *  These are excluded from the beta-only adjudication below. That adjudication accepts a missing
 *  engine key as converter drift, which is true for an authored effect and false for these: here a
 *  missing key means the normalization itself broke, and "the engine's dataset carries no
 *  'castTime'" is a description of the defect, not an excuse for it. Measured — with the renames
 *  disabled the adjudication swallowed every one of these rows, and only the separate
 *  projection-field comparison still failed the gate. */
const ENGINE_NORMALIZED_EFFECT_KEYS = new Set(['enduranceCost', 'castTime']);

/** The bag keys `buildDisplayEffects` mints for a power on its own — what a display bag still
 *  carries when the authored `effects` is empty. Execution stats off `stats`, the lifted
 *  `summon`, the heal pulled out of the damage array, and the flattened movement axes.
 *
 *  Named here rather than derived because the adjudication below needs it per ROW (it has the
 *  bag, not the power), and pinned by [`assertMintedKeysStillMint`] so it cannot drift from the
 *  builder it describes. */
const DISPLAY_MINTED_KEYS = new Set([
  'summon', 'enduranceCost', 'recharge', 'accuracy', 'range', 'castTime', 'radius', 'maxTargets',
  'arc', 'healing', 'buffDuration', 'fly', 'runSpeed', 'jumpSpeed', 'jumpHeight',
  'flyUnenhanced', 'runSpeedUnenhanced', 'jumpSpeedUnenhanced', 'jumpHeightUnenhanced',
]);

/** Does this display bag carry anything the AUTHORED effects supplied?
 *
 *  This is the question the engine-supersedes-beta adjudication actually depends on, and until
 *  STRIP-1 nobody had to ask it: the arm accepts an engine-only row when "the beta's bag has no
 *  such key", reading that as the beta's converter having dropped an effect the parser
 *  recovered. That inference holds while the beta HAS a bag and the missing key is one power's
 *  drift. It does not hold at all once the bag is empty for every power — then the premise is
 *  universally true, the arm accepts the entire corpus, and the gate reports the strip as
 *  hundreds of separate converter drops.
 *
 *  So a bag holding nothing but keys the builder minted for itself is not evidence of drift.
 *
 *  It is not evidence of AGREEMENT either, which is the half this originally got wrong by making
 *  those rows hard. A resolver with no authored input is not a second opinion that disagrees — it
 *  is not a witness at all, and 20,613 of the suite's 21,603 hard rows were that (census
 *  2026-09-07, PROD6B-BETA-PARITY class 1). They leave through [`magnitudeDeltas`]' third bucket:
 *  counted and reported per body, never accepted and never dropped. */
function bagCarriesAuthoredEffects(bag: Record<string, unknown> | undefined): boolean {
  if (!bag) return false;
  return Object.keys(bag).some((key) => !DISPLAY_MINTED_KEYS.has(key));
}

/** Do two authored bag slots state the same thing? Numbers compare through the same f32/f64
 *  skew the tier comparison allows; objects compare per key; one side missing is a difference.
 *  This is what widens `inputsDiffer` from bare numbers to every slot shape (2026-08-19 oracle
 *  re-cut): most bag slots are `{scale, table}` objects, and a number-only read left every
 *  object-slot dataset difference standing as a hard delta. */
function authoredSlotsAgree(a: unknown, b: unknown): boolean {
  if (typeof a === 'number' && typeof b === 'number') return Math.abs(a - b) <= TOLERANCE;
  if (a === b) return true;
  if (a == null || b == null || typeof a !== 'object' || typeof b !== 'object') return false;
  const aKeys = Object.keys(a as object);
  const bKeys = Object.keys(b as object);
  if (aKeys.length !== bKeys.length) return false;
  return aKeys.every((k) =>
    authoredSlotsAgree((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]),
  );
}


/** A by-type label with its segment order removed. The segments are joined without a delimiter
 *  (`RunFlyJmpH…`), so they cannot be re-split reliably (COND-8: a joined string is for asking,
 *  never re-splitting) — the sorted character multiset is the order-free fingerprint instead.
 *  Order itself is not under test for the same reason row order is not: the engine emits its
 *  by-type breakdown in its own map order, the beta in registry order, and the per-type CONTENT
 *  is already graded value-by-value by the expanded rows (`key.subtype`), which are compared
 *  individually above the label. */
function byTypeFingerprint(label: string | null | undefined): string | null {
  return label == null ? null : [...label].sort().join('');
}

/** One power's magnitude comparison, in four parts.
 *
 *  `real` and `adjudicated` are the original two: a disagreement between two sides that both
 *  answered, and one the authored data explains. `unwitnessed` is the third — rows the beta
 *  cannot speak to because STRIP-1 left its bag holding only minted keys, which is most of the
 *  corpus and was previously counted as disagreement. `witnessed` says whether this power's beta
 *  side had any authored input at all, so a body can state the population it really graded
 *  instead of the population it walked. `underlayOutranked` is the one adjudication that carries
 *  a VALUE out: a key both sides resolved from different atoms — the power's own row on the
 *  engine, the summoned pet's underlay on the beta — so the body can pin what the own row
 *  recovered rather than accept a string. */
type MagnitudeDiff = {
  real: string[];
  adjudicated: string[];
  unwitnessed: string[];
  witnessed: boolean;
  underlayOutranked: { key: string; own: number; underlay: number }[];
  additiveFilled: { key: string; base: number; filled: number }[];
};

/** The bag keys the beta holds only because an ADDITIVE conditional filled them into a base bag
 *  STRIP-1 emptied — [`additiveFillIns`]' subject, keyed effectKey -> the entry that filled it. */
type AdditiveFillIns = ReadonlyMap<string, string>;

/** Which of a power's bag keys arrived by an active additive conditional filling an EMPTY base
 *  slot, and from which entry.
 *
 *  Both merges implement one rule, and they wrote it down the same way: an `additive` conditional
 *  contributes only keys the base LACKS, because a colliding key means two simultaneous instances
 *  and overwriting the base with one of them would display a single misleadingly stronger effect.
 *  The beta records the loser as an `extraInstance` for its own row; `mode: 'replace'` is the
 *  mutually-exclusive case and overwrites on both sides, which is why it is skipped here.
 *
 *  They differ in one place only: what they ask for base PRESENCE. The engine asks the
 *  atom-projected base surface (`window_slots`, `effective.rs`' `conditional_delta`); the beta
 *  asks `power.effects`, which STRIP-1 emptied for every power on every fork. So the beta's
 *  collision test is universally false, every additive entry's copy fills in, and the resolver is
 *  handed the CONDITIONAL's atom for a key the power's own atoms carry.
 *
 *  That is class 1's shape one layer down, and it is why the rows below are not a value dispute:
 *  the two sides read different atoms. Measured 2026-09-08 over the four combat-state bodies —
 *  every one of the 980 rows is `off=undefined` (the base states nothing), an exact-value carrier,
 *  and `additive`; not one is a `replace` entry or a base-authored slot. Rebirth's
 *  `Greater_Psi_Blade` is the control: its `psionic_melee_insight` is `replace` on that fork
 *  alone, both sides overwrite, and it is the one fork with no row for that power.
 *
 *  `offBag` is the same power's bag with every toggle off, which is the base surface the beta CAN
 *  still see. A key absent there and present here is the fill-in; requiring the value to equal the
 *  entry's own authored copy is what keeps this from swallowing a genuine contribution.
 *
 *  Two of the four legs are not gradeable by this corpus, and saying so is the point of writing it
 *  down (mutation sweep 2026-09-08 — deleting either leaves the suite green):
 *
 *  * the `offBag` presence test can only fire where the toggles-off bag holds the key, and
 *    STRIP-1 left it holding none — every subject measures `baseBagKeys=0`. It is oriented to
 *    NARROW as the bag comes back, which is what keeps it from becoming the universally-true
 *    guard it exists to compensate for; until then the corpus cannot violate it.
 *  * the exact-value test decides WHICH entry is named, never whether the row routes: where two
 *    active entries claim one key (Dual Pistols' ammo clocks — `iceammo` 10s and `toxicammo` 8s
 *    on one `buffDuration`) the loser fails it and the winner still sets the key. Dropping it
 *    changes the adjudication string and nothing else.
 *
 *  Deleting the `replace` skip, or the detection itself, does go red — the routing legs are
 *  graded. */
function additiveFillIns(
  power: SelectedPower,
  offBag: Record<string, unknown>,
  onBag: Record<string, unknown>,
): AdditiveFillIns {
  const out = new Map<string, string>();
  for (const conditional of power.conditionalEffects ?? []) {
    if (conditional.mode === 'replace') continue;
    const from = conditional.effects as Record<string, unknown> | undefined;
    if (!from) continue;
    for (const [key, value] of Object.entries(from)) {
      if (offBag[key] !== undefined) continue;
      if (JSON.stringify(onBag[key]) !== JSON.stringify(value)) continue;
      out.set(key, conditional.id);
    }
  }
  return out;
}

/** The slots `mezSlotValue` reads — the beta's own atom-native reader for a power's OWN
 *  control / protection row, which is the reader the display bag lost at STRIP-1. */
type OwnMezSlot = Parameters<typeof mezSlotValue>[1];
const OWN_MEZ_SLOTS: ReadonlySet<string> = new Set<OwnMezSlot>(['hold', 'stun', 'sleep', 'immobilize', 'confuse', 'fear']);

/** What a `mag`-format row's quantity has to be, given the `attribType` the atom behind it
 *  states — `classifyMagQuantity`'s table, read the other way round so the engine's row can be
 *  graded from the ATOM rather than from the resolver that no longer sees it. */
const MEZ_QUANTITY_OF_ATTRIB_TYPE: Record<string, string> = {
  Duration: 'mez_duration',
  Magnitude: 'mez_magnitude',
  Expression: 'mez_expression',
  Constant: 'mez_constant',
};

/** Running totals for one test body's magnitude half — how many powers it walked, how many of
 *  those the beta could witness, and the unwitnessed rows themselves.
 *
 *  Every body reports these, because the alternative to reporting them is an arm that looks
 *  green over a population of zero. `stated-skip-beats-silent-skip`. */
type MagnitudeWitness = { powers: number; witnessed: number; unwitnessed: string[] };

function witnessCounter(): MagnitudeWitness {
  return { powers: 0, witnessed: 0, unwitnessed: [] };
}

/** Fold one power's diff into a body's totals, and return its hard deltas for the caller to push. */
function countWitness(w: MagnitudeWitness, mags: MagnitudeDiff): MagnitudeDiff {
  w.powers += 1;
  if (mags.witnessed) w.witnessed += 1;
  w.unwitnessed.push(...mags.unwitnessed);
  return mags;
}

/** The line every body prints, and the claim it is allowed to make.
 *
 *  A body whose `witnessed` is 0 has no resolver oracle for magnitudes at all; its verdict is the
 *  engine-side floor beside it, not this comparison. Saying so in the log is the difference
 *  between a stated skip and a silent one. */
function witnessLine(server: string, label: string, w: MagnitudeWitness): string {
  return `[witness] ${server} ${label}: ${w.witnessed}/${w.powers} powers reached the resolver with an ` +
    `authored bag; ${w.unwitnessed.length} rows unwitnessed` +
    (w.witnessed === 0 ? ' — this body grades NO magnitude comparison (see the engine-side floor)' : '');
}

/** Diff one power's granted magnitudes into the three buckets of [`MagnitudeDiff`] — a real
 *  disagreement, an adjudicated engine-supersedes-beta one, and a row the beta could not witness.
 *  Rows are matched by key, not position — the engine emits them in
 *  bag order while the beta resolver emits them in registry-group order, and row ORDER is the
 *  component's sort, not part of the resolution under test.
 *
 *  An "engine only" row is ACCEPTED only when the beta's own power object has no such effect key:
 *  that means the fork's beta converter dropped an effect the rebuild's parser recovered (the
 *  export is truth — the PROD5 pattern), not that the engine invented a row. If the beta HAS the
 *  key and still resolved no row, the two disagree about resolution and that is a hard delta.
 *
 *  "beta only" is a hard delta — the engine dropping a row the beta carries is an engine defect —
 *  with ONE adjudicated case: a bare `effectDuration` on a power where the engine typed a template
 *  the beta's bag has no key for. Both rows are then the SAME template read twice: the rebuild's
 *  parser recovered its attrib (tspy `Power_Build_Up` → `+Special: Stun`), while the beta's
 *  converter could only keep the duration it hung on. Accepting it needs that typed engine row as
 *  evidence, so a genuine engine drop — no typed row to point at — still fails.
 *
 *  `engineEffects` opens the SYMMETRIC case, and only when the caller can supply the engine's own
 *  copy of the bag (PROD6C-3h, the pool/epic partitions). The rule mirrors the engine-only one
 *  exactly: a beta-only row is adjudicated when the ENGINE's bag has no such key to resolve from,
 *  and stays a hard delta when it HAS the key and still resolved nothing — which is the actual
 *  engine defect this branch exists to catch. Without the bag the branch is unchanged, so every
 *  other sweep keeps failing on beta-only rows.
 *
 *  Both bags are DISPLAY bags (`buildDisplayEffects`), the same input each side resolves from,
 *  so "the engine has no such key" means the engine's dataset yields no such row rather than
 *  merely authoring it elsewhere. That also makes the third adjudication below possible: a row
 *  both sides resolve from a bare number, where those two numbers differ, is converter drift.
 *
 *  2026-08-19 oracle re-cut (the engine vendor moved the engine ahead of the FROZEN legacy
 *  dataset, and every red it opened classified as the legacy data being wrong against the
 *  export — see the per-cluster evidence in the b2 triage): the drift adjudication is
 *  generalized from bare numbers to whole authored slots ([`authoredSlotsAgree`]), in BOTH
 *  directions — a one-sided row whose key the two datasets author differently is drift too,
 *  including the case where the engine's own bag has no such key at all while the engine still
 *  resolved a row: that row came through the projection's deeper walk (a grant edge or mode
 *  redirect the legacy dataset folded into the power itself — Bio Armor's aura taunt is the
 *  canonical pair, legacy `taunt {scale 1.1}` on the stance vs the engine reading the granted
 *  aura's own scale-1 row, the TAUNT-1 own-beats-redirect rule). Rows the two datasets author
 *  IDENTICALLY keep failing hard, which is what preserves the gate's grip on resolution.
 *
 *  There is no linear-self-stack adjudication any more, and the reason is worth keeping. The
 *  legacy `withTargetsHit` multiplies only the keys its dataset's `stacksLinear` names, and
 *  that list used to be authored self-inconsistently: the converter's stacking classifier was
 *  a second hand-maintained copy of the routing rules, and it keyed a `Range|Str` template to
 *  `specialBuff` while the extractor put the VALUE on `rangeBuff`, so sentinel Aim's list named
 *  a key the bag had nothing under and omitted the one that did. Both datasets shipped it, so
 *  the rows were adjudicated on atom evidence instead. STACK-6 closed it upstream — the keys
 *  come from the router now — and the rows are plain greens, which is the bar the exit
 *  condition set. A row that needs the old adjudication is a regression, not an exception. */
function magnitudeDeltas(
  powerName: string,
  engineRows: GrantedMagnitude[],
  betaRows: Map<string, ResolvedMagnitude>,
  betaEffects: Record<string, unknown> | undefined,
  engineEffects?: Record<string, unknown>,
  ownPower?: Power | SelectedPower,
  fillIns?: AdditiveFillIns,
): MagnitudeDiff {
  const out: string[] = [];
  const adjudicated: string[] = [];
  const unwitnessed: string[] = [];
  const underlayOutranked: MagnitudeDiff['underlayOutranked'] = [];
  const additiveFilled: MagnitudeDiff['additiveFilled'] = [];
  const witnessed = bagCarriesAuthoredEffects(betaEffects);
  const engineByKey = new Map(engineRows.map((row) => [row.rowKey, row]));
  // Rows the engine typed off effect keys the beta's own bag lacks — the evidence an untyped
  // beta-only duration is the same template seen through the weaker converter.
  const enginesOwnTypings = engineRows.filter((row) => betaEffects?.[row.effectKey] == null);

  for (const key of new Set([...engineByKey.keys(), ...betaRows.keys()])) {
    const engine = engineByKey.get(key);
    const beta = betaRows.get(key);
    // The authored slots behind this row, one per dataset — the evidence every drift branch
    // reads. Keyed by the row's effectKey (its bag slot), not the rowKey: an expanded by-type
    // row (`resistance.fire`) resolves from its parent slot.
    const effectKey = (beta?.effectKey ?? engine?.effectKey)!;
    const engineSlot = engineEffects?.[effectKey];
    const betaSlot = betaEffects?.[effectKey];
    // A row whose two sides resolved from DIFFERENT authored values is converter drift, not a
    // resolution disagreement — the same evidence `permaEvidence` already accepts for the perma
    // duration. It needs the engine's own bag, is pinned to the specific key, and compares the
    // inputs: two sides that read the SAME value and still disagree stay a hard delta.
    const drift = engineEffects != null && !authoredSlotsAgree(engineSlot, betaSlot);
    const slotOf = (slot: unknown) => (slot === undefined ? 'nothing' : JSON.stringify(slot));
    const driftNote = `the two datasets author '${effectKey}' differently (engine ${slotOf(engineSlot)}, beta ${slotOf(betaSlot)})`;

    if (engine && !beta) {
      if (betaEffects?.[engine.effectKey] == null && bagCarriesAuthoredEffects(betaEffects)) {
        adjudicated.push(`${key}: engine resolved ${engine.label} = ${engine.value.base} but the beta dataset carries no '${engine.effectKey}' effect`);
      } else if (betaEffects?.[engine.effectKey] == null) {
        // The bag carries no authored effect at all, so "no such key" says nothing about this
        // key — see [`bagCarriesAuthoredEffects`]. The beta is not a witness here, so this is
        // neither a delta nor an acceptance: it goes to the third bucket, which every call site
        // counts and states.
        unwitnessed.push(`${powerName}.${key}: engine resolved ${engine.label} = ${engine.value.base}, beta bag has nothing authored`);
      } else if (drift) {
        // The engine resolves a row the beta does not, from a slot the datasets disagree on —
        // the legacy pool/epic `fly` slots against the contract's cap/current split is the
        // corpus population (the `*Unenhanced` twin only the engine's authored data carries).
        adjudicated.push(`${key}: engine-only row — ${driftNote}`);
      } else {
        out.push(`${powerName}.${key}: engine only (beta HAS '${engine.effectKey}' but resolved no row)`);
      }
      continue;
    }
    if (!engine || !beta) {
      const untypedResidue = beta?.effectKey === UNTYPED_DURATION_EFFECT_KEY && enginesOwnTypings.length > 0;
      // The engine-normalized keys stay hard even under drift: a missing `enduranceCost` /
      // `castTime` there means the normalization itself broke, and drift would misread the
      // breakage as a dataset difference.
      const driftExplains = drift && !ENGINE_NORMALIZED_EFFECT_KEYS.has(beta!.effectKey);
      if (untypedResidue) {
        adjudicated.push(`${key}: beta resolved a bare ${beta!.config.label} where the export types the template as ${enginesOwnTypings.map((row) => row.label).join(', ')}`);
      } else if (driftExplains && engineSlot === undefined) {
        adjudicated.push(`${key}: beta resolved ${beta!.config.label} but the engine's dataset carries no '${beta!.effectKey}' effect`);
      } else if (driftExplains) {
        adjudicated.push(`${key}: beta-only row — ${driftNote}`);
      } else {
        out.push(`${powerName}.${key}: beta only (engine resolved no row)`);
      }
      continue;
    }
    // Two rows, two ATOMS. The pseudo-pet merge lays a summoned pet's control UNDER the power's
    // own bag — `{ ...pseudo, ...effects }` here, `merged.insert(key, own)` in granted.rs — so a
    // key the power's own atoms carry is answered by the power and never by the pet, on both
    // sides, by the same declared order. STRIP-1 emptied the beta's own slot and the underlay
    // showed through: the resolver was handed the pet's row (the merge stamps it `petClass`)
    // for a key the engine resolves from the power's own atom. Power Surge is the corpus case:
    // its own `Held −50 @Cur` on the caster is Hold PROTECTION, and its crash pet's EM Pulse
    // `Held +15 Duration` on foes is applied control; the tiers, label and quantity differ
    // because they describe different templates, not because either side misread one. The
    // resolver did read the face it was given correctly — it was given the wrong atom.
    //
    // So the beta is not a witness for this key. The engine's row is graded from the ATOM
    // instead: the own atom's `attribType` fixes the quantity the row must carry (the same
    // table `classifyMagQuantity` reads), and the value is pinned by the body, not accepted.
    const petUnderlay = typeof betaSlot === 'object' && betaSlot !== null && 'petClass' in betaSlot;
    const ownSlot = ownPower !== undefined && OWN_MEZ_SLOTS.has(effectKey)
      ? mezSlotValue(ownPower, effectKey as OwnMezSlot)
      : undefined;
    if (petUnderlay && ownSlot) {
      const owedKind = MEZ_QUANTITY_OF_ATTRIB_TYPE[ownSlot.attribType] ?? 'mez_unstated';
      if (engine.quantity.kind !== owedKind) {
        out.push(`${powerName}.${key}.quantity: own atom is ${ownSlot.attribType}-typed, engine resolved ${engine.quantity.kind}`);
      }
      adjudicated.push(
        `${key}: beta resolved the summoned pet's ${beta.expandedLabel ?? beta.config.label} underlay `
        + `(${beta.quantity.kind} ${beta.tiers.base}) for a key the power's own atoms carry `
        + `(${ownSlot.attribType} ${ownSlot.scale} × ${ownSlot.table}); the own row outranks the underlay `
        + `in both merge orders — engine ${engine.label} ${engine.value.base}`,
      );
      underlayOutranked.push({ key, own: engine.value.base, underlay: beta.tiers.base });
      continue;
    }
    // The beta resolved an ADDITIVE conditional's own copy of a key the power's base atoms
    // carry — [`additiveFillIns`]. Its own merge rule declines exactly this, and only its
    // starved base probe let the copy through, so the two rows describe different atoms and
    // there is nothing here to compare. Graded like the pet underlay above: the engine's row
    // is carried out by VALUE and pinned by the body, never accepted as a string.
    const filledBy = fillIns?.get(effectKey);
    if (filledBy !== undefined) {
      adjudicated.push(
        `${key}: beta resolved ${filledBy}'s own additive copy (${beta.tiers.base}) for a key the `
        + `power's base atoms carry; its merge declines a colliding key and only the emptied base `
        + `bag let this one fill in — engine ${engine.label} ${engine.value.base}`,
      );
      additiveFilled.push({ key: effectKey, base: engine.value.base, filled: beta.tiers.base });
      continue;
    }
    // PROD6C-3b's per-target SHAPE adjudication is gone (PROD6C-3j): the engine's converter now
    // exports the per-foe increment for a MaxHP-fraction absorb too, so the two shapes agree
    // under a dragged slider as they always did at one target. A row that grows on one side
    // alone is a hard delta again.
    for (const tier of ['base', 'enhanced', 'final'] as const) {
      if (withinTolerance(engine.value[tier], beta.tiers[tier])) continue;
      if (drift) {
        adjudicated.push(`${key}.${tier}: ${driftNote}`);
      } else {
        out.push(`${powerName}.${key}.${tier}: engine ${engine.value[tier]} vs beta ${beta.tiers[tier]}`);
      }
    }
    // The row's identity and unit must agree too — a magnitude landing in the right number but
    // the wrong unit or label would render wrong while diffing clean on value alone.
    const betaLabel = beta.expandedLabel ?? beta.config.label;
    if (engine.label !== betaLabel) out.push(`${powerName}.${key}.label: engine ${engine.label} vs beta ${betaLabel}`);
    if (engine.format !== beta.config.format) out.push(`${powerName}.${key}.format: engine ${engine.format} vs beta ${beta.config.format}`);
    if (engine.category !== beta.config.category) out.push(`${powerName}.${key}.category: engine ${engine.category} vs beta ${beta.config.category}`);
    if (engine.quantity.kind !== beta.quantity.kind) {
      out.push(`${powerName}.${key}.quantity: engine ${engine.quantity.kind} vs beta ${beta.quantity.kind}`);
    } else if (beta.quantity.kind === 'mez_duration' && engine.quantity.kind === 'mez_duration') {
      const engineMag = engine.quantity.magnitude ?? null;
      if (engineMag === null || Math.abs(engineMag - beta.quantity.magnitude) > TOLERANCE) {
        if (drift) {
          adjudicated.push(`${key}.mag: engine ${engineMag} vs beta ${beta.quantity.magnitude} — ${driftNote}`);
        } else {
          out.push(`${powerName}.${key}.mag: engine ${engineMag} vs beta ${beta.quantity.magnitude}`);
        }
      }
    }
    // Order-insensitive ([`byTypeFingerprint`]): the segment CONTENT is graded by the expanded
    // rows; the join order is each side's own sort.
    const engineByType = engine.byTypeLabel ?? null;
    const betaByType = beta.byTypeLabel ?? null;
    if (byTypeFingerprint(engineByType) !== byTypeFingerprint(betaByType)) {
      if (drift) {
        adjudicated.push(`${key}.byTypeLabel: engine ${engineByType} vs beta ${betaByType} — ${driftNote}`);
      } else {
        out.push(`${powerName}.${key}.byTypeLabel: engine ${engineByType} vs beta ${betaByType}`);
      }
    }
  }
  return { real: out, adjudicated, unwitnessed, witnessed, underlayOutranked, additiveFilled };
}

/** A bypass slice no Alpha authors — the whole bonus bypassing ED — for probing whether the split
 *  is observable on a given fixture at all. */
function probeBypass(alpha: AlphaFixture): EnhancementBonuses {
  return { ...alpha.bonuses };
}

/**
 * `buildFor`'s fixture, re-slotted to SATURATE Enhancement Diversification in one aspect: up to six
 * generic IOs of the same stat on every power that accepts it, instead of three of different stats.
 *
 * This exists because the ordinary fixture cannot grade Alpha's ED split at all. The split only
 * matters once the raw IO sum plus Alpha's ED-subject slice crosses an ED threshold — below it, ED is
 * the identity and every division of Alpha's value sums to the same total. Three IOs of distinct
 * aspects put roughly 0.42 into each, which with any Alpha stays under Schedule A's 0.7 knee: probed
 * against a bypass ratio no tier uses, that fixture produced ZERO deltas on all three forks. Six of
 * one stat put ~2.5 into a single aspect, so ED bites hard and the division is load-bearing.
 *
 * `stat` is chosen by the caller to be an aspect the equipped Alpha also buffs; a power that does not
 * accept it keeps the ordinary slotting so the fixture stays a full build.
 */
function saturatedBuild(server: Server, stat: string, level = 50): Build {
  const build = buildFor(server, STANDARD_ARCHETYPE_IDS[0], level);
  const saturate = (powers: SelectedPower[]) =>
    powers.map((power) => {
      if (!getAvailableGenericIOs(power).some((available) => available === stat)) return power;
      return {
        ...power,
        slots: Array.from({ length: MAX_SLOTS_PER_POWER }, () => createGenericIOEnhancement(stat as never, 50)),
      };
    });
  build.primary = { ...build.primary, powers: saturate(build.primary.powers) };
  build.secondary = { ...build.secondary, powers: saturate(build.secondary.powers) };
  return build;
}

/** The generic-IO stats whose aspect this Alpha also buffs — the saturation candidates for
 *  [`saturatedBuild`]. Derived from the beta's own stat→aspect map, not a second list. */
function saturationStatsFor(powers: SelectedPower[], alpha: AlphaFixture): string[] {
  const stats = new Set<string>();
  for (const power of powers) {
    for (const stat of getAvailableGenericIOs(power)) {
      const aspect = normalizeAspectName(stat);
      if (aspect && alpha.aspects.includes(aspect)) stats.add(stat);
    }
  }
  return [...stats].sort();
}


/** A power holds at most six slots, so at most six IOs can have been clamped in one aspect. The
 *  bound is what makes the cap-multiple proof a proof rather than a fit to any number. */
const MAX_SLOTS_PER_POWER = 6;

/** Did any projected value for this power change between two runs? Every enhanceable surface the
 *  projection carries, not one chosen aspect: an Alpha that enhances only Heal moves magnitude rows
 *  and no execution tier, and a guard watching recharge alone would call that "no effect". */
/** Did the GRANTED MAGNITUDES move between two projections?
 *
 *  The magnitude-only half of [`projectedValuesMoved`], which returns true on an execution tier
 *  alone. Every "the state reached the engine" floor used that, and the tiers are the dominant
 *  subpopulation: a body could keep asserting the state was observable while the state had
 *  stopped reaching magnitude rows entirely. That was tolerable while the resolver comparison
 *  covered the magnitude half; STRIP-1 removed the resolver's input, so the floor beneath those
 *  bodies IS this now. `mutation-floor-hidden-by-dominant-subpopulation`. */
function magnitudeRowsMoved(
  a: PowerProjection,
  b: PowerProjection,
  tier: 'base' | 'enhanced' | 'final' = 'enhanced',
): boolean {
  const rightRows = new Map(b.grantedMagnitudes.map((row) => [row.rowKey, row]));
  for (const row of a.grantedMagnitudes) {
    const other = rightRows.get(row.rowKey);
    if (other && Math.abs(row.value[tier] - other.value[tier]) > TOLERANCE) return true;
  }
  return false;
}

function projectedValuesMoved(a: PowerProjection, b: PowerProjection): boolean {
  const tiers: (keyof PowerProjection)[] = ['recharge', 'enduranceCost', 'accuracy', 'castTime', 'range'];
  for (const key of tiers) {
    const left = a[key] as ThreeTierValues | null;
    const right = b[key] as ThreeTierValues | null;
    if (!left || !right) continue;
    if (Math.abs(left.enhanced - right.enhanced) > TOLERANCE) return true;
  }
  const rightRows = new Map(b.grantedMagnitudes.map((row) => [row.rowKey, row]));
  for (const row of a.grantedMagnitudes) {
    const other = rightRows.get(row.rowKey);
    if (other && Math.abs(row.value.enhanced - other.value.enhanced) > TOLERANCE) return true;
  }
  return false;
}

/** PROD6D — the projection's own enhancement fractions against `calculatePowerEnhancementBonuses`.
 *  Compared over the UNION of both key sets, so an aspect the engine invents (or drops) is a delta
 *  rather than an absence nobody looks at. */
function enhancementBonusDeltas(
  powerName: string,
  engine: EnhancementBonuses,
  beta: EnhancementBonuses,
): string[] {
  const deltas: string[] = [];
  for (const aspect of new Set([...Object.keys(engine), ...Object.keys(beta)])) {
    const left = engine[aspect] ?? 0;
    const right = beta[aspect] ?? 0;
    if (!withinTolerance(left, right)) {
      deltas.push(`${powerName}.enhancementBonuses.${aspect}: engine ${left} vs beta ${right}`);
    }
  }
  return deltas;
}

/** Diff every power of one fixture: the execution three-tiers, ArcanaTime, perma and granted
 *  magnitudes, against the beta calculators run in the same `state`. The 6B-1 walk inlines this
 *  same body; it is factored out here because PROD6C-3f re-runs the WHOLE diff under two new
 *  inputs, and a second copy of it could drift from the one that grades the base fixture. */
function diffProjection(
  server: Server,
  powers: SelectedPower[],
  build: Build,
  archetypeId: string,
  projection: Map<string, PowerProjection>,
  rawGlobal: ReturnType<typeof mapGlobal>,
  state: ReferenceState = {},
): { deltas: string[]; adjudicated: string[]; rows: number; witness: MagnitudeWitness } {
  const deltas: string[] = [];
  const adjudicated: string[] = [];
  const witness = witnessCounter();
  let rows = 0;

  for (const power of powers) {
    const engine = projection.get(projectionKey(power.powerSet, power.internalName));
    expect(engine, `${server}: no engine projection for ${power.internalName}`).toBeDefined();
    if (!engine) continue;
    const { projection: beta, magnitudes, bag } = betaReference(power, build, archetypeId, rawGlobal, state);
    const perma = permaDelta(engine.perma, beta.perma, undefined, power);
    const mags = countWitness(witness, magnitudeDeltas(
      power.internalName,
      engine.grantedMagnitudes,
      magnitudes,
      bag,
    ));
    rows += engine.grantedMagnitudes.length;
    deltas.push(
      ...tierDelta(`${power.internalName}.recharge`, engine.recharge, beta.recharge),
      ...tierDelta(`${power.internalName}.enduranceCost`, engine.enduranceCost, beta.enduranceCost),
      ...tierDelta(`${power.internalName}.accuracy`, engine.accuracy, beta.accuracy),
      ...tierDelta(`${power.internalName}.castTime`, engine.castTime, beta.castTime),
      ...tierDelta(`${power.internalName}.range`, engine.range, beta.range),
      ...enhancementBonusDeltas(power.internalName, engine.enhancementBonuses, beta.enhancementBonuses),
      ...perma.real.map((d) => `${power.internalName}.${d}`),
      ...mags.real,
    );
    adjudicated.push(
      ...perma.adjudicated.map((d) => `${power.internalName}.${d}`),
      ...mags.adjudicated.map((d) => `${power.internalName}.${d}`),
    );
    const arcanaEngine = engine.arcanaTime ?? null;
    const arcanaBeta = beta.arcanaTime ?? null;
    if (arcanaEngine === null || arcanaBeta === null) {
      if (arcanaEngine !== arcanaBeta) deltas.push(`${power.internalName}.arcanaTime: engine ${arcanaEngine} vs beta ${arcanaBeta}`);
    } else if (Math.abs(arcanaEngine - arcanaBeta) > TOLERANCE) {
      deltas.push(`${power.internalName}.arcanaTime: engine ${arcanaEngine} vs beta ${arcanaBeta}`);
    }
  }
  return { deltas, adjudicated, rows, witness };
}

const suite = artifactsReady ? describe : describe.skip;
if (!artifactsReady) {
  // eslint-disable-next-line no-console
  console.warn('[PROD6B-1 projection parity] skipped — engine artifacts absent; run `npm run build:engine` and build the Node target (see serverParity.test.ts header).');
}

suite('PROD6B-1 — engine per-power projection vs beta calculators, per server', () => {
  beforeAll(() => {
    for (const s of SERVERS) engineHandle(s);
  });

  // [`DISPLAY_MINTED_KEYS`] is the hand-written half of the adjudication floor, and a stale copy
  // of it fails OPEN: a key the builder mints but this set omits reads as an authored effect, and
  // the arm goes back to accepting a bagless power's rows. So re-derive it from the builder over
  // the corpus — hand every power an EMPTY authored bag and collect what still comes out.
  it.each(SERVERS)('%s: the minted-key set still describes buildDisplayEffects', async (server) => {
    await loadDataset(server);
    const minted = new Set<string>();
    let powersSeen = 0;
    for (const atId of STANDARD_ARCHETYPE_IDS) {
      for (const set of getPowersetsForArchetype(atId)) {
        for (const power of set.powers as Power[]) {
          powersSeen += 1;
          const stripped = { ...power, effects: {} as PowerEffects };
          for (const key of Object.keys(buildDisplayEffects(stripped))) minted.add(key);
        }
      }
    }
    expect(powersSeen, `${server}: no powers walked, the derivation is vacuous`).toBeGreaterThan(1000);
    expect(minted.size, `${server}: an emptied bag minted nothing — the builder no longer mints`).toBeGreaterThan(0);
    const undeclared = [...minted].filter((key) => !DISPLAY_MINTED_KEYS.has(key)).sort();
    expect(
      undeclared,
      `${server}: buildDisplayEffects mints these with no authored effects, and DISPLAY_MINTED_KEYS ` +
        `omits them — every one reads as an authored effect and re-opens the arm STRIP-1 made ` +
        `vacuous. Add them to the set.`,
    ).toEqual([]);
  }, 120000);

  // ENGLAG-1 — the display renders the ENGINE's rows now, mapped by `magnitudesFromProjection`.
  //
  // The population is every powerset power of every standard archetype, each projected through
  // `project_power` — the call `usePowerProjection` makes for a power you hover in the picker,
  // which is where the InfoPanel gets these rows. The 18-power fixture build is not the corpus
  // here: it carries no by-type expansion and no protection mez, and a fixture-scoped version of
  // this test passed with the expansion join and the face read both deliberately broken.
  //
  // Two oracles, because neither covers the other's population:
  //
  // - The ENGINE's own row, for the `row_key` -> expansion verdict and the label. The resolver
  //   cannot grade these: a by-type value lives in the authored bag STRIP-1 removed, so every
  //   expanded row is engine-only and a resolver diff sees none of them. What is available is a
  //   JOIN between two fields the engine states independently — an expanded row is keyed
  //   `{effect_key}_{type_key}` AND labelled off the family label, so each checks the other.
  // - The RESOLVER, for the registry join (color, category, quantity), over the rows both
  //   still produce.
  //
  // Then the recovery floor, which is what makes the swap load-bearing: the rows the engine
  // resolves and the display bag no longer carries. Re-point the display at the resolver and it
  // reads zero.
  it.each(SERVERS)('%s: the engine rows map onto the display shape, and recover the bagless powers', async (server) => {
    await loadDataset(server);
    const handle = engineHandle(server);
    const deltas: string[] = [];
    const census = { projected: 0, rows: 0, expanded: 0, faced: 0, qualified: 0, compared: 0, recovered: 0, mezDur: 0, mezMagWithDur: 0, mezMagNoDur: 0 };
    const lost: string[] = [];
    const lostMinted: string[] = [];

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      const build = buildFor(server, atId, 50, false);
      const state = toCharacterStateJson(withoutIllegalSlots(build), CTX);
      for (const set of getPowersetsForArchetype(atId)) {
        for (const power of set.powers as Power[]) {
          const json = handle.project_power(state, set.id!, power.internalName);
          const raw = json ? (JSON.parse(json) as EnginePowerProjection | null) : null;
          if (!raw) continue;
          census.projected += 1;
          const engineRows = mapOnePowerProjection(raw).grantedMagnitudes;
          const adapted = magnitudesFromProjection(engineRows);
          const byKey = new Map(engineRows.map((r) => [r.rowKey, r]));
          const shown = { ...power, powerSet: set.id! } as unknown as SelectedPower;
          // Unslotted on both sides, so neither enhancement nor global reaches either. The tiers
          // are base numbers and the comparison below is shape, not arithmetic.
          const beta = betaMagnitudes(shown, build, atId, {}, {});

          // Every expanded row of one key has to share ONE config object, or the component's
          // reference-equality check stops collapsing the group into its summary row.
          const configs = new Map<string, object>();

          for (const row of adapted) {
            const engine = byKey.get(row.rowKey)!;
            const base = EFFECT_REGISTRY[row.effectKey];
            const where = `${set.id}.${power.internalName}.${row.rowKey}`;
            census.rows += 1;

            if (row.expandedLabel) {
              census.expanded += 1;
              const seen = configs.get(row.effectKey);
              if (seen && seen !== row.config) deltas.push(`${where}: expanded rows of one key hold two config objects`);
              configs.set(row.effectKey, row.config);
              // An expanded row's label is the family label, a `"<family>: <type>"` entry, or
              // the collapsed `"<family> (All)"`. Anything else means `isExpanded` said yes to a
              // row the engine did not expand.
              const ok = engine.label === base.label
                || engine.label.startsWith(`${base.label}: `)
                || engine.label === `${base.label} (All)`;
              if (!ok) deltas.push(`${where}: marked expanded, but label "${engine.label}" is not off "${base.label}"`);
              if (row.config.label !== base.label) deltas.push(`${where}: expanded row's config lost the family label`);
            } else {
              // A row that is NOT an expansion renders `config.label`, so that has to BE the
              // engine's label — including the mez face and the percent-form qualifier the
              // engine folds into it.
              if (row.config.label !== engine.label) {
                deltas.push(`${where}: config.label "${row.config.label}" is not the engine's "${engine.label}"`);
              }
              if (engine.label !== base.label) census.qualified += 1;
            }

            // The face rides on the label, so the label decides whether one is owed.
            const owed = engine.label.endsWith(' Prot') ? 'protection'
              : engine.label.endsWith(' (Self)') ? 'self' : undefined;
            if (owed) census.faced += 1;
            if (row.mezFace !== owed) deltas.push(`${where}: mezFace ${row.mezFace} on label "${engine.label}"`);

            if (row.config.format !== engine.format) deltas.push(`${where}: format ${row.config.format} vs ${engine.format}`);
            if (row.quantity.kind !== engine.quantity.kind) deltas.push(`${where}: quantity ${row.quantity.kind} vs ${engine.quantity.kind}`);
            if (row.byTypeLabel !== (engine.byTypeLabel ?? undefined)) deltas.push(`${where}: byTypeLabel ${row.byTypeLabel} vs ${engine.byTypeLabel}`);

            // ENGLAG-2 — the seconds travel on the row now, not in a `durations` map the bag
            // no longer carries. A MAGNITUDE-valued mez
            // row's tier is the rank, so it must carry its duration here (that is the half the
            // tier cannot show); a DURATION-valued mez row's tier already IS the seconds, so
            // it must NOT (the engine's `recorded_duration` returns `null` for it) — printing
            // a second number beside a tier that is already that number is the MEZDUR-1
            // double-state under a new field name. The adapter reads `row.duration`, never
            // `effects.durations`, so the only injection point is the engine row this walks.
            if (engine.quantity.kind === 'mez_duration') {
              census.mezDur += 1;
              if (row.duration != null) deltas.push(`${where}: duration ${row.duration} on a row whose tier is already seconds`);
            } else if (engine.quantity.kind === 'mez_magnitude') {
              const tree = atomsOf(power);
              if (tree.length > 0 && row.duration != null) census.mezMagWithDur += 1;
              if (row.duration !== (engine.duration ?? undefined)) deltas.push(`${where}: adapted duration ${row.duration} vs engine ${engine.duration}`);
            }

            // …and the REGISTRY join, on the rows the resolver still produces: both sides look
            // the config up by effect key, so a row they share has to come out drawn the same.
            // Only what the registry owns — the engine's own values are graded above, and
            // against the resolver they carry `magnitudeDeltas`' known data divergences.
            const ref = beta.get(row.rowKey);
            if (!ref) continue;
            census.compared += 1;
            if (row.config.colorClass !== ref.config.colorClass) deltas.push(`${where}: colorClass ${row.config.colorClass} vs ${ref.config.colorClass}`);
            if (row.config.category !== ref.config.category) deltas.push(`${where}: category ${row.config.category} vs ${ref.config.category}`);
          }

          if (atomsOf(power).length === 0) continue;
          census.recovered += adapted.filter((r) => !beta.has(r.rowKey)).length;
          for (const [key, ref] of beta) {
            if (adapted.some((r) => r.rowKey === key)) continue;
            // A MINTED key is not an authored effect the swap dropped: both sides derive it
            // for themselves (`buildDisplayEffects` here, `atom_seed`'s power-level fallbacks
            // there), so a delta is those two derivations disagreeing, which is
            // `magnitudeDeltas`' subject. Counted rather than failed, with a ceiling so the
            // population cannot grow unnoticed.
            const where = `${set.id}.${power.internalName}.${key}`;
            if (DISPLAY_MINTED_KEYS.has(ref.effectKey)) lostMinted.push(where);
            else lost.push(where);
          }
        }
      }
    }

    // eslint-disable-next-line no-console
    console.warn(`[ENGLAG-1 adapter] ${server}: ${census.projected} powers, ${census.rows} rows (${census.expanded} expanded, ${census.faced} faced, ${census.qualified} qualified), ${census.compared} cross-checked, ${census.recovered} recovered, ${lost.length} lost (+${lostMinted.length} minted)`);

    // Every rule above is graded only where the corpus carries its shape, so each population is
    // named. The first version of this test asserted none of them and was blind to two.
    expect(census.projected, `${server}: nothing projected`).toBeGreaterThan(500);
    expect(census.expanded, `${server}: no EXPANDED row — the row_key join is ungraded`).toBeGreaterThan(100);
    expect(census.faced, `${server}: no mez FACE — the face read is ungraded`).toBeGreaterThan(10);
    expect(census.qualified, `${server}: no row whose engine label differs from the registry's — the label carry is ungraded`).toBeGreaterThan(10);
    expect(census.compared, `${server}: no row cross-checked against the resolver`).toBeGreaterThan(500);
    // ENGLAG-2: the duration floor. A magnitude-valued mez row must reach the gate CARRYING a
    // non-null duration — otherwise the seconds half is gone again and this grades nothing.
    // Set well under the corpus count so it is a collapse detector, not a pin.
    expect(census.mezMagWithDur, `${server}: no magnitude-valued mez row carrying its duration`).toBeGreaterThan(30);
    expect(deltas.slice(0, 20), `${server}: ${deltas.length} adapter shape deltas`).toEqual([]);
    expect(
      census.recovered,
      `${server}: the engine path recovered no row the display bag has lost. Either the bag is ` +
        `no longer stripped (retire this arm), or the projection stopped seeding from atoms.`,
    ).toBeGreaterThan(5000);
    expect(
      lost.slice(0, 20),
      `${server}: ${lost.length} AUTHORED rows resolve from the display bag and NOT from the ` +
        `engine — the swap DROPS them. Each is a bag key carrying an effect no atom states.`,
    ).toEqual([]);
    // One when this landed (`Assassins_Strike.buffDuration` on homecoming), and it is the two
    // buffDuration derivations disagreeing rather than a lost effect. Ceiling, not zero: the
    // arm exists to notice the population growing.
    expect(
      lostMinted.slice(0, 20).length,
      `${server}: ${lostMinted.length} minted rows the engine does not derive: ${lostMinted.slice(0, 5).join(', ')}`,
    ).toBeLessThan(3);
  }, 900000);

  // …and that the surfaces actually PASS those rows. Every arm above grades
  // `magnitudesFromProjection` directly, so deleting the one prop that reaches the component
  // would leave all of them green while the panel went back to rendering minted keys only.
  // There is no render harness in this repo, so this reads the call sites (ENGLAG-1).
  //
  // `CompareSlottingModal` was the third caller and was held back on the reading that its
  // subject is a HYPOTHETICAL slotting while `project_power` resolves the build's own. PROD6D
  // had already retired that: `useHypotheticalCalculation` runs the engine over the proposed
  // slotting, and the modal was reading `enhancementBonuses` off that very projection while
  // resolving its rows from the stripped bag beside it. It passes the rows since 2026-09-07.
  //
  // This list is the members; `beta-display.test.ts` pins the ROSTER, so a fourth surface
  // cannot ship on the bag path by simply not appearing here.
  it.each([
    ['components/info/InfoPanel.tsx'],
    ['components/info/PowerInfoTooltip.tsx'],
    ['components/modals/CompareSlottingModal.tsx'],
  ])('%s feeds the projection rows to RegistryEffectsDisplay', (relative) => {
    const source = readFileSync(join(__dirname, '..', ...relative.split('/')), 'utf8');
    expect(source, `${relative} no longer adapts the projection`).toMatch(/magnitudesFromProjection\(\w+\.grantedMagnitudes\)/);
    const call = source.slice(source.indexOf('<RegistryEffectsDisplay'));
    expect(
      call.slice(0, call.indexOf('/>')),
      `${relative} stopped passing \`rows\`, so it resolves the stripped display bag again`,
    ).toContain('rows={effectRows}');
  });

  // PROD6B-BETA-PARITY class 2 — which duration the rendered rows state.
  //
  // The precedence in `effectRowDuration.ts` (the row's own duration, then the bag's per-effect
  // `durations` map, then its power-level `buffDuration`) is unchanged. What was wrong was a
  // SUPPLIER. `buildDisplayEffects` backfilled a summon's lifespan into `buffDuration` under the
  // guard "only where the power states no duration of its own" — a guard reading a key STRIP-1
  // removed from every power on every fork, so it became universally true and fired on all
  // 110 / 111 / 120 / 119 summon powers per fork. That put a PET'S LIFESPAN into the slot meaning
  // the power's own effect clock: two different quantities, one slot, and the pet's number won
  // wherever no row could speak. Removed 2026-09-07; the lifespan keeps its own home on all three
  // surfaces through `formatSummonDuration`, which also states the 99999s sentinel as words.
  //
  // The oracle is the ENGINE's rows — the atom-derived durations — not a restatement of the
  // function under test. The live tripwire is the supply census: a bag duration with no authored
  // value behind it means a mint is back, and a mint is what this closed.
  it.each(SERVERS)('%s: the rendered duration follows the engine rows, and no bag key mints one', async (server) => {
    await loadDataset(server);
    const handle = engineHandle(server);
    const deltas: string[] = [];
    // A bag duration with nothing authored behind it — the backfill, or its next incarnation.
    const minted: string[] = [];
    // Powers whose recorded durations reach no row, so the engine's power-level plurality and
    // its rows answer differently. Named below rather than failed.
    const pluralityOutranks: string[] = [];
    const census = { projected: 0, summon: 0, agreedRows: 0, mixed: 0, disputed: 0, bagAnswered: 0 };

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      const build = buildFor(server, atId, 50, false);
      const state = toCharacterStateJson(withoutIllegalSlots(build), CTX);
      for (const set of getPowersetsForArchetype(atId)) {
        for (const power of set.powers as Power[]) {
          const json = handle.project_power(state, set.id!, power.internalName);
          const raw = json ? (JSON.parse(json) as EnginePowerProjection | null) : null;
          if (!raw) continue;
          census.projected += 1;
          if (carriesSummon(power)) census.summon += 1;
          const shown = { ...power, powerSet: set.id! } as unknown as SelectedPower;
          const engineRows = mapOnePowerProjection(raw).grantedMagnitudes;
          const adapted = magnitudesFromProjection(engineRows);
          const bag = displayBag(shown) as BagDurations;
          const where = `${set.id}.${power.internalName}`;

          // The supply census. A surviving bag duration is legitimate only where the power's own
          // `effects` authored one — which is why this reads the AUTHORED object and not the
          // built bag on the right-hand side. Keyed per power, so a same-named template in
          // another set cannot stand in for this one.
          const authored = power.effects as
            { buffDuration?: number; effectDuration?: number; durations?: Record<string, number> } | undefined;
          if (bag.buffDuration != null && authored?.buffDuration == null && authored?.effectDuration == null) {
            minted.push(`${where}.buffDuration=${bag.buffDuration}`);
          }
          if (bag.durations && Object.keys(bag.durations).length > 0
            && Object.keys(authored?.durations ?? {}).length === 0) {
            minted.push(`${where}.durations=${Object.keys(bag.durations).join('/')}`);
          }

          const sources = adapted.map((r) => ({
            effectKey: r.effectKey,
            category: r.config.category,
            duration: r.duration,
          }));
          const rendered = powerLevelDuration(sources, bag);

          // The engine's own power-level answer, off the atom-derived durations map.
          const engineBuffDur = engineRows.find((r) => r.rowKey === 'buffDuration')?.value.base;
          const timedRowDurs = sources
            .filter((r) => r.category !== 'execution' && r.duration != null && r.duration > 0)
            .map((r) => r.duration as number);
          const rowsAgree = timedRowDurs.length > 0
            && timedRowDurs.every((d) => Math.abs(d - timedRowDurs[0]) < 0.001);

          if (rowsAgree) {
            census.agreedRows += 1;
            if (bag.buffDuration != null && Math.abs(bag.buffDuration - timedRowDurs[0]) > 0.001) census.disputed += 1;
            if (rendered == null || Math.abs(rendered - timedRowDurs[0]) > 0.001) {
              deltas.push(`${where}: rendered ${rendered} but the engine rows state ${timedRowDurs[0]} (bag says ${bag.buffDuration})`);
            }
            // The engine's own power-level plurality can outrank every row's duration, and that
            // is not a contradiction: it votes over the whole `durations` map while a row
            // surfaces one only for its OWN bag key. Thunderspy's Time Lord is the case — the
            // self +Recharge and three movement axes record 10s and reach no row carrying it,
            // the eight damageBuff rows record 9.17s and do, so the plurality says 10 and every
            // row that can speak says 9.17. Counted rather than failed: an answer built from the
            // rows that agree is the best claim this component can make. The ceiling is the
            // tripwire, and it is over every agreeing-row power now that the arm is no longer
            // nested inside a `disputed` branch the backfill used to inflate.
            if (engineBuffDur != null && Math.abs(engineBuffDur - timedRowDurs[0]) > 0.001) {
              pluralityOutranks.push(`${where}: rows ${timedRowDurs[0]} vs plurality ${engineBuffDur}`);
            }
          } else if (timedRowDurs.length > 1) {
            // Genuinely mixed durations. One power-level number would be a claim the power does
            // not make, so the honest answer is none unless the bag authored one.
            census.mixed += 1;
            if (rendered != null && (bag.buffDuration == null || Math.abs(rendered - bag.buffDuration) > 0.001)) {
              deltas.push(`${where}: rendered ${rendered} on rows that disagree (${timedRowDurs.join('/')})`);
            }
          }

          // No row carries a duration, so the bag is the only source left and must still answer.
          if (timedRowDurs.length === 0 && bag.buffDuration != null && bag.buffDuration > 0) {
            census.bagAnswered += 1;
            if (rendered == null || Math.abs(rendered - bag.buffDuration) > 0.001) {
              deltas.push(`${where}: rowless power rendered ${rendered}, dropping the bag's ${bag.buffDuration}`);
            }
          }
        }
      }
    }

    // eslint-disable-next-line no-console
    console.warn(`[class-2 duration] ${server}: ${census.projected} powers (${census.summon} summon), ${census.agreedRows} with agreeing rows, ${census.mixed} mixed, ${census.disputed} disputed, ${census.bagAnswered} bag-answered, ${minted.length} minted, ${pluralityOutranks.length} plurality-outranks`);
    expect(census.projected, `${server}: nothing projected`).toBeGreaterThan(500);
    expect(census.agreedRows, `${server}: no power whose rows agree — the rule is ungraded`).toBeGreaterThan(0);
    expect(census.mixed, `${server}: no power with mixed durations — the "no single answer" arm is ungraded`).toBeGreaterThan(0);
    // The `disputed` and `bagAnswered` populations are NOT asserted non-zero any more, and the
    // reason is the closure itself: after the backfill went, the only bag left stating a duration
    // is the handful of powers that AUTHOR one (homecoming's Assassin's Strike is the whole
    // population on that fork, and the other three carry none). Asserting a floor on them would
    // be asserting that the defect is still present. They stay in the census so the numbers are
    // visible; the claim that replaced them is `minted`.
    expect(census.summon, `${server}: no summon power projected — the minted-supplier claim is vacuous`).toBeGreaterThan(50);
    expect(
      minted.slice(0, 10),
      `${server}: ${minted.length} display-bag durations with nothing authored behind them — a `
        + `mint is back. The summon-lifespan backfill was exactly this shape.`,
    ).toEqual([]);
    expect(deltas.slice(0, 20), `${server}: ${deltas.length} duration deltas`).toEqual([]);
    expect(
      pluralityOutranks.length,
      `${server}: ${pluralityOutranks.length} powers whose recorded durations reach no row: ${pluralityOutranks.slice(0, 5).join(', ')}`,
    ).toBeLessThan(35);
  }, 900000);

  it.each(SERVERS)('%s: every selected power projects to the beta calculators', async (server) => {
    await loadDataset(server); // activates this dataset for the beta calculators
    const build = buildFor(server);
    const powers = [...build.primary.powers, ...build.secondary.powers];
    expect(powers.length, `${server}: fixture selected no powers`).toBeGreaterThan(0);

    const totals = engineTotals(server, build);
    const projection = mapPowerProjection(totals.power_projection);
    const rawGlobal = mapGlobal(totals.bonuses, totals.stats);

    // Guard the fixture: at least one power must actually project (else a trivial all-null match).
    expect(projection.size, `${server}: engine projected nothing`).toBeGreaterThan(0);
    const archetypeId = build.archetype.id!;
    // …and the magnitude half must be exercised, else an empty-vs-empty diff passes vacuously.
    const magnitudeRows = [...projection.values()].reduce((n, p) => n + p.grantedMagnitudes.length, 0);
    expect(magnitudeRows, `${server}: engine resolved no granted magnitudes`).toBeGreaterThan(0);

    const matchedRows = powers.reduce((n, power) => {
      const engine = projection.get(projectionKey(power.powerSet, power.internalName));
      if (!engine) return n;
      const { magnitudes } = betaReference(power, build, archetypeId, rawGlobal);
      return n + engine.grantedMagnitudes.filter((r) => magnitudes.has(r.rowKey)).length;
    }, 0);
    const { deltas, adjudicated, witness } = diffProjection(server, powers, build, archetypeId, projection, rawGlobal);

    // eslint-disable-next-line no-console
    console.warn(`[PROD6B-2 magnitudes] ${server}: ${magnitudeRows} engine rows across ${powers.length} powers, ${matchedRows} matched key-for-key`);
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`\n[PROD6B-1 projection parity] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6B-1 projection parity] ${server} (${powers.length} powers)\n    ${deltas.join('\n    ')}`);
    }
    // The magnitude half of this body is unwitnessed: STRIP-1 left the fixture's bags holding
    // minted keys only (0 / 0 / 1 / 0 powers carry an authored one), so the comparison above
    // grades the execution tiers, ArcanaTime, perma and enhancement bonuses, and the magnitude
    // rows only where the resolver can still speak. Floored on the rows existing at all; their
    // shape is graded by the ENGLAG-1 adapter arm over the whole corpus.
    // eslint-disable-next-line no-console
    console.warn(witnessLine(server, 'every selected power projects to the beta calculators', witness));
    expect(deltas).toEqual([]);
  }, 120000); // dataset load + per-power calculators over a full build; matches the suite's
  // --testTimeout, for the same reason as serverParity.ts: 30000 was chosen to beat vitest's 5s
  // default and silently became a ceiling under the suite's 120s once npm test set one.

  // The 6B-1 fixture is one archetype, whose powers are almost all attacks — it never reaches the
  // by-type expansion (defense/resistance), mez protection, absorb, mez duration or knockback
  // distance branches, which live in armor and control sets. This sweeps EVERY standard archetype
  // so those branches are actually graded. Magnitudes only; the execution/perma half is covered
  // above.
  it.each(SERVERS)('%s: granted magnitudes match the beta resolver for every archetype', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    const kinds = new Map<string, number>();
    let rows = 0;
    // PROD6C-3c reach — the rows each side resolved from a key only the pseudo-pet merge
    // supplies. Parity alone would stay green with the merge dropped from BOTH sides, so the
    // merge has to be shown to produce rows at all (the 6B-2b method).
    let petPowers = 0;
    let enginePetRows = 0;
    let betaPetRows = 0;
    // Anti-vacuity for the summon fixture: the widening is only real while it keeps reaching
    // powers that summon. A chooser that stops finding them would leave every arm below green.
    let summonPowers = 0;
    // The keys the beta answered from a pet's underlay where the engine answered from the
    // power's own atom, with both values — graded against [`OWN_ROW_OVER_PET_UNDERLAY`].
    const underlayOutranked: Record<string, { own: number; underlay: number }> = {};

    const engineBundle = bundlePowers(server);
    // Two fixtures per archetype. The first-set default is the 6B corpus; the second picks the
    // sets that SUMMON, because the default reaches almost none of them — measured, the whole
    // rain / patch / henchman family (Caltrops, Trip Mine, Rain of Fire, Haunt, Dark Extraction)
    // produced no delta row of ANY kind, so a class sized off this sweep was sized off fixture
    // coverage and not off the powers carrying the mechanic. Discovered by counting each set's
    // own carriers, so no powerset is named (Rule 0), the same way the 3k/3l sweeps choose.
    const summonRichest = (sets: { powers: Power[] }[]) => {
      const carried = sets.map((set) => set.powers.filter(carriesSummon).length);
      return carried.indexOf(Math.max(...carried));
    };
    const graded = new Set<string>();
    for (const atId of STANDARD_ARCHETYPE_IDS) {
    for (const chooseSet of [undefined, summonRichest]) {
      const build = buildFor(server, atId, 50, true, chooseSet);
      const powers = [...build.primary.powers, ...build.secondary.powers];
      if (powers.length === 0) continue;

      const totals = engineTotals(server, build);
      const projection = mapPowerProjection(totals.power_projection);
      const rawGlobal = mapGlobal(totals.bonuses, totals.stats);

      for (const power of powers) {
        const engine = projection.get(projectionKey(power.powerSet, power.internalName));
        if (!engine) continue;
        // The two fixtures overlap wherever the summon-richest set IS the first one.
        if (graded.has(projectionKey(power.powerSet, power.internalName))) continue;
        graded.add(projectionKey(power.powerSet, power.internalName));
        if (carriesSummon(power as Power)) summonPowers += 1;
        const { magnitudes, bag } = betaReference(power, build, atId, rawGlobal);
        // The engine's OWN copy of the power (the 6C-3h evidence, extended to this sweep by the
        // 2026-08-19 oracle re-cut). Absent on a lookup miss, which keeps every row strict there.
        const enginePower = engineBundle.get(projectionKey(power.powerSet, power.internalName));
        const mags = countWitness(witness, magnitudeDeltas(
          `${atId}/${power.internalName}`,
          engine.grantedMagnitudes,
          magnitudes,
          bag,
          enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, build, atId, rawGlobal)) : undefined,
          power,
        ));
        deltas.push(...mags.real);
        adjudicated.push(
          ...mags.adjudicated.map((d) => `${atId}/${power.internalName}.${d}`),
        );
        for (const row of mags.underlayOutranked) {
          underlayOutranked[`${atId}/${power.internalName}.${row.key}`] = {
            own: Number(row.own.toFixed(2)),
            underlay: Number(row.underlay.toFixed(2)),
          };
        }
        rows += engine.grantedMagnitudes.length;
        const petKeys = pseudoPetKeys(power, shownPower(power, build, atId, rawGlobal));
        if (petKeys.size > 0) {
          petPowers += 1;
          enginePetRows += engine.grantedMagnitudes.filter((row) => petKeys.has(row.effectKey)).length;
          betaPetRows += [...magnitudes.values()].filter((row) => petKeys.has(row.effectKey)).length;
        }
        for (const row of engine.grantedMagnitudes) {
          const kind = row.rowKey === row.effectKey ? row.quantity.kind : `${row.quantity.kind}+expanded`;
          kinds.set(kind, (kinds.get(kind) ?? 0) + 1);
        }
      }
    }
    }

    // eslint-disable-next-line no-console
    console.warn(`[PROD6B-2 magnitudes] ${server} sweep: ${rows} rows — ${[...kinds].map(([k, n]) => `${k}:${n}`).join(' ')}`);
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6B-2 magnitudes] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6B-2 magnitudes] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 60).join('\n    ')}`);
    }
    // eslint-disable-next-line no-console
    console.warn(`[PROD6C-3c pseudo-pet] ${server} sweep: ${petPowers} powers carry pet-only effect keys, ${enginePetRows} engine / ${betaPetRows} beta rows resolve from one`);
    // Guard against a vacuous pass: the sweep must reach the expanded by-type branch.
    expect([...kinds.keys()].some((k) => k.endsWith('+expanded')), `${server}: sweep reached no expanded by-type row`).toBe(true);
    expect(summonPowers, `${server}: the summon fixture reached no power carrying a summon`).toBeGreaterThan(20);
    expect(enginePetRows, `${server}: no engine row resolved from a pseudo-pet key — the merge is ungraded here`).toBeGreaterThan(0);
    expect(betaPetRows, `${server}: no beta row resolved from a pseudo-pet key — the merge is ungraded here`).toBeGreaterThan(0);
    // Anti-vacuity for the magnitude half. STRIP-1 left most of the corpus unable to answer, and
    // an arm that grades zero powers is green for the wrong reason — so the population the
    // resolver CAN still witness is floored rather than assumed.
    expect(
      witness.witnessed,
      witnessLine(server, 'granted magnitudes match the beta resolver for every archetype', witness),
    ).toBeGreaterThan(0);
    // The own-row-over-underlay keys, by value: see [`OWN_ROW_OVER_PET_UNDERLAY`].
    expect(underlayOutranked, `${server}: own row vs pet underlay`).toEqual(OWN_ROW_OVER_PET_UNDERLAY[server] ?? {});
    expect(deltas).toEqual([]);
  }, 120000);

  // PROD6C. The info surfaces render powers the build does NOT hold — the tooltip fires off the
  // picker (`AvailablePowers`), which resolves through `lookupPower`, not the build. No
  // `all_selected()` walk reaches those, so the projection could not answer for them at all and
  // the surfaces would have had to keep a second calculator for the unheld case. `project_power`
  // closes that: it projects an arbitrary ref against the same finalized accumulator.
  //
  // What must hold is that the on-demand answer is the SAME answer, not a parallel one. Each
  // unheld power is graded against the beta calculators run unslotted — the state it is actually
  // in — over every archetype's second powerset, which the fixture never picks.
  it.each(SERVERS)('%s: a power the build does not hold projects on demand', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let projected = 0;
    let magnitudeRows = 0;

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      const build = buildFor(server, atId);
      const held = new Set(
        [...build.primary.powers, ...build.secondary.powers].map((p) => projectionKey(p.powerSet, p.internalName)),
      );
      // A set the fixture did NOT take, so its powers are genuinely unheld.
      const unheldSet = getPowersetsForArchetype(atId).filter(
        (ps) => !ps.dormant && ps.id && ps.id !== build.primary.id && ps.id !== build.secondary.id,
      )[0];
      if (!unheldSet?.id) continue;

      const stateJson = toCharacterStateJson(withoutIllegalSlots(build), CTX);
      const totals = engineTotals(server, build);
      const rawGlobal = mapGlobal(totals.bonuses, totals.stats);

      for (const power of (unheldSet.powers ?? []).filter(pickable)) {
        expect(held.has(projectionKey(unheldSet.id, power.internalName)), `${server}: ${power.internalName} is not actually unheld`).toBe(false);

        const json = engineHandle(server).project_power(stateJson, unheldSet.id, power.internalName);
        const raw = JSON.parse(json) as Parameters<typeof mapOnePowerProjection>[0] | null;
        expect(raw, `${server}: no on-demand projection for ${unheldSet.id}/${power.internalName}`).not.toBeNull();
        if (!raw) continue;
        const engine = mapOnePowerProjection(raw);
        projected += 1;
        magnitudeRows += engine.grantedMagnitudes.length;

        // The beta reference for an UNHELD power: the same calculators, run with no slots.
        const unslotted = { ...power, powerSet: unheldSet.id, level: power.available + 1, slots: [] } as SelectedPower;
        const { projection: beta, magnitudes, bag } = betaReference(unslotted, build, atId, rawGlobal);
        // The engine's OWN copy (the 6C-3h evidence, extended here by the 2026-08-19 re-cut) —
        // for the magnitude drift branches and for the perma null boundary alike.
        const enginePower = bundlePowers(server).get(projectionKey(unheldSet.id, power.internalName));
        const mags = countWitness(witness, magnitudeDeltas(
          `${atId}/${power.internalName}`,
          engine.grantedMagnitudes,
          magnitudes,
          bag,
          enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, build, atId, rawGlobal)) : undefined,
        ));
        const perma = permaDelta(engine.perma, beta.perma, enginePower, unslotted);
        deltas.push(
          ...tierDelta(`${atId}/${power.internalName}.recharge`, engine.recharge, beta.recharge),
          ...tierDelta(`${atId}/${power.internalName}.enduranceCost`, engine.enduranceCost, beta.enduranceCost),
          ...tierDelta(`${atId}/${power.internalName}.accuracy`, engine.accuracy, beta.accuracy),
          ...tierDelta(`${atId}/${power.internalName}.castTime`, engine.castTime, beta.castTime),
          ...tierDelta(`${atId}/${power.internalName}.range`, engine.range, beta.range),
          ...perma.real.map((d) => `${atId}/${power.internalName}.${d}`),
          ...mags.real,
        );
        adjudicated.push(
          ...perma.adjudicated.map((d) => `${atId}/${power.internalName}.${d}`),
          ...mags.adjudicated.map((d) => `${atId}/${power.internalName}.${d}`),
        );
      }
    }

    // eslint-disable-next-line no-console
    console.warn(`[PROD6C on-demand] ${server}: ${projected} unheld powers projected, ${magnitudeRows} magnitude rows`);
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C on-demand] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C on-demand] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 40).join('\n    ')}`);
    }
    // Guard the sweep: an empty walk would pass vacuously.
    expect(projected, `${server}: no unheld power was projected`).toBeGreaterThan(0);
    expect(magnitudeRows, `${server}: no unheld power resolved a magnitude row`).toBeGreaterThan(0);
    // Anti-vacuity for the magnitude half. STRIP-1 left most of the corpus unable to answer, and
    // an arm that grades zero powers is green for the wrong reason — so the population the
    // resolver CAN still witness is floored rather than assumed.
    expect(
      witness.witnessed,
      witnessLine(server, 'a power the build does not hold projects on demand', witness),
    ).toBeGreaterThan(0);
    expect(deltas).toEqual([]);
  }, 120000);

  // PROD6C-3h. The pool and epic partitions, which no projection gate had ever reached: the sweeps
  // above walk powerSETS (the fixture's own two, then an unpicked one), so every pool and epic
  // power went ungraded — the PROD6B-2b blind spot exactly, where `getAllPowersets()` omitting
  // pools and epics hid 100% of that finding.
  //
  // Both partitions arrive in the converter's LEGACY shape: identity only in `fullName`, execution
  // stats under `endurance` / `activationTime`. The beta normalizes them at runtime
  // (`transformPoolPower`); `coh_data` now does the same at bundle load. Two guards below prove the
  // gate can actually SEE a regression in that, rather than passing on a corpus that cannot reach
  // it — the 6C-3f method:
  //
  //   * `renamed` counts powers whose internal name differs from their display name (Swift/Quick,
  //     Hover/Combat_Flight — the game renamed them and kept the original internal name). These are
  //     addressable ONLY through the fullName derivation, so a non-zero count is what makes the
  //     non-null assertion load-bearing.
  //   * `shared` counts internal names published by more than one epic mastery at their own scales.
  //     A lookup that ignores the requested set answers with another archetype's copy, so a
  //     non-zero count is what makes the set-scoped resolution load-bearing.
  it.each(SERVERS)('%s: pool and epic powers project through their legacy shape', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const build = buildFor(server);
    const atId = build.archetype.id as string;
    const stateJson = toCharacterStateJson(withoutIllegalSlots(build), CTX);
    const totals = engineTotals(server, build);
    const rawGlobal = mapGlobal(totals.bonuses, totals.stats);
    const engineBundle = bundlePowers(server);

    const partitions: { label: string; sets: { id: string; powers: Power[] }[] }[] = [
      { label: 'pool', sets: Object.values(getAllPowerPools()) as { id: string; powers: Power[] }[] },
      { label: 'epic', sets: Object.values(getAllEpicPools()) as { id: string; powers: Power[] }[] },
    ];

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let projected = 0;
    let magnitudeRows = 0;
    let renamed = 0;
    const epicNameOwners = new Map<string, Set<string>>();
    // The recovered mirror-blind windows, read off the ENGINE at the tags the roster names. A tag
    // the sweep never reaches simply never lands here, which is the whole point of pinning values.
    const recoveredWindows: Record<string, number | null> = {};

    for (const { label, sets } of partitions) {
      for (const set of sets) {
        for (const power of set.powers ?? []) {
          if (power.internalName !== power.name.replace(/\s+/g, '_')) renamed += 1;
          if (label === 'epic') {
            const owners = epicNameOwners.get(power.internalName) ?? new Set<string>();
            owners.add(set.id);
            epicNameOwners.set(power.internalName, owners);
          }

          const tag = `${label}/${set.id}/${power.internalName}`;
          const json = engineHandle(server).project_power(stateJson, set.id, power.internalName);
          const raw = JSON.parse(json) as Parameters<typeof mapOnePowerProjection>[0] | null;
          expect(raw, `${server}: no projection for ${tag} — the partition's identity is unreachable`).not.toBeNull();
          if (!raw) continue;
          const engine = mapOnePowerProjection(raw);
          projected += 1;
          magnitudeRows += engine.grantedMagnitudes.length;

          // A pool/epic power is never held by this fixture, so the beta reference is the same
          // calculators run unslotted — identical to the on-demand sweep above.
          const unslotted = { ...power, powerSet: set.id, level: power.available + 1, slots: [] } as SelectedPower;
          const { projection: beta, magnitudes, bag } = betaReference(unslotted, build, atId, rawGlobal);
          // The engine's OWN copy of this power — the evidence both adjudications rest on.
          const enginePower = engineBundle.get(projectionKey(set.id, power.internalName));
          const mags = countWitness(witness, magnitudeDeltas(
            tag,
            engine.grantedMagnitudes,
            magnitudes,
            bag,
            enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, build, atId, rawGlobal)) : {},
          ));
          const perma = permaDelta(engine.perma, beta.perma, enginePower, power);
          if (tag in (MIRROR_RECOVERED_WINDOWS[server] ?? {})) {
            recoveredWindows[tag] = engine.perma?.duration ?? null;
          }
          deltas.push(
            ...tierDelta(`${tag}.recharge`, engine.recharge, beta.recharge),
            ...tierDelta(`${tag}.enduranceCost`, engine.enduranceCost, beta.enduranceCost),
            ...tierDelta(`${tag}.accuracy`, engine.accuracy, beta.accuracy),
            ...tierDelta(`${tag}.castTime`, engine.castTime, beta.castTime),
            ...tierDelta(`${tag}.range`, engine.range, beta.range),
            ...perma.real.map((d) => `${tag}.${d}`),
            ...mags.real,
          );
          adjudicated.push(
            ...perma.adjudicated.map((d) => `${tag}.${d}`),
            ...mags.adjudicated.map((d) => `${tag}.${d}`),
          );
        }
      }
    }

    const shared = [...epicNameOwners.values()].filter((owners) => owners.size > 1).length;

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3h partitions] ${server}: ${projected} pool/epic powers projected, ${magnitudeRows} magnitude rows, ` +
        `${renamed} carry an internal name their display name does not yield, ${shared} epic names published by more than one mastery`,
    );
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3h partitions] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.slice(0, 20).join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C-3h partitions] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 40).join('\n    ')}`);
    }
    expect(projected, `${server}: no pool or epic power was projected`).toBeGreaterThan(0);
    expect(magnitudeRows, `${server}: no pool or epic power resolved a magnitude row`).toBeGreaterThan(0);
    expect(
      renamed,
      `${server}: no pool/epic power's internal name differs from its display name — the non-null assertion above cannot detect a lost fullName derivation`,
    ).toBeGreaterThan(0);
    expect(
      shared,
      `${server}: no epic internal name is published by two masteries — this sweep cannot detect an unscoped pool/epic lookup`,
    ).toBeGreaterThan(0);
    // The mirror-blind class, closed 2026-09-07 and pinned by the window it recovered rather than
    // by the absence it used to be. `deltas` already fails if either side goes null on its own —
    // this is the half that check cannot make: that the sweep still REACHES these powers, so a
    // tag quietly leaving the corpus is a red rather than a green with nothing behind it.
    expect(
      recoveredWindows,
      `${server}: the windows the engine's mirror recovered`,
    ).toEqual(MIRROR_RECOVERED_WINDOWS[server] ?? {});
    // No floor on the witnessed population here: it is zero on every fork, and pinning it at zero
    // would assert the strip stays. The verdict for this body's magnitude half is the engine-side
    // floor above; this states the skip rather than leaving it silent.
    // eslint-disable-next-line no-console
    console.warn(witnessLine(server, 'pool and epic powers project through their legacy shape', witness));
    expect(deltas).toEqual([]);
  }, 180000);

  // PROD6C. The active +Strength self-buffs (Power Boost family) land in a magnitude row's FINAL
  // column exactly like a build-wide global — InfoPanel has always folded them in, the tooltip
  // never did, and the engine did neither. Every fixture above activates only toggles and autos,
  // so `strength*` is zero throughout and both sides agreed on a row nobody had augmented: the
  // same shape as the 6B-2c level pin, a rule kept alive by a corpus that could not reach it.
  //
  // Three assertions, and the middle one is the point. Parity alone would stay green if the fold
  // were deleted from BOTH sides, so the gate also proves the fold is observable at all — the
  // REAL resolver run twice over the same fixture, with and without the augmentation (the 6B-2b
  // method: no reimplementation of the code under test).
  it.each(SERVERS)('%s: +Strength self-buffs reach the granted magnitudes', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();
    const candidates = strengthCandidates(server);
    // This used to warn "fork carries no specialBuff power" and RETURN. Between STRIP-1 and
    // 2026-09-07 that branch was taken on ALL FOUR forks, because `grantsStrength` read the
    // emptied `effects.specialBuff` — so the body returned at its first line, green, with nothing
    // graded, and no arm anywhere said so. A floor instead: every fork carries carriers (9 on
    // each, measured), and a discovery function that finds none is broken, not a corpus gap.
    expect(
      candidates.length,
      `${server}: no powerset carries a +Strength power — \`grantsStrength\` found no carrier on a ` +
        `fork that has them, so the discovery is reading something that is gone (it read the ` +
        `stripped \`effects.specialBuff\` until 2026-09-07) and this body grades nothing`,
    ).toBeGreaterThan(0);

    // (1) Some candidate must produce a Strength fraction in a family the fold reads, or the
    // rest of this measures nothing. Most carriers grant ToHit / endurance / movement Strength,
    // which no display row reads a global from.
    const foldable = (g: ReturnType<typeof mapGlobal>) => g.strengthDefense + g.strengthHeal + g.strengthMez;
    const chosen = candidates
      .map((c) => ({ ...c, totals: engineTotals(server, c.build) }))
      .find((c) => foldable(mapGlobal(c.totals.bonuses, c.totals.stats)) > 0);
    expect(chosen, `${server}: none of ${candidates.length} +Strength candidate builds produced a foldable defense/heal/mez fraction`).toBeDefined();
    if (!chosen) return;

    const { build, atId, carriers, totals } = chosen;
    const powers = [...build.primary.powers, ...build.secondary.powers];
    const projection = mapPowerProjection(totals.power_projection);
    const rawGlobal = mapGlobal(totals.bonuses, totals.stats);

    // (2) …and some displayed row must move when it is folded in, or parity is blind to it.
    const plain = convertGlobalBonusesToAspects(rawGlobal);
    const augmented = withStrengthBonuses(plain, rawGlobal);
    let movedRows = 0;
    for (const power of powers) {
      const enh = calculatePowerEnhancementBonuses(
        { name: power.name, slots: power.slots, allowedEnhancements: power.allowedEnhancements },
        build.level,
        getIOSet,
        undefined,
      );
      const without = betaMagnitudes(power, build, atId, enh, plain);
      const with_ = betaMagnitudes(power, build, atId, enh, augmented);
      for (const [key, row] of with_) {
        const other = without.get(key);
        if (other && Math.abs(row.tiers.final - other.tiers.final) > TOLERANCE) movedRows += 1;
      }
    }
    expect(movedRows, `${server}: the +Strength fold changed no displayed row`).toBeGreaterThan(0);

    // …and the fold must reach the ENGINE's magnitude rows, which is the half the resolver can no
    // longer speak for: this body witnesses 0 of its 9 carriers' powers. Same fixture with the
    // carriers switched off, so the only difference between the two runs is the Strength they
    // grant. The fold lands in the FINAL tier, past enhancement, so that is the tier compared.
    const deactivate = (placed: Build['primary']) => ({
      ...placed,
      powers: (placed?.powers ?? []).map((p) => (carriers.includes(p.internalName) ? { ...p, isActive: false } : p)),
    });
    const unfolded = { ...build, primary: deactivate(build.primary), secondary: deactivate(build.secondary), activeModes: [] };
    const plainProjection = mapPowerProjection(engineTotals(server, unfolded).power_projection);
    let engineFolded = 0;
    for (const power of powers) {
      const key = projectionKey(power.powerSet, power.internalName);
      const on = projection.get(key);
      const off = plainProjection.get(key);
      if (on && off && magnitudeRowsMoved(on, off, 'final')) engineFolded += 1;
    }
    expect(
      engineFolded,
      `${server}: switching the +Strength carriers off moved no ENGINE magnitude row — the fold ` +
        `is not reaching them, and the resolver comparison below grades 0 of this fixture's powers`,
    ).toBeGreaterThan(0);

    // (3) …and on that fixture the engine still matches the beta row for row.
    const deltas: string[] = [];
    const adjudicated: string[] = [];
    const engineBundle = bundlePowers(server);
    for (const power of powers) {
      const engine = projection.get(projectionKey(power.powerSet, power.internalName));
      if (!engine) continue;
      const { magnitudes, bag } = betaReference(power, build, atId, rawGlobal);
      // The engine's OWN copy (6C-3h evidence, extended by the 2026-08-19 re-cut). This sweep's
      // build is discovered by data, and on Homecoming it lands on a set whose legacy copy of
      // one power is a DIFFERENT record than the export's (its recharge/accuracy/mez all
      // disagree at base) — every one of those rows is authored drift, not a fold defect.
      const enginePower = engineBundle.get(projectionKey(power.powerSet, power.internalName));
      const mags = countWitness(witness, magnitudeDeltas(
        `${atId}/${power.internalName}`,
        engine.grantedMagnitudes,
        magnitudes,
        bag,
        enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, build, atId, rawGlobal)) : undefined,
      ));
      deltas.push(...mags.real);
      adjudicated.push(...mags.adjudicated.map((d) => `${atId}/${power.internalName}.${d}`));
    }

    // eslint-disable-next-line no-console
    console.warn(`[PROD6C strength] ${server}: ${atId} — strength def/heal/mez ${rawGlobal.strengthDefense}/${rawGlobal.strengthHeal}/${rawGlobal.strengthMez} from ${carriers.length} carrier(s), ${movedRows} rows move under the fold`);
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C strength] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C strength] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 40).join('\n    ')}`);
    }
    // No floor on the witnessed population here: this fixture's carriers are clicks and their
    // powersets' bags are empty, so it is zero on every fork. The verdict for the magnitude half
    // is `engineFolded` above; this states the skip rather than leaving it silent.
    // eslint-disable-next-line no-console
    console.warn(witnessLine(server, '+Strength self-buffs reach the granted magnitudes', witness));
    expect(deltas).toEqual([]);
  }, 60000);

  // PROD6C-3b — the targets-hit slider, which every fixture in this file left at zero. The
  // display bag a surface renders is the built bag AFTER the power's own slider is applied to it
  // (`withTargetsHit`): an effect carrying `perTarget` grows with the foes hit, and one the power
  // lists in `stacksLinear` is multiplied by the capped stack count. The engine had the transform
  // (`stacking.rs`, ported for the TOTALS in PROD6B-2d) but `granted.rs` never called it, so every
  // magnitude row it emitted was a one-target row — and no gate could see that, because
  // `targetsHitValues: {}` is the identity on both sides.
  //
  // Same two-part shape as the level and Alpha sweeps: parity alone would stay green if the
  // transform were dropped from BOTH sides at once, so each side must also be shown to MOVE.
  // A fork whose corpus reaches the transform on no power is reported and skipped rather than
  // passing vacuously — the reach is measured with the beta's own bag builder, not asserted.
  it.each(SERVERS)('%s: granted magnitudes scale with the targets-hit slider', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let sliders = 0;
    let perFoeSliders = 0;
    let stackSliders = 0;
    let bagsChanged = 0;
    let betaMoved = 0;
    let engineMoved = 0;

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      const build = buildFor(server, atId);
      const powers = [...build.primary.powers, ...build.secondary.powers];
      // Data-discovered (Rule 0 — no power is named here): the powers this fork gives a slider,
      // each dragged to the maximum its OWN data declares.
      const sliderMax = new Map<string, number>();
      for (const power of powers) {
        const info = getStackingInfo(power);
        if (!info) continue;
        sliderMax.set(power.internalName, info.maxStacks);
        // Counted per ARM. The stack arm is the dominant subpopulation by an order of magnitude,
        // so a per-target arm that went blind again would leave `sliders` barely moved and every
        // floor below still green — the shape that hid STACKINFO-1 in the first place.
        if (info.label === 'Targets Hit') perFoeSliders += 1; else stackSliders += 1;
      }
      if (sliderMax.size === 0) continue;
      sliders += sliderMax.size;

      // The slider state is keyed by internalName, exactly as the store keys it — including the
      // collisions that keying carries (`characterStateAdapter`), so the engine sees what the
      // surfaces see.
      const ctx = ctxWith({ targetsHitValues: Object.fromEntries(sliderMax) });
      const engineBundle = bundlePowers(server);
      const totals = engineTotals(server, build, ctx);
      const projection = mapPowerProjection(totals.power_projection);
      const unslid = mapPowerProjection(engineTotals(server, build).power_projection);
      // A slider dragged back to Off is an EXPLICIT zero, and on the display that is the
      // identity — not the totals path's "no foes were hit, so the buff does not fire"
      // (PROD6B-2d). No other fixture here sets one: they all leave the map empty, which both
      // sides read as absent, so the two readings of zero looked the same.
      const zeroedTotals = engineTotals(server, build, ctxWith({
        targetsHitValues: Object.fromEntries([...sliderMax.keys()].map((name) => [name, 0])),
      }));
      const zeroed = mapPowerProjection(zeroedTotals.power_projection);
      const rawGlobal = mapGlobal(totals.bonuses, totals.stats);
      // The accumulator sees the slider too, so a stacked self-buff moves the build-wide bonus
      // every row's "final" tier reads. Each comparison below takes the globals from its OWN run.
      const rawGlobalZero = mapGlobal(zeroedTotals.bonuses, zeroedTotals.stats);

      for (const power of powers) {
        const targetsHit = sliderMax.get(power.internalName);
        if (targetsHit === undefined) continue;
        const key = projectionKey(power.powerSet, power.internalName);
        const engine = projection.get(key);
        if (!engine) continue;

        const plainBag = buildDisplayEffects(power);
        const slidBag = withTargetsHit(power, plainBag, targetsHit);
        if (slidBag !== plainBag) bagsChanged += 1;

        const { magnitudes, bag: slidEvidence } = betaReference(power, build, atId, rawGlobal, { targetsHit });
        const { magnitudes: before } = betaReference(power, build, atId, rawGlobal);
        for (const [rowKey, row] of magnitudes) {
          const other = before.get(rowKey);
          if (other && Math.abs(row.tiers.base - other.tiers.base) > TOLERANCE) betaMoved += 1;
        }
        const engineBefore = new Map((unslid.get(key)?.grantedMagnitudes ?? []).map((r) => [r.rowKey, r]));
        for (const row of engine.grantedMagnitudes) {
          const other = engineBefore.get(row.rowKey);
          if (other && Math.abs(row.value.base - other.value.base) > TOLERANCE) engineMoved += 1;
        }

        // The engine's OWN copy of this power, dragged to the same setting: the authored values
        // both sides read, which is what separates a converter-value difference from a calc bug.
        const enginePower = engineBundle.get(key);
        const mags = countWitness(witness, magnitudeDeltas(
          `${atId}/${power.internalName}@${targetsHit}`,
          engine.grantedMagnitudes,
          magnitudes,
          slidEvidence,
          enginePower
            ? displayBag(enginePower as unknown as Power, targetsHit, shownPower(enginePower as unknown as Power, build, atId, rawGlobal))
            : undefined,
        ));
        deltas.push(...mags.real);
        adjudicated.push(...mags.adjudicated.map((d) => `${atId}/${power.internalName}.${d}`));

        const atZero = countWitness(witness, magnitudeDeltas(
          `${atId}/${power.internalName}@0`,
          zeroed.get(key)?.grantedMagnitudes ?? [],
          betaReference(power, build, atId, rawGlobalZero).magnitudes,
          displayBag(power, 0, shownPower(power, build, atId, rawGlobal)),
          enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, build, atId, rawGlobal)) : undefined,
        ));
        deltas.push(...atZero.real);
        adjudicated.push(...atZero.adjudicated.map((d) => `${atId}/${power.internalName}@0.${d}`));
      }
    }

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3b targets-hit] ${server}: ${sliders} powers carry a stacking slider ` +
        `(${perFoeSliders} per-foe, ${stackSliders} stack-count), ` +
        `${bagsChanged} display bags change under it, ${betaMoved} beta / ${engineMoved} engine rows move`,
    );
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3b targets-hit] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.slice(0, 20).join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C-3b targets-hit] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 40).join('\n    ')}`);
    }
    // This used to warn and RETURN here, on the reading that a fork whose corpus reaches the
    // transform on no power should be reported and skipped rather than pass vacuously. That
    // reading is dead: `sliders` is 0 on all four forks, because `getStackingInfo` opens with
    // `if (!power.effects) return null` and STRIP-1 emptied `effects` on every power. The skip
    // condition and the thing it was skipping have the same cause, so the body returned green
    // having graded nothing — and it never went red, which is why it outlasted the 36 that did.
    //
    // Failing loudly instead (Rule 1). This is not a corpus gap to report; it is the discovery
    // function reading a bag that is gone, and the same starvation has taken the InfoPanel's
    // targets-hit slider off every power on every fork.
    expect(
      sliders,
      `${server}: not one power carries a stacking slider — \`getStackingInfo\` reads \`power.effects\`, ` +
        `which STRIP-1 emptied, so it returns null for the whole corpus. The engine's atom-native ` +
        `replacement is in coh_math's stacking.rs; the TS side has not been re-pointed at it.`,
    ).toBeGreaterThan(0);
    expect(
      perFoeSliders,
      `${server}: not one slider is a per-foe one — every carrier found came through the stack ` +
        `arm, so the AoE arm is reading something that is gone (it read \`effects[…].perTarget\` ` +
        `until STACKINFO-1) and the powers that grow per foe without self-stacking are ungraded`,
    ).toBeGreaterThan(0);
    expect(
      stackSliders,
      `${server}: not one slider is a stack-count one — the linear arm found no carrier`,
    ).toBeGreaterThan(0);
    // `bagsChanged` and `betaMoved` are REPORTED, not floored, and the reason is the same one
    // that put the third bucket on `magnitudeDeltas`. Both grade the beta's own bag transform
    // (`withTargetsHit` -> `adjustEffectsForTargets`), whose two inputs are a `perTarget` on a bag
    // VALUE and the power's `stacksLinear` list — both authored, both emptied by STRIP-1. The beta
    // never grew an atom seed for its display bag: ENGLAG-1 re-pointed the rendered rows at the
    // engine projection instead, and all three `RegistryEffectsDisplay` call sites now pass
    // `rows`, so the bag reaches the surface for its `durations` / `buffDuration` annotations
    // alone. A floor here would demand movement from a path the swap retired.
    //
    // The live verdict is the engine's, below, because the slider's whole effect on what a user
    // sees now runs `targetsHitValues` -> `characterStateAdapter` -> `granted_magnitudes` ->
    // `effectRows`. Stating the skip rather than deleting the counters keeps the day the beta bag
    // starts moving again visible.
    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3b targets-hit] ${server}: the beta bag transform is unwitnessed — ${bagsChanged} ` +
        `bags change and ${betaMoved} beta rows move, with no authored \`perTarget\` or ` +
        `\`stacksLinear\` left in the corpus to drive either`,
    );
    expect(engineMoved, `${server}: the slider moved no engine row`).toBeGreaterThan(0);
    // Anti-vacuity for the magnitude half. STRIP-1 left most of the corpus unable to answer, and
    // an arm that grades zero powers is green for the wrong reason — so the population the
    // resolver CAN still witness is floored rather than assumed.
    expect(
      witness.witnessed,
      witnessLine(server, 'granted magnitudes scale with the targets-hit slider', witness),
    ).toBeGreaterThan(0);
    expect(deltas).toEqual([]);
  }, 180000);

  // PROD6C-3k — the EFFECTIVE power. Every fixture above runs the all-off `CTX`, where the only
  // transform that fires is a conditional whose own `defaultActive` is true (12 / 9 / 11 powers per
  // fork). This one drives the states that decide WHICH power the surfaces show: combat mode (the
  // snipe's fast form), the from-Hide toggle (a slow opener vs its mid-combat cast), and every
  // conditional-toggle id the build's own powers publish.
  //
  // The toggle ids are data-discovered (Rule 0 — no fixture may name one; they come from the
  // binary's `requiresExpression` gates at convert time), and the same map reaches both sides: the
  // adapter forwards it to the engine, `betaReference` resolves the beta power through it.
  it.each(SERVERS)('%s: the shown power drives the projection under every combat state', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let quickForms = 0;
    // Of `quickForms`, how many the `combatMode` toggle alone can reach (SNIPE-2) — the rest
    // gate on the caster's own ToHit, a state this fixture never buffs.
    let toggleReachableQuickForms = 0;
    let openers = 0;
    let mergedPowers = 0;
    let mergedRows = 0;
    let castMoved = 0;
    let hiddenMoved = 0;
    const refused: string[] = [];
    // The keys the beta answered from an additive conditional's fill-in where the engine answered
    // from the power's own base atoms, with both values — graded against
    // [`BASE_ROW_OVER_ADDITIVE_FILL`].
    const additiveFilled: Record<string, { base: number; filled: number }> = {};

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      // Build from the sets that CAN grade this: the 6B fixture's first-set pick reaches almost no
      // quick form or conditional (measured — 0 on rebirth), because the mechanics that carry them
      // are spread across an archetype's other sets. Discovered by counting each set's own carriers,
      // so no powerset is named (Rule 0).
      const build = buildFor(server, atId, 50, true, (sets) => {
        const carried = sets.map((set) => set.powers.filter(carriesTransform).length);
        return carried.indexOf(Math.max(...carried));
      });
      const powers = [...build.primary.powers, ...build.secondary.powers];
      if (powers.length === 0) continue;

      // Every conditional this build's powers carry, turned ON, by the scope the DATA declares.
      const globalAdjusters: Record<string, boolean> = {};
      const mechanicAdjusters: Record<string, boolean> = {};
      for (const power of powers) {
        for (const conditional of power.conditionalEffects ?? []) {
          if (conditional.scope === 'global') globalAdjusters[conditional.id] = true;
          else mechanicAdjusters[`${power.internalName}:${conditional.id}`] = true;
        }
        if ((power as unknown as { quickSnipe?: unknown }).quickSnipe != null) {
          quickForms += 1;
          if (!quickSnipeReadsCasterToHit(power)) toggleReachableQuickForms += 1;
        }
        if (power.midCombatCast != null) openers += 1;
      }

      // In combat with every toggle on, and NOT hidden — so the snipe fires its fast form and an
      // opener its mid-combat cast. `hidden` is the same build seen from cover, `CTX` the all-off
      // state every other fixture here runs: three states, one build, so a value that moves can be
      // attributed to the state that moved it.
      const engaged = ctxWith({ combatMode: true, dominationActive: true, globalAdjusters, mechanicAdjusters });
      const hidden = ctxWith({ ...engaged, stalkerHidden: true });
      // The adapter's PROD3 guard refuses a toggle whose conditional carries a caster dashboard
      // buff the engine's TOTALS do not model, and driving every id is how an unclassified one
      // surfaces. Recorded rather than caught silently, and the reach counters below still have to
      // be non-zero, so a fork cannot pass this test by refusing every state.
      let runs;
      try {
        runs = [engaged, hidden].map((ctx) => {
          const totals = engineTotals(server, build, ctx);
          return { ctx, projection: mapPowerProjection(totals.power_projection), rawGlobal: mapGlobal(totals.bonuses, totals.stats) };
        });
      } catch (err) {
        refused.push(`${atId}: ${String(err).split(' — ')[0]}`);
        continue;
      }
      const plainTotals = engineTotals(server, build);
      const plain = mapPowerProjection(plainTotals.power_projection);
      const plainRawGlobal = mapGlobal(plainTotals.bonuses, plainTotals.stats);

      for (const { ctx, projection, rawGlobal } of runs) {
        const tag = ctx === hidden ? 'hidden' : 'engaged';
        for (const power of powers) {
          const key = projectionKey(power.powerSet, power.internalName);
          const engine = projection.get(key);
          if (!engine) continue;
          const { projection: beta, magnitudes, bag } = betaReference(power, build, atId, rawGlobal, { ctx });
          // The same power with every toggle off — the base surface the beta can still see, and
          // the input [`additiveFillIns`] reads for the collision its own merge could not run.
          const offBag = displayBag(power, 0, shownPower(power, build, atId, plainRawGlobal));
          const fillIns = additiveFillIns(power, offBag, bag);
          // The engine's OWN copy under the SAME toggle state (6C-3h evidence, extended by the
          // 2026-08-19 re-cut) — resolved through the same effective-power transform, so a
          // conditional-merged row still compares authored-to-authored.
          const enginePower = bundlePowers(server).get(key);
          const mags = countWitness(witness, magnitudeDeltas(
            `${atId}/${power.internalName}@${tag}`,
            engine.grantedMagnitudes,
            magnitudes,
            bag,
            enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, build, atId, rawGlobal, ctx)) : undefined,
            // No `ownPower`: the pet-underlay adjudication is graded in its own body, and
            // enabling it here would fold two verdicts into one roster.
            undefined,
            fillIns,
          ));
          for (const row of mags.additiveFilled) {
            additiveFilled[`${atId}/${power.internalName}.${row.key}`] = {
              base: Number(row.base.toFixed(2)),
              filled: Number(row.filled.toFixed(2)),
            };
          }
          deltas.push(...mags.real);
          adjudicated.push(...mags.adjudicated.map((d) => `${atId}/${power.internalName}@${tag}.${d}`));
          deltas.push(
            ...tierDelta(`${power.internalName}@${tag}.recharge`, engine.recharge, beta.recharge),
            ...tierDelta(`${power.internalName}@${tag}.enduranceCost`, engine.enduranceCost, beta.enduranceCost),
            ...tierDelta(`${power.internalName}@${tag}.accuracy`, engine.accuracy, beta.accuracy),
            ...tierDelta(`${power.internalName}@${tag}.castTime`, engine.castTime, beta.castTime),
            ...tierDelta(`${power.internalName}@${tag}.range`, engine.range, beta.range),
          );
          if (engine.arcanaTime !== null && beta.arcanaTime !== null && Math.abs(engine.arcanaTime - beta.arcanaTime) > TOLERANCE) {
            deltas.push(`${power.internalName}@${tag}.arcanaTime: engine ${engine.arcanaTime} vs beta ${beta.arcanaTime}`);
          }

          // Reach, measured rather than assumed (the PROD6C-3f method): what each state actually
          // MOVED. A transform that changes nothing cannot be graded by the parity above, however
          // green it goes — so these counters, not the deltas, are what makes this test non-vacuous.
          if (tag === 'engaged') {
            const before = plain.get(key)?.castTime;
            if (before && engine.castTime && Math.abs(before.base - engine.castTime.base) > TOLERANCE) castMoved += 1;
            const changed = [...new Set([...Object.keys(offBag), ...Object.keys(bag)])].filter(
              (bagKey) => JSON.stringify(offBag[bagKey]) !== JSON.stringify(bag[bagKey]),
            );
            if (changed.length > 0) {
              mergedPowers += 1;
              mergedRows += changed.length;
            }
          } else {
            const engagedCast = runs[0].projection.get(key)?.castTime;
            if (engagedCast && engine.castTime && Math.abs(engagedCast.base - engine.castTime.base) > TOLERANCE) hiddenMoved += 1;
          }
        }
      }
    }

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3k effective] ${server}: ${quickForms} quick forms (${toggleReachableQuickForms} combat-mode-reachable), ` +
        `${openers} from-Hide openers — ${castMoved} casts move in combat, ${hiddenMoved} move again from cover, ` +
        `${mergedRows} rows on ${mergedPowers} powers change under the toggles`,
    );
    if (refused.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3k effective] ${server}: ${refused.length} archetype(s) ungraded — the adapter refuses the toggle state:\n    ${refused.join('\n    ')}`);
    }
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3k effective] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.slice(0, 20).join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C-3k effective] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 40).join('\n    ')}`);
    }
    // Anti-vacuity, one per transform, each asserted only where the fork's corpus carries the data
    // and reported when it does not — the PROD6C-3b shape. The conditional merge is the one every
    // fork carries, so it is unconditional: without it this whole sweep would be the CTX sweep again.
    expect(mergedRows, `${server}: no row changes under any toggle — the merge is ungraded here`).toBeGreaterThan(0);
    if (toggleReachableQuickForms > 0) {
      expect(
        castMoved,
        `${server}: ${toggleReachableQuickForms} combat-mode-reachable quick forms in the corpus and not one cast moved in combat`,
      ).toBeGreaterThan(0);
    } else if (quickForms > 0) {
      // Every quick form here gates on the caster's own ToHit (SNIPE-2 — Rebirth/Thunderspy),
      // not on `combatMode`. This fixture drives no ToHit buff, so 0 is the CORRECT reading:
      // asserting >0 would demand the toggle move a state it structurally cannot.
      // eslint-disable-next-line no-console
      console.warn(
        `[PROD6C-3k effective] ${server}: ${quickForms} quick forms, all ToHit-gated (SNIPE-2) — this fixture cannot grade the snipe swap without a ToHit buff`,
      );
    } else {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3k effective] ${server}: no quick form in the corpus — this fork cannot grade the snipe swap`);
    }
    if (hiddenMoved === 0) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3k effective] ${server}: no cast moved from cover — ${openers} openers reached, none whose mid-combat cast differs from its from-Hide one`);
    }
    // Anti-vacuity for the magnitude half. STRIP-1 left most of the corpus unable to answer, and
    // an arm that grades zero powers is green for the wrong reason — so the population the
    // resolver CAN still witness is floored rather than assumed.
    expect(
      witness.witnessed,
      witnessLine(server, 'the shown power drives the projection under every combat state', witness),
    ).toBeGreaterThan(0);
    // The base-row-over-fill-in keys, by value: see [`BASE_ROW_OVER_ADDITIVE_FILL`].
    expect(additiveFilled, `${server}: base row vs additive fill-in`).toEqual(BASE_ROW_OVER_ADDITIVE_FILL[server] ?? {});
    expect(deltas).toEqual([]);
  }, 300000);

  // PROD6C-3l. The fourth effective-power transform: while a caster mode is live the game's
  // PowerRedirector fires a DIFFERENT record, so every projected value describes that record and
  // not the base one. Every fixture above runs with no mode live, which is exactly the state the
  // redirect agrees with the base power in — so no gate here could see it.
  //
  // The modes come from the build's own powers (`modeVariants` keys, converted from the binary's
  // Redirect table), so the fixture discovers which archetypes have them rather than naming any.
  it.each(SERVERS)('%s: a live caster mode redirects the projected power', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let modedPowers = 0;
    let movedPowers = 0;
    let movedMagPowers = 0;
    let movedRows = 0;

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      // Build from the sets that CARRY a mode redirect — the 6B first-set pick reaches none on
      // two of the three forks (measured), so a fixture that did not choose by the data would
      // grade nothing and still pass.
      const base = buildFor(server, atId, 50, true, (sets) => {
        const carried = sets.map((set) => set.powers.filter(carriesModeVariant).length);
        return carried.indexOf(Math.max(...carried));
      });
      const powers = [...base.primary.powers, ...base.secondary.powers];
      const modes = selectableModes(powers as unknown as { modeVariants?: Record<string, unknown> }[]);
      if (modes.length === 0) continue;

      const plainTotals = engineTotals(server, base);
      const plain = mapPowerProjection(plainTotals.power_projection);
      const plainRawGlobal = mapGlobal(plainTotals.bonuses, plainTotals.stats);

      for (const mode of modes) {
        // One mode at a time: the Header selector is single-select, and a variant is chosen by
        // the FIRST live mode its table names, so grading them together would hide the rest.
        const build = { ...base, activeModes: [mode] };
        const totals = engineTotals(server, build);
        const projection = mapPowerProjection(totals.power_projection);
        const rawGlobal = mapGlobal(totals.bonuses, totals.stats);

        for (const power of powers) {
          if (!carriesModeVariant(power as Power)) continue;
          const key = projectionKey(power.powerSet, power.internalName);
          const engine = projection.get(key);
          if (!engine) continue;
          modedPowers += 1;

          const { projection: beta, magnitudes, bag } = betaReference(power, build, atId, rawGlobal);
          const mags = countWitness(witness, magnitudeDeltas(`${atId}/${power.internalName}@${mode}`, engine.grantedMagnitudes, magnitudes, bag));
          deltas.push(...mags.real);
          adjudicated.push(...mags.adjudicated.map((d) => `${atId}/${power.internalName}@${mode}.${d}`));
          deltas.push(
            ...tierDelta(`${power.internalName}@${mode}.recharge`, engine.recharge, beta.recharge),
            ...tierDelta(`${power.internalName}@${mode}.enduranceCost`, engine.enduranceCost, beta.enduranceCost),
            ...tierDelta(`${power.internalName}@${mode}.accuracy`, engine.accuracy, beta.accuracy),
            ...tierDelta(`${power.internalName}@${mode}.castTime`, engine.castTime, beta.castTime),
            ...tierDelta(`${power.internalName}@${mode}.range`, engine.range, beta.range),
          );

          // Reach: what the mode actually MOVED against the no-mode run. A redirect that changes
          // nothing is ungraded however green the parity goes (the PROD6C-3f method).
          //
          // Measured on the DISPLAY BAG, which STRIP-1 emptied — so what survives here is the
          // minted keys, and a redirect that changed only authored effects now moves nothing.
          // Kept because a redirect that swaps the execution stats still shows up in it, and
          // widened below with the engine's own rows, which are the half the bag can no longer
          // speak for.
          const offBag = displayBag(power, 0, shownPower(power, base, atId, plainRawGlobal));
          const changed = [...new Set([...Object.keys(offBag), ...Object.keys(bag)])].filter(
            (bagKey) => JSON.stringify(offBag[bagKey]) !== JSON.stringify(bag[bagKey]),
          );
          const beforeCast = plain.get(key)?.castTime;
          const castMoved = !!beforeCast && !!engine.castTime && Math.abs(beforeCast.base - engine.castTime.base) > TOLERANCE;
          const off = plain.get(key);
          if (off && magnitudeRowsMoved(engine, off)) movedMagPowers += 1;
          if (changed.length > 0 || castMoved) {
            movedPowers += 1;
            movedRows += changed.length;
          }
        }
      }
    }

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3l mode redirect] ${server}: ${modedPowers} power/mode pairs projected, ` +
        `${movedRows} rows on ${movedPowers} of them move against the no-mode run`,
    );
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3l mode redirect] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.slice(0, 20).join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C-3l mode redirect] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 40).join('\n    ')}`);
    }
    // Anti-vacuity: the corpus must actually REACH a redirect, and the redirect must actually
    // change something. Asserted per fork because all three carry mode redirects (measured:
    // 28 / 32 / 22 powers), so none of them may quietly grade nothing.
    expect(modedPowers, `${server}: no power/mode pair reached — the fixture grades no redirect`).toBeGreaterThan(0);
    expect(movedPowers, `${server}: ${modedPowers} pairs reached and not one value moved`).toBeGreaterThan(0);
    // …and the redirect reached the granted MAGNITUDES, read off the engine rather than off the
    // stripped bag above. This body witnesses 0 of its powers on every fork, so the resolver
    // comparison beside it grades none of them and this is the whole verdict for that half.
    expect(
      movedMagPowers,
      `${server}: ${modedPowers} pairs reached and the mode moved no engine MAGNITUDE row — the ` +
        `bag-keyed reach above cannot see this, because STRIP-1 left the bag holding minted keys`,
    ).toBeGreaterThan(0);
    // No floor on the witnessed population here: it is zero on every fork, and pinning it at zero
    // would assert the strip stays. The verdict for this body's magnitude half is the engine-side
    // floor above; this states the skip rather than leaving it silent.
    // eslint-disable-next-line no-console
    console.warn(witnessLine(server, 'a live caster mode redirects the projected power', witness));
    expect(deltas).toEqual([]);
  }, 300000);

  // TSPY-4. The TOTALS half of the mode story 6C-3l covered on the display side. A power's
  // `setsModes` is the ONLY thing that binds a `Source.Mode?` gate for the totals pass, so a fork
  // whose parser resolved those templates to a placeholder published no mode, every mode-gated
  // atom stayed dark, and nothing here could see it — the beta drove the same conditionals from
  // default-off toggles, so both sides agreed on the wrong answer.
  //
  // Deactivating the mode-SETTING power is not on its own evidence: that also removes the setter's
  // own ungated effects. So this measures the CONSUMERS' contribution twice — once with the mode
  // live, once with it dark — and holds that the two differ. Only a bound gate can do that.
  it.each(SERVERS)('%s: a mode-setting toggle binds its Source.Mode? gates in the totals', async (server) => {
    await loadDataset(server);

    let carriers = 0;
    let graded = 0;
    const bound: string[] = [];

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      const base = buildFor(server, atId, 50, false);
      const powers = [...base.primary.powers, ...base.secondary.powers];

      // The modes this build both PUBLISHES (some power's `setsModes`) and CONSUMES (some OTHER
      // power's atom gated on it). Discovered from the powers' own fields — a published mode
      // nothing reads would grade nothing, and a consumed mode nothing publishes cannot bind.
      const modesOf = (p: SelectedPower) => (p as unknown as { setsModes?: string[] }).setsModes ?? [];
      const gatesOf = (p: SelectedPower) => {
        const found = new Set<string>();
        for (const [, token] of JSON.stringify(p).matchAll(/"(\w+) [Ss]ource\.Mode\?/g)) {
          found.add(token);
          found.add(token.replace(/^k/, ''));
        }
        return found;
      };
      const setters = powers.filter((p) => p.isActive && modesOf(p).length > 0);
      const live = new Set(setters.flatMap(modesOf));
      const consumers = powers.filter((p) => !setters.includes(p) && [...gatesOf(p)].some((m) => live.has(m)));
      if (setters.length === 0 || consumers.length === 0) continue;
      carriers += 1;

      const without = (drop: SelectedPower[]) => {
        const keep = (list: SelectedPower[]) => list.filter((p) => !drop.includes(p));
        return { ...base, primary: { ...base.primary, powers: keep(base.primary.powers) }, secondary: { ...base.secondary, powers: keep(base.secondary.powers) } };
      };
      const deactivated = (build: Build) => {
        const off = (list: SelectedPower[]) => list.map((p) => (setters.includes(p) ? { ...p, isActive: false } : p));
        return { ...build, primary: { ...build.primary, powers: off(build.primary.powers) }, secondary: { ...build.secondary, powers: off(build.secondary.powers) } };
      };
      const globals = (build: Build) => {
        const totals = engineTotals(server, build);
        return mapGlobal(totals.bonuses, totals.stats) as unknown as Record<string, number>;
      };

      const withConsumersLive = globals(base);
      const withoutConsumersLive = globals(without(consumers));
      const withConsumersDark = globals(deactivated(base));
      const withoutConsumersDark = globals(deactivated(without(consumers)));

      const moved = Object.keys(withConsumersLive).filter((key) => {
        const contributionLive = (withConsumersLive[key] ?? 0) - (withoutConsumersLive[key] ?? 0);
        const contributionDark = (withConsumersDark[key] ?? 0) - (withoutConsumersDark[key] ?? 0);
        return Math.abs(contributionLive - contributionDark) > TOLERANCE;
      });
      if (moved.length > 0) {
        graded += 1;
        bound.push(`${atId}: ${[...live].join('/')} moves ${moved.slice(0, 6).join(', ')}`);
      }
    }

    if (carriers === 0) {
      // eslint-disable-next-line no-console
      console.warn(`[TSPY-4 mode totals] ${server}: no build both publishes and consumes a mode — this fork cannot grade the binding`);
      return;
    }
    // eslint-disable-next-line no-console
    console.warn(`[TSPY-4 mode totals] ${server}: ${graded}/${carriers} archetype(s) bind a published mode\n    ${bound.join('\n    ')}`);
    expect(graded, `${server}: ${carriers} archetype(s) publish a mode their own powers gate on, and not one gate bound — the parser resolved those Set_Mode templates to a placeholder`).toBeGreaterThan(0);
  }, 300000);

  // PROD6B-2c. Every fixture above builds at 50, which is precisely the level the retired pin
  // agreed with — so a magnitude resolved at a fixed 50 and one resolved at the build level were
  // indistinguishable to every gate in this file. That is how the pin survived 6B-2. This runs the
  // same magnitude diff at a level where the two disagree, and holds the property that made the
  // pin visible: ~70% of the AT tables vary by level, so a correct resolver's numbers MOVE when
  // the build level does.
  //
  // Between them the two assertions close both re-pin directions. A pin re-introduced on ONE side
  // shows up as a level-25 parity delta; a pin re-introduced on BOTH at once still resolves in
  // lockstep, so the moved-row counts catch that instead.
  it.each(SERVERS)('%s: granted magnitudes resolve at the build level, not a pinned one', async (server) => {
    await loadDataset(server);
    const witness = witnessCounter();

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let engineMoved = 0;
    let betaMoved = 0;
    let rows = 0;

    for (const atId of STANDARD_ARCHETYPE_IDS) {
      const atLevel = (level: number) => {
        const build = buildFor(server, atId, level, false);
        const powers = [...build.primary.powers, ...build.secondary.powers];
        const totals = engineTotals(server, build);
        const projection = mapPowerProjection(totals.power_projection);
        const rawGlobal = mapGlobal(totals.bonuses, totals.stats);
        return { build, powers, projection, rawGlobal };
      };

      const low = atLevel(SWEEP_LEVEL);
      const high = atLevel(50);
      if (low.powers.length === 0) continue;

      for (const power of low.powers) {
        const key = projectionKey(power.powerSet, power.internalName);
        const engineLow = low.projection.get(key);
        const engineHigh = high.projection.get(key);
        if (!engineLow) continue;
        const low_ = betaReference(power, low.build, atId, low.rawGlobal);
        const betaLow = low_.magnitudes;
        const betaHigh = betaReference(power, high.build, atId, high.rawGlobal).magnitudes;

        // The engine's OWN copy (6C-3h evidence, extended by the 2026-08-19 re-cut) — the
        // dataset differences the level-50 sweep adjudicates are the same records here.
        const enginePower = bundlePowers(server).get(key);
        const mags = countWitness(witness, magnitudeDeltas(
          `${atId}/${power.internalName}@${SWEEP_LEVEL}`,
          engineLow.grantedMagnitudes,
          betaLow,
          low_.bag,
          enginePower ? displayBag(enginePower as unknown as Power, 0, shownPower(enginePower as unknown as Power, low.build, atId, low.rawGlobal)) : undefined,
        ));
        deltas.push(...mags.real);
        adjudicated.push(...mags.adjudicated.map((d) => `${atId}/${power.internalName}.${d}`));
        rows += engineLow.grantedMagnitudes.length;

        const engineHighByKey = new Map((engineHigh?.grantedMagnitudes ?? []).map((r) => [r.rowKey, r]));
        for (const row of engineLow.grantedMagnitudes) {
          const other = engineHighByKey.get(row.rowKey);
          if (other && Math.abs(other.value.base - row.value.base) > TOLERANCE) engineMoved++;
        }
        for (const [rowKey, row] of betaLow) {
          const other = betaHigh.get(rowKey);
          if (other && Math.abs(other.tiers.base - row.tiers.base) > TOLERANCE) betaMoved++;
        }
      }
    }

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6B-2c] ${server}: ${rows} rows at level ${SWEEP_LEVEL}, ` +
        `${engineMoved} engine / ${betaMoved} beta rows moved vs level 50`,
    );
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6B-2c] ${server} engine-supersedes-beta (accepted, ${adjudicated.length})`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6B-2c] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 60).join('\n    ')}`);
    }

    expect(engineMoved, `${server}: no engine row changed between level ${SWEEP_LEVEL} and 50 — the level is being ignored`).toBeGreaterThan(0);
    expect(betaMoved, `${server}: no beta row changed between level ${SWEEP_LEVEL} and 50 — the level is being ignored`).toBeGreaterThan(0);
    // Anti-vacuity for the magnitude half. STRIP-1 left most of the corpus unable to answer, and
    // an arm that grades zero powers is green for the wrong reason — so the population the
    // resolver CAN still witness is floored rather than assumed.
    expect(
      witness.witnessed,
      witnessLine(server, 'granted magnitudes resolve at the build level, not a pinned one', witness),
    ).toBeGreaterThan(0);
    expect(deltas).toEqual([]);
  }, 180000);

  // PROD6C-3f — the two inputs 6C-1d recorded as ungraded. Every fixture above runs `CTX`, which
  // has every incarnate slot off and every build at 50, so `combineWithAlphaED`'s ED split and
  // `exemplarLevel`'s IO rescale had never been diffed at all: not one row in this file had ever
  // been resolved with an Alpha equipped. Both feed the same magnitude and execution rows PROD6C-3
  // is about to swap onto the projection, so they are closed BEFORE those rows move, not after.
  //
  // Same two-part shape as the 6B-2c sweep, for the same reason: parity alone would stay green if
  // Alpha were dropped from BOTH sides at once, so each test also proves its input MOVES rows.
  it.each(SERVERS)('%s: an equipped Alpha splits through ED on every projected power', async (server) => {
    await loadDataset(server);
    const fixtures = alphaFixturesFor();
    const archetypeId = STANDARD_ARCHETYPE_IDS[0];
    const ctx = ctxWith({ incarnateActive: { ...CTX.incarnateActive, alpha: true } });
    // Both sides read the bypass per aspect from the export now (PROD6C-3g), so every Alpha the
    // fork offers is a hard must-equal — there is no expressibility split left to select on.
    expect(fixtures.length, `${server}: no Alpha carries an enhancement aspect`).toBeGreaterThan(0);

    // Search for an (Alpha, saturated aspect) pair whose ED split this comparison can actually SEE,
    // proven by re-running the diff against a bypass slice no Alpha authors. The ordinary three-IO
    // fixture can see none — measured, not assumed — so the search is what makes this test grade the
    // split rather than only Alpha's presence.
    const probeFixture = (alpha: AlphaFixture, build: Build) => {
      const powers = [...build.primary.powers, ...build.secondary.powers];
      const totals = engineTotals(server, build, ctx);
      const projection = mapPowerProjection(totals.power_projection);
      const rawGlobal = mapGlobal(totals.bonuses, totals.stats);
      const probe = diffProjection(server, powers, build, archetypeId, projection, rawGlobal, {
        alpha: { ...alpha, bypass: probeBypass(alpha) },
      });
      return { powers, projection, rawGlobal, observable: probe.deltas.length > 0 };
    };

    let alpha = fixtures[0];
    let build = buildFor(server);
    build.incarnates = { ...build.incarnates, alpha: alpha.selection };
    let probed = probeFixture(alpha, build);
    let saturatedOn: string | null = null;
    search: for (const candidate of fixtures) {
      const base = buildFor(server);
      for (const stat of saturationStatsFor([...base.primary.powers, ...base.secondary.powers], candidate)) {
        const saturated = saturatedBuild(server, stat);
        saturated.incarnates = { ...saturated.incarnates, alpha: candidate.selection };
        const attempt = probeFixture(candidate, saturated);
        if (!attempt.observable) continue;
        alpha = candidate;
        build = saturated;
        probed = attempt;
        saturatedOn = stat;
        break search;
      }
    }
    const { powers, projection, rawGlobal } = probed;
    const splitGraded = probed.observable;
    const state: ReferenceState = { alpha };

    // (1) Alpha must actually reach the projected values, or the parity below measures nothing.
    // Measured by running the REAL calculators twice over the same fixture — with and without the
    // equipped Alpha — rather than by reimplementing the split here (the PROD6B-2b method).
    const plainProjection = mapPowerProjection(engineTotals(server, build).power_projection);
    let engineMoved = 0;
    let engineMagsMoved = 0;
    let betaMoved = 0;
    for (const power of powers) {
      const key = projectionKey(power.powerSet, power.internalName);
      const withAlpha = projection.get(key);
      const without = plainProjection.get(key);
      if (withAlpha && without && projectedValuesMoved(withAlpha, without)) engineMoved++;
      if (withAlpha && without && magnitudeRowsMoved(withAlpha, without)) engineMagsMoved++;
      const betaWith = betaEnhancement(power, build, state);
      const betaWithout = betaEnhancement(power, build, {});
      for (const aspect of alpha.aspects) {
        if (Math.abs((betaWith[aspect] ?? 0) - (betaWithout[aspect] ?? 0)) > 1e-9) { betaMoved++; break; }
      }
    }
    expect(engineMoved, `${server}: equipping Alpha changed no engine-projected value`).toBeGreaterThan(0);
    expect(betaMoved, `${server}: equipping Alpha changed no beta enhancement bonus`).toBeGreaterThan(0);
    // The magnitude half of that reach, floored separately. The resolver used to grade Alpha's
    // effect on the granted magnitudes row for row; STRIP-1 left it nothing to grade with (this
    // body witnesses 0 / 0 / 3 / 0 powers), so the claim that Alpha reaches the magnitudes at all
    // rests here. `projectedValuesMoved` above is satisfied by an execution tier alone.
    expect(
      engineMagsMoved,
      `${server}: equipping Alpha moved an execution tier but no granted MAGNITUDE row — with the ` +
        `resolver comparison starved by STRIP-1, nothing else grades Alpha's reach into them`,
    ).toBeGreaterThan(0);

    // (2) …and under that Alpha the engine still matches the beta calculators row for row.
    const { deltas, adjudicated, rows, witness } = diffProjection(server, powers, build, archetypeId, projection, rawGlobal, state);

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3f alpha] ${server}: ${alpha.selection.displayName} (${alpha.selection.tier}) ` +
        `on ${alpha.aspects.join('/')} — ${rows} magnitude rows, ${engineMoved} engine / ${betaMoved} beta powers move; ` +
        `${fixtures.length} of the fork's Alphas graded as hard must-equals; ` +
        `ED split ${splitGraded ? `graded here (saturated on ${saturatedOn ?? 'the base fixture'})` : 'NOT observable on any fixture tried'}`,
    );
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3f alpha] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.slice(0, 20).join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C-3f alpha] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 60).join('\n    ')}`);
    }
    // Unwitnessed magnitude half — see the `engineMagsMoved` floor above, which is this body's
    // verdict for it. 0 / 0 / 3 / 0 powers reach the resolver with an authored bag.
    // eslint-disable-next-line no-console
    console.warn(witnessLine(server, 'an equipped Alpha splits through ED on every projected power', witness));
    expect(deltas).toEqual([]);
  }, 60000);

  // The exemplar half. Two levels, because the rule changes across 45: at 45+ the incarnate stays
  // alive and only the non-attuned IOs rescale, while below 45 `areIncarnatesSuppressed` /
  // `incarnates_suppressed` drop Alpha entirely. A one-level test would grade whichever branch it
  // happened to pick and call the other one covered.
  it.each(SERVERS)('%s: an exemplared build rescales IOs, and below 45 suppresses Alpha', async (server) => {
    await loadDataset(server);
    const alpha = alphaFixturesFor()[0];
    if (!alpha) return;

    const deltas: string[] = [];
    const adjudicated: string[] = [];
    let rows = 0;
    let rescaled = 0;
    let magsRescaled = 0;
    const witness = witnessCounter();
    const suppression: string[] = [];

    // 45 is the incarnate floor itself, so `above` keeps them alive while still rescaling IOs and
    // `below` is the first level that suppresses. Read off the beta's own constant rather than
    // written twice.
    const above = INCARNATE_MIN_LEVEL;
    const below = INCARNATE_MIN_LEVEL - 1;

    const exemplarCap = assertSharedExemplarCap(server, above);

    for (const exemplarLevel of [above, below]) {
      const build = buildFor(server);
      build.incarnates = { ...build.incarnates, alpha: alpha.selection };
      const powers = [...build.primary.powers, ...build.secondary.powers];
      const ctx = ctxWith({
        incarnateActive: { ...CTX.incarnateActive, alpha: true },
        exemplarMode: true,
        exemplarLevel,
      });
      const totals = engineTotals(server, build, ctx);
      const projection = mapPowerProjection(totals.power_projection);
      const rawGlobal = mapGlobal(totals.bonuses, totals.stats);

      // Below the floor the incarnate is gone, so the beta reference drops Alpha too. That the
      // ENGINE also drops it is asserted rather than assumed: its exemplared projection must equal
      // the one it produces with no Alpha equipped at all, or one side is keeping a buff the other
      // suppressed.
      const suppressed = areIncarnatesSuppressed(exemplarLevel);
      const state: ReferenceState = { alpha: suppressed ? undefined : alpha, exemplarLevel };
      if (suppressed) {
        const noAlpha = buildFor(server);
        const withoutAlpha = mapPowerProjection(
          engineTotals(server, noAlpha, ctxWith({ exemplarMode: true, exemplarLevel })).power_projection,
        );
        const kept = powers.filter((power) => {
          const key = projectionKey(power.powerSet, power.internalName);
          const a = projection.get(key);
          const b = withoutAlpha.get(key);
          return a && b && projectedValuesMoved(a, b);
        });
        expect(
          kept.map((p) => p.internalName),
          `${server}: exemplared to ${exemplarLevel} (below ${INCARNATE_MIN_LEVEL}) the engine still applied Alpha`,
        ).toEqual([]);
      }

      // The IO rescale must be observable at this level, or parity here is vacuous.
      for (const power of powers) {
        const scaled = betaEnhancement(power, build, { exemplarLevel });
        const unscaled = betaEnhancement(power, build, {});
        for (const aspect of new Set([...Object.keys(scaled), ...Object.keys(unscaled)])) {
          if (Math.abs((scaled[aspect] ?? 0) - (unscaled[aspect] ?? 0)) > 1e-9) { rescaled++; break; }
        }
      }

      // …and observable in the GRANTED MAGNITUDES, not only in the enhancement bonuses feeding
      // them. `rescaled` counts a beta-side aspect moving; it would stay green with the engine's
      // magnitude rows frozen at their level-50 values. That gap used to be covered by the
      // resolver comparison below, which STRIP-1 starved (this body witnesses 0 / 0 / 2 / 0).
      const atFifty = mapPowerProjection(
        engineTotals(server, build, ctxWith({ incarnateActive: { ...CTX.incarnateActive, alpha: true } })).power_projection,
      );
      for (const power of powers) {
        const key = projectionKey(power.powerSet, power.internalName);
        const ex = projection.get(key);
        const full = atFifty.get(key);
        if (ex && full && magnitudeRowsMoved(ex, full)) magsRescaled += 1;
      }

      // Per power, so a delta names the power it came from. Both sides read the same handicap
      // curves now (PROD6C-3g), so every row is a hard must-equal — the cap-multiple adjudication
      // this loop used to carry had nothing left to exempt.
      for (const power of powers) {
        const diff = diffProjection(server, [power], build, build.archetype.id!, projection, rawGlobal, state);
        rows += diff.rows;
        witness.powers += diff.witness.powers;
        witness.witnessed += diff.witness.witnessed;
        witness.unwitnessed.push(...diff.witness.unwitnessed);
        adjudicated.push(...diff.adjudicated.map((d) => `@ex${exemplarLevel} ${d}`));
        deltas.push(...diff.deltas.map((d) => `@ex${exemplarLevel} ${d}`));
      }
      suppression.push(`ex${exemplarLevel}: alpha ${suppressed ? 'suppressed' : 'live'}`);
    }

    // eslint-disable-next-line no-console
    console.warn(
      `[PROD6C-3f exemplar] ${server}: ${rows} magnitude rows across ex${above}/ex${below}, ${rescaled} powers rescaled (${magsRescaled} in their magnitude rows) — ${suppression.join(', ')}; ` +
        `shared exemplar pre-clamp ${exemplarCap}`,
    );
    if (adjudicated.length) {
      // eslint-disable-next-line no-console
      console.warn(`[PROD6C-3f exemplar] ${server} engine-supersedes-beta (accepted, ${adjudicated.length}):\n    ${adjudicated.slice(0, 20).join('\n    ')}`);
    }
    if (deltas.length) {
      // eslint-disable-next-line no-console
      console.error(`\n[PROD6C-3f exemplar] ${server} (${deltas.length} deltas)\n    ${deltas.slice(0, 60).join('\n    ')}`);
    }
    expect(rescaled, `${server}: exemplaring changed no enhancement bonus — the fixture's IOs are all attuned`).toBeGreaterThan(0);
    expect(
      magsRescaled,
      `${server}: exemplaring moved an enhancement bonus but no granted MAGNITUDE row against the ` +
        `level-50 run — with the resolver comparison starved by STRIP-1, nothing else grades the ` +
        `rescale reaching the rows`,
    ).toBeGreaterThan(0);
    // Unwitnessed magnitude half — the `magsRescaled` floor above is this body's verdict for it.
    // eslint-disable-next-line no-console
    console.warn(witnessLine(server, 'an exemplared build rescales IOs, and below 45 suppresses Alpha', witness));
    expect(deltas).toEqual([]);
  }, 90000);
});
