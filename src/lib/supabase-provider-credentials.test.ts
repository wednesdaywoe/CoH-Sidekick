/**
 * F24 — the Discord token this app never uses must not sit in `localStorage`.
 *
 * GoTrue returns `provider_token` and `provider_refresh_token` from the OAuth code
 * exchange, and auth-js stores the token endpoint's answer whole, so both were kept for
 * the life of the session. Nothing here reads either one: the Supabase session that signs
 * every request is `access_token`, and these are for calling Discord on the user's behalf,
 * which no screen in this app does.
 *
 * They are also unrevokable from a client — revoking a Discord token needs the OAuth
 * client secret, which lives in Supabase's provider config — and they sat beside
 * `public/rescue.html`, whose whole purpose is to hand this origin's `localStorage` to
 * whoever asks (F23, F76).
 *
 * **The rule has two directions and the second is the one a careless fix breaks.** Strip
 * too little and the credential stays. Strip too much and the PKCE `code_verifier`, which
 * goes through this same adapter as a JSON string rather than an object, is mangled — and
 * then no sign-in completes at all, which is worse than the finding.
 */
import '@/test/localstorage-polyfill';
import { describe, it, expect, beforeEach } from 'vitest';
import { withoutProviderCredentials, SUPABASE_CLIENT_OPTIONS } from './supabase';

/** Shaped as GoTrue's token response is, because that is what auth-js hands the adapter. */
const sessionBlob = (extra: Record<string, unknown> = {}) =>
  JSON.stringify({
    access_token: 'supabase-access',
    refresh_token: 'supabase-refresh',
    token_type: 'bearer',
    expires_in: 3600,
    expires_at: 4_102_444_800,
    provider_token: 'discord-access-token',
    provider_refresh_token: 'discord-refresh-token',
    user: { id: 'u-1', user_metadata: { full_name: 'Ms "Quote" O\'Brien' } },
    ...extra,
  });

describe('withoutProviderCredentials', () => {
  it('takes both provider credentials out of a session', () => {
    const cleaned = withoutProviderCredentials(sessionBlob());
    expect(cleaned).not.toContain('discord-access-token');
    expect(cleaned).not.toContain('discord-refresh-token');
    // The keys go too, not just the values — an empty-string provider_token is still a
    // field that says this app asked for one.
    expect(cleaned).not.toContain('provider_token');
    expect(cleaned).not.toContain('provider_refresh_token');
  });

  it('leaves the session itself whole, so this is an exclusion and not a projection', () => {
    // Without this, "remove two fields" and "keep only the fields we know about" pass the
    // same test — and the second silently loses whatever auth-js adds next.
    const cleaned = JSON.parse(
      withoutProviderCredentials(sessionBlob({ some_field_auth_js_adds_next: 'keep-me' })),
    );
    expect(cleaned.access_token).toBe('supabase-access');
    expect(cleaned.refresh_token).toBe('supabase-refresh');
    expect(cleaned.expires_at).toBe(4_102_444_800);
    expect(cleaned.user.user_metadata.full_name).toBe('Ms "Quote" O\'Brien');
    expect(cleaned.some_field_auth_js_adds_next).toBe('keep-me');
  });

  it('passes the PKCE code verifier through untouched', () => {
    // auth-js stores it under `${storageKey}-code-verifier` via this same adapter, as a
    // JSON string. Mangling it means no sign-in completes — worse than the finding.
    const verifier = JSON.stringify('a'.repeat(112));
    expect(withoutProviderCredentials(verifier)).toBe(verifier);
  });

  it('passes through anything that is not a JSON object', () => {
    for (const value of ['', 'not json at all', '[1,2,3]', 'null', '42', '"plain string"']) {
      expect(withoutProviderCredentials(value), JSON.stringify(value)).toBe(value);
    }
  });

  it('returns a session with no provider credentials exactly as it arrived', () => {
    // A refresh response carries no provider token, and rewriting it would be churn that
    // could only introduce a difference.
    const plain = JSON.stringify({ access_token: 'a', refresh_token: 'r' });
    expect(withoutProviderCredentials(plain)).toBe(plain);
  });
});

describe('the shipped client applies it on every write', () => {
  const storage = SUPABASE_CLIENT_OPTIONS.auth.storage;

  beforeEach(() => {
    globalThis.localStorage.clear();
  });

  it('is wired into the options the client is built from', () => {
    // The rule is only worth anything if the client actually uses this adapter. Grading
    // the function alone would pass with `storage` never passed to createClient.
    expect(storage).toBeDefined();
    expect(typeof storage!.setItem).toBe('function');
  });

  it('strips on the way in, so nothing lands in localStorage', async () => {
    await storage!.setItem('sb-project-auth-token', sessionBlob());
    const landed = globalThis.localStorage.getItem('sb-project-auth-token') ?? '';
    expect(landed).not.toContain('discord-access-token');
    expect(landed).not.toContain('provider_token');
    expect(landed).toContain('supabase-access');
  });

  it('strips again on the refresh write, which is the one nobody watches', async () => {
    // auth-js rewrites the blob from its in-memory session on every refresh, so a one-off
    // cleanup of storage would be undone an hour later. This is why the rule lives in the
    // adapter rather than in a post-sign-in sweep.
    await storage!.setItem('sb-project-auth-token', sessionBlob());
    await storage!.setItem('sb-project-auth-token', sessionBlob({ access_token: 'refreshed' }));
    const landed = globalThis.localStorage.getItem('sb-project-auth-token') ?? '';
    expect(landed).not.toContain('discord-access-token');
    expect(landed).toContain('refreshed');
  });

  it('round-trips the verifier through the adapter, not just through the function', async () => {
    const verifier = JSON.stringify('b'.repeat(112));
    await storage!.setItem('sb-project-auth-token-code-verifier', verifier);
    expect(await storage!.getItem('sb-project-auth-token-code-verifier')).toBe(verifier);
  });

  it('removes what it is asked to remove', async () => {
    await storage!.setItem('sb-project-auth-token', sessionBlob());
    await storage!.removeItem('sb-project-auth-token');
    expect(await storage!.getItem('sb-project-auth-token')).toBeNull();
  });
});
