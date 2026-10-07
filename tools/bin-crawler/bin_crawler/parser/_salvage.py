"""Parser for salvage.bin — the master salvage catalog (invention crafting,
base/SG, reward, and incarnate salvage).

HC ships salvage.bin as Parse7 (string-table offsets); Rebirth has no
salvage.bin. Each record (verified against the planner's incarnate-salvage.ts):

  @0  name        (str-ref)  internal name, "S_ArcaneCantrip"
  @4  display     (str-ref)  P-hash message key -> "Arcane Cantrip"
  @8  desc        (str-ref)  P-hash (long description)
  @12 desc_short  (str-ref)  P-hash
  @16 icon        (str-ref)  "salvage_ArcaneCantrip.tga"
  @20 type_label  (str-ref)  P-hash (category display string, shared)
  @28 rarity      (u4)       1=common 2=uncommon 3=rare 4=very-rare
  @32 category    (u4)       0=legacy 1=invention 2=special 3=incarnate

Rarity/category enums are AUTHORITATIVE — read from the record's own resolved
type-label (@20) and descriptions: cat 0 "Legacy" (old Arachnos/drop salvage),
1 "Invention" (current IO-crafting salvage), 2 "Special"/"Event", 3 "Incarnate".
Rarity cross-checked: incarnate (cat 3) matches incarnate-salvage.ts; invention
(cat 1) matches known items (Boresight=common, AlchemicalGold=uncommon,
DeificWeapon=rare).
"""
import struct

from ._reader import open_parse7
from ._dataclasses import SalvageRecord

_RARITY = {1: "common", 2: "uncommon", 3: "rare", 4: "very-rare"}
_CATEGORY = {0: "legacy", 1: "invention", 2: "special", 3: "incarnate"}

# Record field offsets (Parse7).
_OFF_NAME = 0
_OFF_DISPLAY = 4
_OFF_ICON = 16
_OFF_RARITY = 28
_OFF_CATEGORY = 32


def parse_salvage(bin_path_or_data, messages=None) -> list[SalvageRecord]:
    """Parse salvage.bin into SalvageRecord list. `messages` is an optional
    MessageTable (from `_messages.load_messages`) used to resolve the display
    P-hash to English; without it, display_name falls back to the de-prefixed
    internal name."""
    r = open_parse7(bin_path_or_data)
    data = r._data

    r.read_u4()              # block_size
    count = r.read_u4()

    def read_str(rec_start, off, strtab_base):
        raw = struct.unpack_from("<I", data, rec_start + off)[0]
        if raw == 0:
            return ""
        pos = strtab_base + raw
        if pos >= len(data):
            return ""
        end = pos
        while end < len(data) and data[end] != 0:
            end += 1
        return bytes(data[pos:end]).decode("ascii", errors="replace")

    records = []
    for _ in range(count):
        rec_len = r.read_u4()
        sub = r.sub_reader(rec_len)
        rec_start = sub.pos
        strtab_base = sub._strtab_base

        name = read_str(rec_start, _OFF_NAME, strtab_base)
        if not name:
            r.skip(rec_len)
            continue

        display_key = read_str(rec_start, _OFF_DISPLAY, strtab_base)
        icon = read_str(rec_start, _OFF_ICON, strtab_base)
        rarity = struct.unpack_from("<I", data, rec_start + _OFF_RARITY)[0] \
            if rec_len > _OFF_RARITY + 4 else 0
        category = struct.unpack_from("<I", data, rec_start + _OFF_CATEGORY)[0] \
            if rec_len > _OFF_CATEGORY + 4 else 0

        display = messages.resolve(display_key) if (messages and display_key) else ""
        if not display:
            # Fallback: de-prefix internal name (S_ArcaneCantrip -> ArcaneCantrip).
            display = name[2:] if name.startswith("S_") else name

        records.append(SalvageRecord(
            name=name,
            display_name=display,
            icon=icon,
            rarity=_RARITY.get(rarity, f"unknown({rarity})"),
            category=_CATEGORY.get(category, f"unknown({category})"),
        ))
        r.skip(rec_len)

    return records
