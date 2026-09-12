/**
 * Slot Level Computation
 *
 * Computes which character level each enhancement slot was "granted" at,
 * based on the game's slot grant schedule.
 *
 * Two modes:
 * - **Respec mode** (slotOrder empty): slots are assigned by power-pick order,
 *   redistributing optimally as if doing a respec.
 * - **Leveling mode** (slotOrder populated): each entry's stored level is
 *   honored where the schedule allows, preserving the user's leveling sequence.
 *
 * Slot 0 on every power is free (comes with the power pick at that level).
 * Additional slots (index 1+) consume from the SLOT_GRANTS pool.
 *
 * Both modes route through one solver, `assignGrants`, and so does the
 * placement probe. Every grant is claimed by at most one slot, and a slot may
 * only claim a grant at or above its power's pick level — which makes this a
 * bipartite matching rather than a walk down a list. See that function for why
 * the difference is load-bearing.
 *
 * A slot the schedule cannot serve resolves to `null`, never to a plausible
 * substitute. The pick level in particular is NOT a safe fallback: on a power
 * taken at 38 it names a level that grants no slots at all, and on a power
 * taken at 3 it is indistinguishable from a real assignment (SLOT-1).
 *
 * Keys: Uses powerKey("category:internalName") to avoid collisions when
 * multiple powers share the same internalName across categories.
 */

import type { Build, SelectedPower } from '@/types';
import { getSlotGrants, getInherentAutoGrantedSlotLevels, getInherentAutoGrantedSlotCount } from '@/data';
import { powerKey, type PowerCategory } from '@/utils/power-key';

/**
 * Count enhancement slots that consume the level-up budget (the "/67" counter).
 *
 * Each power's base slot (index 0) is free. So are any auto-granted "freebie"
 * slots — Rebirth grants Health +2 and Stamina +2 outside the budget (71 total
 * slots, but only 67 allocatable during leveling). Those must NOT count, or the
 * counter goes red at 68–71. Slots placed BEYOND the freebies still count
 * (e.g. 6-slotting Health costs 3 budget slots: 6 − 1 base − 2 freebie). HC has
 * no auto-granted slots, so this reduces to "slots beyond the base".
 */
export function countPlacedBudgetSlots(
  build: Pick<Build, 'primary' | 'secondary' | 'pools' | 'epicPool' | 'inherents' | 'level'>,
): number {
  const extra = (powers: SelectedPower[]) =>
    powers.reduce((sum, p) => {
      const free = 1 + getInherentAutoGrantedSlotCount(p.internalName, build.level);
      return sum + Math.max(0, p.slots.length - free);
    }, 0);
  return (
    extra(build.primary.powers) +
    extra(build.secondary.powers) +
    build.pools.reduce((sum, pool) => sum + extra(pool.powers), 0) +
    (build.epicPool ? extra(build.epicPool.powers) : 0) +
    extra(build.inherents)
  );
}

/**
 * A resolved slot level, or `null` when the grant schedule has nothing left
 * this slot could legally occupy. Rule 1: an unassignable slot surfaces as a
 * visible break, never as a number that reads like a real assignment.
 */
export type SlotLevel = number | null;

/** Number of inherent (auto-granted) slots a power has, if any. */
function inherentCount(power: SelectedPower): number {
  return power.inherentSlotCount ?? 0;
}

/** Levels for a power's inherent slots, parallel to slots[1..inherentCount].
 *  Sourced from the active dataset's inherent-rules, so each server can plug
 *  in its own grant schedule (HC: none, Rebirth: Health [8,16] / Stamina
 *  [12,22], future servers TBD). */
function inherentLevels(power: SelectedPower): readonly number[] {
  const fixed = getInherentAutoGrantedSlotLevels(power.internalName);
  if (fixed.length === 0) return fixed;
  return fixed.slice(0, inherentCount(power));
}

/**
 * First user-allocatable slot index on a power: index 0 is the free base
 * slot, and inherents may carry auto-granted (fixed-level) slots after it.
 * Neither is drawn from the grant pool, and neither can have its level moved.
 */
function userSlotFloor(power: SelectedPower, category: PowerCategory): number {
  return category === 'inherent' ? 1 + inherentCount(power) : 1;
}

