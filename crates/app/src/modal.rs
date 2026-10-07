//! Generic modal shell — a fixed full-viewport backdrop plus a centered
//! flex-column card. The rebuild's reusable dialog primitive (the beta
//! `CoH-Sidekick/src/components/modals/Modal.tsx` — beta-only since DEC8), for content-dense UI that a panel-anchored
//! popover would clip: a `position: fixed` card escapes the panel's `overflow`,
//! unlike an `absolute` popover. Structure is preserved from the beta; styling is
//! the rebuild's token vocabulary, not ported Tailwind.
//!
//! Callers mount it above the grid roots — the enhancement picker does this via
//! [`crate::panels::powers::EnhancementPickerHost`]. The rule dates from when the desktop grid
//! placed surfaces with `transform: translate3d`, which contains a `position: fixed` backdrop
//! and clipped a `Modal` to its panel. Surfaces are placed with `left`/`top` now, but each is
//! still an isolated stacking context, so a modal inside one can't rise above its neighbours.
//!
//! Closes on backdrop click, on the header close button, and on Escape. Clicks
//! inside the card are stopped so they never reach the backdrop's close handler.

use dioxus::prelude::*;

/// Card width tier — mirrors the beta `size` prop. The card is always capped at
/// ~90vh tall and manages its own inner scrolling.
// The full tier set is the reusable dialog API; only some are wired to callers yet
// (the enhancement picker uses `Lg`), so the rest are constructed as more beta modals
// are ported.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalSize {
    Sm,
    Md,
    Lg,
    Xl,
    Full,
}

impl ModalSize {
    fn class(self) -> &'static str {
        match self {
            ModalSize::Sm => "modal-card-sm",
            ModalSize::Md => "modal-card-md",
            ModalSize::Lg => "modal-card-lg",
            ModalSize::Xl => "modal-card-xl",
            ModalSize::Full => "modal-card-full",
        }
    }
}

/// A modal dialog. Renders a backdrop + titled card; `children` fill the card
/// body below the header. The caller owns the open/closed state and simply stops
/// rendering `Modal` to dismiss it (matching the beta's `isOpen` gate).
#[component]
pub fn Modal(
    title: String,
    #[props(default = ModalSize::Md)] size: ModalSize,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "modal-backdrop",
            tabindex: "-1",
            // Focus the backdrop on mount so it receives the Escape keydown without a
            // document-level listener (the beta uses one; Dioxus focuses the node).
            onmounted: move |evt| async move {
                let _ = evt.set_focus(true).await;
            },
            onkeydown: move |evt| {
                if evt.key() == Key::Escape {
                    on_close.call(());
                }
            },
            // Backdrop click closes; card clicks are stopped below, so this only
            // fires for clicks that land on the dimmed surround.
            onclick: move |_| on_close.call(()),
            div {
                class: "modal-card {size.class()}",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-header",
                    h2 { class: "modal-title", "{title}" }
                    button {
                        class: "modal-close",
                        "aria-label": "Close",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
                div { class: "modal-content", {children} }
            }
        }
    }
}
