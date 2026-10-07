"""Shared, remembered selection of the directory each Bin Crawler command reads.

Every command needs a directory of game data — the assets folder holding the
`.pigg` archives (or, for the diff tool, a root holding two such folders). Rather
than retype ``--assets-dir`` every run, :func:`resolve_dir` applies this
precedence:

  1. An explicit path on the command line (deterministic for scripts/CI; also
     remembered for next time).
  2. ``--pick`` forces the folder dialog even when a path is remembered.
  3. A path saved from a previous run (namespaced per ``key``), if it still
     exists and looks valid.
  4. A native folder-picker dialog (tkinter). On a headless box with no GUI it
     falls back to a typed prompt.

The chosen path is saved to a small JSON config (``~/.config/bin-crawler/
config.json``, or ``%APPDATA%\\bin-crawler\\config.json`` on Windows) so later
runs need no flag at all.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Callable

from . import assets_sources
from .parser._pigg import BinResolver

_APP = "bin-crawler"


def _config_path() -> Path:
    if sys.platform.startswith("win"):
        base = Path(os.environ.get("APPDATA") or Path.home() / "AppData" / "Roaming")
    else:
        base = Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config")
    return base / _APP / "config.json"


def _load_config() -> dict:
    try:
        data = json.loads(_config_path().read_text())
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def _save_config(key: str, value: str) -> None:
    path = _config_path()
    cfg = _load_config()
    cfg[key] = value
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(cfg, indent=2))
    except OSError:
        pass  # non-fatal — we just re-prompt next time


def is_assets_dir(path: str | os.PathLike) -> bool:
    """True if `path` is a directory holding .pigg archives or loose .bin files."""
    try:
        return Path(path).is_dir() and BinResolver(path).has_data
    except Exception:
        return False


def saved_assets_dir() -> str | None:
    """The remembered assets directory (raw — may no longer exist), or None.
    Used by the server to auto-load / pre-fill the settings field without ever
    opening a blocking picker."""
    return _load_config().get("assets_dir")


def remember_assets_dir(path: str) -> None:
    """Persist `path` as the remembered assets directory (used by the server's
    web-UI directory picker)."""
    _save_config("assets_dir", path)


def _pick_dialog(title: str, initialdir: str | None) -> str | None:
    """Native folder picker; returns None when cancelled or no GUI is available."""
    try:
        import tkinter as tk
        from tkinter import filedialog
    except Exception:
        return None
    try:
        root = tk.Tk()
        root.withdraw()
        root.update()  # realize the hidden root before opening the dialog
        chosen = filedialog.askdirectory(
            title=title,
            initialdir=initialdir or str(Path.home()),
            mustexist=True,
        )
        root.destroy()
        return chosen or None
    except Exception:
        return None  # e.g. TclError when there is no display


def _prompt_typed(title: str, initial: str | None) -> str | None:
    suffix = f" [{initial}]" if initial else ""
    try:
        raw = input(f"{title}\nPath{suffix}: ").strip()
    except EOFError:
        return None
    return raw or initial


def resolve_dir(
    cli_value: str | None,
    *,
    key: str,
    title: str,
    pick: bool = False,
    validate: Callable[[str | os.PathLike], bool] = is_assets_dir,
    remember: bool = True,
) -> str:
    """Resolve a directory for a command (precedence in the module docstring).

    `key` namespaces the remembered path (e.g. ``"assets_dir"`` vs
    ``"assets_root"``). `validate` gates what counts as a usable directory to
    remember and warns on a bad pick; pass a laxer predicate (e.g.
    ``lambda p: Path(p).is_dir()``) for non-assets directories.
    """
    saved = _load_config().get(key)

    # 1. Explicit flag wins — unless --pick was also given to force a re-choose.
    if cli_value and not pick:
        if remember and validate(cli_value):
            _save_config(key, cli_value)
        return cli_value

    # 2. Remembered path, when not forcing the picker.
    if not pick and saved and validate(saved):
        return saved

    # 3. Folder picker, then typed-prompt fallback (headless / no tkinter).
    initial = cli_value or saved
    chosen = _pick_dialog(title, initial) or _prompt_typed(title, initial)
    if not chosen:
        fallback = cli_value or saved
        if fallback:
            print(f"No directory selected; using {fallback}", file=sys.stderr)
            return fallback
        sys.exit("No directory selected and none remembered — aborting.")

    if remember and validate(chosen):
        _save_config(key, chosen)
    elif not validate(chosen):
        print(f"Warning: {chosen} has no .pigg/.bin data — using it anyway.", file=sys.stderr)
    return chosen


def resolve_assets_dir(cli_value: str | None, *, pick: bool = False) -> str:
    """Resolve the assets directory (the folder containing the .pigg/.bin files)."""
    return resolve_dir(
        cli_value,
        key="assets_dir",
        title="Select the game assets directory (the folder containing the .pigg files)",
        pick=pick,
    )


def add_source_arguments(ap) -> None:
    """The source-selection flags every exporter shares."""
    ap.add_argument("--source", default=None, metavar="DATASET[:RING]",
                    help="Canonical tree to read, e.g. 'homecoming' (its exportable "
                         "ring) or 'homecoming:open_beta'. Preferred over --assets-dir: "
                         "it names a tree instead of typing a path. See "
                         "bin_crawler/assets_sources.json")
    ap.add_argument("--assets-dir", default=None,
                    help="Path to assets directory (with .pigg files or bin/ subdir). "
                         "Must be a tree assets_sources.json names, unless "
                         "--allow-unregistered-source is given.")
    ap.add_argument("--pick", action="store_true",
                    help="Open a folder picker to choose/change the assets directory")
    ap.add_argument("--allow-unregistered-source", action="store_true",
                    help="Permit reading a tree the registry does not name. For ad-hoc "
                         "exploration into a scratch --output-dir; never for an export "
                         "committed to exported_powers/.")


def resolve_export_source(args) -> str:
    """Resolve an exporter's assets directory, refusing trees the registry rejects.

    A path typed at the command line is the one export input nothing downstream
    can check — a wrong-tree export is internally self-consistent and passes
    every gate that reads the export (DATA-GAP-REGISTER PROV-1/PROV-2). So an
    unregistered tree stops the run rather than quietly becoming data. The traps
    are close enough to the real thing to need naming: Homecoming's `closedbeta`
    folder is not the closed beta, and Sweet Tea's `piggs` folder resolves
    `powers.bin` while holding an unrelated corpus.
    """
    if args.source:
        dataset, ring, path = assets_sources.resolve(args.source)
        if not Path(path).is_dir():
            sys.exit(f"{dataset}:{ring} is registered at {path}, which does not exist.")
        return path

    chosen = resolve_assets_dir(args.assets_dir, pick=args.pick)

    if getattr(args, "allow_unregistered_source", False):
        return chosen

    rejected = assets_sources.rejection_reason(chosen)
    if rejected:
        sys.exit(f"Refusing to export from {chosen}:\n  {rejected}")
    if assets_sources.dataset_for_path(chosen) is None:
        sys.exit(
            f"Refusing to export from {chosen}: assets_sources.json does not name it.\n"
            f"  Use --source <dataset>[:<ring>] ({', '.join(assets_sources.datasets())}),\n"
            f"  register the tree if it is a new install, or pass "
            f"--allow-unregistered-source for a scratch export."
        )
    return chosen
