/**
 * Grades `sanitizeAuthorName` and `handleCandidate` — SECURITY_AUDIT.md F69.
 *
 * Every case here is written to fail if the rule it names is deleted, which is
 * the only reason this file is worth its bytes. The two easiest assertions to
 * write here are also the two worthless ones, so both are avoided on purpose:
 * a fixture whose fields all hold their default values cannot see a field go
 * missing, and a length assertion on an ASCII name cannot see a truncation cut
 * a character in half. The inputs below do not hold still.
 */

import { describe, it, expect } from 'vitest';
import { sanitizeAuthorName, handleCandidate, MAX_AUTHOR_NAME } from './author-name';

describe('sanitizeAuthorName', () => {
  it('leaves an ordinary name exactly as typed', () => {
    // The rule this guards is the one nobody writes down: a sanitiser that
    // mangles the common case is worse than the vector it closes.
    expect(sanitizeAuthorName('Wednesday Woe')).toBe('Wednesday Woe');
    expect(sanitizeAuthorName("D'Arcy O'Malley-Smith")).toBe("D'Arcy O'Malley-Smith");
    expect(sanitizeAuthorName('小次郎')).toBe('小次郎');
    expect(sanitizeAuthorName('Statesman \u{1F9B8}')).toBe('Statesman \u{1F9B8}');
  });

  it('returns empty for the non-strings the request body can carry', () => {
    // body.author_name is whatever JSON.parse produced. The old
    // `(body.author_name || '').slice(0, 50)` called .slice on a number.
    for (const bad of [undefined, null, 42, true, {}, [], ['a']]) {
      expect(sanitizeAuthorName(bad)).toBe('');
    }
  });

  // ---- invisible characters ----

  it('strips bidi overrides, so a name cannot render over its neighbour', () => {
    // U+202E RIGHT-TO-LEFT OVERRIDE is the classic: everything after it
    // renders reversed, which is how one name is made to read as another. The
    // cards draw the proved @handle beside the name now, but that only marks
    // what IS verified — it does nothing about one free-text name rendering as
    // a different free-text one, which is why this still has to happen here.
    expect(sanitizeAuthorName('Savant\u202egnittimda')).toBe('Savantgnittimda');
    expect(sanitizeAuthorName('\u202bsavant\u202c')).toBe('savant');
    expect(sanitizeAuthorName('\u2066savant\u2069')).toBe('savant');
  });

  it('strips the zero-width family, so two distinct names cannot render alike', () => {
    // The impersonation that needs no cleverness: this renders as "savant" and
    // stores as a different string, so it passes any uniqueness rule there
    // might ever be while reading as the real name.
    const spoofed = 's\u200bav\u200can\u200dt\ufeff';
    expect(sanitizeAuthorName(spoofed)).toBe('savant');
    // And the reason to strip rather than reject: it collapses ONTO the real
    // name rather than beside it.
    expect(sanitizeAuthorName(spoofed)).toBe(sanitizeAuthorName('savant'));
  });

  it('strips control characters that would split a name across lines', () => {
    expect(sanitizeAuthorName('Savant\n\nAdministrator')).toBe('Savant Administrator');
    expect(sanitizeAuthorName('Savant\u0000\u0007')).toBe('Savant');
    expect(sanitizeAuthorName('Savant\u2028Staff')).toBe('Savant Staff');
  });

  // ---- whitespace ----

  it('collapses the whitespace impostors, not just ASCII space', () => {
    // U+00A0 and U+3000 render as a gap and compare as themselves, which is
    // how a name gets padded into looking like a different one.
    expect(sanitizeAuthorName('Savant\u00a0\u00a0Prime')).toBe('Savant Prime');
    expect(sanitizeAuthorName('\u3000Savant\u3000')).toBe('Savant');
    expect(sanitizeAuthorName('  Savant   Prime  ')).toBe('Savant Prime');
  });

  // ---- the handle sigil ----

  it('strips the @ sigil, which is how this app spells a claimed identity', () => {
    // /author/@handle is a route, and the desktop card draws @handle beside a
    // name whose account holds one. A free-text name may not open with it.
    expect(sanitizeAuthorName('@savant')).toBe('savant');
    expect(sanitizeAuthorName('  @savant')).toBe('savant');
    // Repeats: stripping one layer would leave the claim standing.
    expect(sanitizeAuthorName('@@@savant')).toBe('savant');
    // An invisible in front of the sigil must not shield it.
    expect(sanitizeAuthorName('\u200b@savant')).toBe('savant');
  });

  it('strips a sigil that a space holds apart from the one before it', () => {
    // The defect the first version of this file shipped with. `/^@+/` takes a
    // run of sigils and stops at the space, and the strip runs once, so
    // `@ @savant` came out as `@savant` — still opening with the sigil, still
    // naming a handle. Every assertion here passed `@@savant` and failed
    // `@ @savant` before the class became `[@\\s]`.
    expect(sanitizeAuthorName('@ @savant')).toBe('savant');
    expect(sanitizeAuthorName('@ @ savant')).toBe('savant');
    expect(sanitizeAuthorName('  @  @  savant')).toBe('savant');
    // The impostor spaces are collapsed before this runs, so they cannot hold
    // a sigil apart either.
    expect(sanitizeAuthorName('@\u00a0@savant')).toBe('savant');
    expect(sanitizeAuthorName('@\u3000@savant')).toBe('savant');
  });

  it('leaves a name that is nothing but sigils and spaces empty', () => {
    // The anonymous outcome, not a name made of spaces: `@ @ @` claims a
    // namespace and holds nothing, so nothing is what is left of it.
    expect(sanitizeAuthorName('@ @ @')).toBe('');
  });

  it('keeps an @ that is not the sigil', () => {
    // Stripping every @ would eat a legitimate name. Only a leading one is a
    // namespace claim.
    expect(sanitizeAuthorName('Savant @ Everlasting')).toBe('Savant @ Everlasting');
  });

  // ---- truncation ----

  it('truncates by code point, so an astral character is never cut in half', () => {
    // The defect the old `.slice(0, 50)` carried: slice counts UTF-16 units,
    // so 50 emoji are 100 units and a cut at 50 lands between a surrogate
    // pair, storing a lone surrogate. Asserted on the ROUND TRIP, because a
    // length assertion alone passes on broken output.
    const astral = '\u{1F9B8}'.repeat(60);
    const out = sanitizeAuthorName(astral);
    expect(Array.from(out)).toHaveLength(MAX_AUTHOR_NAME);
    expect(out).toBe('\u{1F9B8}'.repeat(MAX_AUTHOR_NAME));
    // A lone surrogate does not survive a JSON round trip intact; this asserts
    // none was produced.
    expect(JSON.parse(JSON.stringify(out))).toBe(out);
    // `\p{Surrogate}` must carry the `u` flag. Without it the class is matched
    // per UTF-16 unit and a VALID pair matches too, so the assertion fires on
    // correct output — which is exactly what it did when first written.
    expect(out).not.toMatch(/\p{Surrogate}/u);
  });

  it('truncates AFTER normalising, not before', () => {
    // Sixty invisibles in front of a short name consume the whole budget if
    // the cut comes first, storing nothing. The old code sliced the raw body.
    const padded = '\u200b'.repeat(60) + 'Savant';
    expect(sanitizeAuthorName(padded)).toBe('Savant');
  });

  it('does not leave a trailing space when the cut lands on one', () => {
    // 49 + space + more: the 50th code point IS the space, so without the
    // post-truncation trim the stored name ends in one.
    const name = 'a'.repeat(49) + ' bcdef';
    expect(sanitizeAuthorName(name)).toBe('a'.repeat(49));
    // And a cut mid-word keeps the partial word rather than dropping it — the
    // truncation is a length cap, not a word boundary.
    expect(sanitizeAuthorName('a'.repeat(48) + ' bcdef')).toBe('a'.repeat(48) + ' b');
  });

  it('normalises to NFC before measuring', () => {
    // Decomposed "e" + combining acute is two code points and composes to one.
    // Measuring the decomposed form spends two of the fifty on one glyph, and
    // a cut there strands a combining mark onto whatever precedes it.
    const decomposed = 'e\u0301'.repeat(30);
    const out = sanitizeAuthorName(decomposed);
    expect(out).toBe('\u00e9'.repeat(30));
    expect(Array.from(out)).toHaveLength(30);
  });

  it('returns empty when nothing survives', () => {
    expect(sanitizeAuthorName('\u200b\u200c\u202e   \u3000')).toBe('');
    expect(sanitizeAuthorName('@@@')).toBe('');
    expect(sanitizeAuthorName('')).toBe('');
  });
});

