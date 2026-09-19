/**
 * Who is calling an edge function — SECURITY_AUDIT.md F09.
 *
 * Five functions each wrote their own `getUserIdFromAuth`, and all five return
 * `null` for two things that are not the same thing:
 *
 *   - **nobody signed in**, which is an ordinary anonymous caller, and
 *   - **somebody signed in, and we could not confirm it** — an expired or
 *     revoked token, a forged one, a rotated JWT secret, or a sign-in service
 *     that did not answer.
 *
 * Collapsing those two into `null` is a fail-open wherever "anonymous" is the
 * more permissive branch, and in `share-build` it is that twice over: an
 * unverifiable caller has their requested `private` forced to `public`, and the
 * row is filed with `user_id: null` so it never appears in their library and
 * answers to the owner token alone. Both happen under a 200.
 *
 * So this module returns three states, and the caller must decide what to do
 * with the third. Rule 1: a credential we cannot check is refused out loud, not
 * quietly demoted.
 *
 * **The one thing that makes this harder than it sounds.** Both clients send an
 * `Authorization` header on every request whether anyone is signed in or not —
 * supabase-js falls back to the publishable key in `_getAccessToken`, and the
 * Rust client's `Cloud::bearer` does the same for the same reason (a request
 * with no Authorization dies at the gateway). So "presented a bearer token" is
 * NOT "presented a credential", and a rule that refused every bearer the
 * sign-in service rejects would refuse every anonymous share on both clients.
 *
 * The discriminator is therefore what the token *claims to be*, read before
 * anything is verified: only a JWT claiming `role: "authenticated"` is a user
 * session. The project key is `sb_publishable_...` (not a JWT at all) or, in the
 * legacy format, a JWT claiming `role: "anon"`. Reading unverified claims is
 * safe here because of which way the decision falls: a token that fails this
 * test is treated as *anonymous*, the least privileged outcome, so forging the
 * claim buys an attacker nothing. The claim only ever raises the bar — it says
 * "this caller asserts a session, so make them prove it."
 *
 * Split out of the functions rather than written inline for the reason
 * `build-ownership.ts` was: each `index.ts` opens with `https://esm.sh` imports
 * and calls `Deno.serve` at module scope, so no test runner can import it. This
 * file imports nothing, vitest grades it, and `supabase functions deploy` still
 * bundles it.
 */

/** Who the request is from, once the header has been read and checked. */
export type Caller =
  /** No session was asserted. The ordinary signed-out visitor. */
  | { kind: 'anonymous' }
  /** A session was asserted and the sign-in service vouched for it. */
  | { kind: 'user'; userId: string }
  /**
   * A session was asserted and could not be confirmed. Never treat this as
   * `anonymous` — that is the finding. Answer 401 with `detail`.
   */
  | { kind: 'unverified'; detail: string };

/** What a token check came back with. `rejected` and `unreachable` are both refusals,
 *  kept apart only so the person reading the message is told which happened. */
export type Verdict =
  | { outcome: 'user'; userId: string }
  | { outcome: 'rejected' }
  | { outcome: 'unreachable' };

/** The token out of an `Authorization: Bearer <token>` header, or null.
 *
 *  The scheme is matched case-insensitively because RFC 7235 says it is
 *  case-insensitive; an empty token is no token. */
export function bearerToken(authHeader: string | null | undefined): string | null {
  if (typeof authHeader !== 'string') return null;
  const match = /^\s*Bearer\s+(\S+)\s*$/i.exec(authHeader);
  return match ? match[1] : null;
}

/** The JWT payload, decoded WITHOUT verifying anything. Null if it is not a JWT.
 *
 *  Nothing read out of here may be trusted — see `claimsASession` for the only
 *  question it is allowed to answer. */
function unverifiedClaims(token: string): Record<string, unknown> | null {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  try {
    const base64 = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    const padded = base64 + '='.repeat((4 - (base64.length % 4)) % 4);
    const binary = atob(padded);
    // atob yields one char per byte; re-read those bytes as UTF-8 so a payload
    // with a non-ASCII claim parses instead of throwing.
    const text = new TextDecoder().decode(
      Uint8Array.from(binary, (c) => c.charCodeAt(0)),
    );
    const parsed: unknown = JSON.parse(text);
    return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : null;
  } catch {
    return null;
  }
}

/**
 * Does this token assert a signed-in user?
 *
 * The only question the unverified claims are allowed to answer, and it decides
 * one thing: whether failing verification is a refusal (true) or a shrug
 * (false). A project key — `sb_publishable_...`, or a legacy JWT with
 * `role: "anon"` — asserts nothing and is the signed-out state both clients
 * send.
 */
export function claimsASession(token: string): boolean {
  return unverifiedClaims(token)?.role === 'authenticated';
}

/**
 * Read the `Authorization` header and check any session it asserts.
 *
 * `verify` is the one part that needs the network, so it is the caller's to
 * supply; a throw from it is read as `unreachable`, which is a refusal, because
 * the alternative is the fail-open this module exists to remove.
 */
export async function classifyCaller(
  authHeader: string | null | undefined,
  verify: (token: string) => Promise<Verdict>,
): Promise<Caller> {
  const token = bearerToken(authHeader);
  if (token === null || !claimsASession(token)) return { kind: 'anonymous' };

  let verdict: Verdict;
  try {
    verdict = await verify(token);
  } catch {
    verdict = { outcome: 'unreachable' };
  }

  switch (verdict.outcome) {
    case 'user':
      return { kind: 'user', userId: verdict.userId };
    case 'rejected':
      return {
        kind: 'unverified',
        detail: 'This sign-in is no longer valid. Please sign in again and retry.',
      };
    case 'unreachable':
      return {
        kind: 'unverified',
        detail: 'Could not confirm your sign-in right now. Please try again in a moment.',
      };
  }
}
