import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import * as Sentry from '@sentry/react'
import { isCrashReportingEnabled } from '@/utils/crash-consent'
import './index.css'
import App from './App'
import { loadDataset, isDatasetId, type DatasetId } from '@/data/dataset'

// Register window.cohDebug for calculation debug logging + fallback warnings
import '@/utils/calc-debug'
import '@/utils/fallback-warnings'
import { installChunkErrorReload } from '@/utils/chunk-error-reload'
import { applyColorTheme, isColorThemeId, DEFAULT_COLOR_THEME, applyColorMode, isColorMode, DEFAULT_COLOR_MODE } from '@/data/core/themes'

// Apply the persisted color theme + mode to <html> before React mounts so
// there's no flash of the default chrome on reload. Read straight from
// localStorage rather than the store (which hasn't been created yet here).
function bootColorTheme() {
  try {
    const raw = localStorage.getItem('coh-planner-ui')
    const state = raw ? JSON.parse(raw)?.state : undefined
    // The retired 'imperial-light' theme maps to imperial + light mode.
    if (state?.colorTheme === 'imperial-light') {
      applyColorTheme('imperial')
      applyColorMode('light')
      return
    }
    applyColorTheme(isColorThemeId(state?.colorTheme) ? state.colorTheme : DEFAULT_COLOR_THEME)
    applyColorMode(isColorMode(state?.colorMode) ? state.colorMode : DEFAULT_COLOR_MODE)
  } catch {
    applyColorTheme(DEFAULT_COLOR_THEME)
    applyColorMode(DEFAULT_COLOR_MODE)
  }
}
bootColorTheme()

// Install before any dynamic imports run so chunk-load failures during boot
// (e.g. the dataset import below) are caught and recovered from rather than
// leaving the user staring at a blank screen after a fresh deploy.
installChunkErrorReload()

// Sentry — production only. DSN is injected at build time; if it's missing
// the init is a no-op so dev/local builds stay silent.
if (import.meta.env.PROD && import.meta.env.VITE_SENTRY_DSN) {
  Sentry.init({
    dsn: import.meta.env.VITE_SENTRY_DSN,
    // F36. Checked here rather than around `init`, so the switch takes effect on the next
    // crash instead of on the next reload — a setting you have to restart the app to apply
    // is one people reasonably assume did not work. Returning null drops the event before
    // it is sent.
    beforeSend: (event) => (isCrashReportingEnabled() ? event : null),
    // Stale-tab chunk failures are recovered by installChunkErrorReload —
    // filter them here so Sentry doesn't log noise after every deploy.
    ignoreErrors: [
      /Failed to fetch dynamically imported module/i,
      /Importing a module script failed/i,
      /error loading dynamically imported module/i,
      /Unable to preload CSS/i,
      /ChunkLoadError/i,
    ],
  })
}

