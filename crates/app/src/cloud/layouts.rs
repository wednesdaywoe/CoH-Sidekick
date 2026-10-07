//! The account's copy of the layout — one `user_layouts` row per user (docs/LAYOUT-SYNC-PLAN.md).
//!
//! The table is in the beta's `supabase/schema.sql`: RLS lets a signed-in user read, insert and
//! update their own row and nothing else, and anon has no grant. Written straight through
//! PostgREST, as [`super::favorites`] is.
//!
//! `edited_at` is when the layout was last edited on whichever device edited it, carried in
//! milliseconds on this side ([`crate::layout_sync`]) and as a timestamp on the server's.

use super::{Cloud, CloudError};
use crate::layout_sync::LayoutDoc;
use serde::{Deserialize, Serialize};

const TABLE: &str = "user_layouts";

#[derive(Serialize)]
struct RowOut<'a> {
    user_id: &'a str,
    layout: &'a LayoutDoc,
    edited_at: String,
}

#[derive(Deserialize)]
struct RowIn {
    layout: LayoutDoc,
    edited_at: String,
}

/// The account's layout and when it was edited, or `None` if the account has none yet.
pub async fn fetch(cloud: &Cloud, user_id: &str) -> Result<Option<(LayoutDoc, i64)>, CloudError> {
    let filter = format!("eq.{user_id}");
    let rows: Vec<RowIn> = cloud
        .select(TABLE, "layout,edited_at", &[("user_id", &filter)], None)
        .await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let edited_ms = chrono::DateTime::parse_from_rfc3339(&row.edited_at)
        .map_err(|e| CloudError::Decode {
            detail: format!("user_layouts.edited_at is not a timestamp: {e}"),
            body: row.edited_at.clone(),
        })?
        .timestamp_millis();
    Ok(Some((row.layout, edited_ms)))
}

/// Replace the account's layout with this one, edited at `edited_ms`.
pub async fn push(
    cloud: &Cloud,
    user_id: &str,
    layout: &LayoutDoc,
    edited_ms: i64,
) -> Result<(), CloudError> {
    let edited_at = chrono::DateTime::from_timestamp_millis(edited_ms)
        .unwrap_or_default()
        .to_rfc3339();
    let row = RowOut {
        user_id,
        layout,
        edited_at,
    };
    cloud.upsert(TABLE, &[row]).await
}
