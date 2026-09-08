import { describe, it, expect, beforeAll } from 'vitest';
// Read straight from the committed generated base (pre-override) so a future regen
// can't silently undo the summon→pet linkage (GAME-DATA-PRINCIPLES §9).
import { UmbraBeast } from './datasets/thunderspy/generated/powersets/controller/primary/darkness-control/umbra-beast';
import { ShadowField } from './datasets/thunderspy/generated/powersets/controller/primary/darkness-control/shadow-field';
import { Haunt } from './datasets/thunderspy/generated/powersets/controller/primary/darkness-control/haunt';
import { SingleShot } from './datasets/thunderspy/generated/powersets/blaster/primary/beam-blast/single-shot';
// Negative case: a plain single-target Hold must NOT gain a phantom summon.
import { DarkGrasp } from './datasets/thunderspy/generated/powersets/controller/primary/darkness-control/dark-grasp';
// Dominator Shadow Field: its base/Domination summon variants are the complementary
// `kStealth <= / > 0.5` branches — the base (Domination-off) must live at effects.summon.
import { ShadowField as DominatorShadowField } from './datasets/thunderspy/generated/powersets/dominator/primary/darkness-control/shadow-field';
// TSPY10 pseudo-pet debuff powers (player power → summon.entity → PET_ENTITIES).
import { Sleet } from './datasets/thunderspy/generated/powersets/defender/primary/cold-domination/sleet';
import { TarPatch } from './datasets/thunderspy/generated/powersets/defender/primary/dark-miasma/tar-patch';
import { Caltrops } from './datasets/thunderspy/generated/powersets/blaster/secondary/devices/caltrops';
import { PET_ENTITIES } from './datasets/thunderspy/pet-entities';
import { loadDataset } from './dataset';
import { calculatePetDamage, synthesizePseudoPetEffects } from '@/utils/calculations/pet-damage';

/**
 * Thunderspy pet / pseudo-pet summon recovery — the DATA-DRIVEN fix.
 *
 * A Thunderspy effect element is HC's EffectGroup: one header over a struct-array of
 * AttribMods, one per pet. The parser long read only the first, so EVERY Thunderspy
 * summon lost its pet linkage: Umbra Beast / Phantasm / MM henchmen / rains / chain-jump
 * & teleport-strike pseudo-pets (Lightning Rod, Shield Charge, Savage Leap, Disintegrate
 * spread) all exported as a bare shell with no `params.entity_def`, so the converter's
 * Create_Entity handler no-opped and the info panel showed nothing.
 *
 * Each pet's AttribMod carries an `EntCreate` payload in its Params union naming the
 * EntityDef and PriorityList, which is what these assertions read
 * (`params.entity_def` → PET_ENTITIES). Recovering it took two passes: the sibling walk
 * that surfaced the per-pet AttribMods at all (TSPY-4), then the Params union itself
 * (WRAP-1). In between, the pets were reconstructed by scanning the element's raw bytes
 * for entity-def-SHAPED strings — which is why Illusion Control's Mirage used to summon
 * `MirageAttackerHit`, a message key. Choosing by shape rather than by field is the
 * failure `test_thunderspy_entcreate_attach.py` now pins.
 *
 * These re-read the recovered shape from the committed dataset (GAME-DATA-PRINCIPLES §9).
 */
