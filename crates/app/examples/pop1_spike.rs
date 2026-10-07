//! POP1 — what Dioxus 0.7.9's multi-window API actually permits, measured.
//!
//! panel-popout opens on the premise that a popped-out
//! panel "cannot `use_context` the main window's signals, so everything it reads has to be
//! bridged". That is two claims wearing one sentence, and this separates them: whether CONTEXT
//! crosses a VirtualDom boundary, and whether a SIGNAL does. The stream costed the feature as if
//! the answer to both were no.
//!
//! Four things are asked of a running app rather than of the docs:
//!
//! 1. a second window opens from inside a future of the first;
//! 2. a `Signal` created in window A and handed to B's `VirtualDom` as a prop READS in B;
//! 3. a write in A re-renders B — and a write in B re-renders A, which the stream assumed was
//!    the expensive direction and scheduled out of POP2;
//! 4. `use_context` provided in A does NOT reach B (the half expected to be true);
//! 5. a `Memo` derived in A — the shape `totals` actually is — recomputes for B's reads;
//! 6. A's signal, re-provided under its own context newtype at B's root, reaches a deep child
//!    in B through the plain `use_context` every existing panel already calls.
//!
//! Then B's window is closed from A while A keeps writing, because POP4's worry is state
//! stranded in a closed webview.
//!
//! The run is self-driving and prints a tagged transcript, so the outcome is greppable rather
//! than eyeballed. A verdict block is printed before exit.
//!
//! Run: `cargo run -p app --release --features desktop,census-probe --example pop1_spike`

fn main() {
    #[cfg(feature = "desktop")]
    desktop::run();

    #[cfg(not(feature = "desktop"))]
    eprintln!("pop1_spike is a desktop spike — run with `--features desktop`");
}

#[cfg(feature = "desktop")]
mod desktop {
    use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
    use dioxus::prelude::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    /// Provided in A only. B asking for this is the context half of the question.
    #[derive(Clone, Copy, PartialEq)]
    struct MainOnly(i32);

    /// The shape every one of this app's 43 providers actually has: a `Copy` newtype over a
    /// signal. `BuildTotals(Memo<CalculatedTotals>)`, `DashboardConfig(Signal<Dashboards>)`,
    /// `GridColumns(Signal<u32>)` are all this. If the newtype can be re-provided at B's root
    /// and consumed by a deep child there, no panel component needs a change to pop out.
    #[derive(Clone, Copy, PartialEq)]
    struct TotalsLike(Memo<i32>);

    #[derive(Default)]
    struct Log {
        a_renders: Vec<(i32, i32)>,
        b_renders: Vec<(i32, i32)>,
        b_saw_context: Option<bool>,
        b_memo_reads: Vec<i32>,
        deep_child_reads: Vec<i32>,
        second_window_opened: bool,
        writes_after_close: i32,
    }

