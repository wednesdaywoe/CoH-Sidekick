"""Client message resolution for CoH display strings.

Resolves three key formats used throughout the CoH data:
  * `P{crc32}`            — e.g. "P3321619256" -> "Flight" (CRC32 of text)
  * 26-char encoded keys  — e.g. "F3HXLLAENAFZYGLBH9N8LSAKYS" -> Fault's
    display_help. These are opaque lookup keys stored in the file next
    to the text, not computed from the text.
  * plain names           — e.g. "defaultCommanderTitle"

All three resolve through the same mechanism: for record `i` in the key
table, its TEXT lives at `string_pool[records[i+1].idx1]`. Each record's
idx1 is the s1-ordinal of the PREVIOUS record's text — a chained/shifted
encoding that almost certainly reflects how the game's authoring tool
serialized a hash-ordered dictionary.

Usage:
    msgs = load_messages(Path("G:/Homecoming/assets/live/bin/clientmessages-en.bin"))
    msgs.resolve("P3321619256")                 # -> "Flight"
    msgs.resolve("F3HXLLAENAFZYGLBH9N8LSAKYS")  # -> "This powerful stomp..."
"""

import struct
from pathlib import Path


class MessageTable:
    """Key -> text lookup for CoH client messages.

    Keys can be P-hashes, 26-char encoded IDs, or plain names — all share
    the same file-table indirection.
    """

    __slots__ = ("_keys",)

    def __init__(self, keys: dict[str, str]):
        self._keys = keys

    def resolve(self, value):
        if not isinstance(value, str):
            return value
        hit = self._keys.get(value)
        return hit if hit is not None else value

    def __len__(self):
        return len(self._keys)


def load_messages(bin_path_or_data) -> MessageTable:
    """Parse clientmessages-en.bin and return a key->text lookup.

    File layout:
      [u4 date][u4 total_keys][u4 pool_bytes]
      [string_pool: pool_bytes of null-terminated strings]
      [u4 fmt_arg_count][u4 fmt_bytes][fmt_bytes bytes of "Param\\0FmtSpec\\0"]
      [u4 key_count][u4 strlen][strlen bytes]    <-- first record, 8-byte header
      repeated key records:
        [u4 idx1][u4 idx2=idx1+1][u4 arr_cnt][arr_cnt u4s][u4 strlen][strlen bytes]

    For record i, its text lives at string_pool[records[i+1].idx1].
    """
    if isinstance(bin_path_or_data, (bytes, bytearray, memoryview)):
        data = bytes(bin_path_or_data)
    else:
        data = Path(bin_path_or_data).read_bytes()

    _, _total_keys = struct.unpack_from("<II", data, 0)
    pool_bytes, = struct.unpack_from("<I", data, 8)
    pool_start = 12
    pool_end = pool_start + pool_bytes

    # String pool: null-terminated UTF-8 strings in order. Adjacent null
    # bytes produce empty-string entries — those stay in the list so
    # ordinals remain correct.
    pool: list[str] = []
    p = pool_start
    while p < pool_end:
        q = p
        while q < pool_end and data[q] != 0:
            q += 1
        try:
            pool.append(data[p:q].decode("utf-8"))
        except UnicodeDecodeError:
            pool.append(data[p:q].decode("latin1", errors="replace"))
        p = q + 1

    # Skip format-args section: [u4 arg_count][u4 fmt_bytes][fmt_bytes bytes]
    if pool_end + 8 > len(data):
        return MessageTable({})
    _fmt_arg_count, fmt_bytes = struct.unpack_from("<II", data, pool_end)
    p = pool_end + 8 + fmt_bytes
    if p + 8 > len(data):
        return MessageTable({})

    # Key table: first record has an 8-byte header (count + strlen); the
    # rest have 16-byte headers (idx1 + idx2 + arr_cnt + array + strlen).
    _count, first_strlen = struct.unpack_from("<II", data, p)
    first_key = data[p + 8 : p + 8 + first_strlen].decode("utf-8", errors="replace")
    p = p + 8 + first_strlen

    records: list[tuple[int | None, str]] = [(None, first_key)]
    while p + 16 <= len(data):
        idx1, idx2, arr_cnt = struct.unpack_from("<III", data, p)
        # Sanity-bound: idx2 should always be idx1+1, arr_cnt shouldn't
        # explode, idx1 must point inside the string pool.
        if idx2 != idx1 + 1 or arr_cnt > 64 or idx1 >= len(pool):
            break
        str_pos = p + 12 + arr_cnt * 4
        if str_pos + 4 > len(data):
            break
        strlen, = struct.unpack_from("<I", data, str_pos)
        if strlen > 4096 or str_pos + 4 + strlen > len(data):
            break
        key = data[str_pos + 4 : str_pos + 4 + strlen].decode("utf-8", errors="replace")
        records.append((idx1, key))
        p = str_pos + 4 + strlen

    # For record i, text = pool[records[i+1].idx1]. First occurrence of
    # each key wins so that later duplicates don't overwrite the lookup.
    keys: dict[str, str] = {}
    for i in range(len(records) - 1):
        key = records[i][1]
        next_idx1 = records[i + 1][0]
        if next_idx1 is not None and next_idx1 < len(pool):
            if key not in keys:
                keys[key] = pool[next_idx1]

    return MessageTable(keys)
