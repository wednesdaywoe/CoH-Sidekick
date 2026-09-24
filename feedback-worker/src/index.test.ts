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
import { decide, dayOf, untilReset, capsFrom, DEFAULT_CAPS, type Ledger } from './budget';

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

/**
 * A stand-in for the Durable Object namespace that runs the REAL `decide` over
 * an in-memory ledger. It grades the wiring and the rules together; `decide` is
 * also graded on its own below. What it deliberately does NOT model is the one
 * property the object exists for — that two callers cannot interleave — because
 * nothing in a single-threaded test can, and claiming otherwise would be the
 * kind of green that means nothing.
 */
function budget(now = () => Date.now()) {
  // Keyed by object id rather than a single shared ledger, and that detail is
  // load-bearing: `spend` calls `idFromName('budget')`, a CONSTANT, which is
  // the whole reason the global counter is global. A fake that ignored the id
  // would keep one ledger no matter what the worker asked for, so keying the
  // object on the caller's address — which turns the budget into a second
  // per-address cap and removes the bound entirely — would still have gone
  // green. It does not now.
  const ledgers = new Map<string, Ledger | undefined>();
  const calls: string[] = [];
  const namespace = {
    idFromName: (name: string) => name,
    get: (id: string) => ({
      fetch: async (url: string) => {
        const params = new URL(url).searchParams;
        const ip = params.get('ip') ?? 'unknown';
        calls.push(ip);
        // The caps are read off the REQUEST, exactly as the real object does.
        // A fake that used its own copy would pass a worker that stopped
        // sending them -- and the worker sends them because a running Durable
        // Object keeps the env it was built with, which is a production
        // measurement rather than a theory.
        const caps = { global: Number(params.get('global')), perIp: Number(params.get('perIp')) };
        const result = decide(ledgers.get(id), ip, now(), caps);
        ledgers.set(id, result.ledger);
        return new Response(JSON.stringify(result.verdict));
      },
    }),
  };
  return { calls, namespace, ids: () => [...ledgers.keys()], ledgerNow: () => ledgers.get('budget') };
}

/**
 * `null` means "this binding did not deploy", NOT `undefined` — because an
 * explicit `undefined` argument takes the default parameter, so the one test
 * that needs the budget absent was silently getting a working one and passing
 * for the wrong reason. It was caught by the test going red on the first run;
 * a default that is a no-op rather than a live object would have gone green.
 */
