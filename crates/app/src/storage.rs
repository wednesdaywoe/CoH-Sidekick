//! Every localStorage WRITE in the app, funnelled through the window that owns persistence.
//!
//! One window means nothing here; a second one means everything. Measured 2026-09-15 by
//! [`pop3_storage_origin`](../examples/pop3_storage_origin.rs): a second webview's localStorage
//! is a SEPARATE store seeded from disk when that webview is created, behind an identical origin
//! (`dioxus://index.html`). It never sees the first window's live writes, in either direction.
//! That much is only awkward. This is the part that is not:
//!
//! > The moment the second webview writes, the first window's later writes stop landing — gone
//! > from its own store and from the disk the next launch reads. Writes made before that point
//! > survive. A second webview that only READS leaves the first intact.
//!
//! So one stat-config save from a popped-out window costs the user every drag, hide, theme and
//! build save the main window makes for the rest of the session, silently. Two webviews sharing
//! the default `WebContext` directory is what does it — `dioxus-desktop` builds a fresh
//! `wry::WebContext` per webview (`webview.rs:260`) and a `None` data directory means the
//! default one, twice.
//!
//! **The write is moved rather than refused.** `dioxus_document::eval` is `document().eval(..)`
//! and `document()` is a `try_consume_context::<Rc<dyn Document>>()`, so the handle is an
//! ordinary `Rc` and every webview shares the main thread — the same argument POP1 made for
//! signals, applied to the document. The shell root claims its handle here; [`commit`] evaluates
//! through it whichever dom is running. Refusing instead would have traded corruption for a
//! dashboard edit that is silently never saved, which is the same bug wearing a different coat.
//! [`pop3_owner_doc`](../examples/pop3_owner_doc.rs) is the key: B's relayed write lands in A's
//! store, B's own store never sees it, and A keeps writing normally afterward.
//!
//! **Writes only.** Reads stay in the calling document, and a read from a popped-out window is a
//! read of that window's snapshot — stale by however long it has been open. Nothing in the app
//! loads persisted state from a popped window, and this is the reason not to start.
//!
//! DOM work is not storage and does not belong here: `scroll_into_view`, `measure_grid_space` and
//! the `data-theme` half of [`crate::theme::apply_theme`] act on the document the user is looking
//! at, which is exactly the one this module routes away from.

use dioxus::document::Document;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// The document every persisted write is evaluated in. A `thread_local` because every
    /// webview shares the main thread (POP1), so one slot reaches all of them.
    static OWNER: RefCell<Option<Rc<dyn Document>>> = const { RefCell::new(None) };
}

/// Claim the calling dom's document as the one that owns persistence. Called once from the shell
/// root, which is the only component that mounts in the main window and nowhere else.
pub fn claim_owner() {
    let document = dioxus::document::document();
    OWNER.with_borrow_mut(|slot| *slot = Some(document));
}

/// Run a storage-mutating script in the owning window, whichever dom is calling.
///
/// Fire-and-forget, like every `persist_*` that reaches it: the `Eval` is dropped, and each
/// caller's script carries its own `try`/`catch` so a browser with storage disabled is a no-op
/// rather than an error.
///
/// With no owner claimed — unit tests, and the moment before the shell's first render — it falls
/// back to the calling document, which is the pre-POP3 behaviour and correct whenever there is
/// only one window to be in.
pub fn commit(js: String) {
    OWNER.with_borrow(|slot| match slot {
        Some(document) => {
            document.eval(js);
        }
        None => {
            document::eval(&js);
        }
    });
}
