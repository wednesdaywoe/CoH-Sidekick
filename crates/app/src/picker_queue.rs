//! The enhancement picker's multi-select queue — what "select more than one" holds between the
//! gesture and the placement.
//!
//! The picker's whole point is that slotting six pieces into one power should cost six picks and
//! one navigation, not six of each. That means a queue: the user marks pieces while the list
//! stays put, then spends them all at once into the power's empty slots.
//!
//! The order is load-bearing and is the order they were marked in, because that is the order they
//! land in. A queued tile therefore shows its position, so the mapping from "third one I picked"
//! to "third empty slot" is visible before it is committed rather than inferred afterwards.
//!
//! Legality is NOT settled here. A pick can become illegal between marking and placing — its twin
//! slotted in this power, or a unique consumed elsewhere in the build — so the placement path
//! re-asks [`coh_data::slotting_rules::piece_slottable_now`] per pick and skips what no longer
//! passes. This is the beta's 2026-08-17 duplicate-ATO report, which came in through exactly that
//! window: a drag range sweeps every piece between its endpoints, including ones drawn disabled.
//!
//! A set piece can be marked once; every other pick can be marked again. The game refuses a second
//! copy of a set piece in one power, but two Recharge IOs in Hasten is the ordinary build, and a
//! queue that unmarked on the second click made it two trips through the picker.

use coh_data::Enhancement;

/// One marked pick, as the identity of a thing to place rather than the thing itself.
///
/// A descriptor, not a built [`Enhancement`], so the header's craft level / attunement / booster
/// apply at PLACEMENT time. Marking six pieces and then raising the level has to raise all six —
/// building at mark time would stamp each with whatever the dial read when it was clicked, and
/// leave a queue whose pieces silently disagree.
#[derive(Clone, PartialEq, Debug)]
pub enum QueuedPick {
    SetPiece { set_id: String, piece_index: usize },
    Generic { stat: String },
    Special { id: String, category: String },
    Origin { stat: String, tier: String },
}

impl QueuedPick {
    /// Whether one power can hold more than one of this pick. Only a set piece is single-copy.
    pub fn repeatable(&self) -> bool {
        !matches!(self, QueuedPick::SetPiece { .. })
    }
}

/// What a marking click does: another copy of a repeatable pick, a toggle for a set piece.
pub fn press(queue: &mut Vec<QueuedPick>, pick: QueuedPick) {
    if pick.repeatable() {
        queue.push(pick);
    } else {
        toggle(queue, pick);
    }
}

/// Take back the most recent copy of this pick, leaving any earlier copies where they are.
pub fn unmark_last(queue: &mut Vec<QueuedPick>, pick: &QueuedPick) {
    if let Some(at) = queue.iter().rposition(|held| held == pick) {
        queue.remove(at);
    }
}

/// Mark or unmark a pick. Marking appends, so the queue stays in click order; unmarking closes
/// the gap, so the positions shown on the remaining tiles stay contiguous.
pub fn toggle(queue: &mut Vec<QueuedPick>, pick: QueuedPick) {
    match queue.iter().position(|held| held == &pick) {
        Some(at) => {
            queue.remove(at);
        }
        None => queue.push(pick),
    }
}

/// Mark a pick only if it is not already marked — what a drag range needs, since a range can
/// cover pieces the user marked individually a moment ago and re-toggling would unmark them.
pub fn mark(queue: &mut Vec<QueuedPick>, pick: QueuedPick) {
    if !queue.contains(&pick) {
        queue.push(pick);
    }
}

/// The badge on a marked tile: every 1-based place this pick holds in the queue, so two copies of
/// one IO read "1·3" and the slot each lands in stays visible. `None` when unmarked.
pub fn badge(queue: &[QueuedPick], pick: &QueuedPick) -> Option<String> {
    let places: Vec<String> = queue
        .iter()
        .enumerate()
        .filter(|(_, held)| *held == pick)
        .map(|(at, _)| (at + 1).to_string())
        .collect();
    (!places.is_empty()).then(|| places.join("·"))
}

/// The slots a bulk placement would fill: this power's empty slots from the one the picker was
/// opened on, forward.
///
/// Forward from the opened slot rather than from zero, because the slot that was clicked is the
/// one the user is pointing at — filling backwards into earlier empties would put the first
/// marked piece somewhere they did not ask for.
pub fn empty_slots_from(slots: &[Option<Enhancement>], from: usize) -> Vec<usize> {
    slots
        .iter()
        .enumerate()
        .skip(from)
        .filter(|(_, slot)| slot.is_none())
        .map(|(index, _)| index)
        .collect()
}

/// The piece indices a drag covers, in ascending order regardless of which end it started at.
pub fn drag_span(start: usize, end: usize) -> std::ops::RangeInclusive<usize> {
    start.min(end)..=start.max(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recharge() -> QueuedPick {
        QueuedPick::Generic {
            stat: "Recharge".into(),
        }
    }

    fn piece() -> QueuedPick {
        QueuedPick::SetPiece {
            set_id: "Set".into(),
            piece_index: 0,
        }
    }

    #[test]
    fn a_second_press_adds_another_copy_of_a_generic() {
        let mut queue = Vec::new();
        press(&mut queue, recharge());
        press(&mut queue, piece());
        press(&mut queue, recharge());
        assert_eq!(queue, vec![recharge(), piece(), recharge()]);
        assert_eq!(badge(&queue, &recharge()).as_deref(), Some("1·3"));
    }

    #[test]
    fn a_second_press_on_a_set_piece_unmarks_it() {
        let mut queue = Vec::new();
        press(&mut queue, piece());
        press(&mut queue, piece());
        assert!(queue.is_empty());
        assert_eq!(badge(&queue, &piece()), None);
    }

    #[test]
    fn unmark_last_keeps_the_earlier_copy() {
        let mut queue = vec![recharge(), piece(), recharge()];
        unmark_last(&mut queue, &recharge());
        assert_eq!(queue, vec![recharge(), piece()]);
    }
}
