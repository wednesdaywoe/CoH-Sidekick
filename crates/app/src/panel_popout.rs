//! Panels that leave the grid into their own OS window — the second-monitor case
//! (panel-popout).
//!
//! **Desktop only.** The web build's only mechanism is `window.open`, which is a second document
//! and a second WASM instance with no shared arena between them, so the whole argument below
//! stops holding. [`PopOutControl`] compiles on both targets and draws nothing off the desktop,
//! which keeps the cfg out of the grid's chrome.
//!
//! ## There is no bridge
//!
//! The stream was costed against a shared store and a change channel, because a new Dioxus
//! window is a new `VirtualDom` and cannot `use_context` the first one's state. The context half
//! of that is true; the inference is not. POP1 measured it
//! ([`examples/pop1_spike.rs`](../examples/pop1_spike.rs)): a `Signal` handed to the second dom
//! as a PROP reads live there, a write in either window re-renders the other, and a `Memo`
//! recomputes across the boundary. Every webview shares the main thread and
//! `generational-box`'s arena is thread-local, so the value behind a signal is simply reachable
//! from both.
//!
//! So what a popped-out panel costs is the shim in [`PoppedPanel`]: take the context newtypes
//! across as props, re-provide them under their own types at the second root, and the panel
//! component reaches them through the plain `use_context` it already calls. **No panel component
//! changes to pop out** — [`PoppedPanel`] renders the same `StatGroup` the grid does.
//!
//! ## One context is deliberately NOT the main window's
//!
//! The organizer's open state. POP5 asked where a popped-out dashboard's gear should raise the
//! modal, and the answer decided 2026-09-15 is **in the window that was clicked**: the popped
//! root provides its own [`StatsConfigOpen`](crate::panels::stats_config::StatsConfigOpen) and
//! mounts its own `StatsConfigHost`. Sharing the signal is the obvious move and it is wrong
//! twice over — with a host in both windows one click draws the modal in BOTH, and with a host
//! only in the main window a gear clicked on the second monitor raises a dialog on the first,
//! which is the workflow the pop-out exists to avoid.
//!
//! Open state is per window; what the organizer EDITS is `DashboardConfig`, which is shared, so
//! a stat added here lands on the grid as it is clicked. That split is the whole design: the
//! signals that carry the BUILD cross, and the signal that carries where a dialog is do not.
//!
//! ## Why the roster is dashboards only
//!
//! Not read-only-ness — POP1 retired that reason by measuring write-back as free. The cost axis
//! is how many providers the panel's subtree consumes, because each one is a line of the shim,
//! and `StatGroup` consumes exactly three. Powers and Available reach for the ~20 picker and
//! modal signals, and the modal layer assumes one window (a `fixed` backdrop belongs to the
//! document it is in), so those are the deferred half of the stream rather than a bigger shim.
//!
//! ## Off the grid while out
//!
//! A popped-out surface is hidden through [`crate::panel_visibility::set_panel_hidden`], the same
//! one write path the header's hide control uses — so the grid closes over the gap with no
//! layout-engine change, and POP4's return is that call with `false`. What [`PoppedOut`] adds is
//! only the DISCRIMINATOR the hidden state cannot carry: which of the hidden surfaces are hidden
//! because they are out. Without it the visibility modal and the quickbar's pills would offer to
//! put a popped-out panel back on the grid while its window is still open, which draws it twice.
//!
//! [`PoppedOut`] is deliberately NOT persisted. POP3 owns that, and it owns a real question with
//! it — N webviews writing one `sk-layout` key is a race, so the schema change waits for the
//! single-writer decision rather than being guessed at here. The failure that leaves is bounded
//! and recoverable: quit while a panel is out and it comes back hidden with no window, listed in
//! the visibility modal like any other hidden surface.

use crate::grid::model::{GridItem, PanelKind};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The surfaces currently out in their own windows. Provided at the shell root; read by the grid
/// chrome and by every surface that offers to un-hide a panel.
///
/// A set rather than a flag on [`GridItem`] — see the module note on why the persisted schema is
/// POP3's to change and not this item's.
#[derive(Clone, Copy, PartialEq)]
pub struct PoppedOut(pub Signal<HashSet<PanelKind>>);

impl PoppedOut {
    /// Is this surface out in its own window right now?
    ///
    /// A `peek` rather than a read at every call site: the callers below are inside event
    /// handlers and `if` conditions where a tracked read would subscribe a scope that has no
    /// reason to re-render. The two places that must REDRAW on a pop-out — the grid chrome and
    /// the visibility row — read `.0` directly instead.
    pub fn holds(&self, panel: PanelKind) -> bool {
        self.0.peek().contains(&panel)
    }

    /// Re-hide every popped-out surface in a layout that was just rebuilt from the authored
    /// default.
    ///
    /// Four call sites build a layout from `default_layout_for` rather than editing the one on
    /// screen — Reset layout, the viewport re-fit, and two load-time fallbacks — and a fresh
    /// default has no memory of anything being out. The two load-time ones are harmless because
    /// this set is empty before anything has been popped; the other two are live, and without
    /// this they re-admit a surface whose window still has it. Reset layout is where that was
    /// found; the re-fit is the same bug through a door that needs no click, since it fires on a
    /// window resize.
    ///
    /// Reflow only. The guarantee that a popped-out surface is not DRAWN does not rest here —
    /// see the note on the grid's draw filter in [`crate::grid::view`] — because a rule every
    /// writer has to remember is a rule the fifth writer will not.
    pub fn reapply(&self, items: &mut [GridItem]) {
        for panel in self.0.peek().iter().copied() {
            crate::grid::collide::set_hidden(items, panel, true);
        }
    }
}

