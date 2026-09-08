import { describe, it, expect } from 'vitest';
import { getTableValue } from '@/data/datasets/homecoming/at-tables';
import { baseAtoms, recoveryBuffValue, maxHPBuffValue, atomsOfType } from '@/data/core/atom-query';
import type { AtomicEffect } from '@/data/core/atomic-effect';
import { FlashArrow } from '@/data/datasets/homecoming/generated/powersets/defender/primary/trick-arrow/flash-arrow';
import { IceArrow } from '@/data/datasets/homecoming/generated/powersets/defender/primary/trick-arrow/ice-arrow';
import { PoisonGasArrow } from '@/data/datasets/homecoming/generated/powersets/defender/primary/trick-arrow/poison-gas-arrow';
import { DullPain } from '@/data/datasets/homecoming/generated/powersets/scrapper/secondary/regeneration/dull-pain';
import { Revive } from '@/data/datasets/homecoming/generated/powersets/brute/secondary/regeneration/revive';
import { EarthsEmbrace } from '@/data/datasets/homecoming/generated/powersets/brute/secondary/stone-armor/earths-embrace';
import { EMPArrow } from '@/data/datasets/homecoming/generated/powersets/defender/primary/trick-arrow/emp-arrow';
import { ThunderousBlast } from '@/data/datasets/homecoming/generated/powersets/sentinel/primary/electrical-blast/thunderous-blast';
import { Hibernate } from '@/data/datasets/homecoming/generated/powersets/tanker/primary/ice-armor/hibernate';

/**
 * Regression guards for the Trick Arrow effect-collapse fixes (2026-07-05),
 * restated atom-native (STRIP-1 — the generated powerset carries `atoms`, not
 * an `effects` bag, so every `p.effects.<slot>` read this file made no longer
 * exists).
 *
 * The bag-side claims map to atom shapes as follows:
 *   ① `collectTemplatesDeep` drops `enttype target> player eq` PvP groups: the
 *      PvP rows survive as GATED atoms (excluded from `baseAtoms`), so the PvE
 *      values stand.
 *   ③ `extractEffects` coalesces resistable + IgnoreResistance DEBUFF twins into
 *      one slot tagged `unresistable`: the wire atom carries the twin as
 *      `resistible:true` + `resistible:false` rows (`resistible` is first-class,
 *      `!IgnoreResistance`, atomic-effect.ts:151) — the `false` row IS the
 *      unresistable twin.
 *   ④ `getTableValue` aliases `Ranged_Debuff_Dam` → `ranged_debuff_dmg` — a data
 *      module, still live; unchanged.
 *   ② `addOrAccumulate` duration-aware DEBUFF split: a debuff applied twice at
 *      different durations (EMP -Regen 15s+45s, Thunderous Blast -Recovery
 *      10s+20s) is TWO separate atoms, each carrying its own duration — the
 *      "primary + variants" split is the baggage of a bag that had to fold them.
 *      Self-sustain BUFFS still sum (Hibernate's two +Recovery rows → 4 via
 *      `recoveryBuffValue`).
 *   ⑥ The IgnoreStrength/enhanceable +MaxHP twin: two MaxHP atoms at scale 1/1
 *      (Dull Pain), 1.2/1.2 (Revive), 2/2 (Earth's Embrace) — read through
 *      `maxHPBuffValue`/`{ignoreStrength:true}`.
 *   ⑤ EMP Field's buffs come from the pet's pseudo-pet ability tree
 *      (`summon.resolvedEntities`) — the strip left the summon subtree intact,
 *      so this reads `EMPArrow.summon` directly (it was `p.effects.summon`
 *      before, and `effects` is gone).
 */
