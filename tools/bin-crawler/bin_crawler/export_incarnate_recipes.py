"""Export the incarnate crafting recipes (baserecipes.bin) as JSON.

Scope: every DetailRecipe that names an `IncarnateReward` (the craft recipes —
all three families: the current path, the legacy shard path, and the PvP
grants), plus every other recipe sharing a workshop with one of those (the
incarnate worktable's `Conversion|…` tab — the thread/merit salvage store and
the breakdown/sidegrade rows). The workshop join is the scope rule so no
English tab name is ever matched.

Every parsed field of each in-scope record is emitted verbatim; the P-hash
display fields additionally get `*_resolved` twins via clientmessages-en.bin.
Which recipes a planner surface should *use* is a converter decision
(`scripts/emit-contract.cjs`), not made here.

All three forks ship baserecipes.bin, so this export is dataset-namespaced like
the powers tree: homecoming writes `exported_powers/incarnate-recipes.json`,
the forks write `exported_powers/<dataset>/incarnate-recipes.json`. Each output
dir is stamped with `incarnate_recipes_export_manifest.json` for the
export-staleness guard.

Usage:
  python3 -m bin_crawler.export_incarnate_recipes --source homecoming
  python3 -m bin_crawler.export_incarnate_recipes --source rebirth
  python3 -m bin_crawler.export_incarnate_recipes --source thunderspy
"""
import argparse
import json
import sys
from dataclasses import asdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bin_crawler.parser._pigg import BinResolver
from bin_crawler.parser._recipes import parse_recipes
from bin_crawler.parser._messages import load_messages
from bin_crawler.assets_dir import add_source_arguments, resolve_export_source
from bin_crawler.assets_sources import dataset_for_path
from bin_crawler._export_fingerprint import incarnate_recipes_fingerprint
from bin_crawler._export_digest import ExportTree

_RESOLVED_FIELDS = ("display_name", "display_help", "display_short_help",
                    "display_tab_name")


def _in_scope(records):
    """Craft recipes plus everything sharing their workshops (the store)."""
    craft = [r for r in records if r.incarnate_reward]
    workshops = {w for r in craft for w in r.workshops}
    return [r for r in records
            if r.incarnate_reward or (workshops & set(r.workshops))]


def _default_output(assets_dir: str) -> Path:
    known = dataset_for_path(assets_dir)
    if known is None:
        raise SystemExit(
            f"{assets_dir} is not a registered dataset ring — pass --output "
            f"explicitly for an unregistered tree")
    dataset, _ring = known
    base = Path("./exported_powers")
    if dataset != "homecoming":
        base = base / dataset
    return base / "incarnate-recipes.json"


def main():
    ap = argparse.ArgumentParser(
        description="Export incarnate crafting recipes as JSON")
    add_source_arguments(ap)
    ap.add_argument("--output", default=None,
                    help="Output JSON path (default: per-dataset under "
                         "./exported_powers)")
    args = ap.parse_args()

    assets_dir = resolve_export_source(args)
    out_file = Path(args.output) if args.output else _default_output(assets_dir)
    out_file.parent.mkdir(parents=True, exist_ok=True)

    resolver = BinResolver(assets_dir)
    print(f"Source: {resolver.source_description}", flush=True)
    if not resolver.has("baserecipes.bin"):
        raise SystemExit("baserecipes.bin not found — nothing to export.")

    messages = None
    if resolver.has("clientmessages-en.bin"):
        messages = load_messages(resolver.read("clientmessages-en.bin"))
        print(f"Loaded {len(messages)} client messages.")

    records = parse_recipes(resolver.read("baserecipes.bin"))
    scoped = _in_scope(records)
    n_craft = sum(1 for r in scoped if r.incarnate_reward)
    print(f"Parsed {len(records)} recipes; in scope {len(scoped)} "
          f"({n_craft} incarnate craft, {len(scoped) - n_craft} workshop-shared).")

    out_records = []
    for rec in scoped:
        d = asdict(rec)
        for f in _RESOLVED_FIELDS:
            key = d[f]
            d[f + "_resolved"] = messages.resolve(key) if (messages and key) else key
        out_records.append(d)

    # indent=1 is this surface's own formatting and must survive the move
    # behind the write boundary — the digest is over bytes (F78/PROV-1).
    tree = ExportTree(out_file.parent)
    out = {"recipes": out_records}
    tree.write_json(out_file, out, indent=1)
    print(f"Wrote {out_file} ({len(out_records)} records)")

    manifest = {
        "schema": "bin-crawler-export-manifest/3",
        "incarnate_recipes_fingerprint": incarnate_recipes_fingerprint(),
        "source": resolver.provenance(),
        "content_digest": tree.digest(),
        "file_count": tree.file_count,
        "incarnate_recipes_records": len(out_records),
    }
    manifest_file = out_file.parent / "incarnate_recipes_export_manifest.json"
    tree.write_manifest(manifest_file, manifest)
    print(f"Stamped {manifest_file}")


if __name__ == "__main__":
    main()