describe('Thunderspy pet / pseudo-pet summon recovery (data-driven)', () => {
  it('Umbra Beast links to its Pets_Umbra_Beast entity (single pet)', () => {
    expect(UmbraBeast.summon?.entity).toBe('Pets_Umbra_Beast');
    // The entity_def is the verbatim PET_ENTITIES key — getPetEntity resolves it.
    const ent = PET_ENTITIES['Pets_Umbra_Beast'];
    expect(ent).toBeDefined();
    expect(ent.abilities.length).toBeGreaterThan(0);
  });

  it('Shadow Field (location pseudo-pet) links to Pets_Shadow_Field_Controller', () => {
    expect(ShadowField.summon?.entity).toBe('Pets_Shadow_Field_Controller');
    const ent = PET_ENTITIES['Pets_Shadow_Field_Controller'];
    expect(ent).toBeDefined();
    // The field's actual Hold rides the entity's auto-pulse ability, not the summon shell.
    const hasHold = ent.abilities.some(a => (a.effects || []).some(e => e.type === 'Hold'));
    expect(hasHold).toBe(true);
  });

  it('Haunt counts BOTH shades (465-marker count == pet count)', () => {
    expect(Haunt.summon?.entity).toBe('Pets_Shade');
    expect(Haunt.summon?.entityCount).toBe(2);
    expect(PET_ENTITIES['Pets_Shade']).toBeDefined();
  });

  it('an attack pseudo-pet is surfaced: Beam Single Shot → Pets_DisintegrateSpread', () => {
    // The Disintegrate spread is an incidental Create_Entity on a damage attack,
    // gated on the target already Disintegrating — so it rides a conditionalEffect,
    // not the base `effects`, but is emitted alongside the attack's own effects.
    const cond = SingleShot.conditionalEffects?.find(
      c => c.effects?.summon?.entity === 'Pets_DisintegrateSpread',
    );
    expect(cond).toBeDefined();
  });

  it('a plain single-target Hold gains NO phantom summon (no false positive)', () => {
    expect(DarkGrasp.summon).toBeUndefined();
  });

  // --- TSPY9: pet ABILITY extraction (generic tspy `Damage` attrib) -----------
  // convert-pet-entities' extractDamage keyed only on specific `*_Dmg` attribs +
  // `aspect === 'Absolute'`. Thunderspy pets carry the generic `Damage` attrib
  // with the aspect dropped, so every pure-attack pet (Howler Wolf, Demonlings,
  // Knight Minion, …) extracted ZERO damage and was skipped as "no combat
  // abilities" — its summon linked but showed only the pet name. The fix accepts
  // a positive-scale `Damage` on a `*_Damage` table and types it from the shortHelp.
  it('a summoned attack-pet resolves with element-typed damage (Howler Wolf)', () => {
    const wolf = PET_ENTITIES['MastermindPets_Howler_Wolf'];
    expect(wolf).toBeDefined();
    const bite = wolf.abilities.find(a => a.name === 'Vicious_Bite');
    expect(bite).toBeDefined();
    // Element resolved from the shortHelp DMG(Lethal), scale/table from the binary.
    // Three Lethal components, matching its Rebirth twin byte for byte (Homecoming
    // carries the same three, its 0.42 on Melee_InherentDamage). Only the 0.84 was
    // reachable before the parser walked every AttribMod in an element (TSPY-4).
    expect(bite!.damage).toEqual([
      { damageType: 'Lethal', scale: 0.84, table: 'Melee_Damage' },
      { damageType: 'Lethal', scale: 0.1, table: 'Melee_Damage' },
      { damageType: 'Lethal', scale: 0.42, table: 'Melee_Damage' },
    ]);
  });
});

/**
 * TSPY10: pseudo-pet DEBUFF recovery.
 *
 * The player-facing location/patch powers (Sleet, Freezing Rain, Tar Patch,
 * Caltrops, Ice Slick, …) carry ONLY a Create_Entity summon — every debuff that
 * IS the power (-Resistance / -Defense / -Speed) lives on the summoned pet's
 * ability. TSPY9 gave those pets their damage, but extractEffects keyed on the
 * HC attrib names (base_defense / runningspeed / …); Thunderspy names the applied
 * attrib directly (Debuff_Def / Slow / Res_DMG) and drops the target, so every
 * pseudo-pet surfaced its damage but NOT its debuffs. The fix teaches
 * extractEffects tspy's vocabulary: name-encoded debuffs at |scale| (Slow /
 * Debuff_Def / DeBuff_ToHit), and sign-discriminated resource debuffs on a REAL
 * table (Res_DMG<0 → -Resistance; a `*_Ones` marker is dropped as uncomputable,
 * which also stops +Recovery ally-buffs — Adrenalin Boost / Victory Rush — from
 * being mislabeled as -Recovery).
 */
