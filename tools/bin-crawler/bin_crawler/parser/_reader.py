"""Core BinReader for CoH Parse7 binary format.

Parse7 layout:
  [CrypticS magic 8b][CRC32 4b][PascalString "Parse7" = u16 len + bytes]  = 20 bytes header
  [u4 string_table_size][null-terminated strings...]  string table
  [u4 data_block_size][u4 record_count][record...]    data block

Strings are stored as u4 offsets resolved from byte 24 of the file.
Bools are 4 bytes: bit 0 of first byte is value, next 3 bytes padding.
Arrays: u4 count + elements.
Struct arrays: u4 count, each element: u4 length + data.
"""

import struct

MAGIC = b"CrypticS"


class BinReader:
    """Stateful sequential reader for Parse7 binary data."""

    __slots__ = ("_data", "_pos", "_end", "_strtab_base", "_strtab_data")

    def __init__(self, data: bytes | memoryview, *, _string_table: tuple | None = None,
                 _offset: int = 0, _length: int | None = None):
        """Create a reader.

        For top-level files, pass raw file bytes — header + string table are
        parsed automatically.  For sub-readers (length-prefixed records),
        the factory passes pre-resolved string table info.
        """
        if isinstance(data, memoryview):
            self._data = data
        else:
            self._data = memoryview(data)

        if _string_table is not None:
            # Sub-reader: already have string table from parent
            self._strtab_base, self._strtab_data = _string_table
            self._pos = _offset
            self._end = _offset + (_length if _length is not None else len(self._data) - _offset)
            return

        # --- Top-level: parse header ---
        if len(data) < 20:
            raise ValueError("File too small for Parse7 header")
        if self._data[:8].tobytes() != MAGIC:
            raise ValueError(f"Bad magic: expected {MAGIC!r}, got {self._data[:8].tobytes()!r}")

        # Skip CRC (4 bytes), then PascalString format name
        # u16 len at offset 12, then len bytes of format name
        fmt_len = struct.unpack_from("<H", self._data, 12)[0]
        header_end = 14 + fmt_len  # should be 20 for "Parse7"

        # --- String table ---
        strtab_size = struct.unpack_from("<I", self._data, header_end)[0]
        strtab_start = header_end + 4
        # String offsets in the file are relative to byte 24 (header_end + 4)
        self._strtab_base = strtab_start
        self._strtab_data = self._data

        # Pad string table to 4-byte alignment
        strtab_padded = strtab_size + (4 - strtab_size % 4) % 4

        # Data block starts after string table
        data_start = strtab_start + strtab_padded
        self._pos = data_start
        self._end = len(self._data)

    # --- Primitives ---

    def _check(self, n: int):
        if self._pos + n > self._end:
            raise ValueError(
                f"Read of {n} bytes at offset {self._pos} would exceed "
                f"record boundary at {self._end} ({self._end - self._pos} remaining)"
            )

    def read_raw(self, n: int) -> bytes:
        """Consume `n` bytes verbatim. For a trailing block whose layout isn't
        decoded yet: keeping the bytes leaves a future decode something to work
        from, where skipping past them loses the evidence.
        """
        self._check(n)
        out = self._data[self._pos:self._pos + n].tobytes()
        self._pos += n
        return out

    def read_u4(self) -> int:
        self._check(4)
        val = struct.unpack_from("<I", self._data, self._pos)[0]
        self._pos += 4
        return val

    def read_s4(self) -> int:
        self._check(4)
        val = struct.unpack_from("<i", self._data, self._pos)[0]
        self._pos += 4
        return val

    def peek_u4(self) -> int:
        """Read the next u4 without advancing the cursor."""
        self._check(4)
        return struct.unpack_from("<I", self._data, self._pos)[0]

    def read_f4(self) -> float:
        self._check(4)
        val = struct.unpack_from("<f", self._data, self._pos)[0]
        self._pos += 4
        return val

    def read_bool(self) -> bool:
        """Read 4-byte bool: bit 0 of byte 0, then 3 bytes padding."""
        self._check(4)
        b = self._data[self._pos]
        self._pos += 4
        return bool(b & 1)

    def read_string(self) -> str:
        """Read u4 offset, resolve null-terminated string from string table."""
        self._check(4)
        ofs = struct.unpack_from("<I", self._data, self._pos)[0]
        self._pos += 4
        if ofs == 0:
            return ""
        # Resolve: offset is relative to strtab_base
        abs_pos = self._strtab_base + ofs
        end = abs_pos
        while end < len(self._strtab_data) and self._strtab_data[end] != 0:
            end += 1
        return bytes(self._strtab_data[abs_pos:end]).decode("ascii", errors="replace")

    # --- Arrays ---

    def read_u4_array(self) -> list[int]:
        count = self.read_u4()
        result = []
        for _ in range(count):
            result.append(self.read_u4())
        return result

    def read_s4_array(self) -> list[int]:
        count = self.read_u4()
        return [self.read_s4() for _ in range(count)]

    def read_string_array(self) -> list[str]:
        count = self.read_u4()
        return [self.read_string() for _ in range(count)]

    def read_struct_array(self, parser_fn):
        """Read u4 count, then for each: u4 len + parse via parser_fn(sub_reader)."""
        count = self.read_u4()
        results = []
        for _ in range(count):
            rec_len = self.read_u4()
            sub = self.sub_reader(rec_len)
            results.append(parser_fn(sub))
            # Ensure we advance past the full record even if parser didn't consume all
            self._pos += rec_len
        return results

    def skip_struct_array(self):
        """Skip a struct array (u4 count, each: u4 len + data)."""
        count = self.read_u4()
        for _ in range(count):
            rec_len = self.read_u4()
            self._pos += rec_len

    # --- Navigation ---

    def sub_reader(self, size: int) -> "BinReader":
        """Create a child reader for a length-prefixed sub-record."""
        return BinReader(
            self._data,
            _string_table=(self._strtab_base, self._strtab_data),
            _offset=self._pos,
            _length=size,
        )

    def skip(self, n: int):
        self._pos += n

    def remaining(self) -> int:
        return self._end - self._pos

    @property
    def pos(self) -> int:
        return self._pos

    def skip_to_end(self):
        """Skip to end of current scope."""
        self._pos = self._end


