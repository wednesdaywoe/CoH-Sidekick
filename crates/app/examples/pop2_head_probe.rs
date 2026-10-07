//! POP2 — does a second webview's `<head>` get the app's stylesheets?
//!
//! POP1 measured the state bridge and explicitly left this: "stylesheet and asset delivery into
//! the second window" was untested, because the spike rendered bare inline-styled markup. The
//! first click-through of POP2 reported the popped-out panel as having "everything, just with no
//! styling" — so this asks the narrow question on its own, with none of the app's state in the
//! way.
//!
//! Window A declares the app's four stylesheets the way the real root does. Window B declares
//! the SAME four at its own root and then reports its head back. If the desktop document's dedup
//! is per-runtime, B's head carries the four links and B is styled; if the set is shared, B's
//! head is empty and every `create_link` was skipped as already-present.
//!
//! The `asset!` declarations are restated here rather than imported: `app` is a bin-only crate,
//! so an example cannot reach into it — the same reason POP1's spike carries its own types. They
//! point at the same files the app ships.
//!
//! Both windows probe, because "B has no links" only means something next to "A has four".
//!
//! **Answered 2026-09-15: a second webview gets the stylesheets.** Both heads carry the same
//! four links, so the dedup set is per-runtime and nothing about POP2 needs a second delivery
//! path. The report that opened this was an artefact of how the app was launched, not of the
//! second window — see below.
//!
//! **Run it through `cargo run`, never as `target/release/examples/pop2_head_probe`.** Manganis
//! decides at runtime whether the app is bundled by reading `CARGO_MANIFEST_DIR`, which `cargo
//! run` sets and a bare exec does not; run by hand, every `asset!` here resolves to the literal
//! string "This should be replaced by dx as part of the build process". That is not a broken
//! window — it is all four stylesheets collapsing to ONE link, because the dedup key is the href
//! and all four hrefs are now the same placeholder. The README states the rule for the app; it
//! bites an example identically, and it cost this probe a run.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop2_head_probe`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop2_head_probe is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::time::Duration;

    static TOKENS_CSS: Asset = asset!("/assets/tokens.css");
    static APP_CSS: Asset = asset!("/assets/app.css");
    static THEME_ASTORIA_CSS: Asset = asset!("/assets/themes/astoria.css");
    static THEME_SIDEKICK_CSS: Asset = asset!("/assets/themes/sidekick.css");

    /// The app's `DocumentAssets`, minus the font faces (a `Style`, not a `Link` — a different
    /// code path, and not the one under suspicion).
    #[allow(non_snake_case)]
    fn DocumentAssets() -> Element {
        rsx! {
            document::Stylesheet { href: TOKENS_CSS }
            document::Stylesheet { href: THEME_ASTORIA_CSS }
            document::Stylesheet { href: THEME_SIDEKICK_CSS }
            document::Stylesheet { href: APP_CSS }
        }
    }

    /// What each window reports about its own document.
    const PROBE_JS: &str = "return JSON.stringify({\
        links: Array.from(document.querySelectorAll('link')).map(l => l.rel + ' ' + l.href),\
        styles: document.querySelectorAll('style').length,\
        bodyFont: getComputedStyle(document.body).fontFamily,\
        probeColor: getComputedStyle(document.querySelector('.stat-value')).color\
    });";

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP2 head probe — window A")
                        .with_inner_size(LogicalSize::new(520.0, 260.0)),
                ),
            )
            .launch(WindowA);
    }

    /// A node the app's own stylesheet styles. If `app.css` reached the window, `.stat-value`
    /// resolves to the themed colour; if it did not, it is the UA default. That is the whole
    /// test, and it is one the link list alone cannot make — a link tag that 404s still shows
    /// up in `querySelectorAll`.
    fn probe_markup() -> Element {
        rsx! {
            div { class: "stats",
                div { class: "stat-row",
                    span { class: "stat-label", "Probe" }
                    span { class: "stat-value mono", "42" }
                }
            }
        }
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        use_future(move || async move {
            // Let A's own head settle before reading it.
            tokio::time::sleep(Duration::from_millis(900)).await;
            report("A", document::eval(PROBE_JS).await);

            let dom = VirtualDom::new(WindowB);
            dioxus::desktop::window().new_window(
                dom,
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP2 head probe — window B")
                        .with_inner_size(LogicalSize::new(520.0, 260.0)),
                ),
            );
        });

        rsx! {
            DocumentAssets {}
            div { style: "font: 14px system-ui; padding: 12px",
                h3 { "Window A" }
                {probe_markup()}
            }
        }
    }

    #[allow(non_snake_case)]
    fn WindowB() -> Element {
        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(1200)).await;
            report("B", document::eval(PROBE_JS).await);
            println!("\n=========== POP2 HEAD PROBE VERDICT ===========");
            println!("B styled == A styled is the claim POP2 needs.");
            println!("===============================================\n");
            dioxus::desktop::window().close();
            std::process::exit(0);
        });

        rsx! {
            DocumentAssets {}
            div { style: "font: 14px system-ui; padding: 12px",
                h3 { "Window B (the popped-out case)" }
                {probe_markup()}
            }
        }
    }

    fn report(tag: &str, result: Result<serde_json::Value, document::EvalError>) {
        match result {
            Ok(v) => {
                let text = v
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string());
                println!("[{tag}] {text}");
            }
            Err(e) => println!("[{tag}] eval failed: {e:?}"),
        }
    }
}
