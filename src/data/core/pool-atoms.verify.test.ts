/**
 * Plan B — regression guard for the pool / epic-pool atom gap (found 2026-07-15).
 *
 * Pool and epic-pool powers are built by two converters separate from
 * convert-powerset.cjs (`convert-pool-powers.cjs`, `convert-epic-pools.cjs`). Neither
 * emitted `power.atoms`, so ~1,358 powers across the three datasets — Health, Stamina,
 * Tough, Weave, Maneuvers, Assault and the whole epic/patron tier — had no atom
 * representation at all. Every atom-native applier silently fell back to the bag for
 * them: behavior-preserving, and therefore invisible.
 *
 * It stayed invisible because all seven `planb-shadow-*` gates swept only
 * `generated/powersets`, so their "corpus-wide, 0 divergences" claim was structurally
 * silent about ~15% of the corpus. Mutation-testing the gates could not have caught it —
 * every mutant still passes on a corpus that excludes the affected powers. The sweep is
 * now shared (`scripts/planb-shadow-sweep.cjs`) and walks `generated/` whole.
 *
 * This pins the two things that must not regress:
 *   1. pool/epic powers carry atoms, and the atom-native helpers reconstruct them;
 *   2. epic Soul Drain's per-foe scaling, a REAL user-facing bug this uncovered.
 *
 * **BPORT13.** The redirect block below asked its question of `power.effects.damage` and the
 * Cross Punch case of `power.effects.tohitBuff*`. Neither was a bag question in substance —
 * damage is a top-level field on the power object (`power.damage`, the same shape the bag key
 * mirrored) and the ToHit halves have had atom readers since Plan B. So these are re-reads,
 * the same lift `effects.summon` → `power.summon` took in BPORT12, not restatements: every
 * value asserted below is unchanged. Teleport is the one that moves, because `effects.teleport`
 * had no top-level twin; it is restated on the `Mez/Teleport` atom that put the key there.
 */
import { describe, it, expect } from 'vitest';
import { regenBuffValue, recoveryBuffValue, toHitBuffValue, damageBuffValue, atomsOf } from '@/data/core/atom-query';
import { POWER_POOLS_RAW } from '@/data/datasets/homecoming/generated/power-pools';
import { EPIC_POOLS_RAW } from '@/data/datasets/homecoming/generated/epic-pools';

/** Every Power-shaped object in a generated pool tree (they nest inside pool objects). */
function findPower(root: unknown, name: string): any {
  const seen = new Set<unknown>();
  const walk = (node: any): any => {
    if (!node || typeof node !== 'object' || seen.has(node)) return undefined;
    seen.add(node);
    if (Array.isArray(node)) {
      for (const v of node) { const hit = walk(v); if (hit) return hit; }
      return undefined;
    }
    if (node.name === name && (node.effects || node.atoms)) return node;
    for (const v of Object.values(node)) { const hit = walk(v); if (hit) return hit; }
    return undefined;
  };
  return walk(root);
}

describe('pool powers carry atoms (the Phase 3 prerequisite)', () => {
  it('Health reconstructs its +Regen from atoms, not just the bag', () => {
    const health = findPower(POWER_POOLS_RAW, 'Health');
    expect(health).toBeDefined();
    expect(atomsOf(health).length).toBeGreaterThan(0);
    const r = regenBuffValue(health)!;
    expect(r).toBeDefined();
    expect(r.scale).toBeCloseTo(0.4);
    expect(r.table).toBe('Melee_Ones');
  });

  it('Stamina reconstructs its +Recovery from atoms', () => {
    const stamina = findPower(POWER_POOLS_RAW, 'Stamina');
    expect(atomsOf(stamina).length).toBeGreaterThan(0);
    const r = recoveryBuffValue(stamina)!;
    expect(r.scale).toBeCloseTo(0.25);
  });

  it('Cross Punch keeps its Fighting Synergy buff on the IgnoreStrength half only', () => {
    // Its +5% ToHit is `IgnoreStrength`, granted per FIGHTING POOL POWER OWNED
    // (Boxing/Kick) — not per foe. Running the AoE per-target detector over pool powers
    // (tried for pipeline symmetry, reverted) read its Self-targeted Stack template in a
    // 5-target Cone as a per-foe increment: it minted a bogus perTarget AND a `tohitBuff`
    // slot duplicating the existing `tohitBuffUnenhanced`, double-counting the buff.
    const cp = findPower(POWER_POOLS_RAW, 'Cross Punch');
    expect(toHitBuffValue(cp)).toBeUndefined(); // enhanceable half: nothing
    const unenh = toHitBuffValue(cp, { ignoreStrength: true })!;
    expect(unenh.scale).toBeCloseTo(0.05);
    // The `perTarget` half is the whole point: a bogus per-foe stamp here would read the
    // Self-targeted Stack template in a 5-target Cone as an increment and scale the buff by
    // however many foes were hit. Zero, not absent — an undefined would also pass a `?? 0`.
    expect(unenh.perTarget ?? 0).toBe(0);
  });
});

