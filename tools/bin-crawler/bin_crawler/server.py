"""Bin Crawler — stdlib HTTP server with JSON API over parsed bin data.

Supports multiple data sources (e.g. Homecoming, Rebirth).
Usage:
  py -3 -m bin_crawler.server
  py -3 -m bin_crawler.server --source hc="G:/Homecoming/assets/live" --source rebirth="/tmp/rebirth_bins"
"""

import argparse
import json
import sys
import threading
import webbrowser
from dataclasses import asdict
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from socketserver import ThreadingMixIn
from urllib.parse import parse_qs, urlparse

from .parser import (
    parse_powers, parse_powersets, parse_powercats, parse_classes, load_messages, BinResolver,
    parse_boostsets, build_power_category_index,
)
from .assets_dir import is_assets_dir, saved_assets_dir, remember_assets_dir



class DataSource:
    """Holds parsed data for one data source."""
    __slots__ = ("name", "label", "powers", "powers_dicts", "categories",
                 "cat_to_powersets", "total", "powerset_dicts", "powercat_dicts",
                 "class_dicts", "villain_class_dicts")

    def __init__(self, name: str, label: str):
        self.name = name
        self.label = label
        self.powers = []
        self.powers_dicts: list[dict] = []
        self.categories: list[str] = []
        self.cat_to_powersets: dict[str, list[str]] = {}
        self.total: int = 0
        self.powerset_dicts: list[dict] = []
        self.powercat_dicts: list[dict] = []
        self.class_dicts: list[dict] = []
        self.villain_class_dicts: list[dict] = []


class ThreadingHTTPServer(ThreadingMixIn, HTTPServer):
    daemon_threads = True


