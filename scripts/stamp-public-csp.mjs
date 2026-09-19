/**
 * Stamp a Content-Security-Policy meta into every static page under public/ —
 * SECURITY_AUDIT.md F76.
 *
 * `cspPlugin` in vite.config.ts hashes index.html's inline scripts and injects a
 * policy, but it runs in `transformIndexHtml` and these five files never pass
 * through it: vite copies publicDir verbatim. So they ship to coh-sidekick.com
 * on the app's own origin with no policy at all. `rescue.html` is the one that
 * makes it matter — it reads the whole localStorage, `sb-<ref>-auth-token`
 * included, and offers it as a download (F23).
 *
 * A meta policy cannot carry `frame-ancestors`, `report-uri` or `sandbox`;
 * everything below is meta-legal. `style-src 'unsafe-inline'` is the same
 * concession vite.config.ts makes and for the same reason: system-atlas.html
 * uses a `style=""` attribute, which no hash can cover.
 *
 * Run `node scripts/stamp-public-csp.mjs` after editing an inline <script> in
 * any of these; `scripts/public-html-csp.test.ts` reds when a stamp is stale.
 */

import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PUBLIC_DIR = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'public');

/**
 * What each page is allowed, beyond `default-src 'none'` and the script hashes
 * derived from its own inline code. Written out per file rather than sniffed
 * from the markup so that adding a resource to a page is a decision someone
 * makes here, not a directive that widens itself.
 */
export const PUBLIC_PAGES = {
  '404.html': [],
  'og-image.html': ["style-src 'unsafe-inline'", "img-src 'self'"],
  'og-image_carbon.html': ["style-src 'unsafe-inline'", "img-src 'self'"],
  'rescue.html': ["style-src 'unsafe-inline'"],
  'system-atlas.html': ["style-src 'unsafe-inline'"],
};

const INLINE_SCRIPT = /<script(?![^>]*\bsrc=)[^>]*>([\s\S]*?)<\/script>/gi;
const EXISTING_META = /^[ \t]*<meta http-equiv="Content-Security-Policy"[^>]*>\r?\n/im;
const CHARSET = /^([ \t]*)<meta charset=[^>]*>\r?\n/im;

/** The policy `page` should carry, given its current contents. */
export function policyFor(file, html) {
  const extra = PUBLIC_PAGES[file];
  if (!extra) throw new Error(`public/${file} is not in PUBLIC_PAGES; add it with its directives`);

  const hashes = [];
  for (const match of html.matchAll(INLINE_SCRIPT)) {
    if (!match[1]) continue;
    hashes.push(`'sha256-${createHash('sha256').update(match[1], 'utf8').digest('base64')}'`);
  }

  return [
    "default-src 'none'",
    ...(hashes.length ? [`script-src ${hashes.join(' ')}`] : []),
    ...extra,
    "base-uri 'none'",
    "form-action 'none'",
  ].join('; ');
}

/** The file's contents with a current policy meta in place of any stale one. */
export function stamp(file, html) {
  const stripped = html.replace(EXISTING_META, '');
  const line = (indent) =>
    `${indent}<meta http-equiv="Content-Security-Policy" content="${policyFor(file, stripped)}">\n`;

  // After the charset so encoding detection still sees it in the first bytes,
  // and before anything the policy governs. The two files indent differently.
  const charset = CHARSET.exec(stripped);
  if (charset) {
    const at = charset.index + charset[0].length;
    return stripped.slice(0, at) + line(charset[1]) + stripped.slice(at);
  }
  const head = /^([ \t]*)<head[^>]*>\r?\n/im.exec(stripped);
  if (!head) throw new Error(`public/${file} has no <head> to stamp`);
  const at = head.index + head[0].length;
  return stripped.slice(0, at) + line(head[1] + '  ') + stripped.slice(at);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  for (const file of Object.keys(PUBLIC_PAGES)) {
    const path = join(PUBLIC_DIR, file);
    const html = readFileSync(path, 'utf8');
    const stamped = stamp(file, html);
    if (stamped !== html) {
      writeFileSync(path, stamped);
      console.log(`[public-csp] stamped ${file}`);
    } else {
      console.log(`[public-csp] ${file} already current`);
    }
  }
}
