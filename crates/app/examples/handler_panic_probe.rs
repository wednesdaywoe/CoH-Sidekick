//! F18 — is a panic inside a desktop event handler uncaught, and does it take the app down?
//!
//! The row's premise is that `build_io.rs:317`'s handler catches nothing, so the next reachable
//! decode panic arrives the same way its two concrete triggers (F17, F16) did. Both triggers are
//! closed and the row stays open, because what is open is the HANDLER, not either trigger. Every
//! trigger so far was found by reading; this measures the shape they all share.
//!
//! **The event is real, not simulated.** `.click()` from JS dispatches a DOM event that
//! dioxus-desktop carries back over the same synchronous XHR every user click uses
//! (`handleVirtualdomEventSync` to `dioxus://index.html//__events`), so the panic happens where a
//! user's panic would. Calling the Rust closure directly would prove nothing about the handler
//! boundary, which is the only thing this row is about.
//!
//! **The verdict is the exit status**, for the same reason as the F81 probe:
//!
//! | outcome | meaning |
//! |---|---|
//! | dead without a result line | the panic was uncaught - F18's premise holds |
//! | exit 0 with SURVIVED | something caught it; the premise needs revising |
//! | exit 1 | watchdog - no answer, no verdict |
//!
//! ```sh
//! cargo run -p app --features desktop,census-probe --example handler_panic_probe
//! ```

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("handler_panic_probe is a desktop probe - run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::io::Write;
    use std::time::Duration;

    pub fn run() {
        println!("\n=== F18 handler-panic probe ===");
        println!("driving a real click into a handler that panics\n");
        let _ = std::io::stdout().flush();

        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_secs(25));
            println!("=== F18 PROBE: NO ANSWER ===\nNo reply and no death within 25s.\n");
            let _ = std::io::stdout().flush();
            std::process::exit(1);
        });

        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("F18 handler panic probe")
                        .with_inner_size(LogicalSize::new(560.0, 320.0)),
                ),
            )
            .launch(Probe);
    }

    /// The panic lives behind a `()`-returning fn rather than in the closure body: a closure
    /// whose tail is `!` trips never-type fallback under edition 2024. The handler boundary the
    /// row is about is unchanged by the indirection.
    fn boom() {
        println!("--- handler entered; panicking ---");
        let _ = std::io::stdout().flush();
        panic!("F18 probe: synthetic panic inside a desktop event handler");
    }

    #[allow(non_snake_case)]
    fn Probe() -> Element {
        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(400)).await;

            println!("--- dispatching the click now ---");
            let _ = std::io::stdout().flush();

            let r =
                document::eval(r#"document.getElementById("boom").click(); return "dispatched";"#)
                    .await;
            println!("eval returned: {r:?}");

            // Reached only if the handler's panic did not end the process.
            tokio::time::sleep(Duration::from_millis(1200)).await;
            println!(
                "=== VERDICT: SURVIVED ===\n\
                 The handler panicked and this process is still running, so something caught\n\
                 it. F18's premise would need revising.\n"
            );
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        });

        rsx! {
            div { style: "padding:12px;font:13px system-ui",
                h3 { "F18 handler panic probe" }
                button {
                    id: "boom",
                    onclick: move |_| boom(),
                    "boom"
                }
            }
        }
    }
}