/** A power's pick level — the earliest a slot on it may be placed. */
function powerPickLevel(power: SelectedPower): number {
  // This read `category === 'inherent' ? 1` until 2026-09-12, which is a value the
  // export owns: `character_GrantAutoIssuePowers` gates an auto-issued power on
  // `available <= level`, so Rest arrives at 2 and Rebirth's Fitness four at 2
  // while Homecoming's arrive at 1. `createInherentSelectedPower` had already
  // resolved all of that into `power.level`; the constant threw it away and
  // stamped every inherent slot at 1 (MBDEXPORT-18, 20 of its 44).
  return power.level;
}

interface CategorizedPower {
  power: SelectedPower;
  category: PowerCategory;
}

/** Build a flat, sorted array of slot grant levels up to maxLevel. The schedule
 *  is server-aware (Thunderspy grants 71 slots vs the shared 67). */
function buildGrantPool(maxLevel: number, serverId?: string): number[] {
  const pool: number[] = [];
  for (const [lvl, count] of Object.entries(getSlotGrants(serverId))) {
    const level = parseInt(lvl);
    if (level > maxLevel) continue;
    for (let i = 0; i < count; i++) pool.push(level);
  }
  return pool.sort((a, b) => a - b);
}

/**
 * One placed slot that needs a grant from the schedule.
 *
 * `pickLevel` is the floor — a slot cannot sit earlier than its power was
 * taken. `preferred` is the level stored on the slotOrder entry, the level the
 * user actually placed it at while leveling; it is honored where the assignment
 * allows and given up where honoring it would strand another slot.
 */
interface SlotDemand {
  key: string;
  slotIndex: number;
  pickLevel: number;
  preferred?: number;
}

/**
 * Assign each demand a distinct grant, serving as many demands as the schedule
 * possibly can.
 *
 * This is a bipartite matching, and that it is a matching rather than a walk is
 * the whole point. A demand may take any grant at or above its pick level, so a
 * grant freed at level 21 is worthless to a power taken at 38 — but the slot
 * currently holding a level-39 grant may well be re-housable onto that freed 21,
 * which releases the 39 for the power that needs it. A first-come walk cannot
 * see that trade: it declares the build full while a valid assignment sits one
 * swap away, and the caller then invents a level for the slot it could not
 * place. That is SLOT-1, and the augmenting search below is the fix.
 *
 * Stored levels are seeded first, so an untouched build keeps every slot exactly
 * where the user put it. Kuhn's augmenting paths then extend that seed to a
 * maximum matching, displacing a stored level only when leaving it in place
 * would cost some other slot its grant entirely.
 *
 * Returns the grant INDEX per demand, or -1 for a demand the schedule genuinely
 * cannot serve — over-subscription is a real state and it must be reportable.
 */
function assignGrants(demands: SlotDemand[], grantPool: number[]): number[] {
  const demandGrant = new Array<number>(demands.length).fill(-1);
  const grantOwner = new Array<number>(grantPool.length).fill(-1);

  const claim = (demand: number, grant: number) => {
    demandGrant[demand] = grant;
    grantOwner[grant] = demand;
  };

  // Seed with the stored levels that are legal and still free. `grantPool` is
  // sorted, and grants at one level are interchangeable, so the first free
  // exact match is as good as any other.
  demands.forEach((demand, d) => {
    if (demand.preferred === undefined || demand.preferred < demand.pickLevel) return;
    const grant = grantPool.findIndex(
      (level, g) => level === demand.preferred && grantOwner[g] === -1
    );
    if (grant !== -1) claim(d, grant);
  });

  // Grants are scanned lowest-first: a demand that takes the smallest grant it
  // can use leaves the high ones for the demands that have nowhere else to go.
  //
  // Free grants are scanned BEFORE owned ones, and that order is load-bearing.
  // Displacing an owner is how a late power reaches a grant nothing else can
  // spare; doing it when a free grant was available instead rewrites a level the
  // user placed by hand, and the newcomer lands on the low grant it took. That
  // is the reversed leveling order the placement probe reported (SLOT-2).
  // Maximality is untouched — a free grant is an augmenting path of length one,
  // and the second pass still explores every owned grant when none is free.
  const augment = (demand: number, seen: boolean[]): boolean => {
    const { pickLevel } = demands[demand];
    for (let grant = 0; grant < grantPool.length; grant++) {
      if (grantPool[grant] < pickLevel) continue;
      if (seen[grant] || grantOwner[grant] !== -1) continue;
      seen[grant] = true;
      claim(demand, grant);
      return true;
    }
    for (let grant = 0; grant < grantPool.length; grant++) {
      if (grantPool[grant] < pickLevel) continue;
      if (seen[grant]) continue;
      seen[grant] = true;
      const owner = grantOwner[grant];
      if (owner === -1 || augment(owner, seen)) {
        claim(demand, grant);
        return true;
      }
    }
    return false;
  };

  const unseeded = demands
    .map((_, d) => d)
    .filter((d) => demandGrant[d] === -1)
    .sort((a, b) => demands[a].pickLevel - demands[b].pickLevel);
  for (const demand of unseeded) {
    augment(demand, new Array<boolean>(grantPool.length).fill(false));
  }

  return demandGrant;
}