describe('Trick Arrow effect-collapse fixes', () => {
  /** ToHit debuff atoms (base set only — gated PvP rows are excluded). */
  const tohitDebuffAtoms = (p: { name?: string }): AtomicEffect[] =>
    baseAtoms(p as never).filter((a) => a.effectType === 'ToHit');
  /** Damage-debuff rows — DamageBuff with a `*_Debuff_Dam*` table. */
  const damageDebuffAtoms = (p: { name?: string }): AtomicEffect[] =>
    baseAtoms(p as never).filter((a) =>
      a.effectType === 'DamageBuff' && (a.modifierTable ?? '').toLowerCase().includes('debuff_dam'));

  it('④ damage-debuff table name resolves (not half)', () => {
    expect(getTableValue('defender', 'Ranged_Debuff_Dam', 50)).toBeCloseTo(-0.125, 5);
    // Same value whether the power spells it _Dam or _Dmg.
    expect(getTableValue('defender', 'Ranged_Debuff_Dmg', 50)).toBeCloseTo(-0.125, 5);
  });

  it('① Flash Arrow shows PvE ToHit 0.75 / 60s, not PvP 0.5 / 20s', () => {
    const rows = tohitDebuffAtoms(FlashArrow);
    // Base (non-gated) rows are the PvE `critter` ones — scale 0.75, 60s.
    expect(rows.length).toBe(2);
    for (const a of rows) {
      expect(a.scale).toBe(0.75);
      expect(a.duration).toBe(60);
    }
    // The PvP rows (scale 0.5 / 20s) exist but are GATED (`player eq`), so they
    // never make it into the base set.
    const pvpRows = atomsOfType(FlashArrow as never, 'ToHit');
    expect(pvpRows.some((a) => a.scale === 0.5 && a.duration === 20)).toBe(true);
    expect(pvpRows.filter((a) => a.scale === 0.5).every((a) => a.gated)).toBe(true);
  });

  it('③ Flash Arrow ToHit is tagged unresistable (split debuff twin)', () => {
    // resistible:false = the IgnoreResistance twin (atomic-effect.ts:151).
    const rows = tohitDebuffAtoms(FlashArrow);
    expect(rows.some((a) => a.resistible === false)).toBe(true);
  });

  it('③④ Poison Gas -DMG is scale 2 (→25%) and tagged unresistable', () => {
    const rows = damageDebuffAtoms(PoisonGasArrow);
    expect(rows.length).toBeGreaterThan(0);
    for (const a of rows) {
      expect(a.scale).toBe(2); // 2 × -0.125 = -25%
      expect(a.duration).toBe(60);
      expect(a.ignoreStrength).toBe(true);
    }
    // The unresistable twin is present: both a resistible and an
    // IgnoreResistance (`resistible:false`) row exist per damage type.
    expect(rows.some((a) => a.resistible === false)).toBe(true);
    expect(rows.some((a) => a.resistible === true)).toBe(true);
    // Every damage type has both halves.
    const byType = new Map<string, AtomicEffect[]>();
    for (const a of rows) {
      const list = byType.get(a.subType ?? '') ?? [];
      list.push(a);
      byType.set(a.subType ?? '', list);
    }
    for (const [t, list] of byType) {
      expect(list.length, t).toBe(2);
    }
  });

  it('④① Ice Arrow -DMG is scale 1.6 (→20%), single (no twin), -Special 60s', () => {
    const rows = damageDebuffAtoms(IceArrow);
    expect(rows.length).toBeGreaterThan(0);
    for (const a of rows) {
      expect(a.scale).toBe(1.6); // 1.6 × -0.125 = -20%
      // No resistible twin — every damage-debuff row is unresistable already.
      expect(a.resistible).toBe(false);
      expect(a.ignoreStrength).toBe(true);
      expect(a.duration).toBe(60);
    }
    // The "special" debuff half (Enhancement debuffs on Ranged_Special, 60s) is
    // present alongside — Ice Arrow debuffs damage AND enhancement effectiveness.
    const special = baseAtoms(IceArrow as never).filter(
      (a) => a.effectType === 'Enhancement' && (a.modifierTable ?? '') === 'Ranged_Special');
    expect(special.length).toBeGreaterThan(0);
    expect(special.every((a) => a.duration === 60)).toBe(true);
  });

  it('③ stack=Stack +MaxHP twin SUMS across both halves (Dull Pain total 2)', () => {
    // Dull Pain's two scale-1 stack=Stack templates are distinct halves: one
    // enhanceable (maxHPBuff), one IgnoreStrength (maxHPBuffUnenhanced). Both
    // survive so the total stays 2 (+20%) while only the enhanceable half
    // responds to +Healing. (Previously both summed into one enhanceable slot,
    // over-enhancing the unenhanceable half.)
    const enhanceable = maxHPBuffValue(DullPain);
    const unenh = maxHPBuffValue(DullPain, { ignoreStrength: true });
    expect(enhanceable?.scale).toBe(1);
    expect(unenh?.scale).toBe(1);
    expect(enhanceable!.scale + unenh!.scale).toBe(2);
    // The resistible twin exists on the atom: the unresistable (IgnoreResistance)
    // half is the enhanceable-less row.
    const rows = baseAtoms(DullPain as never).filter((a) => a.effectType === 'MaxHP');
    expect(rows.some((a) => a.resistible === false)).toBe(true);
  });

  // ⑥ The IgnoreStrength/enhanceable +MaxHP twin ALWAYS sums both halves,
  // regardless of stack mode — verified in-game via Inexhaustible (stack=No)
  // showing two +66.93 Max-HP entries totalling 133.86. Each half lands in its
  // own slot: the enhanceable one in maxHPBuff, the IgnoreStrength one in
  // maxHPBuffUnenhanced. So Revive's two scale-1.2 halves and Earth's Embrace's
  // two scale-2 halves each keep BOTH (the earlier "Replace twins collapse to
  // one half" reading, based on a mis-derived 188.79-HP figure, was wrong).
  it('⑥ +MaxHP twins keep both halves (Revive 1.2+1.2, Earth\'s Embrace 2+2)', () => {
    const r1 = maxHPBuffValue(Revive);
    const r2 = maxHPBuffValue(Revive, { ignoreStrength: true });
    expect(r1?.scale).toBe(1.2);
    expect(r2?.scale).toBe(1.2);
    const e1 = maxHPBuffValue(EarthsEmbrace);
    const e2 = maxHPBuffValue(EarthsEmbrace, { ignoreStrength: true });
    expect(e1?.scale).toBe(2);
    expect(e2?.scale).toBe(2);
  });

  it('⑤ EMP Field surfaces the +Resistance buff and mez PROTECTION (not offense)', () => {
    const abilities =
      (EMPArrow.summon?.resolvedEntities ?? []).flatMap((re) => re.abilities ?? []);
    const types = new Set(abilities.flatMap((ab) => ab.effects ?? []).map((e) => e.type));
    // The +15% all-resistance buff was previously dropped entirely.
    expect(types.has('ResistanceBuff')).toBe(true);
    // Mez protection to allies must NOT be inverted into offensive control, and
    // EVERY mez attrib of a [Held, Stunned, Sleep] template must surface (not
    // just the first).
    expect(types.has('HoldProtection')).toBe(true);
    expect(types.has('StunProtection')).toBe(true);
    expect(types.has('SleepProtection')).toBe(true);
    expect(types.has('Hold')).toBe(false);
    expect(types.has('Immobilize')).toBe(false);
    // End-drain / recovery-debuff RESISTANCE, not an offensive -Recovery debuff.
    expect(types.has('RecoveryDebuffResist')).toBe(true);
    expect(types.has('RecoveryDebuff')).toBe(false);
  });

  it('② EMP -Regen splits into 45s primary + 15s variant (not summed -1000%)', () => {
    // Two separate -5 Regen atoms at 15s and 45s — the bag folded them into one
    // slot with a durationVariants array; the atoms themselves never summed.
    const regen = baseAtoms(EMPArrow as never).filter((a) => a.effectType === 'Regeneration');
    expect(regen.length).toBe(2);
    expect(regen.map((a) => a.duration).sort()).toEqual([15, 45]);
    expect(regen.every((a) => a.scale === -5)).toBe(true);
  });

  it('② Thunderous Blast -Recovery splits into 20s primary + 10s variant', () => {
    // Two separate -1 Recovery atoms at 10s and 20s.
    const rec = baseAtoms(ThunderousBlast as never).filter((a) => a.effectType === 'Recovery');
    expect(rec.length).toBe(2);
    expect(rec.map((a) => a.duration).sort()).toEqual([10, 20]);
    expect(rec.every((a) => a.scale === -1)).toBe(true);
  });

  it('② self-sustain BUFFS still sum, never split (Hibernate +Recovery stays 4)', () => {
    const rec = recoveryBuffValue(Hibernate);
    expect(rec?.scale).toBe(4); // two +2 Recovery rows sum
    expect(rec?.perTarget).toBeUndefined();
  });
});