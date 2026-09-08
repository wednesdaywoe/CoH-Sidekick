import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data/powersets';
import {
  selfSlowValue,
  selfDamageDebuffValue,
  selfRechargeDebuffValue,
  selfDefenseDebuffValue,
} from '@/data/core/atom-query';

/**
 * Retirement of the bag-level `selfPenalty` boolean → per-atom readers.
 *
 * The old converter set ONE `selfPenalty` flag on the whole effects bag whenever
 * any Self-targeting debuff appeared, and the calc read it bag-wide — so a foe
 * debuff co-located in the same `slow` map (Rebirth Granite's `AnyAffected`
 * -JumpHeight) was dragged onto the caster's own totals, in direct violation of
 * the converter's own "foe slows don't slow the player" classification.
 *
 * The atom readers apply per-ENTRY: `selfSlowValue` / `selfDamageDebuffValue` /
 * `selfRechargeDebuffValue` / `selfDefenseDebuffValue` publish what actually
 * reaches the caster. `hasSelfDirectedPenalty` / `isSelfDirectedEffect` (the old
 * bag predicates) are retired; the claims below now read the SAME readers the
 * totals pass spends (`character-totals.ts`'s slow/recharge/damage blocks).
 *
 * Three claims re-point onto the readers (HC Granite's self slow/-Dmg/-Rech,
 * Reaction Time's silence, Ice Bolt's clean foe-only). One is OVERTURNED, not
 * re-pointed: Rebirth Granite's `-500` movement row has `toWho:'Target'` and
 * `reachesCaster`'s TARGETS-3 join (power recipients, not row aspect) admits it
 * — Granite is Self-only, so the fly-speed/JumpValues the row states are the
 * caster's, whatever the row's aspect says. The old bag tag excluded it and the
 * registered claim pinned that leak as sealed; the reader says it is the
 * caster's own −JumpHeight. That is a number decision, recorded in
 * `docs/streams/atom-migration.md`.
 *
 * The Rage −Def(All) claim needs the bag's scalar `defenseDebuff` slot, written
 * by the converter's `base_defense` branch; `selfDefenseDebuffValue` mirrors it
 * (Defense/All debuff rows only, `toWho:'Self'` tag). Population 2/2/1/0.
 */
describe('selfPenalty → toWho retirement', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
    await loadDataset('rebirth');
  });

  it('HC Granite Armor: every slow entry stays self-directed (no-op vs old flag)', async () => {
    await loadDataset('homecoming');
    const granite = getPowerset('tanker/stone-armor')?.powers.find((p) => p.internalName === 'Granite_Armor');
    expect(granite).toBeDefined();
    // Granite's penalties are ALL Self in HC — each entry must carry the marker.
    for (const e of selfSlowValue(granite!) ?? []) {
      expect(e.axis).toBeDefined();
    }
    // The -damage / -recharge self-penalties are self-directed too.
    expect(selfDamageDebuffValue(granite!)).toEqual({ scale: 0.3, table: 'Melee_Ones' });
    expect(selfRechargeDebuffValue(granite!)).toEqual({ scale: 0.65, table: 'Melee_Ones' });
  });

  it('Rebirth Granite Armor: the foe -JumpHeight is NOT self-directed (leak sealed)', async () => {
    await loadDataset('rebirth');
    const granite = getPowerset('tanker/stone-armor')?.powers.find((p) => p.internalName === 'Granite_Armor');
    expect(granite).toBeDefined();
    // OVERTURNED: the old bag tag (`isSelfDirectedEffect`) excluded this row; the
    // atom join `reachesCaster` (power recipients) admits it, so the -500 on
    // Rebirth granite's Self-only frame reaches the caster and `selfSlowValue`
    // publishes it. The old claim pinned the opposite. See the file header /
    // docs/streams/atom-migration.md — the reader's read is the truer one.
    const jump = selfSlowValue(granite!)?.find((e) => e.axis === 'jumpHeight');
    expect(jump).toBeDefined();
    expect(jump!.scale).toBe(500);
  });

  it('HC Reaction Time: no self-penalty — its Self faces are the OnDeactivate burst, not base', async () => {
    await loadDataset('homecoming');
    const rt = getPowerset('blaster/martial-combat')?.powers.find((p) => p.internalName === 'Reaction_Time');
    expect(rt).toBeDefined();
    // The authored def (Reaction_Time.powers) puts every Target kSelf mod at
    // ApplicationType kOnDeactivate with NEGATIVE slow-table scales: the
    // deactivation speed burst, a self-BUFF that fires once when the toggle drops.
    // The pipeline converter keeps only base-application mods in the bag, so the
    // surviving entries are AnyAffected FOE debuffs — nothing reaches the caster.
    expect(selfSlowValue(rt!)).toEqual([]); // base slow atoms exist, none reach the caster
    expect(selfRechargeDebuffValue(rt!)).toBeUndefined();
    expect(selfDamageDebuffValue(rt!)).toBeUndefined();
  });

  it('a pure foe slow (Ice Bolt) is never self-directed', async () => {
    await loadDataset('homecoming');
    const bolt = getPowerset('blaster/ice-blast')?.powers.find((p) => p.internalName === 'Ice_Bolt');
    expect(bolt).toBeDefined();
    expect(selfSlowValue(bolt!)).toEqual([]); // foe slow, nothing reaches the caster
    expect(selfRechargeDebuffValue(bolt!)).toBeUndefined();
    expect(selfDamageDebuffValue(bolt!)).toBeUndefined();
  });

  // Rage's crash is TWO self-penalties: -100% damage (long-tagged) AND -20%
  // Defense(All). The bag's `defenseDebuff` slot (the scalar, `base_defense`
  // branch) carries the -Def with toWho:'Self' on both AT variants — the DSH6c
  // discriminator gate caught this on its first run (self-penalty|Defense).
  it('HC Rage crash: the -Def(All) is self-directed (DSH6c catch), like its -Dmg', async () => {
    await loadDataset('homecoming');
    for (const setId of ['brute/super-strength', 'tanker/super-strength']) {
      const rage = getPowerset(setId)?.powers.find((p) => p.internalName === 'Rage');
      expect(rage, setId).toBeDefined();
      expect(selfDefenseDebuffValue(rage!), `${setId} -Def self`).toEqual({ scale: 0.2, table: 'Melee_Ones' });
      expect(selfDamageDebuffValue(rage!), `${setId} -Dmg self`).toEqual({ scale: 999, table: 'Melee_Buff_Dmg' });
    }
  });
});
