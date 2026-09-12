/**
 * Quick-cast snipe form.
 *
 * Snipe powers carry a `quickSnipe` variant (fast cast, higher damage). This merges it in when
 * told to, or returns the power unchanged. `fastFormActive` is a plain boolean rather than a
 * condition-aware gate ON PURPOSE — this function has no build/combat context of its own, so
 * whether the fast form SHOULD be active is entirely the caller's call:
 *  - `resolveEffectivePower` evaluates `power.quickSnipe.condition` for real (SNIPE-2 —
 *    Homecoming gates on combat engagement, Rebirth/Thunderspy on ToHit ≥ 97%) before calling
 *    this, since it's answering "what does the build show RIGHT NOW".
 *  - `attack-chain-powers.ts` is answering a different question ("what does this attack chain
 *    look like once the fast form is reachable") and passes its own simplified boolean
 *    deliberately — not a stale copy of this gate.
 *  - The damage-bar SCALE no longer consults this at all. It used to, through
 *    `useBuildMaxAttackDamage`, which folded over the build's picked powers and had to apply the
 *    fast form or a fast snipe would self-clamp at 100%. The scale is now the engine's
 *    `damage_ceiling` over the build's whole powersets, read from set definitions — and
 *    `DamageBlock` widens its reference to cover any power that out-hits it, which is what
 *    absorbs the fast form without a second copy of this gate.
 *
 * Single-sourced so every caller applies the SAME stats/damage merge once it decides the fast
 * form is active. If that merge diverged, a fast snipe's bar numerator (boosted damage) would be
 * normalized against a reference computed from its slow damage and clamp to 100% — and the
 * understated reference could push other powers' bars to 100% too.
 */
import type { Power } from '@/types';

export function applyQuickSnipe<T extends Power>(power: T, fastFormActive: boolean): T {
  if (!fastFormActive || !power.quickSnipe) return power;
  const qs = power.quickSnipe;
  return {
    ...power,
    // The fast (in-combat) snipe has no interruptible channel, so clear the base
    // form's interruptTime when swapping — otherwise it carries over stale.
    stats: power.stats ? { ...power.stats, ...qs.stats, interruptTime: undefined } : power.stats,
    damage: qs.damage,
    // Epic-pool powers store cast/range/accuracy in `effects` rather than `stats`.
    effects: power.effects
      ? {
          ...power.effects,
          ...(qs.stats.castTime != null && { castTime: qs.stats.castTime }),
          ...(qs.stats.range != null && { range: qs.stats.range }),
          ...(qs.stats.accuracy != null && { accuracy: qs.stats.accuracy }),
        }
      : power.effects,
  } as T;
}
