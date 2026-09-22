/**
 * Does this identity string claim to be somebody — SECURITY_AUDIT.md F69, F83.
 *
 * A display name is not unique and never was; two people genuinely called
 * "Savant" both get to be Savant. What this refuses is a name claiming to be
 * something other than a display name: one of the service's own reserved
 * names, or the `@handle` that a *different* account has proved it holds.
 *
 * **F83 is the reason this is a file rather than a function in `share-build`.**
 * F69's rule was applied to `shared_builds.author_name` and stopped there, so
 * `profiles.display_name` — the second free-text identity string, rendered in
 * the same author surfaces, written through a service-role path — had no rule
 * at all. A rule scoped to a column instead of to a concept is a rule with a
 * second column waiting behind it. Measured 2026-09-18: `author_name` was at 0
 * such claims and `display_name` was at 1, found on the first look.
 *
 * The lookup is injected rather than taken as a client, the way
 * `caller-identity.ts` does it, so the rule can be graded without a database
 * while the two callers keep their own queries.
 */

/** What the two lookups came back with. */
export interface HandleLookup {
  /** The name matches a `reserved_handles` row with `reason = 'system'`. */
  reserved: boolean;
  /** The `user_id` of the account holding this handle, or null if nobody does. */
  claimedBy: string | null;
}

/**
 * The refusal message, or null when the name claims nothing.
 *
 * `subject` names the field in the message, because the two callers are
 * different forms and "author name" in a profile dialog would be a bug report.
 */
export async function identityClaimRefusal(
  name: string,
  authUserId: string | null,
  candidateOf: (name: string) => string | null,
  lookup: (candidate: string) => Promise<HandleLookup>,
  subject: 'author name' | 'display name',
): Promise<string | null> {
  const candidate = candidateOf(name);
  // Not shaped like a handle, so it cannot collide with one however it is
  // cased - and the caller skips both round trips, which is the overwhelming
  // majority of writes.
  if (candidate === null) return null;

  const { reserved, claimedBy } = await lookup(candidate);
  if (reserved) {
    return `"${name}" is a reserved name. Please use a different ${subject}.`;
  }
  // An account may of course use its own handle as its name.
  if (claimedBy !== null && claimedBy !== authUserId) {
    return `"${name}" is the handle of a registered account. Please use a different ${subject}.`;
  }
  return null;
}
