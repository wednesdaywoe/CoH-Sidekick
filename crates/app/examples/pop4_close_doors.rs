//! POP4 — which of a popped-out window's close doors reach its own close handler.
//!
//! POP2 built the return on `WindowEvent::CloseRequested`, and its click-through proved that
//! door: closing the window from the title bar handed the panel back. POP4 adds a second door —
//! a dock control INSIDE the window — and the two are not obviously the same event.
//!
//! Reading `dioxus-desktop` says they are not. `DesktopContext::close` sends a
//! `UserWindowEvent::CloseWindow(id)` (`desktop_context.rs:182`), `launch.rs:42` routes that
//! straight into `App::handle_close_requested`, and that function removes the webview from the
//! map (`app.rs:213`) without any `WindowEvent` being dispatched. So a window closed from inside
//! should never see `CloseRequested`, and `UserWindowEvent` is private to that crate, so a
//! handler cannot match it instead.
//!
//! That is a door-closing claim — if it is wrong in the direction that matters, the dock control
//! hands the panel back twice; if it is wrong the OTHER way and POP4 had relied on the handler
//! alone, the panel would be stranded off the grid with its window gone. It gets a key.
//!
//! Three windows, three doors:
//!
//! - **B** closes itself from inside its own dom, which is what the dock control does.
//! - **C** is closed by window A holding C's context, which is the same `UserWindowEvent` path
//!   reached from somewhere else — and the path `pop2_close_order` drives, so this also states
//!   what that spike was really measuring.
//! - Every window logs EVERY `WindowEvent` it receives, so the transcript says what does arrive
//!   when `CloseRequested` does not. POP2 argued `Destroyed` "may never arrive"; this is where
//!   that shows.
//!
//! The verdict names which doors reached a handler. `inside .... reached` would break the claim
//! in `panel_popout::return_to_grid` and make the dock control's explicit call redundant;
//! `inside .... MISSED` confirms the call is load-bearing, not belt-and-braces.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop4_close_doors`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop4_close_doors is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::tao::event::{Event as TaoEvent, WindowEvent};
    use dioxus::desktop::{use_wry_event_handler, Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Set by whichever handler fires. Statics rather than signals: the whole question is what
    /// happens while a dom is being torn down, and a signal in that dom is exactly the thing
    /// that may already be dead.
    static INSIDE_REACHED: AtomicBool = AtomicBool::new(false);
    static OUTSIDE_REACHED: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, PartialEq, Props)]
    struct DoorProps {
        /// Which door this window will be closed by — the label the transcript uses.
        door: &'static str,
        /// True for the window that closes ITSELF, which is the dock control's path.
        closes_itself: bool,
    }

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP4 close doors — window A (main)")
                        .with_inner_size(LogicalSize::new(560.0, 200.0)),
                ),
            )
            .launch(WindowA);
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(700)).await;

            println!("[driver] opening window B (will close itself from inside)");
            let b = dioxus::desktop::window()
                .new_window(
                    VirtualDom::new_with_props(
                        Door,
                        DoorProps {
                            door: "inside",
                            closes_itself: true,
                        },
                    ),
                    window_cfg("window B — closes itself"),
                )
                .resolve()
                .await;
            let _ = b;

            tokio::time::sleep(Duration::from_millis(900)).await;

            println!("[driver] opening window C (will be closed by A)");
            let c = dioxus::desktop::window()
                .new_window(
                    VirtualDom::new_with_props(
                        Door,
                        DoorProps {
                            door: "outside",
                            closes_itself: false,
                        },
                    ),
                    window_cfg("window C — closed by A"),
                )
                .resolve()
                .await;

            tokio::time::sleep(Duration::from_millis(700)).await;
            println!("[driver] A closing C, holding C's own context");
            c.close();

            // Long enough that a late `Destroyed` would have landed and printed.
            tokio::time::sleep(Duration::from_millis(1200)).await;
            verdict();
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window A (main)" }
                p { "driving three close doors — see the terminal" }
            }
        }
    }

    fn window_cfg(title: &str) -> Config {
        Config::new().with_window(
            WindowBuilder::new()
                .with_title(title.to_string())
                .with_inner_size(LogicalSize::new(360.0, 180.0)),
        )
    }

    #[allow(non_snake_case)]
    fn Door(props: DoorProps) -> Element {
        let own_id = dioxus::desktop::window().id();
        let door = props.door;

        use_wry_event_handler(move |event, _| {
            let TaoEvent::WindowEvent {
                window_id, event, ..
            } = event
            else {
                return;
            };
            if *window_id != own_id {
                return;
            }
            // Everything, not just the arm the handler cares about — the point is to see what
            // DOES arrive when the one POP2 relies on does not.
            println!("[{door}] WindowEvent::{}", name_of(event));
            if matches!(event, WindowEvent::CloseRequested) {
                match door {
                    "inside" => INSIDE_REACHED.store(true, Ordering::SeqCst),
                    _ => OUTSIDE_REACHED.store(true, Ordering::SeqCst),
                }
            }
        });

        if props.closes_itself {
            // The dock control's path exactly: `close()` called from inside this window's own
            // dom. Deferred so it happens after mount rather than during it.
            use_future(move || async move {
                tokio::time::sleep(Duration::from_millis(600)).await;
                println!("[driver] window B closing ITSELF from inside its own dom");
                dioxus::desktop::window().close();
            });
        }

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "{props.door} door" }
            }
        }
    }

    /// `WindowEvent` is `#[non_exhaustive]` and not `Debug`-cheap to read at a glance, so the
    /// transcript names only the arms this question is about and lumps the rest.
    fn name_of(event: &WindowEvent<'_>) -> &'static str {
        match event {
            WindowEvent::CloseRequested => "CloseRequested",
            WindowEvent::Destroyed => "Destroyed",
            WindowEvent::Focused(_) => "Focused",
            WindowEvent::Resized(_) => "Resized",
            WindowEvent::Moved(_) => "Moved",
            _ => "(other)",
        }
    }

    fn verdict() {
        let inside = INSIDE_REACHED.load(Ordering::SeqCst);
        let outside = OUTSIDE_REACHED.load(Ordering::SeqCst);
        let word = |reached: bool| if reached { "reached" } else { "MISSED" };

        println!("\n============ POP4 CLOSE-DOORS VERDICT ============");
        println!("closed from inside its own dom  .... {}", word(inside));
        println!("closed by another window        .... {}", word(outside));
        if inside {
            println!();
            println!("BREAKS the claim in `panel_popout::return_to_grid`: a window closed from");
            println!("inside DOES reach its own CloseRequested handler, so the dock control's");
            println!("explicit `return_to_grid` call is a second hand-back, not the only one.");
        } else {
            println!();
            println!("Confirms it: neither programmatic door reaches the handler, so the dock");
            println!("control's `return_to_grid` call is the whole return and not a duplicate.");
            println!("Only the OS door (title bar, window manager, quit) reaches the handler,");
            println!("which is the one POP2's click-through proved and this cannot drive.");
        }
        println!("=================================================\n");
        std::process::exit(if inside { 1 } else { 0 });
    }
}
