#!/usr/bin/env bash
# Open a .mbd in a real Mids Reborn, under Wine.
#
# The point is that Mids fails quietly. An unknown enhancement UID leaves an
# empty slot with no error; a malformed inherent block throws inside LoadBuild's
# try/catch and the build comes up looking merely incomplete. Reading the C#
# tells you what COULD happen — only running it tells you what did. Both bugs
# behind MBDEXPORT-1 were found this way after source reading had produced two
# equally plausible stories.
#
#   ./mids-wine.sh path/to/build.mbd     # open a build
#   ./mids-wine.sh                       # just launch
#
# Setup (once):
#   WINEPREFIX=~/Games/mids-reborn WINEARCH=win64 wineboot -u
#   WINEPREFIX=~/Games/mids-reborn winetricks -q dotnetdesktop8
#   # MRB_Release_*.zip from github.com/LoadedCamel/MidsReborn/releases,
#   # extracted to $PREFIX/drive_c/MidsReborn.
#
# GitHub only ever carries the release the repo was tagged at, and its committed
# database lags — master shipped DB 2026.1.1242 while live was on 2026.5.1337.
# The current app and database come from Mids' own channel instead:
#
#   curl -s https://updates.midsreborn.com/update_manifest.json
#   # then fetch the named .mru files. An .mru is zlib around a
#   # "Mids Reborn Patch Data" container: [str header][i32 count]
#   # then count × ([i32 size][str name][str folder][size bytes]).
#   # The app .mru holds a mids<ver>+db<ver>.zip; unzip that over drive_c/MidsReborn.
# WAS BROKEN 2026-09-10 TO 2026-09-11, in what looked like two stages. It was
# one, and the record is kept because the false second stage cost a day:
#
#   1. Bare `wine MidsReborn.exe` logs "Could not load ICU data. UErrorCode: 2"
#      and the process is gone in ~5s, before any window. Wine 11.0 Staging ships
#      icu.dll/icuin.dll/icuuc.dll in system32, but not the data .NET 8 wants.
#   2. DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1 gets past that — the main window
#      really does appear, titled and sized — and then an "OMIGODHAX" dialog says
#      "The type initializer for 'FastDeepCloner.FastDeepClonerCachedItems' threw
#      an exception", down through Power.ProcessExecutesInner(IPower, Int32
#      rLevel) to MainWindow2.frmMain_Load. No build is displayed.
#
# FIXED 2026-09-11 by app-local ICU, which was the first untried remedy listed
# here and resolves BOTH lines at once:
#
#   curl -sL -o icu.nupkg \
#     https://www.nuget.org/api/v2/package/Microsoft.ICU.ICU4C.Runtime.win-x64/72.1.0.3
#   unzip -j icu.nupkg 'runtimes/win-x64/native/*.dll' -d $PREFIX/drive_c/MidsReborn
#   # then DOTNET_SYSTEM_GLOBALIZATION_APPLOCALICU=72.1.0.3 — set below.
#
# Note the metapackage (Microsoft.ICU.ICU4C.Runtime, 33 KB) carries no DLLs. The
# win-x64 one is 14 MB and holds icudt72.dll, which is the data Wine is missing.
#
# **Stage 2 was never real.** The FastDeepCloner crash is what INVARIANT mode
# does to Mids, not a second defect under it: give .NET real ICU data and the
# main window comes up clean on the same Wine, same prefix, same build of Mids
# that produced the dialog. So the two workarounds were not additive, they were
# alternatives, and the invariant one is simply wrong. The earlier note that the
# dialog fires "regardless of what is loaded" was correct and is why it should
# have been read as environmental sooner — a crash that ignores its input is not
# telling you about its input. Do not read that dialog as evidence about a build
# file, and do not set DOTNET_SYSTEM_GLOBALIZATION_INVARIANT.
#
set -euo pipefail

PREFIX="${MIDS_WINEPREFIX:-$HOME/Games/mids-reborn}"
APP="$PREFIX/drive_c/MidsReborn"
# -f, not -x: an in-app update unzips the exe without a unix exec bit, and Wine
# does not need one. `-x` here reported "no Mids" against a working install.
[ -f "$APP/MidsReborn.exe" ] || { echo "no Mids at $APP — see setup above" >&2; exit 1; }

if [ $# -ge 1 ]; then
  cp "$1" "$APP/test.mbd"
  # Passing the path on the command line only sets LastFileName for the NEXT
  # start (MainWindow2 ctor), and the -load switch is broken upstream — it tests
  # DlgOpen.FileName instead of the argument it was handed. Writing the config is
  # the only route that loads on this run.
  python3 - "$APP/appSettings.json" <<'PY'
import json, sys
p = sys.argv[1]
cfg = json.load(open(p))
cfg['LastFileName'] = r'C:\MidsReborn\test.mbd'
cfg['DisableLoadLastFileOnStart'] = False
# AutomaticUpdates is an AutoUpdate object, not a bool — writing `false` there
# throws a JsonSerializationException on startup before any window appears.
cfg['AutomaticUpdates'] = {'Type': 'Disabled', 'Delay': 3, 'LastChecked': None}
json.dump(cfg, open(p, 'w'), indent=2)
PY
fi

# A prefix booted by Lutris is served by Lutris' own wineserver, and a bare
# `wine` from PATH against it dies with "wine client error: version mismatch"
# and no window. Point WINE at the runner that booted the prefix when that
# happens — e.g.
#   WINE=~/.local/share/lutris/runners/wine/wine-11.10-amd64/bin/wine
WINE="${WINE:-wine}"

# The ICU version must match the DLLs sitting next to MidsReborn.exe; .NET
# resolves icuuc<major>.dll off it. Refuse rather than fall back, because the
# fallback is the invariant mode that produces a plausible-looking broken window.
ICU_VERSION="${MIDS_ICU_VERSION:-72.1.0.3}"
if [ ! -f "$APP/icudt${ICU_VERSION%%.*}.dll" ]; then
  echo "no app-local ICU at $APP/icudt${ICU_VERSION%%.*}.dll — see setup above" >&2
  exit 1
fi

cd "$APP"
WINEPREFIX="$PREFIX" WINEDEBUG=-all \
  DOTNET_SYSTEM_GLOBALIZATION_APPLOCALICU="$ICU_VERSION" \
  setsid nohup "$WINE" MidsReborn.exe >/tmp/mids-wine.log 2>&1 </dev/null &
echo "launched; window appears in ~40s. Screenshot with:"
echo "  W=\$(DISPLAY=:0 xdotool search --name \"Mids' Reborn\" | head -1)"
echo "  DISPLAY=:0 import -window \$W /tmp/mids.png"
echo "An error shows up as a separate 'MessageBoxEx' or 'Microsoft .NET' window — check for those before trusting a screenshot."
