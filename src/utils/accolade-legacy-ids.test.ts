// MUST be first: installs an in-memory localStorage before the store module is evaluated
// (the store caches its persist storage at eval time). The rehydrate door below needs it.
import '@/test/localstorage-polyfill';
import { describe, it, expect, beforeAll, afterEach } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { createEmptyBuild } from '@/types/build';
import type { Build } from '@/types/build';
import { serializeBuildForStorage } from '@/utils/per-server-builds';
import { normalizeAccoladeIds, hydrateBuild, slimBuild } from '@/utils/build-serialization';
import { useBuildStore } from '@/stores/buildStore';

/**
 * Every door a build enters by normalises its accolade list — DATA-GAP ACCOLADE-3.
 *
 * The defect was never the rename itself. The rename was correct and it existed; it just
 * lived in ONE place, `buildStore`'s persisted-state migration, so it covered localStorage
 * and nothing else. A `.skif` opened from disk, a share link, a pasted JSON and a cloud build
 * all reach the build through `hydrateBuild` instead, kept `atlas_medallion`, and lost The
 * Atlas Medallion's +5 Max End out of the totals without a word. A migration on one door is
 * not a migration, which is the whole finding and the reason this file is a per-door sweep
 * rather than a test of the table.
 *
 * The doors below are the census, and it is closed: `importBuild` (v1 and v2/v3/v4 arms),
 * `hydrateBuild` directly (the detailed-totals modal), and the store's rehydrate. The `.mbd`
 * importer is deliberately absent — it resolves each accolade against the live roster and
 * returns `accoladeId(toggle)`, so it cannot emit a legacy id, and it already warns on a miss.
 */

const KEY = 'coh-planner-build';

beforeAll(async () => {
  await loadDataset('homecoming');
});

afterEach(() => {
  localStorage.clear();
});

const LEGACY = ['atlas_medallion', 'freedom_phalanx'];
const CURRENT = ['the_atlas_medallion', 'freedom_phalanx_reserve'];

describe('normalizeAccoladeIds', () => {
  it('renames the two ids that predate the internal-name convention', () => {
    expect(normalizeAccoladeIds(LEGACY)).toEqual(CURRENT);
  });

  /**
   * The other half of the same migration, and the reason they share a function: accolades
   * were once stored as whole `{ id, bonuses, … }` objects, and the id is all that survives
   * because the export owns those values (Rule 0).
   */
  it('folds a legacy object to its id, and renames it in the same pass', () => {
    expect(normalizeAccoladeIds([{ id: 'atlas_medallion', bonuses: [{ stat: 'maxEnd', value: 5 }] }]))
      .toEqual(['the_atlas_medallion']);
  });

  /**
   * Renaming is not resolving. An id the table does not know is the case the totals have to
   * REPORT; inventing an answer here would put ACCOLADE-3's silence back one layer down.
   */
  it('passes an id it does not know through unchanged', () => {
    expect(normalizeAccoladeIds(['not_an_accolade'])).toEqual(['not_an_accolade']);
  });

  it('answers an empty list for a missing or malformed roster', () => {
    expect(normalizeAccoladeIds(undefined)).toEqual([]);
    expect(normalizeAccoladeIds('the_atlas_medallion')).toEqual([]);
    expect(normalizeAccoladeIds([null, 42, { noId: true }])).toEqual([]);
  });
});

describe('every door normalises the accolade roster', () => {
  /** The door ACCOLADE-3 came through: a `.skif` / share link / cloud build. */
  it('hydrateBuild — the file, share-link and cloud-build door', () => {
    expect(hydrateBuild({ accolades: LEGACY }).accolades).toEqual(CURRENT);
  });

  it('importBuild v4 — the slim arm', () => {
    const slim = slimBuild(createEmptyBuild('homecoming'));
    slim.accolades = LEGACY;
    useBuildStore.getState().importBuild(JSON.stringify({ version: 4, build: slim }));
    expect(useBuildStore.getState().build.accolades).toEqual(CURRENT);
  });

  /**
   * v1 is the one arm that never reaches `hydrateBuild` — it takes the whole `Build` object
   * and only converts the set-piece arrays. It is also the exact era that wrote accolades as
   * OBJECTS (the exporter stamped `version: 1` while the store held `{ id, bonuses, … }`), so
   * left raw it lost every accolade in the file, not just the two renames. Found by the
   * ACCOLADE-3 door census rather than by the row, which only named the two.
   */
  it('importBuild v1 — the raw-Build arm, which never folded its objects at all', () => {
    const build = createEmptyBuild('homecoming');
    const v1 = {
      ...build,
      accolades: [{ id: 'atlas_medallion' }, { id: 'iron_man' }],
      sets: {},
    };
    useBuildStore.getState().importBuild(JSON.stringify({ version: 1, build: v1 }));
    expect(useBuildStore.getState().build.accolades).toEqual(['the_atlas_medallion', 'iron_man']);
  });

  /**
   * The door that always worked, driven through the real persist pipeline so the delegation
   * is proved rather than assumed: the table now lives in `normalizeAccoladeIds`, and this
   * says the store still gets the same answer from it.
   */
  it('persist rehydrate — the door that already had the rename', async () => {
    const stored = serializeBuildForStorage(createEmptyBuild('homecoming')) as Record<string, unknown>;
    stored.accolades = LEGACY;
    localStorage.setItem(KEY, JSON.stringify({ state: { build: stored }, version: 0 }));
    await useBuildStore.persist.rehydrate();
    expect(useBuildStore.getState().build.accolades).toEqual(CURRENT);
  });
});

/**
 * A round trip cannot reintroduce the legacy spelling. `slimBuild` writes the ids verbatim,
 * so the only way one comes back is a file authored before 2026-03-21 — which is the case
 * above. Stated so a future writer that starts composing accolade ids trips here first.
 */
describe('the current spelling survives a round trip', () => {
  it('slimBuild → hydrateBuild leaves a current id alone', () => {
    const build: Build = createEmptyBuild('homecoming');
    build.accolades = [...CURRENT];
    expect(hydrateBuild(slimBuild(build) as Record<string, unknown>).accolades).toEqual(CURRENT);
  });
});
