/**
 * Last-resort recovery from "stale tab, new deploy" chunk-load failures.
 *
 * When we deploy a new build, Vite emits chunk files with new hashed
 * filenames. A browser tab still running the previous `index.html` only
 * knows about the old hashes — any lazy `import()` (or `<script>` it
 * tries to load) 404s and the browser reports one of these errors:
 *
 *   - Safari:  "Importing a module script failed"
 *   - Chrome:  "Failed to fetch dynamically imported module"
 *   - Firefox: "error loading dynamically imported module"
 *
 * The service worker (vite-plugin-pwa, prompt mode) is meant to absorb this:
 * it precaches the shell so a controlled tab keeps serving the *old* chunks
 * from cache until the user accepts the update — no 404. It holds only while
 * the old chunks are still somewhere. They are not always: the dataset chunks
 * are deliberately kept out of precache (8-15 MB each, see vite.config.ts) and
 * land in the `sidekick-datasets` runtime cache only if they were fetched
 * while the worker was in control. When that cache is empty or has evicted the
 * entry, a controlled tab asks origin for a chunk the current deploy no longer
 * has, and gets a 404.
 *
 * Which is why the two paths below differ by who is serving the page. On an
 * uncontrolled page a plain reload fetches current assets and fixes it. On a
 * controlled page a plain reload is answered by the same precached shell and
 * fails identically — every time, forever. So there we drop the precache and
 * the worker first, which is what a hard refresh does, and only then reload.
 *
 * Recovery is intentionally silent — the "update available" prompt owns all
 * update messaging now; this is just plumbing. A sessionStorage flag
 * prevents an infinite reload loop if the fresh page also fails (a real
 * bug, not stale assets): the second hit shows a manual-reload toast
 * instead of auto-reloading.
 *
 * The same recovery is inlined in index.html, and has to be: in the common
 * case the failing import is the dataset chunk the boot path needs, so React
 * never mounts and nothing in this bundle ever runs. This copy covers a lazy
 * chunk that fails after the app is alive. They share RECOVERY_FLAG so only
 * one of them can spend the session's single recovery.
 */

import { useUIStore } from '@/stores/uiStore';

const CHUNK_ERROR_PATTERNS: RegExp[] = [
  /Failed to fetch dynamically imported module/i,
  /Importing a module script failed/i,
  /error loading dynamically imported module/i,
  /Unable to preload CSS/i,
  /ChunkLoadError/i,
];

const RELOAD_FLAG = 'sidekick-chunk-reload-attempted';
/** Shared with the inline boot script in index.html — see the note above. */
const RECOVERY_FLAG = 'sidekick-sw-recovery-attempted';

function isChunkLoadError(message: string | undefined | null): boolean {
  if (!message) return false;
  return CHUNK_ERROR_PATTERNS.some((p) => p.test(message));
}

/**
 * Throw away the stale shell and the worker serving it, then reload. Returns
 * true when a reload is on its way, false when the caller should handle it.
 *
 * Only Cache Storage is touched, never localStorage or IndexedDB, so no saved
 * build is at risk.
 *
 * Deleting the precache is the part that makes the reload count: workbox's
 * precache strategy falls back to the network when the entry it wants is gone,
 * so a still-controlled tab with an emptied precache fetches the current
 * index.html and the current chunks it names. Unregistering is belt-and-braces
 * on top of that, and often no-ops, because the reloaded page re-registers the
 * same scope fast enough that the browser revives the registration it was
 * uninstalling. Both measured against a local build on 2026-09-20; the long
 * version is in the index.html copy.
 *
 * The document is re-fetched with `cache: 'reload'` first because GitHub Pages
 * serves index.html with max-age=600 — otherwise the browser's own HTTP cache
 * hands back the same stale HTML for up to ten more minutes and the recovery is
 * spent for nothing.
 */
function recoverFromStaleShell(): boolean {
  if (!navigator.serviceWorker?.controller) return false;
  try {
    if (sessionStorage.getItem(RECOVERY_FLAG) === '1') return false;
    sessionStorage.setItem(RECOVERY_FLAG, '1');
  } catch {
    // Storage blocked (private mode, third-party restrictions). With nowhere to
    // record the attempt this could reload forever, so it does not get to start.
    return false;
  }

  void (async () => {
    try {
      if (window.caches) {
        const keys = await caches.keys();
        await Promise.all(
          keys.filter((k) => k.startsWith('workbox-precache')).map((k) => caches.delete(k)),
        );
      }
      const registrations = await navigator.serviceWorker.getRegistrations();
      await Promise.all(registrations.map((r) => r.unregister()));
      await fetch(window.location.href, { cache: 'reload', credentials: 'same-origin' });
    } catch {
      // Whatever failed, the reload is still the best move left.
    }
    window.location.reload();
  })();

  return true;
}

let handled = false;

function tryShowToast(toast: Parameters<ReturnType<typeof useUIStore.getState>['showToast']>[0]) {
  try {
    useUIStore.getState().showToast(toast);
    return true;
  } catch {
    return false;
  }
}

function handleChunkError(message: string) {
  if (handled) return;
  handled = true;

  const alreadyReloaded = sessionStorage.getItem(RELOAD_FLAG) === '1';

  if (alreadyReloaded) {
    // Second hit in the same session — the reload didn't fix it. Don't
    // loop; ask the user to reload manually (hard-refresh hint covers the
    // edge case where the SW cache is serving the stale shell).
    const shown = tryShowToast({
      message: 'Sidekick failed to load part of the app. Try a hard refresh (Ctrl+Shift+R) or clear your cache.',
      tone: 'warning',
      durationMs: 0,
      action: { label: 'Reload', onClick: () => window.location.reload() },
    });
    if (!shown && typeof window.confirm === 'function') {
      if (window.confirm('Sidekick failed to load part of the app. Reload?')) {
        window.location.reload();
      }
    }
    return;
  }

  // First hit. No toast on either path — the update prompt owns update
  // messaging, and the reload is immediate so a toast wouldn't paint anyway.
  sessionStorage.setItem(RELOAD_FLAG, '1');

  // A service worker is serving this page, so reloading into the same precached
  // shell would reproduce the failure exactly. Drop the shell first.
  if (recoverFromStaleShell()) {
    console.warn('[sidekick] chunk load failed — dropping the stale shell and reloading:', message);
    return;
  }

  // Nothing controls this page: a stale-asset 404 on a tab the worker hasn't
  // taken over. Reload once to fetch current assets.
  console.warn('[sidekick] chunk load failed — auto-reloading once:', message);
  window.location.reload();
}

export function installChunkErrorReload() {
  // After the app has been alive for a while, clear the reload-attempted
  // flag so a future deploy can use the auto-reload path again. 30 s is
  // long enough that any boot-time chunk failure has already fired.
  window.setTimeout(() => sessionStorage.removeItem(RELOAD_FLAG), 30_000);

  window.addEventListener('error', (event) => {
    if (isChunkLoadError(event.message)) handleChunkError(event.message);
  });

  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason as unknown;
    const message =
      typeof reason === 'string'
        ? reason
        : reason && typeof reason === 'object' && 'message' in reason
          ? String((reason as { message: unknown }).message)
          : undefined;
    if (isChunkLoadError(message)) handleChunkError(message ?? 'chunk load error');
  });
}