describe('Thunderspy pseudo-pet debuff recovery (TSPY10)', () => {
  beforeAll(async () => { await loadDataset('thunderspy'); });

  const entityEffects = (key: string) =>
    (PET_ENTITIES[key]?.abilities || []).flatMap(a => a.effects || []);

  it('Sleet: player power → Pets_Sleet_Defender carrying -Res, -Def and -Speed', () => {
    expect(Sleet.summon?.entity).toBe('Pets_Sleet_Defender');
    const types = new Set(entityEffects('Pets_Sleet_Defender').map(e => e.type));
    expect(types).toContain('ResistanceDebuff');
    expect(types).toContain('DefenseDebuff');
    expect(types).toContain('Slow');
  });

  it('Tar Patch: its -Resistance (the point of the power) is on the pet', () => {
    expect(TarPatch.summon?.entity).toBe('Pets_TarPatch');
    const res = entityEffects('Pets_TarPatch').find(e => e.type === 'ResistanceDebuff');
    expect(res).toBeDefined();
    expect(res!.scale).toBeGreaterThan(0);
    expect(res!.table).toMatch(/res_dmg/i);
  });

  it('Caltrops: surfaces its -Speed slow, and NO phantom -Res/-Def', () => {
    expect(Caltrops.summon?.entity).toBe('Pets_Caltrops');
    const types = new Set(entityEffects('Pets_Caltrops').map(e => e.type));
    expect(types).toContain('Slow');
    expect(types.has('ResistanceDebuff')).toBe(false);
    expect(types.has('DefenseDebuff')).toBe(false);
  });

  it('the -Resistance actually COMPUTES to a percent (table resolves, not "—")', () => {
    const r = calculatePetDamage('Pets_Sleet_Defender', 50);
    const res = r?.allEffects.find(e => e.type === 'ResistanceDebuff');
    expect(res).toBeDefined();
    // A resolved table yields a real fraction; an unresolved one leaves value undefined.
    expect(res!.value).toBeGreaterThan(0);
  });

  it('sign-trap + Ones-marker guard: no pet carries a (bogus) -Recovery debuff', () => {
    // +Recovery self/ally-buffs (Adrenalin Boost, Victory Rush) rode `*_Ones`
    // markers the old HC map mislabeled as RecoveryDebuff; the fix drops them all.
    const withRecoveryDebuff = Object.values(PET_ENTITIES).filter(ent =>
      (ent.abilities || []).some(a => (a.effects || []).some(e => e.type === 'RecoveryDebuff')),
    );
    expect(withRecoveryDebuff).toHaveLength(0);
  });

  // Shadow Field is a LOCATION AoE Hold: the player power carries only the summon,
  // so its control (Hold) AND debuff (-ToHit) live entirely on the pseudo-pet.
  // synthesizePseudoPetEffects hoists a non-commandable summon's mez + enhanceable
  // debuffs into the parent power's Power Effects block (so they show as the power's
  // own control/debuff, not just a Summons chip). Before TSPY10 the -ToHit (tspy
  // `DeBuff_ToHit`) was unmapped, so only the Hold surfaced; now both do.
  it('Shadow Field hoists BOTH its Hold control and -ToHit debuff to Power Effects', () => {
    const synth = synthesizePseudoPetEffects(ShadowField.summon);
    expect(synth).not.toBeNull();
    // Control: the location hold surfaces as the power's own mez (Mag 3).
    expect(synth!.hold).toBeDefined();
    expect((synth!.hold as { mag: number }).mag).toBe(3);
    // Debuff: the -ToHit (the TSPY10 addition) rides a real Debuff_ToHit table.
    expect(synth!.tohitDebuff).toBeDefined();
    expect((synth!.tohitDebuff as { table: string }).table).toMatch(/debuff_tohit/i);
  });

  // The DOMINATOR Shadow Field splits its summon into two complementary
  // `kStealth source> 0.5` branches — `<=` (Domination OFF = base/default) and
  // `>` (Domination ON = the `_Domination` variant). `_isConditionalGate` used to
  // treat the `<=` branch as conditional too, so the BASE summon was swallowed
  // into the "Domination Active" conditional and the power rendered nothing by
  // default. The base must sit at effects.summon; the boosted variant stays a
  // conditional. Without the base at effects.summon, neither the pet block nor the
  // hoisted control/debuff show unless Domination is toggled on.
  it('Dominator Shadow Field surfaces its BASE summon by default (Domination off)', () => {
    // Base summon shown by default → the pet block + hoist fire with Domination off.
    expect(DominatorShadowField.summon?.entity).toBe('Pets_Shadow_Field_Dominator');
    const synth = synthesizePseudoPetEffects(DominatorShadowField.summon);
    expect(synth?.hold).toBeDefined();
    expect(synth?.tohitDebuff).toBeDefined();
    // The Domination-boosted variant is preserved as a conditional, not lost.
    const dom = DominatorShadowField.conditionalEffects?.find(c => c.id === 'domination');
    expect(dom?.effects?.summon?.entity).toBe('Pets_Shadow_Field_Dominator_Domination');
  });
});
