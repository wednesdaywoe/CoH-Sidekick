"""
In-memory index built from PiggCollection.

Builds a directory tree and flat lookup tables for fast browsing,
searching, and extraction without requiring SQLite.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from pigg_wrangler.pigg import PiggCollection, PiggEntry


@dataclass
class FileNode:
    """A file entry in the virtual filesystem."""
    entry: PiggEntry
    pigg_name: str  # e.g. "sound.pigg"


@dataclass
class DirNode:
    """A directory node in the virtual filesystem tree."""
    name: str
    full_path: str
    children: dict[str, DirNode] = field(default_factory=dict)
    files: list[FileNode] = field(default_factory=list)
    file_count: int = 0  # recursive count of all files below


class PiggIndex:
    """In-memory index of all files across all PIGG archives.

    Provides directory tree navigation, search, and extraction.
    """

    def __init__(self, assets_dir: str):
        self.collection = PiggCollection(assets_dir)
        self.root = DirNode(name="", full_path="")
        self._all_files: list[FileNode] = []
        self._path_to_node: dict[str, FileNode] = {}
        self._pigg_to_nodes: dict[str, list[FileNode]] = {}
        self._build()

    def _build(self) -> None:
        """Iterate all entries and populate tree + flat index."""
        for reader in self.collection.readers:
            pigg_name = Path(reader.pigg_path).name
            for entry in reader.entries:
                node = FileNode(entry=entry, pigg_name=pigg_name)
                self._all_files.append(node)
                self._path_to_node[entry.path] = node
                self._pigg_to_nodes.setdefault(pigg_name, []).append(node)

                # Walk the directory path, creating DirNode children as needed
                dir_path = entry.directory
                parts = dir_path.split("/") if dir_path != "." else []
                current = self.root
                for part in parts:
                    if not part:
                        continue
                    if part not in current.children:
                        child_path = (current.full_path + "/" + part).lstrip("/")
                        current.children[part] = DirNode(name=part, full_path=child_path)
                    current = current.children[part]
                current.files.append(node)

        # Compute recursive file counts
        self._compute_counts(self.root)

    def _compute_counts(self, node: DirNode) -> int:
        count = len(node.files)
        for child in node.children.values():
            count += self._compute_counts(child)
        node.file_count = count
        return count

    def get_dir(self, path: str) -> DirNode | None:
        """Navigate to a directory node by path string."""
        if not path or path == "/":
            return self.root
        parts = path.strip("/").split("/")
        current = self.root
        for part in parts:
            if part not in current.children:
                return None
            current = current.children[part]
        return current

    def get_dir_filtered(self, path: str, pigg_filter: str) -> dict | None:
        """Get directory contents filtered to a specific PIGG.

        Returns a dict with 'dirs' and 'files' keys, or None if not found.
        Only includes subdirectories that contain files from the given PIGG.
        """
        pigg_nodes = set()
        for fn in self._pigg_to_nodes.get(pigg_filter, []):
            pigg_nodes.add(fn.entry.path)

        dir_node = self.get_dir(path)
        if dir_node is None:
            return None

        # Filter files
        files = [f for f in dir_node.files if f.pigg_name == pigg_filter]

        # Filter subdirs: only include dirs that have files from this PIGG somewhere below
        dirs = []
        for child in sorted(dir_node.children.values(), key=lambda d: d.name):
            if self._dir_has_pigg_files(child, pigg_filter):
                count = self._count_pigg_files(child, pigg_filter)
                dirs.append({
                    "name": child.name,
                    "full_path": child.full_path,
                    "file_count": count,
                })
        return {"dirs": dirs, "files": files}

    def _dir_has_pigg_files(self, node: DirNode, pigg_filter: str) -> bool:
        """Check if a directory or any descendant has files from the given PIGG."""
        if any(f.pigg_name == pigg_filter for f in node.files):
            return True
        return any(self._dir_has_pigg_files(c, pigg_filter) for c in node.children.values())

    def _count_pigg_files(self, node: DirNode, pigg_filter: str) -> int:
        """Count files from a specific PIGG recursively."""
        count = sum(1 for f in node.files if f.pigg_name == pigg_filter)
        for child in node.children.values():
            count += self._count_pigg_files(child, pigg_filter)
        return count

    def search(self, query: str, pigg_filter: str | None = None,
               limit: int = 200, ext_filter: str | None = None) -> list[FileNode]:
        """Substring search across file paths (case-insensitive).

        ext_filter: comma-separated extensions like ".texture,.ogg" to restrict results.
        """
        terms = query.lower().split()
        source = self._pigg_to_nodes.get(pigg_filter, []) if pigg_filter else self._all_files

        # Parse extension filter
        exts: set[str] | None = None
        if ext_filter:
            exts = {e.strip().lower() for e in ext_filter.split(",") if e.strip()}

        results = []
        for node in source:
            if exts and node.entry.extension.lower() not in exts:
                continue
            path_lower = node.entry.path.lower()
            if all(t in path_lower for t in terms):
                results.append(node)
                if len(results) >= limit:
                    break
        return results

    def get_file(self, path: str) -> FileNode | None:
        return self._path_to_node.get(path)

    def extract(self, path: str) -> bytes:
        node = self._path_to_node.get(path)
        if node is None:
            raise KeyError(f"File not found: {path}")
        return self.collection.extract(node.entry)

    def list_piggs(self) -> list[dict]:
        """Return list of PIGG archives with metadata."""
        result = []
        for reader in self.collection.readers:
            name = Path(reader.pigg_path).name
            nodes = self._pigg_to_nodes.get(name, [])
            total_size = sum(n.entry.uncompressed_size for n in nodes)
            result.append({
                "name": name,
                "file_count": len(nodes),
                "total_size": total_size,
            })
        return sorted(result, key=lambda x: x["name"])

    def collect_dir_files(self, path: str, pigg_filter: str | None = None) -> list[FileNode]:
        """Collect all files recursively under a directory."""
        dir_node = self.get_dir(path)
        if dir_node is None:
            return []
        results: list[FileNode] = []
        self._collect_recursive(dir_node, pigg_filter, results)
        return results

    def _collect_recursive(self, node: DirNode, pigg_filter: str | None,
                           results: list[FileNode]) -> None:
        for f in node.files:
            if pigg_filter is None or f.pigg_name == pigg_filter:
                results.append(f)
        for child in node.children.values():
            self._collect_recursive(child, pigg_filter, results)

    @property
    def total_files(self) -> int:
        return len(self._all_files)

    @property
    def total_size(self) -> int:
        return sum(n.entry.uncompressed_size for n in self._all_files)