/// Where a popped-out window sat, in LOGICAL pixels — the size the user chose, not the pixels
/// it covered. Physical would restore wrong on any move between monitors of different scale,
/// which is the move this whole feature exists for.
///
/// The position is optional because it is sourced from a `Moved` event rather than read off the
/// window, and absent until one the app can believe arrives. `Window::outer_position` cannot say
/// "I do not know": it returns `Err` on Android alone, and on Linux it returns tao's own cached
/// pair (`platform_impl/linux/window.rs:449`), which starts at `(0, 0)`.
///
/// Sourcing it from the event does not escape that, which is the correction this design needed.
/// On Linux `Moved` fires and hands over `(0, 0)` anyway — measured 2026-09-15 on KDE Plasma:
/// a drag across the screen produced four events, all of them zero, and a freshly popped panel
/// with nothing on disk to seed it recorded `x: 0.0`. So the `Option` was never doing the work it
/// was written to do, and the position is taken only where the platform reports one at all — see
/// [`backend_reports_position`].
///
/// **It is not a Wayland story, which is the trap worth leaving written down.** The obvious read
/// is that xdg-shell never tells a client where its surface is, and the obvious fix is to detect
/// the backend. Both are wrong here: `dioxus-desktop` forces `GDK_BACKEND=x11` on every Linux
/// build (`app.rs:628`), so this is an X11 client on the backend where `with_position` IS
/// honoured, and a check keyed on `GDK_BACKEND` reads back the value dioxus just wrote. The
/// defect is in what tao can put in the event, not in which display server is running.
///
/// Two gates, because they answer for different sessions. [`spawn_window`] declines to USE a
/// stored position the platform cannot maintain, and [`carried_forward`] drops it from the record
/// rather than re-saving it — without the second, a machine that can place windows inherits a
/// zero written by one that could not, and puts every restored window in the corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: f64,
    pub height: f64,
}

/// Where a restored window should open: the stored pair, and only on a backend this session can
/// act on the answer of.
///
/// Separate from [`spawn_window`] because it is the half of the gate with no window in it, and a
/// stored `(0, 0)` surviving into `with_position` is the regression worth holding a test against.
#[cfg(feature = "desktop")]
fn restorable_position(rect: Option<WindowRect>, backend_reports: bool) -> Option<(f64, f64)> {
    rect.filter(|_| backend_reports)
        .and_then(|rect| rect.x.zip(rect.y))
}

/// A saved record as this session can honestly carry it forward.
///
/// Gating the READ is not enough on its own, and the gap is worth naming: the gate asks what THIS
/// session can do, so a platform that CAN place windows reads a position recorded by one that
/// could not, answers "yes, positions work here", and honours the stale zero. The bad value has
/// to be dropped where it lives — on the platform that cannot report a position, which is the
/// platform that recorded it. [`restore`] seeds the geometry map through here, so the next save
/// writes the record back without a position and the zero washes out.
#[cfg(feature = "desktop")]
fn carried_forward(rect: WindowRect, backend_reports: bool) -> WindowRect {
    if backend_reports {
        rect
    } else {
        WindowRect {
            x: None,
            y: None,
            ..rect
        }
    }
}

/// One surface that was in its own window when the app last closed, and where that window was.
///
/// Its own key (`sk-popped`) rather than a field on [`GridItem`], for two reasons that point the
/// same way. `sk-layout` is keyed by column count — a cell means nothing without the count it
/// was measured in — and being popped out is not a fact about a column count. And the web build
/// reads `sk-layout`, where restoring this would mean nothing: `window.open` is a second document
/// and a second WASM instance, which is the trade the stream rejected at the top.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
pub struct PoppedRecord {
    pub panel: PanelKind,
    /// Absent for a window that has not reported its geometry yet, which is every window for the
    /// frame between asking for it and it existing.
    pub rect: Option<WindowRect>,
}

/// Where each popped-out window is right now, updated as the user drags and resizes it.
///
/// Separate from [`PoppedOut`] rather than folded into it, and the reason is reactivity rather
/// than tidiness: `PoppedOut` is read by the grid's draw filter and by every un-hide control, and
/// a `Moved` event fires continuously through a window drag. One signal would re-render the whole
/// grid on every pixel the user drags a window on the other monitor. Membership changes twice a
/// session; geometry changes sixty times a second.
#[derive(Clone, Copy, PartialEq)]
pub struct PoppedGeometry(pub Signal<HashMap<PanelKind, WindowRect>>);

