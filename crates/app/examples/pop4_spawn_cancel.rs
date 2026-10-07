//! POP4 — does a `spawn`ed task survive the component that spawned it being unmounted?
//!
//! The reaper needs to know which windows are open, and the first attempt registered each id in
//! `open_window`, straight after `new_window(..).resolve().await`. The window opened and the
//! registry stayed empty, so quitting with a panel out still left an orphan.
//!
//! The suspicion: `open_window` runs inside a `spawn` owned by `PopOutControl`'s scope, and its
//! FIRST act is to hide the panel — which takes that surface off the grid, unmounts the control,
//! and takes the task with it. Everything before the first `.await` has already run (the window
//! is created synchronously, which is why one appears at all); everything after it never
//! resumes. The registration is after it.
//!
//! That is a claim about Dioxus's task ownership, and reading source is how the last three wrong
//! claims on this stream got written. So it gets measured.
//!
//! Window A mounts a child, the child spawns a task that yields and then sets a flag, and A
//! unmounts the child while the task is suspended — the same shape as hiding the panel.
//!
//! `flag set after unmount .... false` confirms the diagnosis: work after an await is lost when
//! the owning scope goes, so the id must be registered by something that OUTLIVES the opener —
//! the popped window's own dom. `true` breaks it and sends the search elsewhere.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop4_spawn_cancel`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop4_spawn_cancel is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// Set by the spawned task BEFORE its first await — proof the task started at all.
    static RAN_BEFORE_AWAIT: AtomicBool = AtomicBool::new(false);
    /// Set by the same task AFTER its await. This is the one in question.
    static RAN_AFTER_AWAIT: AtomicBool = AtomicBool::new(false);

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP4 spawn cancel")
                        .with_inner_size(LogicalSize::new(560.0, 200.0)),
                ),
            )
            .launch(WindowA);
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        // Stands in for the grid's draw filter: the surface is on screen, then it is not.
        let mut mounted = use_signal(|| true);

        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            println!("[driver] unmounting the child while its task is suspended");
            mounted.set(false);

            tokio::time::sleep(Duration::from_millis(1600)).await;
            verdict();
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "spawn ownership" }
                if mounted() {
                    Spawner {}
                }
            }
        }
    }

    /// Stands in for `PopOutControl`: it spawns the task, then goes away.
    #[allow(non_snake_case)]
    fn Spawner() -> Element {
        use_hook(|| {
            spawn(async move {
                // Everything up to the first await runs synchronously — this is `new_window`.
                println!("[task] running, before the await");
                RAN_BEFORE_AWAIT.store(true, Ordering::SeqCst);

                // Long enough that the unmount below lands while this is still suspended —
                // the whole point. A shorter wait than the driver's lets the task finish first
                // and measures nothing, which is exactly how the first run of this spike lied.
                tokio::time::sleep(Duration::from_millis(1200)).await;

                // This is `opened.resolve().await` and the registry push after it.
                println!("[task] resumed AFTER the await");
                RAN_AFTER_AWAIT.store(true, Ordering::SeqCst);
            });
        });
        rsx! { p { "spawner mounted" } }
    }

    fn verdict() {
        let before = RAN_BEFORE_AWAIT.load(Ordering::SeqCst);
        let after = RAN_AFTER_AWAIT.load(Ordering::SeqCst);

        println!("\n========= POP4 SPAWN-OWNERSHIP VERDICT =========");
        println!("task started (ran before its await)  .... {before}");
        println!("task resumed after the unmount       .... {after}");
        println!();
        if !before {
            println!("INCONCLUSIVE: the task never started, so nothing is being measured.");
            std::process::exit(2);
        }
        if after {
            println!("BREAKS the diagnosis: a spawned task outlives its scope, so the empty");
            println!("registry has some other cause.");
            std::process::exit(1);
        }
        println!("Confirms it: a `spawn` is owned by the scope that made it, and unmounting");
        println!("that scope drops the task mid-await. `open_window` hides the panel first,");
        println!("which unmounts `PopOutControl` — so the window is created (sync, before the");
        println!("await) and the id is never registered (after it). The registration has to");
        println!("move into the popped window's OWN dom, which outlives the opener.");
        println!("===============================================\n");
        std::process::exit(0);
    }
}
