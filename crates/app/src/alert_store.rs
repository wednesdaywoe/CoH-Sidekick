//! Whether the Rule-of-5 alert is wanted, kept across reloads on the same `localStorage`
//! document channel as [`crate::stats_store`] and [`crate::picker_store`] (the beta's
//! `useUIStore.ruleOf5AlertEnabled`).
//!
//! A preference, not build state: whether you want to be told about wasted bonuses is a property
//! of you, not of the character, so it survives switching builds and datasets exactly as the
//! stat config does. Missing or corrupt state keeps the alert ON — the default has to be the one
//! that reports, since the failure this alert exists to catch is a build quietly paying for
//! nothing.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

const KEY_ALERTS: &str = "sk-alerts";

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredAlerts {
    #[serde(rename = "1")]
    V1 { rule_of_five: bool },
}

/// Persist the setting (fire-and-forget, like [`crate::picker_store::persist`]).
pub fn persist(rule_of_five: bool) {
    let stored = StoredAlerts::V1 { rule_of_five };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_ALERTS:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted setting, or `None` (→ keep the default) for missing or corrupt state.
pub async fn load() -> Option<bool> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_ALERTS:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?;
    let StoredAlerts::V1 { rule_of_five } = serde_json::from_str(text).ok()?;
    Some(rule_of_five)
}