/// The header's pop-out control, a sibling of `.panel-head` for the same reason the fold and
/// hide controls are: `.panel-head > *` is `pointer-events: none` so the header can own the drag
/// gesture, and a button nested inside would never see the click.
///
/// Renders nothing for a surface that has no dashboard (the roster reason above) and nothing at
/// all off the desktop.
#[component]
pub fn PopOutControl(
    item: GridItem,
    title: String,
    /// The committed layout, so the pop-out can take the surface off the grid through the one
    /// write path.
    layout: Signal<Vec<GridItem>>,
    columns: u32,
) -> Element {
    #[cfg(not(feature = "desktop"))]
    {
        // Silence the unused bindings without cfg-ing the signature, which is what keeps the
        // call site in `grid::view` free of its own cfg.
        let _ = (item, title, layout, columns);
        rsx! {}
    }

    #[cfg(feature = "desktop")]
    {
        let Some(dashboard) = item.panel.dashboard() else {
            return rsx! {};
        };

        // Every context this panel's subtree consumes, read HERE rather than in the handler:
        // hooks run in the component body, and these are what cross to the second window.
        let totals = use_context::<crate::panels::stats::BuildTotals>();
        let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>();
        let ui_scale = use_context::<crate::shell::UiScale>();
        let popped = use_context::<PoppedOut>();
        let geometry = use_context::<PoppedGeometry>();

        // Tracked, unlike `holds`: this button has to disappear the moment the panel goes out.
        if popped.0.read().contains(&item.panel) {
            return rsx! {};
        }

        let panel = item.panel;
        let window_title = title.clone();

        rsx! {
            button {
                class: "panel-popout",
                r#type: "button",
                "aria-label": "Pop {title} out into its own window",
                onclick: move |_| {
                    let window_title = window_title.clone();
                    // `new_window` is async because webview2 refuses window creation from inside
                    // a webview callback, which is exactly where this click is.
                    spawn(async move {
                        open_window(
                            PoppedPanelProps {
                                panel: dashboard,
                                grid_panel: panel,
                                totals,
                                dashboards,
                                ui_scale,
                                popped,
                                geometry,
                                layout,
                                columns,
                            },
                            &window_title,
                        )
                        .await;
                    });
                },
                {crate::view::marks::pop_out()}
            }
        }
    }
}

/// Everything the MAIN window owes the popped-out ones: restoring them at launch, saving which
/// are out and where, and closing them when the app quits. Draws nothing, and on the web build it
/// is not even that. Wrapped like [`PopOutControl`] so `shell.rs` carries no `cfg` of its own.
///
/// One component rather than three because they are one invariant seen from three sides — the
/// window and the record agree — and splitting it would put the restore's "do not persist before
/// the load lands" gate in a different file from the persist it gates.
#[component]
pub fn PoppedWindows(
    layout: Signal<Vec<GridItem>>,
    columns: Signal<u32>,
    /// The shell's boot gate: true once the roster and the saved layout are both in. Restoring
    /// before it would re-hide panels into a layout about to be replaced, and would measure the
    /// saved records against the DEFAULT roster — the same race the shell's own reconcile effect
    /// is gated on, one file over.
    restored: Signal<bool>,
) -> Element {
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (layout, columns, restored);
        rsx! {}
    }

    #[cfg(feature = "desktop")]
    {
        rsx! { desktop::PoppedWindows { layout, columns, restored } }
    }
}

#[cfg(feature = "desktop")]
pub use desktop::PoppedPanelProps;

#[cfg(feature = "desktop")]
use desktop::open_window;

#[cfg(feature = "desktop")]
mod desktop {
    use super::{PoppedGeometry, PoppedOut, PoppedRecord, WindowRect};
    use crate::grid::model::{DashboardId, GridItem, PanelKind};
    use crate::panels::dashboards::DashboardConfig;
    use crate::panels::stats::BuildTotals;
    use crate::panels::stats_config::{OrganizerTarget, StatsConfigHost, StatsConfigOpen};
    use crate::shell::UiScale;
    use dioxus::desktop::tao::dpi::PhysicalPosition;
    use dioxus::desktop::tao::event::{Event as TaoEvent, WindowEvent};
    use dioxus::desktop::tao::window::WindowId;
    use dioxus::desktop::{
        use_wry_event_handler, Config, LogicalPosition, LogicalSize, WindowBuilder,
    };
    use dioxus::prelude::*;
    use std::cell::RefCell;
    use std::time::Duration;

    /// Everything the second dom needs, and the whole of it. `totals`, `dashboards` and
    /// `ui_scale` are the shim: the app's context newtypes travelling as props, to be re-provided
    /// under their own types at the popped-out root. The rest the window uses directly.
    #[derive(Clone, PartialEq, Props)]
    pub struct PoppedPanelProps {
        /// Which dashboard's stats to draw — what `StatGroup` takes.
        pub panel: DashboardId,
        /// The same surface as a grid identity, which is what the layout and [`PoppedOut`] are
        /// keyed by. Carried rather than reconstructed so the two can never disagree about which
        /// surface this window is.
        pub grid_panel: PanelKind,
        pub totals: BuildTotals,
        pub dashboards: DashboardConfig,
        pub ui_scale: UiScale,
        pub popped: PoppedOut,
        /// Written by the popped window and read only by [`PoppedWindows`]'s persist. Not part
        /// of the shim — nothing in this window's subtree consumes it, so it is never
        /// re-provided.
        pub geometry: PoppedGeometry,
        pub layout: Signal<Vec<GridItem>>,
        pub columns: u32,
    }

    /// Take the surface off the grid, then open its window — the control's path.
    ///
    /// Marked out first and hidden first, both before the `await`: the window resolves a frame or
    /// more later, and a panel that is still drawn in the grid during that gap flickers.
    ///
    /// The restore at launch does neither ([`PoppedWindows`] has already marked and hidden the
    /// whole saved set in one pass) and calls [`spawn_window`] directly.
    pub async fn open_window(props: PoppedPanelProps, title: &str) {
        let PoppedPanelProps {
            grid_panel,
            mut popped,
            layout,
            columns,
            ..
        } = props.clone();

        popped.0.write().insert(grid_panel);
        crate::panel_visibility::set_panel_hidden(layout, columns, grid_panel, true);
        // And NOT the place to register the new window with `OPEN_WINDOWS`. Registering after
        // this line was the first attempt and it silently did nothing: this fn runs in a `spawn`
        // owned by `PopOutControl`'s scope, and the hide above takes the surface off the grid,
        // unmounts that control, and drops this task. Everything up to the first `.await` has
        // already run, which is why a window appeared at all; anything after it never resumed.
        // [`pop4_spawn_cancel`](../examples/pop4_spawn_cancel.rs) is the key. The popped window
        // registers itself instead, from a dom that outlives this one.
        spawn_window(props, title, None);
    }

