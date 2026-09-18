/**
 * Normalisation for `shared_builds.author_name` — the free-text author label a
 * sharer types, and for a long time the only author identity ANY build card
 * drew. SECURITY_AUDIT.md F69 recorded that as an asymmetry between the two
 * clients; it was not one. Neither card drew the handle: the web card
 * (`components/shared/BuildCard.tsx`) renders `author_name` alone and marks a
 * verified author only by making the name a link, and the desktop card
 * rendered it alone with no mark at all. So an unchecked string here was an
 * impersonation vector on both. The desktop client draws the proved `@handle`
 * beside the name as of 2026-09-18 (`cloud/profile.rs::author_identity`); the
 * web card does not yet, and F69's row says so.
 *
 * Split out of `share-build/index.ts` rather than written inline because the
 * function itself cannot be imported by the test runner — it opens with
 * `https://esm.sh` imports and calls `Deno.serve` at module scope. This file
 * imports nothing, so vitest can grade it (`author-name.test.ts`) and
 * `supabase functions deploy share-build` still bundles it, which is what the
 * `_shared/` convention is for.
 *
 * What this does NOT try to be: a uniqueness rule. Display names are not
 * unique and never were — two people genuinely called "Savant" both get to be
 * Savant. What it removes is the ways a name can claim to be *something other
 * than a display name*: an invisible character that hides the difference
 * between two names, and the `@` sigil that makes a display name read as the
 * claimed-and-unique handle. The collision cases that remain are handled where
 * they belong, by the client drawing the proved `@handle` beside the name.
 */

/** Longest stored author_name. Matches the column's historical `.slice(0, 50)`. */
export const MAX_AUTHOR_NAME = 50;

/**
 * The invisibles that are also SEPARATORS — a tab, the control newlines, and
 * the U+2028/U+2029 line and paragraph separators. These become a space rather
 * than vanishing, and that distinction is the whole reason this constant is
 * separate from [`INVISIBLE`] below.
 *
 * **The first version of this file stripped them, and it was wrong.** Deleting
 * the newlines from `Savant\n\nAdministrator` yields `SavantAdministrator` —
 * two words welded into one name that was never typed, which is a worse
 * misreading than the one the stripping was for. Caught by the test, not by
 * the reasoning that wrote the comment.
 */
const SEPARATORS = /[\t\n\v\f\r\p{Zl}\p{Zp}]/gu;

/**
 * Characters with no legitimate place in a display name, a direct use in
 * spoofing one, and no width to justify a space where they were:
 *
 *   * `\p{Cc}` — the rest of the C0/C1 controls, once [`SEPARATORS`] has taken
 *     the ones that mean "break here".
 *   * `\p{Cf}` — format characters. This is the class that matters: the bidi
 *     overrides (U+202A-U+202E, U+2066-U+2069) that let a name render
 *     right-to-left over its neighbours, the zero-width space/joiner family
 *     (U+200B-U+200D) that makes two distinct strings render identically,
 *     U+00AD soft hyphen, and U+FEFF.
 *
 * **Known cost, accepted:** `\p{Cf}` includes U+200D, so an emoji ZWJ sequence
 * in a name (a family glyph, a profession glyph) degrades to its component
 * emoji rather than being kept whole. That is a rendering loss in a rare name,
 * weighed against a class whose entire remaining use here is hiding one name
 * inside another. Keeping ZWJ would mean enumerating the rest of `Cf`, and an
 * enumeration is the thing that goes stale.
 */
const INVISIBLE = /[\p{Cc}\p{Cf}]/gu;

/**
 * Any run of whitespace, INCLUDING the `\p{Zs}` impostors — U+00A0 no-break
 * space, U+2007 figure space, U+3000 ideographic space. Those render as a gap
 * and compare as a distinct character, which is how a name gets padded into
 * looking like a different one. Collapsed to a single ASCII space rather than
 * stripped, so the words of a real name stay separated.
 */
const WHITESPACE = /[\p{Zs}\s]+/gu;

/**
 * The handle sigil. `@name` is how this app spells a *claimed, unique*
 * identity — it is the `/author/@handle` route, the author pin in the browser,
 * and what the desktop card now draws beside a name whose account actually
 * holds one. A display name opening with it is claiming that namespace without
 * holding anything in it, so the sigil comes off and the name behind it stays.
 *
 * **The class is `[@\s]`, not `@`, and the difference is a defect this file
 * shipped with.** The first version was `/^@+/`, which strips a *run* of
 * sigils — `@@savant` — but not one held apart by a space. Applied once, after
 * the trim, to `@ @savant`, it removes the first sigil and returns
 * `@savant`: a stored value that still opens with the sigil and still names a
 * handle, which is the entire thing this constant exists to prevent. The
 * single pass is what makes it possible, because nothing re-reads what the
 * strip exposed. Found on 2026-09-18 by the canonical client's own sweep over
 * the same rule (`cloud/profile.rs::author_identity`), not by rereading this.
 *
 * Interior sigils are still left alone — `Savant @ Everlasting` is a name, not
 * a claim. What makes a claim is opening with one.
 */
const LEADING_SIGIL = /^[@\s]+/;

/**
 * Normalise a caller-supplied author name into what may be stored.
 *
 * Returns the empty string for anything that is not a string or that has
 * nothing left after normalising — an anonymous share with no author label is
 * the column's own default and an ordinary outcome, not an error.
 *
 * Order is load-bearing:
 *
 *   1. **NFC first.** Otherwise a decomposed name is measured and truncated in
 *      combining marks, and a later composition changes the length.
 *   2. **Separators to a space, THEN strip the widthless invisibles, THEN
 *      collapse.** Each step depends on the one before: a newline must leave a
 *      gap behind it, a zero-width space sitting between two real spaces must
 *      not survive as the thing that keeps them from collapsing, and the
 *      collapse has to run after both to see the spaces they produced.
 *   3. **Sigil after trimming**, so ` @savant` is caught as well as `@savant`.
 *   4. **Truncate LAST, and by code point.** `String.prototype.slice` counts
 *      UTF-16 units, so the old `.slice(0, 50)` could cut an astral character
 *      in half and store a lone surrogate — a real defect the length rule was
 *      hiding, not a hypothetical one. `Array.from` iterates code points.
 */
export function sanitizeAuthorName(raw: unknown): string {
  if (typeof raw !== 'string') return '';

  const cleaned = raw
    .normalize('NFC')
    .replace(SEPARATORS, ' ')
    .replace(INVISIBLE, '')
    .replace(WHITESPACE, ' ')
    .trim()
    .replace(LEADING_SIGIL, '')
    .trim();

  const points = Array.from(cleaned);
  return points.length <= MAX_AUTHOR_NAME
    ? cleaned
    : points.slice(0, MAX_AUTHOR_NAME).join('').trim();
}

/**
 * The lookup key for the two impersonation checks `share-build` runs against
 * the database — `reserved_handles` (the service's own names) and
 * `profiles.handle` (somebody's claimed one). Both columns are `CITEXT`, so
 * the comparison is already case-insensitive at the database; this exists so
 * the *shape* matches what a handle is — `HANDLE_REGEX` in `update-profile` is
 * `^[a-z0-9][a-z0-9_-]{2,29}$`, and a name with a space in it cannot collide
 * with one however it is cased.
 *
 * Returns null when the name could not be a handle at all, which lets the
 * caller skip both round trips for the overwhelming majority of shares.
 */
export function handleCandidate(name: string): string | null {
  const lowered = name.toLowerCase();
  return /^[a-z0-9][a-z0-9_-]{2,29}$/.test(lowered) ? lowered : null;
}