/** Write a solved assignment back onto the per-power level arrays. */
function applyAssignment(
  result: Map<string, SlotLevel[]>,
  demands: SlotDemand[],
  grantPool: number[]
): void {
  const assigned = assignGrants(demands, grantPool);
  demands.forEach((demand, d) => {
    const levels = result.get(demand.key);
    if (!levels) return;
    const grant = assigned[d];
    levels[demand.slotIndex] = grant === -1 ? null : grantPool[grant];
  });
}

/** Every user-allocated slot in the build, with no stored levels to honor. */
function collectRespecDemands(allPowers: CategorizedPower[]): SlotDemand[] {
  const demands: SlotDemand[] = [];
  for (const { power, category } of allPowers) {
    const key = powerKey(category, power.internalName);
    const pickLevel = powerPickLevel(power);
    for (let s = userSlotFloor(power, category); s < power.slots.length; s++) {
      demands.push({ key, slotIndex: s, pickLevel });
    }
  }
  return demands;
}

/**
 * Every user-allocated slot in the build, carrying the stored level from its
 * `slotOrder` entry where it has one.
 *
 * Two rules here were once split across the compute and the placement probe,
 * which is how the two came to disagree (SLOT-1):
 *
 * - An entry that addresses no real user slot — power gone, index past the end
 *   of the row, or pointing into an inherent's auto-granted block — contributes
 *   nothing. The probe used to let such entries consume grants, so it reported
 *   the pool as fuller than the display believed it was.
 * - A placed slot with NO entry is still a placed slot and still needs a grant.
 *   Leveling mode used to skip it, which is why a partially-populated slotOrder
 *   read as "every untouched slot sits at its power's pick level".
 */
function collectLevelingDemands(build: Build, allPowers: CategorizedPower[]): SlotDemand[] {
  const shape = new Map<string, { pickLevel: number; slotCount: number; floor: number }>();
  for (const { power, category } of allPowers) {
    shape.set(powerKey(category, power.internalName), {
      pickLevel: powerPickLevel(power),
      slotCount: power.slots.length,
      floor: userSlotFloor(power, category),
    });
  }

  const demands: SlotDemand[] = [];
  const claimed = new Set<string>();
  for (const entry of build.slotOrder) {
    const category = resolveSlotCategory(build, entry.powerName, entry.category);
    if (!category) continue;
    const key = powerKey(category, entry.powerName);
    const power = shape.get(key);
    if (!power) continue;
    if (entry.slotIndex < power.floor || entry.slotIndex >= power.slotCount) continue;
    // Duplicate entries for one slot would each claim a grant. First wins.
    const id = `${key}|${entry.slotIndex}`;
    if (claimed.has(id)) continue;
    claimed.add(id);
    demands.push({
      key,
      slotIndex: entry.slotIndex,
      pickLevel: power.pickLevel,
      ...(entry.level !== undefined ? { preferred: entry.level } : {}),
    });
  }

  for (const { power, category } of allPowers) {
    const key = powerKey(category, power.internalName);
    const pickLevel = powerPickLevel(power);
    for (let s = userSlotFloor(power, category); s < power.slots.length; s++) {
      if (claimed.has(`${key}|${s}`)) continue;
      demands.push({ key, slotIndex: s, pickLevel });
    }
  }

  return demands;
}

/** Collect all powers in the build. */
function collectAllPowers(build: Build): CategorizedPower[] {
  const allPowers: CategorizedPower[] = [];
  const categoryOrder: PowerCategory[] = ['inherent', 'primary', 'secondary', 'pool', 'epic'];

  for (const p of build.inherents) {
    allPowers.push({ power: p, category: 'inherent' });
  }
  // Auto-granted sub-powers (Kheldian forms) belong here. They are slottable and
  // `countPlacedBudgetSlots` bills their extra slots to the same budget, so
  // holding them out of the demand list made the placement probe and the display
  // compute disagree about how much of the pool was spent (SLOT-1).
  for (const p of build.primary.powers) {
    allPowers.push({ power: p, category: 'primary' });
  }
  for (const p of build.secondary.powers) {
    allPowers.push({ power: p, category: 'secondary' });
  }
  for (const pool of build.pools) {
    for (const p of pool.powers) {
      allPowers.push({ power: p, category: 'pool' });
    }
  }
  if (build.epicPool) {
    for (const p of build.epicPool.powers) {
      allPowers.push({ power: p, category: 'epic' });
    }
  }

  // Sort by effective pick level, then by category for ties.
  const effectiveLevel = (cp: CategorizedPower) => powerPickLevel(cp.power);

  allPowers.sort((a, b) => {
    const aLvl = effectiveLevel(a);
    const bLvl = effectiveLevel(b);
    if (aLvl !== bLvl) return aLvl - bLvl;
    return categoryOrder.indexOf(a.category) - categoryOrder.indexOf(b.category);
  });

  return allPowers;
}

