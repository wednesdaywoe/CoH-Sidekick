//! F28 — does the shipped desktop CSP let the app's own document work?
//!
//! A browser probe loads the same policy into Chromium and WebKit and measures what
//! it permits. It cannot measure the thing this app actually depends on, because it never runs
//! `dioxus-desktop`'s interpreter: **every DOM edit arrives over a loopback WebSocket**
//! (`edits.rs:95` builds `ws://127.0.0.1:{port}/{id}/{key}`, `native.ts:460` opens it) and
//! **every user event is an XHR POST** to `dioxus://index.html//__events`
//! (`handleVirtualdomEventSync`). Both are `connect-src`, in every profile — not just under
//! `dx serve`. A policy that closes `connect-src` renders the app permanently blank and inert.
//!
//! It also answers the question no browser can: whether `'self'` resolves to
//! `dioxus://index.html/`, the custom scheme every stylesheet, font and icon is served from on
//! this platform (`protocol.rs:15-22`).
//!
//! Answers are by OUTCOME, never by reading the policy back: a stylesheet that loaded has a
//! non-zero `cssRules.length`, a font that loaded has `status: "loaded"`, an icon that loaded has
//! a non-zero `naturalWidth`, and an event that completed its round trip has put a marker in the
//! DOM that only Rust can render. A violation listener runs alongside as a corroborating witness
//! — it only catches what is blocked after it is installed, and the stylesheets are already in
//! flight by then. The outcome checks have no such window.
//!
//! The computed COLOUR of a themed node is deliberately not the style oracle. `.stat-value` reads
//! `color: var(--stat-hue, var(--text))`, and both variables come from a theme that a bare probe
//! never activates, so it computes to black whether or not `app.css` loaded — POP2 reported
//! exactly that. `cssRules.length` per sheet cannot be fooled that way.
//!
//! **It grades the SHIPPED policy verbatim**, read from the same `desktop-csp.txt` that
//! `main.rs::desktop_csp` reads, with no debug-profile substitution — the shipped binary is the
//! one the row is about.
//!
//! **Mutation lever.** `argv[1]`, if present, replaces the policy. That is how this probe was
//! shown able to go red in both directions, which is the only reason its green means anything:
//!
//! ```sh
//! cargo run -p app --features desktop,census-probe --example csp_probe            # the shipped policy
//! cargo run -p app --features desktop,census-probe --example csp_probe -- "default-src 'none'"
//! ```
//!
//! A policy that blocks the edits socket produces no answer at all rather than a wrong one, so a
//! watchdog turns that silence into a verdict and a non-zero exit. Without it, the failure this
//! probe exists to catch is indistinguishable from a hung probe.
//!
//! **Run it through `cargo run`, never as `target/debug/examples/csp_probe`.** Manganis decides
//! at runtime whether the app is bundled by reading `CARGO_MANIFEST_DIR`; run by hand, every
//! `asset!` collapses to the same placeholder string and this probe measures nothing. The README
//! states the rule for the app and it bites an example identically — it cost POP2 a run.

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("csp_probe is a desktop probe — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::time::Duration;

    /// The same file `main.rs::desktop_csp` reads. One file means this measures the policy that
    /// ships rather than a copy that agreed with it the day it was written.
    const SHIPPED_CSP: &str = include_str!("../src/desktop-csp.txt");

    // Restated rather than imported: `app` is a bin-only crate, so an example cannot reach into
    // it (the same reason POP1 and POP2 carry their own copies). These point at the same files.
    static TOKENS_CSS: Asset = asset!("/assets/tokens.css");
    static APP_CSS: Asset = asset!("/assets/app.css");
    static THEME_ASTORIA_CSS: Asset = asset!("/assets/themes/astoria.css");
    static THEME_SIDEKICK_CSS: Asset = asset!("/assets/themes/sidekick.css");

    static FONT_SAIRA: Asset =
        asset!("/assets/fonts/Saira-memjYa2wxmKQyPMrZX79wwYZQMhsyuSLiIvS.woff2");
    static FONT_DM_MONO_400: Asset = asset!("/assets/fonts/DMMono-aFTU7PB1QTsUX8KYthqQBA.woff2");
    static FONT_DM_MONO_500: Asset =
        asset!("/assets/fonts/DMMono-aFTR7PB1QTsUX8KYvumzEYOtbQ.woff2");
    static FONT_SN_PRO: Asset = asset!("/assets/fonts/SNPro-NGS1v5zWIAwPIq7hapRO.woff2");
    static FONT_NUNITO: Asset = asset!("/assets/fonts/Nunito-XRXV3I6Li01BKofINeaB.woff2");

    /// A single-file asset, as `shell.rs` declares the brand mark.
    static BRAND_ICON: Asset = asset!("/assets/img/favicon-64x64.png");
    /// The whole image tree as one folder asset, as `view/icons.rs` declares it. Both shapes are
    /// probed because they resolve differently, and the app ships both.
    static IMG_DIR: Asset = asset!("/assets/img");

    fn font_faces() -> String {
        format!(
            "@font-face{{font-family:'Saira';font-style:normal;font-weight:100 900;font-display:swap;src:url({FONT_SAIRA}) format('woff2');}}\
             @font-face{{font-family:'DM Mono';font-style:normal;font-weight:400;font-display:swap;src:url({FONT_DM_MONO_400}) format('woff2');}}\
             @font-face{{font-family:'DM Mono';font-style:normal;font-weight:500;font-display:swap;src:url({FONT_DM_MONO_500}) format('woff2');}}\
             @font-face{{font-family:'SN Pro';font-style:normal;font-weight:200 900;font-display:swap;src:url({FONT_SN_PRO}) format('woff2');}}\
             @font-face{{font-family:'Nunito';font-style:normal;font-weight:200 1000;font-display:swap;src:url({FONT_NUNITO}) format('woff2');}}"
        )
    }

    /// Installed at first render, before any probe runs. Corroboration only — see the module doc.
    const LISTEN_JS: &str = "window.__violations = [];\
        document.addEventListener('securitypolicyviolation', function (e) {\
            window.__violations.push(e.effectiveDirective + ' blocked ' + String(e.blockedURI).slice(0, 90));\
        });\
        return 'listening';";

    /// A 1x1 PNG, used for the `data:` and `blob:` arms of `img-src`.
    const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

    fn probe_js() -> String {
        format!(
            "const out = {{}};\
             out.sheets = Array.from(document.querySelectorAll('link[rel=stylesheet]')).map(function (l) {{\
               const name = String(l.href).split('/').pop();\
               try {{ return name + '=' + (l.sheet ? l.sheet.cssRules.length + ' rules' : 'NOT LOADED'); }}\
               catch (e) {{ return name + '=opaque'; }}\
             }});\
             out.appCssApplied = getComputedStyle(document.querySelector('.stat-value')).fontVariantNumeric;\
             try {{\
               await document.fonts.load('12px Saira');\
               await document.fonts.load('12px \"DM Mono\"');\
               await document.fonts.load('500 12px \"DM Mono\"');\
               await document.fonts.load('12px \"SN Pro\"');\
               await document.fonts.load('12px Nunito');\
             }} catch (e) {{ out.fontLoadThrew = String(e); }}\
             try {{ await document.fonts.ready; }} catch (e) {{}}\
             out.fonts = Array.from(document.fonts).map(function (f) {{ return f.family + '=' + f.status; }});\
             function imgState(id) {{ const i = document.getElementById(id); return i ? (i.naturalWidth > 0 ? 'loaded ' + i.naturalWidth + 'px' : 'BLANK') : 'missing'; }}\
             out.brandIcon = imgState('brand');\
             out.treeIcon = imgState('tree');\
             function tryImg(src) {{ return new Promise(function (r) {{ const i = new Image(); i.onload = function () {{ r('loaded'); }}; i.onerror = function () {{ r('blocked'); }}; i.src = src; }}); }}\
             out.dataImage = await tryImg('data:image/png;base64,{PNG_B64}');\
             const bytes = Uint8Array.from(atob('{PNG_B64}'), function (c) {{ return c.charCodeAt(0); }});\
             out.blobImage = await tryImg(URL.createObjectURL(new Blob([bytes], {{ type: 'image/png' }})));\
             try {{\
               const AF = Object.getPrototypeOf(async function () {{}}).constructor;\
               out.asyncFunction = await new AF('return 41 + 1')();\
             }} catch (e) {{ out.asyncFunction = 'BLOCKED: ' + String(e); }}\
             document.getElementById('clicker').click();\
             await new Promise(function (r) {{ setTimeout(r, 600); }});\
             out.eventRoundTrip = document.getElementById('clicked-marker') ? 'ok' : 'NO RESPONSE';\
             const reached = await fetch('https://example.com/', {{ mode: 'no-cors' }}).then(function () {{ return true; }}).catch(function () {{ return false; }});\
             const refused = window.__violations.some(function (v) {{ return v.indexOf('example.com') >= 0; }});\
             out.remoteFetch = reached ? 'ALLOWED' : (refused ? 'refused by connect-src' : 'failed with no violation - INCONCLUSIVE');\
             out.violations = window.__violations;\
             return JSON.stringify(out, null, 1);"
        )
    }

    pub fn run() {
        // argv[1] is the mutation lever; absent, the shipped policy stands.
        let policy = std::env::args()
            .nth(1)
            .unwrap_or_else(|| SHIPPED_CSP.trim().to_string());
        println!("\n=== CSP under test ===\n{policy}\n");

        // The watchdog runs on an OS thread rather than in a `use_future`, and that is the whole
        // point of it. A policy that blocks the edits socket stops the virtual dom being run at
        // all — `edits_in_progress` holds it until the first edit flushes, and it never does — so
        // a future inside the component is never polled, and the case the watchdog exists for is
        // exactly the case an in-runtime watchdog sleeps through. This one cost a 900s harness
        // timeout to find. `process::exit` takes the thread with it on the success path.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(25));
            println!(
                "=== F28 CSP PROBE: NO ANSWER ===\n\
                 The webview never reported back within 25s, which means it never drew a frame.\n\
                 dioxus-desktop delivers every DOM edit over ws://127.0.0.1 and every user event\n\
                 over an XHR to the dioxus origin, and `connect-src` governs both.\n"
            );
            std::process::exit(1);
        });

        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new()
                    .with_window(
                        WindowBuilder::new()
                            .with_title("F28 CSP probe")
                            .with_inner_size(LogicalSize::new(560.0, 320.0)),
                    )
                    .with_custom_head(format!(
                        r#"<meta http-equiv="Content-Security-Policy" content="{policy}">"#
                    )),
            )
            .launch(Probe);
    }

    #[allow(non_snake_case)]
    fn Probe() -> Element {
        // Only Rust can set this, and only in response to an event that completed its XHR round
        // trip. Its marker in the DOM is therefore proof the event channel is open.
        let mut clicked = use_signal(|| false);

        use_future(move || async move {
            // Give the webview a moment to exist before talking to it; POP2 pays the same cost.
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = document::eval(LISTEN_JS).await;
            // Let the stylesheets, fonts and icons settle.
            tokio::time::sleep(Duration::from_millis(1500)).await;

            match document::eval(&probe_js()).await {
                Ok(v) => {
                    let text = v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string());
                    println!("=== F28 CSP PROBE RESULT ===\n{text}\n");
                }
                Err(e) => println!("=== F28 CSP PROBE FAILED ===\neval failed: {e:?}\n"),
            }
            dioxus::desktop::window().close();
            std::process::exit(0);
        });

        rsx! {
            document::Stylesheet { href: TOKENS_CSS }
            document::Stylesheet { href: THEME_ASTORIA_CSS }
            document::Stylesheet { href: THEME_SIDEKICK_CSS }
            document::Stylesheet { href: APP_CSS }
            document::Style { {font_faces()} }
            div { style: "padding: 12px",
                h3 { "F28 CSP probe" }
                div { class: "stats",
                    div { class: "stat-row",
                        span { class: "stat-label", "Probe" }
                        span { class: "stat-value mono", "42" }
                    }
                }
                img { id: "brand", src: BRAND_ICON, width: "32", height: "32" }
                img {
                    id: "tree",
                    src: "{IMG_DIR}/Archetypes/Class_Blaster.png",
                    width: "32",
                    height: "32",
                }
                button { id: "clicker", onclick: move |_| clicked.set(true), "round trip" }
                if clicked() {
                    div { id: "clicked-marker", "event reached Rust" }
                }
            }
        }
    }
}
