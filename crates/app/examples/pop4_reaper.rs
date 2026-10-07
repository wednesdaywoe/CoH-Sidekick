//! POP4 — does closing the registered pop-out windows actually let the app exit?
//!
//! The reaper has two links, and only one of them a process can drive:
//!
//! 1. the MAIN window's `CloseRequested` reaches a handler registered in the main window —
//!    certain from [`pop4_event_reach`](pop4_event_reach.rs), which measured that `apply_event`
//!    matches a handler's own id and nothing else;
//! 2. `close_window` on each registered id then lets the process exit ON ITS OWN.
//!
//! This is (2), and it is worth a key because the whole bug was an app that would not quit. If
//! removing the popped webview does not reach `exit_on_last_window_close`, the reaper fires, the
//! orphan window disappears, and the process STILL hangs — a fix that looks like it worked.
//!
//! Nothing here calls `process::exit`. That is the point: the verdict is whether the event loop
//! ends by itself, so the harness's own timeout is the failing case. A run that prints
//! `[driver] closed everything` and then hangs is link (2) broken.
//!
//! Link (1) stays the click-through's, because no API produces a `CloseRequested` — see
//! [`pop4_close_doors`](pop4_close_doors.rs).
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop4_reaper`
//! Expect exit 0 within a couple of seconds; a timeout is the failure.

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop4_reaper is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::tao::window::WindowId;
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::cell::RefCell;
    use std::time::Duration;

    thread_local! {
        /// Stands in for `panel_popout`'s `OPEN_WINDOWS` — the same shape, populated the same
        /// way, read the same way.
        static OPEN_WINDOWS: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
    }

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP4 reaper — window A (main)")
                        .with_inner_size(LogicalSize::new(560.0, 220.0)),
                ),
            )
            .launch(WindowA);
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(600)).await;

            for n in 1..=2 {
                let opened = dioxus::desktop::window()
                    .new_window(
                        VirtualDom::new(Popped),
                        Config::new().with_window(
                            WindowBuilder::new()
                                .with_title(format!("POP4 reaper — popped {n}"))
                                .with_inner_size(LogicalSize::new(320.0, 200.0)),
                        ),
                    )
                    .resolve()
                    .await;
                let id = opened.id();
                OPEN_WINDOWS.with_borrow_mut(|open| open.push(id));
                println!("[driver] opened popped window {n}");
            }

            tokio::time::sleep(Duration::from_millis(600)).await;

            // What the reaper does, in the order the real one causes: the popped windows are
            // asked to close, and the main window goes too.
            let desktop = dioxus::desktop::window();
            for id in OPEN_WINDOWS.with_borrow(|open| open.clone()) {
                desktop.close_window(id);
            }
            desktop.close();
            println!("[driver] closed everything — the loop must now end on its own");
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window A (main)" }
                p { "see the terminal" }
            }
        }
    }

    #[allow(non_snake_case)]
    fn Popped() -> Element {
        rsx! {
            div { style: "font: 14px system-ui; padding: 16px", h3 { "popped" } }
        }
    }
}
