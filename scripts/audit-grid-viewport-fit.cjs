/**
 * Grid viewport fit — does the layout follow a REAL window, resized the way a user resizes it?
 *
 * The defect class this exists for is an input the layout stops receiving. The grid's width
 * reaches it through a ResizeObserver and two listeners, and a notification that is never
 * delivered is silent: the handler's only response to not being called is to keep the width it
 * has, which is indistinguishable from a window that never moved. The grid then draws against a
 * window that is no longer there — dead space when the border goes out, surfaces off the edge
 * when it comes in (VF5).
 *
 * Every check that existed when that shipped was blind to it, and the reason generalizes past
 * this repo. Playwright's `setViewportSize` goes through CDP `Emulation.setDeviceMetricsOverride`
 * and forces a relayout, so it always delivers both a resize event and an observer callback.
 * A user dragging a window border delivered NEITHER. Eight measurements across four viewports
 * came back green while the app in front of the reporter was frozen: the driver was supplying
 * the very thing whose absence was the bug.
 *
 * So this drives the OS window instead of the viewport — `viewport: null` on the context so no
 * emulation is applied at all, then CDP `Browser.setWindowBounds` in small steps, which is the
 * path a border drag takes. It is the only instrument here that can go red on a lost
 * notification.
 *
 * Usage:
 *   node scripts/audit-grid-viewport-fit.cjs [--gate] [--headed] [--verbose]
 *                                            [--profile release|debug] [--settle <ms>]
 *
 * Requires a built web bundle. Build either profile; the numbers are the same, measured:
 *   dx build --package app --platform web --release
 *   cd crates/app && dx build --platform web
 *
 * Runs in CI since 2026-09-27, in the `web` job, off that job's debug bundle. It is the first
 * browser gate this repository has ever run — see the step's own comment for why it was held out
 * until then, and `.github/workflows/playwright.yml` for the suite that never existed.
 */

const { chromium } = require('@playwright/test');
const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
// `target/dx/<bin>/<profile>/web/public`, and the bin is `Sidekick` since RB/RC1 named it
// (crates/app/Cargo.toml `[[bin]] name`) — dx keys this directory on the cargo TARGET, so it
// moved when the target did.
const bundleFor = (profile) => path.join(ROOT, `target/dx/Sidekick/${profile}/web/public`);

const args = process.argv.slice(2);
const GATE = args.includes('--gate');
const HEADED = args.includes('--headed');
const VERBOSE = args.includes('--verbose');

/** Which build to drive. `--profile <release|debug>` names one; with neither, release wins over
 *  debug where both exist, because release is what ships and what a hand run has just built.
 *
 *  Both are accepted because CI's `web` job builds debug — the geometry this measures is the
 *  same in either, and a second full wasm compile to get a release copy is minutes of runner
 *  time for a number that does not change. The profile actually used is PRINTED either way: a
 *  gate that silently drove a bundle from an hour ago would report the old layout as current,
 *  which is the trap this script's own header is about. */
const profileArg = args.indexOf('--profile');
const PROFILE = profileArg >= 0 ? args[profileArg + 1] : null;

/** How long each step waits before its first look, in ms. Overridable with `--settle <ms>` for
 *  one purpose: `--settle 0` probes before the app can possibly have answered, which is how the
 *  late-settle path in `measure` is proved to distinguish a slow layout from a stuck one rather
 *  than just forgiving both. A gate's own leniency needs a way to be tested too. */
const settleArg = args.indexOf('--settle');
const SETTLE_MS = settleArg >= 0 ? Number(args[settleArg + 1]) : 700;

/** Slack allowed between the surfaces' extent and the room they were given, in px.
 *  One row's margin: below this a difference is rounding, above it is a layout that stopped
 *  answering the window. */
const SLACK_PX = 10;

/** Room allowed below the lowest surface, in px. A column is a whole number of rows, so the
 *  layout can never end closer to the bottom than one row's pitch (row height + margin = 36);
 *  anything past that is a layout that stopped answering the window's height (VF3). */
const MAX_DEAD_PX = 40;

/** Extra room given to a step that failed its first look, in ms. Five of the app's own 250ms
 *  resize polls: a layout that has not answered the window after five chances to has not lost a
 *  notification, it has no working path to the window at all. See `measure`. */
const LATE_MS = 1250;

/** Widths the window is stepped through. Each step is small enough to look like a drag frame
 *  rather than a jump, and the range crosses the 12-to-8 column boundary in both directions. */
const WIDTH_STEPS = [2400, 2200, 2000, 1800, 1600, 1400, 1200, 1100, 1000, 1100, 1400, 1800, 2200, 2400];

/** Heights the window is stepped through, at a fixed width. The tall end is where a layout
 *  with no height input strands the most room. */
const HEIGHT_STEPS = [1400, 1200, 1000, 900, 1000, 1200, 1400];

const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm',
  '.css': 'text/css', '.json': 'application/json', '.png': 'image/png',
  '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.ico': 'image/x-icon',
};

