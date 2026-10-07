//! Keeping the powers the game grants by itself in step with the build's picks.
//!
//! A lone grant follows from the picks the build holds and the level it has reached
//! ([`coh_data::granted_powers`]), so it is reconciled at every moment those facts change: a
//! power picked or removed, a pool or powerset added, dropped or replaced, the archetype set,
//! the level moved ([`crate::level_control`]), and a build loaded (which is also how a build
//! saved before grants were materialized gains them, and how a dataset switch resolves the
//! same picks against a different fork's gates). Each call sits inside
//! the same [`BuildSession::commit`](crate::build_session::BuildSession::commit) closure as
//! the edit that flips the gate, so one user action stays one undo step.
//!
//! The reconcile is idempotent and preserves the user's toggle state and slotting;
//! [`coh_data::sync_granted_powers`] owns those properties. This is the thin layer that
//! routes its faults to the console — the visible half of an unreadable gate is the
//! picker's fault row, which reads the same expression through the same evaluator.

use coh_data::{CharacterState, PowerDatabase};

/// Reconcile every bucket's power-gated grants, routing faults to the console.
pub fn sync(state: &mut CharacterState, database: &PowerDatabase) {
    for fault in coh_data::sync_granted_powers(state, database) {
        warn(&format!("granted-power gate unreadable: {fault}"));
    }
}

#[cfg(target_arch = "wasm32")]
fn warn(message: &str) {
    web_sys::console::warn_1(&message.into());
}

#[cfg(not(target_arch = "wasm32"))]
fn warn(message: &str) {
    eprintln!("{message}");
}
