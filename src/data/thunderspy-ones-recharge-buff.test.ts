import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { atomsOf } from '@/data/core/atom-query';
import { getPowerPool } from '@/data';
import { getRechargeBounds } from '@/data/at-tables';
import { isPermaEligible } from '@/utils/calculations/perma';
import { SpeedBoost } from './datasets/thunderspy/generated/powersets/controller/secondary/kinetics/speed-boost';
import { SiphonSpeed } from './datasets/thunderspy/generated/powersets/controller/secondary/kinetics/siphon-speed';
import { Absorption } from './datasets/thunderspy/generated/powersets/warshade/epic/umbral-aura/absorption';
import { GrantCover } from './datasets/thunderspy/generated/powersets/tanker/primary/shield-defense/grant-cover';
// Two DIFFERENT powers share the internal name Touch_of_Fear: the Blaster Darkness
// Manipulation "Touch of the Beyond" (advertises "Self +Regeneration") and the Dark
// Melee fear attack (no self-buff). The guard must keep the former, drop the latter.
import { TouchofFear as TouchOfTheBeyond } from './datasets/thunderspy/generated/powersets/blaster/secondary/darkness-manipulation/touch-of-fear';
import { TouchofFear as DarkMeleeTouchOfFear } from './datasets/thunderspy/generated/powersets/brute/primary/dark-melee/touch-of-fear';
import { DisruptingTorrent } from './datasets/thunderspy/generated/powersets/dominator/secondary/kinetic-assault/disrupting-torrent';
import { EquipRobot } from './datasets/thunderspy/generated/powersets/mastermind/primary/robotics/equip-robot';
import { Repair } from './datasets/thunderspy/generated/powersets/mastermind/primary/robotics/repair';
import { FortifyPack } from './datasets/thunderspy/generated/powersets/mastermind/primary/beast-mastery/fortify-pack';
import { RallyTheMilitia } from './datasets/thunderspy/generated/powersets/mastermind/primary/knights/rally-the-militia';

/**
 * Thunderspy `Ones`-attrib buff recovery — the DATA-DRIVEN fix.
 *
 * Thunderspy's older AttribMod schema stores a front string-attrib (the
 * *enhancement aspect* — here the catch-all `Ones` on the `*_Ones` unit tables)
 * plus a separate post-`requires` INDEX array naming the *affected* stat. The
 * parser historically read only the front, so every `Ones`-based recharge /
 * recovery / regen / endurance buff was unclassifiable and dropped — e.g. Hasten
 * had no `rechargeBuff` (no +recharge) and no `buffDuration` (no perma "Track").
 *
 * The fix (`_parse_effect_template_thunderspy` + `ATTRIB_NAME_THUNDERSPY`) decodes
 * the index array — with Thunderspy's RechargeTime at index 89 (HC: 90) — and, for
 * a lone `['Ones']` front, relabels to the real stat when it is one of the
 * high-confidence resource/recharge attribs (recharge/recovery/regen/endurance).
 * Sign alone then routes buff vs debuff. This replaced the earlier shortHelp-driven
 * `recoverThunderspyOnesBuffs` converter workaround (which only reached 3 Self
 * powers) with the actual binary datum — now covering ally buffs and debuffs too.
 *
 * These tests re-read the recovered shape from the committed dataset so a future
 * regen can't silently undo it (GAME-DATA-PRINCIPLES §9). The claims the strip
 * retired with the `effects` bag are now stated on the ATOMS the binary emits —
 * the surfacing under test is a data fact, not an applier routing, so the atoms
 * are the right and the only self-consistent oracle for it. The pool powers
 * (`Hasten`, `Burnout`) projected `effects` until the writer-side strip took the last two
 * emitters; their surfacing claims moved to the atoms with everything else, and the two
 * perma-eligibility claims the bag seam was carrying came back with PERMA-2's atom port.
 */
/** The first always-on atom of `effectType`, optionally narrowed by a predicate. */
function atomOf(
  // `atoms` is OPTIONAL on the beta's `Power`, required on canonical's — a plain
  // `Parameters<typeof atomsOf>[0]` keeps the helper assignable from either.
  power: Parameters<typeof atomsOf>[0],
  effectType: string,
  pred: (a: ReturnType<typeof atomsOf>[number]) => boolean = () => true,
) {
  return atomsOf(power).find((a) => a.effectType === effectType && pred(a));
}