describe('handleCandidate', () => {
  it('matches update-profile HANDLE_REGEX, lowercased', () => {
    // The two collision checks only mean anything if this agrees with what a
    // handle actually is. update-profile: ^[a-z0-9][a-z0-9_-]{2,29}$.
    expect(handleCandidate('Savant')).toBe('savant');
    expect(handleCandidate('WEDNESDAYWOE')).toBe('wednesdaywoe');
    expect(handleCandidate('a_b-c9')).toBe('a_b-c9');
    expect(handleCandidate('abc')).toBe('abc');
    expect(handleCandidate('a'.repeat(30))).toBe('a'.repeat(30));
  });

  it('is null for anything that could not be a handle', () => {
    // Skipping the two round trips for these is the point: a name with a space
    // in it cannot collide with a handle however it is cased.
    expect(handleCandidate('Wednesday Woe')).toBeNull();
    expect(handleCandidate('ab')).toBeNull(); // under 3
    expect(handleCandidate('a'.repeat(31))).toBeNull(); // over 30
    expect(handleCandidate('_savant')).toBeNull(); // leading underscore
    expect(handleCandidate('-savant')).toBeNull(); // leading dash
    expect(handleCandidate('savant!')).toBeNull();
    expect(handleCandidate('小次郎')).toBeNull();
    expect(handleCandidate('')).toBeNull();
  });
});