    /// Open the window and nothing else. `rect` is the geometry a previous session left behind;
    /// `None` is a fresh pop-out, which gets the authored size and lets the OS place it.
    pub fn spawn_window(props: PoppedPanelProps, title: &str, rect: Option<WindowRect>) {
        let dom = VirtualDom::new_with_props(PoppedPanel, props);
        let builder = WindowBuilder::new()
            .with_title(format!("{title} — CoH Sidekick"))
            .with_min_inner_size(LogicalSize::new(240.0, 160.0));
        // Narrow and tall by default: a stat panel is a list of label/value rows, and the grid's
        // own minimum for one is far under the main window's 960px floor (which exists because
        // app.css takes the phone layout below 900px — a breakpoint this window never reaches,
        // because it never draws the grid).
        let size = rect.map_or((380.0, 520.0), |rect| (rect.width, rect.height));
        let builder = builder.with_inner_size(LogicalSize::new(size.0, size.1));
        // Gated on the backend the same way recording one is, because a record outlives the
        // session that wrote it. A store carrying a position from a build that trusted `Moved`,
        // or from a session on a backend that really did report one, is still a store this
        // session cannot place a window from — and under X11 `with_position` is honoured, so an
        // inherited `(0, 0)` would put every restored window in the corner. Declining to record
        // a bad value does nothing for the ones already written down.
        let position = super::restorable_position(rect, backend_reports_position());
        let builder = match position {
            Some((x, y)) => builder.with_position(LogicalPosition::new(x, y)),
            None => builder,
        };

        dioxus::desktop::window().new_window(dom, Config::new().with_window(builder));
    }

    /// Record this window's size, read from its own DOM rather than from tao.
    ///
    /// `window.innerWidth/innerHeight` is the content box, which is exactly what [`spawn_window`]
    /// hands to `with_inner_size`, so the value round-trips. tao's `inner_size()` does not: on
    /// Linux it reports the GTK allocation INCLUDING client-side-decoration shadows while
    /// `with_inner_size` sets the content box, so every restore saved a size 90px larger on each
    /// axis than the one it asked for. Measured 2026-09-15 — asked `with_inner_size(920, 1060)`,
    /// handed back `1010x1150`, with `inner_size` and `outer_size` identical. That is a feedback
    /// loop rather than a fixed offset: the inflated read is what gets saved and asked for next
    /// time, so a panel grew 450px across five restarts.
    ///
    /// Async because the DOM is, and the entry is created HERE and nowhere else. Seeding it with
    /// tao's size first would put the number this exists to replace into the record on any run
    /// where the eval does not land.
    async fn report_size(mut geometry: PoppedGeometry, panel: PanelKind) {
        let Some((width, height)) = viewport_size().await else {
            return;
        };
        let Ok(mut map) = geometry.0.try_write() else {
            return;
        };
        let rect = map.entry(panel).or_insert(WindowRect {
            x: None,
            y: None,
            width,
            height,
        });
        rect.width = width;
        rect.height = height;
    }

    /// This webview's content box, which is the size [`spawn_window`] asked for and the size a
    /// restore has to reproduce.
    async fn viewport_size() -> Option<(f64, f64)> {
        let value = document::eval("return [window.innerWidth, window.innerHeight];")
            .await
            .ok()?;
        let width = value.get(0).and_then(serde_json::Value::as_f64)?;
        let height = value.get(1).and_then(serde_json::Value::as_f64)?;
        Some((width, height))
    }