/** Initialize result map with slot 0 = pick level for every power.
 *  For inherent powers with auto-granted inherent slots (Rebirth Health/Stamina),
 *  pre-fill those slot indices with their fixed grant levels so subsequent
 *  pool consumption skips them. */
function initSlotLevels(allPowers: CategorizedPower[]): Map<string, SlotLevel[]> {
  const result = new Map<string, SlotLevel[]>();
  for (const { power, category } of allPowers) {
    const pickLevel = powerPickLevel(power);
    // Slot 0 comes free with the pick. Every later slot starts unassigned and is
    // filled in by the solver, so a slot the solver cannot serve stays visibly
    // null rather than inheriting the pick level.
    const levels: SlotLevel[] = new Array(power.slots.length).fill(null);
    if (levels.length > 0) levels[0] = pickLevel;
    if (category === 'inherent') {
      const fixed = inherentLevels(power);
      for (let i = 0; i < fixed.length && i + 1 < levels.length; i++) {
        levels[i + 1] = fixed[i];
      }
    }
    result.set(powerKey(category, power.internalName), levels);
  }
  return result;
}

/**
 * Resolve which category a slotOrder entry belongs to.
 * New entries have an explicit `category` field.
 * Legacy entries (no category) fall back to searching the build.
 */
function resolveSlotCategory(
  build: Build,
  powerName: string,
  category?: string
): PowerCategory | null {
  if (category && ['primary', 'secondary', 'pool', 'epic', 'inherent'].includes(category)) {
    return category as PowerCategory;
  }
  // Legacy fallback: search categories in standard order
  if (build.primary.powers.some((p) => p.internalName === powerName)) return 'primary';
  if (build.secondary.powers.some((p) => p.internalName === powerName)) return 'secondary';
  for (const pool of build.pools) {
    if (pool.powers.some((p) => p.internalName === powerName)) return 'pool';
  }
  if (build.epicPool?.powers.some((p) => p.internalName === powerName)) return 'epic';
  if (build.inherents.some((p) => p.internalName === powerName)) return 'inherent';
  return null;
}

/**
 * Respec mode: no leveling history to honor, so every slot is up for grabs and
 * the solver places them all from scratch.
 */
function computeSlotLevelsRespec(build: Build): Map<string, SlotLevel[]> {
  const allPowers = collectAllPowers(build);
  const result = initSlotLevels(allPowers);
  applyAssignment(
    result,
    collectRespecDemands(allPowers),
    buildGrantPool(build.level, build.serverId)
  );
  return result;
}

/**
 * Leveling mode: same solver, with each entry's stored level fed in as a
 * preference.
 *
 * Storing the assigned level per entry is what makes removing a slot behave like
 * Mids — peers keep the level they were placed at instead of cascading down to
 * fill the gap, and the freed grant is what the next placement draws on. The
 * solver adds the half that was missing: when the freed grant sits below the
 * next placement's pick level, it re-houses whichever slot can take it and hands
 * the newly-released high grant over, instead of reporting the build full.
 */
function computeSlotLevelsLeveling(build: Build): Map<string, SlotLevel[]> {
  const allPowers = collectAllPowers(build);
  const result = initSlotLevels(allPowers);
  applyAssignment(
    result,
    collectLevelingDemands(build, allPowers),
    buildGrantPool(build.level, build.serverId)
  );
  return result;
}

/**
 * A key no real power can produce, for the speculative demand the probes below
 * add to the build. `powerKey` always emits `category:name`, so a leading NUL
 * cannot collide.
 */
const PROBE_KEY = '\u0000probe';

/** Every demand the build currently makes on the grant pool. */
function collectDemands(build: Build, allPowers: CategorizedPower[]): SlotDemand[] {
  return build.slotOrder.length > 0
    ? collectLevelingDemands(build, allPowers)
    : collectRespecDemands(allPowers);
}

