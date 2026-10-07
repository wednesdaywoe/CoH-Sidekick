/**
 * WS17 — per-dataset generated leveling-schedule module.
 *
 * Emits `src/data/datasets/<id>/generated/leveling-schedule.ts` from the
 * committed binary export, replacing the hand tables that previously lived in
 * `levels.ts` / `rebirth/index.ts`:
 *
 *   - power-pick + slot-grant schedules   <- leveling_schedule.json (schedules.bin)
 *   - pool/epic unlock levels + pool cap  <- leveling_schedule.json
 *   - per-power auto-granted slot levels  <- power records carrying
 *     `free_boost_slots_on_power` (PARSE6-1; Rebirth Health/Stamina today)
 *
 * Level semantics (power_system.c `CountForLevel`): every schedule array is a
 * sorted list of 0-based security levels; the count of entries <= L is the
 * total granted at 0-based level L. So each entry IS one grant, at 1-based
 * level `entry + 1`, and duplicates mean multiple grants at that level
 * (level 1 grants two power picks as two `0` entries).
 *
 * Per-power free slots (PowerInfo.c powerinfo_GetNumBoostsBought): offsets are
 * counted from the level the power was bought, so a bonus slot at offset `k`
 * lands at 1-based level `available_level + k + 1`. Offset 0 is the base slot
 * every power pick carries (the global schedule's `[0]`), which the planner
 * models separately — only offsets > 0 are emitted as auto-granted levels.
 *
 * Every decode is asserted — an unsorted schedule, a global free-slot grant
 * beyond the base slot, or an override missing its base slot throws instead
 * of shipping a soft-wrong module.
 *
 * Regenerate: node scripts/convert-leveling-schedule.cjs --dataset <id>
 */

const fs = require('fs');
const path = require('path');
const { parseDatasetArg, pipelinePath, NESTED_DATASET_DIRS } = require('./_dataset-paths.cjs');

const datasetId = parseDatasetArg();

// HC ships at the legacy flat layout (`exported_powers/...`); other datasets
// are namespaced under `exported_powers/<id>/` (same convention as
// convert-enhancement-curves.cjs).
const EXPORT_BASE = path.join(__dirname, '..', 'exported_powers');
const EXPORT_ROOT =
  datasetId === 'homecoming' && !fs.existsSync(path.join(EXPORT_BASE, datasetId, 'leveling_schedule.json'))
    ? EXPORT_BASE
    : path.join(EXPORT_BASE, datasetId);

const OUTPUT_PATH = pipelinePath(datasetId, 'leveling-schedule.json');

function fail(message) {
  throw new Error(`[convert-leveling-schedule ${datasetId}] ${message}`);
}

function assertSortedLevels(name, entries) {
  if (!Array.isArray(entries)) fail(`schedule array ${name} missing`);
  for (let i = 0; i < entries.length; i++) {
    if (!Number.isInteger(entries[i]) || entries[i] < 0 || entries[i] > 49) {
      fail(`${name}[${i}] = ${entries[i]} is not a 0-based level in 0..49`);
    }
    if (i > 0 && entries[i] < entries[i - 1]) {
      fail(`${name} is not sorted ascending at index ${i} (CountForLevel semantics require it)`);
    }
  }
}

/** Sorted 0-based schedule entries -> { 1-based level: grants at that level }. */
function grantsByLevel(entries) {
  const grants = {};
  for (const levelZeroBased of entries) {
    const level = levelZeroBased + 1;
    grants[level] = (grants[level] ?? 0) + 1;
  }
  return grants;
}

// ---------------------------------------------------------------------------
// Global schedule (schedules.bin)
// ---------------------------------------------------------------------------

const schedulePath = path.join(EXPORT_ROOT, 'leveling_schedule.json');
const schedule = JSON.parse(fs.readFileSync(schedulePath, 'utf-8')).schedule;

for (const key of ['free_boost_slots_on_power', 'pool_power_set', 'epic_power_set', 'power', 'assignable_boost']) {
  assertSortedLevels(key, schedule[key]);
}

// The planner models exactly one free slot arriving with every power pick
// (slot 0). Any global grant beyond that offset-0 base slot would silently
// break the slot budget, so it must fail loud, not be absorbed.
if (schedule.free_boost_slots_on_power.length !== 1 || schedule.free_boost_slots_on_power[0] !== 0) {
  fail(
    `global FreeBoostSlotsOnPower is [${schedule.free_boost_slots_on_power.join(', ')}] — ` +
    `the planner assumes exactly the offset-0 base slot per pick; model the extra grants before shipping`,
  );
}
if (schedule.power.length === 0) fail('power schedule is empty');
if (schedule.pool_power_set.length === 0) fail('pool_power_set schedule is empty');
if (schedule.epic_power_set.length === 0) fail('epic_power_set schedule is empty');

const powerPicks = grantsByLevel(schedule.power);
const slotGrants = grantsByLevel(schedule.assignable_boost);

