//! The free grid's render layer: `GridContainer` positions every surface by
//! translating cells to pixels through [`super::math`], and `GridSurface` draws
//! one surface at that computed rectangle. This is the only grid module that
//! imports Dioxus — the geometry it renders is decided by the pure engine, so
//! the code here stays declarative.
//!
//! FG3 adds move-drag. The committed layout ([`GridContainer`]'s `state`) is
//! written exactly once, on release; an in-flight [`DragState`] drives the
//! cursor-following surface and the snapped placeholder while a drag is live. The
//! two are separate signals on purpose: the static surfaces read only `state`, so
//! a pointer moving each frame re-renders the drag layer and placeholder and
//! nothing else — the collision engine ([`super::collide::move_item`]) runs once,
//! on commit, never per frame.

use dioxus::prelude::*;

use super::collide::{clamp_resize, fit_item, move_item, place_item, resize_item, toggle_collapse};
use super::math::{container_height, item_to_px, px_to_cell, px_to_span, rows_for_px};
use super::model::{GridConfig, GridItem, PanelKind};
use crate::panels;
use crate::shell::{Db, Selection};

/// How often the resize net re-reads the grid's width when no event has told it to. Fast
/// enough that a border drag looks continuous rather than snapping, slow enough to be free:
/// one `getBoundingClientRect` on one element, and nothing at all unless the number moved.
const POLL_MS: u32 = 250;

/// The grid container's DOM id. Shared rather than spelled out at each site because three of
/// the four readers are JS strings (`layout_store::measure_grid_space`, the resize net below),
/// where a rename desyncs silently: `getElementById` returns null, every caller reads that as
/// "nothing to do", and the grid simply stops answering the window.
pub const GRID_ELEMENT_ID: &str = "desktop-grid";

/// The container's live pixel width, provided as context. Every surface derives
/// its own rectangle from this plus its cells, so a resize updates one signal
/// rather than re-rendering the container's whole child list by hand.
#[derive(Clone, Copy)]
pub struct ContainerWidth(pub Signal<f64>);

/// The column count the grid is currently drawing at, provided as context by the shell and
/// kept current by [`GridContainer`] from its own width measurement.
///
/// Context rather than a prop because the writers are scattered and none of them are the
/// grid: a layout is persisted under the count it was arranged at, and the surfaces that
/// commit one include the header's fold control, the quickbar's pills and the visibility
/// modal — all of them outside [`GridContainer`], all of them needing the same one fact
/// about the window.
#[derive(Clone, Copy)]
pub struct GridColumns(pub Signal<u32>);

/// Which gesture a live drag is: relocating the surface, or resizing it from its
/// south-east corner. FG4 ships that one corner — the only one that leaves the
/// item's origin fixed, so a resize never doubles as a move.
#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Move,
    Resize,
}

/// One live drag, either a move or a resize (`mode`). Everything here is in client
/// pixels except the cells it resolves to; the surface's landing cell and target
/// span are derived from these fields each render rather than stored, so there is
/// one source of truth for where it goes.
#[derive(Clone, PartialEq)]
struct DragState {
    mode: DragMode,
    /// The surface being dragged and its committed cell at grab time. The cell
    /// seeds the placeholder's `w`/`h` and is what a cancelled drag falls back to
    /// (the committed `state` is never touched until release, so cancel is free).
    item: GridItem,
    /// Offset subtracted from the pointer to recover the gesture's anchor corner:
    /// the surface's top-left in [`DragMode::Move`] (so it tracks under the grip),
    /// its bottom-right in [`DragMode::Resize`] (so the corner tracks the cursor).
    grab: (f64, f64),
    /// The grid container's top-left in client pixels, captured once at grab time.
    /// Converts the client-space pointer back into container-relative pixels that
    /// [`px_to_cell`] can snap.
    container_origin: (f64, f64),
    /// Live pointer position in client pixels.
    pointer: (f64, f64),
    /// The pointer's position at grab time; `moved` flips true once the pointer
    /// travels past the threshold, which is what tells a real drag from a click
    /// that happens to land on the drag handle.
    origin_pointer: (f64, f64),
    moved: bool,
}

impl DragState {
    /// Below this many pixels of travel the gesture is still a click, not a drag —
    /// nothing lifts and the layout is left alone.
    const MOVE_THRESHOLD_PX: f64 = 3.0;

    /// The gesture's anchor corner in container-relative pixels (client-space
    /// pointer, minus the grab offset, minus the container origin): the surface's
    /// top-left in [`DragMode::Move`], its bottom-right in [`DragMode::Resize`].
    fn anchor(&self) -> (f64, f64) {
        (
            self.pointer.0 - self.grab.0 - self.container_origin.0,
            self.pointer.1 - self.grab.1 - self.container_origin.1,
        )
    }

    /// The cell the surface would land in if released now (move only).
    fn landing_cell(&self, config: &GridConfig, container_width: f64) -> (u32, u32) {
        px_to_cell(self.anchor(), config, container_width)
    }

    /// The unsnapped pixel size of a resize: the bottom-right anchor minus the
    /// item's committed top-left. Floored at zero so dragging past the origin
    /// doesn't produce a negative box. This drives the live floating copy; the
    /// placeholder snaps it.
    fn resize_size_px(&self, config: &GridConfig, container_width: f64) -> (f64, f64) {
        let (left, top, _, _) = item_to_px(&self.item, config, container_width);
        let (right, bottom) = self.anchor();
        ((right - left).max(0.0), (bottom - top).max(0.0))
    }

    /// The cell span the surface would resize to if released now (resize only),
    /// before clamping.
    fn resized_span(&self, config: &GridConfig, container_width: f64) -> (u32, u32) {
        px_to_span(
            self.resize_size_px(config, container_width),
            config,
            container_width,
        )
    }
}

