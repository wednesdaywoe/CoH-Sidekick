"""
Web server for Pigg Wrangler.

Zero external dependencies -- uses only Python stdlib http.server.
Serves a JSON API and a single-page HTML frontend.
"""

from __future__ import annotations

import argparse
import io
import json
import mimetypes
import sys
import time
import urllib.parse
import zipfile
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from pathlib import Path

from .index_builder import PiggIndex, FileNode
from .texture import (
    texture_to_dds, texture_to_png, texture_to_png_trimmed,
    texture_to_image, get_texture_info,
)

DEFAULT_ASSETS_DIR = r"G:\Homecoming\assets\live"
DEFAULT_ASSETS_ROOT = r"G:\Homecoming\assets"

# Extensions we can render as text in the preview
TEXT_EXTENSIONS = {
    ".txt", ".cfg", ".def", ".mfx", ".ms", ".wtf", ".inc",
    ".powers", ".powersets", ".categories", ".recipe",
    ".contact", ".storyarc", ".taskset", ".spawndef",
    ".scrollset", ".scriptdef", ".npc", ".villain",
    ".zoneevent", ".reward", ".cape", ".minimap", ".bounds",
    ".nd", ".ctm", ".anim",
}

# Extensions the browser can play via <audio>
AUDIO_EXTENSIONS = {".ogg", ".mp3", ".wav"}