    fn log() -> &'static Mutex<Log> {
        static L: OnceLock<Mutex<Log>> = OnceLock::new();
        L.get_or_init(|| Mutex::new(Log::default()))
    }

    /// Both signals are created in A and travel to B by value. `Signal` is `Copy + 'static`, so
    /// this is a prop like any other — no store, no channel, no serialization.
    #[derive(Clone, PartialEq, Props)]
    struct BridgeProps {
        /// Written by A, read by B.
        counter: Signal<i32>,
        /// Written by B, read by A — the reverse direction.
        echo: Signal<i32>,
        /// Derived in A. `totals` is a `Memo`, not a plain `Signal`, and a memo recomputes in
        /// the runtime that created it — so it is asked separately.
        doubled: Memo<i32>,
    }

    pub fn run() {
        dioxus::LaunchBuilder::desktop()
            .with_cfg(
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP1 spike — window A (main)")
                        .with_inner_size(LogicalSize::new(560.0, 340.0)),
                ),
            )
            .launch(WindowA);
    }

    #[allow(non_snake_case)]
    fn WindowA() -> Element {
        let mut counter = use_signal(|| 0);
        let echo = use_signal(|| 0);
        let doubled = use_memo(move || counter() * 10);

        // Provided in A's tree only. Nothing in B's tree can see this unless context crosses.
        use_context_provider(|| MainOnly(1234));

        log().lock().unwrap().a_renders.push((counter(), echo()));
        println!("[A render] counter={} echo={}", counter(), echo());

        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(700)).await;

            // (1) open the second window from the running app
            println!("[driver] opening window B");
            let dom = VirtualDom::new_with_props(
                WindowB,
                BridgeProps {
                    counter,
                    echo,
                    doubled,
                },
            );
            let pending = dioxus::desktop::window().new_window(
                dom,
                Config::new().with_window(
                    WindowBuilder::new()
                        .with_title("POP1 spike — window B (popped out)")
                        .with_inner_size(LogicalSize::new(560.0, 340.0)),
                ),
            );
            let b_window = pending.resolve().await;
            log().lock().unwrap().second_window_opened = true;
            println!("[driver] window B resolved");

            // (3a) write in A, expect B to re-render
            for n in 1..=3 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                counter.set(n);
                println!("[driver] A wrote counter={n}");
            }

            // (3b) give B's write-back a beat to land, then (5) close B from A
            tokio::time::sleep(Duration::from_millis(900)).await;
            println!("[driver] closing window B");
            b_window.close();

            // keep writing into a signal a dead webview was subscribed to
            for n in 4..=5 {
                tokio::time::sleep(Duration::from_millis(400)).await;
                counter.set(n);
                log().lock().unwrap().writes_after_close += 1;
                println!("[driver] A wrote counter={n} after B closed");
            }

            tokio::time::sleep(Duration::from_millis(400)).await;
            verdict();
            dioxus::desktop::window().close();
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window A (main)" }
                p { "counter (A writes): " strong { "{counter}" } }
                p { "echo (B writes): " strong { "{echo}" } }
            }
        }
    }

    #[allow(non_snake_case)]
    fn WindowB(props: BridgeProps) -> Element {
        let counter = props.counter;
        let mut echo = props.echo;
        let doubled = props.doubled;

        // (4) context half — expected None, since B's VirtualDom has its own root
        let saw_context = try_consume_context::<MainOnly>().is_some();
        {
            let mut l = log().lock().unwrap();
            l.b_saw_context = Some(saw_context);
            // (2) the read itself — if this panics or reads stale, the signal half is dead
            l.b_renders.push((counter(), echo()));
            l.b_memo_reads.push(doubled());
        }
        println!(
            "[B render] counter={} doubled(memo from A)={} echo={} saw_main_context={}",
            counter(),
            doubled(),
            echo(),
            saw_context
        );

        // (6) the POP2 shim: re-provide A's signal under the same context newtype at B's root,
        // so B's descendants reach it through the `use_context` they already call.
        use_context_provider(|| TotalsLike(doubled));

        // (3b) write back into a signal owned by A's scope
        use_future(move || async move {
            tokio::time::sleep(Duration::from_millis(1900)).await;
            echo.set(99);
            println!("[driver] B wrote echo=99");
        });

        rsx! {
            div { style: "font: 14px system-ui; padding: 16px",
                h3 { "Window B (popped out)" }
                p { "counter read from A's signal: " strong { "{counter}" } }
                p { "memo derived in A: " strong { "{doubled}" } }
                DeepChildInB {}
                p { "main-window context visible: " strong { "{saw_context}" } }
            }
        }
    }

    /// A stand-in for a real panel component: it takes no props and reaches for its state the
    /// only way the existing panels do — `use_context`. It is two levels below B's root and has
    /// no idea it is in a different window than the signal it reads.
    #[allow(non_snake_case)]
    fn DeepChildInB() -> Element {
        let TotalsLike(totals) = use_context::<TotalsLike>();
        log().lock().unwrap().deep_child_reads.push(totals());
        println!("[B deep child] read via use_context: {}", totals());
        rsx! { p { "deep child (use_context): " strong { "{totals}" } } }
    }

    fn verdict() {
        let l = log().lock().unwrap();
        let b_counters: Vec<i32> = l.b_renders.iter().map(|(c, _)| *c).collect();
        let a_echoes: Vec<i32> = l.a_renders.iter().map(|(_, e)| *e).collect();
        let mut distinct = b_counters.clone();
        distinct.sort_unstable();
        distinct.dedup();

        println!("\n================ POP1 VERDICT ================");
        println!(
            "second window opened ...................... {}",
            l.second_window_opened
        );
        println!(
            "B renders ................................. {:?}",
            l.b_renders
        );
        println!(
            "A renders ................................. {:?}",
            l.a_renders
        );
        println!(
            "signal READ crosses (B saw A's writes) .... {}",
            b_counters.iter().any(|c| *c > 0)
        );
        println!(
            "B re-rendered per A write ................. {} distinct values",
            distinct.len()
        );
        println!(
            "signal WRITE-BACK crosses (A saw echo=99) . {}",
            a_echoes.contains(&99)
        );
        println!(
            "context crosses ........................... {:?} (expected Some(false))",
            l.b_saw_context
        );
        println!(
            "memo READ crosses (B saw A's memo) ....... {:?}",
            l.b_memo_reads
        );
        println!(
            "memo tracked A's writes ................... {}",
            l.b_memo_reads == b_counters.iter().map(|c| c * 10).collect::<Vec<_>>()
        );
        // The deep child reads only `doubled`, so it must render once per A write and NOT for
        // B's unrelated `echo` write — i.e. fine-grained reactivity survives the window boundary
        // rather than every cross-window change re-rendering the whole popped-out tree.
        let expected_child: Vec<i32> = (0..=3).map(|n| n * 10).collect();
        println!(
            "re-provided context reaches deep child .... {:?}",
            l.deep_child_reads
        );
        println!(
            "child rendered once per A write ........... {}",
            l.deep_child_reads == expected_child
        );
        println!(
            "child skipped unrelated echo write ........ {}",
            l.deep_child_reads.len() < l.b_renders.len()
        );
        println!(
            "writes after B closed, no panic ........... {}",
            l.writes_after_close
        );
        println!("==============================================\n");
    }
}
