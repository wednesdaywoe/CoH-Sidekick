//! The Controls modal — the interaction reference (the beta's `ControlsModal`).
//!
//! The beta's sheet describes the beta. This one is written against this tree: every
//! row names a handler that exists in `crates/app` (the hover that drives the Info
//! panel, the slot right-clicks, the stepper drag, the grid's keyboard grip, the
//! document-level undo), and none of the beta's rows survives unless the rebuild
//! does the same act.
//!
//! The desktop/mobile split is the beta's shape: a toggle reading Auto (with the
//! detected device named) / Desktop / Mobile, where Auto asks the platform the
//! same question the app's touch layer does — `(pointer: coarse)`, not a width
//! breakpoint. The toggle is per-open state, exactly as in the beta: nothing is
//! persisted, so a device that changes between opens is re-detected on the next
//! one.

use crate::modal::{Modal, ModalSize};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct ControlsOpen(pub Signal<bool>);

/// The half of the sheet on screen. `Auto` asks the platform; the other two are
/// the user overriding what they were handed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeviceView {
    Auto,
    Desktop,
    Mobile,
}

/// One row: the act on the left of the dash, what it does on the right.
///
/// Public because the Help modal's search reads these rows rather than restating them
/// ([`reference_rows`]) — the sheet stays the one place a gesture is described.
pub struct Row {
    pub action: &'static str,
    pub description: &'static str,
}

/// One section of the sheet. `accent` is the CSS suffix that picks the section's
/// hue, the beta's per-section colour in the rebuild's token vocabulary.
struct Section {
    title: &'static str,
    accent: &'static str,
    rows: &'static [Row],
}

/// The desktop half. Every row is a handler in this tree, not a line of the
/// beta's sheet: the hover is `picked-power-card`'s `onmouseenter` driving the
/// shared selection; the right-clicks are the slot cells' `oncontextmenu`; the
/// handle is the slot stepper's pointer drag; the grip is the grid's keyboard
/// placement state machine.
const DESKTOP: [Section; 3] = [
    Section {
        title: "Planner",
        accent: "planner",
        rows: &[
            Row {
                action: "Hover a power card",
                description:
                    "The Info panel reads it out of the engine's projection, and stays there when you move on.",
            },
            Row {
                action: "Click a slot",
                description: "Opens the enhancement picker for that slot.",
            },
            Row {
                action: "Right-click a filled slot",
                description: "Empties it, the short way round; the picker's own Empty this slot does the same.",
            },
            Row {
                action: "Right-click an empty slot",
                description: "Removes the slot, when the power placed it itself.",
            },
            Row {
                action: "Drag the ‹› handle",
                description: "Adds or removes several slots at once; a tap adds one. The whole run is one undo. Dragged fully left it gives back every slot the power placed.",
            },
            Row {
                action: "Drag a power's level badge",
                description: "In the By Level layout, moves the power to another level: onto an unspent pick it takes that level, onto another power the two swap. Slots and enhancements go with it.",
            },
        ],
    },
    Section {
        title: "Enhancement Picker",
        accent: "picker",
        rows: &[
            Row {
                action: "Click a piece",
                description: "Slots it at the craft level, attunement and boosters the picker header sets.",
            },
            Row {
                action: "The tabs",
                description: "Generic IO, IO Sets, Special and Origin; each is filtered by the power's own allow-lists.",
            },
            Row {
                action: "Category and set size",
                description: "Cut the IO Sets tab — “a 3-piece Hold set” is the real question.",
            },
            Row {
                action: "Hover a slot",
                description: "The rich tooltip: level, aspects, procs and the set bonuses the piece would claim.",
            },
        ],
    },
    Section {
        title: "Panels & layout",
        accent: "layout",
        rows: &[
            Row {
                action: "Drag a panel's header",
                description: "Moves it; the corner handle resizes, the fold control collapses it to one row.",
            },
            Row {
                action: "Grip, then Enter",
                description: "Keyboard placement: arrows move, Shift+arrows resize, Enter places, Escape cancels.",
            },
            Row {
                action: "The quickbar",
                description: "Pins the tools and panels you reach for; click a pin to open or dock it, hover ✕ to unpin.",
            },
        ],
    },
];

