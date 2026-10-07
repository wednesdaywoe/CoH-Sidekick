//! POP2 — what a popped-out window's close handler may touch after the MAIN window is gone.
//!
//! POP1 measured one direction and called the worry answered: window A kept writing a signal B
//! had subscribed to after B closed, and nothing panicked. The reverse order is the one an app
//! reaches by ordinary use — close the main window with a panel still out, then close the panel —
//! and it was never asked.
//!
//! It panics. A's `VirtualDom` is dropped with its window, freeing every signal in it; B's close
//! handler, whose whole job is to hand the panel back, then writes into a dead
//! `generational-box` slot. `Result::unwrap()` on `Dropped(ValueDroppedError)`.
//!
//! So this asks the one question the guard in `panel_popout::return_to_grid` rests on:
//!
//! > after A's window closes, is a signal A owned still readable from B?
//!
//! `readable .... false` is the guard earning its place — there is nothing left to hand the panel
//! back to, and bailing is the whole handling. `readable .... true` would mean A's signals outlive
//! A's window and the guard is costing a real hand-back, which breaks the claim rather than
//! confirming it.
//!
//! ## Why the driver lives in B
//!
//! This spike used to drive from A: A opened B, then A closed itself, then A closed B. It could
//! not work and did not — it hung after its second line, printing no verdict at all, and the
//! result quoted in the stream doc was never reproducible from this file. Two reasons, both of
//! which `pop4_close_doors` later measured directly:
//!
//! - **A's driver dies with A.** The future was a hook in A's dom, so dropping that dom dropped
//!   the future mid-`sleep`. The line that closed B never ran.
//! - **A programmatic close reaches no handler anyway.** `DesktopContext::close` sends a
//!   `UserWindowEvent::CloseWindow` that `dioxus-desktop` routes straight into
//!   `App::handle_close_requested`, which removes the webview without dispatching any
//!   `WindowEvent` — so B's `CloseRequested` arm could never have fired, and no `Destroyed`
//!   arrives either.
//!
//! The fix is to ask the question from the window that survives. B holds A's `WindowId`, closes A
//! through it, and then probes A's signal itself — which is precisely what B's handler would do
//! one moment later, minus the event this process cannot generate. The OS door that DOES reach a
//! handler is the title bar, and no spike can press it; POP2's click-through is what covers that.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop2_close_order`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop2_close_order is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::tao::window::WindowId;
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::time::Duration;

    /// Stands in for `PoppedOut`'s set and the grid layout: signals created in A, handed to B as
    /// props, and written by B's close handler. The real pair are a `HashSet<PanelKind>` and a
    /// `Vec<GridItem>`; the arena does not care which, and the failure is about the slot.
    #[derive(Clone, PartialEq, Props)]
    struct BridgeProps {
        owned_by_a: Signal<i32>,
        /// A's window, so B can close it and stay alive to report. A `WindowId` rather than a
        /// `DesktopContext` because it is `Copy + PartialEq` and so can be a prop;
        /// `DesktopContext::close_window` takes exactly this.
        a_id: WindowId,
    }

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP2 close order — window A (main)")
                        .with_inner_size(LogicalSize::new(520.0, 240.0)),
                ),
            )
            .launch(WindowA);
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        let owned_by_a = use_signal(|| 1);
        let a_id = dioxus::desktop::window().id();

        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(700)).await;
            println!("[driver] A opening window B, then handing the driving to B");
            dioxus::desktop::window().new_window(
                VirtualDom::new_with_props(WindowB, BridgeProps { owned_by_a, a_id }),
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP2 close order — window B (popped out)")
                        .with_inner_size(LogicalSize::new(520.0, 240.0)),
                ),
            );
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window A (main)" }
                p { "owned_by_a = {owned_by_a}" }
            }
        }
    }

    #[allow(non_snake_case)]
    fn WindowB(props: BridgeProps) -> Element {
        let mut owned_by_a = props.owned_by_a;
        let a_id = props.a_id;

        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(600)).await;

            // The order a two-window app reaches by ordinary use: the MAIN window goes first.
            println!("[driver] B closing window A (the main window) FIRST");
            dioxus::desktop::window().close_window(a_id);

            // Long enough for A's webview — and the `VirtualDom` it owns — to be dropped.
            tokio::time::sleep(Duration::from_millis(900)).await;

            // The probe. This is what B's `CloseRequested` handler does before it writes, and
            // the only part of it that does not need an event this process cannot generate.
            let alive = owned_by_a.try_peek().is_ok();
            println!("[probe] A's signal readable from B after A closed .... {alive}");

            println!("\n============ POP2 CLOSE-ORDER VERDICT ============");
            if alive {
                // Prove the read is a real read and not a stale handle, then fail: if the value
                // is genuinely there, the guard is refusing a hand-back it could have made.
                owned_by_a.set(99);
                println!("[GUARD] would have written — A's signals outlived A's window.");
                println!("The guard in `return_to_grid` is too eager and costs a real return.");
                println!("=================================================\n");
                std::process::exit(1);
            }
            println!("[GUARD] skips — nothing left to hand the panel back to.");
            println!("Confirms the guard: B's handler must check before it writes, or it");
            println!("unwraps a `Dropped(ValueDroppedError)` on the first quit taken with a");
            println!("panel still out.");
            println!("=================================================\n");
            std::process::exit(0);
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window B (popped out)" }
                p { "drives the close order — see the terminal" }
            }
        }
    }
}