class ExplorerHandler(BaseHTTPRequestHandler):
    """Routes requests to API handlers or static file serving."""

    # Class-level shared state (set before server starts)
    sources: dict[str, str] = {}       # name -> path
    indexes: dict[str, PiggIndex] = {} # name -> built index (lazy)
    current_source: str = ""
    assets_root: str = ""
    static_dir: Path

    @property
    def index(self) -> PiggIndex:
        return self.indexes[self.current_source]

    def _require_index(self) -> bool:
        """Check that a source is loaded. Sends 503 if not. Returns True if OK."""
        if not self.current_source or self.current_source not in self.indexes:
            self._send_json({
                "error": "No asset source is loaded. Use Settings to configure your assets path.",
                "needs_config": True,
            }, 503)
            return False
        return True

    def log_message(self, format, *args):
        pass  # Suppress default request logging

    # ---- Routing ----

    def do_GET(self):
        parsed = urllib.parse.urlparse(self.path)
        path = parsed.path.rstrip("/") or "/"
        params = urllib.parse.parse_qs(parsed.query)

        routes = {
            "/": self._serve_index,
            "/api/stats": self._api_stats,
            "/api/piggs": self._api_piggs,
            "/api/tree": self._api_tree,
            "/api/search": self._api_search,
            "/api/preview": self._api_preview,
            "/api/download": self._api_download,
            "/api/texture/info": self._api_texture_info,
            "/api/texture/image": self._api_texture_image,
            "/api/texture/dds": self._api_texture_dds,
            "/api/texture/png": self._api_texture_png,
            "/api/texture/png-trimmed": self._api_texture_png_trimmed,
            "/api/sources": self._api_sources,
            "/api/config": self._api_config,
        }

        handler = routes.get(path)
        if handler:
            try:
                handler(params)
            except Exception as e:
                import traceback
                print(f"[ERROR] GET {self.path}: {type(e).__name__}: {e}", file=sys.stderr)
                traceback.print_exc(file=sys.stderr)
                self._send_json({"error": f"{type(e).__name__}: {e}"}, 500)
        else:
            self._serve_static(path)

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(content_length)) if content_length else {}

        path = self.path.rstrip("/")
        routes = {
            "/api/download-zip": self._api_download_zip,
            "/api/download-folder": self._api_download_folder,
            "/api/switch-source": self._api_switch_source,
            "/api/set-assets-root": self._api_set_assets_root,
        }

        handler = routes.get(path)
        if handler:
            try:
                handler(body)
            except Exception as e:
                self._send_json({"error": str(e)}, 500)
        else:
            self._send_json({"error": "Not found"}, 404)

    # ---- Response helpers ----

    def _send_json(self, data, status=200):
        body = json.dumps(data, default=_json_default).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_binary(self, data: bytes, filename: str, content_type: str | None = None):
        if content_type is None:
            content_type = mimetypes.guess_type(filename)[0] or "application/octet-stream"
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Disposition", f'attachment; filename="{filename}"')
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _serve_index(self, params=None):
        index_path = self.static_dir / "index.html"
        if not index_path.exists():
            self.send_error(404, "index.html not found")
            return
        data = index_path.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _serve_static(self, path):
        safe_name = Path(path.lstrip("/")).name
        file_path = self.static_dir / safe_name
        if file_path.exists() and file_path.is_file():
            ctype = mimetypes.guess_type(str(file_path))[0] or "application/octet-stream"
            data = file_path.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        else:
            self.send_error(404)

    # ---- GET API endpoints ----

    def _api_stats(self, params):
        if not self._require_index(): return
        piggs = self.index.list_piggs()
        self._send_json({
            "pigg_count": len(piggs),
            "total_files": self.index.total_files,
            "total_size": self.index.total_size,
        })

    def _api_piggs(self, params):
        if not self._require_index(): return
        self._send_json({"piggs": self.index.list_piggs()})

    def _api_tree(self, params):
        if not self._require_index(): return
        path = _param(params, "path", "")
        pigg_filter = _param(params, "pigg")

        if pigg_filter:
            result = self.index.get_dir_filtered(path, pigg_filter)
            if result is None:
                self._send_json({"error": f"Directory not found: {path}"}, 404)
                return
            files = sorted(result["files"], key=lambda f: f.entry.filename)
            self._send_json({
                "path": path,
                "dirs": result["dirs"],
                "files": [_file_dict(f) for f in files],
            })
        else:
            dir_node = self.index.get_dir(path)
            if dir_node is None:
                self._send_json({"error": f"Directory not found: {path}"}, 404)
                return
            dirs = sorted([
                {"name": c.name, "full_path": c.full_path, "file_count": c.file_count}
                for c in dir_node.children.values()
            ], key=lambda d: d["name"])
            files = sorted(dir_node.files, key=lambda f: f.entry.filename)
            self._send_json({
                "path": path,
                "dirs": dirs,
                "files": [_file_dict(f) for f in files],
            })

    def _api_search(self, params):
        if not self._require_index(): return
        q = _param(params, "q", "")
        pigg_filter = _param(params, "pigg")
        ext_filter = _param(params, "ext")
        limit = int(_param(params, "limit", "200"))

        if not q and not ext_filter:
            self._send_json({"results": [], "total": 0})
            return

        results = self.index.search(q or "", pigg_filter=pigg_filter,
                                    limit=limit, ext_filter=ext_filter)
        self._send_json({
            "results": [_file_dict(f) for f in results],
            "total": len(results),
        })

    def _api_preview(self, params):
        if not self._require_index(): return
        path = _param(params, "path", "")
        node = self.index.get_file(path)
        if not node:
            self._send_json({"error": "File not found"}, 404)
            return

        ext = node.entry.extension

        if ext == ".texture":
            # Texture preview: return metadata + image URL for inline display
            data = self.index.extract(path)
            try:
                tex_info = get_texture_info(data)
                self._send_json({
                    "type": "texture",
                    "image_url": f"/api/texture/image?path={urllib.parse.quote(path)}",
                    "size": node.entry.uncompressed_size,
                    "width": tex_info["width"],
                    "height": tex_info["height"],
                    "image_format": tex_info["image_format"],
                    "dds_format": tex_info.get("dds_format", ""),
                    "has_alpha": tex_info.get("has_alpha", False),
                    "mipmap_count": tex_info.get("mipmap_count", 0),
                    "original_name": tex_info.get("original_name", ""),
                })
            except Exception as e:
                self._send_json({"type": "error", "error": str(e), "size": len(data)})
        elif ext in AUDIO_EXTENSIONS:
            # Return pointer for <audio> element to stream from /api/download
            self._send_json({
                "type": "audio",
                "mime": f"audio/{ext.lstrip('.')}",
                "url": f"/api/download?path={urllib.parse.quote(path)}&inline=1",
                "size": node.entry.uncompressed_size,
            })
        elif ext in TEXT_EXTENSIONS or node.entry.uncompressed_size == 0:
            data = self.index.extract(path)
            text = data.decode("utf-8", errors="replace")
            if len(text) > 100_000:
                text = text[:100_000] + "\n... [truncated]"
            self._send_json({
                "type": "text",
                "content": text,
                "size": len(data),
            })
        else:
            # Hex dump: first 4KB
            data = self.index.extract(path)
            hex_size = min(len(data), 4096)
            hex_lines = []
            for i in range(0, hex_size, 16):
                chunk = data[i:i + 16]
                hex_part = " ".join(f"{b:02x}" for b in chunk)
                ascii_part = "".join(chr(b) if 32 <= b < 127 else "." for b in chunk)
                hex_lines.append(f"{i:08x}  {hex_part:<48s}  {ascii_part}")
            self._send_json({
                "type": "hex",
                "content": "\n".join(hex_lines),
                "size": len(data),
                "showing": hex_size,
            })

    # ---- Texture conversion endpoints ----

    def _api_texture_info(self, params):
        """Return detailed texture metadata as JSON."""
        if not self._require_index(): return
        path = _param(params, "path", "")
        node = self.index.get_file(path)
        if not node:
            self._send_json({"error": "File not found"}, 404)
            return
        data = self.index.extract(path)
        try:
            info = get_texture_info(data)
            self._send_json(info)
        except Exception as e:
            self._send_json({"error": str(e)}, 400)

    def _api_texture_image(self, params):
        """Serve the texture as a browser-displayable image (PNG or JPEG)."""
        if not self._require_index(): return
        path = _param(params, "path", "")
        node = self.index.get_file(path)
        if not node:
            self.send_error(404, "File not found")
            return
        data = self.index.extract(path)
        try:
            image_data, mime_type = texture_to_image(data)
            self.send_response(200)
            self.send_header("Content-Type", mime_type)
            self.send_header("Content-Length", str(len(image_data)))
            self.send_header("Cache-Control", "max-age=3600")
            self.end_headers()
            self.wfile.write(image_data)
        except Exception as e:
            self._send_json({"error": str(e)}, 400)

    def _api_texture_dds(self, params):
        """Download the extracted DDS file (stripped .texture header)."""
        if not self._require_index(): return
        path = _param(params, "path", "")
        node = self.index.get_file(path)
        if not node:
            self.send_error(404, "File not found")
            return
        data = self.index.extract(path)
        try:
            dds_data = texture_to_dds(data)
            dds_name = node.entry.filename.rsplit(".", 1)[0] + ".dds"
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Disposition", f'attachment; filename="{dds_name}"')
            self.send_header("Content-Length", str(len(dds_data)))
            self.end_headers()
            self.wfile.write(dds_data)
        except Exception as e:
            self._send_json({"error": str(e)}, 400)

    def _api_texture_png(self, params):
        """Download the texture converted to PNG (lossless)."""
        if not self._require_index(): return
        path = _param(params, "path", "")
        node = self.index.get_file(path)
        if not node:
            self.send_error(404, "File not found")
            return
        data = self.index.extract(path)
        try:
            png_data = texture_to_png(data)
            png_name = node.entry.filename.rsplit(".", 1)[0] + ".png"
            self.send_response(200)
            self.send_header("Content-Type", "image/png")
            self.send_header("Content-Disposition", f'attachment; filename="{png_name}"')
            self.send_header("Content-Length", str(len(png_data)))
            self.end_headers()
            self.wfile.write(png_data)
        except Exception as e:
            self._send_json({"error": str(e)}, 400)

    def _api_texture_png_trimmed(self, params):
        """Download the texture converted to PNG with transparent borders trimmed."""
        if not self._require_index(): return
        path = _param(params, "path", "")
        node = self.index.get_file(path)
        if not node:
            self.send_error(404, "File not found")
            return
        data = self.index.extract(path)
        try:
            png_data = texture_to_png_trimmed(data)
            png_name = node.entry.filename.rsplit(".", 1)[0] + ".png"
            self.send_response(200)
            self.send_header("Content-Type", "image/png")
            self.send_header("Content-Disposition", f'attachment; filename="{png_name}"')
            self.send_header("Content-Length", str(len(png_data)))
            self.end_headers()
            self.wfile.write(png_data)
        except Exception as e:
            self._send_json({"error": str(e)}, 400)

    def _api_download(self, params):
        if not self._require_index(): return
        path = _param(params, "path", "")
        inline = _param(params, "inline", "")
        node = self.index.get_file(path)
        if not node:
            self.send_error(404, "File not found")
            return

        data = self.index.extract(path)
        content_type = mimetypes.guess_type(node.entry.filename)[0] or "application/octet-stream"

        self.send_response(200)
        self.send_header("Content-Type", content_type)
        if not inline:
            self.send_header("Content-Disposition",
                             f'attachment; filename="{node.entry.filename}"')
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    # ---- Source switching ----

    def _api_sources(self, params):
        sources = []
        for name, path in sorted(self.sources.items()):
            loaded = name in self.indexes
            idx = self.indexes.get(name)
            sources.append({
                "name": name,
                "path": path,
                "file_count": idx.total_files if idx else None,
                "loaded": loaded,
            })
        self._send_json({"sources": sources, "current": self.current_source})

    def _api_config(self, params):
        self._send_json({
            "assets_root": self.assets_root,
            "has_sources": bool(self.sources),
        })

    def _api_set_assets_root(self, body):
        from .config import load_config, save_config

        new_root = body.get("assets_root", "").strip()
        if not new_root:
            self._send_json({"error": "No path provided"}, 400)
            return

        root_path = Path(new_root)
        if not root_path.is_dir():
            self._send_json({"error": f"Directory not found: {new_root}"}, 400)
            return

        sources = _scan_asset_dirs(new_root)
        if not sources:
            self._send_json({
                "error": f"No subdirectories with .pigg files found under: {new_root}"
            }, 400)
            return

        default_source = "live" if "live" in sources else next(iter(sources))

        # Build the default index
        t0 = time.time()
        default_index = PiggIndex(sources[default_source])
        elapsed = time.time() - t0
        print(f"Loaded {default_source}: {default_index.total_files:,} files ({elapsed:.1f}s)")

        # Update class-level state
        ExplorerHandler.sources = sources
        ExplorerHandler.indexes = {default_source: default_index}
        ExplorerHandler.current_source = default_source
        ExplorerHandler.assets_root = new_root

        # Persist to config
        config = load_config()
        config["assets_root"] = new_root
        save_config(config)

        self._send_json({
            "success": True,
            "assets_root": new_root,
            "sources": list(sources.keys()),
            "current": default_source,
            "file_count": default_index.total_files,
        })

    def _api_switch_source(self, body):
        name = body.get("source", "")
        if name not in self.sources:
            self._send_json({"error": f"Unknown source: {name}"}, 404)
            return

        # Build index lazily if not yet loaded
        if name not in self.indexes:
            path = self.sources[name]
            t0 = time.time()
            self.indexes[name] = PiggIndex(path)
            elapsed = time.time() - t0
            idx = self.indexes[name]
            print(f"Loaded {name}: {idx.total_files:,} files ({elapsed:.1f}s)")

        ExplorerHandler.current_source = name
        idx = self.indexes[name]
        piggs = idx.list_piggs()
        self._send_json({
            "source": name,
            "file_count": idx.total_files,
            "total_size": idx.total_size,
            "pigg_count": len(piggs),
        })

    # ---- POST API endpoints ----

    def _api_download_zip(self, body):
        if not self._require_index(): return
        paths = body.get("paths", [])
        convert = body.get("convert", "")  # "", "dds", or "png"
        if not paths:
            self._send_json({"error": "No paths provided"}, 400)
            return

        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
            for p in paths:
                node = self.index.get_file(p)
                if not node:
                    continue
                data = self.index.extract(p)
                _add_to_zip(zf, p, data, convert)

        zip_data = buf.getvalue()
        self.send_response(200)
        self.send_header("Content-Type", "application/zip")
        self.send_header("Content-Disposition", 'attachment; filename="pigg_export.zip"')
        self.send_header("Content-Length", str(len(zip_data)))
        self.end_headers()
        self.wfile.write(zip_data)

    def _api_download_folder(self, body):
        """Download an entire directory as a zip, with optional texture conversion.

        Body: {path: "texture_library/...", convert: "dds"|"png"|"", pigg: ""}
        """
        if not self._require_index(): return
        folder_path = body.get("path", "")
        convert = body.get("convert", "")  # "", "dds", or "png"
        pigg_filter = body.get("pigg") or None

        files = self.index.collect_dir_files(folder_path, pigg_filter)
        if not files:
            self._send_json({"error": f"No files found in: {folder_path}"}, 404)
            return

        # Build zip name from the last directory segment
        dir_name = folder_path.rstrip("/").rsplit("/", 1)[-1] or "root"
        zip_name = f"{dir_name}.zip"

        buf = io.BytesIO()
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
            for fnode in files:
                data = self.index.extract(fnode.entry.path)
                _add_to_zip(zf, fnode.entry.path, data, convert)

        zip_data = buf.getvalue()
        self.send_response(200)
        self.send_header("Content-Type", "application/zip")
        self.send_header("Content-Disposition", f'attachment; filename="{zip_name}"')
        self.send_header("Content-Length", str(len(zip_data)))
        self.end_headers()
        self.wfile.write(zip_data)


