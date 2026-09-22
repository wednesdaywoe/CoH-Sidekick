/**
 * SECURITY_AUDIT.md F12 — what it costs to be admitted to the mail relay.
 *
 * The refusal paths are graded against the real handler rather than by reading
 * the source, because none of them reaches Resend: a 403, a 429, a 503 and a
 * 413 all return before the outbound `fetch`. Only the success case needs a
 * stub, and what it is there to show is the ordering — that the limiter is
 * consulted before any mail is spent.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import worker, { isOriginAllowed, MAX_BODY_BYTES } from './index';

const GOOD = 'https://coh-sidekick.com';

/** A limiter that says yes, and remembers what it was keyed on. */
function limiter(success = true) {
  const keys: string[] = [];
  return {
    keys,
    binding: {
      limit: async ({ key }: { key: string }) => {
        keys.push(key);
        return { success };
      },
    },
  };
}

const envWith = (binding: unknown) => ({
  RESEND_API_KEY: 'test-key',
  FEEDBACK_EMAIL: 'test@example.com',
  FEEDBACK_RATE_LIMIT: binding,
}) as never;

const post = (init: RequestInit & { origin?: string | null; ip?: string } = {}) => {
  const headers = new Headers(init.headers);
  if (init.origin !== null) headers.set('Origin', init.origin ?? GOOD);
  headers.set('CF-Connecting-IP', init.ip ?? '203.0.113.9');
  return new Request('https://feedback.example/', {
    method: 'POST',
    headers,
    body: init.body ?? JSON.stringify({ type: 'bug', description: 'it broke', userAgent: 'x', timestamp: 'now' }),
  });
};

describe('isOriginAllowed', () => {
  it('admits the three origins that are ours', () => {
    expect(isOriginAllowed('https://coh-sidekick.com')).toBe(true);
    expect(isOriginAllowed('https://wednesdaywoe.github.io')).toBe(true);
    expect(isOriginAllowed('http://localhost:3000')).toBe(true);
  });

  it('refuses a host that merely starts with one of them', () => {
    // The bypass this replaced. An Origin has no path, so a prefix match is a
    // suffix wildcard on the host, and every one of these passed it.
    expect(isOriginAllowed('https://coh-sidekick.com.evil.test')).toBe(false);
    expect(isOriginAllowed('https://coh-sidekick.com.attacker.io')).toBe(false);
    expect(isOriginAllowed('http://localhost:3000.evil.test')).toBe(false);
  });

  it('refuses an absent Origin, which is what a native client sends', () => {
    expect(isOriginAllowed(null)).toBe(false);
    expect(isOriginAllowed('')).toBe(false);
  });
});

describe('the feedback worker refuses before it spends mail (F12)', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 200 })));
  });
  afterEach(() => vi.unstubAllGlobals());

  it('turns away a page on a host that only looks like ours', async () => {
    const res = await worker.fetch(post({ origin: 'https://coh-sidekick.com.evil.test' }), envWith(limiter().binding));
    expect(res.status).toBe(403);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('does not echo an origin it refused back as Allow-Origin', async () => {
    const res = await worker.fetch(post({ origin: 'https://coh-sidekick.com.evil.test' }), envWith(limiter().binding));
    expect(res.headers.get('Access-Control-Allow-Origin')).not.toContain('evil.test');
  });

  it('refuses rather than relays when the limiter did not deploy', async () => {
    // Fails closed, like the desktop arm: a missing binding must not read as
    // "no limit configured".
    const res = await worker.fetch(post(), envWith(undefined));
    expect(res.status).toBe(503);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('refuses over the limit, and says for how long', async () => {
    const res = await worker.fetch(post(), envWith(limiter(false).binding));
    expect(res.status).toBe(429);
    expect(res.headers.get('Retry-After')).toBe('60');
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('keys the limit on the edge address, not on the payload account id', async () => {
    // `userId` is the client's own unverified claim; keying on it would let
    // the sender choose their own bucket.
    const lim = limiter();
    await worker.fetch(
      post({ ip: '198.51.100.7', body: JSON.stringify({ type: 'bug', description: 'x', userId: 'pick-me', userAgent: 'x', timestamp: 'now' }) }),
      envWith(lim.binding),
    );
    expect(lim.keys).toEqual(['198.51.100.7']);
  });

  it('refuses an oversized body on its declared length, before reading it', async () => {
    const res = await worker.fetch(
      post({ headers: { 'Content-Length': String(MAX_BODY_BYTES + 1) } }),
      envWith(limiter().binding),
    );
    expect(res.status).toBe(413);
  });

  it('refuses an oversized body that declared no length at all', async () => {
    const res = await worker.fetch(
      post({ body: JSON.stringify({ type: 'bug', description: 'x'.repeat(MAX_BODY_BYTES + 10), userAgent: 'x', timestamp: 'now' }) }),
      envWith(limiter().binding),
    );
    expect(res.status).toBe(413);
  });

  it('consults the limiter before it sends anything', async () => {
    const lim = limiter();
    const res = await worker.fetch(post(), envWith(lim.binding));
    expect(res.status).toBe(200);
    expect(lim.keys).toHaveLength(1);
    expect(globalThis.fetch).toHaveBeenCalledOnce();
    expect(vi.mocked(globalThis.fetch).mock.calls[0][0]).toBe('https://api.resend.com/emails');
  });
});
