/**
 * Guard for text fields that didn't actually resolve to text.
 *
 * Most strings in the bins are stored literally, but some are stored as a
 * MESSAGE-STORE KEY — `P` followed by a 32-bit hash, e.g. `P2937209522` — which
 * the game client resolves at display time against `bin/clientmessages-en.bin`.
 * The parser has no message-store resolver, so those fields come out of the
 * export holding the key itself.
 *
 * Downstream nothing distinguishes that from real text, so it renders: the
 * Mercenaries > Soldiers power showed a tag reading literally "P2937209522".
 * The honest handling is to treat the key as absent — a power with no short help
 * shows no tags, which is true, where a hash chip is false and unreadable.
 *
 * This is a symptom guard, not the fix. The fix is a message-store resolver in
 * `tools/bin-crawler`; this only makes sure an unresolved key can never
 * masquerade as a display string. Keeping it as a named predicate rather than an
 * inline regex is what lets the guard test enumerate the same rule the converters
 * apply.
 */

// `P` + at least six digits. Real short-help text never matches: it always
// carries spaces, punctuation or lower-case words ("Ranged, Moderate DMG(Fire)").
const MESSAGE_STORE_KEY = /^P\d{6,}$/;

/** True when a string is an unresolved message-store key rather than text. */
function isUnresolvedMessageKey(value) {
  return typeof value === 'string' && MESSAGE_STORE_KEY.test(value.trim());
}

/**
 * A display string, or undefined when the export handed us a message-store key
 * instead of one. Use at every point a raw `display_*` field becomes
 * user-visible text.
 *
 * An EMPTY string passes through unchanged. It is a real (if uninformative)
 * value that the schema may require — `Power.description` is non-optional, so
 * swallowing '' here turns a cosmetic guard into a type error two datasets
 * away. This function answers one question only: is this text, or a hash?
 */
function displayText(value) {
  if (typeof value !== 'string') return undefined;
  return isUnresolvedMessageKey(value) ? undefined : value;
}

/**
 * A `display_help` string as text, with the client's markup resolved.
 *
 * The bins store help exactly as the client renders it: `<br>` breaks and `<color #fcfc95>`
 * runs. Deleting every tag with nothing in its place glues the sentences either side of a
 * break together — "...losing health over time.Notes: This power will deal critical damage"
 * — which is what 3,135 Homecoming descriptions shipped. A `<br>` is an authored break, so
 * it becomes a newline; every other tag is client presentation and goes. An unterminated
 * `<` is text, not a tag, and survives (`crates/app/src/view/power_view.rs::paragraphs`
 * makes the same call on the reading side).
 */
function helpText(value) {
  const text = displayText(value);
  if (text === undefined) return undefined;
  return text
    .replace(/<\s*br\s*\/?\s*>/gi, '\n')
    .replace(/<[^>]+>/g, '')
    .replace(/[ \t]+(?=\n)/g, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

module.exports = { displayText, helpText, isUnresolvedMessageKey, MESSAGE_STORE_KEY };
