"""Bin Crawler — parser for City of Heroes binary data files.

Part of the CoH Sidekick tool suite. Reads Cryptic `.bin` files (Parse6 /
Parse7 container formats) from either loose files or from within `.pigg`
archives, via its sibling tool Pigg Wrangler.
"""

import sys
from pathlib import Path

_PIGG_WRANGLER_DIR = (
    Path(__file__).resolve().parent.parent.parent / "pigg-wrangler"
)
if _PIGG_WRANGLER_DIR.is_dir() and str(_PIGG_WRANGLER_DIR) not in sys.path:
    sys.path.insert(0, str(_PIGG_WRANGLER_DIR))