/// The mobile half. No long-press and no touch-hold menu exist in this tree, and
/// that is now a decided absence rather than an unbuilt one: the slot's acts are
/// reached through the picker a tap already opens and through the card's own
/// controls, so a finger and a mouse run the same roads. The rows are the touch
/// forms of acts that are already here
/// — and the two that are new to the platform: the Reorder overlay and the
/// touch-seat growth.
const MOBILE: [Section; 3] = [
    Section {
        title: "Planner",
        accent: "planner",
        rows: &[
            Row {
                action: "Tap a slot",
                description: "Opens the enhancement picker for that slot, the same way a desktop click does.",
            },
            Row {
                action: "Finger-drag the ‹› handle",
                description: "Adds or removes several slots at once; a tap adds one. The whole run is one undo. Dragged fully left it gives back every slot the power placed.",
            },
            Row {
                action: "Finger-drag a power's level badge",
                description: "In the By Level layout, moves the power to another level, or swaps it with the power it is dropped on.",
            },
            Row {
                action: "The ON/OFF pill",
                description: "Turns a caster-buffing power on and off; it is a real checkbox, so a finger and a keyboard both reach it.",
            },
        ],
    },
    Section {
        title: "Enhancement Picker",
        accent: "picker",
        rows: &[
            Row {
                action: "Tap a piece",
                description: "Slots it at the craft level, attunement and boosters the picker header sets.",
            },
            Row {
                action: "Lv / Attune / Boost",
                description: "The picker header's global slotting controls, stamped onto whatever piece you pick next.",
            },
        ],
    },
    Section {
        title: "Panels & layout",
        accent: "layout",
        rows: &[
            Row {
                action: "The stack",
                description: "Below 900px the grid becomes one column; Reorder (in the Display menu) arranges it by drag or ▲/▼.",
            },
            Row {
                action: "Touch targets",
                description: "On a coarse pointer the controls grow to a 44px seat; the glyphs keep their size.",
            },
        ],
    },
];

/// The half that shows on every device: the acts that are identical regardless
/// of what is pressing the keys.
///
/// The Slots section is here for a reason worth stating, because it is the
/// slot-context-menu call made visible: this tree has no slot menu and no
/// long-press, so every act on a slot beyond the direct gestures is reached the
/// same way on a mouse and on a finger. Rows that read identically on both
/// halves belong here — `no_act_reads_identically_on_desktop_and_mobile` is the
/// gate, and it caught these two the first time they were filed as a pair.
const EVERYWHERE: [Section; 2] = [
    Section {
        title: "Slots",
        accent: "planner",
        rows: &[
            Row {
                action: "Empty this slot",
                description: "In the enhancement picker's footer, on a slot that holds a piece: takes it back out. On a touch device this is the way a slot is emptied.",
            },
            Row {
                action: "The ⌫ past the slot handle",
                description: "Empties every slot on that power, keeping the slots. One undo puts them all back.",
            },
            Row {
                action: "A set's name in the picker",
                description: "Unfolds that set's bonuses under its pieces, the tiers this power already reaches lit. Tap or click it again to fold it.",
            },
        ],
    },
    Section {
        title: "Everywhere",
        accent: "everywhere",
        rows: &[
            Row {
                action: "The pin on a power",
                description: "Pins that power in the Info panel, so hovering elsewhere leaves it there; click it again to unpin. Right-click on the card or the L key does the same.",
            },
            Row {
                action: "The ⋮ on a power",
                description: "Opens that power's menu: empty every slot, or remove every slot you added.",
            },
            Row {
                action: "Escape or the backdrop",
                description: "Closes any open modal or menu.",
            },
            Row {
                action: "Ctrl+Z / Ctrl+Y",
                description:
                    "Undo and redo build edits (Cmd on Mac), skipped while a text field is focused.",
            },
            Row {
                action: "The header's undo/redo",
                description: "The same two acts as buttons, beside the identity chip.",
            },
        ],
    },
];

