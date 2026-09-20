"""Export the salvage catalog (salvage.bin) as JSON for the planner.

Emits one record per salvage item with name, resolved display name, icon,
rarity, and category (invention / base / reward / incarnate). The planner's
`convert-salvage.cjs` consumes this to generate the incarnate + invention
salvage registries.

salvage.bin is HC-only (Rebirth's pigg has no salvage.bin), so this exports a
single flat file: exported_powers/salvage.json.

Usage:
  py -3 -m bin_crawler.export_salvage [--assets-dir DIR] [--output DIR]
"""
import argparse
import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bin_crawler.parser._pigg import BinResolver
from bin_crawler.parser._salvage import parse_salvage
from bin_crawler.parser._messages import load_messages
from bin_crawler.assets_dir import add_source_arguments, resolve_export_source
from bin_crawler._export_fingerprint import salvage_fingerprint
from bin_crawler._export_digest import ExportTree


def main():
    ap = argparse.ArgumentParser(description="Export salvage catalog as JSON")
    add_source_arguments(ap)
    ap.add_argument("--output", default=None,
                    help="Output JSON path (default: ./exported_powers/salvage.json)")
    args = ap.parse_args()

    assets_dir = resolve_export_source(args)

    out_file = Path(args.output) if args.output else Path("./exported_powers/salvage.json")
    out_file.parent.mkdir(parents=True, exist_ok=True)

    resolver = BinResolver(assets_dir)
    print(f"Source: {resolver.source_description}", flush=True)

    if not resolver.has("salvage.bin"):
        print("salvage.bin not found (Rebirth has none) — nothing to export.")
        return

    messages = None
    if resolver.has("clientmessages-en.bin"):
        messages = load_messages(resolver.read("clientmessages-en.bin"))
        print(f"Loaded {len(messages)} client messages for display-name resolution.")

    records = parse_salvage(resolver.read("salvage.bin"), messages=messages)
    by_cat = {}
    for r in records:
        by_cat[r.category] = by_cat.get(r.category, 0) + 1
    print(f"Parsed {len(records)} salvage records: {by_cat}")

    # Every written file goes through the tree, which records it for
    # `content_digest` — the guard that the committed bytes are the exported
    # ones (F78/PROV-1). See _export_digest.py.
    tree = ExportTree(out_file.parent)
    out = {"salvage": [asdict(r) for r in records]}
    tree.write_json(out_file, out, indent=2)
    print(f"Wrote {out_file}")

    # Stamp the export-staleness manifest, exactly as the powers/classes/entities
    # exporters do. CI has neither the .pigg archives nor Python, so salvage.json
    # can't be regenerate-and-diffed there; the fingerprint is the only cross-check
    # that this file was produced by the currently-committed salvage exporter and
    # not left stale after a parser edit (the INHERENT-3 residual after WS-ENT
    # guarded `entities/`). Salvage is HC-only, so this runs only where salvage.bin
    # exists — the early-return above skips it for Rebirth/Thunderspy, which carry
    # no manifest. Guarded by src/data/export-staleness.test.ts. See
    # _export_fingerprint.py.
    manifest = {
        "schema": "bin-crawler-export-manifest/3",
        "note": ("salvage_fingerprint is the sha256 of the salvage exporter "
                 "(every .py in bin_crawler) at export time. If it disagrees "
                 "with the current committed exporter source, THIS salvage.json "
                 "is stale — re-run export_salvage and commit. Guarded by "
                 "src/data/export-staleness.test.ts. `source` names the assets "
                 "shard the bytes were read from; guarded by "
                 "src/data/export-provenance.test.ts. `content_digest` is the "
                 "sha256 of the bytes this export WROTE; guarded by "
                 "src/data/export-contents.test.ts."),
        "salvage_fingerprint": salvage_fingerprint(),
        "source": resolver.provenance(),
        "content_digest": tree.digest(),
        "file_count": tree.file_count,
        "salvage_records": len(records),
    }
    manifest_file = out_file.parent / "salvage_export_manifest.json"
    tree.write_manifest(manifest_file, manifest)
    print(f"  Manifest: salvage_fingerprint={manifest['salvage_fingerprint'][:12]}…")


if __name__ == "__main__":
    main()
