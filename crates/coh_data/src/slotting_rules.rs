//! Whether a set piece may be placed into a slot right now — the rule the picker renders as a
//! piece's disabled state, and re-asks at placement time.
//!
//! It lives here, beside the catalog it reads, rather than in the picker, because three callers
//! have to agree about it: the rows the picker draws, the bulk paths that sweep pieces the user
//! never clicked (a drag range covers every piece between its endpoints, disabled or not), and
//! any later re-slotting. A rule the UI owns is a rule each of those re-implements slightly
//! differently. The beta learned this the expensive way — `pieceSlottableNow` was factored out of
//! its picker after a 1st→3rd drag placed a Superior ATO piece that was already in the power.
//!
//! Two rules, from two different scopes:
//!
//! - **Within the power**: the same piece of the same set cannot occupy two of one power's slots.
//! - **Across the build**: a piece the export flags [`unique`](IoSetPiece::unique) may be held
//!   once in the whole build. The flag is the export's, not a rarity guess — see IOUNIQUE-1,
//!   where a converter default said "stackable" about 12 ATO pieces and the downstream guess
//!   written to paper over it was right by accident.

use crate::{CharacterState, Enhancement, EnhancementKind, IoSetCatalog, IoSetPiece};

/// Whose slotting the build-wide single-copy rule is asked about.
///
/// A Compare Slotting configuration is a hypothetical arrangement of ONE power, not a build, so
/// the build-wide rule does not reach into it: two configurations of the same power both holding
/// the build's one Gladiator's Armor is the comparison working, not two copies existing.
pub enum UniqueScope<'a> {
    /// The live build — a unique piece anywhere in it blocks another copy.
    Build {
        state: &'a CharacterState,
        catalog: &'a IoSetCatalog,
    },
    /// A standalone arrangement, graded only against its own slots.
    Isolated,
}

/// Whether `piece` of `set_id` may be placed into a power holding `current_slots`.
///
/// `current_slots` is the DESTINATION's slotting — the build's for a normal pick, a Compare row's
/// for that row — so the two never disagree about which power is being filled.
pub fn piece_slottable_now(
    set_id: &str,
    piece: &IoSetPiece,
    current_slots: &[Option<Enhancement>],
    scope: &UniqueScope<'_>,
) -> bool {
    if holds_piece(current_slots, set_id, piece.num) {
        return false;
    }
    match scope {
        UniqueScope::Isolated => true,
        UniqueScope::Build { state, catalog } => {
            !piece.unique || !unique_piece_held(state, catalog, current_slots, set_id, piece.num)
        }
    }
}

/// Whether any of these slots already holds piece `num` of `set_id`.
fn holds_piece(slots: &[Option<Enhancement>], set_id: &str, num: u8) -> bool {
    slots.iter().flatten().any(|enh| match &enh.kind {
        EnhancementKind::IoSet {
            set_id: held,
            piece_num,
            ..
        } => held == set_id && *piece_num == num,
        _ => false,
    })
}

/// Whether the build already holds this unique piece — in the set itself, or in the set's
/// counterpart.
///
/// A build may hold a set or its Superior twin, never both, so the two share one budget. The
/// export states that outright: the piece's `slot_requires` carries a `BoostsSlotted>` test naming
/// the counterpart beside the one naming itself. It is re-derived here from the id instead,
/// because Homecoming ships no `boosts/` tree to read the reference out of — and a census of the
/// three forks that do ship one found every cross-reference pointing at a variant of the piece
/// itself, so the two derivations agree wherever both can be asked (IOUNIQUE-1).
fn unique_piece_held(
    state: &CharacterState,
    catalog: &IoSetCatalog,
    current_slots: &[Option<Enhancement>],
    set_id: &str,
    num: u8,
) -> bool {
    let mut budget = vec![set_id.to_string()];
    if let Some(counterpart) = counterpart_set_id(catalog, set_id) {
        budget.push(counterpart);
    }
    let held = |slots: &[Option<Enhancement>]| budget.iter().any(|id| holds_piece(slots, id, num));
    // `current_slots` as well as the build, because a bulk placement asks this question about
    // slots it has decided to fill but not yet committed. Without it, marking a set's piece 1 AND
    // its Superior twin's piece 1 in one sweep passes both checks against a build that holds
    // neither, and the batch places a pair the game forbids.
    held(current_slots) || state.all_selected().any(|power| held(&power.slots))
}

/// The set sharing a single-copy budget with this one: a set's Superior twin, or a Superior set's
/// plain one. `None` when the catalog carries no such twin — the prefix is a claim about this
/// dataset's ids, so it is checked against them rather than assumed.
///
/// **Not gated on rarity, and that gate was a bug.** Both planners paired only `ato` sets, which
/// silently let a build hold Blistering Cold and Superior Blistering Cold at once — the Winter
/// sets pair exactly as the archetype sets do, and the export says so in the same field (found by
/// clicking the picker, 2026-09-15). Rebirth also files `winters_gift` as `rare` against a
/// `superior_winters_gift` filed as `event`, so a rarity test breaks on the pair it is meant to
/// describe. The structural claim is the sound one, and it is measured: on all four forks every
/// `superior_`-prefixed set has a plain twin in the catalogue, so the prefix invents no pairs.
fn counterpart_set_id(catalog: &IoSetCatalog, set_id: &str) -> Option<String> {
    const SUPERIOR: &str = "superior_";
    let counterpart = match set_id.strip_prefix(SUPERIOR) {
        Some(plain) => plain.to_string(),
        None => format!("{SUPERIOR}{set_id}"),
    };
    catalog.get(&counterpart).map(|_| counterpart)
}
