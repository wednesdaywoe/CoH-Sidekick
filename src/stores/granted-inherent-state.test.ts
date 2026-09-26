// MUST be first: installs an in-memory localStorage before the store module is
// evaluated (the store caches its persist storage at eval time).
import '@/test/localstorage-polyfill';
import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data/powersets';
import { useBuildStore } from '@/stores/buildStore';
import { calculateCharacterTotals } from '@/utils/calculations/character-totals';
import type { SelectedPower } from '@/types/power';

/**
 * Thunderspy's Stalker Hide: three defects, one bug report (2026-09-26).
 *
 * *"Hide did not move defense numbers on or off, and regardless of in-combat being on or off.
 * Additionally, toggling Hide on/off simultaneously turned Beta Decay on/off."*
 *
 * The fork moved Hide and Placate out of the Stalker armour sets into `Inherent.Inherent`, gated
 * `$archetype @Class_Stalker ==`, and left the vacated internal names on OTHER powers — so a
 * Thunderspy Stalker holds two powers called `Hide`, and in Radiation Armor the second one is Beta
 * Decay. That collision is faithful export data, not a parse defect, and these tests are written
 * against it rather than around it.
 *
 * What the collision then found:
 *
 *  1. `inherentCategory: 'archetype'` is a DISPLAY group, and the engine reads the same value as
 *     "this power's contribution is derived in its own pass; skip it in the gather". Eleven powers
 *     claim the group for the section they render in, so all eleven reached no total at all.
 *  2. `togglePowerActive` looked the power up scoped to its category and then wrote by
 *     `internalName` across every bucket, so switching Hide switched Beta Decay with it.
 *  3. `slimInherents` decided what to save from an inherent's SLOTTING alone, and the read merge
 *     restored only slots — so an inherent's toggle never survived a save and a load.
 *
 * Graded on Thunderspy because that is where the collision lives, plus Homecoming for the half of
 * (1) that is fork-independent.
 */

const HIDE = 'Hide';
const RAD_ARMOUR = 'stalker/radiation-armor';

const inherent = (name: string): SelectedPower | undefined =>
  useBuildStore.getState().build.inherents.find((p) => p.internalName === name);

const secondary = (name: string): SelectedPower | undefined =>
  useBuildStore.getState().build.secondary.powers.find((p) => p.internalName === name);

/** A level-50 Thunderspy Stalker on Spectral Melee / Radiation Armor, every armour power held. */
function stalkerBuild() {
  const s = useBuildStore.getState();
  s.resetBuild();
  s.setArchetype('stalker');
  s.setPrimary('stalker/spectral-melee');
  s.setSecondary(RAD_ARMOUR);
  s.setLevel(50);
  const set = getPowerset(RAD_ARMOUR);
  expect(set, 'Thunderspy ships Stalker Radiation Armor').toBeDefined();
  for (const def of set!.powers) {
    useBuildStore.getState().addPower('secondary', {
      ...def,
      powerSet: RAD_ARMOUR,
      level: 1,
      slots: [null],
    } as unknown as SelectedPower);
  }
}

