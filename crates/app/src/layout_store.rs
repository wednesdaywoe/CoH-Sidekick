//! Layout persistence — where the surfaces sit on the desktop grid, their order in the
//! mobile stack, and how the Powers panel arranges its cards — over the same localStorage
//! channel `theme.rs` uses, identical in the desktop webview and the browser (both go
//! through the document's JS context). Invalid or corrupt persisted state never
//! reaches the app: all load fns validate and the caller keeps the default on
//! `None`. Versioned storage with legacy migration fallback for upgrades.

use crate::grid::collide::{reconcile_layout, validate_layout};
use crate::grid::model::{GridConfig, GridItem, PanelKind};
use crate::grid::{FlowLayout, GridLayout};
use crate::mobile_order::MobileOrder;
use crate::panel_popout::PoppedRecord;
use crate::panels::powers::PowersLayout;
use crate::quickbar::model::{validate_pins, QuickBarItem};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const KEY_DESKTOP: &str = "sk-layout";
pub(crate) const KEY_MOBILE: &str = "sk-mobile-order";
pub(crate) const KEY_POWERS: &str = "sk-powers-layout";
pub(crate) const KEY_QUICKBAR: &str = "sk-quickbar";
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
const KEY_POPPED: &str = "sk-popped";

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredDesktopLayout {
    /// Row-packed `FlowLayout`, the pre-free-grid format. Read-only: a stored V2
    /// is migrated to `GridItem`s and re-saved as V3 on next load.
    #[serde(rename = "2")]
    V2(FlowLayout),
    /// The free grid's `{x,y,w,h}` cells at twelve columns. Read-only since V4: a stored V3
    /// is the layout its author arranged at the only column count that existed, so it migrates
    /// into V4's twelve-column slot and is left alone at every other count.
    #[serde(rename = "3")]
    V3(Vec<Retirable<GridItem>>),
    /// One layout per column count, keyed by the count it was arranged at.
    ///
    /// A layout is cells, and a cell means nothing without the count it was measured in — a
    /// six-cell Powers is half the grid at twelve columns and three quarters at eight. Storing
    /// one layout and reflowing it across counts would mean a window narrowed once and widened
    /// back does not restore the arrangement it started with, because reflow is lossy in the
    /// direction that matters (a wrapped column cannot remember it was ever beside its
    /// neighbour). A slot per count is the same answer the app already gives the 900px line,
    /// where the desktop grid and the mobile stack keep separate saves for the same reason.
    #[serde(rename = "4")]
    V4(BTreeMap<u32, Vec<Retirable<GridItem>>>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredMobileOrder {
    #[serde(rename = "1")]
    V1(Vec<Retirable<PanelKind>>),
}

/// What the user pinned to the quickbar, in the order they pinned it.
///
/// Versioned from the first write rather than "when we need it": the pinned set is a
/// preference, and a preference that comes back wrong reads to its owner as data loss, not as
/// a migration that hasn't been written yet.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
enum StoredQuickbar {
    #[serde(rename = "1")]
    V1(Vec<Retirable<QuickBarItem>>),
}

/// Which surfaces were in their own OS window, and where those windows were.
///
/// Desktop-only in practice: [`crate::panel_popout::PoppedWindows`] is the only caller of either
/// fn and draws nothing off the desktop, so the web build never reads or writes this key. The
/// codec is not `cfg`-gated all the same — it is plain serde over plain data, and a `cfg` here
/// would buy nothing but a second way for the two targets' tests to diverge.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema_version", content = "data")]
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
enum StoredPopped {
    #[serde(rename = "1")]
    V1(Vec<Retirable<PoppedRecord>>),
}

/// One stored entry that may name a surface this build no longer has — `None` once it
/// does. Serde fails a whole sequence on the first element it can't read, so a layout
/// saved when `Identity` and `Combat` were grid surfaces would otherwise be discarded
/// *entirely* the moment they moved to the header, silently resetting every user's
/// arrangement of the eight panels they still have. Parsing element-by-element instead
/// drops only the retired entry; `reconcile_layout` then fills any surface the save
/// predates, and `validate_layout` still rejects genuinely corrupt state.
#[derive(Debug)]
struct Retirable<T>(Option<T>);

impl<'de, T: serde::de::DeserializeOwned> Deserialize<'de> for Retirable<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(Retirable(serde_json::from_value(value).ok()))
    }
}

