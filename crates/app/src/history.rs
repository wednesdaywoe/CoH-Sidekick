//! Undo/redo for build edits (M3 execution plan step 11) — a direct port of the beta
//! `historyStore.ts` (69 lines). No diffing: two stacks of whole-[`CharacterState`]
//! snapshots. In-memory only, like the beta — history resets on reload, it is not
//! persisted (the persisted thing is the build itself, [`crate::build_store`]).
//!
//! Snapshots are cheap because [`CharacterState`] stores identity, not resolved defs (the
//! identity/definition split — see `coh_data::character`), so a `clone` copies name
//! strings and slot vectors, never the heavyweight power/atom definitions.

use coh_data::CharacterState;

/// Cap on retained undo snapshots (beta `HISTORY_LIMIT`). At the cap the oldest is dropped.
pub const HISTORY_LIMIT: usize = 50;

/// The undo/redo stacks plus the re-entry guard. `past` holds states BEFORE past edits
/// (newest last); `future` holds states undone-past (newest last). `Default` = empty
/// history, nothing to undo or redo.
#[derive(Debug, Default, Clone)]
pub struct History {
    past: Vec<CharacterState>,
    future: Vec<CharacterState>,
    /// Set while an undo/redo is applying its restored state, so the mutation wrapper that
    /// normally checkpoints skips it — restoring a snapshot must not itself record one
    /// (beta `_isRestoring`).
    is_restoring: bool,
}

impl History {
    /// Snapshot `current` before a mutation and clear the redo stack (a new edit forks the
    /// timeline). At [`HISTORY_LIMIT`] the oldest snapshot is dropped so the stack never
    /// grows past the cap.
    pub fn checkpoint(&mut self, current: &CharacterState) {
        if self.past.len() >= HISTORY_LIMIT {
            let overflow = self.past.len() - (HISTORY_LIMIT - 1);
            self.past.drain(0..overflow);
        }
        self.past.push(current.clone());
        self.future.clear();
    }

    /// Undo: return the most recent past snapshot to restore, pushing `current` onto the
    /// redo stack. `None` when there is nothing to undo (the caller leaves the build as-is).
    pub fn undo(&mut self, current: &CharacterState) -> Option<CharacterState> {
        let restored = self.past.pop()?;
        self.future.push(current.clone());
        Some(restored)
    }

    /// Redo: return the most recently undone snapshot to restore, pushing `current` back
    /// onto the undo stack. `None` when there is nothing to redo.
    pub fn redo(&mut self, current: &CharacterState) -> Option<CharacterState> {
        let restored = self.future.pop()?;
        self.past.push(current.clone());
        Some(restored)
    }

    /// Drop all history (e.g. on load of a different build).
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn is_restoring(&self) -> bool {
        self.is_restoring
    }

    pub fn set_restoring(&mut self, value: bool) {
        self.is_restoring = value;
    }
}