class StatsHandler(BaseHTTPRequestHandler):
    # Shared class-level state (set before server starts)
    sources: dict[str, DataSource] = {}
    default_source: str = ""
    static_dir: Path = Path(__file__).resolve().parent / "static"

    # Single-dir (web-UI) mode state. The server binds its port immediately and
    # parses in the background, so the launcher's port-based status detection
    # sees it online right away and the directory can be (re)chosen from the web
    # UI cogwheel without a blocking startup picker.
    assets_dir: str = ""             # currently configured dir for the default source
    load_status: str = "unconfigured"  # unconfigured | loading | ready | error
    load_message: str = "Choose your game assets folder to begin."
    _load_lock = threading.Lock()
    _load_generation = 0             # bumped per load so a stale parse can't clobber a newer one

    def log_message(self, fmt, *args):
        sys.stderr.write(f"{args[0]}\n")

    def _params(self):
        return {k: v[0] for k, v in parse_qs(urlparse(self.path).query).items()}

    def _get_source(self) -> DataSource:
        p = self._params()
        name = p.get("source", self.default_source)
        # Until a background parse finishes, `sources` may be empty — serve an
        # empty source rather than KeyError so the UI can show the loading state.
        src = self.sources.get(name) or self.sources.get(self.default_source)
        return src if src is not None else _EMPTY_SOURCE

    def _json(self, obj, status=200):
        body = json.dumps(obj, ensure_ascii=False).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _serve_static(self, filename):
        fpath = self.static_dir / filename
        if not fpath.is_file():
            self.send_error(404)
            return
        data = fpath.read_bytes()
        ct_map = {".html": "text/html", ".css": "text/css", ".js": "application/javascript"}
        ext = fpath.suffix
        ct = ct_map.get(ext, "application/octet-stream")
        self.send_response(200)
        self.send_header("Content-Type", ct)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    # --- Routes ---

    def do_GET(self):
        path = urlparse(self.path).path
        routes = {
            "/": lambda: self._serve_static("index.html"),
            "/api/config": self._api_config,
            "/api/sources": self._api_sources,
            "/api/stats": self._api_stats,
            "/api/categories": self._api_categories,
            "/api/powersets": self._api_powersets,
            "/api/powers": self._api_powers,
            "/api/export": self._api_export,
            "/api/classes": self._api_classes,
        }
        handler = routes.get(path)
        if handler:
            try:
                handler()
            except Exception as e:
                self._json({"error": str(e)}, 500)
        else:
            self.send_error(404)

    def do_POST(self):
        path = urlparse(self.path).path
        if path == "/api/set-assets-dir":
            try:
                self._api_set_assets_dir()
            except Exception as e:
                self._json({"error": str(e)}, 500)
        else:
            self.send_error(404)

    def _api_config(self):
        """Current single-dir state for the web UI cogwheel: the configured
        folder and the background-load status (unconfigured / loading / ready /
        error)."""
        self._json({
            "assets_dir": self.assets_dir,
            "saved_dir": saved_assets_dir() or "",
            "status": self.load_status,
            "message": self.load_message,
            "multi_source": len(self.sources) > 1,
        })

    def _api_set_assets_dir(self):
        """Set/change the assets folder from the web UI, persist it, and kick off
        a background re-parse. Validates that the path actually holds .pigg/.bin
        data so a typo doesn't silently wipe the current data."""
        length = int(self.headers.get("Content-Length", "0"))
        try:
            payload = json.loads(self.rfile.read(length) or b"{}")
        except json.JSONDecodeError:
            self._json({"ok": False, "error": "invalid json"}, 400)
            return
        new_dir = (payload.get("path") or "").strip()
        if not new_dir:
            self._json({"ok": False, "error": "no path given"}, 400)
            return
        if not is_assets_dir(new_dir):
            self._json({"ok": False, "error": "That folder has no .pigg or .bin game data."}, 400)
            return
        remember_assets_dir(new_dir)
        start_default_load(new_dir)
        self._json({"ok": True, "status": StatsHandler.load_status})

    def _api_sources(self):
        self._json([
            {"name": s.name, "label": s.label, "total": s.total,
             "categories": len(s.categories),
             "powersets": sum(len(v) for v in s.cat_to_powersets.values())}
            for s in self.sources.values()
        ])

    def _api_stats(self):
        src = self._get_source()
        self._json({
            "source": src.name,
            "total": src.total,
            "categories": len(src.categories),
            "powersets": sum(len(v) for v in src.cat_to_powersets.values()),
        })

    def _api_categories(self):
        src = self._get_source()
        self._json(src.categories)

    def _api_powersets(self):
        src = self._get_source()
        p = self._params()
        cat = p.get("category", "")
        if cat and cat in src.cat_to_powersets:
            self._json(src.cat_to_powersets[cat])
        else:
            all_ps = []
            for ps_list in src.cat_to_powersets.values():
                all_ps.extend(ps_list)
            self._json(sorted(set(all_ps)))

    def _api_powers(self):
        src = self._get_source()
        p = self._params()
        q = p.get("q", "").lower()
        category = p.get("category", "")
        powerset = p.get("powerset", "")
        area = p.get("area", "")
        sort_col = p.get("sort", "full_name")
        desc = p.get("desc", "") == "1"
        limit = int(p.get("limit", "200"))

        results = src.powers_dicts
        boost = p.get("boost", "")
        if q:
            results = [pw for pw in results
                       if q in pw["full_name"].lower()
                       or q in pw["display_name"].lower()]
        if category:
            results = [pw for pw in results if pw["category"] == category]
        if powerset:
            results = [pw for pw in results if pw["powerset"] == powerset]
        if area:
            results = [pw for pw in results if pw["effect_area_name"] == area]
        if boost:
            results = [pw for pw in results
                       if boost in pw["boosts_allowed"]]

        # Sort
        valid_cols = {
            "full_name", "display_name", "category", "powerset", "power_name",
            "effect_area_name", "max_targets_hit", "range", "radius",
            "activate_period", "recharge_time", "endurance_cost",
        }
        if sort_col not in valid_cols:
            sort_col = "full_name"
        results = sorted(results, key=lambda r: r.get(sort_col, ""),
                         reverse=desc)

        total_matched = len(results)
        if limit:
            results = results[:limit]

        self._json({"source": src.name, "count": total_matched, "powers": results})

    def _api_classes(self):
        src = self._get_source()
        p = self._params()
        name = p.get("name", "")
        include_villain = p.get("villain", "") == "1"

        classes = src.class_dicts
        if include_villain:
            classes = classes + src.villain_class_dicts
        if name:
            classes = [c for c in classes if c["name"] == name]

        self._json({"source": src.name, "count": len(classes), "classes": classes})

    def _api_export(self):
        src = self._get_source()
        p = self._params()
        category = p.get("category", "")
        powerset = p.get("powerset", "")

        if powerset and category:
            # Export a single powerset + its powers
            ps_key = category + "." + powerset
            ps = next((ps for ps in src.powerset_dicts if ps["key"] == ps_key), None)
            powers = [pw for pw in src.powers_dicts
                      if pw["category"] == category and pw["powerset"] == powerset]
            self._json({
                "source": src.name,
                "powerset": ps,
                "powers": powers,
            })
        elif powerset:
            # Powerset without category — match all categories
            matching_ps = [ps for ps in src.powerset_dicts
                           if ps["key"].endswith("." + powerset)]
            powers = [pw for pw in src.powers_dicts if pw["powerset"] == powerset]
            self._json({
                "source": src.name,
                "powersets": matching_ps,
                "powers": powers,
            })
        elif category:
            # Export a category + its powersets + all powers
            pc = next((pc for pc in src.powercat_dicts if pc["key"] == category), None)
            cat_ps_names = src.cat_to_powersets.get(category, [])
            ps_keys = {category + "." + ps_name for ps_name in cat_ps_names}
            powersets = [ps for ps in src.powerset_dicts if ps["key"] in ps_keys]
            powers = [pw for pw in src.powers_dicts if pw["category"] == category]
            self._json({
                "source": src.name,
                "powercat": pc,
                "powersets": powersets,
                "powers": powers,
            })
        else:
            # Export everything
            self._json({
                "source": src.name,
                "powers": src.powers_dicts,
                "powersets": src.powerset_dicts,
                "powercats": src.powercat_dicts,
            })


