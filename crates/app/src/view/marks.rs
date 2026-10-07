//! The app's chrome marks — the panel headers' controls and the quickbar's actions — drawn
//! rather than typed.
//!
//! Everything else in the app states a mark as a character and sizes it through the ink ramp in
//! `tokens.css` — a target ink height divided by the ratio of ink to em for that glyph. The ramp
//! exists because a font decides how much of an em a `▼` or a `✕` actually fills, and the ratios
//! differ enough that one font-size across several marks draws them at visibly different sizes.
//!
//! Three problems the ramp cannot solve, all of them on the header controls:
//!
//! - **The ratios are a property of the resolved stack**, so they are measured, not known. The
//!   ramp's own comment carries a warning that an earlier table of them was wrong about five
//!   rows, and an instruction to re-measure in the app. That is a standing liability on marks
//!   this small.
//! - **A typed mark has no weight.** `✕` is a hairline in the stack this app resolves, and no
//!   font-size fixes thin — past a point it just makes a bigger thin thing.
//! - **`⚙` cannot be drawn at chrome size by a font.** At the scale these sit it resolves to a
//!   blob, which is why it needed its own 0.50 divisor and still read soft.
//!
//! A drawn mark answers all three: the viewBox is the ink box (so the box IS the size, in every
//! stack), `stroke-width` is a real dimension, and the shape is ours. `--icon-chrome` and
//! `--icon-stroke` in `tokens.css` carry the two numbers; the shapes below carry nothing but
//! geometry, and take their colour from the control they sit in (`stroke: currentColor`).
//!
//! All three are `aria-hidden`: the control around them already carries the label, and a mark
//! that announced itself would double it.

use dioxus::prelude::*;

/// The fold control's chevron. Drawn open (pointing down, "this is expanded"); the folded state
/// is the same mark rotated a quarter turn, exactly as the typed caret was — one shape for both
/// states, so neither can drift from the other, and the rotation is what the eye reads as the
/// hinge moving.
pub fn chevron(folded: bool) -> Element {
    rsx! {
        svg {
            class: if folded { "panel-icon panel-icon--folded" } else { "panel-icon" },
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M2.75 4.75 L6 8.25 L9.25 4.75" }
        }
    }
}

/// The hide control's cross. Two strokes over a 6-unit span of the 12-unit box — half the box,
/// so it reads a shade under the title's cap height beside it, which is what keeps it chrome.
pub fn close() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M3 3 L9 9 M9 3 L3 9" }
        }
    }
}

/// The stat panels' config control.
///
/// Sliders rather than the cog it replaces, on the control's own meaning: this opens "choose
/// which stats to show", which is a set of things turned on and off, not a settings screen. The
/// cog was the vaguer mark of the two and also the one that could not survive being drawn this
/// small. The tracks break under each knob so the knob reads as riding the track rather than
/// sitting on top of it.
pub fn sliders() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M1.5 4 H2.6 M5.4 4 H10.5 M1.5 8 H6.6 M9.4 8 H10.5" }
            circle { cx: "4", cy: "4", r: "1.4" }
            circle { cx: "8", cy: "8", r: "1.4" }
        }
    }
}

/* ---- the quickbar's action marks ----
One per action on the top row, and the reason the row carries marks at all is that it is the
only thing separating it from the panel row beneath it: two rows of plain words read as one
list of twenty-four things. A mark on every entry of the upper row and none on the lower is
what says these are two different kinds of control.

Every one is drawn in the same 12-unit box as the panel marks above, so a single
`--icon-chrome` sizes the whole app's chrome and no mark can drift a pixel off its neighbours.
They name the action's *shape* — a magnifier for a lookup, a sigma for a sum, a flask for a
hypothetical — never the game entity behind it, because the label beside each one already
says which entity. */

/// Accolades: a rosette — a disc over two splayed ribbon tails. An accolade is a thing earned
/// and worn, which is what a rosette is and what a trophy cup is not.
///
/// The tails splay outward to the full width of the box, and that is the whole trick. Drawn
/// with the tails hanging straight down the shape necks in under the disc and reads as a
/// keyhole; hung above the disc instead, as a medal ribbon, the two strokes read as horns. Only
/// the wide splay makes the notch between them survive being 13px across.
pub fn accolades() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            circle { cx: "6", cy: "4.4", r: "2.9" }
            path { d: "M4 6.5 L2.4 11 L6 9 L9.6 11 L8 6.5" }
        }
    }
}

