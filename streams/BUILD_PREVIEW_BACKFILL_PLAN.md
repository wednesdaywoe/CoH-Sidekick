---
project: coh-sidekick
kind: plan
title: Backfill Build Preview Images
id-prefix: PREVBF
area: shared-builds
created: 2026-09-03
satisfied: 2026-09-03
relates:
  - BUILD_PREVIEW_IMAGE_PLAN.md
  - OPEN_ITEMS.md
---

# Backfill Build Preview Images

Follow-on to [BUILD_PREVIEW_IMAGE_PLAN.md](BUILD_PREVIEW_IMAGE_PLAN.md) (closed
2026-09-03), which only produces a preview image at share time (EMBED3). Every
build shared *before* that shipped — the whole HC Brainstorm import, plus
anything shared/updated by any means that isn't the live app's Share button
(Copy Link, metadata Edit, visibility toggle) — has `preview_image_path: null`
forever and will keep unfurling with the generic site-wide image. Confirmed
live 2026-09-03 against three real pre-existing builds (`TIZbOh3wV5`,
`OdCM_Drr9R`, `L-Szyl6zcY`) — none of the actions available on
`BuildDetailPage` call `shareBuild()`/`capturePreviewBase64()`, so none of
them ever populate the column no matter how many times the link is copied.

**Widened 2026-09-03, same day, before any code landed:** EMBED's own
"Deferred" section named a second, related gap — "re-render older previews if
the visual template changes later... revisit if the template changes" — and
gated it on the template actually changing. It just did (the EMBED2
legibility fix, commit `614261b342`, bigger type/icons on the same 1200×630
layout), and the user hit it immediately: a build shared under the old
template still shows the old small-text image indefinitely, confirmed live
(bundle check showed the deployed JS is current; the stale image is real, not
a caching artifact of the fix itself). A *missing* image and a *stale* image
are the same user-facing failure — "this build's image doesn't reflect
reality" — and the fix for one is 90% the fix for the other (same capture
pipeline, same isolation mechanism, same trigger point); building
missing-only now and bolting on staleness right after would just be rework.
Widening the thesis rather than opening a third doc.

**Thesis:** a public or unlisted build's preview image is generated when
missing and refreshed when it was rendered under an older visual template
than the one live today — both automatically, in an isolated hidden capture
pass the next time any browser views its `/builds/:id` page — without
visibly affecting that browser's own UI or touching that browser's own
locally-saved build.

**satisfied-when:** PREVBF1..PREVBF8 all `[x]`

## Preconditions

Checked now (cheap, code-derived):
- EMBED1–6 all shipped and live — this doc's whole mechanism reuses
  `SharePreviewCapture`/`BuildPreviewCard`/`capturePreviewBase64()`/the
  `build-previews` bucket unmodified; nothing here re-implements rendering.
- `buildStore`'s `persist` middleware writes to a single fixed `localStorage`
  key (`'coh-planner-build'`), with no cross-tab `BroadcastChannel` —
  confirmed in [src/stores/buildStore.ts](../src/stores/buildStore.ts)
  (`name: 'coh-planner-build'`, plain `createJSONStorage(() => localStorage)`).
  This is the fact the isolation decision below leans on: writes are
  same-origin, always-on (`persist` writes on every state change), and
  visible to every tab only via that one shared key.
- `StatsDashboard` (and therefore `SharePreviewCapture`) is mounted inside
  `MainLayout`, which wraps *every* route via the root route's `Outlet` —
  confirmed in [src/router.tsx](../src/router.tsx) and
  [MainLayout.tsx](../src/components/layout/MainLayout.tsx). `MainLayout`
  also renders live store state app-wide (header build name/archetype/sets),
  confirmed live via Playwright. This is why hijacking the *visible* tab's
  store (see Decision below) was rejected, not just theorized about.