// ---------------------------------------------------------------------------
// Per-power auto-granted slots (FreeBoostSlotsOnPower overrides, PARSE6-1)
// ---------------------------------------------------------------------------

// Other datasets nest under the HC flat root — a homecoming walk must not
// pick up their records.
// Derived, not a literal. This read `new Set(['rebirth', 'thunderspy'])` until 2026-09-26 and
// so omitted `brainstorm`, which made a homecoming run walk into `exported_powers/brainstorm/`.
const WALK_EXCLUDE = EXPORT_ROOT === EXPORT_BASE ? NESTED_DATASET_DIRS : new Set();

function* walkJsonFiles(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    if (entry.isDirectory()) {
      if (!WALK_EXCLUDE.has(entry.name) || dir !== EXPORT_ROOT) yield* walkJsonFiles(path.join(dir, entry.name));
    } else if (entry.name.endsWith('.json')) {
      yield path.join(dir, entry.name);
    }
  }
}

const autoGrantedSlotLevels = {};
for (const filePath of walkJsonFiles(EXPORT_ROOT)) {
  const raw = fs.readFileSync(filePath, 'utf-8');
  if (!raw.includes('"free_boost_slots_on_power"')) continue;
  const record = JSON.parse(raw);
  // leveling_schedule.json itself carries the key; power records are
  // distinguished by full_name.
  if (typeof record.full_name !== 'string') continue;

  const offsets = record.free_boost_slots_on_power;
  assertSortedLevels(`${record.full_name} free_boost_slots_on_power`, offsets);
  if (offsets[0] !== 0) {
    fail(`${record.full_name} override [${offsets.join(', ')}] lacks the offset-0 base slot the planner models on every power`);
  }
  if (!Number.isInteger(record.available_level) || record.available_level < 0) {
    fail(`${record.full_name} carries free_boost_slots_on_power but has no usable available_level`);
  }
  if (autoGrantedSlotLevels[record.name] !== undefined) {
    fail(`duplicate free_boost_slots_on_power carrier name "${record.name}" — the inherent-rules key must be unique`);
  }

  const bonusLevels = offsets.slice(1).map((offset) => record.available_level + offset + 1);
  autoGrantedSlotLevels[record.name] = bonusLevels;
}

// ---------------------------------------------------------------------------
// Emit
// ---------------------------------------------------------------------------


const grantNames = Object.keys(autoGrantedSlotLevels).sort();

/*
 * THE SHAPE THIS WRITES — `pipeline/<id>/leveling-schedule.json`, one object under the
 * key `LEVELING_SCHEDULE`. Derived from the committed binary export
 * (`leveling_schedule.json` = schedules.bin, plus per-power FreeBoostSlotsOnPower
 * overrides) — WS17. Levels are 1-BASED; grant counts follow power_system.c
 * CountForLevel semantics. The field notes below were the doc comments on the
 * `LevelingScheduleData` interface this used to spell into TypeScript.
 *
 *   powerPicks             Power picks granted at each level. Level 1 grants 2 —
 *                          a primary and a secondary.
 *   maxPowerPicks          Total power picks by level 50.
 *   slotGrants             Placeable enhancement slots granted at each level.
 *   totalSlots             Total placeable slots by level 50.
 *   poolUnlockLevel        Level of the first pool-powerset pick.
 *   maxPowerPools          Total pool-powerset picks — the power-pool cap.
 *   epicPoolLevel          Level of the first epic-powerset pick.
 *   autoGrantedSlotLevels  Levels at which the named power receives auto-granted bonus
 *                          slots (the per-power FreeBoostSlotsOnPower override, OUTSIDE
 *                          the totalSlots user budget). The offset-0 base slot is left
 *                          out: the planner models it on every power pick already.
 *                          Key-sorted, because an object whose key order tracked the
 *                          export's would diff on a reordering that means nothing.
 */
const out = JSON.stringify({
  LEVELING_SCHEDULE: {
    dataset: datasetId,
    powerPicks,
    maxPowerPicks: schedule.power.length,
    slotGrants,
    totalSlots: schedule.assignable_boost.length,
    poolUnlockLevel: schedule.pool_power_set[0] + 1,
    maxPowerPools: schedule.pool_power_set.length,
    epicPoolLevel: schedule.epic_power_set[0] + 1,
    autoGrantedSlotLevels: Object.fromEntries(grantNames.map((n) => [n, autoGrantedSlotLevels[n]])),
  },
});

fs.mkdirSync(path.dirname(OUTPUT_PATH), { recursive: true });
fs.writeFileSync(OUTPUT_PATH, out);
console.log(
  `  wrote ${path.relative(path.join(__dirname, '..'), OUTPUT_PATH)} ` +
  `(picks ${schedule.power.length}, slots ${schedule.assignable_boost.length}, pools ${schedule.pool_power_set.length}, ` +
  `auto-granted: ${grantNames.length ? grantNames.join(', ') : 'none'})`,
);
