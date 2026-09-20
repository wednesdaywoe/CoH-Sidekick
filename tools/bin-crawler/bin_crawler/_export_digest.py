"""What an export actually wrote — SECURITY_AUDIT.md F78, DATA-GAP-REGISTER PROV-1.

`_export_fingerprint.py` answers "which exporter ran" and the manifest's
`source` answers "which assets tree did it read". Neither answers the third
question, and it is the one `exported_powers/` most needs asked: **are the bytes
sitting in the tree right now the bytes the exporter wrote?**

Nothing hashed them. Every gate downstream — converter validation, contract
totals, regen-diff, the fixtures — consumes the export as ground truth and is
internally self-consistent with whatever it finds there, so an edit made to a
committed power file after the export is indistinguishable from a real parse.
That is the FLAGS-2 shape the mandate names, one layer lower: not a bad read
becoming authoritative, but a good read quietly amended afterwards.

The guard: each exporter records, in its own manifest, a `content_digest` over
every file it wrote, and `src/data/export-contents.test.ts` recomputes it from
the committed tree. A file edited, added, or deleted after the export moves the
digest and the gate goes red; the only green path back is a re-export.

What this does and does not buy. It is not an attestation — a single committer
can always re-export or re-stamp. What it removes is the *silent* case: partial
re-exports, stray scripts, a bad merge, an orphan left behind by a category that
stopped existing. Afterwards those are a red gate instead of new ground truth,
and a deliberate amendment has to carry a changed digest in the same diff, where
a reviewer can see it. `path_safety.py` put it exactly: a bad write "leaves no
trace to find afterwards either". This is the trace.

Exactness comes from the write boundary, not from a walk. `ExportTree` is the
one door every exported file goes through, so the recorded set is what the
exporter wrote by construction — no glob, no subtraction, no allowlist of which
subtrees belong to whom. The JS gate has to rederive that set from the committed
tree and does so by nearest-enclosing-manifest (the surfaces nest:
`exported_powers/` holds `tables/`, `entities/` and the three sibling datasets),
which is a rule rather than a list, and disagreement shows up as a `file_count`
mismatch before it shows up as a digest mismatch.

The fold is `_export_fingerprint._fold`, shared rather than reimplemented, so
"hash a set of files" means one thing in this package. The JS side must
replicate it byte for byte — sorted (posix relpath, bytes) folded into sha256 as
`relpath\\0content\\0` — exactly as it already does for the source fingerprint.
A divergence surfaces as a permanently-red guard, never a silent gap.
"""
from __future__ import annotations

import json
from pathlib import Path

from bin_crawler._export_fingerprint import _fold


class ExportTree:
    """The write boundary for one exporter surface, and its content digest.

    Every file an export writes goes through `write_json`/`write_text`, which
    records `(relpath, bytes)` as it writes. `digest()` folds what was recorded.

    The manifest is stamped through `write_manifest`, which is deliberately NOT
    recorded: a file cannot carry its own hash. The JS gate excludes manifests
    by the same rule.
    """

    def __init__(self, root: Path):
        self.root = Path(root)
        self._entries: list[tuple[str, bytes]] = []
        self._written: set[str] = set()

    def write_text(self, path: Path, text: str) -> None:
        """Write `text` to `path` (under `root`) and record it.

        A second write to the same path is fatal. `export_classes.py` already
        refuses a colliding class name on the grounds that last-write-wins
        "would silently drop one", and the powers tree had exactly that bug
        without the refusal: two case-variant powerset groups shared one
        directory and one index.json overwrote the other (PROV-7). The digest
        is what found it — it counted one more write than the tree held — so
        the boundary that counts is also the right place to refuse.
        """
        path = Path(path)
        rel = path.relative_to(self.root).as_posix()
        if rel in self._written:
            raise SystemExit(
                f"export collision: {rel} written twice. Last-write-wins would "
                f"silently drop the first — resolve the names that collapse "
                f"onto this path before exporting (Rule 1).")
        data = text.encode('utf-8')
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        self._written.add(rel)
        self._entries.append((rel, data))

    def write_json(self, path: Path, obj, *, indent: int = 2,
                   trailing_newline: bool = False) -> None:
        """Serialize `obj` and write it. `indent` mirrors each call site's own
        formatting — the bytes on disk must not change just because the write
        moved behind this boundary."""
        text = json.dumps(obj, indent=indent)
        if trailing_newline:
            text += '\n'
        self.write_text(path, text)

    def write_manifest(self, path: Path, manifest: dict) -> None:
        """Stamp the manifest. Not recorded — see the class docstring."""
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')

    @property
    def file_count(self) -> int:
        return len(self._entries)

    def digest(self) -> str:
        """sha256 over every file written through this boundary."""
        return _fold(self._entries)