/**
 * Solve the build as it stands plus one speculative slot, and report the level
 * that slot lands on — `null` when the schedule cannot serve it.
 *
 * The probe IS the compute, deliberately: it runs the same solver over the same
 * demands. Answering from a separate walk down `slotOrder` is exactly how the
 * probe and the display came to disagree, and a probe that under-reports what is
 * free is what makes a placement land with no level at all (SLOT-1).
 */
function probeGrantLevel(
  build: Build,
  pickLevel: number,
  existing: SlotDemand[]
): number | null {
  const grantPool = buildGrantPool(build.level, build.serverId);
  const probe: SlotDemand = { key: PROBE_KEY, slotIndex: 0, pickLevel };
  const assigned = assignGrants([...existing, probe], grantPool);
  const grant = assigned[assigned.length - 1];
  return grant === -1 ? null : grantPool[grant];
}

/**
 * The grant level a new slot on a power with `pickLevel` would be placed at, or
 * `null` when every grant the slot could legally occupy is already spoken for.
 *
 * `null` is a real answer, not an error: 25 slots on powers taken at 38 or later
 * cannot all be served by the 24 grants Homecoming issues from 39 on. Callers
 * must refuse the placement rather than invent a level for it.
 */
export function findNextAvailableGrantLevel(
  build: Build,
  pickLevel: number
): number | null {
  return probeGrantLevel(build, pickLevel, collectDemands(build, collectAllPowers(build)));
}

/**
 * Compute slot level assignments for every power in the build.
 *
 * Returns a Map keyed by powerKey ("category:internalName"), where each value is
 * parallel to the power's slots array. Index 0 = power pick level (the free
 * slot), index 1+ = the grant level consumed for that slot, or `null` where the
 * schedule has no grant left the slot could legally occupy.
 *
 * `levelUpMode` off (SLOT-3): a real in-game respec hands the player their full
 * earned slot budget as one freely assignable pool — no power's pick level
 * bounds which grant a slot can use, so there is no such thing as "this slot's
 * level" outside a sequential leveling walk. Returns an empty map; every
 * consumer already treats a missing entry as "nothing to show" rather than an
 * error, so this needs no further special-casing at the call sites.
 *
 * `levelUpMode` on: unchanged — leveling mode if `slotOrder` has entries
 * (honoring any stored preferences, including level-less ones written while
 * the mode was off — see `collectLevelingDemands`), otherwise respec mode.
 */
export function computeAllSlotLevels(build: Build, levelUpMode: boolean): Map<string, SlotLevel[]> {
  if (!levelUpMode) return new Map();
  // Defensive: `slotOrder` is normally initialized by every load path
  // (importBuild / hydrateBuild / rehydrate migration), but a build whose
  // rehydrate migration aborted partway can reach here with it undefined.
  // This is the single chokepoint for slot-level computation, so guarding
  // here keeps every consumer (useSlotLevels, freeze/move helpers, exports)
  // from crashing on a half-migrated build.
  if (!build.slotOrder || build.slotOrder.length === 0) {
    return computeSlotLevelsRespec(build);
  }
  return computeSlotLevelsLeveling(build);
}

/**
 * Slot levels for export interop (Mids .mbd, print, forum text) — unlike
 * `computeAllSlotLevels`, always returns a real, schedule-legal assignment.
 * Mids' own file format carries a `Level` per slot regardless of how the
 * build was planned, so an export needs *some* number even when
 * `levelUpMode` is off. That number is a synthetic packing (respec mode's
 * solver, ignoring pick order), not a claim about the user's actual leveling
 * history — callers should say so in the UI once, not per slot (SLOT-3).
 */
export function computeExportSlotLevels(build: Build, levelUpMode: boolean): Map<string, SlotLevel[]> {
  if (!levelUpMode) return computeSlotLevelsRespec(build);
  return computeAllSlotLevels(build, levelUpMode);
}

/**
 * Rewrite stored slot levels the solver could not honor to the level it actually
 * assigned. Mutates the build in place; returns whether anything changed.
 *
 * SLOT-2 stamped every placement with the lowest grant in the build, so a build
 * leveled through that defect carries a run of entries all claiming one level.
 * The display is already right — the solver seeds what the schedule can serve
 * and assigns the rest forward — but an entry whose level no grant can honor is,
 * on every recompute, indistinguishable from an entry that never had one. It
 * cascades on a removal instead of holding its place, which is the entire reason
 * a level is stored.
 *
 * A stored level the solver CAN honor always survives, because honoring it is
 * what makes computed and stored agree. So a difference means the stored level
 * was already dead, and writing over it repairs rather than rewrites the
 * leveling history. `scrubFabricatedSlotLevels` cannot do this job: the levels
 * here are ones the schedule genuinely issues, just more often than it issues
 * them.
 */
