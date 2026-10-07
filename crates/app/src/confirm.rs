//! The shared confirm modal for destructive acts (the beta's
//! `CoH-Sidekick/src/components/modals/ConfirmModal.tsx` — the port source lives in the beta
//! only; DEC8 deleted this repo's copy of the client): a small [`Modal`] that states the
//! consequence and offers two answers.
//!
//! Not every destructive act earns one. An act that commits through
//! [`crate::build_session::BuildSession::commit`] is one Ctrl+Z away, and an act
//! whose control has a place to stand states the question in its own seat (the
//! main menu's New build confirms in place, in the row's own place). This
//! component is for the rest: an irreversible act whose trigger is a bare row
//! with no seat wide enough to state the consequence in.
//!
//! The caller owns the open state and stops rendering this to dismiss, like
//! every [`Modal`] caller. Escape, the backdrop and the header ✕ all answer
//! Cancel — a confirm that could be dismissed by accident into the destructive
//! answer would confirm nothing.

use crate::modal::{Modal, ModalSize};
use dioxus::prelude::*;

/// Which answer is the go. The confirm button takes the colour channel of the
/// act: destroying reads as the careful one, replacing reads as the ordinary one.
// The full tier set is the beta ConfirmModal's API; only `Danger` is wired to a caller
// yet, the same position `ModalSize` holds beside it.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConfirmVariant {
    Danger,
    Primary,
}

#[component]
pub fn ConfirmModal(
    title: String,
    message: String,
    #[props(default = String::from("Confirm"))] confirm_label: String,
    #[props(default = String::from("Cancel"))] cancel_label: String,
    #[props(default = ConfirmVariant::Danger)] variant: ConfirmVariant,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let confirm_class = match variant {
        ConfirmVariant::Danger => "seg is-destructive",
        ConfirmVariant::Primary => "seg is-primary",
    };
    rsx! {
        Modal {
            title,
            size: ModalSize::Sm,
            on_close: on_cancel,
            div { class: "confirm",
                p { class: "confirm__message", "{message}" }
                div { class: "confirm__actions",
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| on_cancel.call(()),
                        "{cancel_label}"
                    }
                    button {
                        class: "{confirm_class}",
                        r#type: "button",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}
