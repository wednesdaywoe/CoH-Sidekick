"""PIGG archive format library.

Reads the Cryptic `.pigg` archive format used by City of Heroes.
Provides:
    PiggEntry       — a single file record within an archive
    PiggArchive     — read-only access to one .pigg file
    PiggReader      — compatibility alias for PiggArchive
    PiggCollection  — unified view across all .pigg files in a directory

Pure stdlib; no external dependencies.
"""

from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


PIGG_SIGNATURE = 0x123
FILENAME_TABLE_SIGNATURE = 0x6789
HEADER_SIZE = 16
ENTRY_SIZE = 48
FILENAME_TABLE_HEADER_SIZE = 12


@dataclass(frozen=True)
class PiggEntry:
    """A single file record within a PIGG archive."""

    path: str
    offset: int
    uncompressed_size: int
    compressed_size: int

    @property
    def filename(self) -> str:
        return self.path.rsplit("/", 1)[-1] if "/" in self.path else self.path

    @property
    def directory(self) -> str:
        if "/" not in self.path:
            return "."
        return self.path.rsplit("/", 1)[0]

    @property
    def extension(self) -> str:
        name = self.filename
        if "." not in name:
            return ""
        return "." + name.rsplit(".", 1)[1]


class PiggArchive:
    """Read-only index of a single .pigg archive file."""

    def __init__(self, pigg_path: str | Path):
        self.pigg_path: str = str(pigg_path)
        self.entries: list[PiggEntry] = []
        self._by_path: dict[str, PiggEntry] = {}
        self._by_filename: dict[str, PiggEntry] = {}
        self._build_index()

    def _build_index(self) -> None:
        with open(self.pigg_path, "rb") as f:
            data = f.read()

        sig, _unk1, _unk2, _unk3, entry_count = struct.unpack_from(
            "<IHHII", data, 0
        )
        if sig != PIGG_SIGNATURE:
            raise ValueError(
                f"Bad PIGG signature at {self.pigg_path}: {sig:#x}"
            )

        raw_entries: list[dict[str, int]] = []
        off = HEADER_SIZE
        for _ in range(entry_count):
            vals = struct.unpack_from("<IIIIIII4II", data, off)
            raw_entries.append(
                {
                    "name_idx": vals[1],
                    "size": vals[2],
                    "offset": vals[4],
                    "compressed_size": vals[11],
                }
            )
            off += ENTRY_SIZE

        table_sig = struct.unpack_from("<I", data, off)[0]
        if table_sig != FILENAME_TABLE_SIGNATURE:
            raise ValueError(
                f"Bad filename-table signature in {self.pigg_path}: "
                f"{table_sig:#x}"
            )
        off += FILENAME_TABLE_HEADER_SIZE

        filenames: list[str] = []
        for _ in range(entry_count):
            (str_len,) = struct.unpack_from("<I", data, off)
            off += 4
            name = (
                data[off : off + str_len]
                .rstrip(b"\x00")
                .decode("ascii", errors="replace")
            )
            filenames.append(name)
            off += str_len

        for raw in raw_entries:
            path = filenames[raw["name_idx"]]
            entry = PiggEntry(
                path=path,
                offset=raw["offset"],
                uncompressed_size=raw["size"],
                compressed_size=raw["compressed_size"],
            )
            self.entries.append(entry)
            self._by_path[path] = entry
            self._by_filename[entry.filename] = entry

    def has(self, name: str) -> bool:
        """Check membership by full internal path or by basename."""
        return name in self._by_path or name in self._by_filename

    def get(self, name: str) -> PiggEntry | None:
        """Look up an entry by full path or basename."""
        return self._by_path.get(name) or self._by_filename.get(name)

    def extract(self, target: PiggEntry | str) -> bytes:
        """Extract a file's bytes. Accepts a PiggEntry, full path, or basename."""
        if isinstance(target, PiggEntry):
            entry: PiggEntry | None = target
        else:
            entry = self._by_path.get(target) or self._by_filename.get(target)
        if entry is None:
            raise KeyError(f"{target!r} not found in {self.pigg_path}")

        with open(self.pigg_path, "rb") as f:
            f.seek(entry.offset)
            if entry.compressed_size > 0:
                return zlib.decompress(f.read(entry.compressed_size))
            return f.read(entry.uncompressed_size)

    def list_files(self) -> list[str]:
        """All basenames in the archive."""
        return [e.filename for e in self.entries]

    def list_paths(self) -> list[str]:
        """All full internal paths in the archive."""
        return [e.path for e in self.entries]


PiggReader = PiggArchive


class PiggCollection:
    """Unified view of all .pigg archives in a directory.

    On construction, walks the given directory for *.pigg files and loads
    each as a PiggArchive. Provides extraction by PiggEntry or full path
    across the entire set; on path collisions, the first archive wins
    (archives are processed in sorted order).
    """

    def __init__(self, assets_dir: str | Path):
        self.assets_dir = Path(assets_dir)
        self.readers: list[PiggArchive] = []
        self._entry_owner: dict[int, PiggArchive] = {}
        self._path_index: dict[str, tuple[PiggArchive, PiggEntry]] = {}

        if not self.assets_dir.is_dir():
            return

        for pigg_path in sorted(self.assets_dir.glob("*.pigg")):
            try:
                archive = PiggArchive(pigg_path)
            except (ValueError, OSError):
                continue
            self.readers.append(archive)
            for entry in archive.entries:
                self._entry_owner[id(entry)] = archive
                self._path_index.setdefault(entry.path, (archive, entry))

    def extract(self, target: PiggEntry | str) -> bytes:
        """Extract by PiggEntry or by full internal path string."""
        if isinstance(target, PiggEntry):
            owner = self._entry_owner.get(id(target))
            if owner is None:
                raise KeyError(
                    "PiggEntry does not belong to any archive in this collection"
                )
            return owner.extract(target)

        hit = self._path_index.get(target)
        if hit is None:
            raise KeyError(f"{target!r} not found in any archive")
        archive, entry = hit
        return archive.extract(entry)

    def has(self, path: str) -> bool:
        return path in self._path_index

    def list_paths(self) -> list[str]:
        return list(self._path_index.keys())

    def iter_entries(self) -> Iterable[tuple[PiggArchive, PiggEntry]]:
        for archive in self.readers:
            for entry in archive.entries:
                yield archive, entry
