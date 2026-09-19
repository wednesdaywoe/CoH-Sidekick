/// <reference types="vitest/config" />
import { defineConfig, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { sentryVitePlugin } from '@sentry/vite-plugin'
import { VitePWA } from 'vite-plugin-pwa'
import path from 'path'
import { execSync } from 'child_process'
import { createHash } from 'node:crypto'
import { existsSync } from 'node:fs'

const BUILD_TIME = Date.now()

/** Parse git log into changelog entries for injection at build time */
function getChangelogData(): string {
  try {
    const raw = execSync('git log --format="%H|%aI|%s" --no-merges -200', {
      encoding: 'utf-8',
      cwd: __dirname,
    }).trim()

    const entries = raw.split('\n').filter(Boolean).map(line => {
      const [hash, date, ...rest] = line.split('|')
      let message = rest.join('|') // subject may contain |
      let type = 'update'

      // Extract conventional commit prefix
      const prefixMatch = message.match(/^(feat|fix|refactor|chore|docs|ci|style|test|perf)(\(.+?\))?:\s*/i)
      if (prefixMatch) {
        const prefix = prefixMatch[1].toLowerCase()
        type = prefix === 'feat' ? 'feat' : prefix === 'fix' ? 'fix' : 'update'
        message = message.slice(prefixMatch[0].length)
      }

      // Clean up message
      message = message.replace(/Co-Authored-By:.*/gi, '').trim()
      if (message.length > 0) {
        message = message.charAt(0).toUpperCase() + message.slice(1)
      }

      return { hash: hash.slice(0, 7), date: date.split('T')[0], message, type }
    })

    return JSON.stringify(entries)
  } catch {
    // Fallback if git is unavailable
    return '[]'
  }
}

const CHANGELOG_DATA = getChangelogData()

/**
 * Inject a Content-Security-Policy <meta> into the built index.html.
 *
 * Build-only: GitHub Pages can't set HTTP headers, and dev/HMR relies on inline
 * scripts + eval that a strict CSP would block. The two inline <script> blocks
 * in index.html (the GH-Pages SPA redirect and the serverId loading label) are
 * SHA-256 hashed here automatically, so `script-src` never needs 'unsafe-inline'
 * — and because we hash the *post-transform* HTML, the hashes always match what
 * actually ships (edit the inline scripts freely; the build re-hashes them).
 *
 * The security-critical directive is `connect-src`: even if a dependency were
 * compromised, the browser blocks it from exfiltrating the Supabase session
 * token to any host not on this list. `frame-src` allows this origin itself
 * (the off-screen share-image capture iframe) plus the in-app
 * "Support Sidekick" donation iframe (Buy Me a Coffee), which is cross-origin
 * and therefore already walled off from this origin's storage by the browser.
 *
 * Note: `frame-ancestors` (clickjacking) is header-only and ignored in <meta>,
 * so it's omitted — it would need a host that can send response headers.
 */
function cspPlugin(): Plugin {
  return {
    name: 'sidekick-csp',
    apply: 'build',
    transformIndexHtml: {
      order: 'post',
      handler(html) {
        // Match every INLINE <script> (no src= attribute) and hash its body.
        const inlineScript = /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/gi
        const scriptHashes: string[] = []
        for (const match of html.matchAll(inlineScript)) {
          const code = match[1]
          if (!code) continue
          const digest = createHash('sha256').update(code, 'utf8').digest('base64')
          scriptHashes.push(`'sha256-${digest}'`)
        }

        const csp = [
          `default-src 'self'`,
          // 'wasm-unsafe-eval' is what lets the calc engine run: Chrome governs
          // WebAssembly.instantiate under script-src, and without this token it blocks the
          // .wasm outright — the whole dashboard reads 0% with only a console error. It grants
          // wasm compilation ONLY; it does not re-enable eval() or inline script.
          `script-src 'self' 'wasm-unsafe-eval' ${scriptHashes.join(' ')}`.trim(),
          // Landing page + Tailwind use inline style="" attributes (un-hashable);
          // fonts.googleapis.com serves the SN Pro / Nunito stylesheet.
          `style-src 'self' 'unsafe-inline' https://fonts.googleapis.com`,
          `font-src 'self' https://fonts.gstatic.com data:`,
          // Same-origin icons + data URIs + OAuth (Discord) / Supabase avatars.
          `img-src 'self' data: https://cdn.discordapp.com https://*.supabase.co`,
          // The complete set of hosts the app legitimately talks to. Anything
          // else (i.e. an exfiltration attempt) is blocked by the browser:
          //   *.supabase.co  — shared builds + auth (REST + realtime websocket)
          //   *.sentry.io    — error reporting
          //   wednesdaywoe.github.io — status banner status.json
          //   ...workers.dev — feedback form endpoint
          `connect-src 'self' https://*.supabase.co wss://*.supabase.co https://*.sentry.io https://wednesdaywoe.github.io https://coh-planner-feedback.wedswoe.workers.dev`,
          // 'self' is the hidden preview-capture iframe: BuildDetailPage frames
          // this same origin at /?previewCapture=<id> to render a build's share
          // image off-screen (streams/BUILD_PREVIEW_BACKFILL_PLAN.md, PREVBF7).
          // It was missing until 2026-09-03, so the on-view backfill worked in
          // dev — where there is no CSP — and was blocked in production for
          // every build; only the quick-share path ever wrote an image.
          // The rest is the in-app "Support Sidekick" donation iframe.
          `frame-src 'self' https://buymeacoffee.com https://www.buymeacoffee.com`,
          `worker-src 'self'`,
          `manifest-src 'self'`,
          `base-uri 'self'`,
          `form-action 'self'`,
          `object-src 'none'`,
        ].join('; ')

        return html.replace(
          /<head>/i,
          `<head>\n    <meta http-equiv="Content-Security-Policy" content="${csp}" />`,
        )
      },
    },
  }
}

// https://vite.dev/config/
/**
 * Vitest-only: swap the browser engine module for its Node twin.
 *
 * `src/engine/engine.ts` is browser-shaped (`__wbg_init` + `fetch` + a `?url` wasm import),
 * so under vitest it can never load a dataset — `recalcJson` returned null and every test
 * calling `calculateCharacterTotals` graded the all-zero fallback instead of the engine that
 * ships. `engine.node.ts` is the same API over the wasm-node artifact.
 *
 * A plugin rather than `test.alias` because two different specifiers reach the module (the
 * hook's `@/engine/engine`, engineTotals' relative `./engine`) and the relative one needs its
 * importer to disambiguate. Inert outside vitest — `dev`/`build` never see it.
 */
const engineNodeSwapPlugin: Plugin = {
  name: 'engine-node-swap',
  enforce: 'pre',
  resolveId(source, importer) {
    if (!process.env.VITEST) return null
    // `@/…` is rewritten to an absolute path by vite's own alias plugin before this runs,
    // so match the resolved form as well as engineTotals' relative `./engine`.
    const id = source.replace(/\\/g, '/')
    const isEngine =
      id.endsWith('/src/engine/engine') ||
      (id === './engine' && !!importer && importer.replace(/\\/g, '/').includes('/src/engine/'))
    return isEngine ? path.resolve(__dirname, 'src/engine/engine.node.ts') : null
  },
}

/**
 * Vitest-only: swap a dataset's module graph for its prebuilt esbuild bundle.
 *
 * A dataset is ~7,300 generated modules and ~48 MB of TypeScript. 142 of the suite's
 * test files reach one through `loadDataset()`, and each paid ~14s to transform and
 * execute that graph again inside its own isolated worker — most of the suite's wall
 * clock, for data that never changes between files.
 *
 * `scripts/build-dataset-bundles.mjs` flattens each dataset to one import-free ESM file
 * before the run (~2s for all four). Node imports that natively in ~200ms, so the swap
 * is worth roughly 50x per test file that loads a dataset.
 *
 * Only the dataset ROOT specifier is redirected — `./datasets/homecoming`, or the
 * absolute form vite's alias plugin rewrites `@/data/datasets/homecoming` into. A test
 * importing a file INSIDE the dataset folder still gets the real module, and so does an
 * explicit `.../datasets/homecoming/index`, which is how
 * `src/data/dataset-bundle-fidelity.test.ts` grades the bundle against the graph.
 *
 * Fails loud rather than falling through: a dataset root with no bundle means globalSetup
 * did not run, and resolving to the graph instead would just be the slow suite, silently.
 * Inert outside vitest — `dev`/`build` never see it.
 */
const datasetBundleSwapPlugin: Plugin = {
  name: 'dataset-bundle-swap',
  enforce: 'pre',
  resolveId(source) {
    if (!process.env.VITEST) return null
    const id = source.replace(/\\/g, '/')
    const match = /(?:^|\/)datasets\/([^/]+)$/.exec(id)
    if (!match) return null
    const name = match[1]
    // Only a real dataset root — anything else that happens to sit under a "datasets"
    // folder resolves normally.
    if (!existsSync(path.resolve(__dirname, 'src/data/datasets', name, 'index.ts'))) return null
    const bundle = path.resolve(__dirname, '.dataset-bundles', name + '.mjs')
    if (!existsSync(bundle)) {
      throw new Error(
        `no test bundle for dataset '${name}' at ${bundle}. ` +
          `vitest builds these in globalSetup (scripts/build-dataset-bundles.mjs); ` +
          `run 'node scripts/build-dataset-bundles.mjs' if you are driving vite by hand.`,
      )
    }
    return bundle
  },
}

export default defineConfig({
  // Base path — '/' for custom domain (coh-sidekick.com)
  base: '/',
  plugins: [
    engineNodeSwapPlugin,
    datasetBundleSwapPlugin,
    react(),
    tailwindcss(),
    VitePWA({
      // Controlled updates: a new service worker installs and waits; the app
      // surfaces an "update available" prompt and only activates on user
      // confirmation. Never silently auto-update.
      registerType: 'prompt',
      // Keep the hand-maintained public/manifest.json (it carries the .skif
      // file_handlers entry the generator would not preserve). Don't generate
      // or inject a manifest — index.html already links it.
      manifest: false,
      injectRegister: null,
      workbox: {
        // Precache the app shell: JS, CSS, HTML, plus the calc engine (the .wasm and
        // the three contract bundles). Deliberately NOT images — public/img holds
        // hundreds of enhancement/archetype icons; precaching them would download the
        // entire icon library on SW install. They are runtime-cached on demand below.
        //
        // The engine is in the shell because without it the app computes NOTHING: a
        // cold load that can't reach the .wasm or its bundle renders the "engine failed"
        // banner and zeros everywhere. It is shell, not data.
        //
        // Precache rather than runtime-cache, specifically because the bundles are NOT
        // content-hashed (`homecoming.json.gz` keeps its name across rebuilds). A
        // CacheFirst runtime rule would therefore pin the FIRST bundle a visitor ever
        // fetched and happily feed it to a newer .wasm forever — a silently mismatched
        // engine, which is worse than a slow one. Workbox revisions precached entries by
        // content hash, so a changed bundle is re-fetched when the new SW installs, and
        // the engine's two halves update together or not at all.
        //
        // Cost is ~5.8 MB on SW install (1.5 wasm + 1.9/1.2/1.2 bundles) and yes, two of
        // the three bundles are for datasets a given visitor may never open — unlike the
        // globIgnored dataset chunks below, they're small enough that splitting them
        // would buy less than the mismatch risk it reintroduces.
        globPatterns: ['**/*.{js,css,html,wasm}', 'engine/contract/*.json.gz'],
        // Exclude the per-dataset chunks (`dataset-<id>-*.js`, named via
        // build.rollupOptions.output.chunkFileNames). Each is 8-15 MB and only
        // ONE is ever loaded per visitor (the active server, chosen at boot),
        // so precaching all three would download ~34 MB of data on SW install —
        // ~2/3 of it for datasets that visitor never opens.
        //
        // They are NOT left to the network, though — see the dataset runtimeCaching
        // rule below. Excluding them from precache while the precached shell hard-
        // references them by hash is what caused the "Taking a while to load?" boot
        // failures: a tab controlled by deploy N's SW serves deploy N's shell from
        // precache, but that shell imports `dataset-homecoming-<N-hash>.js`, which
        // deploy N+1 deleted. Boot 404s before React mounts, so neither the update
        // prompt nor the recovery toast can render, and the auto-reload re-serves the
        // same stale shell. Only a hard refresh escapes. Verified in production
        // 2026-07-26: two deploys 7 min apart left the older shell pointing at three
        // dataset chunks that were already 404 at origin.
        globIgnores: ['assets/dataset-*.js'],
        // engine.ts fetches each contract bundle as `<server>.json.gz?v=<content hash>` so an
        // HTTP cache can't hand a stale bundle to a newer .wasm (see engine.ts). Precache keys
        // carry no query, so `v` must be ignored here or every bundle fetch would MISS the
        // precache and go to network — losing offline support for the engine's data half.
        // Ignoring it is safe precisely because precache already pairs the halves by revision:
        // for an SW-controlled load the parameter has nothing left to do. The two defaults
        // (`utm_*`, `fbclid`) are restated because setting this replaces them.
        ignoreURLParametersMatching: [/^utm_/, /^fbclid$/, /^v$/],
        // Precache is now the app shell only (~1.7 MB entry + CSS). This low cap
        // is a regression tripwire: a globbed file over the limit is a hard
        // build error, so if a future change re-leaks a whole dataset (8 MB+)
        // into an eager/precached chunk, the build fails loudly here.
        // History: 16 MiB, then 40 MiB on 2026-07-17 to fit the ~29 MB data
        // chunk that used to be welded into the eager entry; that data now
        // lives in the globIgnored dataset chunks (perf/dataset-lazy-facades).
        maximumFileSizeToCacheInBytes: 5 * 1024 * 1024,
        cleanupOutdatedCaches: true,
        // SPA navigations are served the precached index.html — instant + works
        // offline. Freshness is governed by the controlled update prompt (the
        // waiting SW carries the new shell), so navigations are precache-backed
        // rather than NetworkFirst. See "Sidekick reliability plan.md".
        navigateFallback: '/index.html',
        runtimeCaching: [
          {
            // The per-dataset chunks that globIgnores keeps out of precache.
            // Runtime-cached so a shell frozen at deploy N can still resolve the
            // deploy-N dataset chunk after deploy N+1 has removed it from origin —
            // restoring the "a controlled tab keeps serving the old chunks" premise
            // that chunk-error-reload.ts is written against.
            //
            // CacheFirst is safe here in a way it explicitly is NOT for the contract
            // bundles above: those keep a fixed name across rebuilds, so a pinned
            // copy can be fed to a newer .wasm. Dataset chunks are content-hashed, so
            // a cached entry is byte-identical to what its URL means forever, and can
            // only ever be served to the shell that asked for that exact hash.
            //
            // Only chunks actually fetched are stored, so the ~34 MB precache-all
            // objection doesn't apply — a typical visitor holds one. Capped by entry
            // count (≈2 deploys × 3 datasets) and deliberately given NO maxAgeSeconds:
            // a TTL would expire the very chunk a stale shell still needs.
            urlPattern: ({ url, sameOrigin }) =>
              sameOrigin && /^\/assets\/dataset-[a-z]+-[\w-]+\.js$/.test(url.pathname),
            handler: 'CacheFirst',
            options: {
              cacheName: 'sidekick-datasets',
              expiration: { maxEntries: 6 },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
          {
            // Same-origin images (the /img icon library). CacheFirst with an
            // entry cap + TTL so the cache can't grow unbounded.
            urlPattern: ({ request, sameOrigin }) =>
              sameOrigin && request.destination === 'image',
            handler: 'CacheFirst',
            options: {
              cacheName: 'sidekick-images',
              expiration: {
                maxEntries: 600,
                maxAgeSeconds: 60 * 60 * 24 * 30, // 30 days
              },
              cacheableResponse: { statuses: [0, 200] },
            },
          },
        ],
      },
      // The status banner's status.json is fetched cross-origin from the
      // separate status repo, so the SW never intercepts or caches it — no rule
      // needed. version.json polling has been removed in favour of this SW's
      // own update lifecycle.
    }),
    // Sentry source map upload — only loaded when SENTRY_AUTH_TOKEN is set
    // (i.e. CI builds). Skipping it locally avoids needing the token and
    // sidesteps any network activity from the plugin during dev builds.
    ...(process.env.SENTRY_AUTH_TOKEN
      ? [sentryVitePlugin({
          org: 'wednesdaywoe',
          project: 'coh-sidekick',
          authToken: process.env.SENTRY_AUTH_TOKEN,
        })]
      : []),
    // Keep LAST so its post-transform runs after any other HTML transform,
    // ensuring the inline-script hashes match the final shipped bytes.
    cspPlugin(),
  ],
  define: {
    __BUILD_TIME__: JSON.stringify(BUILD_TIME),
    __CHANGELOG_DATA__: CHANGELOG_DATA,
  },
  test: {
    // Dataset loading is seconds, not milliseconds, so the 5s defaults are wrong for this
    // suite — bare `vitest` used to red ~28 files on loadDataset alone. They belong here
    // rather than in the npm script so every entry point grades identically.
    testTimeout: 120_000,
    hookTimeout: 120_000,
    // Flatten each dataset to one ESM file before the run; see datasetBundleSwapPlugin.
    globalSetup: ['./scripts/build-dataset-bundles.mjs'],
    server: {
      deps: {
        // The bundles are plain, import-free ESM — let node import them directly. Routing
        // 30 MB through vite's transform costs ~7s per test file and changes nothing.
        external: [/[.]dataset-bundles[/]/],
      },
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      react: path.resolve(__dirname, 'node_modules/react'),
      'react-dom': path.resolve(__dirname, 'node_modules/react-dom'),
      // Plasmic/SSR aliases removed
    },
  },
  // Serve public folder for static assets (img folder is inside public/)
  publicDir: 'public',
  build: {
    outDir: 'dist',
    // 'hidden' generates source maps for Sentry upload without exposing a
    // sourceMappingURL comment in the deployed JS (so browsers won't fetch
    // the maps from the public site).
    //
    // Which is also why they are conditional on the same token the upload plugin is:
    // with no sourceMappingURL nothing but Sentry can ever ask for them, so a build
    // without the token writes ~75 MB of maps that are read by nobody and deleted. They
    // cost 1.2 GB of peak heap and 12s of the ~42s build, measured on this tree. The
    // shipped JS neither references them nor changes shape — 'hidden' puts no
    // sourceMappingURL in it either way — so this changes what a local or build-check run
    // spends, and changes production not at all: deploy.yml sets SENTRY_AUTH_TOKEN, so the
    // maps it uploads are still generated.
    sourcemap: process.env.SENTRY_AUTH_TOKEN ? 'hidden' : false,
    rollupOptions: {
      output: {
        // Give the per-dataset dynamic-import chunks a stable, greppable name
        // (`dataset-<id>-<hash>.js`) so the service worker can exclude them
        // from precache by glob (see workbox.globIgnores). This is naming ONLY
        // — it does NOT move modules between chunks (a directory-based
        // manualChunks would, and would re-pull the ~100 KB of small modules
        // still statically imported from datasets/* back into the eager entry).
        // Each dataset's index.ts is the facade module of its own dynamic chunk.
        //
        // The directory name is captured rather than listed. A listed roster misses the
        // next dataset silently and expensively: brainstorm fell out, so its chunk was
        // named after its facade module (`index-*.js`), globIgnores did not match it, and
        // workbox tried to precache 20 MB of dataset — failing the build at the SW step
        // with a message that names the chunk but not the cause.
        chunkFileNames: (chunkInfo) => {
          const id = chunkInfo.facadeModuleId
          const m = id?.match(/[/\\]datasets[/\\]([^/\\]+)[/\\]index\.ts$/)
          return m ? `assets/dataset-${m[1]}-[hash].js` : 'assets/[name]-[hash].js'
        },
      },
    },
  },
  server: {
    port: 3000,
    open: true,
  },
})
