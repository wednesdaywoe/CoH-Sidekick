"""Resolve .bin files from pigg archives or loose directories.

The pigg archive format itself is handled by the Pigg Wrangler library;
this module only provides the bin-file resolution layer that Bin Crawler
needs to find `powers.bin`, `powersets.bin`, etc., given an assets
directory that may contain any mix of pigg archives and loose files.
"""

from __future__ import annotations

import hashlib
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from pigg_wrangler.pigg import PiggArchive


class BinResolver:
    """Resolve .bin files from pigg archives or a loose directory.

    Usage:
        resolver = BinResolver("G:/Homecoming/assets/live")
        powers_data = resolver.read("powers.bin")

    Search order:
      1. Pigg archives (bin.pigg, bin_powers.pigg, ...) in the directory
      2. Loose .bin files in a bin/ subdirectory
      3. Loose .bin files directly in the directory
    """

    def __init__(self, assets_dir: str | Path):
        self.assets_dir = Path(assets_dir)
        self._piggs: list[PiggArchive] = []
        self._bin_dir: Path | None = None

        seen: set[Path] = set()
        # `*serverbin*` covers Rebirth's `v2_serverbin.pigg` which holds
        # classes.bin / villain_classes.bin / boostsets.bin alongside the
        # main `v2_bin.pigg` (which only has powers/messages/etc.). HC
        # ships everything in one `bin.pigg`, so the serverbin pattern is
        # a no-op for HC's layout.
        for pattern in ("bin*.pigg", "*_bin.pigg", "*_bin_*.pigg", "*serverbin*.pigg"):
            for pigg_path in sorted(self.assets_dir.glob(pattern)):
                if pigg_path in seen:
                    continue
                seen.add(pigg_path)
                try:
                    self._piggs.append(PiggArchive(pigg_path))
                except (ValueError, OSError):
                    pass

        bin_subdir = self.assets_dir / "bin"
        if bin_subdir.is_dir():
            self._bin_dir = bin_subdir
        elif any(self.assets_dir.glob("*.bin")):
            self._bin_dir = self.assets_dir

    @property
    def has_data(self) -> bool:
        """True if this directory actually yielded readable .bin sources
        (pigg archives or loose files) — used to validate a chosen/remembered
        assets directory before trusting it."""
        return bool(self._piggs or self._bin_dir)

    @property
    def source_description(self) -> str:
        if self._piggs:
            pigg_names = [Path(p.pigg_path).name for p in self._piggs]
            return f"{self.assets_dir} (piggs: {', '.join(pigg_names)})"
        if self._bin_dir:
            return f"{self._bin_dir} (loose files)"
        return f"{self.assets_dir} (no data found)"

    def provenance(self) -> dict:
        """Identify the assets tree these bytes came from, for the export manifest.

        Homecoming publishes several shards under one `assets/` root (`live`,
        `beta`, `experimental`, …) which are divergent BRANCHES, not points on
        one timeline — so an export taken from the wrong one is wrong in both
        directions at once, and being internally self-consistent it passes every
        check that reads only the export (DATA-GAP-REGISTER PROV-1; it shipped
        once, in `8a32f60c42`). `shard` is the gateable name; `sources` is the
        byte-level evidence, which is what distinguishes two trees that share a
        basename.

        `sources` covers every archive/loose bin this resolver can see, not only
        the files a given exporter read: the question being answered is "which
        shard is this", which is a property of the tree as a whole. That
        over-covers by design, the same trade `_export_fingerprint.py` makes for
        its parser glob — a self-maintaining set beats a curated one.
        """
        sources = [
            {
                "name": path.name,
                "bytes": stat.st_size,
                "modified": (
                    datetime.fromtimestamp(stat.st_mtime, timezone.utc)
                    .isoformat(timespec="seconds")
                    .replace("+00:00", "Z")
                ),
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
            for path, stat in sorted(
                ((p, p.stat()) for p in self._source_files()),
                key=lambda entry: entry[0].name,
            )
        ]
        assets_dir = self.assets_dir.resolve()
        return {
            "assets_dir": assets_dir.as_posix(),
            "shard": assets_dir.name,
            "sources": sources,
        }

    def _source_files(self) -> list[Path]:
        """Every file this resolver would read bytes out of, deduplicated."""
        files = [Path(pigg.pigg_path) for pigg in self._piggs]
        if self._bin_dir:
            files.extend(sorted(self._bin_dir.glob("*.bin")))
        seen: set[Path] = set()
        unique = []
        for path in files:
            resolved = path.resolve()
            if resolved not in seen:
                seen.add(resolved)
                unique.append(path)
        return unique

    def has(self, filename: str) -> bool:
        for pigg in self._piggs:
            if pigg.has(filename):
                return True
        if self._bin_dir and (self._bin_dir / filename).is_file():
            return True
        return False

    def read(self, filename: str) -> bytes:
        """Read a .bin file, preferring pigg archives over loose files."""
        for pigg in self._piggs:
            if pigg.has(filename):
                return pigg.extract(filename)

        if self._bin_dir:
            loose = self._bin_dir / filename
            if loose.is_file():
                return loose.read_bytes()

        raise FileNotFoundError(
            f"{filename!r} not found in piggs or "
            f"{self._bin_dir or self.assets_dir}"
        )

    def read_to_tempfile(self, filename: str) -> Path:
        """Extract a bin file to a temp path, for parsers that expect file paths.

        Files are cached in the system temp dir keyed by content hash, so
        repeated reads within a session reuse the same file.
        """
        data = self.read(filename)
        digest = hashlib.md5(data[:4096]).hexdigest()[:12]
        tmp_dir = Path(tempfile.gettempdir()) / "coh_bin_crawler"
        tmp_dir.mkdir(exist_ok=True)
        tmp_path = tmp_dir / f"{filename}.{digest}"
        if not tmp_path.exists():
            tmp_path.write_bytes(data)
        return tmp_path
