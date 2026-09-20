import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { relative } from 'node:path';
import {
  REPO_ROOT,
  SURFACES,
  canonicalSource,
  matchesRegisteredRoot,
  type Dataset,
} from './export-manifests';

/**
 * Export-provenance guard — which assets tree did these bytes come from?
 *
 * DATA-GAP-REGISTER PROV-1. Homecoming publishes several shards under one
 * `assets/` root, and they are divergent BRANCHES rather than points on one
 * timeline. An export taken from the wrong shard is therefore wrong in both
 * directions at once — missing shipped fixes while carrying unreleased content
 * — and because it is internally self-consistent it passes every check that
 * reads only the export: the staleness guard compares exporter fingerprints,
 * converter validation and contract totals read the export as ground truth.
 * That is the FLAGS-2 shape the mandate names, a bad read UPSTREAM of the
 * export becoming authoritative data.
 *
 * It shipped once (`8a32f60c42`): Homecoming exported from `assets/beta`
 * instead of `assets/live`, reverting a fix Sidekick had already published and
 * carrying an unreleased revamp, with every suite green. The honest diff DID
 * surface all 11 deltas — they were then explained away — so the diff alone was
 * never the guard. This is: the exporter now records the shard it read, and
 * here that record is held to the one shard users may see.
 */

// Bumped to /3 by F78's content-digest work: every manifest now also carries
// `content_digest` and `file_count` (guarded by export-contents.test.ts). The
// pin stays EXACT rather than becoming a >= 2 range — its job is to make a
// manifest written by an older exporter fail distinctly from a wrong-shard
// one, and a range would quietly readmit the shape this guard exists to
// reject. Schema 2 carried `source`, so this bump is about what a CURRENT
// manifest must look like, not about PROV-1's own floor.
const SCHEMA = 'bin-crawler-export-manifest/3';

interface SourceFile {
  name: string;
  bytes: number;
  modified: string;
  sha256: string;
}
interface Provenance {
  assets_dir: string;
  shard: string;
  sources: SourceFile[];
}

function readManifest(path: string): Record<string, unknown> {
  expect(
    existsSync(path),
    `Missing ${relative(REPO_ROOT, path)} — re-run its exporter to stamp it.`,
  ).toBe(true);
  return JSON.parse(readFileSync(path, 'utf-8'));
}

/** Every (dataset, surface, manifest) triple the map declares. */
const ENTRIES = SURFACES.flatMap((surface) =>
  Object.entries(surface.manifests).map(([dataset, manifestPath]) => ({
    dataset: dataset as Dataset,
    tree: surface.tree,
    manifestPath: manifestPath as string,
    reexport: surface.reexport(dataset, dataset === 'homecoming' ? '' : `/${dataset}`),
  })),
);

describe.each(ENTRIES)('export-provenance ($dataset, $tree)', (entry) => {
  const manifest = readManifest(entry.manifestPath);
  const where = relative(REPO_ROOT, entry.manifestPath);

  it('records provenance at all (manifest predates PROV-1 otherwise)', () => {
    expect(
      manifest.schema,
      `${where} is a pre-provenance manifest. It cannot say which assets tree ` +
        `produced this export, which is the whole of PROV-1. Re-export: ${entry.reexport}`,
    ).toBe(SCHEMA);
    expect(manifest.source, `${where} has no 'source' block.`).toBeTypeOf('object');
  });

  const canonical = canonicalSource(entry.dataset);

  it(`was read from the ${canonical.shard} ring`, () => {
    const source = manifest.source as Provenance;
    expect(
      source.shard,
      `${where} was exported from the '${source.shard}' ring (${source.assets_dir}), ` +
        `but ${entry.dataset} may only ship bytes from '${canonical.shard}'. ` +
        `A wrong-ring export is a divergent BRANCH, not merely stale — it drops shipped ` +
        `fixes and carries unreleased content at the same time. Re-export: ${entry.reexport}`,
    ).toBe(canonical.shard);
  });

  /**
   * The ring NAME does not identify a tree — PROV-2. A stale snapshot copied out
   * of a live install keeps its basename, so `bins/…/tspy` and the real `tspy`
   * are indistinguishable by ring alone, and the snapshot shipped a month-old
   * corpus while passing the check above. The path around the ring is what
   * separates them: a copy sits under no registered root.
   *
   * Matching is root-relative rather than one absolute path, because a committed
   * manifest may have been stamped on a different workstation than the one
   * running this test. Roots vary per machine; ring subpaths do not.
   */
  it('was read from a registered install, not a copy of one', () => {
    const source = manifest.source as Provenance;
    expect(
      matchesRegisteredRoot(source.assets_dir ?? '', entry.dataset, canonical.subpath),
      `${where} records assets_dir '${source.assets_dir}', which is not ` +
        `<a registered root>/${canonical.subpath} for ${entry.dataset}. A copy of an ` +
        `install keeps its ring name but goes stale in place.\n` +
        `  Registered roots: ${canonical.roots.map((r) => `${r.host} (${r.path})`).join(', ')}\n` +
        `  Either this was exported from an unregistered tree — the exact thing the ` +
        `registry exists to prevent — or this machine's root is missing from ` +
        `assets_sources.json. Re-export with --source ${entry.dataset}.`,
    ).toBe(true);
  });

  it('names the source files it read, with a digest for each', () => {
    const { sources } = manifest.source as Provenance;
    expect(Array.isArray(sources) && sources.length > 0, `${where} lists no sources.`).toBe(true);
    for (const file of sources) {
      expect(file.sha256, `${where}: ${file.name} has no sha256`).toMatch(/^[0-9a-f]{64}$/);
      expect(file.bytes, `${where}: ${file.name} is empty`).toBeGreaterThan(0);
      expect(file.modified, `${where}: ${file.name} has no mtime`).toMatch(
        /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/,
      );
    }
  });
});

/**
 * A dataset's surfaces are exported from one assets tree, so they must agree on
 * the bytes they read. Disagreement means the trees were built from different
 * game builds — the half-migrated state a per-tree check cannot see, since each
 * tree is individually self-consistent.
 */
describe('export-provenance cross-surface agreement', () => {
  const byDataset = new Map<Dataset, typeof ENTRIES>();
  for (const entry of ENTRIES) {
    byDataset.set(entry.dataset, [...(byDataset.get(entry.dataset) ?? []), entry]);
  }

  it.each([...byDataset.keys()])('%s: every surface read the same bytes', (dataset) => {
    const entries = byDataset.get(dataset)!;
    const fingerprints = entries.map((entry) => {
      const { sources } = readManifest(entry.manifestPath).source as Provenance;
      return {
        tree: entry.tree,
        digest: sources.map((f) => `${f.name}:${f.sha256}`).join('|'),
      };
    });
    const [first, ...rest] = fingerprints;
    for (const other of rest) {
      expect(
        other.digest,
        `${dataset}: ${other.tree} was exported from different source bytes than ` +
          `${first.tree}. The surfaces come from one assets tree, so this means they ` +
          `were built from different game builds — re-export ${dataset}'s surfaces together.`,
      ).toBe(first.digest);
    }
  });
});