/// A live keyboard-driven placement (FG6), the pointer drag's cell-native twin.
/// Where a pointer drag tracks pixels and snaps, this edits integer cells directly:
/// arrows nudge `working`'s origin, Shift+arrows its span, and the committed layout
/// is never touched until Enter. Escape just drops this, so the surface snaps back
/// to its committed cell — that cell *is* the origin, so no separate copy is kept.
#[derive(Clone, Copy, PartialEq)]
struct KeyboardMove {
    /// The candidate rectangle being edited. Its `panel` identifies which surface
    /// is in a keyboard session; only that surface reads it instead of committed.
    working: GridItem,
}

/// The surface's own top and bottom hairlines (`.panel { border: 1px solid var(--seam) }`),
/// which sit outside both of the boxes a content fit measures. The fit has to pay for them
/// or a fitted body's last line lands under the bottom border and `.panel`'s
/// `overflow: hidden` takes it.
const SURFACE_BORDER_PX: f64 = 2.0;

/// Records one measured box's height into `slot`, and stops the event at the box it describes.
///
/// The stop is the reason this is a function rather than two closures. A resize reading is only
/// ever an answer about its own element, and the two renderers disagree about who else gets to
/// hear it: the web renderer forwards the DOM event's real `bubbles` flag, and the synthetic
/// resize event is dispatched non-bubbling, so it stays put. The native webview's interpreter
/// forwards *every* event as bubbling regardless of the flag, so on desktop a surface's reading
/// climbs to [`GridContainer`]'s own `onresize` and re-answers the grid's width with a header's
/// — which narrows every surface, which resizes these boxes, which bubbles again. The grid winds
/// itself down to a single narrow column in a few frames.
fn record_height(evt: Event<ResizeData>, mut slot: Signal<f64>) {
    evt.stop_propagation();
    if let Ok(size) = evt.get_border_box_size() {
        slot.set(size.height);
    }
}

/// True for the space bar, which arrives as a single-character key. Space both
/// begins and commits a keyboard session (alongside Enter), so it needs its own
/// test — `Key::Enter` doesn't cover it.
fn is_space(key: &Key) -> bool {
    matches!(key, Key::Character(c) if c == " ")
}

/// The keyboard placement state machine for one surface's grip button (FG6). Kept
/// out of `rsx!` so the flow reads top-to-bottom. Cell-native throughout — no pixel
/// or container-width math — because arrows step whole cells; the committed `state`
/// changes only on the Enter/Space commit, which runs the pure [`place_item`] once.
fn keyboard_keydown(
    evt: &Event<KeyboardData>,
    item: GridItem,
    // `title` is handed in rather than read off the kind: a dashboard's name is the user's, and
    // this is a free fn with no context to read it from.
    title: &str,
    config: &GridConfig,
    mut keyboard: Signal<Option<KeyboardMove>>,
    mut announce: Signal<String>,
    mut state: Signal<Vec<GridItem>>,
    drag: Signal<Option<DragState>>,
) {
    let key = evt.key();

    // The session for *this* surface, if one is live (a keydown on another surface's
    // grip while it holds the session is not ours to act on).
    let Some(working) = (*keyboard.peek())
        .filter(|k| k.working.panel == item.panel)
        .map(|k| k.working)
    else {
        // No session yet: Enter/Space starts one, unless a pointer drag is running.
        if (key == Key::Enter || is_space(&key)) && drag.peek().is_none() {
            evt.prevent_default();
            keyboard.set(Some(KeyboardMove { working: item }));
            announce.set(format!(
                "Moving {title}. Arrow keys move, Shift plus arrow keys resize, Enter to place, Escape to cancel."
            ));
        }
        return;
    };

    // Enter/Space places the surface: commit the working rectangle through the pure
    // engine (which pushes and compacts), persist, and end the session.
    if key == Key::Enter || is_space(&key) {
        evt.prevent_default();
        let mut items = state.peek().clone();
        place_item(
            &mut items,
            working.panel,
            working.x,
            working.y,
            working.w,
            working.h,
            config,
        );
        crate::layout_store::persist_desktop(&items, config.columns);
        state.set(items);
        keyboard.set(None);
        announce.set(format!(
            "{title} placed at row {}, column {}.",
            working.y + 1,
            working.x + 1
        ));
        return;
    }

    // Escape reverts: the committed layout was never touched, so dropping the
    // session snaps the surface back to its origin cell.
    if key == Key::Escape {
        evt.prevent_default();
        keyboard.set(None);
        announce.set(format!("{title} move cancelled."));
        return;
    }

    if !matches!(
        key,
        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
    ) {
        return;
    }
    evt.prevent_default();

    let mut next = working;
    let message = if evt.modifiers().shift() {
        // A folded surface is drawn at its header height, not `h`, so a vertical resize
        // would move a number nothing on screen reflects. Say that instead — the pointer
        // resize handles it by having no handle at all while folded.
        if working.collapsed && matches!(key, Key::ArrowUp | Key::ArrowDown) {
            announce.set(format!(
                "{title} is collapsed. Expand it to resize its height."
            ));
            return;
        }
        // A fitted surface's height belongs to its content. `clamp_resize` already holds it,
        // so the keys would be silently inert; say so instead, the way the collapsed case
        // does, rather than announcing a resize that didn't happen.
        if working.panel.fits_content() && matches!(key, Key::ArrowUp | Key::ArrowDown) {
            announce.set(format!("{title} sizes itself to its content."));
            return;
        }
        // Shift+arrows resize from the fixed south-east corner, clamped exactly as
        // the pointer resize is, so the announced size is the size that will commit.
        let (requested_w, requested_h) = match key {
            Key::ArrowLeft => (working.w.saturating_sub(1), working.h),
            Key::ArrowRight => (working.w + 1, working.h),
            Key::ArrowUp => (working.w, working.h.saturating_sub(1)),
            Key::ArrowDown => (working.w, working.h + 1),
            _ => unreachable!(),
        };
        let (w, h) = clamp_resize(&working, requested_w, requested_h, config);
        next.w = w;
        next.h = h;
        format!("{title} resized to {w} wide by {h} tall.")
    } else {
        // Arrows nudge the origin one cell, clamped to the grid (x to the right edge,
        // y saturating at the top).
        match key {
            Key::ArrowLeft => next.x = working.x.saturating_sub(1),
            Key::ArrowRight => next.x = (working.x + 1).min(config.columns - working.w),
            Key::ArrowUp => next.y = working.y.saturating_sub(1),
            Key::ArrowDown => next.y = working.y + 1,
            _ => unreachable!(),
        }
        format!("{title} at row {}, column {}.", next.y + 1, next.x + 1)
    };

    keyboard.set(Some(KeyboardMove { working: next }));
    announce.set(message);
}

