//! A build arriving as a URL fragment — the `#…` on a share link or an `/import` link.
//!
//! The fragment is base64, sometimes over a deflate stream, and what comes out is one of two
//! documents this repo already has a reader for: a JSON build, or the text a game client's
//! `/buildsave` wrote. This module unwraps the transport and says which reader takes what it
//! found. It resolves nothing, and it holds no [`crate::PowerDatabase`].
//!
//! **Transport and payload are orthogonal, so they are decided separately.** The beta pairs
//! them — its uncompressed branch accepts a payload only if it starts with `{` — which makes a
//! deflated game export readable and a plain one not, for no reason either format states. Here
//! the base64 is undone, then each way of reading the bytes is tried in turn, and the first
//! result a reader claims wins. A `/buildsave` text travels compressed or not, and so does a
//! build.
//!
//! **The refusal belongs to the reader that can state it precisely**, which is why
//! [`LinkPayload::Json`] carries text rather than a parsed anything: a `.skif` needs a dataset
//! to read, and deciding v5 versus the legacy tier is [`crate::skif::probe_version`]'s job. A
//! second router here could only disagree with that one. The game-export arm is asymmetric
//! because its reader needs no dataset, so proving the arm and producing its value are one act.

use crate::game_export::{self, ExportError, GameExport};
use flate2::read::{DeflateDecoder, ZlibDecoder};
use std::io::Read;

/// What a fragment turned out to be carrying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkPayload {
    /// A JSON document: a `.skif` of any version, or an external planner's export. Which one is
    /// a question for the router that already answers it, not for this one.
    Json(String),
    /// A `/buildsave` export: the text as the link carried it, and the read that proved it was
    /// one. Both, because the reader downstream parks TEXT across a dataset reload and re-reads
    /// it there, while the arm was only decidable by parsing — so returning one of the two would
    /// make somebody redo the other's work.
    GameExport { text: String, export: GameExport },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("the link carries no build data")]
    Empty,
    #[error("the fragment is not base64")]
    NotBase64,
    #[error("the link decoded to bytes that are not text")]
    NotText,
    #[error("the link carries more than {limit} bytes, so it is not a build")]
    TooLarge { limit: usize },
    #[error("the link decoded, but nothing here reads what it carried: {refusal}")]
    Unreadable { refusal: ExportError },
}

/// The most a fragment may carry, encoded.
///
/// Bounds the read before any allocation sized by it. The base64 decoder reserves
/// `text.len() / 4 * 3` up front, so an unbounded fragment is an allocation sized by a stranger
/// before a single byte has been validated.
///
/// **Measured, with the headroom stated.** The largest fragment in `fixtures/import-link` is
/// `game-export/charnel.frag` at 17,452 base64 characters. One mebibyte is about sixty times
/// that, so refusing above it cannot refuse a real link.
const MAX_FRAGMENT_BYTES: usize = 1 << 20;

/// The most a fragment may inflate to, and the same reasoning as
/// [`crate::mxd`]'s `MAX_INFLATED_BYTES`, which this deliberately matches.
///
/// The input bound above does not imply this one: deflate's ratio is unbounded, so a few hundred
/// bytes that pass `MAX_FRAGMENT_BYTES` can still inflate to gigabytes. `read_to_end` materialised
/// all of it and only then asked whether it was text.
///
/// **Measured, with the headroom stated.** Across the seventeen fixtures in
/// `fixtures/import-link`, the largest inflates to 41,927 bytes (`planner/homecoming-kheldian`),
/// and the ratios run 3.41 to 12.64. One mebibyte is about twenty-five times the largest, so a
/// document declaring more than this is not a build whatever else it is.
const MAX_INFLATED_BYTES: usize = 1 << 20;

