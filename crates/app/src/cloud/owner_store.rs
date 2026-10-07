//! The two `localStorage` keys RB4e inherits from the beta, and the one rule they share.
//!
//! The WASM build replaces the beta ON THE SAME ORIGIN, so it does not start with empty storage —
//! it opens the storage the beta left. Two of its keys are compatibility contracts rather than
//! implementation details, which is to say: the cost of getting them wrong is not an error, it is
//! silence.
//!
//! * `coh-planner-owner-tokens` — how an anonymous share stays editable by whoever made it. The
//!   server keeps only a SHA-256 of the token ([`share-build/index.ts`] hashes it on the way in),
//!   so this map is the only copy of the secret that exists. Change its name or its JSON shape and
//!   every anonymous build a user made in the beta becomes uneditable and undeletable, with no
//!   error anywhere to find it by — the builds still load, still render, and simply never again
//!   answer to their author.
//! * `coh-planner-quick-share` — the short-link cache, so re-copying an unchanged build is instant
//!   and does not burn a rate-limit slot. Cleared on sign-out, so one user's link is not updated
//!   by the next user on that browser.
//!
//! Two more keys on this origin belong elsewhere under the same contract, and the same silent
//! failure: `sb-<project-ref>-auth-token`, the session itself, is [`super::session`]'s, and
//! `coh-planner-favorites` is [`super::favorites`]'. The three storage primitives at the bottom of
//! this file are `pub(super)` for that last one — an inherited key is read and written the same
//! way whichever module owns what is in it, and a second copy of `read_key` would be a second
//! place to get the try/catch wrong.
//!
//! # Why a write never re-serialises a parsed view
//!
//! The obvious shape for the token map is `BTreeMap<String, String>`, and it is wrong in a way
//! that only shows up on real data. One unexpected value — a number, a null, an entry some future
//! version of either client wrote — fails the whole parse, and the beta's own reader answers a
//! failed parse with `{}` (`sharedBuilds.ts:203-209`). Ported literally, that means: one malformed
//! entry, the map reads as empty, the next write persists the empty map, and **every** token is
//! gone. The failure is one key wide and the blast radius is the whole store.
//!
//! So reads here are tolerant and writes are surgical. [`tokens_of`] keeps the entries it
//! understands without claiming the ones it does not, and [`with_token_set`]/[`with_token_removed`]
//! edit the parsed object IN PLACE and re-emit it, so an entry this code has no opinion about
//! survives a round trip it was never part of. Rule 1 says fail loud rather than fall back to a
//! default; storage this app does not own is the case where the loud failure would be a modal the
//! user cannot act on, so the rule it obeys instead is the one behind it — never destroy evidence.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// How an anonymous share stays editable. See the module doc before changing this string.
pub const OWNER_TOKENS_KEY: &str = "coh-planner-owner-tokens";

/// The one-click short-link cache. See the module doc before changing this string.
pub const QUICK_SHARE_KEY: &str = "coh-planner-quick-share";

// ============================================================
// Owner tokens — the pure half.
// ============================================================

/// Read the stored map as JSON, tolerating everything a foreign writer might have left.
///
/// Anything that is not a JSON object reads as no object at all, which is the beta's behaviour and
/// the only honest one: a string or an array under this key is not a token map with a problem, it
/// is not a token map.
fn parse_object(text: &str) -> Map<String, Value> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

/// The `id → token` pairs in a stored map, skipping entries whose value is not a string.
///
/// Skipping rather than failing: an entry this reader cannot use is one build that cannot be
/// edited, and failing the parse instead would make it every build.
pub fn tokens_of(text: &str) -> Vec<(String, String)> {
    parse_object(text)
        .into_iter()
        .filter_map(|(id, value)| match value {
            Value::String(token) => Some((id, token)),
            _ => None,
        })
        .collect()
}

/// The token for one build, or `None` when this browser does not hold one.
pub fn token_of(text: &str, build_id: &str) -> Option<String> {
    match parse_object(text).get(build_id) {
        Some(Value::String(token)) => Some(token.clone()),
        _ => None,
    }
}

/// The stored map with one token set, every other entry preserved verbatim.
pub fn with_token_set(text: &str, build_id: &str, token: &str) -> String {
    let mut map = parse_object(text);
    map.insert(build_id.to_string(), Value::String(token.to_string()));
    Value::Object(map).to_string()
}

/// The stored map with one token dropped, every other entry preserved verbatim.
pub fn with_token_removed(text: &str, build_id: &str) -> String {
    let mut map = parse_object(text);
    map.remove(build_id);
    Value::Object(map).to_string()
}

// ============================================================
// The quick-share cache — the pure half.
// ============================================================

/// What the short-link cache remembers: which row a build was last quick-shared into, and what
/// the build looked like at the time.
///
/// `template_version` rides along because the cached URL points at a row whose social image was
/// rendered under it — an unchanged build whose CARD template has moved on still needs a re-share
/// to refresh the image, so a version older than the current one is a miss even on an exact
/// fingerprint match. Field names are the beta's (`camelCase` on the wire), because this struct
/// reads what the beta wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickShare {
    pub share_id: String,
    pub fingerprint: String,
    /// Absent in cache entries written before the field existed; `None` is a miss, not a match.
    #[serde(default)]
    pub template_version: Option<u32>,
}