/// The desktop grid surface. Measures its own pixel width on mount and on every
/// resize into a [`ContainerWidth`] context, sets its height from the lowest
/// item edge (absolute children can't stretch it), and renders each surface.
///
/// First paint is gated on a real measurement: the webview reports width
/// asynchronously (`get_client_rect` is a future), so rendering surfaces before
/// the width is known would place them all at a zero-width column and then jump.
#[component]
pub fn GridContainer(
    database: Db,
    selection: Signal<Selection>,
    state: Signal<Vec<GridItem>>,
) -> Element {
    let drag = use_signal(|| Option::<DragState>::None);
    // The one surface (if any) in a keyboard placement session, and the polite
    // screen-reader narration of the latest gesture step. Keyboard moves are
    // discrete keypresses, not per-frame pointer motion, so — unlike the pointer
    // drag's committed/in-flight split — every surface may read `keyboard` directly;
    // a keydown re-rendering all six surfaces is cheap.
    let keyboard = use_signal(|| Option::<KeyboardMove>::None);
    let announce = use_signal(String::new);

    let width = use_signal(|| 0.0_f64);
    use_context_provider(|| ContainerWidth(width));
    // The vertical room the grid has, kept current by the same net that keeps `width` current.
    // Zero until the first reading, which is what the re-fit below gates on.
    let available = use_signal(|| 0.0_f64);

    // Derived per render from the measured width, not a literal: the count is what the
    // window can pay for at `MIN_COL_PX` a column (see `GridConfig::for_width`). Reads the
    // widest count until the first measurement lands, which is what `measured` gates paint on
    // anyway — an unmeasured grid must not spend that gap laid out for a small window.
    let config = use_memo(move || GridConfig::for_width(width()));
    let mut grid_columns = use_context::<GridColumns>().0;
    // Which surfaces are out in their own OS windows. Read here rather than at each use, and
    // read TRACKED at both of them, because a pop-out has to repaint the grid it just left.
    let popped = use_context::<crate::panel_popout::PoppedOut>();
    // Resolved to an owned set before `rsx!`, like every other read here: the draw filter below
    // runs per surface, and a guard held across the whole render is what the house rule about
    // keeping logic above `rsx!` exists to stop. Tracked, so a pop-out repaints the grid.
    let popped_now = popped.0.read().clone();
    // The roster every completion here is measured against. Read once at the top, because a
    // layout reconciled against the wrong roster is a layout with orphan or missing surfaces.
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;

    let height = use_memo(move || container_height(&state.read(), &config()));

    // Crossing a column-count boundary re-answers the layout, because a layout is cells and
    // the same cells are a different arrangement at a different count. The saved layout for
    // the count being entered wins; failing that, the default authored FOR that count
    // (`default_layout_for`), fitted to the room the window has exactly as a fresh install is.
    //
    // Seeded at the widest count rather than at zero, so a window that opens wide never runs
    // this: the shell's mount future has already loaded that layout, and re-entering the count
    // it just settled would be a second load racing the first. A window that opens narrow does
    // run it, reaching the same answer the mount future reaches — the same measurement of the
    // same element — so the duplicate write is the same layout twice, not two layouts fighting.
    let mut applied = use_signal(|| GridConfig::default().columns);
    use_effect(move || {
        let columns = config().columns;
        if width() <= 0.0 || columns == applied() {
            return;
        }
        applied.set(columns);
        grid_columns.set(columns);
        spawn(async move {
            let roster = crate::panels::dashboards::surfaces(&dashboards.peek());
            if let Some(saved) = crate::layout_store::load_desktop(columns, &roster).await {
                state.set(saved);
                return;
            }
            let rows = match crate::layout_store::measure_grid_space().await {
                Some(space) => crate::grid::model::column_rows_for(space.height, &config()),
                None => crate::grid::model::DEFAULT_COLUMN_ROWS,
            };
            crate::layout_store::mark_layout_authored();
            state.set(GridItem::default_layout_for(columns, rows, &roster));
        });
    });

    // The height axis' half of the same idea, and the reason the grid stops leaving a band of
    // unused window under itself. The columns are re-authored at the height the room now pays
    // for, so a taller window grows the four columns rather than stranding the space below
    // them. `row_height` deliberately stays constant: it is the divisor `rows_for_px` converts
    // a fitted surface's pixels through, so deriving it from the layout's own extent would
    // feed back into the extent it was derived from.
    //
    // The slack goes to the columns and not to every row for the same reason. The band
    // surfaces fit their content, so stretching them adds blank space inside a panel that
    // already had the height it wanted, while the columns are the ones with more to show.
    //
    // Only while the layout is the app's own — the moment the user arranges anything it is
    // theirs, and re-fitting would re-author over every hide and every drag.
    //
    // Keyed on the tallest non-fitted surface rather than on the whole layout, because the
    // band's heights are measured and land a frame later: comparing layouts would see its own
    // fit as a difference and re-author forever.
    //
    // The key is compared against the SAME key over the layout that would be authored, never
    // against `column_rows_for`'s answer directly. [`tallest_authored_rows`] is a proxy for the
    // column height and not the column height itself, and comparing a proxy against the thing
    // it stands for is what left 134px of dead window under a regrown layout. Building the
    // candidate first also means the roster, the column count and
    // the pop-out reapply are all inputs to the decision rather than things done after it.
    use_effect(move || {
        let room = available();
        let config = config();
        if room <= 0.0 || width() <= 0.0 || crate::layout_store::layout_is_user_arranged() {
            return;
        }
        let rows = crate::grid::model::column_rows_for(room, &config);
        let roster = crate::panels::dashboards::surfaces(&dashboards.peek());
        let mut items = GridItem::default_layout_for(config.columns, rows, &roster);
        // A rebuilt default has no memory of a surface being out in its own window, and this
        // one fires on a window resize — no click involved. Applied before the comparison, not
        // after: a popped-out surface is hidden, the key skips hidden surfaces, and a candidate
        // keyed before the reapply would disagree with the live layout over a pop-out alone.
        popped.reapply(&mut items);
        if crate::grid::model::tallest_authored_rows(&state.peek())
            == crate::grid::model::tallest_authored_rows(&items)
        {
            return;
        }
        state.set(items);
    });
    let measured = width() > 0.0;
    // A surface that is out in its own window is not on the grid either, so an empty grid is
    // still an empty grid while one is popped out. Same predicate as the draw filter below, and
    // for the same reason it has to be: these two are the only readers that decide whether the
    // page has anything on it.
    let all_hidden = use_memo(move || {
        let out = popped.0.read();
        state
            .read()
            .iter()
            .all(|item| item.hidden || out.contains(&item.panel))
    });

    // Only the *active* drag's slug — not the whole `DragState` — so the container
    // element re-renders when a drag begins and ends, not on every pointer move.
    // It rides on `data-drag` to hide the origin surface (see `.surface` CSS)
    // while its cursor-following copy is drawn by `DragLayer`.
    let dragging_slug = use_memo(move || {
        drag.read()
            .as_ref()
            .filter(|d| d.moved)
            .map(|d| d.item.panel.slug())
    });
    // Whether a drag is live at all, as a memo so gating the overlay on it doesn't
    // subscribe the container to every pointer move — only the None↔Some edges.
    let drag_active = use_memo(move || drag.read().is_some());
    // The overlay's cursor modifier for the live gesture, memoized so it re-renders the
    // overlay on the gesture's edges (it never changes mid-drag) rather than on every
    // pointer move. A resize of a surface that fits its content moves one axis only —
    // `clamp_resize` holds its height — so that gesture must not advertise the diagonal.
    let drag_cursor = use_memo(move || match drag.read().as_ref() {
        Some(d) if d.mode != DragMode::Resize => "",
        Some(d) if d.item.panel.fits_content() => "drag-overlay--resize-x",
        Some(_) => "drag-overlay--resize",
        None => "",
    });

    // Escape cancels an in-flight drag. A document-level listener rather than the
    // overlay's own keydown: focusing the transient overlay is unreliable in the
    // webview, and a captured drag can pin focus elsewhere. The guard clears only a
    // live drag and never prevents default, so Escape stays free for everything
    // else. Re-registered per mount, replacing the prior listener (dataset switch
    // remounts the grid), so exactly one is ever bound.
    use_future(move || async move {
        let mut drag = drag;
        let mut eval = document::eval(
            "if (window.__skGridEsc) document.removeEventListener('keydown', window.__skGridEsc);\
             window.__skGridEsc = (e) => { if (e.key === 'Escape') dioxus.send('esc'); };\
             document.addEventListener('keydown', window.__skGridEsc);",
        );
        while eval.recv::<String>().await.is_ok() {
            if drag.peek().is_some() {
                drag.set(None);
            }
        }
    });

    // The safety net under the element's own `onresize` below, which is the primary and can
    // miss. A missed notification is silent, because the handler's only response to not being
    // called is to keep the width it has — indistinguishable from a window that never moved.
    // The grid then stays placed against a window that is no longer there: dead space when the
    // border goes out, surfaces off the edge when it comes in. Both directions of one stale
    // measurement, which is what makes it read as two separate bugs.
    //
    // **The poll is the load-bearing part, not the listeners.** Dragging a window border was
    // measured not to deliver either a `resize` event or a ResizeObserver callback in the
    // reporter's environment, while a programmatic resize delivers both — which is exactly the
    // case an automated check drives, so every test here passes while the app is frozen on a
    // real desktop. An input the layout cannot function without must not depend on being told;
    // 4 Hz against one `getBoundingClientRect` is the price of never being stuck, and the
    // change filter below means a window that isn't moving costs one DOM read and nothing else.
    //
    // The listeners stay in front of it so the common case is still instant rather than up to
    // 250ms late. All three routes share one `send`, so the net cannot disagree with itself,
    // and it reads the grid element's OWN width — the same number the observer would report.
    //
    // Registered like the Escape listener above and for the same reason: the guard tears down
    // the prior interval and listeners before binding, so a remount leaves exactly one set.
    use_future(move || async move {
        let mut width = width;
        let mut available = available;
        let mut eval = document::eval(&format!(
            "if (window.__skGridResize) {{\
               removeEventListener('resize', window.__skGridResize);\
               document.removeEventListener('visibilitychange', window.__skGridResize);\
               clearInterval(window.__skGridPoll);\
             }}\
             let lastW = -1, lastH = -1;\
             const send = () => {{\
               const el = document.getElementById('{GRID_ELEMENT_ID}');\
               if (!el) return;\
               const r = el.getBoundingClientRect();\
               const pad = parseFloat(getComputedStyle(el.parentElement).paddingBottom) || 0;\
               const h = window.innerHeight - (r.top + window.scrollY) - pad;\
               if (r.width > 0 && (r.width !== lastW || h !== lastH)) {{\
                 lastW = r.width; lastH = h; dioxus.send([r.width, h]);\
               }}\
             }};\
             let queued = false;\
             window.__skGridResize = () => {{\
               if (queued) return;\
               queued = true;\
               requestAnimationFrame(() => {{ queued = false; send(); }});\
             }};\
             addEventListener('resize', window.__skGridResize);\
             document.addEventListener('visibilitychange', window.__skGridResize);\
             window.__skGridPoll = setInterval(send, {POLL_MS});"
        ));
        while let Ok(pair) = eval.recv::<Vec<f64>>().await {
            let (Some(&px), Some(&room)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if px > 0.0 && px != *width.peek() {
                width.set(px);
            }
            // The room below the grid's top edge, measured through the document so a scrolled
            // page reports the same budget an unscrolled one does. Zero or less means the grid
            // is off screen; keep the last real answer rather than fitting to nothing.
            if room > 0.0 && room != *available.peek() {
                available.set(room);
            }
        }
    });

    rsx! {
        div {
            id: GRID_ELEMENT_ID,
            "data-drag": dragging_slug().unwrap_or_default(),
            style: "position: relative; height: {height}px;",
            onmounted: move |evt| {
                let mut width = width;
                async move {
                    if let Ok(rect) = evt.get_client_rect().await {
                        width.set(rect.width());
                    }
                }
            },
            // The primary width input. An `Err` here, or a notification the engine never
            // delivers, leaves the width untouched on purpose: the window-resize net above
            // re-reads this element and answers instead. This is the only path that catches a
            // width change the window did NOT cause (a scrollbar appearing, the header
            // rewrapping), which is why the net does not replace it.
            onresize: move |evt| {
                let mut width = width;
                if let Ok(size) = evt.get_border_box_size() {
                    width.set(size.width);
                }
            },

            if measured {
                // Hidden surfaces keep their entry in the layout (see `GridItem::hidden`)
                // but draw nothing; every fn the grid's geometry runs through already
                // treats them as absent, so this is the last place that has to.
                //
                // Popped-out surfaces are hidden too, and are filtered here ANYWAY rather than
                // relying on that. A pop-out commits the hide through `set_panel_hidden`, but
                // four other writers rebuild the layout from the authored default and drop the
                // flag on the floor — so "hidden" is how the grid closes the gap, and this is
                // what makes drawing a surface that is live in another window impossible rather
                // than merely unlikely. It is one read, at the one place that draws.
                for item in state
                    .read()
                    .iter()
                    .copied()
                    .filter(|item| !item.hidden && !popped_now.contains(&item.panel))
                {
                    GridSurface {
                        key: "{item.panel.slug()}",
                        item,
                        config: config(),
                        drag,
                        keyboard,
                        announce,
                        state,
                        database: database.clone(),
                        selection,
                    }
                }

                DragPlaceholder { drag, config: config() }
                DragLayer { drag, config: config(), database: database.clone(), selection }
            }
        }

        // A grid with every surface hidden derives a height of zero, so the prompt sits
        // OUTSIDE the container rather than inside a box with no room to draw it.
        if all_hidden() {
            crate::panel_visibility::AllPanelsHidden {}
        }

        if drag_active() {
            DragOverlay { state, drag, config: config(), cursor: drag_cursor() }
        }

        // The keyboard gesture's running commentary for assistive tech. A visually
        // hidden polite live region: each step (enter, nudge, resize, place, cancel)
        // replaces the text, and the screen reader speaks it without stealing focus
        // from the handle. Empty until the first keyboard step.
        div {
            class: "sr-only",
            "aria-live": "polite",
            role: "status",
            "{announce}"
        }
    }
}

/// One committed surface, absolutely positioned at its cell. It renders from its
/// own `item` prop, except while *it* is the surface in a keyboard session — then
/// it renders at that session's `working` cell so the panel visibly moves under the
/// arrow keys, with the committed layout still untouched until Enter. A live pointer
/// drag re-renders the drag layer, not these. The header is the pointer drag handle
/// (`onpointerdown` seeds a [`DragState`]); the grip button is the keyboard handle.
#[component]
fn GridSurface(
    item: GridItem,
    config: GridConfig,
    drag: Signal<Option<DragState>>,
    keyboard: Signal<Option<KeyboardMove>>,
    announce: Signal<String>,
    state: Signal<Vec<GridItem>>,
    database: Db,
    selection: Signal<Selection>,
) -> Element {
    let container_width = use_context::<ContainerWidth>().0;
    let stats_config_open = use_context::<crate::panels::stats_config::StatsConfigOpen>().0;
    // What this surface is called. Resolved once here rather than at each of the six places
    // below that say it, because for a dashboard the answer is the user's typed name and lives
    // in the roster, not on the kind.
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let title = crate::panels::dashboards::title_of(item.panel, &dashboards.read());
    let slug = item.panel.slug();
    // Is THIS the surface whose floating copy is being drawn? Mirrors `dragging_slug`'s own
    // `moved` filter, so a press that never became a drag hides nothing.
    let dragging = drag
        .read()
        .as_ref()
        .is_some_and(|state| state.moved && state.item.panel == item.panel);

    // A fitted surface ([`PanelKind::fits_content`]) measures itself instead of carrying a
    // chosen height. Both boxes are observed rather than one, because neither alone is the
    // answer: the header's height is a density token the app is free to retune, and the body
    // is the only box whose height is the content's.
    let fits = item.panel.fits_content();
    let head_px = use_signal(|| 0.0_f64);
    let body_px = use_signal(|| 0.0_f64);
    // The one field of `item` the effect below may hold. Every other field changes under a
    // surface — `collapsed` and `h` most of all — and the effect's closure is built once, on
    // first render, so anything captured there is frozen at whatever it was then. `panel` is
    // the exception because the container keys each surface by its slug: this component
    // instance renders that surface for its whole life or is replaced outright.
    let panel = item.panel;

    // Commit from an effect rather than from either `onresize`, so the two measurements
    // coalesce: they arrive separately on first paint, and a commit off whichever landed
    // first would fit the surface to half of itself and then correct it a frame later.
    //
    // Deliberately NOT persisted. A measured height is a fact about this viewport's width,
    // not about the layout, so writing it to storage would restore a 1920px fit onto a
    // 1366px window — and it would write on every content change besides. The stored `h`
    // stays whatever the last real gesture left, and the first measurement after a load
    // overrules it.
    use_effect(move || {
        if !fits {
            return;
        }
        // Zero until the first measurement lands. There is nothing to fit to yet, and
        // `rows_for_px`'s floor would otherwise fit every surface to two rows on mount.
        let content = head_px() + body_px();
        if content <= 0.0 {
            return;
        }
        let rows = rows_for_px(content + SURFACE_BORDER_PX, &config);
        let mut state = state;
        // The surface's LIVE rectangle, never the `item` prop. This closure is built once, on
        // first render, so anything read off a captured `item` is frozen at what it was then —
        // which is why `panel` is the only field held above.
        //
        // Read REACTIVELY rather than peeked, so this effect re-runs when the layout is written
        // by anyone. That makes the fit self-repairing, and it has to be: a measurement is
        // discarded by any write that re-authors the surface's height, and nothing re-fires a
        // resize observer for a box whose content did not change. The mount future is exactly
        // such a write — it lands an authored default AFTER the first measurement has been
        // committed — so with a peek here the surface keeps the authored seed forever and
        // clips whatever the seed was too short for.
        //
        // It converges rather than looping: the write below changes the height, the re-run
        // measures the same content, and `live.h == rows` returns before taking the borrow
        // again. A sibling displaced by the compaction re-runs too and returns on the same
        // check, since its own height did not change.
        let Some(live) = state.read().iter().copied().find(|i| i.panel == panel) else {
            return;
        };
        // A folded surface draws no body and wears a tighter header, while `h` still means the
        // height it unfolds to — so nothing measurable right now describes the number being
        // written. Wait for the unfold, which resizes both boxes and re-runs this.
        //
        // And skip a measurement that agrees with the height already stored, which is the
        // common case by far: taking the mutable borrow anyway would mark the whole layout
        // dirty and re-render every surface for nothing.
        if live.collapsed || live.h == rows {
            return;
        }
        // The write goes through the live vector, never through clone-mutate-set. A whole
        // column of surfaces re-fits in one tick when the dashboard's stat set changes, and
        // a clone taken before a sibling's commit writes back over it — which showed up as
        // exactly one surface in the column keeping a height a row short of its content,
        // clipped and with no scrollbar left to reach what it lost.
        state.with_mut(|items| {
            fit_item(items, panel, rows);
        });
    });

    // This surface's live rectangle: its committed cell, unless it is the one being
    // placed by keyboard, in which case the in-progress `working` cell.
    let keyboard_working = keyboard()
        .filter(|k| k.working.panel == item.panel)
        .map(|k| k.working);
    let rendered = keyboard_working.unwrap_or(item);
    // Whole pixels, placed with `left`/`top` rather than a transform. A transformed surface is
    // composited as its own layer, and Chrome draws text in a layer with greyscale smoothing
    // instead of ClearType; the column widths divide the container into thirds, so the offsets
    // also landed between pixels. Together they made the whole panel read softer than the beta.
    let (x, y, width, height) = item_to_px(&rendered, &config, container_width());
    let (x, y, width, height) = (x.round(), y.round(), width.round(), height.round());
    let placing = keyboard_working.is_some();

    let mut classes = String::from("panel surface");
    if placing {
        classes.push_str(" surface--placing");
    }
    if item.collapsed {
        classes.push_str(" is-collapsed");
    }

    rsx! {
        section {
            class: "{classes}",
            "data-id": "{slug}",
            // Hiding the origin surface while its floating copy is drawn is this attribute's only
            // job. It sits on the surface rather than pairing the container's dragged slug
            // against each `data-id` in CSS, because CSS cannot compare an ancestor's attribute
            // VALUE to a descendant's — which is why the rule it replaces had to enumerate one
            // selector per surface, and why it went stale the moment a surface was added or
            // retired. A dashboard's slug could not be enumerated at all.
            "data-dragging": dragging,
            "aria-label": "{title}",
            style: "position: absolute; left: {x}px; top: {y}px; width: {width}px; height: {height}px;",

            header {
                class: "panel-head",
                // The header's contribution to a content fit. Recorded unconditionally, even
                // while folded (where `.panel.is-collapsed` tightens this box by 3px and the
                // reading describes nothing the fit wants). Filtering here looked like the
                // tidier place for it and is the one thing that must not happen: a dropped
                // measurement is permanent, because nothing re-fires an observer for a box
                // that has stopped changing, and the surface then fits forever against a
                // header height of zero. A *premature* reading costs nothing by comparison —
                // the effect below repairs it the moment the next one lands.
                onresize: move |evt| record_height(evt, head_px),
                onpointerdown: move |evt| {
                    let mut drag = drag;
                    // Suppress the browser's native selection gesture: without this the
                    // same press that starts a drag also anchors a text selection, which
                    // extends into whatever panel body the pointer drags over.
                    evt.prevent_default();
                    // A mouse grab abandons any keyboard session cleanly — the two
                    // gestures never run at once.
                    keyboard.set(None);
                    let pointer = evt.client_coordinates();
                    let grab = evt.element_coordinates();
                    // The container origin is recovered from the surface's own
                    // committed rectangle rather than measured: the pointer sits at
                    // container_origin + surface_top_left + grab, so subtract the two
                    // knowns to get the third. Avoids an async rect read on grab.
                    let (surface_x, surface_y, _, _) = item_to_px(&item, &config, container_width());
                    drag.set(Some(DragState {
                        mode: DragMode::Move,
                        item,
                        grab: (grab.x, grab.y),
                        container_origin: (pointer.x - grab.x - surface_x, pointer.y - grab.y - surface_y),
                        pointer: (pointer.x, pointer.y),
                        origin_pointer: (pointer.x, pointer.y),
                        moved: false,
                    }));
                },
                // The keyboard drag handle. Tab-focusable (its `pointer-events:none`,
                // inherited from `.panel-head > *`, only bars the mouse — the header
                // owns pointer drags); Enter/Space begins a placement session, arrows
                // move by a cell, Shift+arrows resize, Enter/Space places, Escape
                // reverts. Every handled key is `prevent_default`ed so the page
                // neither scrolls on arrows nor click-fires on Space.
                button {
                    class: "grip",
                    r#type: "button",
                    "aria-label": "Move {title} panel",
                    "aria-pressed": placing,
                    onkeydown: {
                        // Cloned into the closure rather than moved: the five chrome controls
                        // below still need the name, and a `String` is not `Copy`.
                        let title = title.clone();
                        move |evt: Event<KeyboardData>| {
                            keyboard_keydown(
                                &evt, item, &title, &config, keyboard, announce, state, drag,
                            );
                        }
                    },
                    "⠿"
                }
                span { class: "panel-title", "{title}" }
            }

            // The fold control. A SIBLING of the header, not a child, for the same reason
            // the resize handle is one: `.panel-head > *` is pointer-events:none so the
            // header can own the drag gesture, and a button nested inside would never
            // receive the click. It commits through the pure engine exactly as a resize
            // does, because folding changes how many rows the surface occupies and the
            // layout has to reflow around it.
            button {
                class: "panel-fold",
                r#type: "button",
                "aria-expanded": !item.collapsed,
                "aria-label": if item.collapsed { "Expand {title} panel" } else { "Collapse {title} panel" },
                onclick: move |_| {
                    // A live keyboard placement holds a rectangle that is about to be stale.
                    // Drop it, exactly as a pointer grab does.
                    keyboard.set(None);
                    let mut items = state.peek().clone();
                    toggle_collapse(&mut items, item.panel);
                    crate::layout_store::persist_desktop(&items, config.columns);
                    state.set(items);
                },
                {crate::view::marks::chevron(item.collapsed)}
            }

            // The hide control, taking the surface off the grid entirely. A sibling of the
            // header for the same pointer-events reason as the fold, and parked outside it
            // so a hide can never be mistaken for the start of a drag. It commits through
            // the same one write path the modal's rows use.
            button {
                class: "panel-hide",
                r#type: "button",
                "aria-label": "Hide {title} panel",
                onclick: move |_| {
                    // A live keyboard placement holds a rectangle for a surface that is
                    // about to leave the grid. Drop it, exactly as the fold does.
                    keyboard.set(None);
                    crate::panel_visibility::set_panel_hidden(state, config.columns, item.panel, true);
                },
                {crate::view::marks::close()}
            }

            // A dashboard's gear, deep-linking the organizer to this panel's own card. A
            // sibling of the header for the same pointer-events reason as the fold control, and
            // absent on every surface that holds no stats.
            if let Some(dashboard) = item.panel.dashboard() {
                button {
                    class: "panel-config",
                    r#type: "button",
                    "aria-label": "Choose which stats {title} shows",
                    onclick: move |_| {
                        let mut open = stats_config_open;
                        open.set(Some(crate::panels::stats_config::OrganizerTarget::Panel(dashboard)));
                    },
                    {crate::view::marks::sliders()}
                }
            }

            // The pop-out control, taking the surface into its own OS window. A sibling of the
            // header for the same pointer-events reason as the fold, and outermost-but-one in the
            // lane: it is the only control here that is not about this window, so it sits beyond
            // the gear and inside the hide. Draws nothing on a surface that holds no stats, and
            // nothing at all off the desktop — see `crate::panel_popout`.
            crate::panel_popout::PopOutControl {
                item,
                title: title.clone(),
                layout: state,
                columns: config.columns,
            }

            if !item.collapsed {
                // A fitted surface's body is wrapped in a box that takes its content's
                // height (`.surface-fit` is `flex: 0 0 auto`) rather than the surface's, so
                // what the observer reports is what the content needs and not what the last
                // fit already granted it — measuring the panel's own box instead would be a
                // loop, each fit confirming itself.
                if fits {
                    div {
                        class: "surface-fit",
                        onresize: move |evt| record_height(evt, body_px),
                        SurfaceBody { panel: item.panel, database: database.clone(), selection }
                    }
                } else {
                    SurfaceBody { panel: item.panel, database: database.clone(), selection }
                }
            }

            // South-east resize handle. Grab is zeroed and the container origin is
            // recovered so the item's bottom-right corner sits under the pointer at
            // grab time (`resize_size_px` == the committed size, no jump); from
            // there the surface grows by exactly the pointer's travel, wherever in
            // the handle the grab began. The handle is a sibling of the header, so
            // `.panel-head > *` pointer-events don't touch it.
            //
            // A folded surface has none: its rendered height is its header, not `h`, so a
            // drag from that corner would be resizing a box the screen isn't showing.
            //
            // A fitted surface keeps the handle but only widens by it — `clamp_resize` holds
            // its height — so it carries the one-axis cursor rather than the diagonal.
            if !item.collapsed {
            div {
                class: if fits { "resize-handle resize-handle--width" } else { "resize-handle" },
                "aria-hidden": "true",
                onpointerdown: move |evt| {
                    let mut drag = drag;
                    // Same as the header grab: cancel the native selection gesture so a
                    // resize doesn't drag-select the body text under the corner.
                    evt.prevent_default();
                    let pointer = evt.client_coordinates();
                    let (surface_x, surface_y, surface_w, surface_h) =
                        item_to_px(&item, &config, container_width());
                    drag.set(Some(DragState {
                        mode: DragMode::Resize,
                        item,
                        grab: (0.0, 0.0),
                        container_origin: (
                            pointer.x - surface_x - surface_w,
                            pointer.y - surface_y - surface_h,
                        ),
                        pointer: (pointer.x, pointer.y),
                        origin_pointer: (pointer.x, pointer.y),
                        moved: false,
                    }));
                },
            }
            }
        }
    }
}

/// The snapped landing zone. A container-relative box at the cell the surface
/// would drop into, sized to the dragged item; it transitions between cells (CSS)
/// while the dragged surface tracks the cursor unsnapped above it.
#[component]
fn DragPlaceholder(drag: Signal<Option<DragState>>, config: GridConfig) -> Element {
    let container_width = use_context::<ContainerWidth>().0;
    let drag = drag.read();
    let Some(state) = drag.as_ref().filter(|d| d.moved) else {
        return rsx! {};
    };

    let target = match state.mode {
        DragMode::Move => {
            let (cell_x, cell_y) = state.landing_cell(&config, container_width());
            GridItem {
                x: cell_x,
                y: cell_y,
                ..state.item
            }
        }
        DragMode::Resize => {
            let (span_w, span_h) = state.resized_span(&config, container_width());
            let (w, h) = clamp_resize(&state.item, span_w, span_h, &config);
            GridItem { w, h, ..state.item }
        }
    };
    let (x, y, width, height) = item_to_px(&target, &config, container_width());

    rsx! {
        div {
            class: "grid-placeholder",
            style: "transform: translate3d({x}px, {y}px, 0); width: {width}px; height: {height}px;",
        }
    }
}

/// The dragged surface itself, following the cursor unsnapped. Fixed-positioned in
/// client space so it floats above the overlay; `pointer-events: none` so the
/// overlay underneath still receives the move/up that drives the drag.
#[component]
fn DragLayer(
    drag: Signal<Option<DragState>>,
    config: GridConfig,
    database: Db,
    selection: Signal<Selection>,
) -> Element {
    let container_width = use_context::<ContainerWidth>().0;
    let drag = drag.read();
    let Some(state) = drag.as_ref().filter(|d| d.moved) else {
        return rsx! {};
    };

    // Move: the whole surface follows the cursor at its committed size. Resize:
    // the top-left stays pinned at the committed cell (client-space) while the
    // box grows to the unsnapped pointer size, so the surface reads as stretching
    // in place. The origin surface is hidden either way (`data-drag`), so this
    // copy is what the eye tracks.
    let (left, top, width, height) = match state.mode {
        DragMode::Move => {
            let (_, _, w, h) = item_to_px(&state.item, &config, container_width());
            (
                state.pointer.0 - state.grab.0,
                state.pointer.1 - state.grab.1,
                w,
                h,
            )
        }
        DragMode::Resize => {
            let (surface_x, surface_y, _, _) = item_to_px(&state.item, &config, container_width());
            let (w, h) = state.resize_size_px(&config, container_width());
            (
                state.container_origin.0 + surface_x,
                state.container_origin.1 + surface_y,
                w,
                h,
            )
        }
    };
    let panel = state.item.panel;
    let collapsed = state.item.collapsed;
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let title = crate::panels::dashboards::title_of(panel, &dashboards.read());

    rsx! {
        section {
            class: if collapsed { "panel surface surface--dragging is-collapsed" } else { "panel surface surface--dragging" },
            "aria-hidden": "true",
            style: "position: fixed; transform: translate3d({left}px, {top}px, 0); width: {width}px; height: {height}px;",

            header { class: "panel-head",
                span { class: "grip", "⠿" }
                span { class: "panel-title", "{title}" }
            }

            if !collapsed {
                SurfaceBody { panel, database, selection }
            }
        }
    }
}

/// The fixed, full-viewport surface that owns a live drag. It exists only while a
/// drag is in flight; it tracks the pointer (flipping `moved` once past the
/// threshold), commits on release through the pure engine, and cancels on Escape.
/// Full-viewport so a drag survives the pointer leaving the grid.
#[component]
fn DragOverlay(
    state: Signal<Vec<GridItem>>,
    drag: Signal<Option<DragState>>,
    config: GridConfig,
    /// The gesture's cursor modifier class, empty for a move. Decided by the container,
    /// which is the only place that knows both the mode and which surface is under it.
    cursor: &'static str,
) -> Element {
    let container_width = use_context::<ContainerWidth>().0;

    rsx! {
        div {
            class: "drag-overlay {cursor}",
            onpointermove: move |evt| {
                let mut drag = drag;
                let point = evt.client_coordinates();
                drag.with_mut(|active| {
                    let Some(active) = active.as_mut() else { return };
                    active.pointer = (point.x, point.y);
                    if !active.moved {
                        let dx = point.x - active.origin_pointer.0;
                        let dy = point.y - active.origin_pointer.1;
                        if dx * dx + dy * dy > DragState::MOVE_THRESHOLD_PX.powi(2) {
                            active.moved = true;
                        }
                    }
                });
            },
            onpointerup: move |_| {
                let mut drag = drag;
                let mut state = state;
                if let Some(active) = drag.peek().clone() {
                    if active.moved {
                        let mut items = state.peek().clone();
                        match active.mode {
                            DragMode::Move => {
                                let (cell_x, cell_y) = active.landing_cell(&config, container_width());
                                move_item(&mut items, active.item.panel, cell_x, cell_y, &config);
                            }
                            DragMode::Resize => {
                                let (span_w, span_h) = active.resized_span(&config, container_width());
                                resize_item(&mut items, active.item.panel, span_w, span_h, &config);
                            }
                        }
                        // Persist the committed layout, mirroring mobile's persist-on-reorder.
                        crate::layout_store::persist_desktop(&items, config.columns);
                        state.set(items);
                    }
                }
                drag.set(None);
            },
        }
    }
}

/// Resolve a [`PanelKind`] to its content component. The grid stores which
/// surface, never the component — so a saved layout survives a component
/// refactor (the surface-registry indirection from the skill).
#[component]
fn SurfaceBody(panel: PanelKind, database: Db, selection: Signal<Selection>) -> Element {
    // Exhaustive on purpose, and it must stay that way. A `_ => rsx! {}` arm is the shortcut
    // this shape invites — it compiles, it silences the one variant you forgot, and it draws a
    // permanently blank panel that looks like a data problem rather than a missing arm.
    match panel {
        PanelKind::Powers => rsx! { panels::powers::PowersPanel { database } },
        PanelKind::Available => rsx! { panels::powers::AvailablePanel { database } },
        PanelKind::Pools => rsx! { panels::powers::PoolsPanel { database } },
        PanelKind::SetBonuses => rsx! { panels::set_bonus_totals::SetBonusTotals { database } },
        PanelKind::Info => rsx! { panels::info::Info { database, selection } },
        PanelKind::Dashboard(id) => rsx! { panels::stats::StatGroup { panel: id } },
    }
}
