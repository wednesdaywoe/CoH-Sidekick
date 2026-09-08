import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data';
import {
  baseAtoms, gatedAtoms, baseAtomsOfType, isDebuffAtom,
  resistanceBuffValue, resistanceSelfDebuffValue, maxHPBuffValue,
} from '@/data/core/atom-query';
import type { AtomicEffect, EffectType, Power } from '@/types';

/**
 * Bio Armor's "Evolving Armor" (internal `Adaptation`) grants defense only in
 * Defensive Adaptation and regen/recovery only in Efficient (`rested`), riding
 * `kDefensiveAdaptation Source.Mode?` / `kRestedAdaptation Source.Mode?` gated
 * groups in the binary. The converter's per-target pass once ignored that gate and
 * folded those scales into the always-on base, so the armor granted defense in
 * every stance while the matching conditional carried the bare per-foe increment.
 *
 * These claims read the atoms' own `gated` partition, not the retired base bag.
 * Each stance-gated axis is stated twice — its atoms EXIST and none of them is
 * base — because the one-sided form (`base.defenseBuff` is undefined) passed for
 * any reason at all once the bag went, including the axis vanishing outright.
 * The Defensive/Efficient conditionals below still read `conditionalEffects`,
 * which the strip did not reach; when it does, they move the same way.
 */

/** Which ATs carry each power, per dataset. The powers are absent on some ATs and
 *  the whole Bio Armor set on one, so an unstated skip would let a roster change
 *  pass as a silent no-op (`if (!power) return`, which is what these replaced). */
const ROSTER: Record<string, {
  sets: string[]; evolvingArmor: string[]; inexhaustible: string[]; offensive: string[];
  /** Where the Efficient stance's per-foe RECOVERY increment survives to the atoms.
   *  Everywhere but one, and the exception is a converter verdict, not missing data — see
   *  the `efficientRecoveryPerFoe` note at the assertion. */
  efficientRecoveryPerFoe: string[];
}> = {
  homecoming: {
    sets: ['scrapper', 'brute', 'tanker', 'stalker', 'sentinel'],
    // Stalker and Sentinel run Hide instead of the +Res/per-foe toggle.
    evolvingArmor: ['scrapper', 'brute', 'tanker'],
    inexhaustible: ['scrapper', 'brute', 'tanker', 'sentinel'],
    offensive: ['scrapper', 'brute', 'tanker', 'stalker', 'sentinel'],
    efficientRecoveryPerFoe: ['scrapper', 'brute', 'tanker'],
  },
  rebirth: {
    // Rebirth ships no Sentinel Bio Armor at all.
    sets: ['scrapper', 'brute', 'tanker', 'stalker'],
    evolvingArmor: ['scrapper', 'brute', 'tanker'],
    inexhaustible: ['scrapper', 'brute', 'tanker'],
    offensive: ['scrapper', 'brute', 'tanker', 'stalker'],
    efficientRecoveryPerFoe: ['scrapper', 'brute', 'tanker'],
  },
  thunderspy: {
    // No Sentinel Bio Armor, as Rebirth.
    sets: ['scrapper', 'brute', 'tanker', 'stalker'],
    // Stalker included, unlike the other three forks, and it is not a slip. Thunderspy's
    // Stalker armor is internally `Hide` and carries the stance system itself — 66 atoms,
    // all three `k*Adaptation` gate tokens, all three stance conditionals — where the other
    // forks give Stalker a plain Hide and no stances. The display-name lookup lands on it
    // for the same reason it exists: the internal name is the ambiguous one.
    evolvingArmor: ['scrapper', 'brute', 'tanker', 'stalker'],
    // Stalker's is internally `Boundless_Energy`, so the by-internal-name lookup misses it.
    inexhaustible: ['scrapper', 'brute', 'tanker'],
    // Empty, and measured rather than assumed: Thunderspy ships NO `Offensive_Adaptation`
    // power on any AT, nor the Defensive/Efficient switchers. Its stances exist only as
    // conditionals on the armor, so there is no power to carry the caster -Res claim.
    offensive: [],
    // Brute is the one hole in the corpus, and it is the export's shape rather than a loss:
    // Thunderspy joins the Rested per-foe pair onto ONE template there,
    // `attribs: ["Regeneration","Recovery"]` at scale 0.05, where its own Scrapper/Tanker/
    // Stalker and every other fork ship two. `classifyTemplateForStacking` returns on the
    // first resource attrib, so the patch speaks for `regenBuff` alone and
    // `_perTargetAttribs` deliberately withholds the stamp from the Recovery atom rather
    // than mark a slot nothing patched. Adjudicated in gaps/pipeline-provenance.md,
    // "A template is not an atom".
    efficientRecoveryPerFoe: ['scrapper', 'tanker', 'stalker'],
  },
  brainstorm: {
    sets: ['scrapper', 'brute', 'tanker', 'stalker', 'sentinel'],
    evolvingArmor: ['scrapper', 'brute', 'tanker'],
    inexhaustible: ['scrapper', 'brute', 'tanker', 'sentinel'],
    offensive: ['scrapper', 'brute', 'tanker', 'stalker', 'sentinel'],
    efficientRecoveryPerFoe: ['scrapper', 'brute', 'tanker'],
  },
};