impl<T: Serialize> Serialize for Retirable<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// Keep only the entries that named a surface this build still has.
fn live<T>(stored: Vec<Retirable<T>>) -> Vec<T> {
    stored.into_iter().filter_map(|entry| entry.0).collect()
}

/// Wrap live values for writing — every entry a real surface, so nothing is dropped on
/// the way out.
fn retirable<T>(values: &[T]) -> Vec<Retirable<T>>
where
    T: Clone,
{
    values.iter().cloned().map(|v| Retirable(Some(v))).collect()
}

/// Whether the desktop layout on screen is one the USER arranged, as opposed to one the app
/// authored from [`GridItem::default_layout_for`].
///
/// It gates the height re-fit ([`crate::grid::view::GridContainer`]): a default follows the
/// window as it resizes, and the moment the user arranges anything it is theirs and the app
/// stops moving it. Getting that backwards silently discards work — a re-fit re-authors the
/// whole layout, so re-fitting an arranged one would drop every hide and every drag.
///
/// A flag hung off [`persist_desktop`] rather than set at each gesture, because it has to be
/// impossible to forget. Every gesture in the app already funnels through that one fn (the
/// header's fold, the quickbar's pills, the visibility modal, every drag commit), so joining
/// the funnel is automatic and a new writer cannot miss a list it never had to join. The two
/// paths that author rather than arrange call [`mark_layout_authored`] explicitly, and Reset
/// calls it *after* its persist, being the one place that does both.
static USER_ARRANGED: AtomicBool = AtomicBool::new(false);

/// True once anything has persisted a layout — see [`USER_ARRANGED`].
pub fn layout_is_user_arranged() -> bool {
    USER_ARRANGED.load(Ordering::Relaxed)
}

/// Declare the layout on screen to be the app's own, so the window may keep re-fitting it.
pub fn mark_layout_authored() {
    USER_ARRANGED.store(false, Ordering::Relaxed);
}

/// Persist the desktop layout for the column count it was arranged at (fire-and-forget, like
/// `theme::apply_theme`). Written on every drag/resize commit and once more after a legacy
/// migration.
///
/// Also records that the layout is the user's ([`USER_ARRANGED`]), which is what stops the
/// window from re-authoring it on the next resize.
///
/// Read-modify-write inside the one `eval`, rather than an async read followed by a write:
/// this is called from drag commits that are not async, and the other counts' slots have to
/// survive a write to this one. The whole document is replaced when it is not already a V4 —
/// a V3 reaching here means the migration in [`load_desktop`] has already had its chance.
pub fn persist_desktop(items: &[GridItem], columns: u32) {
    USER_ARRANGED.store(true, Ordering::Relaxed);
    let Ok(json) = serde_json::to_string(&retirable(items)) else {
        return;
    };
    let slot = columns.to_string();
    let js = format!(
        "try {{\
            let doc = null;\
            try {{ doc = JSON.parse(localStorage.getItem({KEY_DESKTOP:?})); }} catch (_) {{}}\
            if (!doc || doc.schema_version !== '4' || typeof doc.data !== 'object') {{\
                doc = {{ schema_version: '4', data: {{}} }};\
            }}\
            doc.data[{slot:?}] = JSON.parse({json:?});\
            localStorage.setItem({KEY_DESKTOP:?},\
                JSON.stringify({{ schema_version: '4', data: doc.data }}));\
        }} catch (_) {{}}"
    );
    crate::layout_sync::commit_tracked(KEY_DESKTOP, js);
}