function ErrorFallback() {
  // Escape hatch for the "corrupt build crash loop": a bad imported/persisted
  // build crashes the planner render → this fallback shows → plain Reload
  // re-reads the same persisted build from localStorage and crashes again.
  // (A failed /import also leaves its #<data> fragment in the URL.) Clearing
  // the persisted build and booting at the app root — dropping any fragment —
  // breaks the loop without making the user clear browser storage by hand.
  const startFresh = () => {
    try {
      localStorage.removeItem('coh-planner-build')
    } catch {
      /* localStorage unavailable (private mode / disabled) — reload anyway */
    }
    // Hard navigation to BASE_URL drops the /import#<data> fragment and forces
    // a clean boot with no persisted build.
    window.location.href = import.meta.env.BASE_URL || '/'
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-slate-950 p-6">
      <div className="max-w-md text-center space-y-4">
        <h1 className="text-xl font-semibold text-slate-100">Something went wrong</h1>
        <p className="text-sm text-slate-400">
          Sidekick hit an unexpected error and couldn't render. The issue has been reported.
          Try reloading — if it keeps happening (e.g. a build that won't load), start fresh
          to clear the saved build and return to the planner.
        </p>
        <div className="flex items-center justify-center gap-3">
          <button
            onClick={() => window.location.reload()}
            className="px-4 py-2 rounded bg-sky-600 hover:bg-sky-500 text-on-info text-sm font-medium"
          >
            Reload
          </button>
          <button
            onClick={startFresh}
            className="px-4 py-2 rounded bg-slate-700 hover:bg-slate-600 text-slate-100 text-sm font-medium"
          >
            Start fresh
          </button>
        </div>
      </div>
    </div>
  )
}

// Resolves after the dataset is loaded AND the persisted build is rehydrated.
// Anything that mutates the build during boot (the .skif handler below) must
// await this so it lands after hydration instead of being clobbered by it.
let bootReady: Promise<void>

// Handle .skif files opened via PWA file association
if ('launchQueue' in window) {
  (window as any).launchQueue.setConsumer(async (launchParams: any) => {
    for (const fileHandle of launchParams.files) {
      const file = await fileHandle.getFile();
      const text = await file.text();
      try {
        const { useBuildStore } = await import('./stores/buildStore');
        // Import the opened file AFTER boot so it wins over the rehydrated build
        // and has the active dataset available for power resolution.
        await bootReady
        useBuildStore.getState().importBuild(text);
      } catch {
        console.error('Failed to import .skif file');
      }
    }
  });
}

// Load the active dataset before mounting React. The data-layer facades
// (e.g. @/data/at-tables, @/data/archetypes) read from the active dataset
// synchronously, so they must not be touched until this resolves.
//
// Resolution order:
//   1. `?serverId=<id>` URL query param (dev/QA override).
//   2. Pre-peek at the persisted Zustand store so the saved build's
//      dataset loads first and we don't flash the wrong one.
//   3. Default to Homecoming.
function bootServerId(): DatasetId {
  try {
    const param = new URLSearchParams(window.location.search).get('serverId');
    if (isDatasetId(param)) return param;
    const raw = localStorage.getItem('coh-planner-build');
    if (!raw) return 'homecoming';
    const parsed = JSON.parse(raw);
    // New per-server shape stores the last-active server; fall back to the
    // legacy single-slot `build.serverId` for stores that haven't migrated yet
    // (bootServerId runs against raw localStorage, before persist migrate).
    const id = parsed?.state?.activeServerId ?? parsed?.state?.build?.serverId;
    return isDatasetId(id) ? id : 'homecoming';
  } catch {
    return 'homecoming';
  }
}

// ?previewCapture=<id> boots the app in a hidden capture-mode iframe to render
// and snapshot someone else's build (see
// streams/BUILD_PREVIEW_BACKFILL_PLAN.md, PREVBF4/5). buildStore's persist
// already swapped to an in-memory stub for this boot (see
// isPreviewCaptureMode in stores/buildStore.ts), so importBuild() here can't
// touch this browser's real saved build — it only has to win the race against
// the (harmless, empty) rehydrate above.
function previewCaptureId(): string | null {
  return new URLSearchParams(window.location.search).get('previewCapture')
}

bootReady = loadDataset(bootServerId()).then(async () => {
  // Hydrate the persisted build only AFTER the dataset is loaded — its rehydrate
  // migrations (inherent reconciliation, syncBuildDefinitions) read the active
  // dataset. The store is created with `skipHydration: true`; bootServerId()
  // already pre-peeked the persisted serverId so the matching dataset is active.
  const { useBuildStore } = await import('./stores/buildStore')
  await useBuildStore.persist.rehydrate()

  const captureId = previewCaptureId()
  if (captureId) {
    // Records whether the *target* build actually loaded — read by
    // SharePreviewCapture's capture pass (PREVBF5) before it uploads
    // anything. Without this, a fetch failure would silently capture and
    // upload the default empty build as if it were captureId's real preview.
    window.__previewCapture = { id: captureId, ready: false }
    try {
      const { getSharedBuild } = await import('@/services/sharedBuilds')
      const shared = await getSharedBuild(captureId)
      if (shared) {
        useBuildStore.getState().importBuild(JSON.stringify(shared.build_json))
        window.__previewCapture.ready = true
      }
    } catch {
      // ready stays false — PREVBF5 reports failure instead of capturing.
    }
  }
})

bootReady.then(() => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <Sentry.ErrorBoundary fallback={<ErrorFallback />}>
        <App />
      </Sentry.ErrorBoundary>
    </StrictMode>,
  )
})
