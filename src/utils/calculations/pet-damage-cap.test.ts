import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getArchetype } from '@/data/archetypes';
import { getPetEntity } from '@/data/pet-entities';
import { calculatePetDamage, calculateResolvedPseudoPetDamage } from './pet-damage';
import { getPetClassAttribs } from './pet-stats';
import { Burn as BruteBurn } from '@/data/datasets/homecoming/generated/powersets/brute/secondary/fiery-aura/burn';

/**
 * A summon's damage strength: one additive multiplier, capped.
 *
 * Reported from beta 2026-08-16 — a Brute's Burn read 1,171,602 damage on the
 * Damage block once team buffs pushed damage strength into the thousands of
 * percent, while every power the player cast directly stopped at the archetype
 * ceiling. Two defects in one expression, and each hides in a range the other
 * one doesn't:
 *
 *  1. NO CAP. `damagePerHitFinal` was `damagePerHit × (1 + enh) × (1 + buffs)`
 *     with nothing clamping it. Invisible at ordinary buff levels, unbounded
 *     above the cap — which is precisely where a planner is being asked a
 *     question worth answering.
 *  2. MULTIPLICATIVE STRENGTH. Enhancement and buffs are ONE additive damage
 *     strength (`1 + enh + buffs`), which is what the player-power path folds
 *     and what the atoms' `Abs` aspect means. Multiplying them overstates every
 *     buffed summon by `enh × buffs` — 13% at a realistic 95%/30%, and never
 *     loud enough to look like a bug.
 *
 * Both halves are graded below, and both are graded IN VIOLATION: a strength
 * over the cap, and a strength where additive and multiplicative differ. An
 * under-cap-only corpus cannot see a missing clamp, and an enh-only or
 * buff-only corpus cannot see a wrong fold — either one is zero in the cross
 * term.
 *
 * Which ceiling applies is a per-path question, and the two paths answer it
 * from different rows:
 *  - `calculatePetDamage` — a real entity with a class row of its own. A summon
 *    is a second character (COH-DATA-MODEL §6), so its own class's cap binds.
 *  - `calculateResolvedPseudoPetDamage` — the synthesized location shells
 *    (Burn's flames, the rains, Storm Cell) have no class row and resolve their
 *    damage against the SUMMONER's AT table, so the summoner's cap binds.
 *
 * Caps are read from the dataset here, never written down: they are the
 * export's to state, and a hardcoded 4 would still pass after a rebalance that
 * moved them.
 */
describe('summoned damage stops at the damage-strength cap', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  }, 180000);

  /** The buff level in the beta report — silly on purpose, and the point. */
  const ABSURD_BUFF = 100; // +10,000%

  describe('synthesized pseudo-pet (Burn) — the summoner\'s cap', () => {
    // `summon` is top-level since the writer-side strip lifted it out of the bag (BPORT7).
    const flames = BruteBurn.summon!.resolvedEntities![0];
    // Lazily — the dataset is not loaded while the describe body runs.
    const bruteCap = () => getArchetype('brute')!.stats.damageCap!;

    it('the fixture is the real thing: a copyCreatorMods shell with damage', () => {
      expect(flames.copyCreatorMods).toBe(true);
      expect(flames.abilities.some(a => a.damage.length > 0)).toBe(true);
      expect(bruteCap()).toBeGreaterThan(1);
    });

    it('clamps Final to base × cap once strength exceeds it', () => {
      const cap = bruteCap();
      const r = calculateResolvedPseudoPetDamage(flames, 'brute', 50, 0.95, true, ABSURD_BUFF)!;
      expect(r.capped).toBe(true);
      for (const ab of r.abilities) {
        expect(ab.damagePerHitFinal).toBeCloseTo(ab.damagePerHit * cap, 6);
        // The pre-fix number, for the record: it was ~7,300× the capped one.
        expect(ab.damagePerHitFinal).toBeLessThan(ab.damagePerHit * (1.95 * (1 + ABSURD_BUFF)));
      }
      expect(r.totalDpsFinal).toBeCloseTo(r.totalDpsBase * cap, 6);
    });

    it('folds enhancement and buffs additively below the cap', () => {
      const r = calculateResolvedPseudoPetDamage(flames, 'brute', 50, 0.95, true, 0.30)!;
      expect(r.capped).toBe(false);
      const ab = r.abilities[0];
      expect(ab.damagePerHitFinal).toBeCloseTo(ab.damagePerHit * (1 + 0.95 + 0.30), 6);
      // Not (1 + enh) × (1 + buffs) — that reads 2.535 against the true 2.25.
      expect(ab.damagePerHitFinal).not.toBeCloseTo(ab.damagePerHit * 1.95 * 1.30, 3);
      // Enhanced is the slotting-alone tier and stays uncapped and buff-free.
      expect(ab.damagePerHitEnhanced).toBeCloseTo(ab.damagePerHit * 1.95, 6);
    });

    it('leaves enhancement out of the strength when mods are not copied', () => {
      const r = calculateResolvedPseudoPetDamage(flames, 'brute', 50, 0.95, false, 0.30)!;
      const ab = r.abilities[0];
      expect(ab.damagePerHitEnhanced).toBeCloseTo(ab.damagePerHit, 6);
      expect(ab.damagePerHitFinal).toBeCloseTo(ab.damagePerHit * 1.30, 6);
    });

    it('caps the per-type breakdown by the same multiplier as the total', () => {
      const r = calculateResolvedPseudoPetDamage(flames, 'brute', 50, 0.95, true, ABSURD_BUFF)!;
      for (const ab of r.abilities) {
        const summed = ab.damageByType.reduce((s, d) => s + d.final, 0);
        expect(summed).toBeCloseTo(ab.damagePerHitFinal, 6);
      }
    });

    it('reads the cap of whichever archetype summoned it', () => {
      const forEach = (['brute', 'blaster', 'defender'] as const).map(at => {
        const c = getArchetype(at)!.stats.damageCap!;
        const r = calculateResolvedPseudoPetDamage(flames, at, 50, 0, false, ABSURD_BUFF)!;
        return r.abilities[0].damagePerHitFinal / r.abilities[0].damagePerHit / c;
      });
      // Each lands on ITS archetype's cap — the ratio is 1 for all three even
      // though the caps themselves differ.
      for (const ratio of forEach) expect(ratio).toBeCloseTo(1, 6);
    });
  });

  describe('entity-backed pet — its own class row\'s cap', () => {
    const ENTITY = 'MastermindPets_Soldier';

    it('clamps to the PET class cap, not the summoner archetype\'s', () => {
      const entity = getPetEntity(ENTITY)!;
      const petCap = getPetClassAttribs(entity.characterClass)!.damageCap!;
      const r = calculatePetDamage(ENTITY, 50, 1, undefined, 0.95, true, ABSURD_BUFF, [])!;
      expect(r.capped).toBe(true);
      for (const ab of r.abilities) {
        expect(ab.damagePerHitFinal).toBeCloseTo(ab.damagePerHit * petCap, 6);
      }
      expect(r.aggregateDpsFinal).toBeCloseTo(r.aggregateDpsBase * petCap, 6);
    });

    it('folds additively and does not flag a cap below it', () => {
      const r = calculatePetDamage(ENTITY, 50, 1, undefined, 0.95, true, 0.30, [])!;
      expect(r.capped).toBe(false);
      const ab = r.abilities[0];
      expect(ab.damagePerHitFinal).toBeCloseTo(ab.damagePerHit * 2.25, 6);
      expect(ab.damagePerHitFinal).not.toBeCloseTo(ab.damagePerHit * 1.95 * 1.30, 3);
    });
  });
});
