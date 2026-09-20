"""Export pet entity data from villaindef.bin as CoD2-shape JSON.

Produces one JSON file per player-relevant pet entity, matching the schema
that `scripts/convert-pet-entities.cjs` expects (the same shape City of
Data 2.0 used to produce). Wildcards in `defaults.powers` are expanded
against `powersets.bin` so `defaults.power_full_names` matches what the
game actually grants.

Output:
  <output-dir>/<entity_name_lowercased>.json

Filtered to the prefixes the planner cares about: `Pets_*`,
`MastermindPets_*`, `IncarnatePets_*`, `Villain_Pets_*`. Skipping the
several thousand NPC/critter records keeps the output small and matches
the file set that `convert-pet-entities.cjs` already iterates.

Usage:
  py -3 -m bin_crawler.export_entities [--assets-dir DIR] [--output-dir DIR]

Defaults: --assets-dir G:/Homecoming/assets/live, output to
./exported_powers/<assets-dir-name>/entities/.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bin_crawler.parser._pigg import BinResolver
from bin_crawler.assets_dir import add_source_arguments, resolve_export_source
from bin_crawler.parser._entities import parse_entities, EntityRecord
from bin_crawler.parser._powersets import parse_powersets
from bin_crawler.parser._messages import load_messages
from bin_crawler._export_fingerprint import entities_fingerprint
from bin_crawler._export_digest import ExportTree
# Reuse the powers exporter's source-aware Primalist gating so the two export
# paths stay in lockstep. Importing is side-effect-free (export_powers guards
# its CLI behind `if __name__ == '__main__'`).
from bin_crawler.export_powers import (
    PRIMALIST_CATEGORIES,
    _source_has_primalist_class,
)


PET_PREFIXES = ("pets_", "mastermindpets_", "incarnatepets_", "villain_pets_")

# villaindef.bin filename varies by case across servers (HC ships
# `villaindef.bin`, Rebirth ships `VillainDef.bin`). Try both.
VILLAINDEF_CANDIDATES = ("villaindef.bin", "VillainDef.bin")


def _normalize_class(name: str) -> str:
    """`Class_Minion_Pets` -> `minion_pets` (matches CoD2's encoding)."""
    if name.lower().startswith("class_"):
        name = name[len("class_"):]
    return name.lower()


def _build_powerset_index(ps_records) -> dict[str, list[tuple[str, str]]]:
    """Return {`Cat.Set` -> [(short_name, display_name)]} for wildcard
    expansion. Only retains the Pets / Mastermind_Pets / Villain_Pets
    categories — that's where pet powers live.
    """
    index: dict[str, list[tuple[str, str]]] = {}
    for ps in ps_records:
        if not ps.powers:
            continue
        # ps.key is e.g. "Pets.Tornado", powers are full dotted names.
        powers = []
        for full in ps.powers:
            short = full.rsplit(".", 1)[-1]
            powers.append((short, short.replace("_", " ")))
        index[ps.key] = powers
    return index


def _expand_powers(rec: EntityRecord, ps_index: dict[str, list[tuple[str, str]]]):
    """Return parallel arrays (full_names, display_names, levels) with
    `*` wildcards expanded against the powerset's actual power list.
    """
    full_names: list[str] = []
    display_names: list[str] = []
    levels: list[int] = []
    for entry in rec.powers:
        if entry.power == "*":
            ps_key = f"{entry.power_category}.{entry.power_set}"
            for short, disp in ps_index.get(ps_key, []):
                full_names.append(f"{ps_key}.{short}")
                display_names.append(disp)
                levels.append(entry.level)
        else:
            full_names.append(f"{entry.power_category}.{entry.power_set}.{entry.power}")
            display_names.append(entry.power.replace("_", " "))
            levels.append(entry.level)
    return full_names, display_names, levels


def entity_to_dict(rec: EntityRecord, ps_index, msgs=None) -> dict:
    full_names, display_names, levels = _expand_powers(rec, ps_index)

    return {
        "name": rec.name,
        "group_name": rec.name.split("_")[0] if "_" in rec.name else "",
        "commandable_pet": int(rec.commandable_pet),
        "power_tags": rec.power_tags,
        "copy_creator_mods": bool(rec.copy_creator_mods),
        "can_zone": bool(rec.can_zone),
        "defaults": {
            "rank": rec.rank_raw,
            "ai_config": rec.ai_config,
            "character_class_name": _normalize_class(rec.character_class_name),
            "description": msgs.resolve(rec.description) if msgs else rec.description,
            "display_name": msgs.resolve(rec.display_name) if msgs else rec.display_name,
            "display_class_name": rec.display_class_name,
            "power_full_names": full_names,
            "power_display_names": display_names,
            "power_levels": levels,
            "powers": [
                {
                    "power_category": p.power_category,
                    "power_set": p.power_set,
                    "power": p.power,
                    "level": p.level,
                }
                for p in rec.powers
            ],
        },
        # `max_level` is null wherever the record states no upper bound: only
        # Homecoming widened `Level` into a range. No synthesized entry stands in
        # for an empty list — every record on all three forks carries real ones,
        # and inventing a 1-50 band was how Rebirth's unparsed levels block
        # looked like data (ENT-1).
        "levels": [
            {
                "level": L.level,
                "max_level": L.max_level,
                "experience": L.experience,
                "display_names": [msgs.resolve(d) if msgs else d for d in L.display_names],
                "costumes": L.costumes,
            }
            for L in rec.levels
        ],
        "source_file": rec.source_file,
    }


def main():
    ap = argparse.ArgumentParser(description="Export pet entity data as JSON")
    add_source_arguments(ap)
    ap.add_argument("--output-dir", default=None,
                    help="Output directory (default: ./exported_powers/<assets-dir-name>/entities)")
    args = ap.parse_args()

    assets_dir = resolve_export_source(args)

    if args.output_dir is None:
        source_name = Path(assets_dir).name or "export"
        output_dir = Path("./exported_powers") / source_name / "entities"
    else:
        output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    resolver = BinResolver(assets_dir)
    print(f"Source: {resolver.source_description}")
    print(f"Output: {output_dir}")

    villaindef_name = next((n for n in VILLAINDEF_CANDIDATES if resolver.has(n)), None)
    if villaindef_name is None:
        print("villaindef.bin not found in this assets dir.")
        return 1
    print(f"Reading {villaindef_name}...")
    entities = parse_entities(resolver.read(villaindef_name))
    print(f"  {len(entities)} entity records.")

    ps_records = parse_powersets(resolver.read("powersets.bin"))
    ps_index = _build_powerset_index(ps_records)
    print(f"  {len(ps_records)} powersets, {len(ps_index)} indexed for expansion.")

    msgs = None
    if resolver.has("clientmessages-en.bin"):
        msgs = load_messages(resolver.read("clientmessages-en.bin"))
        print(f"  {len(msgs)} messages loaded.")

    # Source-aware Primalist gating (mirrors export_powers): HC/Rebirth carry
    # ORPHAN Primalist pet entities (pets_primalwolf, the per-attack lifesteal
    # heal pseudo-pets) whose powers live in the Thunderspy-only Primalist
    # categories. Drop them unless the source defines a Primalist class, so the
    # leak that reached pet-entities.ts can't recur. Reference-based: an entity
    # is Primalist iff it references a power in a Primalist category — robust and
    # in lockstep with the powers-export discriminator.
    drop_primalist = not _source_has_primalist_class(resolver)
    if drop_primalist:
        print("  No Primalist class in source — dropping orphan Primalist pet entities.")

    # Every written file goes through the tree, which records it for
    # `content_digest` (F78/PROV-1). See _export_digest.py.
    tree = ExportTree(output_dir)
    written = 0
    skipped_no_powers = 0
    skipped_primalist = 0
    for rec in entities:
        if not rec.name.lower().startswith(PET_PREFIXES):
            continue
        if not rec.powers:
            skipped_no_powers += 1
            continue
        if drop_primalist and any(
            p.power_category in PRIMALIST_CATEGORIES for p in rec.powers
        ):
            skipped_primalist += 1
            continue
        d = entity_to_dict(rec, ps_index, msgs=msgs)
        out_file = output_dir / f"{rec.name.lower()}.json"
        tree.write_json(out_file, d, indent=2)
        written += 1

    print(f"\nWrote {written} pet entity files (skipped {skipped_no_powers} with "
          f"no powers, {skipped_primalist} orphan Primalist).")

    # Stamp the export-staleness manifest, exactly as export_powers.py /
    # export_classes.py do for their trees. CI has neither the .pigg archives nor
    # Python, so the `entities/` tree can't be regenerate-and-diffed there; the
    # fingerprint is the only cross-check that this tree was produced by the
    # currently-committed entities exporter and not left stale after a parser edit
    # (the INHERENT-3 residual after WS3 guarded `tables/`). Guarded by
    # src/data/export-staleness.test.ts. See _export_fingerprint.py.
    manifest = {
        'schema': 'bin-crawler-export-manifest/3',
        'note': ('entities_fingerprint is the sha256 of the entities exporter '
                 '(every .py in bin_crawler) at export time. If it disagrees '
                 'with the current committed exporter source, THIS entities/ '
                 'tree is stale — re-run export_entities for this dataset and '
                 'commit. Guarded by src/data/export-staleness.test.ts. '
                 '`source` names the assets shard the bytes were read from; '
                 'guarded by src/data/export-provenance.test.ts. '
                 '`content_digest` is the sha256 of the bytes this export '
                 'WROTE; guarded by src/data/export-contents.test.ts.'),
        'entities_fingerprint': entities_fingerprint(),
        'source': resolver.provenance(),
        'content_digest': tree.digest(),
        'file_count': tree.file_count,
        'entity_files': written,
    }
    tree.write_manifest(output_dir / '_export_manifest.json', manifest)

    print(f"  Manifest: entities_fingerprint={manifest['entities_fingerprint'][:12]}…")


if __name__ == "__main__":
    sys.exit(main() or 0)