    /// Open at the size that was saved, rather than at the size the platform makes of that number.
    ///
    /// `with_inner_size` sets the window allocation and the DOM reports the content box, and on
    /// Linux those differ by the client-side decorations, so round-tripping a saved size loses the
    /// difference every launch. Measured 2026-09-15: a panel shrank 75px of height per restart,
    /// the mirror of the growth tao's own `inner_size()` produced in the other direction.
    ///
    /// **One correction is not enough, which is the part worth writing down.** The offset belongs
    /// to the CALL, not to the window. `with_inner_size` on an unrealised window and
    /// `set_inner_size` on a realised one disagree: asking 744 at build time gave 669 of content,
    /// and correcting by that 75 to `set_inner_size(819)` gave 792 rather than 744, because the
    /// second call only owes 27. A single correction therefore overshoots by a constant — the
    /// +48 per restart it was measured producing.
    ///
    /// So it converges instead of computing: re-ask by the error, measure again, repeat. Both
    /// offsets are constants, so it settles on the second correction and the third round only
    /// confirms it. Nothing here assumes a pixel count, which matters because the decoration
    /// height belongs to the user's theme.
    ///
    /// [`restore`] seeds the geometry map before the window opens, so the entry already holds the
    /// size this window is meant to reach. A fresh pop-out has no target and nothing to correct.
    async fn settle_size(geometry: PoppedGeometry, panel: PanelKind) {
        let target = geometry
            .0
            .peek()
            .get(&panel)
            .map(|rect| (rect.width, rect.height));
        let Some((target_width, target_height)) = target else {
            report_size(geometry, panel).await;
            return;
        };
        let (mut asked_width, mut asked_height) = (target_width, target_height);
        for _ in 0..3 {
            let Some((width, height)) = viewport_size().await else {
                return;
            };
            let (short_by, shy_by) = (target_width - width, target_height - height);
            if short_by.abs() < 1.0 && shy_by.abs() < 1.0 {
                break;
            }
            asked_width += short_by;
            asked_height += shy_by;
            dioxus::desktop::window().set_inner_size(LogicalSize::new(asked_width, asked_height));
            // The resize is not in the webview's `window.inner*` the instant the call returns, and
            // reading it too early feeds the loop its own stale answer.
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        report_size(geometry, panel).await;
    }

    /// Record where this window is, from the payload of a `Moved` event and nothing else. See
    /// [`WindowRect`] for why the position cannot be read the way a size can, and why an event
    /// carrying one is still not enough to believe it.
    ///
    /// `try_write` rather than `write`: the main window's dom owns this signal, and a `Moved` can
    /// arrive while the app is quitting and that dom is already gone. The same
    /// `ValueDroppedError` [`return_to_grid`] guards against, reached through a different door.
    ///
    /// Only an existing entry is updated. The size is what makes a record worth saving and it
    /// arrives from [`report_size`]; a position with no size beside it has nothing to restore.
    fn report_position(
        mut geometry: PoppedGeometry,
        panel: PanelKind,
        moved_to: PhysicalPosition<i32>,
    ) {
        if !backend_reports_position() {
            return;
        }
        let scale = dioxus::desktop::window().scale_factor();
        let moved_to = moved_to.to_logical::<f64>(scale);
        let Ok(mut map) = geometry.0.try_write() else {
            return;
        };
        if let Some(rect) = map.get_mut(&panel) {
            rect.x = Some(moved_to.x);
            rect.y = Some(moved_to.y);
        }
    }

    /// Whether this platform's `Moved` event carries a position worth recording.
    ///
    /// `false` on Linux, measured rather than reasoned: 2026-09-15 on KDE Plasma, dragging a
    /// popped-out panel across the screen produced four `Moved` events and every one of them
    /// carried `PhysicalPosition { x: 0, y: 0 }`. This is not a Wayland story. `dioxus-desktop`
    /// forces `GDK_BACKEND=x11` on every Linux build to dodge a WebKitGTK bug (`app.rs:628`), so
    /// the app is an X11 client — the backend where `with_position` IS honoured — and tao still
    /// has nothing usable to put in the event. A `(0, 0)` meaning "I do not know" is the
    /// soft-wrong number Rule 1 exists to keep out.
    ///
    /// The first attempt asked the environment which backend was in play, and was wrong twice
    /// over: it read `GDK_BACKEND` back from the value dioxus itself had just written, and the
    /// defect is not backend-conditional in the first place.
    ///
    /// Left trusting and untested: a native X11 session rather than XWayland, and every non-Linux
    /// target. Declining costs nothing — the position never worked on Linux, so recording
    /// `(0, 0)` was strictly worse than recording nothing.
    fn backend_reports_position() -> bool {
        cfg!(not(target_os = "linux"))
    }

    thread_local! {
        /// Every popped-out window currently open.
        ///
        /// A `thread_local` rather than a signal because every webview shares the main thread
        /// (POP1: `SharedContext.pending_webviews` is a `RefCell`), and because the reader is an
        /// event handler rather than a component — there is nothing here to re-render on.
        ///
        /// It exists only for [`PoppedWindowReaper`]. [`PoppedOut`] answers "which SURFACES are
        /// elsewhere", which the grid and every un-hide control need on both targets; this
        /// answers "which WINDOWS are open", which only the desktop has and only the reaper
        /// asks. Folding the id into `PoppedOut` would put a `tao` type in a struct that
        /// compiles to wasm.
        static OPEN_WINDOWS: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
    }

    /// Forget a window that is closing under its own steam, so the reaper does not later ask a
    /// dead id to close. Called from both of [`return_to_grid`]'s doors.
    fn forget_window(id: WindowId) {
        OPEN_WINDOWS.with_borrow_mut(|open| open.retain(|open_id| *open_id != id));
    }

    /// The main window's side of the feature: restore at launch, save what is out, reap at quit.
    /// Rendered once at the shell root; draws nothing.
    #[component]
    pub fn PoppedWindows(
        layout: Signal<Vec<GridItem>>,
        columns: Signal<u32>,
        restored: Signal<bool>,
    ) -> Element {
        let popped = use_context::<PoppedOut>();
        let geometry = use_context::<PoppedGeometry>();
        let totals = use_context::<BuildTotals>();
        let dashboards = use_context::<DashboardConfig>();
        let ui_scale = use_context::<UiScale>();

        // Two gates, not one, and the gap between them is the whole reason. `started` stops the
        // restore running twice; `loaded` opens the persist below. Sharing one flag would let the
        // persist fire in the window between asking for the saved record and it arriving, and
        // what it would persist in that window is the empty set — over the record it is waiting
        // for. Same shape as the shell's `arrangement_restored`, one door further in.
        let mut started = use_signal(|| false);
        let mut loaded = use_signal(|| false);

        use_effect(move || {
            if !restored() || *started.peek() {
                return;
            }
            started.set(true);
            spawn(async move {
                restore(
                    popped, geometry, layout, columns, totals, dashboards, ui_scale,
                )
                .await;
                loaded.set(true);
            });
        });

        // Debounced, because `Moved` fires for every pixel of a window drag and each write is an
        // IPC round trip into the main window's JS context. 250ms is short enough that a pop-out
        // followed straight away by a quit still lands, and long enough that dragging a window
        // across a monitor is one write rather than two hundred.
        let mut generation = use_signal(|| 0u64);
        use_effect(move || {
            let records = records_of(&popped.0.read(), &geometry.0.read());
            if !loaded() {
                return;
            }
            let this = *generation.peek() + 1;
            generation.set(this);
            spawn(async move {
                tokio::time::sleep(Duration::from_millis(250)).await;
                if *generation.peek() != this {
                    return;
                }
                crate::layout_store::persist_popped(&records);
            });
        });

        rsx! { PoppedWindowReaper {} }
    }

    /// What to save: every surface that is out, with the geometry its window last reported.
    ///
    /// Sorted by slug because the membership is a `HashSet` — without it the same state
    /// serialises to a different string run to run, which turns every save into a diff and makes
    /// the stored value useless to read by eye.
    ///
    /// Geometry is looked up rather than joined, so an entry left behind by a panel that has
    /// since docked is ignored here and kept in the map. That is deliberate: pop the same panel
    /// out again in the same session and it reopens where the user last put it.
    fn records_of(
        out: &std::collections::HashSet<PanelKind>,
        geometry: &std::collections::HashMap<PanelKind, WindowRect>,
    ) -> Vec<PoppedRecord> {
        let mut records: Vec<PoppedRecord> = out
            .iter()
            .map(|panel| PoppedRecord {
                panel: *panel,
                rect: geometry.get(panel).copied(),
            })
            .collect();
        records.sort_by_key(|record| record.panel.slug());
        records
    }

    /// Reopen the windows a previous session left open.
    ///
    /// The whole saved set is marked out and hidden in ONE pass before any window is asked for,
    /// rather than per panel through [`open_window`]: the grid's draw filter reads [`PoppedOut`],
    /// so a panel marked after the layout is committed is a panel drawn on the grid for a frame
    /// and then yanked.
    ///
    /// Nothing is persisted here. The saved layout already carries these panels as hidden — that
    /// is what `set_panel_hidden` wrote when they were popped out — so [`PoppedOut::reapply`] is
    /// re-stating it, not changing it, and persisting would additionally declare a layout the app
    /// may have authored to be the user's own (see `layout_store::USER_ARRANGED`).
    async fn restore(
        mut popped: PoppedOut,
        mut geometry: PoppedGeometry,
        mut layout: Signal<Vec<GridItem>>,
        columns: Signal<u32>,
        totals: BuildTotals,
        dashboards: DashboardConfig,
        ui_scale: UiScale,
    ) {
        let Some(records) = crate::layout_store::load_popped().await else {
            return;
        };

        // A saved surface whose dashboard has since been deleted parses perfectly — a
        // `DashboardId` is an id, and `Retirable` can only drop what serde cannot read. The
        // roster is what knows, and it is the same filter the popped window applies to itself
        // when a dashboard leaves under it.
        let roster = crate::panels::dashboards::surfaces(&dashboards.0.peek());
        let wanted: Vec<(DashboardId, PoppedRecord)> = records
            .into_iter()
            .filter(|record| roster.contains(&record.panel))
            .filter_map(|record| record.panel.dashboard().map(|id| (id, record)))
            .collect();
        if wanted.is_empty() {
            return;
        }

        popped
            .0
            .write()
            .extend(wanted.iter().map(|(_, record)| record.panel));
        let mut items = layout.peek().clone();
        popped.reapply(&mut items);
        layout.set(items);

        let columns = columns.peek().to_owned();
        for (id, record) in wanted {
            let rect = record
                .rect
                .map(|rect| super::carried_forward(rect, backend_reports_position()));
            if let Some(rect) = rect {
                geometry.0.write().insert(record.panel, rect);
            }
            let title = crate::panels::dashboards::title_of(record.panel, &dashboards.0.peek());
            spawn_window(
                PoppedPanelProps {
                    panel: id,
                    grid_panel: record.panel,
                    totals,
                    dashboards,
                    ui_scale,
                    popped,
                    geometry,
                    layout,
                    columns,
                },
                &title,
                rect,
            );
        }
    }

    /// Closes every popped-out window when the MAIN window closes. Rendered once at the shell
    /// root; draws nothing.
    ///
    /// **The app does not quit while one is open.** `dioxus-desktop` exits only when the LAST
    /// webview goes (`app.rs:215`), so closing the main window removes its webview, leaves the
    /// popped one in the map, and the process keeps running behind an orphan window whose every
    /// signal was just freed. Reported from the click-through 2026-09-15 — "quit with a panel
    /// still out: no panic, but the popped-out panel remains" — and the guard in
    /// [`return_to_grid`] is what made it easy to miss: the handler ran, declined to write, and
    /// returned quietly, which from the inside looks exactly like a clean quit.
    ///
    /// **It lives here, in the main window, because a handler cannot watch another window.**
    /// The first fix put the arm in the popped window, on POP2's claim that the main window's
    /// close "comes through here too". It does not: `WindowEventHandlers::apply_event` skips any
    /// handler whose registered id differs from the event's, so that arm was dead code.
    /// [`pop4_event_reach`](../examples/pop4_event_reach.rs) is the key.
    ///
    /// **It closes rather than handing the panels back**, though their signals are alive for
    /// this one tick. The hand-back writes two of them, a write into a dom being torn down is
    /// the failure class that panicked here before, and the fallback is already written down:
    /// the panel returns hidden on the next launch, which POP2 recorded as bounded and left to
    /// POP3's persistence.
    #[component]
    pub fn PoppedWindowReaper() -> Element {
        // Both resolved in the component body and MOVED into the handler, rather than fetched
        // inside it. `use_wry_event_handler` does enter the scope's runtime, so `window()` would
        // resolve — but this handler runs while the app is tearing itself down, and a context
        // lookup is one more thing that has to still be true at that moment. The handle is an
        // `Rc<DesktopService>`; holding it costs a refcount and removes the question.
        let desktop = dioxus::desktop::window();
        let own_id = desktop.id();
        use_wry_event_handler(move |event, _| {
            let TaoEvent::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } = event
            else {
                return;
            };
            if *window_id != own_id {
                return;
            }
            for id in OPEN_WINDOWS.with_borrow(|open| open.clone()) {
                desktop.close_window(id);
            }
        });
        rsx! {}
    }

