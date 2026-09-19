/**
 * F01 — the web client must not adopt a session it finds in a URL.
 *
 * auth-js defaults to the implicit flow, and this client never said otherwise. Implicit
 * plus `detectSessionInUrl` means a link to our own origin carrying
 * `#access_token=...&refresh_token=...` is read, verified against `/auth/v1/user` and
 * stored as the visitor's session — no `state`, nothing tying those tokens to a sign-in
 * this browser started. Login CSRF: the victim goes on using the site as the attacker,
 * and every build they save lands in the attacker's account.
 *
 * So the assertion is behavioural rather than a re-reading of the source. It drives the
 * real client with a real attacker URL and demands three things: no session, no request
 * carrying the attacker's token, and nothing left in storage. Put `flowType` back to
 * `'implicit'` and all three go the other way — the fetch happens, the session is adopted,
 * and the storage key appears.
 *
 * It grades `SUPABASE_CLIENT_OPTIONS` against a placeholder project rather than the
 * singleton, because the singleton is `null` without `VITE_SUPABASE_*` and CI has neither.
 */
import '@/test/localstorage-polyfill';
import { readFileSync } from 'node:fs';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createClient } from '@supabase/supabase-js';
import { SUPABASE_CLIENT_OPTIONS } from './supabase';

const PROJECT = 'https://abcdefghijklmnopqrst.supabase.co';
const ANON = 'anon-key-placeholder';

/** Shaped exactly as a provider redirect back to us, because that is what the attacker copies. */
const STOLEN_TOKENS =
  '#access_token=attacker-access-token&refresh_token=attacker-refresh-token' +
  '&expires_in=3600&token_type=bearer&provider_token=attacker-discord-token';

let fetched: string[] = [];
const realFetch = globalThis.fetch;

function browserAt(href: string) {
  fetched = [];
  // isBrowser() is `window && document`, and that is all auth-js needs to run the
  // URL-detection path — see auth-js helpers.js:43.
  Object.defineProperty(globalThis, 'window', {
    configurable: true,
    value: {
      location: { href, hash: href.includes('#') ? href.slice(href.indexOf('#')) : '' },
      history: { state: null, replaceState: () => {} },
    },
  });
  Object.defineProperty(globalThis, 'document', { configurable: true, value: {} });
  localStorage.clear();

  globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input instanceof Request ? input.url : input);
    fetched.push(url);
    // Answer as the identity service would, so the implicit path can actually succeed.
    // A stub that failed would make this test pass for the wrong reason.
    return new Response(
      JSON.stringify({ id: '00000000-0000-0000-0000-000000000000', aud: 'authenticated' }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    );
  }) as typeof fetch;
}

beforeEach(() => {
  browserAt(`https://coh-sidekick.com/${STOLEN_TOKENS}`);
});

afterEach(() => {
  globalThis.fetch = realFetch;
  Reflect.deleteProperty(globalThis, 'window');
  Reflect.deleteProperty(globalThis, 'document');
});

describe('the shipped Supabase client, handed a session in the URL', () => {
  it('refuses tokens in the fragment', async () => {
    const client = createClient(PROJECT, ANON, SUPABASE_CLIENT_OPTIONS);

    const { data } = await client.auth.getSession();

    expect(data.session).toBeNull();
    expect(fetched.join('\n')).not.toContain('attacker-access-token');
    expect(fetched.filter((u) => u.includes('/auth/v1/user'))).toEqual([]);
    expect(localStorage.length).toBe(0);
  });

  it('refuses tokens in the query string too, which is the copy nothing clears', async () => {
    // Implicit only ever cleared `window.location.hash`, so the query-borne variant stayed
    // in the address bar and in history after adoption. Under PKCE neither is adopted.
    browserAt(`https://coh-sidekick.com/?${STOLEN_TOKENS.slice(1)}`);
    const client = createClient(PROJECT, ANON, SUPABASE_CLIENT_OPTIONS);

    const { data } = await client.auth.getSession();

    expect(data.session).toBeNull();
    expect(fetched.join('\n')).not.toContain('attacker-access-token');
    expect(localStorage.length).toBe(0);
  });

  it('is on PKCE at the constructed client, not just in our literal', () => {
    const client = createClient(PROJECT, ANON, SUPABASE_CLIENT_OPTIONS);
    // Reads the property auth-js assigned from settings (GoTrueClient.js:131), so a
    // misspelled option name reds here instead of passing an object nobody reads.
    expect((client.auth as unknown as { flowType: string }).flowType).toBe('pkce');
  });
});

/**
 * The three tests above grade `SUPABASE_CLIENT_OPTIONS`. Nothing in them would notice the
 * options being dropped from the `createClient` call while the constant sat there unread,
 * which is the one-character regression that reopens F01 — so that call is asserted on the
 * module's own source, the same way the F19 projection is.
 */
describe('the singleton is built from those options', () => {
  const source = readFileSync(new URL('./supabase.ts', import.meta.url), 'utf8');

  it('calls createClient exactly once, and passes them', () => {
    const calls = source.match(/createClient\(/g) ?? [];
    expect(calls).toHaveLength(1);
    expect(source).toMatch(/createClient\(\s*supabaseUrl,\s*supabaseAnonKey,\s*SUPABASE_CLIENT_OPTIONS\s*\)/);
  });
});