- `getSharedBuild(id)` is already callable with no auth, by any visitor —
  confirmed (it's what `BuildDetailPage` calls today for anyone).

## Decision — isolation mechanism (2026-09-03, engineering call)

A hidden same-origin `<iframe>` boots a **fresh instance of the app** in a
"capture mode" (own JS realm, own module state, own `useBuildStore`
instance) to render and capture the backfill target, rather than importing
it into the *visible* tab's singleton store and restoring it afterward.

Rejected: same-tab store hijack (`importBuild()` the target, capture, restore
the original state). Two independent reasons, not one: (1) `MainLayout`
renders live store state app-wide, so even a brief hijack would flicker the
wrong build's name/archetype into the header for every visitor who happens to
land on an un-imaged build's page — user-visible breakage for a feature that
is supposed to be invisible; (2) `persist` writes the hijacked state to the
visitor's *real* `localStorage` key on every change, so the hijack window is
a real (if narrow) data-loss risk — a crash or tab-close mid-window would
permanently leave the backfill target sitting in that visitor's own saved
build slot. An invisible background feature has no business risking someone
else's actual work.

## Decision — capture-mode storage (2026-09-03, engineering call)

`buildStore`'s `persist` `storage` adapter swaps to an in-memory stub
(never touches real `localStorage`) whenever the app boots with the
capture-mode URL param present. Confined to `buildStore.ts`'s persist config
— this is what makes `importBuild()` safe *inside* the iframe even though
it's technically the same origin: capture-mode boots simply never write to
the shared key, regardless of iframe isolation.

## Decision — anonymous-write security (2026-09-03, user-chosen; widened same
day for versioning)

