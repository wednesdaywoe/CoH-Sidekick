//! F81 — does a malformed percent-escape under `dioxus://` really abort the process?
//!
//! `dioxus-asset-resolver-0.7.9/src/native.rs:46` decodes the URI path with
//! `percent_decode_str(path).decode_utf8().expect("expected URL to be UTF-8 encoded")`. `%FF` is
//! not valid UTF-8, so the `expect` fires. It sits inside wry's `extern "C" fn start_task`, a
//! nounwind boundary, so the claim is that it ABORTS rather than unwinds and that `catch_unwind`
//! cannot reach it.
//!
//! **This probe's verdict is whether it survives.** There is no in-process assertion that can
//! grade an abort, so the measurement is the exit status and the caller reads it:
//!
//! | outcome | meaning |
//! |---|---|
//! | killed by SIGABRT (shell reports 134) | F81 reproduced against a running build |
//! | exit 0 with a status line | the request was answered; F81 does not reproduce here |
//! | exit 1 | watchdog - no answer, no verdict |
//!
//! stdout is flushed before the request on purpose: an abort discards whatever is still buffered,
//! and without the flush a reproduction is indistinguishable from a probe that never got started.
//!
//! **The app's own fix is not what this grades.** `cloud::avatar::avatar_src` already refuses
//! everything but an absolute https url on two hosts, so no stranger's `avatar_url` reaches this
//! resolver any more. F81 stays open against the DEPENDENCY, and that is the half measured here:
//! the probe steers straight at the resolver rather than through `avatar_src`.
//!
//! The escape is `argv[1]`, defaulting to `/%FF`, because only one can ever be tested per run -
//! the first that aborts ends the process.
//!
//! ```sh
//! cargo run -p app --features desktop,census-probe --example percent_decode_abort_probe
//! cargo run -p app --features desktop,census-probe --example percent_decode_abort_probe -- /%C0%80
//! ```

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("percent_decode_abort_probe is a desktop probe - run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::io::Write;
    use std::sync::OnceLock;
    use std::time::Duration;

    /// `launch` takes a plain `fn() -> Element`; the target travels here rather than as an arg.
    static TARGET: OnceLock<String> = OnceLock::new();

    const SHIPPED_CSP: &str = include_str!("../src/desktop-csp.txt");

    pub fn run() {
        let target = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "/%FF".to_string());

        println!("\n=== F81 percent-decode abort probe ===");
        println!("target: dioxus://index.html{target}");
        println!("expecting: SIGABRT (134) if the row holds\n");
        let _ = std::io::stdout().flush();

        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(25));
            println!("=== F81 PROBE: NO ANSWER ===\nNo reply and no abort within 25s.\n");
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        });

        let cfg = Config::new()
            .with_window(
                WindowBuilder::new()
                    .with_title("F81 percent-decode abort probe")
                    .with_inner_size(LogicalSize::new(560.0, 320.0)),
            )
            .with_custom_head(format!(
                r#"<meta http-equiv="Content-Security-Policy" content="{}">"#,
                SHIPPED_CSP.trim()
            ));

        TARGET.set(target).expect("probe target set twice");

        dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(Probe);
    }

    #[allow(non_snake_case)]
    fn Probe() -> Element {
        use_future(move || async move {
            let target = TARGET.get().expect("probe target unset").clone();
            tokio::time::sleep(Duration::from_millis(400)).await;

            println!("--- issuing the request now ---");
            let _ = std::io::stdout().flush();

            let js = format!(
                r#"
                try {{
                  const r = await fetch({target:?});
                  const b = await r.text();
                  return "answered status=" + r.status + " bytes=" + b.length;
                }} catch (e) {{
                  return "threw " + String(e);
                }}
                "#
            );

            match document::eval(&js).await {
                Ok(v) => {
                    let text = v
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string());
                    println!("=== F81 PROBE RESULT ===\n{text}");
                    println!(
                        "=== VERDICT: NOT REPRODUCED ===\n\
                         The process is still alive and the request was answered, so this escape\n\
                         did not reach the `expect` on this platform's resolver path.\n"
                    );
                }
                Err(e) => println!("=== F81 PROBE: eval failed ===\n{e:?}\n"),
            }
            let _ = std::io::stdout().flush();
            dioxus::desktop::window().close();
            std::process::exit(0);
        });

        rsx! {
            div { style: "padding:12px;font:13px system-ui",
                h3 { "F81 percent-decode abort probe" }
                p { "If this window vanishes without a result line, that is the finding." }
            }
        }
    }
}