/// Persist which surfaces are out in their own windows (fire-and-forget).
///
/// A whole-document `setItem` rather than the read-modify-write [`persist_desktop`] needs,
/// because the caller always holds the complete set — there is no per-column-count slot here to
/// preserve, and nothing to merge with.
///
/// **Only the main window may call this, like every other persist**, and for this one the reason
/// is the feature itself: see [`crate::storage`] for what a write from a popped-out webview does
/// to the main window's store.
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
pub fn persist_popped(records: &[PoppedRecord]) {
    let stored = StoredPopped::V1(retirable(records));
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_POPPED:?}, {json:?}); }} catch (_) {{}}");
    crate::storage::commit(js);
}

/// The surfaces that were in their own windows, dropping any this build no longer has.
///
/// `None` and `Some(vec![])` are the same outcome here and the distinction is not worth drawing,
/// unlike [`load_quickbar`] where an empty save is the user having unpinned everything. Nobody
/// pops every panel back in to record an opinion; they just pop them back in.
///
/// No reconcile and no validate. There is nothing to complete — a surface missing from this list
/// is a surface on the grid — and nothing to be corrupt in the sense a layout can be: entries are
/// independent, so a bad one is dropped by [`Retirable`] and costs the others nothing. The caller
/// still filters against the live roster, because a `Dashboard` id parses perfectly whether or
/// not that dashboard still exists.
// Desktop-only. `panel_popout`'s `mod desktop` is `#[cfg(feature = "desktop")]` and is the
// only caller, so a web or default build reports this unused. NOT `cfg`-gated to match,
// deliberately: see the note at layout_store.rs:72 -- the codec is plain serde over plain
// data, and a second `cfg` here would only give the two targets another way to diverge.
// Annotated 2026-09-26 so `cargo check` is clean on every feature set, which is what a lint
// gate needs. This is live code; do not delete it.
#[allow(dead_code)]
pub async fn load_popped() -> Option<Vec<PoppedRecord>> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_POPPED:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?.to_string();

    let StoredPopped::V1(stored) = serde_json::from_str::<StoredPopped>(&text).ok()?;
    Some(live(stored))
}

/// Persist the mobile order (fire-and-forget).
pub fn persist_mobile(order: &MobileOrder) {
    let stored = StoredMobileOrder::V1(retirable(&order.0));
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_MOBILE:?}, {json:?}); }} catch (_) {{}}");
    crate::layout_sync::commit_tracked(KEY_MOBILE, js);
}

/// Persist the pinned quickbar set (fire-and-forget). Written on every pin, unpin and
/// reorder.
pub fn persist_quickbar(pins: &[QuickBarItem]) {
    let stored = StoredQuickbar::V1(retirable(pins));
    let Ok(json) = serde_json::to_string(&stored) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_QUICKBAR:?}, {json:?}); }} catch (_) {{}}");
    crate::layout_sync::commit_tracked(KEY_QUICKBAR, js);
}

/// The persisted pinned set if present and structurally valid; `None` keeps the default —
/// which is what a fresh install, a cleared browser and an unreadable value all get.
///
/// Element-by-element tolerant like the two loads above it: a pin naming a panel or a tool
/// this build no longer has is dropped by `live`, and the rest of the row survives. Without
/// that, retiring one tool would silently reset everyone's row to the default.
///
/// No reconcile step, unlike [`load_desktop`], and the absence is deliberate — see
/// [`validate_pins`] for why a pinned set has nothing to complete: a missing item is an
/// unpinned item, so appending one would pin something the user didn't ask for.
pub async fn load_quickbar() -> Option<Vec<QuickBarItem>> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_QUICKBAR:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?.to_string();

    let StoredQuickbar::V1(stored) = serde_json::from_str::<StoredQuickbar>(&text).ok()?;
    let pins = live(stored);
    validate_pins(&pins).then_some(pins)
}

/// The room the desktop grid has in the window as it is right now — both axes, because both
/// are inputs to the default now: the height decides the column rows
/// ([`column_rows_for`](crate::grid::model::column_rows_for)) and the width decides the column
/// count ([`columns_for`](crate::grid::model::columns_for)).
#[derive(Debug, Clone, Copy)]
pub struct GridSpace {
    /// The grid container's own width — what a column is a twelfth (or an eighth) of.
    pub width: f64,
    /// Everything under the grid's top edge, less the inset below it.
    pub height: f64,
}