/// Read a URL fragment as a build, with or without its leading `#`.
pub fn decode_fragment(fragment: &str) -> Result<LinkPayload, LinkError> {
    let encoded = fragment.strip_prefix('#').unwrap_or(fragment).trim();
    if encoded.is_empty() {
        return Err(LinkError::Empty);
    }
    // Before `base64`, which reserves three bytes for every four it is handed.
    if encoded.len() > MAX_FRAGMENT_BYTES {
        return Err(LinkError::TooLarge {
            limit: MAX_FRAGMENT_BYTES,
        });
    }
    let bytes = base64(encoded).ok_or(LinkError::NotBase64)?;

    // Raw deflate is what the planner writes, zlib is the same stream under a header some
    // encoder might add, and the bytes themselves are the uncompressed case. A stream that is
    // not deflate at all usually fails to inflate, and where it doesn't the output is not
    // text — so UTF-8 is the filter that keeps a garbage arm from being offered to a reader.
    let readable: Vec<String> = [inflate_raw(&bytes), inflate_zlib(&bytes), Some(bytes)]
        .into_iter()
        .flatten()
        .filter_map(|out| String::from_utf8(out).ok())
        .collect();

    for text in &readable {
        if text.trim_start().starts_with('{') {
            return Ok(LinkPayload::Json(text.clone()));
        }
        if let Ok(export) = game_export::parse(text) {
            return Ok(LinkPayload::GameExport {
                text: text.clone(),
                export,
            });
        }
    }

    // Nothing claimed it. Report the last candidate's own refusal rather than a generic one:
    // the arms run compressed-first, so the last is the uncompressed reading — the bytes as the
    // link actually carried them, which is the one a person can check against what they pasted.
    match readable.last() {
        Some(text) => Err(LinkError::Unreadable {
            refusal: game_export::parse(text).expect_err("a claimed text would have returned"),
        }),
        None => Err(LinkError::NotText),
    }
}

fn inflate_raw(bytes: &[u8]) -> Option<Vec<u8>> {
    inflate(DeflateDecoder::new(bytes))
}

fn inflate_zlib(bytes: &[u8]) -> Option<Vec<u8>> {
    inflate(ZlibDecoder::new(bytes))
}

/// Inflate, bounded.
///
/// `take(limit + 1)` rather than `limit`, so hitting the cap is distinguishable from landing
/// exactly on it: a stream that fills the extra byte is over, and is dropped rather than
/// truncated. Returning a truncated buffer would be the soft-wrong failure Rule 1 refuses — the
/// arms downstream would read a prefix of a build and offer whatever it parsed as.
fn inflate(decoder: impl Read) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    decoder
        .take(MAX_INFLATED_BYTES as u64 + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() > MAX_INFLATED_BYTES {
        return None;
    }
    (!out.is_empty()).then_some(out)
}

/// Base64 to bytes, tolerant in the three ways a fragment that has been through a URL bar, a
/// chat client or an email needs: either alphabet, padding optional, embedded whitespace
/// ignored. Decode only: nothing in this repo writes a fragment, so there is no encoder here to
/// stay in step with (`app::clipboard::base64` is the app's, and it encodes PNG bytes for JS).
fn base64(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    let mut padded = false;
    for byte in text.bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            padded = true;
            continue;
        }
        // A sextet after the padding means the padding was not the tail, so this is not one
        // encoded value and guessing which half to keep would invent a build.
        if padded {
            return None;
        }
        acc = acc << 6 | u32::from(sextet(byte)?);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    // 6 leftover bits is a lone trailing character: no length of input produces it.
    (bits < 6).then_some(out)
}

fn sextet(byte: u8) -> Option<u8> {
    Some(match byte {
        b'A'..=b'Z' => byte - b'A',
        b'a'..=b'z' => byte - b'a' + 26,
        b'0'..=b'9' => byte - b'0' + 52,
        // URL-safe `-`/`_` alongside standard `+`/`/`: a fragment is written in one alphabet
        // and read in whichever survived the trip.
        b'+' | b'-' => 62,
        b'/' | b'_' => 63,
        _ => return None,
    })
}
