//! Which damage reading the Info card's hero line states (the beta's `damageDisplayMode`), and
//! whether slotted damage procs count toward it (the beta's `includeProcDamageInDPS`), kept
//! across reloads on the same `localStorage` channel as [`crate::ui_scale`]. Preferences about
//! the reader, not the build: they move no dashboard total, so they survive switching builds and
//! datasets.

use crate::view::power_view::DamageMetric;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

const KEY_DAMAGE_METRIC: &str = "sk-damage-metric";
const KEY_PROC_DAMAGE: &str = "sk-include-proc-damage";

/// The chosen reading, provided at the shell root. A surface rendered outside the shell (a
/// preview with no provider) falls back to [`DamageMetric::Damage`] rather than failing.
#[derive(Clone, Copy, PartialEq)]
pub struct DamageMetricPref(pub Signal<DamageMetric>);

/// Whether a power's damage reading, damage bar and attack-chain damage carry its slotted damage
/// procs' average. On by default, as in the beta.
#[derive(Clone, Copy, PartialEq)]
pub struct ProcDamagePref(pub Signal<bool>);

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredDamageMetric {
    #[serde(rename = "1")]
    V1 { metric: DamageMetric },
}

/// Persist the setting (fire-and-forget, like [`crate::ui_scale::persist`]).
pub fn persist(metric: DamageMetric) {
    let stored = StoredDamageMetric::V1 { metric };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js =
        format!("try {{ localStorage.setItem({KEY_DAMAGE_METRIC:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted setting, or `None` (→ keep the default) for missing or corrupt state.
pub async fn load() -> Option<DamageMetric> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_DAMAGE_METRIC:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?;
    let StoredDamageMetric::V1 { metric } = serde_json::from_str(text).ok()?;
    Some(metric)
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredProcDamage {
    #[serde(rename = "1")]
    V1 { include: bool },
}

/// Persist the proc-damage switch (fire-and-forget).
pub fn persist_proc_damage(include: bool) {
    let stored = StoredProcDamage::V1 { include };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js =
        format!("try {{ localStorage.setItem({KEY_PROC_DAMAGE:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted proc-damage switch, or `None` (→ keep the default) for missing or corrupt state.
pub async fn load_proc_damage() -> Option<bool> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_PROC_DAMAGE:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?;
    let StoredProcDamage::V1 { include } = serde_json::from_str(text).ok()?;
    Some(include)
}