const AT_SETS = ['scrapper', 'brute', 'tanker', 'stalker', 'sentinel'];

// Look up by DISPLAY name: the +Res/per-foe toggle is internally "Adaptation" on
// Scrapper/Brute/Tanker, but "Adaptation" is the STANCE SWITCHER on Stalker and
// Sentinel. The display name is unambiguous.
function evolvingArmor(setId: string): Power | undefined {
  return getPowerset(setId)?.powers.find((p) => p.name === 'Evolving Armor');
}

function byInternal(setId: string, internalName: string): Power | undefined {
  return getPowerset(setId)?.powers.find((p) => p.internalName === internalName);
}

function conditional(power: Power, id: string) {
  return (power.conditionalEffects ?? []).find((c) => c.id === id);
}

function countOfType(atoms: readonly AtomicEffect[], type: EffectType): number {
  return atoms.filter((a) => a.effectType === type).length;
}

/** The mode token each stance-gated axis is gated on. Matched on the token alone:
 *  Rebirth spells the pronoun `source.Mode?` where Homecoming spells it
 *  `Source.Mode?`, and the token is the part that names the stance. */
const GATED_AXES: readonly (readonly [EffectType, string])[] = [
  ['Defense', 'kDefensiveAdaptation'],
  ['Regeneration', 'kRestedAdaptation'],
  ['Recovery', 'kRestedAdaptation'],
];

