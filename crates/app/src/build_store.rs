//! Build persistence — the DOM half (M3 execution plan step 10, D6). The pure envelope
//! codec (versioning, `select_active`/`compose`) lives in [`coh_data::build_store`]; this
//! module is only the `localStorage` get/set, on the same document JS channel `theme.rs`
//! and `layout_store.rs` use (identical in the desktop webview and the browser).
//!
//! **Boot order (the ported constraint — beta ARCHITECTURE.md Boot Sequence).** The dataset
//! MUST load before the build is restored: the calc resolves a build's power/enhancement
//! definitions from the loaded dataset, and the web target fetches the dataset async, so
//! restoring first would calculate against absent data. The shell reads the envelope once
//! via [`load`], aims the initial dataset at its `active_dataset` (the beta `bootServerId`
//! pre-peek), and defers the actual build restore until the `db` resource is `Some(Ok(_))`.

use coh_data::{build_store, StoredBuilds};
use dioxus::prelude::*;

const KEY_BUILD: &str = "sk-build";

/// Persist the whole per-dataset envelope (fire-and-forget, like `theme::apply_theme`).
/// The envelope — not just the active build — so a dataset switch stays non-destructive:
/// the caller `compose`s the active build into the envelope, then persists the result.
pub fn persist(builds: &StoredBuilds) {
    let Ok(json) = build_store::encode(builds) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_BUILD:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted envelope if present and well-formed; `None` (→ an empty workspace) for
/// missing or corrupt state. Run this only after the active dataset has loaded (see the
/// module boot-order note).
pub async fn load() -> Option<StoredBuilds> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_BUILD:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?.to_string();
    build_store::decode(&text)
}