# ---- Utilities ----

def _add_to_zip(zf: zipfile.ZipFile, path: str, data: bytes, convert: str) -> None:
    """Add a file to a zip, optionally converting .texture files."""
    if convert and path.lower().endswith(".texture"):
        try:
            if convert == "dds":
                converted = texture_to_dds(data)
                zf.writestr(path.rsplit(".", 1)[0] + ".dds", converted)
            elif convert == "png":
                converted = texture_to_png(data)
                zf.writestr(path.rsplit(".", 1)[0] + ".png", converted)
            elif convert == "png-trimmed":
                converted = texture_to_png_trimmed(data)
                zf.writestr(path.rsplit(".", 1)[0] + ".png", converted)
            else:
                zf.writestr(path, data)
        except Exception:
            # Conversion failed — include the raw file instead
            zf.writestr(path, data)
    else:
        zf.writestr(path, data)


def _param(params: dict, key: str, default: str | None = None) -> str | None:
    values = params.get(key, [])
    return values[0] if values else default


def _json_default(obj):
    if isinstance(obj, bytes):
        return obj.hex()
    if isinstance(obj, Path):
        return str(obj)
    return str(obj)


def _file_dict(f: FileNode) -> dict:
    """Convert a FileNode to a plain dict for JSON."""
    e = f.entry
    return {
        "path": e.path,
        "filename": e.filename,
        "directory": e.directory,
        "extension": e.extension,
        "size": e.uncompressed_size,
        "compressed_size": e.compressed_size,
        "pigg": f.pigg_name,
    }


