//! Favourites — the local list, the account mirror, and the union that reconciles them (RB4g).
//!
//! The only surface in this client with real merge semantics, and the three rules behind it are
//! the beta's (`sharedBuilds.ts:64-198`):
//!
//! 1. **`localStorage` under `coh-planner-favorites` is the truth the UI draws from.** It is
//!    always available, so the star works signed out and flips without waiting on a network.
//! 2. **A signed-in change is *also* mirrored to the `favorites` table, fire-and-forget.** The
//!    local list is already correct when the write leaves, so a failure costs nothing the user
//!    can see — and [`sync`] reconciles on the next login.
//! 3. **[`sync`] is a UNION, not a replace.** Server rows come down, local-only rows go up, and
//!    the cache becomes both. That is what makes the first login after favouriting-while-signed-
//!    out just work, and what carries a favourite from one device to another.
//!
//! The pushes go **one row per request** ([`upsert_one`], called in a loop) rather than as one
//! batch. A favourite can point at a build that has since been deleted, and that row fails its
//! own foreign key; batched, one such row would abort the whole push and every other local-only
//! favourite would be lost with it. The beta makes the same trade with `Promise.allSettled`
//! (`sharedBuilds.ts:158-171`); the only difference here is that the requests are sequential,
//! which costs latency on a first login and nothing else.
//!
//! # What a union cannot express, and why it is ported anyway
//!
//! A union has no tombstones, so **un-favouriting is the one edit [`sync`] can undo.** Clear a
//! star while signed in and the delete fails, or clear it on one device while another still
//! holds it locally, and the next [`sync`] on the device that still has the row pushes it back
//! up — the build returns. This is inherited, not introduced: the beta's merge has the same hole
//! and the row that specifies this one names the union by name. Recording it here rather than
//! quietly adding a deletion log, because a tombstone table is a schema change and a schema
//! change is not this row's to make.
//!
//! # The storage key is a compatibility contract
//!
//! `coh-planner-favorites` is one more of the beta's keys this build inherits on the same
//! origin, under [`super::owner_store`]'s contract and read through its storage primitives.
//! Unlike the owner tokens it is not the only copy of anything — a signed-in user's favourites
//! also live on their account — but a signed-OUT user's favourites are local and nowhere else,
//! so a write that drops an entry it did not understand drops it for good. Reads are therefore
//! tolerant and writes surgical, exactly as over there: [`with_id_added`] and friends edit the
//! stored ARRAY in place and re-emit it, so an element this code has no opinion about survives a
//! round trip it was never part of.

use super::owner_store::{clear_key, read_key, write_key};
use super::shared_builds::{self, SharedBuildSummary};
use super::{Cloud, CloudError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The local favourites list. See the module doc before changing this string.
pub const FAVORITES_KEY: &str = "coh-planner-favorites";

/// The table the mirror writes — the only one this client WRITES through PostgREST rather than
/// through an edge function, because it is the only one the beta writes that way too.
const FAVORITES_TABLE: &str = "favorites";

// ============================================================
// The stored list — the pure half.
// ============================================================

/// Read the stored value as a JSON array, tolerating everything a foreign writer might have left.
///
/// Anything that is not an array reads as no array at all: an object or a string under this key
/// is not a favourites list with a problem, it is not a favourites list.
fn parse_array(text: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| match value {
            Value::Array(items) => Some(items),
            _ => None,
        })
        .unwrap_or_default()
}