def load_source(name: str, label: str, assets_dir: Path) -> DataSource:
    """Load and parse all binary data from an assets directory into a DataSource.

    Accepts either a directory containing .pigg archives (preferred) or a
    directory of loose .bin files (legacy). Pigg archives are always preferred
    as they contain the most up-to-date data from the game client.
    """
    src = DataSource(name, label)

    resolver = BinResolver(assets_dir)
    print(f"  [{name}] Source: {resolver.source_description}", flush=True)

    if not resolver.has("powers.bin"):
        print(f"  Warning: powers.bin not found via {resolver.source_description}", file=sys.stderr)
        return src

    # Load message table for display name resolution
    msgs = None
    if resolver.has("clientmessages-en.bin"):
        print(f"  [{name}] Loading message table...", flush=True)
        msgs = load_messages(resolver.read("clientmessages-en.bin"))
        print(f"  [{name}] {len(msgs)} messages loaded.", flush=True)

    import time as _time

    def _timed(label: str, parse_fn, filename: str):
        print(f"  [{name}] Parsing {filename} (this may take a while)...", flush=True)
        t0 = _time.monotonic()
        result = parse_fn(resolver.read(filename))
        print(f"  [{name}] {len(result)} {label} loaded in {_time.monotonic() - t0:.1f}s.", flush=True)
        return result

    # Parse powers (this is the slow one — typically 60-120s for HC's powers.bin)
    powers = _timed("power records", parse_powers, "powers.bin")

    # Parse powersets
    if resolver.has("powersets.bin"):
        ps_records = _timed("powerset records", parse_powersets, "powersets.bin")
        src.powerset_dicts = [asdict(ps) for ps in ps_records]

    # Parse powercats
    if resolver.has("powercats.bin"):
        pc_records = _timed("powercat records", parse_powercats, "powercats.bin")
        src.powercat_dicts = [asdict(pc) for pc in pc_records]

    # Parse classes
    if resolver.has("classes.bin"):
        cls_records = _timed("class records", parse_classes, "classes.bin")
        src.class_dicts = [asdict(c) for c in cls_records]

    if resolver.has("villain_classes.bin"):
        vcls_records = _timed("villain class records", parse_classes, "villain_classes.bin")
        src.villain_class_dicts = [asdict(c) for c in vcls_records]

    # Parse boostsets.bin — the authoritative per-IO-set list of powers each set
    # can slot into, reversed into a power full_name → planner-category index.
    # The powers.bin record itself does NOT carry the slottable set categories
    # (its `allowed_boostset_cats` field is always empty — see _powers.py), so
    # without this the web view shows boosts_allowed but no set categories.
    set_cats_index: dict[str, list[str]] = {}
    if resolver.has("boostsets.bin"):
        boost_sets = _timed("IO sets", parse_boostsets, "boostsets.bin")
        set_cats_index = build_power_category_index(boost_sets)
        print(f"  [{name}] {len(set_cats_index)} powers indexed to set categories.", flush=True)

    # Resolve P-hash display names
    if msgs:
        for pw in powers:
            pw.display_name = msgs.resolve(pw.display_name)
            pw.display_help = msgs.resolve(pw.display_help)
            pw.short_help = msgs.resolve(pw.short_help)

    # Build lookup structures
    powers_dicts = []
    cat_set = set()
    cat_to_ps: dict[str, set[str]] = {}
    for pw in powers:
        d = asdict(pw)
        d["category"] = pw.category
        d["powerset"] = pw.powerset
        d["power_name"] = pw.power_name
        d["effect_area_name"] = pw.effect_area_name
        # boostsets.bin-derived set categories (None when not indexed / no
        # boostsets.bin, mirroring export_powers.py's three-state contract).
        d["allowed_set_categories"] = set_cats_index.get(pw.full_name) if set_cats_index else None
        powers_dicts.append(d)
        cat_set.add(pw.category)
        cat_to_ps.setdefault(pw.category, set()).add(pw.powerset)

    src.powers = powers
    src.powers_dicts = powers_dicts
    src.total = len(powers)
    src.categories = sorted(cat_set)
    src.cat_to_powersets = {k: sorted(v) for k, v in cat_to_ps.items()}

    return src


