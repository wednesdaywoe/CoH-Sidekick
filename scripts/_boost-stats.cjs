/**
 * The `boosts_allowed` vocabulary shared by the enhancement converters.
 *
 * A boost record states what it enhances in its own `boosts_allowed` list —
 * aspect tokens mixed with the origin/Hamidon tokens that gate *who* may slot
 * it. This module owns the split and the token -> planner-stat mapping, so the
 * special-enhancement registry and the boost index read one table rather than
 * two copies that can drift apart.
 *
 * The mapping is a CLOSED vocabulary: an unlisted token is a new data variant
 * and must break the build rather than be silently dropped.
 */

// `Hamidon` marks special-origin slotting; the origin names gate who can slot,
// not what is enhanced.
const NON_ASPECT_TOKENS = new Set(['Hamidon', 'Magic', 'Mutation', 'Natural', 'Science', 'Technology']);

// boosts_allowed token -> planner stat(s). `Heal` covers Absorb too: the
// HC/Rebirth heal templates carry the Absorb attrib alongside Heal_Dmg.
const BOOST_TYPE_STATS = {
  Accuracy: ['Accuracy'],
  Damage: ['Damage'],
  Recharge: ['Recharge'],
  EnduranceDiscount: ['EnduranceReduction'],
  Recovery: ['EnduranceModification'],
  Heal: ['Healing', 'Absorb'],
  Buff_ToHit: ['ToHit'],
  Buff_Defense: ['Defense'],
  Debuff_ToHit: ['ToHit Debuff'],
  Debuff_Defense: ['Defense Debuff'],
  Res_Damage: ['Resistance'],
  Range: ['Range'],
  Hold: ['Hold'],
  Stun: ['Stun'],
  Sleep: ['Sleep'],
  Immobilize: ['Immobilize'],
  Fear: ['Fear'],
  Confuse: ['Confuse'],
  Intangible: ['Intangible'],
  Slow: ['Slow'],
  Taunt: ['Taunt'],
  SpeedRunning: ['Run Speed'],
  Jump: ['Jump'],
  SpeedFlying: ['Fly'],
  // Carried by the common-IO and origin families, which the special registries
  // never reach.
  Interrupt: ['Interrupt'],
  Knockback: ['Knockback'],
};

/** The aspect tokens of a boost record — `boosts_allowed` minus the slotting gates. */
function aspectTokens(boostsAllowed) {
  return (boostsAllowed || []).filter((t) => !NON_ASPECT_TOKENS.has(t));
}

/**
 * The planner stats a boost record enhances, in `boosts_allowed` order and
 * de-duplicated. `where` names the record for the error a new token raises.
 */
function aspectStats(boostsAllowed, where) {
  const stats = [];
  for (const token of aspectTokens(boostsAllowed)) {
    const mapped = BOOST_TYPE_STATS[token];
    if (!mapped) {
      throw new Error(
        `${where}: unknown boosts_allowed token "${token}" — extend BOOST_TYPE_STATS in scripts/_boost-stats.cjs deliberately`,
      );
    }
    for (const stat of mapped) if (!stats.includes(stat)) stats.push(stat);
  }
  return stats;
}

module.exports = { NON_ASPECT_TOKENS, BOOST_TYPE_STATS, aspectTokens, aspectStats };
