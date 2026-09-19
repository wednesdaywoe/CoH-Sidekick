/**
 * Who may write to a `shared_builds` row — SECURITY_AUDIT.md F30, F31, F80.
 *
 * Three functions guarded one resource three different ways. `delete-build` and
 * `share-build`'s update path accepted a bare owner token on an account-owned
 * build; `claim-builds` went further and re-assigned `user_id` to whoever held
 * the token. `update-build-visibility` refused token-only callers outright. The
 * weakest of four is what an attacker uses, and one of them is destructive.
 *
 * The rule, stated once: the owner token is the whole authority for an
 * UNCLAIMED build, because an anonymous sharer has no other handle on it. Once
 * `user_id` is set the token stops being sufficient. It lives in localStorage
 * on whatever machine ran the share, it is never rotated, and `public/rescue.html`
 * will hand the whole store to anyone who asks (F23, F76) — so it is a weaker
 * credential than a session, and it must not outrank one.
 *
 * Split out of the three functions rather than written inline for the reason
 * `author-name.ts` was: each `index.ts` opens with `https://esm.sh` imports and
 * calls `Deno.serve` at module scope, so no test runner can import it. This file
 * imports nothing, vitest grades it, and `supabase functions deploy` still
 * bundles it.
 */

/** A `shared_builds` row's claim state, as the three callers read it. */
export interface BuildOwnership {
  /** `shared_builds.user_id` — null while the build is unclaimed. */
  buildUserId: string | null;
  /** Whether the caller presented an owner token hashing to `owner_token_hash`. */
  tokenMatches: boolean;
  /** The caller's authenticated user id, or null when not logged in. */
  authUserId: string | null;
}

/**
 * True when the caller may update or delete this build.
 *
 * Claimed builds answer to their account and nothing else; unclaimed builds
 * answer to the token. An authenticated caller holding the token of a build
 * they never claimed still passes, which is the ordinary case: you shared it
 * anonymously, then logged in.
 */
export function mayWriteBuild({ buildUserId, tokenMatches, authUserId }: BuildOwnership): boolean {
  if (buildUserId !== null) {
    return authUserId !== null && authUserId === buildUserId;
  }
  return tokenMatches;
}

/**
 * True when the caller may set `user_id` on this build.
 *
 * Stricter than `mayWriteBuild` because claiming is not a write to the build,
 * it is a transfer of the build. A token holder can claim what nobody owns and
 * re-claim what is already theirs; taking it from another account is the
 * finding (F31), and the token holder is exactly who would do it.
 */
export function mayClaimBuild({ buildUserId, tokenMatches, authUserId }: BuildOwnership): boolean {
  if (!tokenMatches || authUserId === null) return false;
  return buildUserId === null || buildUserId === authUserId;
}