export function reconcileStoredSlotLevels(build: Build, levelUpMode: boolean): boolean {
  if (!levelUpMode) return false;
  if (!build.slotOrder?.length) return false;
  const levels = computeAllSlotLevels(build, levelUpMode);
  let changed = false;
  for (const entry of build.slotOrder) {
    const category = resolveSlotCategory(build, entry.powerName, entry.category);
    if (!category) continue;
    const level = levels.get(powerKey(category, entry.powerName))?.[entry.slotIndex];
    // Same rule as the freezers: a slot the schedule could not serve stays
    // level-less, so the solver can re-house it once the pressure lifts.
    if (level === null || level === undefined) continue;
    if (entry.level === level) continue;
    entry.level = level;
    changed = true;
  }
  return changed;
}

/**
 * Ensure every non-base, non-inherent-auto slot on every power has a
 * matching `slotOrder` entry with a stored `level`. Mutates the build in
 * place. Safe to run repeatedly.
 *
 * Why this exists: builds imported from Mids (and some legacy paths) leave
 * `slotOrder` empty even though the powers carry many extra slots. That's
 * fine for *initial* display — `computeAllSlotLevels` falls into respec
 * mode and assigns levels from the grant pool by pick order. But as soon
 * as the user adds OR removes a single slot, `slotOrder` becomes
 * non-empty, which flips computation into leveling mode. Leveling mode
 * only sets levels for slots with a matching `slotOrder` entry; every
 * other slot collapses to its power's pick level. The visual symptom is
 * "all my slot levels suddenly mirror their power's pick level".
 *
 * Running this once on import / rehydration captures the respec-mode
 * levels and locks them in as stored levels, so subsequent
 * add/remove interactions preserve every other slot's position.
 */
export function ensureSlotOrderPopulated(build: Build, levelUpMode: boolean): boolean {
  if (!levelUpMode) return false;
  const allPowers = collectAllPowers(build);

  // Build a set of (category, powerName, slotIndex) keys for existing entries
  const existing = new Set<string>();
  for (const e of build.slotOrder) {
    const cat = resolveSlotCategory(build, e.powerName, e.category);
    if (!cat) continue;
    existing.add(`${cat}|${e.powerName}|${e.slotIndex}`);
  }

  const computed = computeAllSlotLevels(build, levelUpMode);
  const newEntries: Build['slotOrder'][number][] = [];

  for (const { power, category } of allPowers) {
    const key = powerKey(category, power.internalName);
    const levels = computed.get(key);
    if (!levels) continue;
    // Skip slot 0 (free with the power) and any auto-granted inherent slots
    // (those sit at fixed levels and aren't user-allocated).
    const skipUntil = category === 'inherent' ? 1 + inherentCount(power) : 1;
    for (let s = skipUntil; s < power.slots.length; s++) {
      if (existing.has(`${category}|${power.internalName}|${s}`)) continue;
      const level = levels[s];
      // A slot the schedule could not serve has no level to freeze. Writing one
      // anyway is what turned a transient over-subscription into a stored level
      // that no grant matches, which no later solve could ever honor (SLOT-1).
      if (level === null || level === undefined) continue;
      newEntries.push({
        powerName: power.internalName,
        slotIndex: s,
        category,
        level,
      });
    }
  }

  if (newEntries.length === 0) return false;
  build.slotOrder = [...build.slotOrder, ...newEntries];
  return true;
}

/**
 * Back-fill `level` on slotOrder entries that don't have one yet. Mutates
 * the build in place. Safe to run repeatedly. Used as a migration when
 * loading legacy builds so the Mids-style remove/replace behavior kicks in
 * immediately without waiting for the user to re-place every slot.
 */
