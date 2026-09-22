/**
 * Grades the rolling window and `backfill-preview`'s use of it — SECURITY_AUDIT.md F11.
 *
 * F11's fix is a limit, and a limit has two ways to be wrong. It can fail to
 * bound the thing it exists to bound, which is the finding. Or it can bound a
 * legitimate visitor, which breaks "automatic on view" and is the reason the
 * 2026-09-03 decision rejected the controls it rejected. Both directions are
 * below, and so is the ordering the limit depends on: metered after the version
 * gate, spent before the write.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { callerIp, gradeWindow, windowStart } from './rate-window';

const HOUR = 60 * 60 * 1000;
const NOW = Date.parse('2026-09-19T12:00:00.000Z');

const grade = (over: Partial<Parameters<typeof gradeWindow>[0]> = {}) =>
  gradeWindow({ used: 0, limit: 30, oldest: null, now: NOW, windowHours: 1, ...over });

describe('windowStart', () => {
  it('is exactly one window behind now', () => {
    expect(windowStart(NOW, 1)).toBe('2026-09-19T11:00:00.000Z');
    expect(windowStart(NOW, 2)).toBe('2026-09-19T10:00:00.000Z');
  });
});

describe('gradeWindow', () => {
  it('lets a caller through right up to the limit, and refuses the one after', () => {
    // The off-by-one that decides whether the limit is 30 or 31.
    expect(grade({ used: 29 }).exceeded).toBe(false);
    expect(grade({ used: 30 }).exceeded).toBe(true);
    expect(grade({ used: 31 }).exceeded).toBe(true);
  });

  it('counts the request being graded when it reports what is left', () => {
    expect(grade({ used: 0 }).remaining).toBe(29);
    expect(grade({ used: 29 }).remaining).toBe(0);
  });

  it('never reports a negative remaining', () => {
    expect(grade({ used: 99 }).remaining).toBe(0);
  });

  it('frees a slot when the oldest in-window request ages out, not on the hour', () => {
    // The whole point of a rolling window: a caller who spread their requests
    // out waits seconds, not a full hour.
    const oldest = new Date(NOW - 59 * 60 * 1000).toISOString();
    const verdict = grade({ used: 30, oldest });
    expect(verdict.retryAfterSeconds).toBe(60);
    expect(verdict.resetAt).toBe(new Date(NOW + 60 * 1000).toISOString());
  });

  it('makes a caller who burst their whole allowance wait the full window', () => {
    const oldest = new Date(NOW).toISOString();
    expect(grade({ used: 30, oldest }).retryAfterSeconds).toBe(3600);
  });

  it('falls back to a full window when the oldest timestamp is missing or unreadable', () => {
    // The pessimistic end on purpose: retrying immediately on the strength of a
    // timestamp we could not read turns a failed query into a free request.
    for (const oldest of [null, undefined, '', 'not a date']) {
      expect(grade({ used: 30, oldest }).retryAfterSeconds).toBe(3600);
    }
  });

  it('never hands back a Retry-After in the past', () => {
    // A row whose created_at has already aged out of the window — a clock that
    // stepped forward, or a sweep that raced the count.
    const stale = new Date(NOW - 3 * HOUR).toISOString();
    expect(grade({ used: 30, oldest: stale }).retryAfterSeconds).toBe(0);
  });

  it('rounds a part-second up, so Retry-After is never short', () => {
    const oldest = new Date(NOW - HOUR + 1500).toISOString();
    expect(grade({ used: 30, oldest }).retryAfterSeconds).toBe(2);
  });
});

describe('callerIp', () => {
  const headers = (init: Record<string, string>) => new Headers(init);

  it('takes the client entry from a forwarded-for list', () => {
    expect(callerIp(headers({ 'x-forwarded-for': '203.0.113.7, 10.0.0.1, 10.0.0.2' })))
      .toBe('203.0.113.7');
  });

  it('falls back to the Cloudflare header, then to one shared bucket', () => {
    expect(callerIp(headers({ 'cf-connecting-ip': '198.51.100.4' }))).toBe('198.51.100.4');
    expect(callerIp(headers({}))).toBe('unknown');
  });

  it('does not let an empty forwarded-for become its own allowance', () => {
    // `??` would return '' here, and every caller sending an empty header would
    // get a private bucket keyed on the empty string.
    expect(callerIp(headers({ 'x-forwarded-for': '' }))).toBe('unknown');
    expect(callerIp(headers({ 'x-forwarded-for': '   ' }))).toBe('unknown');
    expect(callerIp(headers({ 'x-forwarded-for': ' , 10.0.0.1' }))).toBe('unknown');
  });
});

/**
 * As with F09's wiring assertions: the handler cannot be imported, so its source
 * is read. The needles live in this file and the haystack is another one, which
 * is what keeps them from matching themselves.
 */
