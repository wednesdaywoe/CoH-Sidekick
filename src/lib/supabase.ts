/**
 * Supabase client singleton for shared builds feature
 */

import { createClient } from '@supabase/supabase-js';
import type { SupabaseClientOptions, SupportedStorage } from '@supabase/supabase-js';

const supabaseUrl = import.meta.env.VITE_SUPABASE_URL;
const supabaseAnonKey = import.meta.env.VITE_SUPABASE_ANON_KEY;

/**
 * The provider's own credentials, which this app holds onto for nobody —
 * SECURITY_AUDIT.md F24.
 *
 * `provider_token` is a Discord access token and `provider_refresh_token` renews it.
 * GoTrue returns both from the OAuth code exchange and auth-js stores the token
 * endpoint's answer whole, so both sat in `localStorage` for the life of the session.
 * Nothing in this app reads either: the Supabase session that signs every request is
 * `access_token`, and these are for calling Discord on the user's behalf, which no
 * screen here does.
 *
 * They are also a credential this app **cannot revoke** — revoking a Discord token
 * needs the OAuth client secret, which lives in Supabase's provider config and reaches
 * no client. A credential that cannot be retired is one to not hold, especially beside
 * `public/rescue.html`, whose whole purpose is to hand this origin's `localStorage` to
 * whoever asks for it (F23, F76).
 */
const NOT_OURS_TO_KEEP = ['provider_token', 'provider_refresh_token'] as const;

/**
 * `value` with those two fields removed, or `value` unchanged when it is not a session.
 *
 * Every non-session write must pass through untouched, and there is a real one: auth-js
 * stores the PKCE `code_verifier` under `${storageKey}-code-verifier` through this same
 * adapter, as a JSON *string* rather than an object. Mangling that would break the sign-in
 * this whole flow exists for, so anything that is not a plain object is returned as it
 * arrived — and so is anything that fails to parse, because a write we cannot read is not
 * a write to rewrite.
 */
export function withoutProviderCredentials(value: string): string {
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    return value;
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) return value;

  const session = parsed as Record<string, unknown>;
  if (!NOT_OURS_TO_KEEP.some((field) => field in session)) return value;
  for (const field of NOT_OURS_TO_KEEP) delete session[field];
  return JSON.stringify(session);
}

/**
 * `localStorage`, with F24's rule on the way in.
 *
 * A storage adapter rather than a post-hoc cleanup of the stored blob, because auth-js
 * rewrites that blob from its in-memory session on every refresh — so a cleanup would be
 * undone an hour later, and the second write is the one nobody watches. Every write goes
 * through here, which also means a session stored before this rule existed is cleaned the
 * next time it is touched, with no migration to run.
 *
 * `globalThis` and the try/catch rather than `window.localStorage` directly: this module
 * is imported at module scope by tests that run in vitest's node environment, and by the
 * capture-mode boot, and a throw here would take the whole import with it.
 */
const store = (): Storage | null => {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
};

const sessionStorage: SupportedStorage = {
  getItem: (key) => store()?.getItem(key) ?? null,
  setItem: (key, value) => {
    store()?.setItem(key, withoutProviderCredentials(value));
  },
  removeItem: (key) => {
    store()?.removeItem(key);
  },
};

/**
 * Exported so a test can grade the options the shipped client is built from without
 * needing `VITE_SUPABASE_*` — CI has neither, so a guard that reached for the real
 * singleton would grade `null` and pass forever.
 *
 * `flowType` is the whole reason this object exists. auth-js defaults to `'implicit'`
 * (`GoTrueClient.js:24`), and under implicit + `detectSessionInUrl` this client adopts
 * any session handed to it in a URL: `#access_token=...&refresh_token=...&expires_in=...
 * &token_type=bearer` on our own origin is fetched against `/auth/v1/user` and stored,
 * with no `state` and nothing tying the tokens to a sign-in this browser started. That
 * is login CSRF — a victim lands on the attacker's account and their saves, their
 * favourites and every build they share go there.
 *
 * PKCE closes it by binding the callback to a `code_verifier` this browser generated and
 * kept: the callback carries `?code=`, the exchange needs the verifier, and an attacker
 * cannot mint one for someone else's browser. It also refuses the implicit shape outright
 * rather than ignoring it — `_getSessionFromURL` throws "Not a valid PKCE flow url." on a
 * token-bearing fragment before any network call — which is what makes the old attack a
 * refusal and not merely a failure.
 *
 * `storage` is F24's half, and it is here rather than in a post-sign-in sweep because
 * auth-js rewrites the stored blob on every refresh — see `sessionStorage` above.
 */
export const SUPABASE_CLIENT_OPTIONS = {
  auth: { flowType: 'pkce', storage: sessionStorage },
} satisfies SupabaseClientOptions<'public'>;

export const supabase = supabaseUrl && supabaseAnonKey
  ? createClient(supabaseUrl, supabaseAnonKey, SUPABASE_CLIENT_OPTIONS)
  : null;
