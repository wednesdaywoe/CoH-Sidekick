//! POP3 — do two webviews share one localStorage, and what does the next launch read?
//!
//! The stream costed POP3 against "N webviews writing one `sk-layout` key is a race" and left the
//! single-writer decision to this item. That sentence assumes the two windows reach the SAME
//! store, which nothing in the stream ever checked — and the source argues against it:
//! `dioxus-desktop` builds a fresh `wry::WebContext` per webview (`webview.rs:260`), and on Linux
//! a `None` data directory means `WebContext::builder().build()`, a distinct `WebKitWebContext`
//! with its own storage process, twice over the same default data directory.
//!
//! Four questions, in the order they matter:
//!
//! 1. Does A read back its own key across two evals? Without this the rest is an instrument
//!    reading. The first run of this file said "B sees nothing A wrote" on a cold store where
//!    nothing anywhere was readable.
//! 2. Does B, created after A wrote, read A's value?
//! 3. Does A read a value B wrote after B existed? (The direction a dock takes.)
//! 4. What does a SECOND PROCESS read afterward — which is the launch POP3 restores from, and
//!    the only place a lost update is visible.
//! 5. Does A keep a key it wrote WHILE B was alive? Split by whether B writes at all
//!    (`POP3_B_WRITES=0`), because "B's writes clobber A" and "a second WebContext existing
//!    clobbers A" are different defects with different fixes: the first is settled by a
//!    single-writer rule, the second is not settled by anything short of moving the store.
//!
//! **Every probe value carries a per-run nonce**, because the answer to 2 is otherwise
//! indistinguishable from B reading the last run's leftovers off disk. That is not hypothetical:
//! with a fixed `"written-by-A"` this file graded `B sees what A wrote .... true` twice running,
//! and the value B actually held was the previous run's.
//!
//! Phase 3 additionally overlaps a read-modify-write in both windows, holding the value for 400ms
//! between the read and the write. `persist_desktop` has no await inside its eval, so this does
//! not measure the real app's odds — it measures whether the mechanism can lose an update at all,
//! which is the half a design decision needs.
//!
//! **Answered 2026-09-15, and the stream's premise was wrong in the direction that matters.**
//! There is no shared store to race over. A second webview's localStorage is a SEPARATE store,
//! seeded from disk when that webview is created: B never read a value A wrote this run, in
//! either direction, on any of six runs. The origin string is identical (`dioxus://index.html`),
//! so this is two storage backends behind one origin, not two origins.
//!
//! The consequence is worse than a lost update, and it is why single-writer is the whole answer:
//!
//! ```text
//! B writes:  A keeps its own write made while B was alive .... false   (x2 runs)
//! B reads:   A keeps its own write made while B was alive .... true    (x2 runs)
//! ```
//!
//! **The moment the second webview writes, the first webview's later writes stop landing** — not
//! merged wrong, gone, from its own in-memory store and from the disk the next launch reads.
//! Writes A made BEFORE B's first write survive. The control is what makes this a finding rather
//! than a guess: B doing nothing but reads leaves A intact, twice.
//!
//! So a popped-out window must never write persisted state, and the defect that rule prevents is
//! not "the dock is not saved" but "nothing the main window does after a dock is saved".
//!
//! No `asset!` here, so the `cargo run` rule the other spikes carry does not bite. Run:
//! `cargo run -p app --release --features desktop,census-probe --example pop3_storage_origin`
//! then `POP3_PHASE=read cargo run … --example pop3_storage_origin` for question 4.

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop3_storage_origin is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::sync::OnceLock;
    use std::time::Duration;

    /// Whether the popped-out stand-in writes at all — see question 5.
    fn b_writes() -> bool {
        std::env::var("POP3_B_WRITES").as_deref() != Ok("0")
    }

    /// This run's stamp, so no probe can be satisfied by a value left on disk by the last one.
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

    /// Launched with no data directory, exactly as `main.rs` does — the default context is the
    /// configuration under test, not a convenience.
    pub fn run() {
        let reading = std::env::var("POP3_PHASE").as_deref() == Ok("read");
        let title = if reading {
            "POP3 storage — second process"
        } else {
            "POP3 storage — window A"
        };
        let root = if reading { SecondProcess } else { WindowA };
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title(title)
                        .with_inner_size(LogicalSize::new(460.0, 220.0)),
                ),
            )
            .launch(root);
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

    async fn get(key: &str) -> String {
        ask(&format!("return localStorage.getItem('{key}');")).await
    }

    fn verdict(claim: &str, holds: bool) {
        println!("[verdict] {claim} .... {holds}");
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        use_future(move || async move {
            let nonce = nonce();
            println!("[A] nonce .... {nonce}");
            ask(
                "localStorage.removeItem('pop3-a'); localStorage.removeItem('pop3-b');\
                 localStorage.removeItem('pop3-rmw'); localStorage.removeItem('pop3-solo');\
                 return 'cleared';",
            )
            .await;
            ask(&format!(
                "localStorage.setItem('pop3-a', '{nonce}-from-A'); return 'ok';"
            ))
            .await;

            // Question 1, and the spike is worthless without it.
            let own = get("pop3-a").await;
            println!("[A] reads its own pop3-a .... {own}");
            verdict(
                "A's own write survives its own next eval",
                own == format!("{nonce}-from-A"),
            );
            println!("[A] origin .... {}", ask("return location.origin;").await);

            dioxus::desktop::window().new_window(
                VirtualDom::new(WindowB),
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP3 storage — window B")
                        .with_inner_size(LogicalSize::new(460.0, 220.0)),
                ),
            );

            // Both windows enter their read-modify-write at about t=1200ms and hold the value for
            // 400ms before writing it back, so the two are certain to overlap.
            tokio::time::sleep(Duration::from_millis(1200)).await;
            let wrote = ask(&format!(
                "let doc = {{}}; try {{ doc = JSON.parse(localStorage.getItem('pop3-rmw')) || {{}}; }} catch (_) {{}}\
                 await new Promise((r) => setTimeout(r, 400));\
                 doc.fromA = '{nonce}';\
                 localStorage.setItem('pop3-rmw', JSON.stringify(doc));\
                 return JSON.stringify(doc);"
            ))
            .await;
            println!("[A] rmw wrote .... {wrote}");

            // Question 5: written after B exists, and read back at the end.
            ask(&format!(
                "localStorage.setItem('pop3-solo', '{nonce}-from-A-with-B-open'); return 'ok';"
            ))
            .await;

            tokio::time::sleep(Duration::from_millis(900)).await;
            let solo = get("pop3-solo").await;
            println!("[A] reads pop3-solo .... {solo}");
            verdict(
                &format!(
                    "A keeps its own write made while B was alive (B writes: {})",
                    b_writes()
                ),
                solo == format!("{nonce}-from-A-with-B-open"),
            );

            let saw_b = get("pop3-b").await;
            println!("[A] reads pop3-b .... {saw_b}");
            verdict(
                "A sees a write B made after B existed",
                saw_b == format!("{nonce}-from-B"),
            );

            let rmw = get("pop3-rmw").await;
            println!("[A] reads pop3-rmw .... {rmw}");
            verdict(
                "both windows' RMW fields survived, in A",
                rmw.contains("fromA") && rmw.contains("fromB"),
            );
            std::process::exit(0);
        });

        rsx! { div { style: "font: 14px system-ui; padding: 12px", "Window A (stands in for the main window)" } }
    }

    #[allow(non_snake_case)]
    fn WindowB() -> Element {
        use_future(move || async move {
            let nonce = nonce();
            tokio::time::sleep(Duration::from_millis(600)).await;
            let saw_a = get("pop3-a").await;
            println!("[B] reads pop3-a .... {saw_a}");
            verdict(
                "B sees a write A made before B existed",
                saw_a == format!("{nonce}-from-A"),
            );
            println!("[B] origin .... {}", ask("return location.origin;").await);

            if !b_writes() {
                println!("[B] POP3_B_WRITES=0 — B touches localStorage for reads only");
                return;
            }
            ask(&format!(
                "localStorage.setItem('pop3-b', '{nonce}-from-B'); return 'ok';"
            ))
            .await;

            tokio::time::sleep(Duration::from_millis(500)).await;
            let wrote = ask(&format!(
                "let doc = {{}}; try {{ doc = JSON.parse(localStorage.getItem('pop3-rmw')) || {{}}; }} catch (_) {{}}\
                 await new Promise((r) => setTimeout(r, 400));\
                 doc.fromB = '{nonce}';\
                 localStorage.setItem('pop3-rmw', JSON.stringify(doc));\
                 return JSON.stringify(doc);"
            ))
            .await;
            println!("[B] rmw wrote .... {wrote}");
        });

        rsx! { div { style: "font: 14px system-ui; padding: 12px", "Window B (stands in for the popped-out window)" } }
    }

    /// Question 4: one window, no writes, reporting what the app's own launch would read. The
    /// nonce is this process's and so matches nothing — the values are read for their CONTENT,
    /// which carries the writing run's stamp.
    #[allow(non_snake_case)]
    fn SecondProcess() -> Element {
        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(600)).await;
            println!("[next launch] pop3-a .... {}", get("pop3-a").await);
            println!("[next launch] pop3-b .... {}", get("pop3-b").await);
            println!("[next launch] pop3-rmw .... {}", get("pop3-rmw").await);
            println!("[next launch] pop3-solo .... {}", get("pop3-solo").await);
            std::process::exit(0);
        });

        rsx! { div { style: "font: 14px system-ui; padding: 12px", "Second process" } }
    }
}
