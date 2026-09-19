/**
 * Supabase client singleton for shared builds feature
 */

import { createClient } from '@supabase/supabase-js';
import type { SupabaseClientOptions } from '@supabase/supabase-js';

const supabaseUrl = import.meta.env.VITE_SUPABASE_URL;
const supabaseAnonKey = import.meta.env.VITE_SUPABASE_ANON_KEY;

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
 */
export const SUPABASE_CLIENT_OPTIONS = {
  auth: { flowType: 'pkce' },
} as const satisfies SupabaseClientOptions<'public'>;

export const supabase = supabaseUrl && supabaseAnonKey
  ? createClient(supabaseUrl, supabaseAnonKey, SUPABASE_CLIENT_OPTIONS)
  : null;