/// The build ids in a stored list, skipping elements that are not strings.
///
/// Skipping rather than failing, for [`super::owner_store::tokens_of`]'s reason one size down: an
/// element this reader cannot use is one favourite that does not draw, and failing the parse
/// instead would make it all of them.
pub fn ids_of(text: &str) -> Vec<String> {
    parse_array(text)
        .into_iter()
        .filter_map(|value| match value {
            Value::String(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// Whether a build is in a stored list.
pub fn contains_id(text: &str, build_id: &str) -> bool {
    parse_array(text)
        .iter()
        .any(|value| value.as_str() == Some(build_id))
}

/// The stored list with one id appended, every other element preserved verbatim. Appending
/// rather than prepending is the beta's `ids.push` — the order is the order they were starred in.
pub fn with_id_added(text: &str, build_id: &str) -> String {
    let mut items = parse_array(text);
    if !items.iter().any(|value| value.as_str() == Some(build_id)) {
        items.push(Value::String(build_id.to_string()));
    }
    Value::Array(items).to_string()
}

/// The stored list with one id dropped, every other element preserved verbatim.
pub fn with_id_removed(text: &str, build_id: &str) -> String {
    let mut items = parse_array(text);
    items.retain(|value| value.as_str() != Some(build_id));
    Value::Array(items).to_string()
}

/// The stored list unioned with what the server holds — local order first, then the server rows
/// this browser did not have, duplicates collapsed.
///
/// `Array.from(new Set([...localIds, ...serverIds]))`, restated: a `Set` keeps first-seen order
/// and collapses repeats, and it collapses them across the WHOLE sequence, so a local list that
/// somehow held one id twice comes out holding it once. Elements that are not strings ride along
/// in place, because the beta's spread carries them too and this write is not the place to decide
/// they are worthless.
pub fn with_server_ids_merged(text: &str, server_ids: &[String]) -> String {
    let mut items = parse_array(text);
    let mut merged: Vec<Value> = Vec::with_capacity(items.len() + server_ids.len());
    for item in items.drain(..) {
        if !merged.contains(&item) {
            merged.push(item);
        }
    }
    for id in server_ids {
        let value = Value::String(id.clone());
        if !merged.contains(&value) {
            merged.push(value);
        }
    }
    Value::Array(merged).to_string()
}

/// PostgREST's `in.(…)` list, encoded the way `postgrest-js` encodes `.in(column, values)`
/// (`postgrest-js/dist/index.cjs:618-624`): duplicates collapsed by a `Set`, and any value
/// holding one of `,` `(` `)` wrapped in double quotes so the list still parses as a list.
///
/// A build id has never held one of those characters — they are nanoids — but the encoder is
/// copied rather than simplified, because "the ids are safe" is a fact about today's id format
/// and this is the place that would silently mis-slice the query when it changes.
pub fn in_filter(ids: &[String]) -> String {
    let mut seen: Vec<&str> = Vec::with_capacity(ids.len());
    let mut out = String::from("in.(");
    for id in ids {
        if seen.contains(&id.as_str()) {
            continue;
        }
        if !seen.is_empty() {
            out.push(',');
        }
        seen.push(id.as_str());
        if id.contains([',', '(', ')']) {
            out.push('"');
            out.push_str(id);
            out.push('"');
        } else {
            out.push_str(id);
        }
    }
    out.push(')');
    out
}

// ============================================================
// The stored list — the I/O half.
// ============================================================

/// Every build id this browser has starred.
pub async fn ids() -> Vec<String> {
    ids_of(&read_key(FAVORITES_KEY).await)
}

/// Whether this browser has starred a build.
pub async fn is_favorite(build_id: &str) -> bool {
    contains_id(&read_key(FAVORITES_KEY).await, build_id)
}

/// Flip one build's star and answer with the state it is now in.
///
/// The local write is the whole of what the caller waits on — the account mirror is the caller's
/// next step and deliberately not this function's, so the surface that has an [`Account`] does
/// the signing-in part and the surface that does not still gets a working star.
///
/// [`Account`]: super::account::Account
pub async fn toggle(build_id: &str) -> bool {
    let stored = read_key(FAVORITES_KEY).await;
    let now_favorite = !contains_id(&stored, build_id);
    let next = if now_favorite {
        with_id_added(&stored, build_id)
    } else {
        with_id_removed(&stored, build_id)
    };
    write_key(FAVORITES_KEY, &next);
    now_favorite
}

/// Drop the local list. Called on sign-out, so the next user on this browser does not inherit the
/// previous one's favourites — or push them up to their own account on their first [`sync`].
///
/// Safe because a signed-in user's favourites live on their account and come back down on their
/// next login. The owner TOKEN map deliberately survives the same sign-out
/// ([`super::account::Account::sign_out`]): those belong to the browser, these belong to whoever
/// was signed in.
pub fn clear_cache() {
    clear_key(FAVORITES_KEY);
}

// ============================================================
// The account mirror.
// ============================================================

/// One row of the `favorites` table, as this client reads it back.
#[derive(Debug, Deserialize)]
struct FavoriteRow {
    build_id: String,
}

/// One row of the `favorites` table, as this client writes it. The column names are the table's.
#[derive(Debug, Serialize)]
struct FavoriteWrite<'a> {
    user_id: &'a str,
    build_id: &'a str,
}

/// Every build id the signed-in user has favourited on their account.
pub async fn server_ids(cloud: &Cloud, user_id: &str) -> Result<Vec<String>, CloudError> {
    let filter = format!("eq.{user_id}");
    let rows: Vec<FavoriteRow> = cloud
        .select(
            FAVORITES_TABLE,
            "build_id",
            &[("user_id", filter.as_str())],
            None,
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.build_id).collect())
}

/// Add one row. An upsert rather than an insert because the row may already be there — a second
/// device that starred the same build got there first, and re-starring is not an error.
async fn upsert_one(cloud: &Cloud, user_id: &str, build_id: &str) -> Result<(), CloudError> {
    let row = FavoriteWrite { user_id, build_id };
    cloud.upsert(FAVORITES_TABLE, &[row]).await
}

/// Drop one row. Both filters are sent because the primary key is both columns, and because
/// [`Cloud::delete`] refuses an unfiltered delete on a table every user shares.
async fn delete_one(cloud: &Cloud, user_id: &str, build_id: &str) -> Result<(), CloudError> {
    let user_filter = format!("eq.{user_id}");
    let build_filter = format!("eq.{build_id}");
    cloud
        .delete(
            FAVORITES_TABLE,
            &[
                ("user_id", user_filter.as_str()),
                ("build_id", build_filter.as_str()),
            ],
        )
        .await
}

/// Write one favourite change through to the account.
///
/// Returns the failure rather than swallowing it, so the call site owns how loud it is; the star
/// discards it on purpose (see [`toggle`] and the module doc), because the local list is already
/// right and [`sync`] is what reconciles.
pub async fn mirror(
    cloud: &Cloud,
    user_id: &str,
    build_id: &str,
    favorited: bool,
) -> Result<(), CloudError> {
    if favorited {
        upsert_one(cloud, user_id, build_id).await
    } else {
        delete_one(cloud, user_id, build_id).await
    }
}

/// Reconcile the local list with the signed-in user's account favourites — the union in the
/// module doc, run once when auth resolves to a user.
///
/// Best-effort in one specific direction: **a read that failed leaves the local list untouched.**
/// The beta returns early on `error` for that reason (`sharedBuilds.ts:154`) and it is the half
/// worth keeping — merging against an empty server answer this client could not actually verify
/// would be indistinguishable from an account that really has no favourites, and the cache would
/// come out unchanged anyway. A push that fails is per-row and costs that row (see the module
/// doc); the pull and the cache write are what this returns on.
pub async fn sync(cloud: &Cloud, user_id: &str) -> Result<(), CloudError> {
    let stored = read_key(FAVORITES_KEY).await;
    let local = ids_of(&stored);
    let server = server_ids(cloud, user_id).await?;

    for build_id in local.iter().filter(|id| !server.contains(id)) {
        // One row per request, and a refusal here is that row's. A favourite pointing at a
        // since-deleted build fails its own foreign key; the others still go up.
        let _ = upsert_one(cloud, user_id, build_id).await;
    }

    write_key(FAVORITES_KEY, &with_server_ids_merged(&stored, &server));
    Ok(())
}

/// The shared-build rows behind this browser's favourites, newest-updated first.
///
/// No ids means no request: an empty `in.()` is not a query for nothing, it is a malformed
/// filter, and the beta short-circuits for the same reason (`sharedBuilds.ts:188`).
///
/// A favourite whose build has been deleted, or which the view's RLS will not hand this viewer
/// (it grants the public rows — see [`super::shared_builds`]'s module doc), simply does not come
/// back, and that is the honest answer rather than an error: the local list is a list of ids, and
/// an id is not a promise that the row is still there to be read.
pub async fn favorite_builds(
    cloud: &Cloud,
    ids: &[String],
) -> Result<Vec<SharedBuildSummary>, CloudError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let filter = in_filter(ids);
    // Through the module that owns the projection, not through a `select` of its own: the
    // columns a list read asks for are named in exactly two places and this is neither (F19).
    shared_builds::list_rows(cloud, &[("id", filter.as_str())], Some("updated_at.desc")).await
}