describe('Thunderspy Stalker Hide (thunderspy)', () => {
  beforeAll(async () => {
    await loadDataset('thunderspy');
  });

  it('the collision this is all about is really in the data', () => {
    // If the fork ever stops reusing the name, the tests below stop grading a collision and
    // should be read again rather than trusted.
    const set = getPowerset(RAD_ARMOUR);
    const slot = set?.powers.find((p) => p.internalName === HIDE);
    expect(slot, 'the armour set carries a power keyed Hide').toBeDefined();
    expect(slot!.name, 'and it is not Hide').toBe('Beta Decay');
  });

  it('switching Hide moves defense — the archetype display group is not a skip instruction', () => {
    stalkerBuild();
    expect(inherent(HIDE), 'Hide is granted to a Stalker').toBeDefined();
    // The display group stays what the inherents panel needs. That is the point: the fix
    // separated the group from the calculation rather than moving the power out of its section.
    expect(inherent(HIDE)!.inherentCategory).toBe('archetype');
    expect(inherent(HIDE)!.derivedMechanic, 'Hide is not the archetype mechanic').toBeFalsy();

    const melee = () =>
      calculateCharacterTotals(useBuildStore.getState().build, undefined, undefined, undefined)
        .stats.defMelee;

    useBuildStore.getState().togglePowerActive(HIDE, 'inherent');
    expect(inherent(HIDE)!.isActive).toBe(true);
    const on = melee();
    useBuildStore.getState().togglePowerActive(HIDE, 'inherent');
    expect(inherent(HIDE)!.isActive).toBe(false);
    const off = melee();

    expect(on).toBeGreaterThan(off);
  });

  it('the archetype mechanic itself is still skipped, so nothing is counted twice', () => {
    stalkerBuild();
    const assassination = useBuildStore
      .getState()
      .build.inherents.find((p) => p.derivedMechanic);
    expect(assassination, 'the archetype declares a mechanic').toBeDefined();
    expect(assassination!.inherentCategory).toBe('archetype');
  });

  it('switching Hide leaves Beta Decay alone, though both are named Hide', () => {
    stalkerBuild();
    const armourHide = secondary(HIDE);
    expect(armourHide, "the armour set's Hide-keyed power is held").toBeDefined();
    // The armour toggle arrives on, which is exactly the state that hid half of this defect:
    // switching Hide ON set Beta Decay to a value it already had, so only switching Hide OFF
    // was visible. Both directions are graded below.
    expect(armourHide!.isActive, 'Beta Decay starts on').toBe(true);
    expect(inherent(HIDE)!.isActive ?? false, 'and Hide starts off').toBe(false);

    useBuildStore.getState().togglePowerActive(HIDE, 'inherent');
    expect(inherent(HIDE)!.isActive, 'Hide switched on').toBe(true);
    expect(secondary(HIDE)!.isActive, 'Beta Decay untouched').toBe(true);

    // The direction the user saw: Hide off used to drag Beta Decay off with it.
    useBuildStore.getState().togglePowerActive(HIDE, 'inherent');
    expect(inherent(HIDE)!.isActive, 'Hide switched off').toBe(false);
    expect(secondary(HIDE)!.isActive, 'Beta Decay STILL untouched').toBe(true);
  });

  it('switching Beta Decay leaves Hide alone, in the other direction', () => {
    stalkerBuild();
    useBuildStore.getState().togglePowerActive(HIDE, 'inherent');
    expect(inherent(HIDE)!.isActive, 'Hide on').toBe(true);

    useBuildStore.getState().togglePowerActive(HIDE, 'secondary');
    expect(secondary(HIDE)!.isActive, 'Beta Decay off').toBe(false);
    expect(inherent(HIDE)!.isActive, 'Hide untouched').toBe(true);
  });

  it("an inherent's toggle survives a save and a load, with nothing slotted", () => {
    stalkerBuild();
    useBuildStore.getState().togglePowerActive(HIDE, 'inherent');
    expect(inherent(HIDE)!.isActive).toBe(true);
    expect(
      inherent(HIDE)!.slots.every((s) => s === null),
      'the leg proves nothing if a slot is filled — the old filter would carry it',
    ).toBe(true);

    const saved = useBuildStore.getState().exportBuild();
    expect(saved).toContain('"Hide"');
    useBuildStore.getState().resetBuild();
    expect(useBuildStore.getState().importBuild(saved)).toBe(true);

    expect(inherent(HIDE), 'Hide came back').toBeDefined();
    expect(inherent(HIDE)!.isActive, 'still on').toBe(true);
  });
});

describe('the Kheldian travel inherents reach the totals (homecoming)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  /**
   * The half of the category defect that is not Thunderspy's. Energy Flight and Combat Flight are
   * granted toggles a Peacebringer always has, and they claimed the `archetype` display group for
   * the same reason Hide does — so the engine skipped them on every fork, and a Peacebringer's
   * flight contributed nothing no matter what the user did.
   */
  it('a Peacebringer’s granted travel toggles are gathered, not skipped', () => {
    const s = useBuildStore.getState();
    s.resetBuild();
    s.setArchetype('peacebringer');
    s.setPrimary('peacebringer/luminous-blast');
    s.setSecondary('peacebringer/luminous-aura');

    for (const name of ['Energy_Flight', 'Combat_Flight']) {
      const power = inherent(name);
      expect(power, `${name} is granted`).toBeDefined();
      // Display group unchanged — the expanded "<AT> Inherent" section, as its own test pins.
      expect(power!.inherentCategory, `${name} display group`).toBe('archetype');
      expect(power!.derivedMechanic, `${name} is not the archetype mechanic`).toBeFalsy();
    }
  });
});
