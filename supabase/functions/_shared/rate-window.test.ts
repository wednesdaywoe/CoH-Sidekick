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
    const shape = source.indexOf('readPngDimensions(bytes)');
    const priv = source.indexOf("row.visibility === 'private'");
    const metered = source.indexOf("from('rate_limits')");
    expect(shape).toBeGreaterThan(-1);
    expect(priv).toBeGreaterThan(-1);
    expect(shape).toBeLessThan(metered);
    expect(priv).toBeLessThan(metered);
  });
});