# Empty placeholder served while a background parse is still running (or no
# folder is configured yet), so the API never KeyErrors on a missing source.
_EMPTY_SOURCE = DataSource("", "")


def _background_load(path: str, name: str = "hc", label: str = "Homecoming") -> None:
    """Parse `path` off the request thread and swap it in as the default source.
    A generation counter guards against an older parse finishing after a newer
    one was requested (rapid folder changes)."""
    StatsHandler._load_generation += 1
    gen = StatsHandler._load_generation
    StatsHandler.load_status = "loading"
    StatsHandler.load_message = f"Parsing {path} … (1-3 min on first load)"
    try:
        src = load_source(name, label, Path(path))
        if gen != StatsHandler._load_generation:
            return  # a newer load superseded this one
        with StatsHandler._load_lock:
            StatsHandler.sources = {name: src}
            StatsHandler.default_source = name
            StatsHandler.assets_dir = path
        if src.total > 0:
            StatsHandler.load_status = "ready"
            StatsHandler.load_message = f"{src.total:,} powers loaded."
        else:
            StatsHandler.load_status = "error"
            StatsHandler.load_message = "No power data found in that folder."
    except Exception as e:  # noqa: BLE001 — surface any parse error to the UI
        if gen == StatsHandler._load_generation:
            StatsHandler.load_status = "error"
            StatsHandler.load_message = f"Failed to load: {e}"


def start_default_load(path: str) -> None:
    """Kick off `_background_load` in a daemon thread (returns immediately)."""
    threading.Thread(target=_background_load, args=(path,), daemon=True).start()


def main():
    ap = argparse.ArgumentParser(description="Power Stats Browser")
    ap.add_argument("--assets-dir", type=Path, default=None,
                    help="Path to assets directory (with .pigg files or bin/ subdir)")
    ap.add_argument("--source", action="append", metavar="NAME=PATH",
                    help="Named data source, e.g. --source hc=G:/Homecoming/assets/live "
                         "(advanced/multi-source; loads synchronously before serving)")
    ap.add_argument("--port", "-p", type=int, default=8090)
    ap.add_argument("--no-browser", action="store_true")
    args = ap.parse_args()

    addr = ("127.0.0.1", args.port)
    server = ThreadingHTTPServer(addr, StatsHandler)
    url = f"http://127.0.0.1:{args.port}"

    if args.source:
        # Advanced multi-source mode (e.g. the diff tool): load every named
        # source synchronously, then serve. No web-UI directory picker here.
        import time as _time
        print("=" * 60, flush=True)
        print(f"Bin Crawler — loading {len(args.source)} data source(s).", flush=True)
        print("Parsing the .pigg archives can take 1-3 minutes per source.", flush=True)
        print("=" * 60, flush=True)
        sources: dict[str, DataSource] = {}
        for spec in args.source:
            if "=" not in spec:
                print(f"Invalid --source format: {spec!r} (expected NAME=PATH)", file=sys.stderr)
                sys.exit(1)
            name, path_str = spec.split("=", 1)
            label = name.replace("_", " ").title()
            print(f"\nLoading source '{name}' from {path_str}...", flush=True)
            t0 = _time.monotonic()
            sources[name] = load_source(name, label, Path(path_str))
            print(f"  [{name}] loaded in {_time.monotonic() - t0:.1f}s.", flush=True)
        StatsHandler.sources = sources
        StatsHandler.default_source = next(iter(sources))
        StatsHandler.load_status = "ready"
        StatsHandler.load_message = f"{sum(s.total for s in sources.values()):,} powers loaded."
        print(f"\nServing {len(sources)} source(s) on {url}", flush=True)
    else:
        # Single-dir (web-UI) mode — the launcher's path. Bind the port NOW and
        # parse in the background, so the launcher sees the server online
        # immediately and the folder can be chosen / changed from the web-UI
        # cogwheel. Auto-load the --assets-dir flag or the remembered folder if
        # it's valid; otherwise wait for the user to pick one in the UI.
        initial = str(args.assets_dir) if args.assets_dir else saved_assets_dir()
        print(f"Bin Crawler serving on {url}", flush=True)
        if initial and is_assets_dir(initial):
            print(f"Loading remembered folder in the background: {initial}", flush=True)
            start_default_load(initial)
        else:
            StatsHandler.assets_dir = initial or ""
            StatsHandler.load_status = "unconfigured"
            StatsHandler.load_message = (
                "Choose your game assets folder (the one with the .pigg files) "
                "via the ⚙ button."
            )
            print("No valid folder configured yet — set one via the web UI (⚙).", flush=True)

    if not args.no_browser:
        webbrowser.open(url)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.")
        server.shutdown()


if __name__ == "__main__":
    main()