/// Read the cache, or `None` if it is absent, unparseable, or missing either required field —
/// exactly the three the beta's `readQuickShareCache` folds together, and for its reason: a cache
/// that cannot be trusted is a cache miss, which costs one network call and nothing else.
pub fn parse_quick_share(text: &str) -> Option<QuickShare> {
    let cache: QuickShare = serde_json::from_str(text).ok()?;
    (!cache.share_id.is_empty() && !cache.fingerprint.is_empty()).then_some(cache)
}

/// Whether a cached entry may be re-copied without a network call.
///
/// Both halves must agree: the build must be byte-identical to the one that was shared, AND the
/// card template must be the one the stored image was rendered under.
pub fn quick_share_hits(cache: &QuickShare, fingerprint: &str, template_version: u32) -> bool {
    cache.fingerprint == fingerprint && cache.template_version == Some(template_version)
}

// ============================================================
// The I/O half.
// ============================================================

/// Read a key, from the document the caller is in.
///
/// Reads stay local by [`crate::storage`]'s rule; only writes are routed there. Returns the empty
/// string for an absent key so every caller above can treat "nothing stored" and "stored nothing"
/// as one case.
pub(super) async fn read_key(key: &str) -> String {
    let Ok(key) = serde_json::to_string(key) else {
        return String::new();
    };
    let js = format!("try {{ return localStorage.getItem({key}); }} catch (_) {{ return null; }}");
    let Ok(value) = dioxus::document::eval(&js).await else {
        return String::new();
    };
    value.as_str().unwrap_or_default().to_string()
}

/// Write a key, through the window that owns persistence ([`crate::storage`]).
///
/// The payload reaches JS as a JSON string literal rather than through `{:?}`: Rust's debug
/// escaping is close enough to JS's to be tempting and is not the same language, and what travels
/// here is a build id and a server-minted token — text from outside this program.
pub(super) fn write_key(key: &str, json: &str) {
    let (Ok(key), Ok(json)) = (serde_json::to_string(key), serde_json::to_string(json)) else {
        return;
    };
    crate::storage::commit(format!(
        "try {{ localStorage.setItem({key}, {json}); }} catch (_) {{}}"
    ));
}

pub(super) fn clear_key(key: &str) {
    let Ok(key) = serde_json::to_string(key) else {
        return;
    };
    crate::storage::commit(format!(
        "try {{ localStorage.removeItem({key}); }} catch (_) {{}}"
    ));
}

/// The owner token this browser holds for a build, or `None`.
pub async fn owner_token(build_id: &str) -> Option<String> {
    token_of(&read_key(OWNER_TOKENS_KEY).await, build_id)
}

/// Every `id → token` pair this browser holds. What `claim-builds` is handed.
pub async fn owner_tokens() -> Vec<(String, String)> {
    tokens_of(&read_key(OWNER_TOKENS_KEY).await)
}

/// Remember the token a fresh share minted. Read-modify-write, so a concurrent entry survives.
pub async fn remember_owner_token(build_id: &str, token: &str) {
    let next = with_token_set(&read_key(OWNER_TOKENS_KEY).await, build_id, token);
    write_key(OWNER_TOKENS_KEY, &next);
}

/// Forget a build's token, after the row it opened is gone.
pub async fn forget_owner_token(build_id: &str) {
    let next = with_token_removed(&read_key(OWNER_TOKENS_KEY).await, build_id);
    write_key(OWNER_TOKENS_KEY, &next);
}

pub async fn quick_share() -> Option<QuickShare> {
    parse_quick_share(&read_key(QUICK_SHARE_KEY).await)
}

pub fn remember_quick_share(cache: &QuickShare) {
    let Ok(json) = serde_json::to_string(cache) else {
        return;
    };
    write_key(QUICK_SHARE_KEY, &json);
}

/// Drop the short-link cache. Called on sign-out, so the next user on this browser does not
/// re-copy — or silently update — the previous user's unlisted build.
pub fn clear_quick_share() {
    clear_key(QUICK_SHARE_KEY);
}

// ============================================================
// The last visibility picked in the save surface.
// ============================================================

/// This app's own key, not one inherited from the beta, so its shape is free to change.
pub const SAVE_VISIBILITY_KEY: &str = "sidekick-save-visibility";

/// Anything unreadable is "nothing remembered", and the surface falls back to its default.
pub fn parse_save_visibility(text: &str) -> Option<super::shared_builds::BuildVisibility> {
    serde_json::from_str(text).ok()
}

pub async fn save_visibility() -> Option<super::shared_builds::BuildVisibility> {
    parse_save_visibility(&read_key(SAVE_VISIBILITY_KEY).await)
}

pub fn remember_save_visibility(visibility: super::shared_builds::BuildVisibility) {
    let Ok(json) = serde_json::to_string(&visibility) else {
        return;
    };
    write_key(SAVE_VISIBILITY_KEY, &json);
}
