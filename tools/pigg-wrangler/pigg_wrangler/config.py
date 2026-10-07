"""
Persistent configuration for Pigg Wrangler.

Stores a JSON config file next to the executable (frozen builds)
or next to the project root (source runs) for portability.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

CONFIG_FILENAME = "pigg_wrangler.json"


def _config_path() -> Path:
    if getattr(sys, "frozen", False):
        # PyInstaller: config lives next to PiggWrangler.exe
        return Path(sys.executable).parent / CONFIG_FILENAME
    else:
        # Source: config lives in the project root (parent of package dir)
        return Path(__file__).parent.parent / CONFIG_FILENAME


def load_config() -> dict:
    """Load config from disk. Returns empty dict if missing or invalid."""
    path = _config_path()
    if not path.exists():
        return {}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return {}


def save_config(config: dict) -> None:
    """Save config to disk."""
    path = _config_path()
    path.write_text(json.dumps(config, indent=2), encoding="utf-8")


def get_assets_root(config: dict) -> str | None:
    """Return the assets_root from config, or None if not set."""
    return config.get("assets_root")
