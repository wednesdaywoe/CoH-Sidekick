//! A titled, collapsible column of power cards — the `.power-column` shape shared by every
//! picked-power section (Primary, Secondary, each pool, the epic pool), by each inherents
//! category, and by each band of the by-level ladder.
//!
//! Collapse lives per group rather than per panel: a build carries far more granted and pool
//! powers than a panel dragged small on the free grid can show at once, so folding the groups
//! a build isn't working on is what keeps the rest reachable without scrolling. The count stays
//! in the header while folded, because a group that hides both its cards and how many it has
//! reads as an empty one.
//!
//! The fold is deliberately not persisted — it is a "not right now", not a property of the
//! build.

use dioxus::prelude::*;

/// One collapsible group of cards.
///
/// `starts_collapsed` is read once, when the group mounts; every toggle after that is the
/// user's. Groups pass `true` for the ones a build almost never touches.
#[component]
pub fn PowerGroup(
    title: String,
    /// How many cards the group holds, shown beside the title so a folded group still says
    /// how much it is hiding.
    count: usize,
    #[props(default = false)] starts_collapsed: bool,
    children: Element,
) -> Element {
    let mut collapsed = use_signal(|| starts_collapsed);

    rsx! {
        div { class: "power-column",
            button {
                class: "group-title",
                "aria-expanded": !collapsed(),
                onclick: move |_| collapsed.toggle(),
                span {
                    class: if collapsed() {
                        "group-title__caret caret--folded"
                    } else {
                        "group-title__caret"
                    },
                    "▼"
                }
                span { class: "group-title__text", "{title}" }
                span { class: "group-title__count", "{count}" }
            }
            if !collapsed() {
                {children}
            }
        }
    }
}
