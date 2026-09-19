/**
 * Every static page under public/ carries a current CSP — SECURITY_AUDIT.md F76.
 *
 * These files bypass `cspPlugin` by construction (vite copies publicDir
 * verbatim), so nothing but this gate stands between an edit and a page that
 * ships to the app's own origin with no policy. Two ways that happens, and both
 * are covered below: a new page nobody stamped, and an edited inline <script>
 * whose hash no longer matches the meta above it.
 */

import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { PUBLIC_PAGES, stamp } from './stamp-public-csp.mjs';

const PUBLIC_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'public');
const pages = readdirSync(PUBLIC_DIR).filter(f => f.endsWith('.html')).sort();

describe('public/*.html content security policy', () => {
  it('knows about every page on disk', () => {
    // A page added to public/ without a line in PUBLIC_PAGES would otherwise
    // ship with no policy and nothing would say so.
    expect(pages).toEqual(Object.keys(PUBLIC_PAGES).sort());
  });

  it.each(pages)('%s carries the policy its own contents require', page => {
    const html = readFileSync(join(PUBLIC_DIR, page), 'utf8');
    // stamp() is idempotent, so a file that already holds a current meta is its
    // own fixed point. Anything else means the hash drifted from the script.
    expect(html, `run 'node scripts/stamp-public-csp.mjs'`).toBe(stamp(page, html));
  });

  it.each(pages)('%s allows no inline script but its own', page => {
    const html = readFileSync(join(PUBLIC_DIR, page), 'utf8');
    const meta = /<meta http-equiv="Content-Security-Policy" content="([^"]+)">/i.exec(html);
    expect(meta, `${page} has no policy meta`).not.toBeNull();

    const policy = meta![1];
    expect(policy).toContain(`default-src 'none'`);
    // The concession the app's own CSP makes for style attributes must not
    // leak into script-src, which is the directive that matters here.
    const scriptSrc = /script-src ([^;]*)/.exec(policy)?.[1] ?? '';
    expect(scriptSrc).not.toContain(`'unsafe-inline'`);
    expect(scriptSrc).not.toContain(`'unsafe-eval'`);
  });
});
