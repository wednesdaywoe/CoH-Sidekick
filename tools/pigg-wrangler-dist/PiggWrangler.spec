# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec for Pigg Wrangler
#
# Build:   cd tools/pigg-wrangler-dist && py -m PyInstaller PiggWrangler.spec
# Output:  dist/PiggWrangler/PiggWrangler.exe  (one-folder mode)

import os

block_cipher = None

# This spec lives in tools/pigg-wrangler-dist/
# The source lives in tools/pigg-wrangler/ (sibling).
dist_dir = os.path.dirname(os.path.abspath(SPEC))
source_dir = os.path.join(os.path.dirname(dist_dir), "pigg-wrangler")
package_dir = os.path.join(source_dir, "pigg_wrangler")

a = Analysis(
    [os.path.join(package_dir, "__main__.py")],
    pathex=[source_dir],  # so "import pigg_wrangler.*" resolves
    binaries=[],
    datas=[
        # Bundle the static web assets
        (os.path.join(package_dir, "static"), os.path.join("pigg_wrangler", "static")),
        # Bundle the icon
        (os.path.join(dist_dir, "piggwrangler.ico"), "."),
    ],
    hiddenimports=[],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    cipher=block_cipher,
)

pyz = PYZ(a.pure, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="PiggWrangler",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=True,
    icon=os.path.join(dist_dir, "piggwrangler.ico"),
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name="PiggWrangler",
)
