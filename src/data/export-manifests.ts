/**
 * The export-manifest map — shared by the two guards that read it.
 *
 * TEST-ONLY infrastructure: this module touches `node:path` and the committed
 * `exported_powers/` tree, so it must never be imported from app code (it is
 * deliberately absent from the `src/data` barrel).
 *
 * Each exporter under `tools/bin-crawler` stamps its output dir with an
 * `_export_manifest.json`. Two independent questions are asked of those files:
 *
 *   - `export-staleness.test.ts`  — was this tree produced by the exporter
 *     source that is committed now? (parser→export currency)
 *   - `export-provenance.test.ts` — which assets tree were the bytes read
 *     from? (DATA-GAP-REGISTER PROV-1)
 *
 * The map of surface→dataset→manifest path lives here so the two guards cannot
 * drift into disagreeing about which trees exist.
 */
import { createHash } from 'node:crypto';
import { readFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, dirname, basename, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

export const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const PKG_ROOT = join(REPO_ROOT, 'tools', 'bin-crawler', 'bin_crawler');
export const EXPORTED = join(REPO_ROOT, 'exported_powers');

/**
 * The canonical assets-source registry the exporters resolve `--source`
 * against (`bin_crawler/assets_sources.json`). The guards read the same file so
 * a tree is named in exactly one place — the alternative is a second copy of
 * the paths here, drifting from the one the exporters actually use.
 *
 * Schema 2 splits a tree into the part that varies by machine and the part that
 * does not: each dataset lists every ROOT it has been seen at (one per
 * workstation, possibly written home-relative), and each ring names only its
 * SUBPATH inside that root. This mirrors `bin_crawler/assets_sources.py` — the
 * two resolutions must stay in step, so change them together.
 */
interface Root {
  host: string;
  path: string;
  note?: string;
}
interface Ring {
  subpath: string;
  note?: string;
}
interface DatasetEntry {
  exportable_ring: string;
  roots: Root[];
  rings: Record<string, Ring>;
}
interface AssetsSources {
  datasets: Record<string, DatasetEntry>;
}

export const ASSETS_SOURCES: AssetsSources = JSON.parse(
  readFileSync(join(PKG_ROOT, 'assets_sources.json'), 'utf-8'),
);


/** A path as the Python records it: POSIX, relative to `root`. */
export function posixRel(root: string, file: string): string {
  return relative(root, file).split('\\').join('/');
}

/**
 * sha256 over a set of files — the TS half of `_export_fingerprint._fold`.
 *
 * Sorted by relpath, folded as `relpath\0<bytes>\0`. Both guards use it: the
 * staleness guard over the exporter's SOURCE, the contents guard over the
 * export's OUTPUT. The Python has exactly one implementation for the same four
 * uses, and these two must match it byte for byte — a divergence surfaces as a
 * permanently-red guard, never a silent gap, which is why it lives here once
 * rather than in each test.
 */
export function foldFiles(entries: { rel: string; bytes: Buffer }[]): string {
  const ordered = [...entries].sort((a, b) => (a.rel < b.rel ? -1 : a.rel > b.rel ? 1 : 0));
  const h = createHash('sha256');
  for (const e of ordered) {
    h.update(e.rel, 'utf-8');
    h.update(Buffer.from([0]));
    h.update(e.bytes);
    h.update(Buffer.from([0]));
  }
  return h.digest('hex');
}

const posix = (p: string) => p.split('\\').join('/');
const expandHome = (p: string) => (p.startsWith('~/') ? join(homedir(), p.slice(2)) : p);

function datasetEntry(dataset: string): DatasetEntry {
  const entry = ASSETS_SOURCES.datasets[dataset];
  if (!entry) throw new Error(`assets_sources.json names no dataset '${dataset}'`);
  return entry;
}

/**
 * The install root for a dataset ON THIS MACHINE — whichever registered root
 * exists here. Mirrors `assets_sources.root_report`: exactly one present is the
 * normal case, and with none present the first candidate is handed back so a
 * caller's own "does not exist" message still names a registered path.
 */
function resolvedRoot(entry: DatasetEntry): string {
  const candidates = entry.roots.map((r) => expandHome(r.path));
  return candidates.find((p) => existsSync(p)) ?? candidates[0];
}

/** The tree a committed export of `dataset` must have been read from. */
export function canonicalSource(dataset: string): {
  path: string;
  shard: string;
  ring: string;
  subpath: string;
  roots: Root[];
} {
  const entry = datasetEntry(dataset);
  const ring = entry.exportable_ring;
  const { subpath } = entry.rings[ring];
  return {
    path: posix(join(resolvedRoot(entry), subpath)),
    shard: basename(subpath),
    ring,
    subpath,
    roots: entry.roots,
  };
}

/**
 * Does `assetsDir` sit at <some registered root>/<subpath>?
 *
 * This is PROV-2's copy-detection without pinning the answer to one
 * workstation: a manifest stamped `/home/…/.wine/…/assets/live` and one stamped
 * `/Users/…/Library/…/assets/live` each satisfy their own machine's root, while
 * a snapshot copied to `bins/…/tspy` matches no root at all and still fails.
 * Home-relative roots compare by suffix, so neither guard has to know whose home
 * directory the export ran in.
 */
export function matchesRegisteredRoot(
  assetsDir: string,
  dataset: string,
  subpath: string,
): boolean {
  const actual = posix(assetsDir).replace(/\/+$/, '');
  return datasetEntry(dataset).roots.some((root) => {
    const rootPath = posix(root.path);
    const homeRelative = rootPath.startsWith('~/');
    const expected = `${homeRelative ? rootPath.slice(2) : rootPath}/${subpath}`;
    if (!homeRelative) return actual === expected;
    return actual === expected || actual.endsWith(`/${expected}`);
  });
}

/**
 * One entry per exporter surface. `exporterFile` is the entry module appended
 * to the shared `parser/**\/*.py` glob (mirrors parser_fingerprint /
 * classes_fingerprint / entities_fingerprint in _export_fingerprint.py);
 * `manifestKey` is the fingerprint field each exporter writes. The surfaces
 * intentionally share the whole parser glob, so a parser edit invalidates all
 * of them — the accepted over-coverage documented in _export_fingerprint.py.
 *
 * Per-dataset manifests: HC lives at the tree root; rebirth/thunderspy are
 * nested under their assets-dir basenames, mirroring each explicit --output-dir.
 */
export const SURFACES = [
  {
    tree: 'exported_powers',
    exporterFile: 'export_powers.py',
    manifestKey: 'parser_fingerprint',
    reexport: (ds: string, sub: string) =>
      `py -3 -m bin_crawler.export_powers --assets-dir <${ds} pigg dir> --output-dir exported_powers${sub}`,
    manifests: {
      homecoming: join(EXPORTED, '_export_manifest.json'),
      brainstorm: join(EXPORTED, 'brainstorm', '_export_manifest.json'),
      rebirth: join(EXPORTED, 'rebirth', '_export_manifest.json'),
      thunderspy: join(EXPORTED, 'thunderspy', '_export_manifest.json'),
    },
  },
  {
    tree: 'exported_powers/tables',
    exporterFile: 'export_classes.py',
    manifestKey: 'classes_fingerprint',
    reexport: (ds: string, sub: string) =>
      `py -3 -m bin_crawler.export_classes --assets-dir <${ds} pigg dir> --output-dir exported_powers${sub}/tables`,
    manifests: {
      homecoming: join(EXPORTED, 'tables', '_export_manifest.json'),
      brainstorm: join(EXPORTED, 'brainstorm', 'tables', '_export_manifest.json'),
      rebirth: join(EXPORTED, 'rebirth', 'tables', '_export_manifest.json'),
      thunderspy: join(EXPORTED, 'thunderspy', 'tables', '_export_manifest.json'),
    },
  },
  {
    tree: 'exported_powers/entities',
    exporterFile: 'export_entities.py',
    manifestKey: 'entities_fingerprint',
    reexport: (ds: string, sub: string) =>
      `py -3 -m bin_crawler.export_entities --assets-dir <${ds} pigg dir> --output-dir exported_powers${sub}/entities`,
    manifests: {
      homecoming: join(EXPORTED, 'entities', '_export_manifest.json'),
      brainstorm: join(EXPORTED, 'brainstorm', 'entities', '_export_manifest.json'),
      rebirth: join(EXPORTED, 'rebirth', 'entities', '_export_manifest.json'),
      thunderspy: join(EXPORTED, 'thunderspy', 'entities', '_export_manifest.json'),
    },
  },
  {
    // Homecoming-install only: Rebirth/Thunderspy piggs carry no salvage.bin, so
    // export_salvage early-returns for them and stamps no manifest. Brainstorm is the
    // same install one ring over, so it DOES ship salvage — the omission here is about
    // which game the bytes come from, not which dataset is first-class.
    // The manifest is a sibling file (salvage.json is a flat file, and the tree
    // root's _export_manifest.json already belongs to the powers surface).
    tree: 'exported_powers/salvage.json',
    exporterFile: 'export_salvage.py',
    manifestKey: 'salvage_fingerprint',
    reexport: () =>
      `py -3 -m bin_crawler.export_salvage --assets-dir <homecoming pigg dir>`,
    manifests: {
      // dataset-absent: rebirth, thunderspy — their piggs carry no `salvage.bin`, so
      // `export_salvage` early-returns and stamps no manifest. The note on this entry has the
      // whole of it; the marker sits on the rows because a reason twelve lines up is a reason
      // the next editor of the rows does not read (BRAIN-12).
      homecoming: join(EXPORTED, 'salvage_export_manifest.json'),
      brainstorm: join(EXPORTED, 'brainstorm', 'salvage_export_manifest.json'),
    },
  },
  {
    tree: 'exported_powers/incarnate-recipes.json',
    exporterFile: 'export_incarnate_recipes.py',
    manifestKey: 'incarnate_recipes_fingerprint',
    reexport: (ds: string) =>
      `python3 -m bin_crawler.export_incarnate_recipes --source ${ds}`,
    manifests: {
      homecoming: join(EXPORTED, 'incarnate_recipes_export_manifest.json'),
      brainstorm: join(EXPORTED, 'brainstorm', 'incarnate_recipes_export_manifest.json'),
      rebirth: join(EXPORTED, 'rebirth', 'incarnate_recipes_export_manifest.json'),
      thunderspy: join(EXPORTED, 'thunderspy', 'incarnate_recipes_export_manifest.json'),
    },
  },
] as const;

export type Dataset = 'homecoming' | 'rebirth' | 'thunderspy' | 'brainstorm';