function serve(root) {
  const server = http.createServer((req, res) => {
    const rel = decodeURIComponent(req.url.split('?')[0]);
    let file = path.join(root, rel === '/' ? 'index.html' : rel);
    if (!file.startsWith(root)) return res.writeHead(403).end();
    if (!fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      file = path.join(root, 'index.html');
      if (!fs.existsSync(file)) return res.writeHead(404).end();
    }
    res.writeHead(200, { 'content-type': MIME[path.extname(file)] || 'application/octet-stream' });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok(server)));
}

/** One reading of the grid, taken from the page. `surfaceRight`/`surfaceBottom` are the extent
 *  the app actually placed things at, which is the number that goes stale; the element's own
 *  rect is what the browser gave it, which never does. The gap between the two IS the bug. */
const PROBE = () => {
  const el = document.getElementById('desktop-grid');
  const surfaces = [...document.querySelectorAll('#desktop-grid .surface')];
  if (!el || !surfaces.length) return null;
  const r = el.getBoundingClientRect();
  const rects = surfaces.map((e) => e.getBoundingClientRect());
  return {
    vw: window.innerWidth,
    vh: window.innerHeight,
    gridLeft: Math.round(r.left),
    gridWidth: Math.round(r.width),
    placedWidth: Math.round(Math.max(...rects.map((x) => x.right)) - r.left),
    placedBottom: Math.round(Math.max(...rects.map((x) => x.bottom))),
    overflowX: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    surfaces: surfaces.length,
    poll: typeof window.__skGridPoll,
  };
};

async function settle(page, ms = SETTLE_MS) {
  await page.waitForTimeout(ms);
}