const envWith = (
  binding: unknown,
  budgetNamespace: unknown = budget().namespace,
  caps = DEFAULT_CAPS,
) => ({
  RESEND_API_KEY: 'test-key',
  FEEDBACK_EMAIL: 'test@example.com',
  FEEDBACK_RATE_LIMIT: binding,
  FEEDBACK_BUDGET: budgetNamespace ?? undefined,
  FEEDBACK_DAILY_BUDGET: String(caps.global),
  FEEDBACK_DAILY_PER_IP: String(caps.perIp),
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

/**
 * R78, which F12's 2026-09-22 pass closed by reading `buildEmailHtml` and
 * seeing `escapeHtml` on the string fields. Re-measured 2026-09-23 by driving
 * the real handler and reading what reached Resend: the three build-context
 * numbers went out raw, which is the exact field class R78 was filed on. The
 * hole was in the signature, not in the table — `escapeHtml(str: string)` is
 * not something you call on a field typed `number`, so nobody did. Grading the
 * rendered HTML rather than the source is the point of these three.
 */
describe('nothing the sender chose reaches the email unescaped (R78)', () => {
  /** Drives the handler with a hostile payload and hands back the email HTML. */
  async function render(buildContext: unknown): Promise<string> {
    let html = '';
    vi.stubGlobal('fetch', vi.fn(async (_url: unknown, init: RequestInit) => {
      html = JSON.parse(String(init.body)).html;
      return new Response('{}', { status: 200 });
    }));
    const res = await worker.fetch(
      post({ body: JSON.stringify({ type: 'bug', description: 'it broke', userAgent: 'x', timestamp: 'now', buildContext }) }),
      envWith(limiter().binding),
    );
    expect(res.status).toBe(200);
    return html;
  }

  const ctx = (over: Record<string, unknown>) => ({
    archetype: 'Blaster', primary: 'Fire', secondary: 'Devices',
    pools: [], epicPool: null, level: 50, powerCount: 24, slotCount: 67, ...over,
  });

  it('escapes the build-context numbers, which are strings if the sender says so', async () => {
    const html = await render(ctx({
      level: '<img src=x onerror=alert(1)>',
      powerCount: '<script>PWN</script>',
      slotCount: '"><b>SLOT</b>',
    }));
    expect(html).not.toContain('<img src=x');
    expect(html).not.toContain('<script>PWN');
    expect(html).not.toContain('<b>SLOT</b>');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
  });

  it('escapes a pool name, and survives `pools` not being an array', async () => {
    expect(await render(ctx({ pools: ['<u>POOL</u>'] }))).toContain('&lt;u&gt;POOL&lt;/u&gt;');
    // `.length` on a string is a number and `.map` is not a function: this threw
    // before, taking out a request the limiter had already been charged for.
    expect(await render(ctx({ pools: 'Flight' }))).toContain('None');
  });

  it('renders rather than throws when a field the interface calls a string is null', async () => {
    const html = await render(ctx({ archetype: null, epicPool: undefined }));
    expect(html).toContain('Build Context');
  });
});


/**
 * SECURITY_AUDIT.md F12 — the daily budget, which is the clause the rate-limit
 * binding could not close.
 *
 * The row's own words for why: "A configured limit is not a measurement of a
 * limit." A `[[ratelimits]]` binding admitted 19 of 30 against a configured 3,
 * because Cloudflare's counters are per-location and approximate. Everything
 * below grades a number that is exact by construction, and the reason it can be
 * exact is the one thing these tests cannot show — a single Durable Object
 * instance serialising every caller. What they CAN show is that the rules are
 * right and that nothing reaches Resend without being charged.
 */
describe('the daily budget is a bound and not a target (F12)', () => {
  const day = (d: string) => Date.parse(`${d}T12:00:00.000Z`);

  it('rolls the counters on the UTC day and not before', () => {
    expect(dayOf(Date.parse('2026-09-23T23:59:59.999Z'))).toBe('2026-09-23');
    expect(dayOf(Date.parse('2026-09-24T00:00:00.000Z'))).toBe('2026-09-24');
  });

  it('reports the seconds to the next reset, for Retry-After', () => {
    expect(untilReset(Date.parse('2026-09-23T23:59:00.000Z'))).toBe(60);
    expect(untilReset(Date.parse('2026-09-23T00:00:00.000Z'))).toBe(86_400);
  });

  it('refuses the caller at their own cap without touching the global count', () => {
    // The per-address cap is what stops one caller emptying the shared budget,
    // so a refusal from it must not spend the thing it is protecting.
    const caps = { global: 100, perIp: 2 };
    let ledger: Ledger | undefined;
    for (let i = 0; i < 3; i += 1) ledger = decide(ledger, '198.51.100.7', day('2026-09-23'), caps).ledger;
    expect(ledger!.perIp['198.51.100.7']).toBe(2);
    expect(ledger!.global).toBe(2);
  });

  it('refuses everybody once the day is spent, whoever is asking', () => {
    const caps = { global: 2, perIp: 10 };
    let ledger: Ledger | undefined;
    ledger = decide(ledger, 'a', day('2026-09-23'), caps).ledger;
    ledger = decide(ledger, 'b', day('2026-09-23'), caps).ledger;
    const third = decide(ledger, 'c', day('2026-09-23'), caps);
    expect(third.verdict).toMatchObject({ ok: false, refused: 'global' });
    // And a refused submission is not charged: 'c' has no key, because only an
    // admitted one writes one. That is what bounds the map's growth.
    expect(third.ledger.global).toBe(2);
    expect(third.ledger.perIp).not.toHaveProperty('c');
  });

  it('starts the count again on the next UTC day', () => {
    const caps = { global: 1, perIp: 1 };
    const spent = decide(undefined, 'a', day('2026-09-23'), caps).ledger;
    expect(decide(spent, 'a', day('2026-09-23'), caps).verdict.ok).toBe(false);
    const tomorrow = decide(spent, 'a', day('2026-09-24'), caps);
    expect(tomorrow.verdict.ok).toBe(true);
    expect(tomorrow.ledger).toMatchObject({ day: '2026-09-24', global: 1 });
  });

  it('does not carry yesterday`s per-address counts into today', () => {
    // A stale ledger is a DIFFERENT ledger, not a smaller one. Reading
    // yesterday's map today would refuse a caller for something they did
    // before the reset they were told to wait for.
    const caps = { global: 10, perIp: 1 };
    const spent = decide(undefined, 'a', day('2026-09-23'), caps).ledger;
    expect(decide(spent, 'a', day('2026-09-24'), caps).ledger.perIp).toEqual({ a: 1 });
  });
});

describe('nothing reaches Resend without being charged (F12)', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 200 })));
  });
  afterEach(() => vi.unstubAllGlobals());

  it('refuses rather than relays when the budget did not deploy', async () => {
    const res = await worker.fetch(post(), envWith(limiter().binding, null));
    expect(res.status).toBe(503);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });

  it('asks one object for everybody, which is what makes the global count global', async () => {
    const b = budget();
    const env = envWith(limiter().binding, b.namespace);
    await worker.fetch(post({ ip: '203.0.113.1' }), env);
    await worker.fetch(post({ ip: '203.0.113.2' }), env);
    expect(b.ids()).toEqual(['budget']);
  });

  it('charges the budget on the edge address, like the limiter', async () => {
    const b = budget();
    await worker.fetch(post({ ip: '198.51.100.7' }), envWith(limiter().binding, b.namespace));
    expect(b.calls).toEqual(['198.51.100.7']);
  });

  it('turns the caller away at their own cap, and says when to come back', async () => {
    const b = budget();
    const env = envWith(limiter().binding, b.namespace, { global: 100, perIp: 1 });
    expect((await worker.fetch(post(), env)).status).toBe(200);
    const res = await worker.fetch(post(), env);
    expect(res.status).toBe(429);
    expect(Number(res.headers.get('Retry-After'))).toBeGreaterThan(0);
    expect(await res.json()).toMatchObject({ error: expect.stringContaining('a lot of feedback') });
    expect(globalThis.fetch).toHaveBeenCalledOnce();
  });

  it('turns everybody away once the day is spent, and says so differently', async () => {
    // The two messages differ because the remedies do. A caller over their own
    // cap did this; a caller meeting the global one did not.
    const b = budget();
    const env = envWith(limiter().binding, b.namespace, { global: 1, perIp: 10 });
    expect((await worker.fetch(post({ ip: '203.0.113.1' }), env)).status).toBe(200);
    const res = await worker.fetch(post({ ip: '203.0.113.2' }), env);
    expect(res.status).toBe(429);
    expect(await res.json()).toMatchObject({ error: expect.stringContaining('daily limit') });
    expect(globalThis.fetch).toHaveBeenCalledOnce();
  });

  it('spends no budget on a request that was never going to be mail', async () => {
    // The ordering this exists for: charged at the send, not at the door. A
    // budget charged before validation is drainable with bodies that 400,
    // which turns a bound on spend into an outage for everyone else.
    const b = budget();
    const res = await worker.fetch(
      post({ body: JSON.stringify({ type: 'bug', userAgent: 'x', timestamp: 'now' }) }),
      envWith(limiter().binding, b.namespace),
    );
    expect(res.status).toBe(400);
    expect(b.calls).toEqual([]);
    expect(b.ledgerNow()).toBeUndefined();
  });

  it('spends no budget on a body that was refused for its size', async () => {
    const b = budget();
    const res = await worker.fetch(
      post({ headers: { 'Content-Length': String(MAX_BODY_BYTES + 1) } }),
      envWith(limiter().binding, b.namespace),
    );
    expect(res.status).toBe(413);
    expect(b.calls).toEqual([]);
  });
});