/// The pixel height the desktop grid has to work with in the window as it is right now:
/// everything under the grid's top edge, less the inset below it — which is read off
/// `.grid-root`'s computed padding rather than restated here, so the number stays the CSS's.
///
/// `None` when there is nothing to measure — the element is absent, or the viewport is narrow
/// enough that the desktop grid is `display: none` and its rect is a zero. Both mean "keep the
/// authored default", never "fit to zero".
///
/// Read off the live DOM rather than computed from the chrome's heights, because the header
/// wraps, the quickbar's row is whatever the user pinned to it, and the fail-loud status strip
/// appears and disappears — every one of those moves the grid's top edge, and a layout sized
/// against an assumed chrome height would be wrong in exactly the cases that matter.
///
/// Waits for the element by frame rather than measuring whatever is there when the caller
/// happens to ask. An `eval` round-trip completing says nothing about the render's DOM
/// mutations having been applied — the caller's first attempt at this ran early enough that
/// `getElementById` returned null, which reads as "keep the authored default" and so failed
/// silently and completely. `FRAMES` of waiting is generous for a mount and still bounded,
/// and the null it eventually returns is the honest answer for the one case that never
/// resolves: a viewport narrow enough that the desktop grid is `display: none`, where the
/// element is real but has no width and there is no desktop layout on screen to size.
pub async fn measure_grid_space() -> Option<GridSpace> {
    const FRAMES: u32 = 60;
    let id = crate::grid::view::GRID_ELEMENT_ID;
    let js = format!(
        "try {{\
            for (let i = 0; i < {FRAMES}; i++) {{\
                const el = document.getElementById('{id}');\
                if (el) {{\
                    const r = el.getBoundingClientRect();\
                    if (r.width > 0) {{\
                        const pad = parseFloat(getComputedStyle(el.parentElement).paddingBottom);\
                        return [r.width, window.innerHeight - r.top - (pad || 0)];\
                    }}\
                }}\
                await new Promise((done) => requestAnimationFrame(done));\
            }}\
            return null;\
        }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let pair = value.as_array()?;
    let width = pair.first()?.as_f64()?;
    let height = pair.get(1)?.as_f64()?;
    (width > 0.0 && height > 0.0).then_some(GridSpace { width, height })
}

/// The persisted desktop layout if present and structurally valid; `None` keeps
/// the default. Tries the current V3 free-grid format first, then migrates a
/// stored V2 `FlowLayout` or a legacy unversioned `GridLayout` up to `GridItem`s.
///
/// Reconcile BEFORE validating: a layout saved before a surface was added is
/// missing it, and `validate_layout` requires every surface present, so validating
/// first would reject (and discard) an otherwise-good layout on every surface
/// addition. `reconcile_layout` appends the missing surface; `validate_layout` then
/// still rejects genuinely corrupt layouts (duplicates, off-grid, overlaps).
///
/// A layout that comes back from here is the user's, so it records that ([`USER_ARRANGED`])
/// and the window stops re-fitting it. Wrapped around the read rather than set at each of the
/// five places that return one, so a new return path joins automatically.
pub async fn load_desktop(columns: u32, expected: &[PanelKind]) -> Option<Vec<GridItem>> {
    let found = load_desktop_stored(columns, expected).await;
    if found.is_some() {
        USER_ARRANGED.store(true, Ordering::Relaxed);
    }
    found
}

/// Read a stored desktop document whichever order its two keys are in.
///
/// Serde reads an adjacently tagged enum with `data` ahead of `schema_version` by buffering
/// `data` first, and the buffer cannot read V4's `"12"` keys as the `u32`s they stand for, so
/// the whole layout fails to parse and the user gets the default. The account's copy comes back
/// in that order — Postgres `jsonb` sorts keys — and [`persist_desktop`] keeps whatever order it
/// finds. Found 2026-10-01, when a synced layout reset on every load. Reading the tag first,
/// here, works for both orders.
fn parse_desktop(text: &str) -> serde_json::Result<StoredDesktopLayout> {
    use serde::de::Error;
    let mut doc: serde_json::Value = serde_json::from_str(text)?;
    let data = doc
        .get_mut("data")
        .map(serde_json::Value::take)
        .ok_or_else(|| serde_json::Error::custom("no data"))?;
    match doc
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
    {
        Some("4") => serde_json::from_value(data).map(StoredDesktopLayout::V4),
        Some("3") => serde_json::from_value(data).map(StoredDesktopLayout::V3),
        Some("2") => serde_json::from_value(data).map(StoredDesktopLayout::V2),
        other => Err(serde_json::Error::custom(format!(
            "unknown schema_version {other:?}"
        ))),
    }
}

async fn load_desktop_stored(columns: u32, expected: &[PanelKind]) -> Option<Vec<GridItem>> {
    let config = GridConfig::for_columns(columns);
    let widest = GridConfig::default().columns;
    let js = format!(
        "try {{ return localStorage.getItem({KEY_DESKTOP:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?.to_string();

    match parse_desktop(&text) {
        Ok(StoredDesktopLayout::V4(slots)) => {
            let items = reconcile_layout(
                live(
                    slots
                        .into_iter()
                        .find_map(|(count, items)| (count == columns).then_some(items))?,
                ),
                expected,
            );
            if validate_layout(&items, &config, expected) {
                return Some(items);
            }
        }
        // Every pre-V4 format was written when twelve columns was the only count there was,
        // so it migrates into that slot and answers for no other. A user who has arranged a
        // twelve-column layout and opens at eight gets the authored eight-column default —
        // not a reflow of their arrangement — and their own layout is still there, untouched,
        // the moment the window is wide enough to hold it again.
        Ok(StoredDesktopLayout::V3(items)) => {
            let items = reconcile_layout(live(items), expected);
            if validate_layout(&items, &GridConfig::for_columns(widest), expected) {
                persist_desktop(&items, widest); // upgrade V3 → V4 so this runs once
                return (columns == widest).then_some(items);
            }
        }
        Ok(StoredDesktopLayout::V2(flow)) => {
            let items = reconcile_layout(flow_to_items(&flow), expected);
            if validate_layout(&items, &GridConfig::for_columns(widest), expected) {
                persist_desktop(&items, widest); // upgrade V2 → V4 so this runs once
                return (columns == widest).then_some(items);
            }
        }
        Err(_) => {}
    }

    // Oldest fallback: unversioned `GridLayout` rects.
    if let Ok(legacy) = serde_json::from_str::<GridLayout>(&text) {
        let items = reconcile_layout(grid_to_items(&legacy), expected);
        if validate_layout(&items, &GridConfig::for_columns(widest), expected) {
            persist_desktop(&items, widest);
            return (columns == widest).then_some(items);
        }
    }

    None
}

/// Build a free-grid layout from legacy rows — each an ordered `(panel, width)`
/// list. Neither legacy format carried a real height (a flow row was one CSS-Grid
/// row tall), so each surface takes its minimum usable height (`min_size`), which
/// for every current surface is also its default-layout height; the caller's
/// `reconcile_layout` compacts the rows together afterward.
fn rows_to_items(rows: &[Vec<(PanelKind, u32)>]) -> Vec<GridItem> {
    let mut items = Vec::new();
    let mut y = 0u32;
    for row in rows {
        let mut x = 0u32;
        let mut row_height = 0u32;
        for &(panel, width) in row {
            let height = panel.min_size().1;
            items.push(GridItem::new(panel, x, y, width, height));
            x += width;
            row_height = row_height.max(height);
        }
        y += row_height;
    }
    items
}

/// V2 migration: a `FlowLayout` is already row-packed, so its rows map straight
/// to [`rows_to_items`].
fn flow_to_items(flow: &FlowLayout) -> Vec<GridItem> {
    let rows: Vec<Vec<(PanelKind, u32)>> = flow
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|item| (item.panel, item.width as u32))
                .collect()
        })
        .collect();
    rows_to_items(&rows)
}

