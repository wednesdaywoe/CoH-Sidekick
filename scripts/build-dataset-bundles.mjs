/**
 * Bundle each dataset's module graph into one standalone ESM file, for vitest only.
 *
 * A dataset is ~7,300 generated TypeScript modules (~48 MB). `loadDataset()` pulls the
 * whole graph through a dynamic import, so every test file that touches a dataset paid
 * ~14-28s to transform and execute those 7,300 modules again in its own isolated worker.
 * 142 of the suite's 277 test files do exactly that, and it was most of the wall clock.
 *
 * esbuild flattens one dataset in ~230ms, and the result is import-free: `@/types` is
 * type-only (erased) and the single value dependency, `@/data/_layer`, is two pure
 * functions with no module state, so inlining it clones no state that anything observes.
 * Node then imports the bundle natively in ~200ms instead of ~14s — vite never sees it
 * (see `test.server.deps.external` in vite.config.ts), so nothing re-transforms 30 MB.
 *
 * Correctness rests on this being the SAME SOURCE, flattened: vitest's swap plugin
 * redirects only the dataset ROOT specifier, so a test importing a file inside the
 * dataset folder still gets the real module. `src/data/dataset-bundle-fidelity.test.ts`
 * grades the bundle against the module graph and is the gate on that claim.
 *
 * Staleness is not possible by construction: the bundles are rebuilt on every vitest
 * run (globalSetup) and live in a gitignored folder, so there is no committed artifact
 * to drift. That costs ~1s per suite run, which is the price of never debugging a test
 * that graded yesterday's data.
 */

import { build } from 'esbuild';
import { createRequire } from 'node:module';
import { mkdirSync, rmSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = resolve(REPO_ROOT, 'src');
const OUT_DIR = resolve(REPO_ROOT, '.dataset-bundles');

const { ALL_DATASETS } = createRequire(import.meta.url)('./_dataset-paths.cjs');

/** Where the swap plugin and the fidelity test expect dataset `id`'s bundle. */
export function bundlePath(id) {
  return resolve(OUT_DIR, `${id}.mjs`);
}

export async function buildDatasetBundles({ quiet = false } = {}) {
  rmSync(OUT_DIR, { recursive: true, force: true });
  mkdirSync(OUT_DIR, { recursive: true });

  const started = Date.now();
  await Promise.all(
    ALL_DATASETS.map(async (id) => {
      const result = await build({
        entryPoints: [resolve(SRC, 'data/datasets', id, 'index.ts')],
        bundle: true,
        format: 'esm',
        platform: 'neutral',
        outfile: bundlePath(id),
        alias: { '@': SRC },
        logLevel: 'warning',
        metafile: true,
      });

      // Fail loud on anything the bundle could not inline. An external import here means
      // the dataset reached into app code with module state, and the bundle would hand
      // tests a second instance of it — a divergence no assertion in the suite would see.
      const imports = Object.values(result.metafile.outputs)[0]?.imports ?? [];
      const external = imports.filter((i) => i.external);
      if (external.length) {
        throw new Error(
          `dataset '${id}' bundle is not self-contained; esbuild left ${external.length} ` +
            `external import(s): ${[...new Set(external.map((i) => i.path))].join(', ')}.\n` +
            `Bundling a module that app code also imports would fork its state. Either keep the ` +
            `dataset free of it, or teach this script to share the instance.`,
        );
      }
    }),
  );

  if (!quiet) {
    console.log(
      `[dataset-bundles] ${ALL_DATASETS.length} dataset(s) bundled in ${Date.now() - started}ms -> ${OUT_DIR}`,
    );
  }
}

/** vitest globalSetup entry. */
export default async function setup() {
  await buildDatasetBundles({ quiet: true });
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await buildDatasetBundles();
}