for (const ds of ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'] as const) {
  const roster = ROSTER[ds];

  describe(`Evolving Armor stance-gating — ${ds}`, () => {
    beforeAll(async () => {
      await loadDataset(ds);
    });

    for (const at of AT_SETS) {
      const setId = `${at}/bio-armor`;

      it(`${setId}: the stance-gated axes are gated on the atoms; the base +Res is not`, () => {
        const power = evolvingArmor(setId);
        expect(!!power, `${setId} has Evolving Armor`).toBe(roster.evolvingArmor.includes(at));
        if (!power) return;

        const base = baseAtoms(power);
        const gated = gatedAtoms(power);
        for (const [type, token] of GATED_AXES) {
          const rows = gated.filter((a) => a.effectType === type);
          expect(rows.length, `${setId} gated ${type} atoms`).toBeGreaterThan(0);
          expect(countOfType(base, type), `${setId} base ${type} atoms`).toBe(0);
          for (const a of rows) {
            expect((a.requiresExpression ?? []).join(' '), `${setId} ${type} gate`).toContain(token);
          }
        }

        // Resistance is the axis that forks: an always-on half and a stance-gated
        // half on the same power, which is why "no base X" alone says nothing.
        expect(countOfType(base, 'Resistance'), `${setId} base Resistance atoms`).toBeGreaterThan(0);
        expect(countOfType(gated, 'Resistance'), `${setId} gated Resistance atoms`).toBeGreaterThan(0);
        const res = resistanceBuffValue(power);
        expect(res?.smashing?.scale, `${setId} base +Res(smashing)`).toBeCloseTo(0.55, 4);
        expect(res?.smashing?.perTarget, `${setId} base +Res(smashing) per foe`).toBeCloseTo(0.05, 4);
        expect(res?.psionic?.scale, `${setId} base +Res(psionic)`).toBeCloseTo(0.33, 4);
      });

      it(`${setId}: Defensive conditional carries the full per-foe defense`, () => {
        const power = evolvingArmor(setId);
        if (!power) return;
        const def = conditional(power, 'defensiveadaptation');
        expect(def, `${setId} defensiveadaptation conditional`).toBeTruthy();
        const smashing = (def!.effects as Record<string, any>)?.defenseBuff?.smashing;
        expect(smashing, `${setId} Defensive defenseBuff.smashing`).toBeTruthy();
        // Full value = base (Replace) + per-foe (Continuous), an order of magnitude
        // above the bare per-foe increment the old fold surfaced.
        expect(smashing.scale, `${setId} Defensive smashing scale`).toBeGreaterThan(0.3);
        expect(smashing.perTarget, `${setId} Defensive smashing perTarget`).toBeGreaterThan(0);
      });

      it(`${setId}: Efficient conditional carries per-foe regen/recovery`, () => {
        const power = evolvingArmor(setId);
        if (!power) return;
        const eff = conditional(power, 'restedadaptation');
        expect(eff, `${setId} restedadaptation conditional`).toBeTruthy();
        const e = eff!.effects as Record<string, any>;
        const regen = e?.regenBuffUnenhanced ?? e?.regenBuff;
        const recovery = e?.recoveryBuffUnenhanced ?? e?.recoveryBuff;
        expect(regen, `${setId} Efficient regen`).toBeTruthy();
        expect(recovery, `${setId} Efficient recovery`).toBeTruthy();
        expect(regen.perTarget, `${setId} Efficient regen perTarget`).toBeGreaterThan(0);
        // Stated both ways round rather than asserted only where it holds. A per-foe
        // increment the classifier declined to speak for must be ABSENT, not zero and not
        // some other number: `_perTargetAttribs` withholding the stamp is the correct
        // outcome, and an increment appearing here later would mean the stamp reached a
        // slot nothing patched — the bug that narrowing was added to fix. See the roster's
        // thunderspy row and gaps/pipeline-provenance.md, "A template is not an atom".
        if (roster.efficientRecoveryPerFoe.includes(at)) {
          expect(recovery.perTarget, `${setId} Efficient recovery perTarget`).toBeGreaterThan(0);
        } else {
          expect(recovery.perTarget, `${setId} Efficient recovery perTarget withheld`)
            .toBeUndefined();
          expect(recovery.scale, `${setId} Efficient recovery still carries its base`)
            .toBeGreaterThan(0);
        }
      });

      // Inexhaustible's +MaxHP is a co-applied enhanceable/unenhanceable twin
      // ("half of this max-HP increase is unenhanceable"). Verified in-game: the
      // attribute monitor shows two +66.93 entries totalling 133.86. On the atoms
      // the twin is one `ignoreStrength` boolean, which is the axis the bag minted
      // a second slot for, so both halves are asked separately here too.
      it(`${setId}: Inexhaustible keeps BOTH halves of the +MaxHP twin`, () => {
        const inex = byInternal(setId, 'Inexhaustible');
        expect(!!inex, `${setId} has Inexhaustible`).toBe(roster.inexhaustible.includes(at));
        if (!inex) return;
        const enhanceable = maxHPBuffValue(inex);
        const unenhanceable = maxHPBuffValue(inex, { ignoreStrength: true });
        expect(enhanceable?.scale, `${setId} Inexhaustible enhanceable maxHP`).toBeGreaterThan(0);
        expect(unenhanceable?.scale, `${setId} Inexhaustible unenhanceable maxHP`).toBeGreaterThan(0);
        expect(unenhanceable!.scale, `${setId} Inexhaustible halves are equal`)
          .toBeCloseTo(enhanceable!.scale, 4);
        expect(unenhanceable!.table).toBe(enhanceable!.table);
      });

      // Offensive Adaptation's -7.5% Res(all) lands on the CASTER. The wire says so
      // and the reader must agree, so both are asked: `resistanceSelfDebuffValue`
      // stamps `toWho:'Self'` as a literal on everything it returns, so asserting
      // that alone would grade the reader against itself. The atoms' own `toWho` is
      // the independent half.
      it(`${setId}: Offensive Adaptation's -Res is stated, and read, as a self penalty`, () => {
        const off = byInternal(setId, 'Offensive_Adaptation');
        expect(!!off, `${setId} has Offensive Adaptation`).toBe(roster.offensive.includes(at));
        if (!off) return;

        const wire = baseAtomsOfType(off, 'Resistance').filter(isDebuffAtom);
        expect(wire.length, `${setId} Offensive -Res atoms`).toBe(8);
        for (const a of wire) {
          expect(a.toWho, `${setId} Offensive -Res(${a.subType}) recipient`).toBe('Self');
          expect(a.scale, `${setId} Offensive -Res(${a.subType}) scale`).toBeCloseTo(-0.075, 4);
        }

        const rd = resistanceSelfDebuffValue(off);
        expect(Object.keys(rd ?? {}).sort(), `${setId} Offensive self -Res types`).toEqual(
          ['cold', 'energy', 'fire', 'lethal', 'negative', 'psionic', 'smashing', 'toxic'],
        );
        expect(rd!.smashing.scale, `${setId} Offensive self -Res magnitude`).toBeCloseTo(0.075, 4);
        expect(rd!.smashing.toWho).toBe('Self');
      });
    }
  });
}