/// Legacy migration: group the 1-based rects into reading-order rows (cells that
/// share a `y`), then rebuild through [`rows_to_items`]. Column x-positions are
/// re-packed left-to-right, so the original 1-based x gaps don't need decoding;
/// `saturating` guards a corrupt coordinate, and `validate_layout` rejects the
/// result if the source was genuinely malformed.
fn grid_to_items(grid: &GridLayout) -> Vec<GridItem> {
    let mut cells = grid.cells.clone();
    cells.sort_by_key(|c| (c.y, c.x));

    let mut rows: Vec<Vec<(PanelKind, u32)>> = Vec::new();
    let mut current_y: Option<u8> = None;
    for cell in &cells {
        if current_y != Some(cell.y) {
            rows.push(Vec::new());
            current_y = Some(cell.y);
        }
        rows.last_mut()
            .unwrap()
            .push((cell.panel, cell.w.max(1) as u32));
    }
    rows_to_items(&rows)
}

/// Persist the Powers panel's card arrangement (fire-and-forget). Unversioned: the value
/// is one enum, and an unreadable one falls back to the default rather than migrating.
pub fn persist_powers_layout(layout: PowersLayout) {
    let Ok(json) = serde_json::to_string(&layout) else {
        return;
    };
    let js = format!("try {{ localStorage.setItem({KEY_POWERS:?}, {json:?}); }} catch (_) {{}}");
    crate::layout_sync::commit_tracked(KEY_POWERS, js);
}

