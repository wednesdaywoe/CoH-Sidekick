import { describe, it, expect } from 'vitest';
import { DATASET_IDS, type Dataset, type DatasetId } from './dataset';

/**
 * The gate on the vitest dataset swap (see `datasetBundleSwapPlugin` in vite.config.ts).
 *
 * Under vitest, `loadDataset()` no longer executes a dataset's ~7,300 modules — it gets the
 * flattened esbuild bundle, which is ~50x faster to load. That substitution is only
 * legitimate while the bundle IS the same data, so this file is the one place that pays the
 * real cost: it loads BOTH forms of every dataset and compares them.
 *
 * The two forms are named by SPECIFIER, not by path. The swap plugin redirects a dataset's
 * ROOT specifier and nothing else, so `./datasets/<id>` arrives as the bundle while
 * `./datasets/<id>/index` still reaches the module graph. That grades the redirect itself
 * rather than a file that merely ought to be what the redirect serves — and it keeps the
 * 30 MB bundles out of TypeScript's program, which `allowJs` would otherwise parse into an
 * out-of-memory `tsc`.
 *
 * Deliberately ONE file for all four datasets, though that makes it the suite's longest.
 * Split into four, the graph loads run concurrently and contend for memory with everything
 * else: the suite went from 60s to 73s and `powerProjectionParity` alone slowed by 19s.
 * Serializing the four heavy loads inside one worker is the faster shape, measured.
 *
 * Functions can't be compared by value, so they are compared by behaviour; everything else
 * is compared by a structural serialization that pins key order.
 */

type Loader = () => Promise<{ default: Dataset }>;

/** Redirected by the swap plugin — this is the flattened bundle under vitest. */
const BUNDLE: Record<DatasetId, Loader> = {
  homecoming: () => import('./datasets/homecoming'),
  rebirth: () => import('./datasets/rebirth'),
  thunderspy: () => import('./datasets/thunderspy'),
  brainstorm: () => import('./datasets/brainstorm'),
};

/** Naming `index` explicitly misses the plugin's root match — the real module graph. */
const GRAPH: Record<DatasetId, Loader> = {
  homecoming: () => import('./datasets/homecoming/index'),
  rebirth: () => import('./datasets/rebirth/index'),
  thunderspy: () => import('./datasets/thunderspy/index'),
  brainstorm: () => import('./datasets/brainstorm/index'),
};

/** Serializable shape: functions become an arity marker, object keys are sorted. */
function shape(value: unknown): unknown {
  if (typeof value === 'function') return `[fn/${value.length}]`;
  if (Array.isArray(value)) return value.map(shape);
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value as object).sort()) {
      out[key] = shape((value as Record<string, unknown>)[key]);
    }
    return out;
  }
  return value;
}

it('every dataset is graded — a new one must be named in both maps', () => {
  expect(Object.keys(GRAPH).sort()).toEqual([...DATASET_IDS].sort());
  expect(Object.keys(BUNDLE).sort()).toEqual([...DATASET_IDS].sort());
});

describe.each(DATASET_IDS)('%s: bundle matches the module graph', (id) => {
  it('is served by the swap plugin, not by the graph', async () => {
    const [graph, bundle] = await Promise.all([GRAPH[id](), BUNDLE[id]()]);
    // If these were the same object the comparisons below would be vacuous — the swap
    // would have silently stopped applying and this gate would pass on nothing.
    expect(bundle.default).not.toBe(graph.default);
  });

  it('serializes identically', async () => {
    const [graph, bundle] = await Promise.all([GRAPH[id](), BUNDLE[id]()]);
    expect(JSON.stringify(shape(bundle.default))).toBe(JSON.stringify(shape(graph.default)));
  });

  it('purple-patch functions agree across the level-difference range', async () => {
    const [graph, bundle] = await Promise.all([GRAPH[id](), BUNDLE[id]()]);
    for (let diff = -20; diff <= 20; diff++) {
      expect(bundle.default.purplePatch.getBaseToHit(diff)).toBe(
        graph.default.purplePatch.getBaseToHit(diff),
      );
      expect(bundle.default.purplePatch.getCombatModifier(diff)).toBe(
        graph.default.purplePatch.getCombatModifier(diff),
      );
      for (const mode of ['standard', 'incarnate'] as const) {
        expect(bundle.default.purplePatch.getDefenseSoftcap(diff, mode)).toBe(
          graph.default.purplePatch.getDefenseSoftcap(diff, mode),
        );
      }
    }
  });
});