/**
 * The caps reach the object with the request — SECURITY_AUDIT.md F12.
 *
 * Measured against production on 2026-09-23 and not guessed: two deploys in a
 * row, each changing a `[vars]` cap, and each probe was answered by the
 * PREVIOUS deploy's value. A Durable Object is constructed once and then
 * lives, so an env read inside it is the env it was born with. The caps are
 * read in the worker, per request, and travel in the URL.
 */
describe('a cap change takes effect without waiting for an eviction (F12)', () => {
  beforeEach(() => vi.stubGlobal('fetch', vi.fn(async () => new Response('{}', { status: 200 }))));
  afterEach(() => vi.unstubAllGlobals());

  it('sends both caps to the object', async () => {
    const seen: string[] = [];
    const namespace = {
      idFromName: (n: string) => n,
      get: () => ({
        fetch: async (url: string) => {
          seen.push(url);
          return new Response(JSON.stringify({ ok: true, refused: null, resetIn: 1 }));
        },
      }),
    };
    await worker.fetch(post(), envWith(limiter().binding, namespace, { global: 7, perIp: 3 }));
    expect(seen).toHaveLength(1);
    const params = new URL(seen[0]).searchParams;
    expect(params.get('global')).toBe('7');
    expect(params.get('perIp')).toBe('3');
  });

  it('reads a var that is absent as the default and one that is nonsense as an error', () => {
    // Falling back on absence is a default; falling back on garbage is a cap
    // silently ceasing to be the cap somebody set.
    expect(capsFrom({})).toEqual(DEFAULT_CAPS);
    expect(capsFrom({ FEEDBACK_DAILY_BUDGET: '5' }).global).toBe(5);
    expect(capsFrom({ FEEDBACK_DAILY_BUDGET: '0' }).global).toBe(0);
    expect(() => capsFrom({ FEEDBACK_DAILY_PER_IP: 'lots' })).toThrow(/non-negative integer/);
    expect(() => capsFrom({ FEEDBACK_DAILY_PER_IP: '-1' })).toThrow(/non-negative integer/);
    expect(() => capsFrom({ FEEDBACK_DAILY_PER_IP: '2.5' })).toThrow(/non-negative integer/);
  });

  it('refuses everything at a cap of zero, which is what the production probe set', async () => {
    const b = budget();
    const res = await worker.fetch(post(), envWith(limiter().binding, b.namespace, { global: 80, perIp: 0 }));
    expect(res.status).toBe(429);
    expect(globalThis.fetch).not.toHaveBeenCalled();
  });
});
