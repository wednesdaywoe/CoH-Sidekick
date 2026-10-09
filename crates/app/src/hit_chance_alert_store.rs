//! Whether the Hit Chance Alert is on, kept across reloads on the same `localStorage` channel as
//! [`crate::slot_level_store`] (the beta's `useUIStore.hitChanceAlertEnabled`). A preference, not
//! build state. Missing or corrupt state keeps it OFF, the beta's default.

use dioxus::prelude::*;

const KEY_HIT_CHANCE_ALERT: &str = "sk-hit-chance-alert";

/// Persist the setting (fire-and-forget, like [`crate::slot_level_store::persist`]).
pub fn persist(enabled: bool) {
    let js = format!(
        "try {{ localStorage.setItem({KEY_HIT_CHANCE_ALERT:?}, {}); }} catch (_) {{}}",
        if enabled { "\"true\"" } else { "\"false\"" }
    );
    crate::storage::commit(js);
}

/// The persisted setting; `None` (missing, or anything we didn't write) keeps the default.
pub async fn load() -> Option<bool> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_HIT_CHANCE_ALERT:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
