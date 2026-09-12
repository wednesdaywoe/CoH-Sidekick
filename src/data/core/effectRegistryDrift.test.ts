/**
 * PROD6B-2 drift gate — the beta's EFFECT_REGISTRY and the rebuild's
 * `contract/effect-registry.json` must agree on every RESOLUTION field.
 *
 * The engine resolves per-power granted magnitudes from the contract file; this component
 * registry resolves the same rows for display. Two hand-maintained copies of the same rules
 * is the FLAGS-2 shape — self-consistent on each side, silently divergent between them —
 * so this test is the thing that makes them one source rather than two.
 *
 * Presentation-only fields (`colorClass`, `renderAs`) are deliberately absent from the
 * contract and are not compared: the engine has no business knowing Tailwind classes.
 */
import { describe, it, expect } from 'vitest';
import { EFFECT_REGISTRY, type EffectDisplayConfig } from './effect-registry';
import {
  EFFECT_RESOLUTION,
  EFFECT_TYPE_LABELS,
  EFFECT_MEZ_LABELS,
} from '@/data/generated/effect-registry.generated';
import { TYPE_LABELS_FULL, MEZ_LABELS_FOR_DRIFT_GATE } from '@/components/info/powerDisplayUtils';

/** Every field of `EffectDisplayConfig` the contract owns. */
const RESOLUTION_FIELDS = [
  'label',
  'category',
  'format',
  'enhancementAspect',
  'strengthAspect',
  'calculation',
  'priority',
  'canBeByType',
  'expandByType',
  'precision',
  'baseMultiplier',
  'flatPercentPerScale',
  'scalarFromTablePercent',
  'valueFromTable',
  'maxHpFractionPercentForm',
  // Authored in the contract and mirrored here. Graded like any other resolution field: an
  // ungraded field is how this copy drifted into being a third registry in the first place.
  'summaryTokens',
] as const;

/** Fields that stay beta-side because they describe rendering, not resolution. */
const PRESENTATION_FIELDS = ['colorClass', 'renderAs'] as const;

const resolutionOf = (config: Record<string, unknown>): Record<string, unknown> => {
  const out: Record<string, unknown> = {};
  for (const field of RESOLUTION_FIELDS) {
    if (config[field] !== undefined) out[field] = config[field];
  }
  return out;
};

describe('effect registry drift (PROD6B-2)', () => {
  it('registers exactly the same effect keys as the contract', () => {
    expect(Object.keys(EFFECT_REGISTRY).sort()).toEqual(Object.keys(EFFECT_RESOLUTION).sort());
  });

  it('agrees with the contract on every resolution field of every effect', () => {
    for (const key of Object.keys(EFFECT_REGISTRY)) {
      const beta = resolutionOf(EFFECT_REGISTRY[key] as unknown as Record<string, unknown>);
      const contract = EFFECT_RESOLUTION[key];
      expect(contract, `contract is missing effect ${key}`).toBeDefined();
      expect(beta, `effect ${key} drifted from contract/effect-registry.json`).toEqual(contract);
    }
  });

  it('agrees with the contract on the shared label vocabularies', () => {
    // The label maps are the other half of the shared resolution data — the engine builds an
    // expanded row's label from them. A missing entry silently changes a row's label on one side
    // only (it cost exactly that: `knockback`/`knockup`/`repel` were absent from the first draft
    // of the contract and the engine rendered raw keys).
    expect(TYPE_LABELS_FULL).toEqual(EFFECT_TYPE_LABELS);
    expect(MEZ_LABELS_FOR_DRIFT_GATE).toEqual(EFFECT_MEZ_LABELS);
  });

  it('carries no field the contract cannot express', () => {
    // A new resolution field added to the beta config but not to the contract would be
    // invisible to the engine — the drift this gate exists to catch. Presentation fields
    // are the only allowed exception.
    const known = new Set<string>([...RESOLUTION_FIELDS, ...PRESENTATION_FIELDS]);
    for (const [key, config] of Object.entries(EFFECT_REGISTRY)) {
      const unknownFields = Object.keys(config as unknown as EffectDisplayConfig).filter(
        (field) => !known.has(field)
      );
      expect(unknownFields, `effect ${key} carries unmodelled field(s)`).toEqual([]);
    }
  });
});