/// Every row of the sheet, paired with the section it sits under.
///
/// The Help modal searches this instead of carrying its own copy of the gestures. Two surfaces
/// describing one act is exactly the drift the content-drift note below is about, and handing
/// the rows out is what makes the other reader a reader rather than a second author.
///
/// Both device halves are walked, so a search reaches the touch form of an act from a desktop
/// and the mouse form from a phone. The reader is looking something up, not being told what
/// their own hardware does.
pub fn reference_rows() -> impl Iterator<Item = (&'static str, &'static Row)> {
    DESKTOP
        .iter()
        .chain(MOBILE.iter())
        .chain(EVERYWHERE.iter())
        .flat_map(|section| section.rows.iter().map(move |row| (section.title, row)))
}

/// The platform's own answer to "is a finger on screen", the same query the
/// app's touch layer branches on — a pointer kind, not a width. Guarded for
/// non-browser contexts, where the honest answer is "not touch".
async fn is_touch() -> bool {
    let js = "typeof window !== 'undefined' && window.matchMedia('(pointer: coarse)').matches";
    document::eval(js)
        .await
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// The one Controls modal, hosted at the shell root like every other overlay:
/// a `fixed` backdrop rendered inside a grid surface would be contained by its
/// `transform` (see [`crate::modal`]).
///
/// The host only gates; the body is a separate component that mounts and
/// unmounts with `open`, so its view and touch signals reset on every open —
/// the beta's own shape, where the `isOpen` gate remounts the modal and its
/// `useState` starts fresh each time.
#[component]
pub fn ControlsHost() -> Element {
    let mut open = use_context::<ControlsOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        ControlsBody { on_close: move |_| open.set(false) }
    }
}

/// The modal and its content. Mounted only while open, so the device toggle
/// and the touch reading are per-open state, never persisted.
#[component]
fn ControlsBody(on_close: EventHandler<()>) -> Element {
    let mut view = use_signal(|| DeviceView::Auto);
    let mut touch = use_signal(|| false);
    // Runs on this component's mount, i.e. on every open: a device that
    // changed since the last open is re-detected rather than read from a
    // stale flag.
    use_future(move || async move {
        touch.set(is_touch().await);
    });

    let show_desktop =
        matches!(view(), DeviceView::Desktop) || (matches!(view(), DeviceView::Auto) && !touch());
    let show_mobile =
        matches!(view(), DeviceView::Mobile) || (matches!(view(), DeviceView::Auto) && touch());
    let sections: Vec<&Section> = DESKTOP
        .iter()
        .filter(|_| show_desktop)
        .chain(MOBILE.iter().filter(|_| show_mobile))
        .chain(EVERYWHERE.iter())
        .collect();
    let auto_label = if touch() {
        "Auto (Mobile)".to_string()
    } else {
        "Auto (Desktop)".to_string()
    };

    rsx! {
        Modal {
            title: "Controls".to_string(),
            size: ModalSize::Lg,
            on_close,
            div { class: "controls",
                div { class: "controls__toggle", role: "group", "aria-label": "Device view",
                    button {
                        class: if matches!(view(), DeviceView::Auto) {
                            "seg active"
                        } else {
                            "seg"
                        },
                        r#type: "button",
                        "aria-pressed": matches!(view(), DeviceView::Auto),
                        onclick: move |_| view.set(DeviceView::Auto),
                        "{auto_label}"
                    }
                    button {
                        class: if matches!(view(), DeviceView::Desktop) {
                            "seg active"
                        } else {
                            "seg"
                        },
                        r#type: "button",
                        "aria-pressed": matches!(view(), DeviceView::Desktop),
                        onclick: move |_| view.set(DeviceView::Desktop),
                        "Desktop"
                    }
                    button {
                        class: if matches!(view(), DeviceView::Mobile) {
                            "seg active"
                        } else {
                            "seg"
                        },
                        r#type: "button",
                        "aria-pressed": matches!(view(), DeviceView::Mobile),
                        onclick: move |_| view.set(DeviceView::Mobile),
                        "Mobile"
                    }
                }
                for section in sections {
                    div { key: "{section.title}-{section.accent}", class: "controls__section",
                        h3 { class: "controls__title controls__title--{section.accent}", "{section.title}" }
                        ul { class: "controls__list",
                            for row in section.rows {
                                li { key: "{row.action}", class: "controls__row",
                                    span { class: "controls__action", "{row.action}" }
                                    span { class: "controls__dash", "—" }
                                    span { "{row.description}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