    /// Hand the surface back to the grid. The ONE implementation of the return, called by both
    /// doors, because tao does not let them be one event: [`DesktopContext::close`] sends a
    /// `UserWindowEvent::CloseWindow` that `dioxus-desktop` routes straight into
    /// `handle_close_requested` (`launch.rs:42`), so a window closed from INSIDE never produces
    /// the `WindowEvent::CloseRequested` the handler in [`PoppedPanel`] listens for — and
    /// `UserWindowEvent` is private to that crate, so the handler cannot match it either.
    ///
    /// Idempotent on purpose, since that asymmetry is a platform detail rather than a guarantee:
    /// removing from a set that no longer holds the key and un-hiding a surface that is already
    /// shown are both no-ops, so a door that fires twice costs nothing.
    ///
    /// Returns nothing to the caller and reports nothing on the bail. The main window may
    /// already be gone — quitting closes it first, which drops its dom and frees every signal in
    /// it, and only THEN does this window get its own close, so both writes would land on a dead
    /// `generational-box` slot. That is not hypothetical: it panicked with `ValueDroppedError`
    /// on the first quit taken with a panel still out. Bailing is the whole handling, not a
    /// swallowed error — these writes exist to hand the panel back to a grid, and if the signals
    /// holding that grid are gone then so is the grid, the layout they would have been persisted
    /// into is already written, and there is nothing left to put anything back into.
    ///
    /// POP1 measured the other direction — A writing a signal B had subscribed to, after B
    /// closed, which is fine. This is B writing A's signal after A closed, and it is the one
    /// order a two-window app reaches by the ordinary act of quitting.
    fn return_to_grid(
        mut popped: PoppedOut,
        layout: Signal<Vec<GridItem>>,
        columns: u32,
        grid_panel: PanelKind,
    ) {
        if popped.0.try_peek().is_err() || layout.try_peek().is_err() {
            return;
        }
        popped.0.write().remove(&grid_panel);
        // The one write path, as the pop-out used going out. Its `scroll_into_view` evals in
        // THIS document, where `.surface[data-id=…]` does not exist, so the scroll is a no-op
        // here — the script's own `if (el)` makes that silent rather than an error. Accepted
        // rather than routed around: the scroll exists for a control in the main window's
        // quickbar that is far from where the surface lands, and a panel returning from its own
        // window returns to the cell the user last saw it in, with their eyes already moving to
        // that window. Calling `set_hidden` directly to dodge the eval would buy a no-op and
        // cost the single write path this module is built on.
        crate::panel_visibility::set_panel_hidden(layout, columns, grid_panel, false);
    }