export function backfillSlotOrderLevels(build: Build, levelUpMode: boolean): boolean {
  if (!levelUpMode) return false;
  if (build.slotOrder.length === 0) return false;
  const needsBackfill = build.slotOrder.some((e) => e.level === undefined);
  if (!needsBackfill) return false;

  const levels = computeSlotLevelsLeveling(build);
  let changed = false;
  for (const entry of build.slotOrder) {
    if (entry.level !== undefined) continue;
    const cat = resolveSlotCategory(build, entry.powerName, entry.category);
    if (!cat) continue;
    const key = powerKey(cat, entry.powerName);
    const powerLevels = levels.get(key);
    if (!powerLevels || entry.slotIndex >= powerLevels.length) continue;
    const level = powerLevels[entry.slotIndex];
    // Same guard as `ensureSlotOrderPopulated`: an unassignable slot stays
    // level-less so the solver can re-house it once the pressure lifts.
    if (level === null) continue;
    entry.level = level;
    changed = true;
  }
  return changed;
}

/**
 * Drop stored slot levels the grant schedule does not issue. Mutates the build
 * in place; returns whether anything changed.
 *
 * Before SLOT-1, a placement the allocator could not serve was written out with
 * no level and then DISPLAYED at its power's pick level — and the rehydrate
 * backfill froze that display value in as a stored level. A power taken at 38 on
 * Homecoming produced `level: 38`, a level that grants no slots at all, so the
 * entry could never be honored again and fell through to greedy on every
 * recompute. Clearing the level rather than the entry keeps the slot and lets
 * the solver re-house it properly.
 */
export function scrubFabricatedSlotLevels(build: Build): boolean {
  if (!build.slotOrder?.length) return false;
  const grants = getSlotGrants(build.serverId);
  let changed = false;
  for (const entry of build.slotOrder) {
    if (entry.level === undefined) continue;
    if ((grants[entry.level] ?? 0) > 0) continue;
    delete entry.level;
    changed = true;
  }
  return changed;
}

// ============================================================
// SLOT-LEVEL MOVE / SWAP
// ============================================================

/**
 * Identifies one allocated slot on one power. `category` is optional — when
 * omitted it is resolved from the build (matching `removeSlot`'s by-name
 * behavior), so callers that only have a power's internal name can still
 * address its slots.
 */
export interface SlotLevelRef {
  powerName: string;
  slotIndex: number;
  category?: PowerCategory;
}

/** A power resolved to a concrete category, as `collectAllPowers` sees it. */
interface ResolvedRef {
  power: SelectedPower;
  category: PowerCategory;
}

/** Resolve a SlotLevelRef to its power + concrete category, or null. */
function resolveRef(build: Build, ref: SlotLevelRef): ResolvedRef | null {
  const category = resolveSlotCategory(build, ref.powerName, ref.category);
  if (!category) return null;
  const cp = collectAllPowers(build).find(
    (c) => c.category === category && c.power.internalName === ref.powerName
  );
  return cp ? { power: cp.power, category } : null;
}

/**
 * Whether a single slot is a user-allocated slot whose level can be moved
 * (i.e. not the free base slot and not an auto-granted inherent slot, and it
 * actually exists on the power). Used to gate the "Move slot level…" menu item.
 */
export function isMovableSlot(build: Build, ref: SlotLevelRef): boolean {
  const r = resolveRef(build, ref);
  if (!r) return false;
  return (
    ref.slotIndex >= userSlotFloor(r.power, r.category) &&
    ref.slotIndex < r.power.slots.length
  );
}

/**
 * Whether the grant levels of two slots can be swapped. Both must be
 * user-allocated slots (see `isMovableSlot`), and after the swap each slot's
 * level must still be >= its power's pick level (a slot can't sit at a level
 * earlier than its power was taken).
 */
export function canMoveSlotLevel(
  build: Build,
  source: SlotLevelRef,
  target: SlotLevelRef,
  levelUpMode: boolean
): boolean {
  // Nothing to swap: outside leveling mode a slot carries no level at all
  // (SLOT-3), so there is no dated value to trade between two slots.
  if (!levelUpMode) return false;

  const s = resolveRef(build, source);
  const t = resolveRef(build, target);
  if (!s || !t) return false;

  // Same slot → nothing to swap.
  if (s.category === t.category && source.powerName === target.powerName && source.slotIndex === target.slotIndex) {
    return false;
  }

  if (!isMovableSlot(build, source) || !isMovableSlot(build, target)) return false;

  const levels = computeAllSlotLevels(build, levelUpMode);
  const sLevel = levels.get(powerKey(s.category, source.powerName))?.[source.slotIndex];
  const tLevel = levels.get(powerKey(t.category, target.powerName))?.[target.slotIndex];
  // `null` is a slot the schedule could not place at all — there is no level on
  // it to trade away, so the swap is not offered.
  if (sLevel === undefined || sLevel === null) return false;
  if (tLevel === undefined || tLevel === null) return false;

  // After the swap, source holds tLevel and target holds sLevel.
  return tLevel >= powerPickLevel(s.power) &&
    sLevel >= powerPickLevel(t.power);
}

