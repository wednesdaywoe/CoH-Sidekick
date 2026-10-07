//! POP3 — can a second window's write be made to land in the FIRST window's localStorage?
//!
//! [`pop3_storage_origin`](pop3_storage_origin.rs) settled that it must: two webviews hold
//! separate stores, and the moment the second one writes, the first one's later writes stop
//! landing. So a popped-out window has to stop writing — but POP5 put the stat organizer inside
//! it, and gating its persist would just trade corruption for a dashboard edit that is silently
//! never saved. The write has to be MOVED, not dropped.
//!
//! The candidate mechanism is that `document::eval` is not special: `dioxus_document::eval` is
//! `document().eval(..)`, and `document()` is a `try_consume_context::<Rc<dyn Document>>()`
//! (`dioxus-document-0.7.9/src/lib.rs:14`). An `Rc` is an `Rc`, every webview shares the main
//! thread, so the first window can stash its handle where any dom can reach it — the same
//! argument POP1 made for signals, applied to the document instead of the arena.
//!
//! Two questions, and the second is the one that makes it a fix rather than a trick:
//!
//! 1. Does a script B evaluates through A's handle run in A's store?
//! 2. Does A keep writing normally afterward? A relayed write is only worth having if it leaves
//!    A's store healthy — if the relay poisons A the way B's own write does, the handle changed
//!    nothing and the answer has to be a different store entirely.
//!
//! B makes NO direct write of its own here, deliberately: `pop3_storage_origin` already measured
//! that path, and a direct write in this run would poison A's store and take question 2 with it.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop3_owner_doc`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop3_owner_doc is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::document::Document;
    use dioxus::prelude::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::OnceLock;
    use std::time::Duration;

    thread_local! {
        /// Window A's document handle, as the real thing would hold the main window's.
        static OWNER_DOC: RefCell<Option<Rc<dyn Document>>> = const { RefCell::new(None) };
    }

    fn nonce() -> &'static str {
        static NONCE: OnceLock<String> = OnceLock::new();
        NONCE.get_or_init(|| {
            format!(
                "run-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or_default()
            )
        })
    }

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP3 owner doc — window A")
                        .with_inner_size(LogicalSize::new(460.0, 220.0)),
                ),
            )
            .launch(WindowA);
    }

    async fn ask(js: &str) -> String {
        match document::eval(js).await {
            Ok(v) => v
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string()),
            Err(e) => format!("eval failed: {e:?}"),
        }
    }

    fn verdict(claim: &str, holds: bool) {
        println!("[verdict] {claim} .... {holds}");
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        // Claimed from an EFFECT, exactly as the shell root claims it, and the timing is the
        // point. `dioxus::document::document()` falls back to a silent no-op document when it
        // finds none in context, so a claim made too early would leave the app with an owner that
        // discards every write handed to it — and nothing would say so. An effect runs after the
        // first render, when the renderer's document is certainly there; this spike is what says
        // that is true rather than plausible.
        use_effect(|| {
            OWNER_DOC.with_borrow_mut(|slot| *slot = Some(dioxus::document::document()));
        });

        use_future(move || async move {
            let nonce = nonce();
            println!("[A] nonce .... {nonce}");
            ask("localStorage.removeItem('pop3-relayed');\
                 localStorage.removeItem('pop3-after'); return 'cleared';")
            .await;

            dioxus::desktop::window().new_window(
                VirtualDom::new(WindowB),
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP3 owner doc — window B")
                        .with_inner_size(LogicalSize::new(460.0, 220.0)),
                ),
            );

            // B relays at ~600ms.
            tokio::time::sleep(Duration::from_millis(1400)).await;
            let relayed = ask("return localStorage.getItem('pop3-relayed');").await;
            println!("[A] reads pop3-relayed .... {relayed}");
            verdict(
                "B's relayed write landed in A's store",
                relayed == format!("{nonce}-relayed-by-B"),
            );

            // Question 2: A's own store still works after the relay.
            ask(&format!(
                "localStorage.setItem('pop3-after', '{nonce}-after'); return 'ok';"
            ))
            .await;
            tokio::time::sleep(Duration::from_millis(700)).await;
            let after = ask("return localStorage.getItem('pop3-after');").await;
            println!("[A] reads pop3-after .... {after}");
            verdict(
                "A still writes its own store after a relayed write",
                after == format!("{nonce}-after"),
            );
            std::process::exit(0);
        });

        rsx! { div { style: "font: 14px system-ui; padding: 12px", "Window A (owns persistence)" } }
    }

    #[allow(non_snake_case)]
    fn WindowB() -> Element {
        use_future(move || async move {
            let nonce = nonce();
            tokio::time::sleep(Duration::from_millis(600)).await;

            // The relay, fire-and-forget exactly as every `persist_*` in the app is.
            let doc = OWNER_DOC.with_borrow(|slot| slot.clone());
            match doc {
                Some(doc) => {
                    doc.eval(format!(
                        "try {{ localStorage.setItem('pop3-relayed', '{nonce}-relayed-by-B'); }} catch (_) {{}}"
                    ));
                    println!("[B] relayed a write through A's document handle");
                }
                None => {
                    println!("[B] no owner document was claimed — the mechanism is not even wired")
                }
            }

            // What B's OWN store thinks, to show the relay did not also write here.
            tokio::time::sleep(Duration::from_millis(400)).await;
            println!(
                "[B] its own store's pop3-relayed .... {}",
                ask("return localStorage.getItem('pop3-relayed');").await
            );
        });

        rsx! { div { style: "font: 14px system-ui; padding: 12px", "Window B (the popped-out window)" } }
    }
}