/// The persisted Powers-panel arrangement; `None` (missing, or a variant this build no
/// longer has) keeps the default.
pub async fn load_powers_layout() -> Option<PowersLayout> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_POWERS:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    serde_json::from_str(value.as_str()?).ok()
}

/// The persisted mobile order if present and structurally valid; `None` keeps
/// the default.
pub async fn load_mobile(expected: &[PanelKind]) -> Option<MobileOrder> {
    let js = format!(
        "try {{ return localStorage.getItem({KEY_MOBILE:?}); }} catch (_) {{ return null; }}"
    );
    let value = document::eval(&js).await.ok()?;
    let text = value.as_str()?.to_string();

    if let Ok(StoredMobileOrder::V1(order)) = serde_json::from_str::<StoredMobileOrder>(&text) {
        // Reconcile before validating — an order saved before a panel was added is missing
        // it, and `validate` requires every panel exactly once (see `load_desktop`). A
        // retired panel has already been dropped by `live`.
        let order = MobileOrder(live(order)).reconcile(expected);
        if order.validate(expected) {
            return Some(order);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str = r#"{"panel":"Powers","x":0,"y":0,"w":2,"h":6,"min_width":2,"min_height":6,"max_width":null,"max_height":null,"pinned":false,"collapsed":false,"hidden":false}"#;

    /// The order the account's copy comes back in (`jsonb` sorts `data` first), which serde's
    /// own adjacently tagged read rejects on V4's numeric keys.
    #[test]
    fn a_desktop_layout_reads_with_its_keys_in_either_order() {
        let tag_first = format!(r#"{{"schema_version":"4","data":{{"12":[{ITEM}]}}}}"#);
        let data_first = format!(r#"{{"data":{{"12":[{ITEM}]}},"schema_version":"4"}}"#);
        assert!(serde_json::from_str::<StoredDesktopLayout>(&data_first).is_err());

        for text in [tag_first, data_first] {
            let Ok(StoredDesktopLayout::V4(slots)) = parse_desktop(&text) else {
                panic!("did not read as V4: {text}");
            };
            assert_eq!(
                live(slots[&12].iter().map(|i| Retirable(i.0)).collect()).len(),
                1
            );
        }
    }

    #[test]
    fn an_unknown_version_is_refused() {
        assert!(parse_desktop(r#"{"schema_version":"9","data":{}}"#).is_err());
    }
}