describe('Thunderspy Ones-attrib buff recovery (data-driven)', () => {
  beforeAll(async () => {
    await loadDataset('thunderspy');
  });

  it('Hasten recovers its +70% recharge buff and 120s duration from the binary', () => {
    const hasten = getPowerPool('speed')?.powers.find((p) => p.internalName === 'Hasten');
    expect(hasten).toBeDefined();
    // Was `hasten.effects.rechargeBuff` / `.buffDuration`. The pool bag went with the
    // writer-side strip, so the recovered datum is stated where the binary puts it — the same
    // move the rest of this file already made, and the reason its header carved the pool powers
    // out as "still project `effects` today". They do not.
    const buff = atomOf(hasten!, 'RechargeTime', (a) => a.aspect === 'Str');
    expect(buff).toBeDefined();
    expect(buff!.scale).toBeCloseTo(0.7, 5);
    expect(buff!.modifierTable).toBe('Melee_Ones');
    expect(buff!.ignoreStrength).toBe(true);
    expect(buff!.duration).toBe(120);
  });

  // Given back by PERMA-2 (2026-09-04): the predicate reads the atoms, so the 120s window the
  // test above recovers is the one it measures — 120s recharge against a 120s caster-side
  // window, which is what makes Hasten the archetypal Track button.
  it('Hasten is perma-eligible (the Track button appears)', () => {
    const hasten = getPowerPool('speed')?.powers.find((p) => p.internalName === 'Hasten');
    expect(isPermaEligible(hasten!)).toBe(true);
  });

  it('Speed Boost recovers BOTH +recharge and +recovery (multi-stat ally buff the shortHelp hack could not)', () => {
    const recharge = atomOf(SpeedBoost, 'RechargeTime', (a) => a.aspect === 'Str');
    expect(recharge).toBeDefined();
    expect(recharge!.scale).toBeCloseTo(0.5, 5);
    expect(recharge!.modifierTable).toBe('Melee_Ones');
    expect(recharge!.ignoreStrength).toBe(true);
    // Recovery halves: 0.5 (Target face) + 0.25 (Self face) — Thunderspy authors a
    // SECOND, smaller tier of the same stats in its own ungated effect group; nothing
    // in the data separates the two, so the resource slot's documented same-table SUM
    // applies (HC and Rebirth carry only the 0.5 tier — a fork rebalance, not a
    // misread). The atoms carry both, and both ride one 240s duration.
    const recovery = SpeedBoost && atomsOf(SpeedBoost).filter((a) => a.effectType === 'Recovery' && a.aspect === 'Cur');
    expect(recovery!.length).toBe(2);
    expect(recovery!.reduce((s, a) => s + a.scale, 0)).toBeCloseTo(0.75, 5);
    expect(recovery!.every((a) => a.modifierTable === 'Melee_Ones')).toBe(true);
    expect(atomOf(SpeedBoost, 'Recovery')!.duration).toBe(240);
  });

  it('Siphon Speed routes its negative-scale Ones template to a recharge DEBUFF', () => {
    // -0.2 RechargeTime to a Foe is the debuff arm; the +0.2 Self twin is the steal's
    // self-buff, the same +0.2 read HC and Rebirth both carry. Sign discriminates the
    // two arms on the one recovered attrib.
    const debuff = atomOf(SiphonSpeed, 'RechargeTime', (a) => a.scale < 0);
    expect(debuff).toBeDefined();
    expect(debuff!.scale).toBeCloseTo(-0.2, 5);
    expect(debuff!.modifierTable).toBe('Melee_Ones');
    expect(debuff!.ignoreStrength).toBe(true);
    const buff = atomOf(SiphonSpeed, 'RechargeTime', (a) => a.scale > 0);
    expect(buff).toBeDefined();
    expect(buff!.scale).toBeCloseTo(0.2, 5);
    expect(buff!.ignoreStrength).toBe(true);
  });

  it('Burnout (instant power-reset, not a +recharge buff) stays ineligible', () => {
    const burnout = getPowerPool('speed')?.powers.find((p) => p.internalName === 'Burnout');
    expect(burnout).toBeDefined();
    // The 60s the bag published as `buffDuration` is the crash window — a `MaxEndurance -25`
    // on the caster — and not a buff at all, which is the whole point of the ineligibility.
    // Stated on the atom now that the pool bag is gone.
    const crash = atomOf(burnout!, 'MaxEndurance', (a) => a.aspect === 'Max');
    expect(crash).toBeDefined();
    expect(crash!.duration).toBe(60);
    // The beta's `isPermaEligible` is a declared fork of canonical's (PERMA-2, this branch):
    // it takes optional `RechargeBounds` and a NO-BOUNDS call deliberately reads eligible, so
    // canonical's bare `toBe(false)` grades the fork rather than the claim. The claim itself —
    // an instant power-reset is not something the caster keeps up — is the bounded call, which
    // is also the only call the panel makes: `recharge / cap > duration` vetoes the 60s crash.
    expect(isPermaEligible(burnout!, getRechargeBounds('scrapper'))).toBe(false);
  });

  // --- Disambiguation vetoes (guardThunderspyOnesBuffs) ---------------------
  // Thunderspy drops the AttribMod aspect AND per-template target, so an index-89
  // RechargeTime template can be a real +recharge buff OR a resistance-to-slow, and
  // a positive resource template on a foe attack looks like a caster self-buff. The
  // discriminator that holds the slot apart is the ATOM's face, not the name.

  it('Absorption (Kheldian +Res passive) does NOT gain a phantom +recharge buff (aspect-trap)', () => {
    // Its RechargeTime Ones template is resistance-to-slow with the aspect dropped
    // (aspect Res, the resistance face) — NOT the Cur/Str arm a +recharge buff rides.
    expect(atomOf(Absorption, 'RechargeTime', (a) => a.aspect === 'Str' || a.aspect === 'Cur')).toBeUndefined();
    // The +Res (Energy/Negative) is surfaced from the `Res_DMG`-front index array
    // (tspy-resist-tohit-vocab, 2026-07-09) — byte-identical to HC's own Absorption.
    const res = atomsOf(Absorption).filter((a) => a.effectType === 'Resistance' && a.aspect === 'Res');
    expect(res.map((a) => a.subType).sort()).toEqual(['Energy', 'Negative']);
    expect(res[0].modifierTable).toMatch(/res_dmg/i);
  });

  it('Grant Cover keeps its defense but not a phantom +recharge (its recharge is +RES(Recharge Debuff))', () => {
    // The RechargeTime template is the resistance face again (aspect Res), never a
    // +recharge buff arm.
    expect(atomOf(GrantCover, 'RechargeTime', (a) => a.aspect === 'Str' || a.aspect === 'Cur')).toBeUndefined();
    // Defense rows exist, aimed at the team (toWho Target) — Grant Cover gives defense
    // even though it does not buff the caster's own running total.
    expect(atomsOf(GrantCover).some((a) => a.effectType === 'Defense' && a.aspect === 'Cur')).toBe(true);
  });

  it('Disrupting Torrent (foe attack, no self-buff advertised) does NOT gain a caster +regen (target-trap)', () => {
    expect(DisruptingTorrent.targetType).toBe('Foe');
    // Its resource rows are all foe-facing (toWho Target); none reaches the caster.
    expect(atomsOf(DisruptingTorrent).some((a) => (a.effectType === 'Regeneration' || a.effectType === 'Recovery') && a.toWho === 'Self')).toBe(false);
  });

  it('the foe-target veto is shortHelp-aware: it drops the phantom regen but keeps a genuine advertised one', () => {
    // Both target a Foe and share internalName Touch_of_Fear, but only the Blaster
    // one advertises "Self +Regeneration" — so its regen atoms survive while the Dark
    // Melee fear attack's do not.
    expect(TouchOfTheBeyond.shortHelp).toMatch(/\+\s*Regeneration/i);
    expect(atomsOf(TouchOfTheBeyond).some((a) => a.effectType === 'Regeneration' && a.toWho === 'Self')).toBe(true);
    expect(atomsOf(DarkMeleeTouchOfFear).some((a) => a.effectType === 'Regeneration' && a.toWho === 'Self')).toBe(false);
  });

  it('the pet-target veto is shortHelp-aware: Rally the Militia keeps its Self +Def/+Regen', () => {
    // Same targets_affected=['MyPet'] as the phantom cases, but its shortHelp is
    // "Self, Pets +Defense, +Regeneration" — it genuinely buffs the MM, so the atoms
    // carry both. (The regen is a Target-face row aimed at the caster's pets and self;
    // the surfacing claim is that it exists at all, not that the applier routes it.)
    expect(RallyTheMilitia.shortHelp).toMatch(/\bself\b/i);
    expect(atomsOf(RallyTheMilitia).some((a) => a.effectType === 'Regeneration')).toBe(true);
    expect(atomsOf(RallyTheMilitia).some((a) => a.effectType === 'Defense' && a.aspect === 'Cur')).toBe(true);
  });

  // --- Pet target-trap (guardThunderspyOnesBuffs, `targets_affected=['MyPet']`) ------
  // The MM pet-upgrade powers are auto-pulse PBAoEs cast on Self whose effects land on
  // the henchmen (the binary's `targets_affected` says MyPet, but the per-template target
  // is dropped). Their uniform, unadvertised +15% Recovery therefore reads as a caster
  // self-buff and leaked into the MM's Recovery. Drop it — but shortHelp-aware, so a power
  // that genuinely buffs Self+Pets keeps its buff. The surfacing claims are stated on the
  // atom RECIPIENT: a pet-facing row is toWho Target with targets_affected MyPet, and the
  // leak is precisely a row the caster would absorb — none of these have one.

  it('Equip Robot does NOT leak the pets’ +15% Recovery into the MM (pet target-trap)', () => {
    expect(atomsOf(EquipRobot).some((a) => a.effectType === 'Recovery' && a.toWho === 'Self')).toBe(false);
  });

  it('Repair does NOT leak its pet Endurance heal into the MM', () => {
    expect(atomsOf(Repair).some((a) => a.effectType === 'Endurance' && a.toWho === 'Self')).toBe(false);
  });

  it('Fortify Pack (Pets-only +Def/+Regen, no Self) drops BOTH the phantom regen and defense', () => {
    expect(FortifyPack.shortHelp).not.toMatch(/\bself\b/i);
    expect(atomsOf(FortifyPack).some((a) => (a.effectType === 'Regeneration' || a.effectType === 'Defense') && a.toWho === 'Self')).toBe(false);
  });
});