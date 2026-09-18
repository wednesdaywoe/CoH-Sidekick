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
 *   * `\p{Default_Ignorable_Code_Point}` — the property Unicode maintains for
 *     precisely this question: code points a conforming renderer draws as
 *     nothing. It is what catches the HANGUL FILLER family — U+3164, U+115F,
 *     U+1160, U+FFA0 — which are category `Lo`, *letters* that render blank,
 *     and so walked past all three of the classes above. SECURITY_AUDIT.md
 *     F69: `\u3164@admin` reached both clients with its sigil intact.
 *
 * **Known cost, accepted:** `\p{Cf}` includes U+200D, so an emoji ZWJ sequence
 * in a name (a family glyph, a profession glyph) degrades to its component
 * emoji rather than being kept whole. That is a rendering loss in a rare name,
 * weighed against a class whose entire remaining use here is hiding one name
 * inside another. Keeping ZWJ would mean enumerating the rest of `Cf`, and an
 * enumeration is the thing that goes stale.
 *
 * **Second known cost, same trade:** the default-ignorables include the
 * variation selectors U+FE00-U+FE0F, so an emoji's presentation selector comes
 * off with them and a glyph may render text-style where the sharer meant
 * emoji-style. Accepted for the reason above, and cheaper than the ZWJ loss
 * this file already took: the character survives, only its styling does not.
 */
const INVISIBLE = /[\p{Cc}\p{Cf}\p{Default_Ignorable_Code_Point}]/gu;

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
 * **This constant was twice a regex and twice wrong, which is why it is now
 * one character and the rule lives in [`stripClaimedPrefix`].** The first
 * version was `/^@+/`: it strips a *run* of sigils — `@@savant` — but not one
 * held apart by a space, so `@ @savant` lost its first sigil and returned
 * `@savant`, a stored value still opening with the sigil and still naming a
 * handle. The second was `/^[@\s]+/`, which fixed that by widening the class,
 * and was bypassed in turn by `ㅤ@admin` — a blank-rendering *letter* is
 * neither `@` nor `\s`. Both failures are one failure: a leading-run regex has
 * to enumerate what may be skipped, and the list is never finished.
 *
 * So the skipping is stated the other way round now. [`OPENER`] says what may
 * *end* the prefix, and everything before the first of those goes, sigil or
 * blank or both in any order. The two historical cases fall out rather than
 * being handled: `@ @savant` and `ㅤ@admin` both reduce because a space
 * and a filler are equally "not something a name opens with".
 *
 * Interior sigils are still left alone — `Savant @ Everlasting` is a name, not
 * a claim. What makes a claim is opening with one.
 */
const SIGIL = '@';

/**
 * What a display name may OPEN with — stated as a positive class on purpose,
 * because the subtractive version of this rule is the one that shipped
 * bypassed (F69). A name may open with anything that draws ink: a letter, a
 * number, a combining mark, a symbol (an emoji name is a real name), or
 * punctuation. What it may not open with is anything that draws nothing,
 * because the only thing an invisible opener does is hide the character behind
 * it — and the character worth hiding is the [`SIGIL`] above.
 *
 * The two exclusions are the blanks those ink categories would otherwise
 * admit. The default-ignorables are already gone by the time this runs
 * ([`INVISIBLE`] takes them); they are named here anyway so the class is true
 * read on its own. U+2800 BRAILLE PATTERN BLANK is the one that needs naming
 * for real — an empty braille cell, category `So`, *not* default-ignorable,
 * and it renders as a gap.
 *
 * **This goes stale too, and that is the point of the shape.** A blank glyph
 * Unicode adds tomorrow that is neither default-ignorable nor U+2800 will be
 * admitted as an opener. What changed is which way the staleness falls: an
 * unfamiliar *non-ink* character is now refused by default, so the failure
 * mode is a name that loses a leading character rather than a sigil that
 * survives to both clients. Subtraction failed open; this fails closed.
 *
 * Written as a lookahead rather than the `v`-flag set difference that says
 * this more directly (`[[…]--[…]]`). That flag is ES2024, this file runs under
 * whatever Deno the edge runtime happens to ship, and an unsupported flag is a
 * SyntaxError at module load — which would take `share-build` down whole
 * rather than degrade it. The two forms were swept against each other over
 * 71,200 code points and disagreed on none, so the older spelling costs
 * nothing but the indirection.
 */
const OPENER = /(?!\p{Default_Ignorable_Code_Point}|\u2800)[\p{L}\p{N}\p{M}\p{S}\p{P}]/u;

/**
 * Drop everything in front of the name: any character that may not open one,
 * and the sigil itself, up to the first character that may.
 *
 * One pass over the code points rather than two regex replaces, because the
 * two cases interleave — `@\u3164@admin` needs the sigil taken, then the
 * filler, then the sigil again, and any fixed number of passes is the same
 * enumeration bug wearing a different hat.
 */
function stripClaimedPrefix(name: string): string {
  const points = Array.from(name);
  let i = 0;
  while (i < points.length && (points[i] === SIGIL || !OPENER.test(points[i]))) i += 1;
  return points.slice(i).join('');
}

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
 *   3. **Prefix strip after trimming**, so ` @savant` is caught as well as
 *      `@savant`, and so the whitespace impostors are already spaces rather
 *      than characters the opener class would have to rule on.
 *   4. **Truncate LAST, and by code point.** `String.prototype.slice` counts
 *      UTF-16 units, so the old `.slice(0, 50)` could cut an astral character
 *      in half and store a lone surrogate — a real defect the length rule was
 *      hiding, not a hypothetical one. `Array.from` iterates code points.
 */
export function sanitizeAuthorName(raw: unknown): string {
  if (typeof raw !== 'string') return '';

  const cleaned = stripClaimedPrefix(
    raw
      .normalize('NFC')
      .replace(SEPARATORS, ' ')
      .replace(INVISIBLE, '')
      .replace(WHITESPACE, ' ')
      .trim(),
  ).trim();

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