/// Set Bonus Finder: a magnifier. It is the one action that runs from an effect back to the
/// sets granting it, which is a lookup and nothing else.
pub fn finder() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            circle { cx: "5.3", cy: "5.3", r: "3.2" }
            path { d: "M7.6 7.6 L10.6 10.6" }
        }
    }
}

/// Totals: a sigma. The sheet is every stat with its sources summed, and the summation sign
/// says that in one glyph that no picture of a list does.
pub fn totals() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M9.4 2.3 H3 L6.1 6 L3 9.7 H9.4" }
        }
    }
}

/// Chains: a cycle arrow. The action packs a rotation and reads its sustained DPS, so the mark
/// is the loop, not the link.
///
/// A chain link was the first draft and the obvious one — it matches the label's word. It also
/// spends most of its ink on two arcs that at 13px are three pixels each, so it drew as a
/// squiggle beside marks that draw as shapes. The loop is what the thing *is* (a rotation
/// repeated until endurance runs out), and it survives being small.
pub fn chains() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M7.8 2.9 A3.6 3.6 0 1 1 4.2 2.9" }
            path { d: "M3.5 4.3 L4.2 2.9 L2.6 2.8" }
        }
    }
}

/// What-if: a flask. The layer injects buffs the build does not have to see what would happen,
/// which is an experiment run on the build rather than a fact about it.
pub fn what_if() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M4.8 1.65 v3.075 L2.1 9 a1.2 1.2 0 0 0 1.05 1.8 h5.7 A1.2 1.2 0 0 0 9.9 9 L7.2 4.725 V1.65" }
            path { d: "M4.2 1.65 h3.6" }
        }
    }
}

/// Proc sources: a bolt. A proc is the effect that fires on a chance, and the bolt is the
/// app's mark for a thing that happens on its own.
pub fn procs() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M6.9 1.5 L3 6.825 h2.7 l-0.6 3.675 l3.9 -5.325 H6.3 z" }
        }
    }
}

/// Compare Powersets: two columns, which is the shape the comparison takes on screen.
pub fn columns() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            rect { x: "1.4", y: "1.8", width: "3.6", height: "8.4", rx: "1" }
            rect { x: "7", y: "1.8", width: "3.6", height: "8.4", rx: "1" }
        }
    }
}

/// Compare Slotting: the same two columns with a slot in each. Deliberately a sibling of
/// [`columns`] rather than an unrelated shape — the two actions sit next to each other and
/// differ only in what is being compared, so the marks differ only in what is inside them.
pub fn slotting() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            rect { x: "1.4", y: "1.8", width: "3.6", height: "8.4", rx: "1" }
            rect { x: "7", y: "1.8", width: "3.6", height: "8.4", rx: "1" }
            circle { cx: "3.2", cy: "6", r: "1" }
            circle { cx: "8.8", cy: "6", r: "1" }
        }
    }
}

/// Enhancement List: a bulleted list, which is literally what the action produces.
pub fn list() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M4.4 2.9 H10.6 M4.4 6 H10.6 M4.4 9.1 H10.6" }
            path { d: "M1.6 2.9 h0.01 M1.6 6 h0.01 M1.6 9.1 h0.01" }
        }
    }
}

/// Enhancement Tools: a column of slots with one arrow lifting all of them. A sibling of
/// [`list`] — the two actions read the same build's slotting, and this is the one that writes
/// to it, so the slots are the shape they share and the arrow is the whole difference.
pub fn lift() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            circle { cx: "3", cy: "2.6", r: "1.15" }
            circle { cx: "3", cy: "6", r: "1.15" }
            circle { cx: "3", cy: "9.4", r: "1.15" }
            path { d: "M8.4 10 V2.6" }
            path { d: "M6.7 4.3 L8.4 2.6 L10.1 4.3" }
        }
    }
}

/// Controls: a keyboard. The keys are zero-length strokes rather than dots, so `stroke-linecap`
/// draws them at exactly the stroke width every other mark is drawn at.
pub fn keyboard() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            rect { x: "1", y: "3.1", width: "10", height: "5.8", rx: "1.3" }
            path { d: "M3.2 5.3 h0.01 M5.1 5.3 h0.01 M7 5.3 h0.01 M8.9 5.3 h0.01 M3.9 7.2 H8.1" }
        }
    }
}