/**
 * Return a NEW build with the grant levels of two slots swapped. Enhancers are
 * untouched — they live in `power.slots`, not `slotOrder`, so swapping levels
 * is purely a slotOrder operation. Returns null if the swap is invalid.
 *
 * Freezes current computed levels into `slotOrder` first (via
 * `ensureSlotOrderPopulated`) so the swap is well-defined even from respec
 * mode or a partially-populated slotOrder.
 */
export function applySlotLevelMove(
  build: Build,
  source: SlotLevelRef,
  target: SlotLevelRef,
  levelUpMode: boolean
): Build | null {
  if (!canMoveSlotLevel(build, source, target, levelUpMode)) return null;

  const s = resolveRef(build, source)!;
  const t = resolveRef(build, target)!;

  const next: Build = { ...build, slotOrder: [...build.slotOrder] };
  ensureSlotOrderPopulated(next, levelUpMode);
  backfillSlotOrderLevels(next, levelUpMode);

  const levels = computeAllSlotLevels(next, levelUpMode);
  // Both non-null: `canMoveSlotLevel` above refuses the swap otherwise.
  const sLevel = levels.get(powerKey(s.category, source.powerName))![source.slotIndex]!;
  const tLevel = levels.get(powerKey(t.category, target.powerName))![target.slotIndex]!;

  const matches = (
    entry: Build['slotOrder'][number],
    powerName: string,
    slotIndex: number,
    category: PowerCategory
  ): boolean =>
    entry.powerName === powerName &&
    entry.slotIndex === slotIndex &&
    resolveSlotCategory(next, entry.powerName, entry.category) === category;

  next.slotOrder = next.slotOrder.map((entry) =>
    matches(entry, source.powerName, source.slotIndex, s.category)
      ? { ...entry, level: tLevel }
      : matches(entry, target.powerName, target.slotIndex, t.category)
        ? { ...entry, level: sLevel }
        : entry
  );

  return next;
}

/** Identifies a destination power (not a specific slot) for a relocation. */
export interface PowerRef {
  powerName: string;
  category?: PowerCategory;
}

/**
 * Whether the slot at `source` can be RELOCATED onto `target` power — the
 * "move a slot from one power to another" gesture. This is distinct from
 * `canMoveSlotLevel`, which only swaps two slots' grant levels in place.
 *
 * Valid when: the source is a user-allocated slot (not a free base slot or an
 * auto-granted inherent slot), the target resolves to a DIFFERENT power, the
 * target has an open slot (`slots.length < maxSlots`), and the schedule can
 * actually place the slot on the target.
 *
 * That last check is not redundant with the slot BUDGET, which is net-neutral
 * across a relocation. The schedule is not: freeing a grant at level 21 does
 * nothing for a power taken at 38, and dragging a slot across that gap is
 * precisely the gesture that used to produce a slot stamped with an impossible
 * level (SLOT-1).
 */
export function canRelocateSlot(
  build: Build,
  source: SlotLevelRef,
  target: PowerRef,
  levelUpMode: boolean,
): boolean {
  if (!isMovableSlot(build, source)) return false;
  const s = resolveRef(build, source);
  const t = resolveRef(build, {
    powerName: target.powerName,
    slotIndex: 0,
    category: target.category,
  });
  if (!s || !t) return false;
  // Same power → nothing to relocate (use "move slot level" for in-place).
  if (s.category === t.category && source.powerName === target.powerName) return false;
  // Target needs an open slot.
  if (t.power.slots.length >= t.power.maxSlots) return false;
  // Outside leveling mode there is no dated schedule to check the relocation
  // against — the budget is net-neutral across a move, so an open slot is
  // the only requirement (SLOT-3).
  if (!levelUpMode) return true;
  return relocatedGrantLevel(build, source, s, t) !== null;
}

/**
 * The grant level a relocated slot would land on, or `null` when the schedule
 * cannot serve it once the source slot's own grant is returned to the pool.
 */
function relocatedGrantLevel(
  build: Build,
  source: SlotLevelRef,
  s: ResolvedRef,
  t: ResolvedRef
): number | null {
  const sourceKey = powerKey(s.category, source.powerName);
  const remaining = collectDemands(build, collectAllPowers(build)).filter(
    (d) => !(d.key === sourceKey && d.slotIndex === source.slotIndex)
  );
  return probeGrantLevel(build, powerPickLevel(t.power), remaining);
}
