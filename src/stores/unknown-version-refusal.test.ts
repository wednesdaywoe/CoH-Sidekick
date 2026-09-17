// MUST be first: installs an in-memory localStorage before the store module is
// evaluated (the store caches its persist storage at eval time).
import '@/test/localstorage-polyfill';
import { describe, it, expect, beforeAll, vi } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { createEmptyBuild } from '@/types/build';
import { slimBuild } from '@/utils/build-serialization';
import { useBuildStore } from '@/stores/buildStore';

/**
 * An export version this reader does not know must be REFUSED, not guessed at.
 *
 * The v1 arm used to be `importBuild`'s fall-through, so it was also the arm
 * every future version landed in — and v1 is a whole `Build` object where every
 * version since is the slim shape. A newer export was therefore spread into a
 * Build, `sets` came back undefined, and the result rendered as a mangled build
 * rather than an error. That is a soft-wrong read: the reader guessed, and it
 * guessed in the direction nobody re-checks.
 *
 * It stopped being hypothetical when the canonical Rust rebuild started writing
 * v5 into the same `shared_builds` rows this app reads (cloud-social RB4e). A
 * build shared from the rebuild and opened here would have drawn a corrupt build
 * on the live site for as long as the two ran side by side.
 */

beforeAll(async () => {
  await loadDataset('homecoming');
});

/** A v5 payload as `skif::encode` writes it: the slim shape, keyed `dataset`.
 *
 * Deliberately COMPLETE enough that the old fall-through would have accepted it.
 * The first cut of this fixture omitted `pools`, and the v1 arm then died on
 * `pools is not iterable` — which returned false for the wrong reason and made
 * the gate look like it was working when it was not. The payload a guard is
 * tested with has to be one the bug would actually have shipped.
 */
function v5Json(): string {
  return JSON.stringify({
    version: 5,
    build: {
      name: 'From the rebuild',
      dataset: 'homecoming',
      archetype: 'controller',
      level: 50,
      primary: { id: 'illusion_control', powers: [] },
      secondary: { id: 'kinetics', powers: [] },
      pools: [],
      epicPool: null,
      inherents: [],
      accolades: [],
      incarnates: {},
      slotOrder: [],
    },
  });
}

function v4Json(): string {
  const build = createEmptyBuild('homecoming');
  build.level = 50;
  return JSON.stringify({ version: 4, build: slimBuild(build) });
}

describe('importBuild version gate', () => {
  it('refuses a version newer than it reads instead of falling into the v1 arm', () => {
    const before = useBuildStore.getState().build;
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {});

    const accepted = useBuildStore.getState().importBuild(v5Json());

    expect(accepted).toBe(false);
    // The refusal is loud — the version it could not read is in the message,
    // because "import failed" alone sends the reader to the wrong file.
    expect(logged).toHaveBeenCalled();
    expect(String(logged.mock.calls[0])).toMatch(/5/);
    // And nothing was applied: a refused import leaves the build it refused for.
    expect(useBuildStore.getState().build).toBe(before);
    logged.mockRestore();
  });

  it('still reads the versions it supports', () => {
    expect(useBuildStore.getState().importBuild(v4Json())).toBe(true);
  });

  it('still reads a v1 file, which is what the fall-through was FOR', () => {
    const v1 = JSON.stringify({
      version: 1,
      build: { ...createEmptyBuild('homecoming'), sets: {} },
    });
    expect(useBuildStore.getState().importBuild(v1)).toBe(true);
  });
});