    /// The popped-out window's root: the shim, the document the main window has, its own
    /// organizer, and the same `StatGroup` the grid renders.
    #[component]
    pub fn PoppedPanel(props: PoppedPanelProps) -> Element {
        let PoppedPanelProps {
            panel,
            grid_panel,
            totals,
            dashboards,
            ui_scale,
            popped,
            geometry,
            layout,
            columns,
        } = props;

        // The shim. Re-providing each newtype under its own type is what lets `StatGroup` — two
        // levels down and with no idea it is in another window — reach these through the plain
        // `use_context` it already calls. Nothing is copied: these are the main window's signals.
        use_context_provider(|| totals);
        use_context_provider(|| dashboards);
        use_context_provider(|| ui_scale);

        // The one context that is this window's OWN rather than the main window's — see the
        // module note on why open state does not cross while the build does.
        //
        // This settles a POP2 defect as well as carrying POP5's control: `StatGroup` sets this
        // signal itself, from the "Empty — add stats" button it draws when a dashboard has no
        // stats in it. Bridged, that button was setting the MAIN window's organizer open with no
        // host in this one to show it and no focus change to say where it went — a dead control
        // on precisely the panel with nothing else to click.
        let mut config_open = use_signal(|| Option::<OrganizerTarget>::None);
        use_context_provider(|| StatsConfigOpen(config_open));

        // A second webview is a second document: it inherits neither the theme attribute nor the
        // zoom the main window set on ITS `<html>`, so both are applied here too. The theme reads
        // the same `localStorage` (one origin per app, not per window); the zoom tracks the live
        // signal, so the main window's scale control moves this window with it.
        use_effect(crate::theme::apply_saved_theme);
        let scale = ui_scale.pct;
        use_effect(move || crate::ui_scale::apply(scale()));

        // This window's own id, resolved once: all three of the doors below deregister with it,
        // and the OS-close handler filters on it.
        let own_id = dioxus::desktop::window().id();

        // Registered from HERE rather than from the opener, because this dom outlives that one.
        // `open_window` is a `spawn` owned by `PopOutControl`, and hiding the panel unmounts
        // that control and drops the task mid-await, so a registration after the await never
        // ran — the window opened, the registry stayed empty, and the reaper had nothing to
        // close. `use_hook` rather than `use_effect`: this is once-at-mount bookkeeping with no
        // dependency to re-run on.
        use_hook(|| OPEN_WINDOWS.with_borrow_mut(|open| open.push(own_id)));

        // Where this window is, for POP3 to reopen it here next launch. Reported at mount and
        // then on every move and resize, into a signal the MAIN window persists — this one may
        // not write storage at all, and [`crate::storage`] is where that rule and its measurement
        // live.
        use_hook(|| spawn(settle_size(geometry, grid_panel)));
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
            match event {
                WindowEvent::Moved(position) => report_position(geometry, grid_panel, *position),
                WindowEvent::Resized(_) => {
                    spawn(report_size(geometry, grid_panel));
                }
                _ => {}
            }
        });

        // The roster can lose this panel while its window is open, and POP5 put that door one
        // click away INSIDE this window: the organizer deletes panels. A window drawing a
        // dashboard the roster no longer has draws nothing, and it has nothing to hand back
        // either — the shell's roster reconcile (`shell.rs`) has already dropped the `GridItem`
        // this window would return to. So it drops out of the popped set, which would otherwise
        // hold a surface that does not exist marked as being somewhere, and closes.
        //
        // Not a new hazard, only a nearer one: deleting a popped-out dashboard from the MAIN
        // window's organizer was always reachable and stranded this window the same way.
        use_effect(move || {
            if dashboards.0.read().panel(panel).is_some() {
                return;
            }
            let mut popped = popped;
            if let Ok(mut out) = popped.0.try_write() {
                out.remove(&grid_panel);
            }
            // The third door, and the third place to say so: `close()` reaches no handler
            // (`pop4_close_doors`), so this path has to deregister itself exactly as the dock
            // control does. A stale id costs nothing at the reaper — `handle_close_requested`
            // returns for an id the webview map has lost — but a registry that only ever grows
            // is a wrong answer waiting for a reader who trusts it.
            forget_window(own_id);
            dioxus::desktop::window().close();
        });

        // Closing the window hands the panel back, whichever door did it. This arm is the OS
        // one — the title bar's close box, the window manager, the app quitting — and the dock
        // control below is the other; `return_to_grid` is what keeps them one behaviour.
        use_wry_event_handler(move |event, _| {
            // `CloseRequested`, not `Destroyed`. The event loop applies every registered handler
            // in `App::tick` BEFORE it dispatches the event, and its `CloseRequested` arm is
            // what drops this webview and the dom this handler lives in — so the request is the
            // last moment the handler exists to hear anything. `Destroyed` is dispatched after
            // that drop, which on a hook whose cleanup is `handler.remove()` means it may never
            // arrive, and the panel would be left off the grid with nothing to put it back.
            let TaoEvent::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } = event
            else {
                return;
            };
            // Belt and braces, not the filter. `WindowEventHandlers::apply_event` already skips
            // any handler whose registered `window_id` differs from the event's, and
            // `use_wry_event_handler` registers with THIS window's id — so this handler is only
            // ever offered this window's own events.
            //
            // POP2's comment here said the opposite ("the main window's own close comes through
            // here too, hence the id check") and POP4 built a satellite arm on it that could
            // never fire. Measured and corrected by
            // [`pop4_event_reach`](../examples/pop4_event_reach.rs): B's handler saw its own
            // events and never A's. Closing the popped window when the app quits is therefore
            // the MAIN window's job — see [`PoppedWindowReaper`].
            if *window_id != own_id {
                return;
            }
            forget_window(own_id);
            return_to_grid(popped, layout, columns, grid_panel);
        });

        // Live rather than carried as a prop, so a rename in the main window's organizer — or in
        // this one's — retitles these controls instead of freezing the name the pop-out was
        // taken under. The window's own title bar is the frozen copy, and tao owns that.
        let title = crate::panels::dashboards::title_of(grid_panel, &dashboards.0.read());

        rsx! {
            crate::DocumentAssets {}
            div { class: "popped-panel",
                // POP2 drew no chrome here at all, on the argument that the grid header's
                // controls are about a grid this window does not have. That is true of the grip,
                // the fold and the hide — and false of these two, which is one of four argued as
                // four. The gear is about what the panel HOLDS, and a dashboard's contents are
                // the one thing a dashboard is for; the dock is about this window, which is the
                // only surface in the app that has one.
                div { class: "popped-chrome",
                    button {
                        class: "popped-control",
                        r#type: "button",
                        "aria-label": "Choose which stats {title} shows",
                        onclick: move |_| {
                            config_open.set(Some(OrganizerTarget::Panel(panel)));
                        },
                        {crate::view::marks::sliders()}
                    }
                    button {
                        class: "popped-control",
                        r#type: "button",
                        "aria-label": "Put {title} back on the grid",
                        onclick: move |_| {
                            // Both halves, in this order, and not just `close()`: see
                            // `return_to_grid` for why a window closed from inside never reaches
                            // the handler above.
                            forget_window(own_id);
                            return_to_grid(popped, layout, columns, grid_panel);
                            dioxus::desktop::window().close();
                        },
                        {crate::view::marks::pop_in()}
                    }
                }
                div { class: "popped-body",
                    crate::panels::stats::StatGroup { panel }
                }
            }
            // This window's own organizer, a sibling of the panel rather than a child for the
            // containment reason every modal in the app shares. The reason is weaker here —
            // there is no grid in this document, so no `transform`ed ancestor to contain a
            // `fixed` backdrop — but mounting it at the root anyway keeps one rule instead of
            // two, and the next surface to reach this window may not be so simple.
            StatsConfigHost {}
        }
    }
}
