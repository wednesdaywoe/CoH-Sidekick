//! POP4 — whether a window's event handler ever sees ANOTHER window's events.
//!
//! POP2's close handler carries an id check with a comment saying "the main window's own close
//! comes through here too, hence the id check". POP4's satellite fix was built on that sentence:
//! the popped window would notice the MAIN window closing and close itself, so quitting the app
//! with a panel out would not leave an orphan behind.
//!
//! It did leave an orphan behind. Reading `dioxus-desktop` says why —
//! `WindowEventHandlers::apply_event` (`event_handlers.rs`) skips any handler whose registered
//! `window_id` differs from the event's, and `use_wry_event_handler` registers with the current
//! window's id. So a handler in window B is only ever offered B's own `WindowEvent`s, POP2's id
//! check is redundant rather than load-bearing, and the sentence justifying it is wrong.
//!
//! This is the key for that, because it is a claim about a library's dispatch and reading source
//! is how the last two wrong claims on this stream got written.
//!
//! **Driven with `Resized`, not a close.** `pop4_close_doors` established that no programmatic
//! close produces a `WindowEvent` at all, so a close cannot be used to ask a question about
//! DELIVERY — the absence would be explained twice over. A resize is a `WindowEvent` this process
//! can actually cause (`set_inner_size`), which separates the two questions: if B's handler sees
//! A's `Resized`, cross-window delivery works and the orphan has some other cause; if it sees
//! only its own, delivery is per-window and the satellite arm can never fire.
//!
//! Verdict lines: `B saw A's events .... false` confirms the dispatch rule and condemns the
//! fix built on POP2's sentence. `true` would break it and send the search elsewhere.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop4_event_reach`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop4_event_reach is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::tao::event::Event as TaoEvent;
    use dioxus::desktop::tao::window::WindowId;
    use dioxus::desktop::{use_wry_event_handler, Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Did the SECOND window's handler ever receive an event belonging to the first?
    static B_SAW_A: AtomicBool = AtomicBool::new(false);
    /// Did it receive its own? The control: without this, `B_SAW_A == false` is equally well
    /// explained by the handler never running at all.
    static B_SAW_B: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, PartialEq, Props)]
    struct WatchProps {
        a_id: WindowId,
    }

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP4 event reach — window A (main)")
                        .with_inner_size(LogicalSize::new(560.0, 220.0)),
                ),
            )
            .launch(WindowA);
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        let a_id = dioxus::desktop::window().id();

        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            println!("[driver] A opening window B");
            let b = dioxus::desktop::window()
                .new_window(
                    VirtualDom::new_with_props(WindowB, WatchProps { a_id }),
                    Config::new().with_window(
                        WindowBuilder::new()
                            .with_title("POP4 event reach — window B")
                            .with_inner_size(LogicalSize::new(360.0, 180.0)),
                    ),
                )
                .resolve()
                .await;

            tokio::time::sleep(Duration::from_millis(700)).await;
            println!("[driver] resizing A — a WindowEvent that genuinely belongs to A");
            dioxus::desktop::window().set_inner_size(LogicalSize::new(600.0, 260.0));

            tokio::time::sleep(Duration::from_millis(700)).await;
            println!("[driver] resizing B — the control, an event that belongs to B");
            b.set_inner_size(LogicalSize::new(400.0, 220.0));

            tokio::time::sleep(Duration::from_millis(900)).await;
            verdict();
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window A (main)" }
                p { "driving — see the terminal" }
            }
        }
    }

    #[allow(non_snake_case)]
    fn WindowB(props: WatchProps) -> Element {
        let a_id = props.a_id;
        let own_id = dioxus::desktop::window().id();

        use_wry_event_handler(move |event, _| {
            let TaoEvent::WindowEvent { window_id, .. } = event else {
                return;
            };
            if *window_id == a_id {
                println!("[B] received an event belonging to A");
                B_SAW_A.store(true, Ordering::SeqCst);
            }
            if *window_id == own_id {
                B_SAW_B.store(true, Ordering::SeqCst);
            }
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window B" }
                p { "watching for A's events" }
            }
        }
    }

    fn verdict() {
        let saw_a = B_SAW_A.load(Ordering::SeqCst);
        let saw_b = B_SAW_B.load(Ordering::SeqCst);

        println!("\n============ POP4 EVENT-REACH VERDICT ============");
        println!("B's handler ran at all (saw its own) .... {saw_b}");
        println!("B saw A's events                     .... {saw_a}");
        println!();
        if !saw_b {
            println!("INCONCLUSIVE: B's handler never ran, so it cannot be asked about A.");
            std::process::exit(2);
        }
        if saw_a {
            println!("BREAKS the dispatch reading: handlers DO receive other windows' events,");
            println!("so POP2's id check is load-bearing and the orphan has another cause.");
            std::process::exit(1);
        }
        println!("Confirms it: `apply_event` filters by the handler's registered window_id, so");
        println!("a handler in B is only ever offered B's own events. POP2's comment claiming");
        println!("the main window's close 'comes through here too' is WRONG, its id check is");
        println!("redundant, and POP4's satellite arm keyed on that comment can never fire.");
        println!("The reaper has to live in the window that OWNS the event — the main one.");
        println!("=================================================\n");
        std::process::exit(0);
    }
}
