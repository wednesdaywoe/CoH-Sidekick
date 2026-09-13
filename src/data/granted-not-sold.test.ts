import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getAllPowersets, GRANTED_POWER_GROUPS } from '@/data';
import { isBuyablePick } from '@/data/power-requires';
import type { Power } from '@/types';

/**
 * ROSTER-2 — a power the game hands over is not a pick, and the export says which.
 *
 * Rebirth's Wind Control ships Clear Skies at level 31, granted once Vacuum and Vortex are
 * trained. The picker offered it anyway, so the set read as ten choices where the game sells
 * nine. Thunderspy sells the same set and Pale Blade's Fetid Presence the same way.
 *
 * The cause was where the answer was read from. `AutoIssue` is the grant, on the power, in the
 * export; this side was instead inferring it from `GRANTED_POWER_GROUPS`, a hand-written map of
 * which parent grants which child, which named neither power. The Rust planner had read the mark
 * directly all along (`pick_gate`, powers.rs) — the two planners disagreed and only one was
 * looking at the data.
 *
 * The gate that should have caught this was SHOWFLAGS-2's third leg, which asserts a full AT
 * powerset offers at least nine picks. It grades one direction. A set offering TEN passes it,
 * and Wind Control did, on two forks, for as long as it shipped. The other direction is not
 * writable as a count — Broad Sword really does carry ten members and sells nine of them by
 * making Slice and Boomerang Slice exclude one another — so these legs grade the axis instead.
 */

const DATASETS = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'] as const;

/**
 * Floors measured 2026-09-13, just under the real counts. A zero here means the leg stopped
 * asking rather than started passing — the SHOWFLAGS-1 lesson, and the reason both legs carry
 * one. Graded population is the AutoIssue powers at a real unlock level: the ones a picker
 * could plausibly offer, as opposed to the -1 sentinel rows nothing was ever going to show.
 */
const FLOORS: Record<(typeof DATASETS)[number], number> = {
  homecoming: 20, // 24 measured — Kheldian form attacks, Energy/Combat Flight, Shadow Step/Recall
  rebirth: 15, // 19 measured — the above, plus Clear Skies ×2 and Shadow Slipping
  thunderspy: 3, // 3 measured — Clear Skies ×2 and Fetid Presence
  brainstorm: 20, // 24 measured, the same as its parent fork
};

/**
 * Slottable names the hand-written map still carries, measured the same day. Rebirth's zero is
 * the real number and not a load failure: it grants nothing slottable, reaching its Nova attacks
 * by PowerRedirector instead, so the map was already contributing nothing to this fork's picker.
 * That is what made it worth checking whether the other three needed it either.
 */
const HAND_WRITTEN_FLOORS: Record<(typeof DATASETS)[number], number> = {
  homecoming: 20,
  rebirth: 0,
  thunderspy: 22,
  brainstorm: 20,
};

/** AutoIssue powers a picker could plausibly reach — i.e. not behind the auto-grant sentinel. */
function grantedAtPickableLevel(powers: Power[]): Power[] {
  return powers.filter(
    (p) => p.autoIssue && !(p.available < 0 || p.available >= 0x80000000),
  );
}

for (const ds of DATASETS) {
  describe(`granted powers are never sold — ${ds}`, () => {
    beforeAll(async () => {
      await loadDataset(ds);
    }, 120000);

    it('an AutoIssue power is never offered as a pick', () => {
      let graded = 0;
      const offered: string[] = [];
      for (const [id, ps] of Object.entries(getAllPowersets())) {
        for (const p of grantedAtPickableLevel(ps.powers)) {
          graded++;
          if (isBuyablePick(p)) offered.push(`${id}:${p.internalName}`);
        }
      }
      expect(offered).toEqual([]);
      expect(graded).toBeGreaterThanOrEqual(FLOORS[ds]);
    }, 120000);

    /**
     * The key on the retirement. Dropping `GRANTED_POWER_GROUPS` from the pick filter rested on
     * a measurement — it caught nothing `AutoIssue` does not, on all four forks — and that is
     * exactly the kind of claim that stops the next session looking. A dataset that adds a
     * slottable granted member the export does not mark breaks it here, by name, instead of
     * quietly putting the power back on sale.
     */
    it('the retired hand-written grant map claims nothing AutoIssue misses', () => {
      const handWritten = new Set<string>(
        Object.values(GRANTED_POWER_GROUPS)
          .filter((g) => g.slottable)
          .flatMap((g) => g.grantedPowers),
      );
      expect(handWritten.size).toBeGreaterThanOrEqual(HAND_WRITTEN_FLOORS[ds]);
      const unreached: string[] = [];
      for (const [id, ps] of Object.entries(getAllPowersets())) {
        for (const p of ps.powers) {
          if (!p.internalName || !handWritten.has(p.internalName)) continue;
          if (!isBuyablePick(p)) continue; // withheld on some other axis; not at issue
          unreached.push(`${id}:${p.internalName}`);
        }
      }
      expect(unreached).toEqual([]);
    }, 120000);
  });
}
