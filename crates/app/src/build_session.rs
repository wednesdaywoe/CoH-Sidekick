//! The live build-editing session: the working [`CharacterState`], its per-dataset
//! persistence envelope, and the undo/redo [`History`], bundled as a Dioxus context so any
//! panel can edit the build without the grid threading three signals through every layer.
//!
//! Every edit goes through [`BuildSession::commit`], which is the one place that records an
//! undo checkpoint and re-persists — so "mutate the build" and "the change is undoable and
//! saved" are the same act (design-notes: one source of truth by construction). Undo/redo
//! restore a snapshot and re-persist without recording a new checkpoint (the `is_restoring`
//! guard, beta `historyStore` `_isRestoring`).

use crate::build_store;
use crate::history::History;
use coh_data::{build_store as envelope, CharacterState, StoredBuilds};
use dioxus::prelude::*;

/// The shared build-editing state. `Copy` (all fields are `Signal`, itself `Copy`), so it
/// is cheap to pull from context and pass by value. `PartialEq` (signals compare by identity)
/// so it can be a memoized component prop.
#[derive(Clone, Copy, PartialEq)]
pub struct BuildSession {
    /// The build currently being edited (the active dataset's working build).
    pub build: Signal<CharacterState>,
    /// The per-dataset persistence envelope — the active build plus every other dataset's
    /// carried-over build, so a dataset switch stays non-destructive.
    pub stored: Signal<StoredBuilds>,
    /// Undo/redo snapshots for this session (in-memory; resets on reload).
    pub history: Signal<History>,
}

impl BuildSession {
    /// Apply `mutate` as one undoable, persisted edit: checkpoint the pre-edit state (unless
    /// we are mid-restore), run the mutation, then fold the build into the envelope and
    /// persist it.
    pub fn commit(mut self, mutate: impl FnOnce(&mut CharacterState)) {
        if !self.history.peek().is_restoring() {
            let snapshot = self.build.peek().clone();
            self.history.write().checkpoint(&snapshot);
        }
        self.build.with_mut(mutate);
        self.persist();
    }

    /// Undo the last edit, if any. Restores the previous snapshot and re-persists.
    pub fn undo(mut self) {
        let current = self.build.peek().clone();
        // Bind so the history write-lock drops before `apply_restored` writes it again.
        let restored = self.history.write().undo(&current);
        if let Some(restored) = restored {
            self.apply_restored(restored);
        }
    }

    /// Redo the last undone edit, if any.
    pub fn redo(mut self) {
        let current = self.build.peek().clone();
        let restored = self.history.write().redo(&current);
        if let Some(restored) = restored {
            self.apply_restored(restored);
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.read().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.read().can_redo()
    }

    /// Set the working build for a freshly loaded dataset and clear its history — the
    /// snapshots of the previous dataset's build don't apply to this one. Does NOT persist
    /// (the envelope already holds this build; loading is not an edit).
    pub fn load(mut self, build: CharacterState) {
        self.build.set(build);
        self.history.write().clear();
    }

    /// Fold the current build into the envelope and write it to `localStorage`. The one
    /// side-effecting sink every edit funnels through.
    fn persist(mut self) {
        let next = envelope::compose(&self.build.peek(), &self.stored.peek());
        self.stored.set(next.clone());
        build_store::persist(&next);
    }

    /// Apply a restored snapshot without recording it as a new checkpoint.
    fn apply_restored(mut self, restored: CharacterState) {
        self.history.write().set_restoring(true);
        self.build.set(restored);
        self.history.write().set_restoring(false);
        self.persist();
    }
}