class Parse6BinReader(BinReader):
    """Reader for Parse6 binary data with inline pascal strings.

    Parse6 stores strings as u16(len) + chars + pad to 4-byte alignment,
    instead of Parse7 u4 offset into a separate string table.
    """

    __slots__ = ()

    def __init__(self, data: bytes | memoryview, *, _offset: int = 0,
                 _length: int | None = None):
        # Use a sentinel string table that will not be accessed
        super().__init__(
            data,
            _string_table=(0, memoryview(b"")),
            _offset=_offset,
            _length=_length,
        )

    def read_string(self) -> str:
        """Read inline pascal string: u16 len + chars + pad to 4-byte alignment."""
        self._check(2)
        slen = struct.unpack_from("<H", self._data, self._pos)[0]
        self._pos += 2
        if slen == 0:
            # Empty string: pad position to 4-byte alignment
            pad = (4 - self._pos % 4) % 4
            self._pos += pad
            return ""
        self._check(slen)
        s = bytes(self._data[self._pos:self._pos + slen]).rstrip(b"\x00").decode(
            "utf-8", errors="replace"
        )
        self._pos += slen
        # Pad to 4-byte alignment
        pad = (4 - self._pos % 4) % 4
        self._pos += pad
        return s

    def sub_reader(self, size: int) -> "Parse6BinReader":
        """Create a child Parse6 reader for a length-prefixed sub-record."""
        return Parse6BinReader(
            self._data,
            _offset=self._pos,
            _length=size,
        )


def _detect_format(data: bytes | memoryview) -> str:
    """Detect whether a CrypticS binary is Parse6 or Parse7."""
    if len(data) < 20:
        raise ValueError("File too small for CrypticS header")
    fmt_len = struct.unpack_from("<H", data, 12)[0]
    fmt_name = bytes(data[14:14 + fmt_len]).decode("ascii", errors="replace")
    return fmt_name


def _open_parse6(data: bytes | memoryview) -> Parse6BinReader:
    """Open a Parse6 Files1 binary and return a reader at the data block."""
    if isinstance(data, memoryview):
        mv = data
    else:
        mv = memoryview(data)

    # Header: CrypticS(8) + CRC(4) + PStr("Parse6") + PStr("Files1")
    fmt_len = struct.unpack_from("<H", mv, 12)[0]
    after_fmt = 14 + fmt_len
    container_len = struct.unpack_from("<H", mv, after_fmt)[0]
    after_container = after_fmt + 2 + container_len

    file_rec_size = struct.unpack_from("<I", mv, after_container)[0]
    nominal = after_container + 8 + file_rec_size
    file_size = len(mv)

    # Scan nearby offsets to find the data block size marker
    found = False
    data_block_offset = nominal
    for delta in range(-8, 8):
        test_offset = nominal + delta
        if test_offset < 0 or test_offset + 4 > file_size:
            continue
        candidate_size = struct.unpack_from("<I", mv, test_offset)[0]
        if candidate_size == file_size - test_offset - 4:
            data_block_offset = test_offset
            found = True
            break

    if not found:
        raise ValueError(
            f"Could not locate Parse6 data block (tried near offset {nominal})"
        )

    return Parse6BinReader(mv, _offset=data_block_offset, _length=file_size - data_block_offset)


def open_parse7(path_or_data) -> BinReader:
    """Open a CrypticS binary file (Parse6 or Parse7) and return a reader at the data block.

    Accepts either a file path (str/Path) or raw bytes/memoryview.
    """
    if isinstance(path_or_data, (bytes, memoryview)):
        data = path_or_data
    else:
        from pathlib import Path
        data = Path(path_or_data).read_bytes()

    if len(data) < 8 or data[:8] != MAGIC:
        raise ValueError(f"Bad magic: expected {MAGIC!r}, got {data[:8]!r}")

    fmt = _detect_format(data)
    if fmt == "Parse7":
        return BinReader(data)
    elif fmt == "Parse6":
        return _open_parse6(data)
    else:
        raise ValueError(f"Unsupported format: {fmt!r} (expected Parse6 or Parse7)")
