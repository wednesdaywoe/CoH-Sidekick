/**
 * What may be stored in `profiles.avatar_url` — SECURITY_AUDIT.md F07.
 *
 * `update-profile` copies `user_metadata.avatar_url` straight into the column,
 * and `user_metadata` is writable by its own user: one `PUT /auth/v1/user` with
 * `{"data": {"avatar_url": "..."}}` and the string is whatever they typed. The
 * column is then rendered as a raw `<img src>` by the author card, the author
 * filter (once per row, eight rows per search), the author page and the profile
 * page, to strangers — so the value decides where a viewer's browser makes a
 * request, and therefore who learns that viewer's IP and user-agent.
 *
 * The rule is an allow-list of one host, and the reason it can be that narrow is
 * measured rather than assumed. There is no avatar upload anywhere in either
 * client; the only writer is this copy from the Discord OAuth claim, and a
 * census of production on 2026-09-19 found **452 avatars across 530 profiles,
 * every one of them on `https://cdn.discordapp.com`, and 78 null**. Nothing
 * legitimate has ever been anywhere else.
 *
 * **Refusing at the write is what makes the render sites safe.** They are five
 * on the web and three on the desktop, they are in two languages, and only one
 * of the two has a CSP at all. A stored column that cannot hold a hostile value
 * needs no cooperation from any of them — and it is the one place the rule can
 * be applied once. `cloud/avatar.rs::avatar_src` stays as the desktop's own
 * `img-src`, because a value written before this rule existed is still a value
 * that renders, and because F81's abort is reached through the renderer.
 *
 * A refused url is stored as NULL, which every render site already handles: it
 * is the same "no avatar" placeholder a user without Discord gets. A refusal is
 * a missing picture, never a broken page.
 */

/** The one host an avatar may be served from. Discord's OAuth CDN, which is
 *  where every avatar the app has ever stored actually lives. */
export const AVATAR_HOST = 'cdn.discordapp.com';

/** Nothing legitimate is anywhere near this long. A url that is really a
 *  payload is refused before any of the parsing below runs on it. */
const MAX_AVATAR_URL_LENGTH = 512;

/** True for a character no real url contains and that two parsers may read
 *  differently — a control character, a DEL, or any whitespace. */
function splitsTheUrl(text: string): boolean {
  if (/\s/.test(text)) return true;
  for (const ch of text) {
    const code = ch.charCodeAt(0);
    if (code <= 0x1f || code === 0x7f) return true;
  }
  return false;
}

/**
 * `raw` if it may be stored and rendered, otherwise `null`.
 *
 * Deliberately a hand-checked prefix and authority rather than `new URL()`:
 * the question is not "does this parse", it is "does every parser that later
 * reads this agree about its host", and a url carrying a control character, a
 * space or a userinfo section is one they disagree about. Those are refused
 * outright rather than normalised.
 */
export function storableAvatarUrl(raw: unknown): string | null {
  if (typeof raw !== 'string') return null;
  if (raw.length === 0 || raw.length > MAX_AVATAR_URL_LENGTH) return null;
  if (splitsTheUrl(raw)) return null;

  if (!raw.startsWith('https://')) return null;
  const rest = raw.slice('https://'.length);

  const authority = rest.split(/[/?#]/)[0];
  // `https://cdn.discordapp.com@evil.example/` has an authority of
  // `evil.example`, and reading the host as "the part before the slash" is
  // exactly how that trick works.
  if (authority.includes('@')) return null;

  const host = authority.split(':')[0].toLowerCase();
  return host === AVATAR_HOST ? raw : null;
}
