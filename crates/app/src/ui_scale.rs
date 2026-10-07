//! UI scale — app chrome, nothing the calc reads. Kept across reloads on the same
//! `localStorage` channel as [`crate::alert_store`] and [`crate::picker_store`]: a preference
//! about the reader, not the build, so it survives switching builds and datasets.
//!
//! The app's own CSS is overwhelmingly px-based (one lone `rem` in the whole sheet), so the
//! usual "scale the root font-size and let `rem` cascade" trick doesn't reach anything here.
//! [`apply`] instead sets `zoom` on `<html>`: unlike `transform: scale()`, `zoom` participates
//! in layout, so the free grid's absolutely-positioned panels ([`crate::grid::view`]) stay
//! visually consistent rather than being stretched over unscaled space.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// Percent, clamped [`MIN_PCT`]..=[`MAX_PCT`] in steps of [`STEP_PCT`]. `100` is the unscaled
/// baseline and the default for a pre-field-era browser (missing or corrupt stored state).
pub const DEFAULT_PCT: u32 = 100;
pub const MIN_PCT: u32 = 80;
pub const MAX_PCT: u32 = 150;
pub const STEP_PCT: u32 = 10;

const KEY_UI_SCALE: &str = "sk-ui-scale";

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredUiScale {
    #[serde(rename = "1")]
    V1 { pct: u32 },
}

/// Set `zoom` on the document root. Fire-and-forget, like [`crate::theme::apply_theme`]; safe
/// to call before a persisted value has loaded — it just repaints once more when the real value
/// arrives, the same accepted flash [`crate::shell::RuleOfFiveAlert`]'s default takes.
pub fn apply(pct: u32) {
    let js = format!("document.documentElement.style.zoom = '{pct}%';");
    document::eval(&js);
}

/// Persist the setting (fire-and-forget, like [`crate::picker_store::persist`]).
pub fn persist(pct: u32) {
    let stored = StoredUiScale::V1 { pct };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_UI_SCALE:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted setting, clamped to the representable range, or `None` (→ keep
/// [`DEFAULT_PCT`]) for missing, corrupt, or out-of-range state.
pub async fn load() -> Option<u32> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_UI_SCALE:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?;
    let StoredUiScale::V1 { pct } = serde_json::from_str(text).ok()?;
    (MIN_PCT..=MAX_PCT).contains(&pct).then_some(pct)
}
