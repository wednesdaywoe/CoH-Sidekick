//! The desktop client's `img-src`.
//!
//! Three surfaces render a stranger's `avatar_url` straight into an `img src`: the
//! build browser's author card and author filter, and your own profile. The web
//! build is covered by the CSP `vite.config.ts` injects, which allows exactly
//! `'self' data: https://cdn.discordapp.com`. The desktop build has no CSP at
//! all, so this function is that directive, written out.
//!
//! **`https://*.supabase.co` was on both lists until 2026-09-19, and it was the
//! weakest thing on either.** That pattern admits any Supabase project on the
//! internet — an attacker-controllable origin class, sitting inside the one rule
//! that decides where a stranger's `avatar_url` may send a viewer's browser. It
//! was there for an avatar upload that does not exist in either client, and a
//! census of production on that date found every one of 452 stored avatars on
//! `cdn.discordapp.com`, with 78 null and nothing else. It bought nothing and
//! cost the whole rule, so both lists are one host now.
//!
//! The write side went with it: `_shared/avatar-url.ts` refuses the same shapes
//! before the column is written, so nothing hostile is stored to be drawn. This
//! function stays because a value written before that rule existed still
//! renders, and because F81's abort is reached through the renderer.
//!
//! What makes it more than hardening: a RELATIVE url resolves against the
//! `dioxus://` scheme, and `dioxus-asset-resolver 0.7.9` decodes the percent
//! escapes of such a url with `.decode_utf8().expect(...)` inside wry's
//! `extern "C" fn start_task` — a nounwind boundary, so a malformed escape
//! aborts the process rather than unwinding, and no `catch_unwind` can reach it.
//! An `avatar_url` of `/%FF` is therefore a stored, stranger-triggered kill on
//! any client that renders it. Refusing everything but an absolute https url on
//! a known host means nothing we draw ever enters that resolver.
//!
//! The upstream `expect` is still there and F81 stays open against it; this
//! closes the one path this app opens to it.

/// The avatar url `raw` if it is safe to hand to an `img src`, otherwise `None`.
///
/// Callers render the "no avatar" placeholder on `None`, which is the same thing
/// they already do for an absent url — a refused avatar is a missing picture,
/// never a broken client.
pub fn avatar_src(raw: &str) -> Option<&str> {
    // Nothing legitimate is anywhere near this long; a url that is really a
    // payload is refused before any of the parsing below runs on it.
    if raw.len() > 512 {
        return None;
    }
    // A control character or a space in an attribute value is never part of a
    // real url and is how one url is made to read as two.
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }

    let rest = raw.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    // `https://cdn.discordapp.com@evil.example/` has an authority of
    // `evil.example`, and reading the host as "the part before the slash" is how
    // that trick works.
    if authority.contains('@') {
        return None;
    }
    let host = authority.split(':').next()?.to_ascii_lowercase();

    // The same one origin the web CSP names, and for the same reason: Discord
    // serves the OAuth avatar, and nothing else has ever served one.
    if host != "cdn.discordapp.com" {
        return None;
    }
    Some(raw)
}