describe('epic-pool Soul Drain scales per foe (a real bug the widened sweep found)', () => {
  // The epic-pool converter never ran detectStackingEffects, so the entire epic/patron
  // tier shipped its AoE self-buffs FLAT. Epic Soul Drain was {scale:1}/{scale:4} while
  // the identical Dark Melee primary power is {1.2, +0.2/foe}/{4.8, +0.8/foe} — so every
  // Blaster/Controller/Corruptor taking Soul Drain via an epic pool saw +1.0 ToHit and
  // +4.0 damage instead of +2.6 / +10.4 at 8 foes.
  const soulDrain = () => findPower(EPIC_POOLS_RAW, 'Soul Drain');

  it('matches its Dark Melee powerset twin on ToHit', () => {
    const t = toHitBuffValue(soulDrain())!;
    expect(t.scale).toBeCloseTo(1.2);
    expect(t.perTarget).toBeCloseTo(0.2);
  });

  it('matches its powerset twin on Damage', () => {
    const d = damageBuffValue(soulDrain())!;
    expect(d.scale).toBeCloseTo(4.8);
    expect(d.perTarget).toBeCloseTo(0.8);
  });

  it('scales to 8 foes at the calc formula rather than staying flat', () => {
    const t = toHitBuffValue(soulDrain())!;
    const d = damageBuffValue(soulDrain())!;
    expect(t.scale + (t.perTarget ?? 0) * 7).toBeCloseTo(2.6); // was 1.0
    expect(d.scale + (d.perTarget ?? 0) * 7).toBeCloseTo(10.4); // was 4.0
  });
});

describe('redirect-only pool/epic powers resolve their redirect chain', () => {
  // The pool converters collected `collectTemplatesDeep(rawJson.effects)` behind an
  // `if (rawJson.effects?.length)` guard and never followed redirects, so a power whose
  // own `effects` is empty produced an empty bag. Ten powers were affected; the six epic
  // snipes had NO DAMAGE IN THE PLANNER AT ALL. Their powerset twins are the same
  // redirect-only shape and always worked, purely because convert-powerset.cjs converts
  // them — which is what makes this a converter-divergence bug rather than a data quirk.
  // Fixed 2026-07-15 by sharing `collectBaseTemplates` across all three converters.

  it.each([
    ['Psionic Lance', 'Psionic', 3.56],
    ['Frozen Spear', 'Cold', 3.56],
    ['Mace Beam', 'Energy', 3.56],
    ['Zapp', 'Energy', 3.56],
    ['Moonbeam', 'Negative', 4.5],
  ])('%s deals %s damage (was: none at all)', (name, type, scale) => {
    const p = findPower(EPIC_POOLS_RAW, name as string);
    const d = p.damage;
    expect(d, `${name} has no damage`).toBeDefined();
    expect(d.type).toBe(type);
    expect(d.scale).toBeCloseTo(scale as number);
  });

  it('LRM Rocket deals its Smashing + Lethal split', () => {
    const d = findPower(EPIC_POOLS_RAW, 'LRM Rocket').damage;
    expect(d.map((x: any) => x.type)).toEqual(['Smashing', 'Lethal']);
    expect(d[0].scale).toBeCloseTo(1);
    expect(d[1].scale).toBeCloseTo(1.49);
  });

  it('drops the chance-0 Fiery Embrace bonus rather than shipping it as base damage', () => {
    // Every snipe redirect carries an FE bonus template (Fire_Dmg ~1.6 on Melee_Damage,
    // chance 0.0 — the engine flips it to 1 only while Fiery Embrace is up). It must not
    // land as unconditional damage on a Cold/Energy/Negative attack.
    for (const n of ['Frozen Spear', 'Zapp', 'Moonbeam', 'Mace Beam']) {
      const d = findPower(EPIC_POOLS_RAW, n).damage;
      const types = (Array.isArray(d) ? d : [d]).map((x: any) => x.type);
      expect(types, `${n} shipped the FE bonus`).not.toContain('Fire');
    }
  });

  it('Aid Other resolves its heal through the redirect, matching its powerset twin', () => {
    // Empathy's Heal Other is the oracle: same 1.96 / Ranged_Heal.
    const d = findPower(POWER_POOLS_RAW, 'Aid Other').damage;
    expect(d.type).toBe('Heal');
    expect(d.scale).toBeCloseTo(1.96);
    expect(d.table).toBe('Ranged_Heal');
  });

  it('Teleport and Teleport Target recover their effects', () => {
    // `effects.teleport` was the only claim here with no top-level twin, so it is restated on
    // the atom that minted the key: a `Mez` row with the `Teleport` sub-type. Both powers are
    // redirect-only, which is what made them the bug — an unresolved chain leaves the atom list
    // empty and this reads as a power that does nothing.
    for (const n of ['Teleport', 'Teleport Target']) {
      const p = findPower(POWER_POOLS_RAW, n);
      const atoms = atomsOf(p);
      expect(atoms.length, `${n} resolved no redirect`).toBeGreaterThan(0);
      expect(
        atoms.some((a) => a.effectType === 'Mez' && a.subType === 'Teleport'),
        `${n} has no teleport row`,
      ).toBe(true);
    }
  });
});

describe('all-gated powers still get atoms (the atom emit is not bag-gated)', () => {
  // The atom emit sat inside `if (allTemplates.length > 0)` — the BAG's view. A power
  // whose every effect group is gated has zero base templates but a full gated set, so
  // it got no atoms at all: the 16 Mastermind upgrade powers, Heat Loss, Victory Rush.
  // Guarding on the atom set instead is what fixes it; every such atom is `gated`, so
  // `baseAtoms` stays empty and no applier changes behavior.
  it('Victory Rush has atoms even though its bag sees nothing', () => {
    const vr = findPower(POWER_POOLS_RAW, 'Victory Rush');
    expect(atomsOf(vr).length).toBeGreaterThan(0);
    expect(atomsOf(vr).every((a) => a.gated), 'a base atom leaked').toBe(true);
  });
});
