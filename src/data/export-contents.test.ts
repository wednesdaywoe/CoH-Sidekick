import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { REPO_ROOT, SURFACES, foldFiles, posixRel } from './export-manifests';

/**
 * Export-contents guard — the trust-root gate (SECURITY_AUDIT F78, PROV-1).
 *
 * The two older guards ask about the export's PROVENANCE: staleness asks which
 * exporter ran, provenance asks which assets tree it read. Neither asks the
 * question `exported_powers/` most needs asked — are the bytes sitting in the
 * tree right now the bytes that export wrote?
 *
 * Nothing hashed them. The converters, contract totals, regen-diff and the
 * fixtures all consume this tree as ground truth and are self-consistent with
 * whatever they find in it, so a power file edited after the export is
 * indistinguishable from a real parse. That is the FLAGS-2 shape one layer
 * below the parser: not a bad read becoming authoritative, but a good read
 * quietly amended afterwards. `path_safety.py` named the same hole from the
 * other side — a bad write "leaves no trace to find afterwards either".
 *
 * Each exporter now records `content_digest` over every file it wrote, through
 * the one write boundary in `_export_digest.ExportTree`. Here we rederive that
 * set from the committed tree and refold it. An edited, added, or deleted file
 * moves the digest and this goes red; the only green path back is a re-export.
 *
 * WHAT THIS IS NOT. It is not an attestation — a single committer can always
 * re-export or re-stamp, and no gate in a one-writer repo can prevent that.
 * What it removes is the SILENT case: partial re-exports, stray scripts, bad
 * merges, and orphans left behind when a category stops existing. Afterwards
 * those are a red gate rather than new ground truth, and a deliberate
 * amendment has to carry a changed digest in the same diff where a reviewer
 * can see it.
 *
 * OWNERSHIP IS A RULE, NOT A LIST. The surfaces nest: `exported_powers/` holds
 * `tables/`, `entities/` and the three sibling datasets, each with its own
 * manifest. A file belongs to the surface whose manifest directory is its
 * NEAREST enclosing one. That is derived from the manifest map rather than
 * written down as an exclusion list, because a hand-maintained list of which
 * subtree belongs to whom is precisely what this project keeps burning on.
 * Manifests are excluded — a file cannot carry its own hash — and the two flat
 * single-file surfaces (`salvage.json`, `incarnate-recipes.json`) are claimed
 * by name.
 *
 * `file_count` is asserted before the digest on purpose. Both catch the same
 * failures, but a count mismatch says "17 files too many" where a digest
 * mismatch says only "different", and the first is a bug report.
 */

/** Every tree-owning directory: one per manifest, across all surfaces. */
const OWNER_DIRS: string[] = [
  ...new Set(
    SURFACES.flatMap((s) => Object.values(s.manifests).map((m) => dirname(m as string))),
  ),
];

/** The flat single-file surfaces, claimed by name rather than by directory. */
const FLAT_FILES = new Set(
  SURFACES.filter((s) => s.tree.endsWith('.json')).flatMap((s) =>
    Object.values(s.manifests).map((m) =>
      join(dirname(m as string), s.tree.split('/').pop() as string),
    ),
  ),
);

const MANIFEST_NAMES = new Set(
  SURFACES.flatMap((s) => Object.values(s.manifests).map((m) => (m as string).split('/').pop() as string)),
);

function walkFiles(dir: string): string[] {
  const out: string[] = [];
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, ent.name);
    if (ent.isDirectory()) out.push(...walkFiles(full));
    else if (ent.isFile()) out.push(full);
  }
  return out;
}

/** The owner dir of `file`: the deepest tree-owning directory containing it. */
function ownerOf(file: string): string | undefined {
  let best: string | undefined;
  for (const d of OWNER_DIRS) {
    if (file.startsWith(d + '/') && (!best || d.length > best.length)) best = d;
  }
  return best;
}

/**
 * The files a surface's manifest claims: everything under its directory whose
 * nearest enclosing manifest dir is that directory, minus manifests and minus
 * the flat-file surfaces that sit beside it.
 */
function territory(manifestPath: string, tree: string): string[] {
  const root = dirname(manifestPath);
  if (tree.endsWith('.json')) {
    const f = join(root, tree.split('/').pop() as string);
    return existsSync(f) ? [f] : [];
  }
  return walkFiles(root).filter(
    (f) =>
      ownerOf(f) === root &&
      !MANIFEST_NAMES.has(f.split('/').pop() as string) &&
      !FLAT_FILES.has(f),
  );
}

describe.each(SURFACES)('export-contents guard ($tree)', (surface) => {
  for (const [dataset, manifestPath] of Object.entries(surface.manifests)) {
    const sub = dataset === 'homecoming' ? '' : `/${dataset}`;
    const root = dirname(manifestPath as string);
    const treeDir = relative(REPO_ROOT, root);

    it(`${dataset}: ${surface.tree} holds the bytes the export wrote`, () => {
      expect(
        existsSync(manifestPath as string),
        `Missing ${relative(REPO_ROOT, manifestPath as string)} — export ${dataset} to stamp it.`,
      ).toBe(true);
      const manifest = JSON.parse(readFileSync(manifestPath as string, 'utf-8'));

      expect(
        manifest.content_digest,
        `${treeDir} has no content_digest: it predates the F78 trust-root guard ` +
          `(schema 2). Re-export ${dataset} (${surface.reexport(dataset, sub)}) to stamp one.`,
      ).toMatch(/^[0-9a-f]{64}$/);

      const files = territory(manifestPath as string, surface.tree);

      expect(
        files.length,
        `${treeDir} holds ${files.length} files but its export wrote ${manifest.file_count}. ` +
          `Files were added to or removed from the tree after it was exported — the ` +
          `usual causes are a partial re-export, or an orphan left behind by something ` +
          `that stopped being exported. Re-export ${dataset} (${surface.reexport(dataset, sub)}).`,
      ).toBe(manifest.file_count);

      const digest = foldFiles(
        files.map((f) => ({ rel: posixRel(root, f), bytes: readFileSync(f) })),
      );

      expect(
        digest,
        `${treeDir} does NOT hold the bytes its export wrote — a file under it was ` +
          `edited after export. This tree is a trust root: every converter, the ` +
          `contract and the fixtures take it as ground truth, so an edit here is ` +
          `indistinguishable from a real parse downstream. Re-export ${dataset} ` +
          `(${surface.reexport(dataset, sub)}) rather than re-stamping.`,
      ).toBe(manifest.content_digest);
    });
  }
});