describe('backfill-preview meters the write', () => {
  const source = readFileSync(
    new URL('../backfill-preview/index.ts', import.meta.url),
    'utf8',
  );

  it('has a per-IP window at all', () => {
    expect(source).toContain("import { callerIp, gradeWindow, windowStart }");
    expect(source).toContain('status: 429');
    expect(source).toMatch(/const BACKFILL_RATE_LIMIT = \d+;/);
    // Added 2026-09-22 with F38's twin, which is where the hole was found:
    // without this line the block passes with the comparison mutated away,
    // because the 429 it guards is still in the file.
    expect(source).toContain('if (used >= BACKFILL_RATE_LIMIT) {');
  });

  it('meters after the version gate, so a finished page costs nothing to reload', () => {
    const gate = source.indexOf('>= CURRENT_PREVIEW_TEMPLATE_VERSION');
    const metered = source.indexOf("from('rate_limits')");
    expect(gate).toBeGreaterThan(-1);
    expect(metered).toBeGreaterThan(gate);
  });

  it('spends the slot before the write, not after it', () => {
    // A slot recorded only on success is not a limit: the way to exceed it
    // would be to fail.
    const spent = source.indexOf("from('rate_limits').insert(");
    const uploaded = source.indexOf(".from('build-previews')");
    expect(spent).toBeGreaterThan(-1);
    expect(uploaded).toBeGreaterThan(spent);
  });

  it('still refuses a private build and a non-PNG before any of that', () => {
    // The two checks the 2026-09-03 decision actually bought. A rate limit that
    // arrived by loosening them would be a worse trade than the finding.
    // The shape check moved to `_shared/preview-image.ts` for F85 - same
    // check, now the one both writers make.
    const shape = source.indexOf('gradePreviewImage(bytes)');
    // The visibility gate moved to `_shared/preview-visibility.ts` for F08 —
    // same gate, one copy. What this case is about is unchanged: it runs
    // before the meter.
    const priv = source.indexOf('previewMayExist(row.visibility)');
    const metered = source.indexOf("from('rate_limits')");
    expect(shape).toBeGreaterThan(-1);
    expect(priv).toBeGreaterThan(-1);
    expect(shape).toBeLessThan(metered);
    expect(priv).toBeLessThan(metered);
  });
});

describe('share-build keys its window through the same helper (F10)', () => {
  const source = readFileSync(
    new URL('../share-build/index.ts', import.meta.url),
    'utf8',
  );

  // F10 was filed as "rate limit keyed on client-supplied first X-Forwarded-For
  // entry". Measured against production 2026-09-22 and the premise is false:
  // a forged `X-Forwarded-For`, repeated or not, and a forged `X-Real-IP` are
  // all discarded before the function sees them, and forging `CF-Connecting-IP`
  // is refused by Cloudflare itself with a 403. The first entry is the edge's
  // own value. What the measurement DID find is below: share-build is the
  // function `rate-window.ts` was extracted from, and the copy left behind had
  // drifted from it.
  it('reads no forwarding header of its own', () => {
    expect(source).toContain("import { callerIp, gradeWindow, windowStart }");
    expect(source).not.toMatch(/headers\.get\(\s*'x-forwarded-for'/);
    expect(source).not.toMatch(/headers\.get\(\s*'cf-connecting-ip'/);
  });

  it('keeps no second copy of the window arithmetic', () => {
    // The `??` this replaced handed back `''` for an empty forwarded-for,
    // which is not nullish — so every such caller got a private allowance.
    // One copy of the rule is the guard; `callerIp`'s own empty-header test
    // above is what that copy is now held to.
    expect(source).toContain('const clientIp = callerIp(req.headers);');
    expect(source).toContain('windowStart(Date.now(), RATE_WINDOW_HOURS)');
    expect(source).toContain('gradeWindow({');
    expect(source).not.toMatch(/RATE_WINDOW_HOURS \* 60 \* 60 \* 1000/);
  });

  it('asks for the oldest row in a way that tolerates there being none', () => {
    // `.single()` errors on zero rows where `.maybeSingle()` does not; the
    // error was swallowed, so this was latent rather than live. Scoped to the
    // window block on purpose — the lookup at the update path has the same
    // shape and is not this finding's to change.
    const block = source.slice(
      source.indexOf('---- Rate limiting ----'),
      source.indexOf("from('rate_limits').insert("),
    );
    expect(block).toContain('.maybeSingle();');
    expect(block).not.toContain('.single();');
  });
});

describe('auction-prices meters the upstream spend (F38)', () => {
  const source = readFileSync(
    new URL('../auction-prices/index.ts', import.meta.url),
    'utf8',
  );

  // F38: `verify_jwt = false` and a server-held third-party key, so without a
  // limit this is an open proxy onto somebody else's API quota.
  it('has a per-IP window at all, through the shared helper', () => {
    expect(source).toContain("import { callerIp, gradeWindow, windowStart }");
    expect(source).toContain('status: 429');
    expect(source).toMatch(/const AUCTION_RATE_LIMIT = \d+;/);
    // The comparison itself, not just the refusal it guards. A first version
    // of this block asserted the constant and the 429 and passed happily with
    // the check mutated to `if (false)` - the refusal is still IN the source,
    // it just never runs. Found by mutating, not by reading.
    expect(source).toContain('if (used >= AUCTION_RATE_LIMIT) {');
  });

  it('takes its own action, so it cannot eat another function\'s allowance', () => {
    expect(source).toContain("const RATE_LIMIT_ACTION = 'auction';");
  });

  it('meters after the cache read, so a fully-cached page costs nothing', () => {
    const cached = source.indexOf("from('auction_prices')");
    const metered = source.indexOf("from('rate_limits')");
    expect(cached).toBeGreaterThan(-1);
    expect(metered).toBeGreaterThan(cached);
    expect(source).toContain('if (stale.length > 0) {');
  });

  it('spends the slot before the upstream fetch, not after', () => {
    const spent = source.indexOf("from('rate_limits').insert(");
    const fetched = source.indexOf('fetchPrice(id, apiKey)');
    expect(spent).toBeGreaterThan(-1);
    expect(fetched).toBeGreaterThan(spent);
  });

  it('does not hand the caller the exception text', () => {
    // A function whose job is holding a key the caller must not see should not
    // return whatever an unexpected throw was carrying.
    expect(source).not.toContain('error: String(err)');
    expect(source).toContain("error: 'Price lookup failed'");
  });
});