Any visitor's view can trigger a write, with no owner check — that's the
whole point of "automatic on view." To bound the abuse surface: **write-once
per template version** (the backend only writes when the row's stored
`preview_template_version` is behind the version live today — never when it's
already current) plus a **server-side shape check** (decoded bytes must be a
valid PNG, exactly 1200×630, under the existing `MAX_PREVIEW_IMAGE_BYTES`
cap). This is the null-only "write-once" from the original decision,
generalized: a never-generated row (`preview_image_path IS NULL`) is just the
`preview_template_version IS NULL` case. Rejected: restricting the trigger to
owner-only views (closes the surface entirely but defeats the point — a
build only backfills when its owner happens to revisit it) and a per-build
rate limit on top (blunts rapid racing but doesn't close anything the shape
check doesn't already bound). Accepted residual risk: a determined attacker
running a modified client could still plant one wrong-but-correctly-shaped
image before the real one generates for that version, since there's no way to
verify image *content* server-side without the on-demand server-render
approach EMBED already rejected (unverified WASM-in-Deno) — and now that
regeneration is version-gated rather than one-shot-forever, that attacker
gets a fresh opportunity on every future template bump, not just once. Judged
acceptable: still cosmetic-only, still narrow, and an owner re-sharing via
EMBED3 is a separate, unrestricted write that always wins regardless of what
this path last wrote.

**Amended 2026-09-19, SECURITY_AUDIT.md F11 — one premise above was checked
against the corpus and does not hold there.** The decision itself stands and is
left as it was written; this is what was measured afterwards.

The residual priced above is a *race*: an attacker who happens upon a build
plants one image before the real capture lands. The per-build rate limit was
rejected on that reading, and on that reading correctly — it adds nothing to a
single build that the shape check does not already bound. What the reading
missed is that the **target set is enumerable**. `preview_template_version` sits
in the anon `GRANT SELECT` list on `shared_builds`, and it has to: it is the
fact a visitor's own client reads to decide whether to capture at all. So one
anonymous PostgREST query returns every writable row — measured 2026-09-19,
**2,469 of 2,668**, in a single request — and with no limit of any kind on the
function that is a scripted sweep, not a race. The image it plants is what the
build-og Worker serves as `og:image` to every crawler that unfurls that build's
link, and `previewCacheKey` keys on `preview_template_version`, the very column
this write moves, so the cache-busting built for legitimate regeneration
carries a planted image just as promptly.

Owner-only triggering stays rejected, and so does a per-build limit. What was
added is the control this section never weighed: a **per-IP rolling window**
(30 writes/hour, sharing `rate_limits` with `share-build` under its own
`action`), metered after the version gate so reloading a finished page costs
nothing, and spent before the write so failing is not a way around it. A
visitor still backfills every stale build they actually look at; a sweep of
2,469 needs 2,469 addresses. The accepted residual is unchanged in kind and now
bounded in quantity.

One correction while here, to the text and not the code: this section says the
shape check is "exactly 1200×630". The shipped check has always been against
`PREVIEW_CARD_WIDTH` and `PREVIEW_CARD_HEIGHT`, which became 1200×880 when the
card grew to seat the grid (see BUILD_PREVIEW_IMAGE_PLAN.md). The sentence was
wrong about the code, not the other way round.

## Active

- [x] **PREVBF1** — schema + version constant: add
      `preview_template_version INTEGER` to `shared_builds` (migration in
      [supabase/schema.sql](../supabase/schema.sql), existing rows land `NULL`
      — indistinguishable from "never generated" by design, since a `NULL`
      path already means that). Define
      `CURRENT_PREVIEW_TEMPLATE_VERSION = 1` as a literal constant, duplicated
      (frontend + every edge function that writes the column, same pattern as
      `MAX_PREVIEW_IMAGE_BYTES`) with a comment: bump this by hand, in every
      copy, whenever `BuildPreviewCard`'s visual template changes.
      verify: file:supabase/schema.sql, fn:preview_template_version
- [x] **PREVBF2** — stamp the version on real shares too: `share-build`'s
      `uploadPreviewImage` sets `preview_template_version =
      CURRENT_PREVIEW_TEMPLATE_VERSION` alongside `preview_image_path` on both
      the create and update branches, so an owner's normal re-share also
      keeps the version current — not just this doc's new capture path.
      Deploy via `supabase functions deploy share-build` (production push —
      confirm with the user first).
      needs: PREVBF1
      verify: file:supabase/functions/share-build/index.ts, fn:uploadPreviewImage
- [x] **PREVBF3** — `buildStore` capture-mode storage swap: detect the
      capture-mode URL param at store-creation time; when present, `storage:`
      resolves to an in-memory `Storage`-shaped stub instead of `localStorage`.
      verify: file:src/stores/buildStore.ts, fn:createJSONStorage
- [x] **PREVBF4** — `main.tsx` capture-mode boot: recognize
      `?previewCapture=<id>&serverId=<sid>`, fetch the target via
      `getSharedBuild(id)`, call `importBuild()` with its `build_json`, let
      the app render normally (no confirmation dialog, no navigation — this
      path never goes through `BuildDetailPage`'s `handleLoadBuild`).
      needs: PREVBF3
      verify: file:src/main.tsx, fn:importBuild
- [x] **PREVBF5** — capture + report, run from inside the capture-mode boot.
      **Revised from "poll for stabilization" to a precise signal**: found
      while building it that `useCharacterCalculation` already exposes
      exactly the right readiness flag —`useEngineStore`'s `loaded[serverId]`
      flips from the wasm engine's async load, and the totals memo produces
      "boot-time empty totals" before that (SPIKE5's own comment). Waiting on
      that flag directly is both simpler and more correct than polling
      rendered output for stability. `window.__previewCapture.ready`
      (set by PREVBF4) is checked first — a fetch failure reports 'failed'
      immediately rather than capturing the default empty build under the
      target's id. 15s hard timeout either way.
      needs: PREVBF4
      verify: file:src/components/export-image/SharePreviewCapture.tsx, fn:capturePreviewBase64
- [x] **PREVBF6** — new edge function
      `supabase/functions/backfill-preview/index.ts`: `{id, preview_image_base64}`,
      no auth. Validates the row exists, `visibility !== 'private'`, the
      row's stored `preview_template_version` is `NULL` or `<
      CURRENT_PREVIEW_TEMPLATE_VERSION` (version-gated write — never when
      already current), decoded PNG is exactly 1200×630 (read the IHDR chunk)
      and within `MAX_PREVIEW_IMAGE_BYTES`. Uploads to the existing
      `build-previews` bucket at the existing `previews/<id>.png` convention
      and sets both `preview_image_path` and `preview_template_version` —
      reusing EMBED3's `uploadPreviewImage` shape rather than a parallel
      implementation. Deploy via `supabase functions deploy backfill-preview`
      (production push — confirm with the user first).
      needs: PREVBF1
      verify: file:supabase/functions/backfill-preview/index.ts, fn:uploadPreviewImage
- [x] **PREVBF7** — wire the trigger into `BuildDetailPage`: on load, if
      `build.visibility !== 'private'` and (`preview_image_path` is null or
      `preview_template_version` is null or behind
      `CURRENT_PREVIEW_TEMPLATE_VERSION`), mount a hidden `<iframe>` at
      `/?previewCapture=<id>&serverId=<serverId>`; remove it on the
      completion `postMessage` or a hard timeout (~25s). Dedupe per pageview
      so re-renders don't spawn a second iframe. **Caught live**: the iframe
      was first sized 1×1 (it's invisible either way, via `position: fixed`
      off-screen + `opacity: 0`, same as `SharePreviewCapture`'s own trick) —
      that produced "Image rendering produced no output" every time, because
      the 1200×630 card has nowhere to lay out inside a 1×1 viewport. Fixed
      by sizing the iframe 1300×750 (still fully invisible); confirmed via a
      local capture run that the "no output" error was gone.
      needs: PREVBF5, PREVBF6
      verify: file:src/pages/BuildDetailPage.tsx, fn:previewCapture
- [x] **PREVBF8** — end-to-end verification against production, done on the
      user's own original build (`TIZbOh3wV5`, "Tisiphone WIP" — the build
      that started this doc, missing a preview since before EMBED shipped):
      migration applied live (`preview_template_version` present via
      `get-build`), both edge functions deployed
      (`backfill-preview`, `share-build`). Visited `/builds/TIZbOh3wV5`
      locally against the production backend — no console errors, hidden
      iframe appeared and self-removed within ~10s. Confirmed after: `get-build`
      shows `preview_image_path: previews/TIZbOh3wV5.png`,
      `preview_template_version: 1`; fetched the actual PNG — a correct,
      legible render of the real build (name, AT/sets, stats, taken/skipped
      icons). A second visit mounted no capture iframe at all (client-side
      gate). A direct POST replaying the same image against the now-current
      row returned `{"success":true,"skipped":true}` (server-side version
      gate). `coh-sidekick.com/builds/TIZbOh3wV5` now serves the correct
      `og:image` via the Worker. The visiting tab's own `localStorage` build
      was unaffected (small default-build size before and after, not the
      much larger captured build_json).
      needs: PREVBF7

**Correction, 2026-09-03 (found live, after this doc closed): the on-view
backfill never ran in production.** The site's CSP (built by `cspPlugin` in
[vite.config.ts](../vite.config.ts)) declared
`frame-src https://buymeacoffee.com https://www.buymeacoffee.com` with no
`'self'`, so the browser refused to frame `/?previewCapture=<id>` at all:

    Framing 'https://coh-sidekick.com/?previewCapture=...' violates the
    following Content Security Policy directive: "frame-src
    https://buymeacoffee.com https://www.buymeacoffee.com"

Every stored preview came from the quick-share path instead (`share-build`
captures in-page and needs no frame), which is why builds refreshed on "Copy
Short Link" and never on view.

PREVBF8's verification is what let it through, and the flaw is in its method,
not its diligence: it ran the frontend from the **dev server** against the
production backend. `cspPlugin` is `apply: 'build'`, so dev serves no CSP
whatsoever — the one policy that governs this mechanism was the one thing the
test could not see. Every observation in PREVBF8 is true and none of it was
evidence about production.

*A mechanism gated by a build-time-only policy has to be verified against a
built artifact.* Backend-is-production is not the same claim as
frontend-is-production, and this feature's failure lived entirely in the half
that wasn't. Re-verified against the deployed site after adding `'self'`:
capture iframe frames, `POST /functions/v1/backfill-preview` returns 200,
`preview_template_version` advances, and the stored PNG is the current
template.

## Out of scope

- Restricting the trigger to owner-only views — decided against in
  §Decision — anonymous-write security; defeats "automatic on view."
- A server-side rate limit per build id beyond version-gated write — decided
  against in the same section; the version gate plus shape check was judged
  sufficient.
- Retrying a failed backfill/refresh attempt on a schedule — no dedicated
  retry/backoff. A failed attempt just leaves the row behind, and the next
  organic view tries again for free.
- A migration script to bulk-stamp every pre-existing row's
  `preview_template_version` — unnecessary; `NULL` already means "generate
  regardless of version," which is the correct behavior for every row that
  predates this column.
