//! Picker-defaults persistence — the crafting level, attunement, and booster the
//! enhancement picker stamps onto pieces ([`crate::picker_defaults::PickerDefaults`]),
//! kept across reloads on the same `localStorage` document channel as
//! [`crate::layout_store`] and [`crate::build_store`]. The beta persists these as
//! `useUIStore` globals so the user's last slotting choice sticks; this is the DOM
//! half. Missing or corrupt state keeps the fresh defaults (validated on load).

use crate::picker_defaults::BOOST_MAX;
use crate::picker_memory::PickerMemory;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

const KEY_PICKER: &str = "sk-picker-defaults";

/// The persisted picker slotting defaults (the beta `globalIOLevel` /
/// `attunementEnabled` / `globalBoostLevel`), plus where the picker was left per power.
#[derive(Serialize, Deserialize)]
struct StoredDefaults {
    io_level: u8,
    attuned: bool,
    boost: u8,
    /// Where each power's picker was last navigated to. `#[serde(default)]` so state written
    /// before this field existed restores its levels and simply has no places yet — an absent
    /// memory is an empty one, which is the same thing a new install has.
    #[serde(default)]
    memory: PickerMemory,
}

/// Persist the current picker defaults (fire-and-forget, like [`crate::build_store::persist`]).
pub fn persist(io_level: u8, attuned: bool, boost: u8, memory: PickerMemory) {
    let stored = StoredDefaults {
        io_level,
        attuned,
        boost,
        memory,
    };
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_PICKER:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The persisted `(io_level, attuned, boost)` if present and in range; `None`
/// (→ keep the fresh defaults) for missing or corrupt state. An out-of-band level
/// or booster is treated as corrupt rather than silently clamped — a stored value
/// outside the valid band is not a value we wrote (Rule 1: reject, don't recover).
///
/// The CRAFT level is not checked here and cannot be: the band it must sit in is the dataset's
/// own boost roster, which this restore runs before the database is loaded. The caller clamps
/// it against [`crate::picker_defaults::CraftBand`] once there is one — which is also what
/// folds in a level persisted from before that band was read off the export (BOOST-6).
pub async fn load() -> Option<(u8, bool, u8, PickerMemory)> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_PICKER:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?;
    let stored: StoredDefaults = serde_json::from_str(text).ok()?;
    if stored.boost > BOOST_MAX {
        return None;
    }
    Some((stored.io_level, stored.attuned, stored.boost, stored.memory))
}
