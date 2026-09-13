#!/usr/bin/env python3
"""
DSH9 bootstrap — Mids Reborn EnhDB.mhd reader.

Faithful Python port of MidsReborn's Enhancement DB binary load order:
  - Core/DatabaseAPI.cs: LoadEnhancementDb
  - Core/Enhancement.cs: Enhancement(BinaryReader)
  - Core/EnhancementSet.cs: EnhancementSet(BinaryReader)

This reader is intentionally structural-first: it preserves enhancement/set
identity, proc flags, and set-bonus links (power-name/index pairs), and keeps
FX payloads at template granularity via the existing read_i12 Effect reader.

Usage:
  python3 read_enhdb.py
  python3 read_enhdb.py --mode enhancements --grep "winter"
  python3 read_enhdb.py --mode sets --out enhdb_sets.jsonl
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Any

import read_i12

HEADER = "Mids Reborn Enhancement Database"

# The Homecoming enhancement database, beside the main one `read_i12.DEFAULT_MHD` reads,
# so the two DSH legs are on one Mids install rather than on two by coincidence. This
# pointed into `MidsReborn-master/` until PROV-3 (2026-09-13); that tree still HOLDS an
# `EnhDB.mhd`, which is why nothing broke loudly — it is simply an older one
# (bf5b978e… against the prefix's b5b4379d…), so DSH9 was grading against a database
# nothing else here reads. The Rebirth pair is byte-identical between the two trees,
# which is what makes the Homecoming difference signal rather than noise.
DEFAULT_MHD = os.path.join(os.path.dirname(read_i12.DEFAULT_MHD), "EnhDB.mhd")

E_EFF_MODE = ["Enhancement", "FX", "PowerEnh", "PowerProc"]
E_BUFF_DEBUFF = ["Any", "BuffOnly", "DeBuffOnly"]
E_ENH_MUTEX = [
    "None",
    "Stealth",
    "ArchetypeA",
    "ArchetypeB",
    "ArchetypeC",
    "ArchetypeD",
    "ArchetypeE",
    "ArchetypeF",
]
E_TYPE = ["None", "Normal", "InventO", "SpecialO", "SetO"]
E_PVX = ["Any", "PvE", "PvP"]
E_ENHANCE = [
    "None",
    "Accuracy",
    "Damage",
    "Defense",
    "EnduranceDiscount",
    "Endurance",
    "SpeedFlying",
    "Heal",
    "HitPoints",
    "Interrupt",
    "JumpHeight",
    "SpeedJumping",
    "Mez",
    "Range",
    "RechargeTime",
    "X_RechargeTime",
    "Recovery",
    "Regeneration",
    "Resistance",
    "SpeedRunning",
    "ToHit",
    "Slow",
    "Absorb",
]


def _enum_name(table: list[str], value: int) -> str:
    if 0 <= value < len(table):
        return table[value]
    return f"Unknown({value})"


def _schedule_name(value: int) -> str:
    # Enums.eSchedule: None=-1, A=0, B=1, C=2, D=3, Multiple=4
    if value == -1:
        return "None"
    table = ["A", "B", "C", "D", "Multiple"]
    if 0 <= value < len(table):
        return table[value]
    return f"Unknown({value})"


def _read_fx_block(r: read_i12.Reader) -> dict[str, Any]:
    """Enhancement FX payload uses Effect(BinaryReader) and is marked as an
    enhancement-scoped effect in C# (isEnhancementEffect=true)."""
    fx = read_i12.read_effect(r)
    fx["is_enhancement_effect"] = True
    return fx


def _read_enhancement_effect(r: read_i12.Reader) -> dict[str, Any]:
    mode = r.int32()
    buff_mode = r.int32()
    enhance_id = r.int32()
    enhance_sub_id = r.int32()
    schedule = r.int32()
    multiplier = r.single()
    has_fx = r.boolean()
    out: dict[str, Any] = {
        "mode": _enum_name(E_EFF_MODE, mode),
        "buff_mode": _enum_name(E_BUFF_DEBUFF, buff_mode),
        "enhance": {
            "id": enhance_id,
            "id_name": _enum_name(E_ENHANCE, enhance_id),
            "sub_id": enhance_sub_id,
        },
        "schedule": _schedule_name(schedule),
        "multiplier": round(multiplier, 6),
    }
    if has_fx:
        out["fx"] = _read_fx_block(r)
    return out


def _read_enhancement(r: read_i12.Reader) -> dict[str, Any]:
    static_index = r.int32()
    name = r.string()
    short_name = r.string()
    desc = r.string()
    type_id = r.int32()
    sub_type_id = r.int32()

    class_ids = [r.int32() for _ in range(r.int32() + 1)]

    image = r.string()
    set_index = r.int32()
    uid_set = r.string()
    effect_chance = r.single()
    level_min = r.int32()
    level_max = r.int32()
    unique = r.boolean()
    mutex = r.int32()
    buff_mode = r.int32()

    effects = [_read_enhancement_effect(r) for _ in range(r.int32() + 1)]

    uid = r.string()
    recipe_name = r.string()
    superior = r.boolean()
    is_proc = r.boolean()
    is_scalable = r.boolean()

    out: dict[str, Any] = {
        "static_index": static_index,
        "name": name,
        "short_name": short_name,
        "desc": desc,
        "type": _enum_name(E_TYPE, type_id),
        "sub_type_id": sub_type_id,
        "class_ids": class_ids,
        "image": image,
        "set_index": set_index,
        "uid_set": uid_set,
        "effect_chance": round(effect_chance, 6),
        "level_min": level_min,
        "level_max": level_max,
        "unique": unique,
        "mutex": _enum_name(E_ENH_MUTEX, mutex),
        "buff_mode": _enum_name(E_BUFF_DEBUFF, buff_mode),
        "effects": effects,
        "uid": uid,
        "recipe_name": recipe_name,
        "superior": superior,
        "is_proc": is_proc,
        "is_scalable": is_scalable,
    }
    return out


