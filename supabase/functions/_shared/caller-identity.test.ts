/**
 * Grades `classifyCaller` — SECURITY_AUDIT.md F09.
 *
 * The finding is one collapsed distinction, so the suite is built around the
 * two directions of it, and a suite that only had one direction would pass a
 * broken function either way:
 *
 *   - **Refuse an unconfirmable session.** Return `anonymous` here and
 *     `share-build` publishes a build the user asked to keep private, under a
 *     200, and files it with no owner. This is the finding.
 *   - **Let a real anonymous caller through.** Both clients send the project's
 *     publishable key as a bearer on every signed-out request, so a rule that
 *     refused every bearer the sign-in service rejects would refuse every
 *     anonymous share on both clients. That is the failure a fix is most
 *     likely to ship, and it is worse than the finding.
 *
 * The tokens below are built rather than pasted so it is visible which claim
 * each case turns on.
 */

import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  bearerToken,
  claimsASession,
  classifyCaller,
  type Verdict,
} from './caller-identity';

const ALICE = '11111111-1111-1111-1111-111111111111';

/** base64url, the way a JWT encodes its segments. */
const segment = (value: unknown): string =>
  Buffer.from(JSON.stringify(value), 'utf8')
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');

/** A JWT with the given claims. Unsigned on purpose — nothing here verifies one. */
const jwt = (claims: Record<string, unknown>): string =>
  `${segment({ alg: 'HS256', typ: 'JWT' })}.${segment(claims)}.signature-not-checked-here`;

/** What a signed-in user's access token looks like to this module. */
const sessionToken = (extra: Record<string, unknown> = {}) =>
  jwt({ sub: ALICE, role: 'authenticated', exp: 4102444800, ...extra });

/** The legacy project key format: a JWT, but claiming the anon role. */
const legacyAnonKey = jwt({ role: 'anon', iss: 'supabase', exp: 4102444800 });

/** The current project key format. Not a JWT at all. */
const publishableKey = 'sb_publishable_AbCdEf0123456789';

const vouchesFor = (userId: string) =>
  vi.fn(async (): Promise<Verdict> => ({ outcome: 'user', userId }));
const rejects = () => vi.fn(async (): Promise<Verdict> => ({ outcome: 'rejected' }));
const unreachable = () => vi.fn(async (): Promise<Verdict> => ({ outcome: 'unreachable' }));
const explodes = () =>
  vi.fn(async (): Promise<Verdict> => {
    throw new TypeError('fetch failed');
  });

describe('bearerToken', () => {
  it('reads the token out of a bearer header', () => {
    expect(bearerToken('Bearer abc.def.ghi')).toBe('abc.def.ghi');
  });

  it('matches the scheme case-insensitively, as RFC 7235 says it is', () => {
    expect(bearerToken('bearer abc')).toBe('abc');
    expect(bearerToken('BEARER abc')).toBe('abc');
  });

  it('tolerates the whitespace a proxy might add', () => {
    expect(bearerToken('  Bearer   abc  ')).toBe('abc');
  });

  it('finds no token in an absent, empty, or differently-schemed header', () => {
    expect(bearerToken(null)).toBeNull();
    expect(bearerToken(undefined)).toBeNull();
    expect(bearerToken('')).toBeNull();
    expect(bearerToken('Bearer ')).toBeNull();
    expect(bearerToken('Basic dXNlcjpwYXNz')).toBeNull();
  });
});

describe('claimsASession', () => {
  it('is true only for a token claiming the authenticated role', () => {
    expect(claimsASession(sessionToken())).toBe(true);
  });

  it('is false for both project key formats', () => {
    // These are what a signed-out caller sends. If either read as a session,
    // every anonymous share on both clients would be refused.
    expect(claimsASession(publishableKey)).toBe(false);
    expect(claimsASession(legacyAnonKey)).toBe(false);
  });

  it('is false for anything that is not a JWT, without throwing', () => {
    expect(claimsASession('not-a-jwt')).toBe(false);
    expect(claimsASession('a.b.c')).toBe(false);
    expect(claimsASession('....')).toBe(false);
    expect(claimsASession(`${segment({ x: 1 })}.!!!not-base64!!!.sig`)).toBe(false);
    // A JWT whose payload is valid base64url but not an object.
    expect(claimsASession(`x.${segment([1, 2, 3])}.sig`)).toBe(false);
    expect(claimsASession(`x.${segment('a string')}.sig`)).toBe(false);
    expect(claimsASession(`x.${segment(null)}.sig`)).toBe(false);
  });

  it('reads a payload carrying non-ASCII claims', () => {
    // atob is byte-wise; a name with an accent in it must not throw the whole
    // classification into the anonymous branch.
    expect(claimsASession(sessionToken({ full_name: 'Zoe Renee' + String.fromCharCode(0x301) }))).toBe(true);
  });
});

