//! Level-up mode persistence — whether the planner is currently walking a character up
//! through its levels, over the same `localStorage` channel as [`crate::picker_store`] and
//! [`crate::layout_store`].
//!
//! It persists because the mode is a session someone is in the middle of, not a preference
//! they set once: a reload that silently dropped back to respec mode would re-open every gate
//! the mode had closed, mid-walk. The beta keeps it in `useUIStore`'s partialize for the same
//! reason.
//!
//! It is stored HERE and not in the build because it changes nothing about the character —
//! only which edits the UI will accept. A build carries no record of how it was planned, so
//! the flag is neither undoable nor exported, and a build made in level-up mode is
//! indistinguishable from the same build made in one sitting (which is what the game would
//! say about it too).

use dioxus::prelude::*;

const KEY_LEVEL_UP: &str = "sk-level-up-mode";

/// Persist the mode (fire-and-forget, like [`crate::picker_store::persist`]).
pub fn persist(enabled: bool) {
    let js = format!(
        "try {{ localStorage.setItem({KEY_LEVEL_UP:?}, {}); }} catch (_) {{}}",
        if enabled { "\"true\"" } else { "\"false\"" }
    );
    crate::storage::commit(js);
}

/// The persisted mode; `None` (missing, or anything we didn't write) keeps the default —
/// off, so a fresh planner opens in the respec flow the whole app is otherwise built around.
pub async fn load() -> Option<bool> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_LEVEL_UP:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
