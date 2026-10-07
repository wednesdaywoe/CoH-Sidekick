//! Whether slot levels are drawn under the slots, kept across reloads on the same `localStorage`
//! channel as [`crate::level_up_store`] (the beta's `useUIStore.showSlotLevels`).
//!
//! A preference, not build state: the levels are tracked on every build either way (see
//! [`coh_data::slot_levels`]); this only decides whether the powers panel shows them. Missing or
//! corrupt state keeps them ON, the beta's default.

use dioxus::prelude::*;

const KEY_SHOW_SLOT_LEVELS: &str = "sk-show-slot-levels";

/// Persist the setting (fire-and-forget, like [`crate::level_up_store::persist`]).
pub fn persist(shown: bool) {
    let js = format!(
        "try {{ localStorage.setItem({KEY_SHOW_SLOT_LEVELS:?}, {}); }} catch (_) {{}}",
        if shown { "\"true\"" } else { "\"false\"" }
    );
    crate::storage::commit(js);
}

/// The persisted setting; `None` (missing, or anything we didn't write) keeps the default.
pub async fn load() -> Option<bool> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_SHOW_SLOT_LEVELS:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
