"""Export per-archetype/per-pet-class data from classes.bin and
villain_classes.bin as JSON files.

Output shape matches the legacy CoD2 `tables/<at>.json` files that the
planner's `scripts/extract-at-tables.cjs` consumes:

    {
      "primary_category": "Blaster_RANGED",
      "secondary_category": "Blaster_SUPPORT",
      "named_tables": { "Melee_Damage": [...50 or 105 floats...], ... }
    }

Usage:
  py -3 -m bin_crawler.export_classes [--assets-dir DIR] [--output-dir DIR]

Defaults: --assets-dir G:/Homecoming/assets/live, output to
./exported_powers/<assets-dir-name>/tables/.
"""

import argparse
import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bin_crawler.parser._pigg import BinResolver
from bin_crawler.assets_dir import add_source_arguments, resolve_export_source
from bin_crawler.parser._classes import parse_classes
from bin_crawler.parser._messages import load_messages
from bin_crawler._export_fingerprint import classes_fingerprint
from bin_crawler._export_digest import ExportTree


def _normalize_class_name(name: str) -> str:
    """`Class_Blaster` -> `blaster`, `Class_Minion_Pets` -> `minion_pets`."""
    if name.lower().startswith("class_"):
        name = name[len("class_"):]
    return name.lower()


def main():
    ap = argparse.ArgumentParser(description="Export AT class/pet tables as JSON")
    add_source_arguments(ap)
    ap.add_argument("--output-dir", default=None,
                    help="Output directory (default: ./exported_powers/<assets-dir-name>/tables)")
    args = ap.parse_args()

    assets_dir = resolve_export_source(args)

    if args.output_dir is None:
        source_name = Path(assets_dir).name or "export"
        output_dir = Path("./exported_powers") / source_name / "tables"
    else:
        output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    resolver = BinResolver(assets_dir)
    print(f"Source: {resolver.source_description}", flush=True)
    print(f"Output: {output_dir}", flush=True)

    msgs = load_messages(resolver.read("clientmessages-en.bin")) if resolver.has("clientmessages-en.bin") else None

    # Every written file goes through the tree, which records it for
    # `content_digest` (F78/PROV-1). See _export_digest.py.
    tree = ExportTree(output_dir)
    written = 0
    # Guard the normalize-to-filename step against a silent last-write-wins drop.
    # The forks' villain_classes.bin ships two `Class_Minion_Henchman` records
    # that both normalize to `minion_henchman.json`; without this, the second
    # `write_text` overwrites the first with no trace. Per the fail-loud mandate a
    # dropped record must never vanish silently: an identical twin is a benign,
    # announced skip; a *differing* collision aborts the export.
    written_payloads: dict[str, str] = {}  # key -> serialized JSON already written

    for bin_name in ("classes.bin", "villain_classes.bin"):
        if not resolver.has(bin_name):
            print(f"  {bin_name}: not found, skipping")
            continue

        records = parse_classes(resolver.read(bin_name))
        print(f"  {bin_name}: {len(records)} records")

        for c in records:
            key = _normalize_class_name(c.name)
            if not key:
                continue
            out_file = output_dir / f"{key}.json"
            display_name = msgs.resolve(c.display_name) if msgs else c.display_name
            display_help = msgs.resolve(c.display_help) if msgs else c.display_help
            display_short_help = msgs.resolve(c.display_short_help) if msgs else c.display_short_help
            out = {
                "name": c.name,
                "display_name": display_name,
                "display_help": display_help,
                "display_short_help": display_short_help,
                "icon": c.icon,
                "primary_category": c.primary_category,
                "secondary_category": c.secondary_category,
                "pool_category": c.pool_category,
                "epic_pool_category": c.epic_pool_category,
                "allowed_origins": c.allowed_origins,
                "special_restrictions": c.special_restrictions,
                "store_requires": c.store_requires,
                "locked_tooltip": msgs.resolve(c.locked_tooltip) if msgs else c.locked_tooltip,
                "product_code": c.product_code,
                "reduction_class": c.reduction_class,
                "reduce_as_archvillain": c.reduce_as_archvillain,
                "level_up_respecs": c.level_up_respecs,
                "archetype_shots": c.archetype_shots,
                "creation_stats": c.creation_stats,
                "playstyle_flags": c.playstyle_flags,
                "mechanic_tip": c.mechanic_tip,
                "named_tables": c.named_tables,
                "attribs": c.attribs,
            }
            # Schema-dependent fields: present only on datasets that
            # serialize them (see _classes.py module docstring).
            if c.villain_rank is not None:
                out["villain_rank"] = c.villain_rank
            if c.mechanic_bar_raw is not None:
                out["mechanic_bar_raw"] = c.mechanic_bar_raw
            if c.mechanic_gap_raw is not None:
                out["mechanic_gap_raw"] = c.mechanic_gap_raw
            if c.tail_scalar_raw is not None:
                out["tail_scalar_raw"] = round(c.tail_scalar_raw, 6)
            # Defaults everywhere today; exported only when they deviate so a
            # future data change surfaces instead of vanishing.
            if c.connect_hp_and_status:
                out["connect_hp_and_status"] = True
            if c.defiant_scale != 1.0:
                out["defiant_scale"] = c.defiant_scale
            if c.extra_raw:
                out["extra_raw"] = c.extra_raw
            payload = json.dumps(out, indent=2)
            prior = written_payloads.get(key)
            if prior is not None:
                if prior == payload:
                    print(f"  NOTE: '{c.name}' -> {key}.json duplicates an "
                          f"earlier byte-identical record; skipped (no data lost)")
                    continue
                raise SystemExit(
                    f"Class-name collision: '{c.name}' normalizes to "
                    f"'{key}.json', already written by a DIFFERENT record. "
                    f"Last-write-wins would silently drop one — resolve the "
                    f"source bin or the normalization before exporting."
                )
            written_payloads[key] = payload
            tree.write_text(out_file, payload)
            written += 1

    # Stamp the export-staleness manifest, exactly as export_powers.py does for
    # the powers tree. CI has neither the .pigg archives nor Python, so the
    # `tables/` tree can't be regenerate-and-diffed there; the fingerprint is the
    # only cross-check that this tree was produced by the currently-committed
    # classes exporter and not left stale after a parser edit (the WS3 gap).
    # Guarded by src/data/export-staleness.test.ts. See _export_fingerprint.py.
    manifest = {
        'schema': 'bin-crawler-export-manifest/3',
        'note': ('classes_fingerprint is the sha256 of the classes exporter '
                 '(every .py in bin_crawler) at export time. If it disagrees '
                 'with the current committed exporter source, THIS tables/ tree '
                 'is stale — re-run export_classes for this dataset and commit. '
                 'Guarded by src/data/export-staleness.test.ts. `source` names '
                 'the assets shard the bytes were read from; guarded by '
                 'src/data/export-provenance.test.ts. `content_digest` is the '
                 'sha256 of the bytes this export WROTE; guarded by '
                 'src/data/export-contents.test.ts.'),
        'classes_fingerprint': classes_fingerprint(),
        'source': resolver.provenance(),
        'content_digest': tree.digest(),
        'file_count': tree.file_count,
        'class_files': written,
    }
    tree.write_manifest(output_dir / '_export_manifest.json', manifest)

    print(f"Wrote {written} class JSON files to {output_dir}")
    print(f"  Manifest: classes_fingerprint={manifest['classes_fingerprint'][:12]}…")


if __name__ == "__main__":
    main()