/// The pin toggle in the quickbar's More menu. One shape for both states, like
/// [`chevron`]: the head, the shaft and the point are the same geometry either way, and
/// *filled* is what says the item is on the row.
///
/// Fill rather than a second glyph because the two states are the same thing present or
/// absent, and the app already spends its outline/solid distinction on exactly that (a dashed
/// enhancement slot against a filled one). A second shape would have to be learned; this one
/// is already known from every other holder in the app.
///
/// Drawn at a slant, point down-left, which is how a pin reads as *stuck into* something
/// rather than as a tack lying on it — upright it is a lollipop at this size.
pub fn pin(pinned: bool) -> Element {
    rsx! {
        svg {
            class: if pinned { "panel-icon panel-icon--solid" } else { "panel-icon" },
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M7.2 1.4 L10.6 4.8 L8.95 5.25 L7.1 7.1 L7.45 9.05 L6.5 10 L2 5.5 L2.95 4.55 L4.9 4.9 L6.75 3.05 Z" }
            path { d: "M3.9 8.1 L1.6 10.4" }
        }
    }
}

/// The main menu's trigger: three rules. The only mark in the header that stands for a MENU
/// rather than for a thing, and the one shape a reader arrives already knowing — which is the
/// whole argument for it over anything more descriptive. Nothing else in this app draws
/// parallel full-width rules, so it cannot be confused with [`list`], whose rules are shorter
/// and carry leading dots.
pub fn menu() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M2 3.4 H10 M2 6 H10 M2 8.6 H10" }
        }
    }
}

/// The pop-out control's mark: a box with its top-right corner opened, and an arrow leaving
/// through the gap.
///
/// The corner is BROKEN rather than complete, which is the whole reading — a closed box with an
/// arrow over it says "external link", and at 12 units the arrow is the only thing separating
/// the two meanings. Opening the corner puts the departure in the frame itself, so the mark
/// still says "this leaves" if the arrow is the part that goes soft on a low-DPI panel.
///
/// The arrow's tail is the box's own diagonal, so nothing here is at an angle the other marks
/// aren't; `close` is the same 45°.
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
pub fn pop_out() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M7.5 2.5 H2.5 V9.5 H9.5 V4.5 M6 6 L9.75 2.25 M6.75 2.25 H9.75 V5.25" }
        }
    }
}

/// The dock control's mark: [`pop_out`]'s box with the arrow reversed, coming back in through
/// the same opened corner.
///
/// Deliberately not a distinct shape. The pair is one gesture in two directions, and a reader
/// who has learnt the broken corner as "this surface travels through here" reads the return
/// without learning anything new — which a second metaphor (a home, a grid, an inward chevron)
/// would cost. The box is byte-identical to `pop_out`'s so the two cannot drift; only the
/// arrow's three segments are rotated a half turn about the diagonal's midpoint.
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
pub fn pop_in() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M7.5 2.5 H2.5 V9.5 H9.5 V4.5 M9.75 2.25 L6 6 M9 6 H6 V3" }
        }
    }
}

/* ---- the mobile nav's tab marks ----
The bottom bar's five tabs each carry a mark over a word, the beta's `MobileBottomNav` shape.
Options borrows [`sliders`] and Menu borrows [`menu`]; these three are the ones nothing else in
the app already drew. Same 12-unit box, same stroke, so the bar sizes them as one set. */

/// Home: a house — a roof peak over a walled box with a door gap in its floor line.
pub fn home() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M1.5 6 L6 1.8 L10.5 6 M3 4.8 V10.2 H5 V7.6 H7 V10.2 H9 V4.8" }
        }
    }
}

/// Dashboard: three bars of different heights on a baseline — the stat panels, read as numbers
/// side by side.
pub fn dashboard() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M1.5 10.5 H10.5 M3.5 10.5 V6.5 M6 10.5 V2.5 M8.5 10.5 V5" }
        }
    }
}

/// Incarnate: a lightning bolt, the beta's mark for the same tab.
pub fn incarnate() -> Element {
    rsx! {
        svg {
            class: "panel-icon",
            view_box: "0 0 12 12",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: "M7 1.2 L2.8 6.8 H6 L5 10.8 L9.2 5.2 H6 Z" }
        }
    }
}