def _read_bonus_item(r: read_i12.Reader, include_pv: bool) -> dict[str, Any]:
    special = r.int32()
    alt = r.string()
    pv_mode = None
    slotted = None
    if include_pv:
        pv_mode = _enum_name(E_PVX, r.int32())
        slotted = r.int32()
    n = r.int32() + 1
    links = []
    for _ in range(n):
        links.append({"name": r.string(), "index": r.int32()})

    out: dict[str, Any] = {
        "special": special,
        "alt_string": alt,
        "links": links,
    }
    if include_pv:
        out["pv_mode"] = pv_mode
        out["slotted"] = slotted
    return out


def _read_enhancement_set(r: read_i12.Reader) -> dict[str, Any]:
    display_name = r.string()
    short_name = r.string()
    uid = r.string()
    desc = r.string()
    set_type = r.int32()
    image = r.string()
    level_min = r.int32()
    level_max = r.int32()

    enhancement_indices = [r.int32() for _ in range(r.int32() + 1)]

    bonus = [_read_bonus_item(r, include_pv=True) for _ in range(r.int32() + 1)]
    special_bonus = [_read_bonus_item(r, include_pv=False) for _ in range(r.int32() + 1)]

    return {
        "display_name": display_name,
        "short_name": short_name,
        "uid": uid,
        "desc": desc,
        "set_type": set_type,
        "image": image,
        "level_min": level_min,
        "level_max": level_max,
        "enhancement_indices": enhancement_indices,
        "bonus": bonus,
        "special_bonus": special_bonus,
    }


def read_enhdb(buf: bytes) -> tuple[list[dict[str, Any]], list[dict[str, Any]], float, bool]:
    r = read_i12.Reader(buf)
    header = r.string()
    if header != HEADER:
        raise ValueError(f"unexpected header: {header!r} (expected {HEADER!r})")

    version = r.single()
    enhancements = [_read_enhancement(r) for _ in range(r.int32() + 1)]
    sets = [_read_enhancement_set(r) for _ in range(r.int32() + 1)]

    aligned = r.pos == len(buf)
    if not aligned:
        raise ValueError(
            f"alignment check FAILED: parsed through byte {r.pos}, file size {len(buf)}. "
            "Enhancement/EnhancementSet field order is likely misread."
        )
    return enhancements, sets, version, aligned


def _emit_jsonl(records: list[dict[str, Any]], out_path: str) -> None:
    out = sys.stdout if out_path == "-" else open(out_path, "w", encoding="utf-8")
    try:
        for rec in records:
            out.write(json.dumps(rec, ensure_ascii=False) + "\n")
    finally:
        if out is not sys.stdout:
            out.close()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Mids EnhDB.mhd reader (DSH9 bootstrap)")
    ap.add_argument("--mhd", default=DEFAULT_MHD, help="path to EnhDB.mhd (default: HC)")
    ap.add_argument("--mode", choices=["summary", "enhancements", "sets", "both"], default="summary")
    ap.add_argument("--out", default="-", help="JSONL output path ('-' = stdout)")
    ap.add_argument("--grep", default=None, help="case-insensitive name filter")
    ap.add_argument("--limit", type=int, default=None, help="emit first N after filtering")
    args = ap.parse_args(argv)

    mhd_path = os.path.abspath(args.mhd)
    if not os.path.isfile(mhd_path):
        print(f"error: no such file: {mhd_path}", file=sys.stderr)
        return 2

    with open(mhd_path, "rb") as fh:
        enh, sets, version, _ = read_enhdb(fh.read())

    proc_count = sum(1 for e in enh if e.get("is_proc"))
    fx_count = sum(1 for e in enh for eff in e.get("effects", []) if "fx" in eff)

    if args.mode == "summary":
        print(
            f"[read_enhdb] {os.path.basename(mhd_path)}: version={version:.6g}, "
            f"enhancements={len(enh)}, sets={len(sets)}, proc_enhancements={proc_count}, "
            f"effects_with_fx={fx_count}, alignment=OK",
            file=sys.stderr,
        )
        return 0

    needle = args.grep.lower() if args.grep else None
    records: list[dict[str, Any]] = []

    if args.mode in ("enhancements", "both"):
        selected = [e for e in enh if needle is None or needle in e["name"].lower() or needle in e["uid"].lower()]
        if args.limit is not None:
            selected = selected[: args.limit]
        if args.mode == "enhancements":
            records = selected
        else:
            records.extend({"record_type": "enhancement", **e} for e in selected)

    if args.mode in ("sets", "both"):
        selected = [s for s in sets if needle is None or needle in s["display_name"].lower() or needle in s["uid"].lower()]
        if args.limit is not None:
            selected = selected[: args.limit]
        if args.mode == "sets":
            records = selected
        else:
            records.extend({"record_type": "set", **s} for s in selected)

    _emit_jsonl(records, args.out)
    print(
        f"[read_enhdb] emitted {len(records)} record(s) from {os.path.basename(mhd_path)} "
        f"(mode={args.mode}, grep={args.grep!r}, limit={args.limit})",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
