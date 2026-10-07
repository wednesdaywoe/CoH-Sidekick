//! F86 — does the `dioxus://` asset protocol serve files outside the bundle?
//!
//! The row survived verification and was never reproduced against a running build, which is the
//! only thing that settles it. `dioxus-asset-resolver-0.7.9/src/native.rs` decides confinement
//! like this:
//!
//! ```ignore
//! if !uri_path.exists() || uri_path.starts_with("/assets/") { uri_path = bundle_root.join(..) }
//! uri_path.exists().then_some(uri_path)
//! ```
//!
//! So a decoded path that ALREADY exists and does not begin `/assets/` is served from wherever it
//! is on disk — the bundle root is consulted only when the literal path missed. `into_response`
//! then attaches `Access-Control-Allow-Origin: *`.
//!
//! **Measured by outcome, never by reading the resolver back.** A path is confined if the fetch
//! 404s and unconfined if bytes come back, and for the planted file the bytes must carry a
//! sentinel this process wrote — so "it served something" cannot be confused with "it served the
//! file we meant". `/etc/hostname` is fetched alongside it because a planted file alone invites
//! the objection that the probe created its own result.
//!
//! **The URLs are RELATIVE on purpose.** They resolve against the document's `dioxus://index.html/`
//! base, which is the same resolution a stranger's relative `img src` gets — F81's stated
//! mechanism, and the reason these two rows share a build.
//!
//! **It runs under the SHIPPED CSP by default**, read from the same `desktop-csp.txt` the app
//! reads, because a finding about what ships must be graded against what ships. That policy is
//! not expected to help: `connect-src 'self' dioxus:` makes every one of these targets
//! same-origin. `argv[1] == "nocsp"` drops it, which separates "the resolver refused" from "the
//! policy refused" if the default ever comes back clean.
//!
//! ```sh
//! cargo run -p app --features desktop,census-probe --example asset_scope_probe
//! cargo run -p app --features desktop,census-probe --example asset_scope_probe -- nocsp
//! ```
//!
//! **Run it through `cargo run`, never as the built binary** — manganis decides at runtime
//! whether the app is bundled by reading `CARGO_MANIFEST_DIR`, and by hand every `asset!`
//! collapses to a placeholder. The rule cost POP2 a run and it bites an example identically.

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("asset_scope_probe is a desktop probe - run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::sync::OnceLock;
    use std::time::Duration;

    /// `launch` takes a plain `fn() -> Element`, so the probe's context cannot be passed as an
    /// argument and travels here instead.
    static CTX: OnceLock<(String, String, String)> = OnceLock::new();

    const SHIPPED_CSP: &str = include_str!("../src/desktop-csp.txt");

    /// Restated rather than imported: `app` is a bin-only crate, so an example cannot reach into
    /// it. Same reason csp_probe, POP1 and POP2 carry their own copies.
    static APP_CSS: Asset = asset!("/assets/app.css");

    /// Written by this process, outside the bundle, and never referenced by the app. If these
    /// bytes come back over `dioxus://`, the protocol read a file it has no business reaching.
    fn plant_marker() -> (std::path::PathBuf, String) {
        let pid = std::process::id();
        let sentinel = format!("F86-SENTINEL-{pid}-CONFINEMENT-BROKEN");
        let path = std::env::temp_dir().join(format!("f86-probe-{pid}.txt"));
        std::fs::write(&path, &sentinel).expect("probe could not plant its marker file");
        (path, sentinel)
    }

    /// A second planted file, with an extension the resolver does not know. `get_mime_from_ext`
    /// ends `Some(_) => "text/html; charset=utf-8"`, so an unrecognised extension is declared
    /// HTML - and `dioxus://index.html` is the app's own origin. This measures whether arbitrary
    /// bytes on disk are served as a same-origin DOCUMENT rather than merely disclosed.
    fn plant_html() -> std::path::PathBuf {
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("f86-probe-{pid}.log"));
        std::fs::write(
            &path,
            "<html><body><script>/*F86*/</script>planted</body></html>",
        )
        .expect("probe could not plant its html file");
        path
    }

    pub fn run() {
        let no_csp = std::env::args().nth(1).as_deref() == Some("nocsp");
        let (marker_path, sentinel) = plant_marker();
        let html_path = plant_html();

        println!("\n=== F86 asset-scope probe ===");
        println!("planted: {}", marker_path.display());
        println!("planted: {}", html_path.display());
        println!("sentinel: {sentinel}");
        println!(
            "csp: {}\n",
            if no_csp {
                "DROPPED (argv nocsp)"
            } else {
                "shipped"
            }
        );

        let marker_url = marker_path.to_string_lossy().to_string();
        let sentinel_for_js = sentinel.clone();

        // Same watchdog rationale as csp_probe: if the webview never draws, no future inside the
        // component is ever polled, so the case worth catching is the one an in-runtime timer
        // sleeps through.
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(25));
            println!(
                "=== F86 PROBE: NO ANSWER ===\n\
                 The webview never reported within 25s. That is not a verdict on confinement.\n"
            );
            std::process::exit(1);
        });

        let mut cfg = Config::new().with_window(
            WindowBuilder::new()
                .with_title("F86 asset scope probe")
                .with_inner_size(LogicalSize::new(560.0, 320.0)),
        );
        if !no_csp {
            cfg = cfg.with_custom_head(format!(
                r#"<meta http-equiv="Content-Security-Policy" content="{}">"#,
                SHIPPED_CSP.trim()
            ));
        }

        CTX.set((
            marker_url,
            sentinel_for_js,
            html_path.to_string_lossy().to_string(),
        ))
        .expect("probe context set twice");

        dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(Probe);
    }

    #[allow(non_snake_case)]
    fn Probe() -> Element {
        let (marker_url, sentinel, html_url) = CTX.get().expect("probe context unset");

        use_future({
            let marker_url = marker_url.clone();
            let sentinel = sentinel.clone();
            let html_url = html_url.clone();
            move || {
                let marker_url = marker_url.clone();
                let sentinel = sentinel.clone();
                let html_url = html_url.clone();
                async move {
                    tokio::time::sleep(Duration::from_millis(400)).await;

                    let js = probe_js(&marker_url, &html_url);
                    match document::eval(&js).await {
                        Ok(v) => {
                            let text = v
                                .as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| v.to_string());
                            println!("=== F86 PROBE RESULT ===\n{text}\n");
                            let reproduced = text.contains(&sentinel);
                            if reproduced {
                                println!(
                                    "=== VERDICT: F86 REPRODUCED ===\n\
                                     A file this process wrote outside the bundle came back over\n\
                                     `dioxus://`, sentinel intact. The protocol is not confined to\n\
                                     the asset root.\n"
                                );
                            } else {
                                println!(
                                    "=== VERDICT: NOT REPRODUCED ===\n\
                                     The planted file did not come back. Read the rows above\n\
                                     before calling this confinement - a 404 on the positive\n\
                                     control means the probe measured nothing.\n"
                                );
                            }
                        }
                        Err(e) => println!("=== F86 PROBE FAILED ===\neval failed: {e:?}\n"),
                    }
                    let _ = std::fs::remove_file(&marker_url);
                    let _ = std::fs::remove_file(&html_url);
                    dioxus::desktop::window().close();
                    std::process::exit(0);
                }
            }
        });

        rsx! {
            // The positive control fetches THIS sheet's resolved href. A literal
            // "/assets/app.css" is not a real url - manganis content-hashes the filename - and
            // the first run of this probe 404'd its own control for exactly that reason.
            document::Stylesheet { href: APP_CSS }
            div { style: "padding:12px;font:13px system-ui",
                h3 { "F86 asset scope probe" }
                p { "Fetching over the dioxus:// scheme; result goes to stdout." }
            }
        }
    }

    /// Four targets, and the two controls are what make the other two mean anything.
    fn probe_js(marker_url: &str, html_url: &str) -> String {
        let nonce = std::process::id();
        format!(
            r#"
            // The control is the sheet the page actually loaded, read back from the DOM, so it
            // is a url the bundle really serves rather than one the probe guessed.
            const sheet = document.querySelector('link[rel="stylesheet"]');
            const controlUrl = sheet ? sheet.getAttribute("href") : "/assets/app.css";

            const targets = [
              {{ name: "control-bundle-asset", url: controlUrl, expect: "serve" }},
              {{ name: "control-absent", url: "/no-such-path-{nonce}", expect: "404" }},
              {{ name: "planted-outside-bundle", url: "{marker_url}", expect: "404 if confined" }},
              {{ name: "planted-unknown-ext", url: "{html_url}", expect: "404 if confined" }},
              {{ name: "etc-hostname", url: "/etc/hostname", expect: "404 if confined" }}
            ];
            const out = [];
            for (const t of targets) {{
              try {{
                const r = await fetch(t.url);
                const body = await r.text();
                out.push({{
                  name: t.name, url: t.url, expect: t.expect,
                  status: r.status, bytes: body.length,
                  acao: r.headers.get("access-control-allow-origin"),
                  ctype: r.headers.get("content-type"),
                  head: body.slice(0, 80)
                }});
              }} catch (e) {{
                out.push({{ name: t.name, url: t.url, expect: t.expect, error: String(e) }});
              }}
            }}
            return JSON.stringify(out, null, 2);
            "#
        )
    }
}