describe('classifyCaller', () => {
  it('calls nobody anonymous once a session has been asserted', async () => {
    // The finding, stated as one property: every outcome for a session-claiming
    // token is `user` or `unverified`. None of them is `anonymous`.
    for (const verify of [vouchesFor(ALICE), rejects(), unreachable(), explodes()]) {
      const caller = await classifyCaller(`Bearer ${sessionToken()}`, verify);
      expect(caller.kind).not.toBe('anonymous');
    }
  });

  it('refuses a session the service will not vouch for', async () => {
    const caller = await classifyCaller(`Bearer ${sessionToken()}`, rejects());
    expect(caller).toEqual({ kind: 'unverified', detail: expect.stringMatching(/sign in again/i) });
  });

  it('refuses when the sign-in service does not answer', async () => {
    // Rule 1: an outage is not a demotion to anonymous. Both the explicit
    // verdict and a thrown fetch land here.
    for (const verify of [unreachable(), explodes()]) {
      const caller = await classifyCaller(`Bearer ${sessionToken()}`, verify);
      expect(caller.kind).toBe('unverified');
      expect((caller as { detail: string }).detail).toMatch(/try again/i);
    }
  });

  it('tells the two refusals apart, so the message names what happened', async () => {
    const stale = await classifyCaller(`Bearer ${sessionToken()}`, rejects());
    const down = await classifyCaller(`Bearer ${sessionToken()}`, unreachable());
    expect(stale).not.toEqual(down);
  });

  it('returns the user the service vouched for', async () => {
    const caller = await classifyCaller(`Bearer ${sessionToken()}`, vouchesFor(ALICE));
    expect(caller).toEqual({ kind: 'user', userId: ALICE });
  });

  it('does not read exp itself — an expired session is still the service to judge', async () => {
    // Deciding expiry from unverified claims would make a clock skew into a
    // refusal and a forged exp into an acceptance. The token is sent on.
    const expired = sessionToken({ exp: 1 });
    expect(await classifyCaller(`Bearer ${expired}`, vouchesFor(ALICE))).toEqual({
      kind: 'user',
      userId: ALICE,
    });
    expect((await classifyCaller(`Bearer ${expired}`, rejects())).kind).toBe('unverified');
  });

  it('treats a signed-out caller as anonymous and never asks the service', async () => {
    // The other direction, and the one a careless fix breaks. Both clients send
    // the project key as a bearer on every signed-out request.
    for (const header of [null, undefined, '', 'Bearer ' + publishableKey, 'Bearer ' + legacyAnonKey]) {
      const verify = rejects();
      expect(await classifyCaller(header, verify)).toEqual({ kind: 'anonymous' });
      expect(verify).not.toHaveBeenCalled();
    }
  });

  it('hands the verifier the exact token it was sent', async () => {
    const token = sessionToken();
    const verify = vouchesFor(ALICE);
    await classifyCaller(`Bearer ${token}`, verify);
    expect(verify).toHaveBeenCalledWith(token);
  });
});

/**
 * The rule above is graded in isolation, which leaves one gap: nothing in it
 * proves `share-build` actually *asks*. The handler cannot be imported — its
 * `index.ts` opens with `https://esm.sh` imports and calls `Deno.serve` at
 * module scope — so the wiring is asserted over its source instead.
 *
 * The needles live in this file and the haystack is a different one, which is
 * the only reason this works: an assertion whose literal sits inside the file
 * it searches matches itself and stays green with the call site gutted. That
 * was a real defect in F01's guard, found by mutating it.
 */
describe('share-build asks, and refuses what comes back unverified', () => {
  const source = readFileSync(
    new URL('../share-build/index.ts', import.meta.url),
    'utf8',
  );

  it('has no path left that reads an unconfirmable session as anonymous', () => {
    // The helper this replaced. Its whole contract was `Promise<string | null>`,
    // so reintroducing it reintroduces the finding.
    expect(source).not.toContain('getUserIdFromAuth');
    expect(source).toContain("import { classifyCaller");
  });

  it('answers 401 on an unverified caller', () => {
    const at = source.indexOf("caller.kind === 'unverified'");
    expect(at, 'the handler never tests for an unverified caller').toBeGreaterThan(-1);
    expect(source.slice(at, at + 400)).toContain('status: 401');
  });

  it('derives authUserId from the classified caller and nothing else', () => {
    const assignments = source.match(/const authUserId = .*/g) ?? [];
    expect(assignments).toEqual([
      "const authUserId = caller.kind === 'user' ? caller.userId : null;",
    ]);
  });

  it('classifies before it validates, meters, or writes', () => {
    // A refusal that arrived after the rate-limit insert would spend a slot the
    // caller never got to use.
    const classified = source.indexOf('await classifyRequestCaller(');
    const metered = source.indexOf("from('rate_limits')");
    const written = source.indexOf("from('shared_builds')");
    expect(classified).toBeGreaterThan(-1);
    expect(classified).toBeLessThan(metered);
    expect(classified).toBeLessThan(written);
  });
});