def _scan_asset_dirs(root: str) -> dict[str, str]:
    """Scan a root directory for subdirectories containing .pigg files."""
    sources: dict[str, str] = {}
    root_path = Path(root)
    if not root_path.is_dir():
        return sources
    for child in sorted(root_path.iterdir()):
        if child.is_dir() and any(child.glob("*.pigg")):
            sources[child.name] = str(child)
    return sources


def main():
    parser = argparse.ArgumentParser(description="Pigg Wrangler - browse CoH PIGG archives")
    parser.add_argument("--port", "-p", type=int, default=8085)
    parser.add_argument("--assets-dir", default=None,
                        help="Single assets directory (disables auto-scan)")
    parser.add_argument("--assets-root", default=None,
                        help="Parent directory to scan for asset subdirs")
    parser.add_argument("--no-browser", action="store_true")
    args = parser.parse_args()

    import webbrowser
    from .config import load_config, get_assets_root

    # Load persisted config
    config = load_config()

    # Discover sources
    if args.assets_dir:
        # Explicit single directory -- no scanning
        name = Path(args.assets_dir).name
        sources = {name: args.assets_dir}
        default_source = name
        effective_root = ""
    else:
        # Priority: CLI --assets-root > config file > DEFAULT_ASSETS_ROOT
        if args.assets_root:
            effective_root = args.assets_root
        else:
            effective_root = get_assets_root(config) or DEFAULT_ASSETS_ROOT

        sources = _scan_asset_dirs(effective_root)
        if sources:
            default_source = "live" if "live" in sources else next(iter(sources))
            print(f"Found {len(sources)} asset sources: {', '.join(sources)}")
        else:
            default_source = ""

    # Build the default source index (if we have sources)
    if sources and default_source:
        default_path = sources[default_source]
        print(f"Loading {default_source} ({default_path})...")
        t0 = time.time()
        default_index = PiggIndex(default_path)
        elapsed = time.time() - t0
        pigg_count = len(default_index.list_piggs())
        print(f"Indexed {default_index.total_files:,} files across {pigg_count} PIGG archives ({elapsed:.1f}s)")
        indexes = {default_source: default_index}
    else:
        indexes = {}
        print("No PIGG archives found. Open the web UI to configure your assets path.")

    static_dir = Path(__file__).parent / "static"
    ExplorerHandler.sources = sources
    ExplorerHandler.indexes = indexes
    ExplorerHandler.current_source = default_source
    ExplorerHandler.assets_root = effective_root
    ExplorerHandler.static_dir = static_dir

    url = f"http://127.0.0.1:{args.port}"
    server = ThreadingHTTPServer(("127.0.0.1", args.port), ExplorerHandler)
    print(f"Pigg Wrangler running at {url}")
    print("Press Ctrl+C to stop.")

    if not args.no_browser:
        webbrowser.open(url)

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.")
    finally:
        server.server_close()
