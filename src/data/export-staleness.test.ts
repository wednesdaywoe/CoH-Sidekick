import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { REPO_ROOT, PKG_ROOT, SURFACES, foldFiles, posixRel } from './export-manifests';

/**
 * Export-staleness guard — the parser→export currency gate.
 *
 * `exported_powers/<dataset>` (and its `tables/` subtree) is produced by the
 * Python bin parser reading the gitignored `.pigg` archives. CI has neither the
 * archives nor Python, so these trees — unlike the generated TS under
 * `src/data/datasets` (regenerated from committed `exported_powers` by the
 * regen-diff workflow) — cannot be regenerate-and-diffed. A parser change that
 * ships without a matching re-export leaves the committed JSON stale and every
 * downstream fix inert (the powers tree bit twice: the 2026-07-06 tspy hybrid
 * relabel; the incarnate one-dataset regen. The `tables/` tree was the WS3 gap
 * — in sync only because CLASSES-1 happened to regenerate it, guarded by
 * nothing going forward).
 *
 * The cross-check: each exporter stamps its output dir with an
 * `_export_manifest.json` recording the fingerprint of that exporter's SOURCE
 * at export time (every .py in the bin_crawler package). Here we recompute that
 * fingerprint from the committed sources and assert every dataset's manifest
 * matches. If the exporter changed but a dataset was not re-exported, its
 * recorded fingerprint diverges and this test goes red; the only fix is to
 * actually re-export that dataset (which re-stamps).
 *
 * The glob covers the WHOLE package, not `parser/` plus one entry module as it
 * did until F78. The helpers beside the exporters — `path_safety.py`,
 * `assets_dir.py` — decide what gets written and were outside it, so a change
 * to the path rule or the source resolver changed the export with nothing to
 * notice. All five fingerprints are therefore equal by construction now; the
 * five names remain because each still answers a per-surface question.
 *
 * The fold MUST replicate bin_crawler/_export_fingerprint._fold byte for byte;
 * it lives once in export-manifests.ts and is shared with the contents guard,
 * exactly as the Python shares `_fold`. A divergence between the two
 * implementations surfaces as a permanently-red guard, never a silent gap.
 *
 * This guard answers only "was this tree produced by the committed exporter?".
 * Which assets tree the BYTES came from is guarded by export-provenance.test.ts,
 * and whether the committed bytes are still the ones that export WROTE is
 * guarded by export-contents.test.ts. All three read the manifest map in
 * export-manifests.ts.
 */

function walkPy(dir: string): string[] {
  const out: string[] = [];
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    if (ent.name === '__pycache__') continue;
    const full = join(dir, ent.name);
    if (ent.isDirectory()) out.push(...walkPy(full));
    else if (ent.name.endsWith('.py')) out.push(full);
  }
  return out;
}

/** sha256 of an exporter's source — mirrors _fingerprint() in the Python. */
function computeFingerprint(): string {
  return foldFiles(walkPy(PKG_ROOT).map((f) => ({ rel: posixRel(PKG_ROOT, f), bytes: readFileSync(f) })));
}

describe.each(SURFACES)('export-staleness guard ($tree)', (surface) => {
  const expected = computeFingerprint();

  it('computes a stable 64-hex fingerprint from the committed exporter source', () => {
    expect(expected).toMatch(/^[0-9a-f]{64}$/);
  });

  for (const [dataset, manifestPath] of Object.entries(surface.manifests)) {
    const sub = dataset === 'homecoming' ? '' : `/${dataset}`;
    const treeDir = relative(REPO_ROOT, dirname(manifestPath));
    it(`${dataset}: ${surface.tree} matches the current parser (not stale)`, () => {
      expect(
        existsSync(manifestPath),
        `Missing ${relative(REPO_ROOT, manifestPath)} — export ${dataset} with ${surface.exporterFile} to stamp it.`,
      ).toBe(true);
      const manifest = JSON.parse(readFileSync(manifestPath, 'utf-8'));
      expect(
        manifest[surface.manifestKey],
        `${treeDir} is STALE: it was produced by a different bin_crawler exporter ` +
          `than what is committed now. Re-run the export for ${dataset} (${surface.reexport(dataset, sub)}) ` +
          `and commit the refreshed tree.`,
      ).toBe(expected);
    });
  }
});
