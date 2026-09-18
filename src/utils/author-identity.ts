/**
 * What a shared build's author line is allowed to say — the web half of
 * SECURITY_AUDIT.md F69, mirroring `cloud/profile.rs::author_identity` in the
 * canonical repo.
 *
 * `author_name` is free text the sharer typed. `author_handle` rides the same
 * row because `shared_builds_with_author` joins the owner's profile onto it,
 * and it is the only part of an author line a client can prove. Drawing them
 * as one string is what made the free-text half an identity claim rather than
 * a label: a card reading `@tigereyes` is indistinguishable from a card
 * belonging to the account that actually holds that handle.
 *
 * **F69 was filed as an asymmetry between this card and the desktop one, and
 * that premise was wrong.** This card never drew the handle either — it marked
 * a verified author only by making the name a link, which says nothing to
 * anyone who does not hover it. So this is not parity work; it is the same
 * defect, on the client that most people actually use.
 *
 * **The invariant is one line: an `@` on an author line always came from a
 * handle the join proved, never from `author_name`.**
 *
 * The server-side normaliser does not make this redundant.
 * `_shared/author-name.ts` runs at write time and the rows already stored were
 * never rewritten: measured against production on 2026-09-18, across all 5,007
 * rows, 64 public rows open with `@` — 45 naming a handle nobody holds, 18
 * naming one the row's own account does hold, and one anonymous share
 * (`user_id IS NULL`) naming a handle that exists. No row has a signed-in
 * account claiming a different account's handle.
 *
 * That last row is the argument rather than an incident: its dullest reading
 * is that the holder of that handle shared while signed out and typed their
 * own name, and nothing in the data separates that from the other reading.
 * A client that cannot tell the two apart must not draw the one that would be
 * a claim.
 */

/** A row with no author label and no account behind it. */
export interface AnonymousAuthor {
  kind: 'anonymous';
}

/** A display name with nothing behind it. Never carries a sigil. */
export interface UnverifiedAuthor {
  kind: 'unverified';
  display: string;
}

/**
 * A handle the profile join proved, and whatever free-text name the share
 * carried beside it.
 *
 * `display` is empty when the share named no author; the handle then stands
 * alone rather than the line reading "Anonymous", because the row demonstrably
 * has an owner. Not a hypothetical: of the 1,328 public rows whose author
 * holds a handle, 101 carry no `author_name` (measured 2026-09-18).
 */
export interface VerifiedAuthor {
  kind: 'verified';
  display: string;
  handle: string;
}

export type AuthorIdentity = AnonymousAuthor | UnverifiedAuthor | VerifiedAuthor;

/** The handle sigil. Mirrors `SIGIL` in `_shared/author-name.ts`. */
const SIGIL = '@';

/**
 * What a display name may open with: anything that draws ink, and nothing that
 * draws none. Mirrors `OPENER` in `_shared/author-name.ts`, and the comment
 * there carries the full argument for the shape.
 *
 * **This is the third spelling of this rule, and the first two were bypassed
 * in the same way.** `/^@+/` stopped at a space, so `@ @savant` kept its
 * sigil. `/^[@\s]+/` fixed that and stopped at a blank-rendering *letter*, so
 * `\u3164@admin` — U+3164 HANGUL FILLER, category `Lo` — kept its sigil and
 * this card drew it. A leading-run regex has to list what may be skipped, and
 * the list was never going to be finished; stating what may END the run
 * instead is what makes the next blank a dropped character rather than a live
 * claim.
 */
const OPENER = /(?!\p{Default_Ignorable_Code_Point}|\u2800)[\p{L}\p{N}\p{M}\p{S}\p{P}]/u;

/**
 * Drop the leading run that is not the name: the sigil, and anything that may
 * not open one. One pass over the code points, because the two interleave —
 * `@\u3164@admin` defeats any fixed order of two replaces.
 */
function stripClaimedPrefix(value: string): string {
  const points = Array.from(value);
  let i = 0;
  while (i < points.length && (points[i] === SIGIL || !OPENER.test(points[i]))) i += 1;
  return points.slice(i).join('');
}

/**
 * Decide what an author line says, from the two columns that carry it.
 *
 * Case survives: a display name is a label, not a lookup key, and `Savant` is
 * not `savant`. The handle is lowercased because it IS the key — it is matched
 * against `profiles.handle` and it is what `/author/$handle` routes on.
 *
 * What this deliberately does not mirror is the rest of `sanitizeAuthorName` —
 * the bidi overrides, the zero-width family, the `\p{Zs}` impostors. Those are
 * how one name is made to render as another rather than how it claims a
 * namespace; they are removed at the door BEFORE the sigil rule runs, and no
 * stored row carries one (zero across all 5,007 rows, 2026-09-18). A second
 * copy of that table kept in sync for a population of zero is the trade being
 * refused. The sigil is copied because the sigil has a population of 64.
 *
 * **That bound was drawn from a measurement that could not see the thing that
 * broke it.** The zero was counted over bidi, zero-width and `\p{Zs}` — three
 * named classes — and U+3164 is in none of them: it is a one-column
 * blank-*rendering* letter, so the query that justified "a population of zero"
 * never asked about its class. The population of leading blank-renderers in
 * stored rows is still unmeasured; [`stripClaimedPrefix`] does not need the
 * number, because it refuses to draw the claim either way. Measuring it is
 * F69's remaining question, not a precondition for this rule.
 */
export function authorIdentity(
  authorName: string | null | undefined,
  authorHandle: string | null | undefined,
): AuthorIdentity {
  const display = stripClaimedPrefix(authorName ?? '').trim();
  const handle = stripClaimedPrefix(authorHandle ?? '').trim().toLowerCase();
  if (handle) return { kind: 'verified', display, handle };
  return display ? { kind: 'unverified', display } : { kind: 'anonymous' };
}