async function main() {
  const candidates = PROFILE ? [PROFILE] : ['release', 'debug'];
  const profile = candidates.find((p) => fs.existsSync(path.join(bundleFor(p), 'index.html')));
  if (!profile) {
    for (const p of candidates) console.error(`No built bundle at ${bundleFor(p)}`);
    console.error('Build first: dx build --package app --platform web --release');
    process.exit(2);
  }
  const BUNDLE = bundleFor(profile);
  const built = fs.statSync(path.join(BUNDLE, 'index.html')).mtime.toISOString();
  console.log(`grid-viewport-fit: driving the ${profile} bundle, built ${built}`);

  const server = await serve(BUNDLE);
  const url = `http://127.0.0.1:${server.address().port}/`;
  const browser = await chromium.launch({ headless: !HEADED });
  // `viewport: null` is the whole point: the page's size comes from the real window, so no
  // device-metrics override is in play and a window resize has to reach the app the same way a
  // user's does.
  const context = await browser.newContext({ viewport: null });
  const page = await context.newPage();

  const failures = [];
  /** Steps that were wrong at the first look and right at the second. Not failures — see
   *  `measure` — but never silent either: a gate whose subject is a late notification has to
   *  say when it saw one. */
  const late = [];
  const rows = [];

  try {
    const cdp = await context.newCDPSession(page);
    const { windowId } = await cdp.send('Browser.getWindowForTarget');
    const setBounds = async (width, height) => {
      await cdp.send('Browser.setWindowBounds', { windowId, bounds: { width, height } });
    };

    // Sized before the first paint, because the desktop grid does not exist below 900px — the
    // mobile stack owns that range and `#desktop-grid` is `display: none`. A default headless
    // window is well under it, so waiting for a surface first would wait forever.
    await setBounds(1800, 1000);
    await page.goto(url);
    await page.waitForFunction(() => document.querySelectorAll('#desktop-grid .surface').length > 0, { timeout: 30_000 });
    await settle(page, 1500);

    // A fresh profile opens the first-run welcome, whose backdrop swallows every click — so
    // phase 3, the only phase that clicks anything, timed out against it rather than failing.
    // Every run here is a fresh profile, so this is not an occasional state: it is the state.
    // Dismissed rather than suppressed, because the dismissal is what a first user does too.
    const welcome = page.locator('.modal-backdrop button:has-text("Got it")');
    if (await welcome.count()) {
      await welcome.first().click();
      await settle(page, 400);
    }

    /** Everything wrong with one reading, as a list of sentences. Pure, and that is what lets
     *  the same rules grade a first look and a second one. `room` turns on the height leg. */
    const faults = (label, p, room) => {
      if (!p) return [`${label}: grid not present`];
      const out = [];
      // Width: the surfaces span the element they are placed in. On the authored default every
      // column is occupied, so a short extent means the placement used a stale container width.
      const widthGap = p.gridWidth - p.placedWidth;
      if (Math.abs(widthGap) > SLACK_PX) {
        out.push(`${label}: placed against ${p.placedWidth}px inside a ${p.gridWidth}px grid (off by ${widthGap})`);
      }
      if (p.overflowX > 0) {
        out.push(`${label}: page overflows horizontally by ${p.overflowX}px`);
      }
      // Height: the room below the lowest surface. A layout with no height input strands all of
      // it, which is VF3 — reported separately so a width regression and a height gap can never
      // be mistaken for each other.
      if (room && p.vh - p.placedBottom > MAX_DEAD_PX) {
        out.push(`${label}: ${p.vh - p.placedBottom}px of window left unused below the layout`);
      }
      return out;
    };

    /** Probe one step, grade it, and on a fault look ONCE more after [`LATE_MS`].
     *
     *  **The second look is not a retry that buries a red.** The defect this script exists for is
     *  a notification that never arrives, and the app re-reads the window every 250ms on its own
     *  poll — so a layout still wrong after `LATE_MS` is wrong for good, and one that has come
     *  right answered the window late rather than not at all. The two deserve different verdicts
     *  and used to get the same one.
     *
     *  Added 2026-09-27 against a measurement, not a hunch: one run in twelve reported
     *  `w=1400` placed 167px wide of a 1374px grid with the page overflowing by 149px, on a busy
     *  machine, and eleven runs of the identical steps were clean. A gate that reds one push in
     *  twelve teaches people to re-run it, which is how the five red jobs this repository deleted
     *  earned their reputation. A late settle is now PRINTED and counted rather than either failed
     *  or hidden. */
    const measure = async (label, room) => {
      let p = await page.evaluate(PROBE);
      const first = faults(label, p, room);
      if (first.length) {
        await settle(page, LATE_MS);
        p = await page.evaluate(PROBE);
        const second = faults(label, p, room);
        if (second.length) {
          failures.push(...second);
        } else {
          late.push(`${label}: right only after a further ${LATE_MS}ms — first look: ${first.join('; ')}`);
        }
      }
      rows.push({ label, ...(p || {}), ...(room && p ? { dead: p.vh - p.placedBottom } : {}) });
    };

    // Labelled with the step's ordinal, not just its size. Both step lists visit the same size
    // twice on purpose — that round trip IS the test — so a bare `w=1400` names two different
    // moments and a failure report could not say which one went wrong. Found the hard way: an
    // intermittent width failure at `w=1400` was unattributable to either visit.
    for (const [i, w] of WIDTH_STEPS.entries()) {
      await setBounds(w, 1000);
      await settle(page);
      await measure(`w=${w} #${i + 1}`, false);
    }

    for (const [i, h] of HEIGHT_STEPS.entries()) {
      await setBounds(1800, h);
      await settle(page);
      await measure(`h=${h} #${i + 1}`, true);
    }

    // Phase 3 — an arranged layout is the user's and the re-fit must not re-author it. The
    // height fit re-authors the WHOLE layout, so a guard that reads the wrong way silently
    // discards every hide and every drag. Exercised with a real gesture rather than a flag.
    await setBounds(1800, 900);
    await settle(page);
    const hideBtn = page.locator('button.panel-hide').first();
    const hidLabel = await hideBtn.getAttribute('aria-label');
    const before = await page.evaluate(() => document.querySelectorAll('#desktop-grid .surface').length);
    await hideBtn.click();
    await settle(page);
    const afterHide = await page.evaluate(() => document.querySelectorAll('#desktop-grid .surface').length);
    if (afterHide !== before - 1) {
      failures.push(`arranged: hiding "${hidLabel}" left ${afterHide} surfaces, expected ${before - 1}`);
    }
    await setBounds(1800, 1400);
    await settle(page, 1200);
    const afterGrow = await page.evaluate(() => document.querySelectorAll('#desktop-grid .surface').length);
    if (afterGrow !== afterHide) {
      failures.push(`arranged: growing the window re-authored over the user's layout (${afterHide} -> ${afterGrow} surfaces)`);
    }
    rows.push({ label: 'arranged', vw: 1800, vh: 1400, gridWidth: 0, placedWidth: 0, overflowX: 0, surfaces: afterGrow, poll: 'number' });

    const first = rows.find((r) => r.poll);
    if (!first || first.poll !== 'number') {
      failures.push('resize poll is not bound — the served bundle predates VF5');
    }
  } finally {
    await browser.close();
    server.close();
  }

  if (VERBOSE) {
    for (const r of rows) {
      const d = r.dead === undefined ? '' : `  dead=${r.dead}`;
      console.log(`  ${r.label.padEnd(12)} vw=${r.vw} vh=${r.vh}  grid=${r.gridWidth}  placed=${r.placedWidth}  overflowX=${r.overflowX}${d}`);
    }
  }

  const heightRows = rows.filter((r) => r.dead !== undefined);
  const worstDead = heightRows.length ? Math.max(...heightRows.map((r) => r.dead)) : 0;

  for (const l of late) console.warn(`  ! ${l}`);

  if (failures.length) {
    console.error(`FAIL grid-viewport-fit: ${failures.length} of ${rows.length} steps`);
    for (const f of failures) console.error(`  - ${f}`);
    if (GATE) process.exit(1);
    return;
  }

  console.log(`OK grid-viewport-fit: ${rows.length} real-window steps, width + height + arranged`);
  console.log(`   worst unused height below the layout: ${worstDead}px (limit ${MAX_DEAD_PX})`);
  if (late.length) {
    console.log(`   ${late.length} step(s) answered the window only on the second look, ${LATE_MS}ms later`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
