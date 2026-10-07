//! Cell↔pixel geometry for the free grid. Pure functions over plain structs,
//! no Dioxus — this is unit-tested in milliseconds without a browser, which is
//! the whole point of keeping it separate from the view.

use super::model::{GridConfig, GridItem};

/// The pixel width of a single column at the current container width. The gaps
/// between columns are subtracted first, then the remainder is split evenly.
pub fn col_width(config: &GridConfig, container_width: f64) -> f64 {
    let gaps = config.margin.0 * (config.columns as f64 - 1.0);
    let usable = container_width - config.padding.0 * 2.0 - gaps;
    usable / config.columns as f64
}

/// An item's pixel rectangle `(x, y, width, height)`. A `w`-column item spans
/// `w` column widths **plus the `w - 1` gaps it swallows** — the gap count is
/// one less than the span, and getting it wrong drifts items out of alignment
/// as they widen. Rows come from [`GridItem::height`], not `h`, so a folded
/// surface is drawn at its header height rather than the height it will return to.
pub fn item_to_px(
    item: &GridItem,
    config: &GridConfig,
    container_width: f64,
) -> (f64, f64, f64, f64) {
    let cw = col_width(config, container_width);
    let rows = item.height() as f64;
    let x = config.padding.0 + item.x as f64 * (cw + config.margin.0);
    let y = config.padding.1 + item.y as f64 * (config.row_height + config.margin.1);
    let w = item.w as f64 * cw + (item.w as f64 - 1.0) * config.margin.0;
    let h = rows * config.row_height + (rows - 1.0) * config.margin.1;
    (x, y, w, h)
}

/// The cell span a pixel size snaps to — the inverse of [`item_to_px`]'s
/// dimensions. A `w`-wide item measures `w * cw + (w - 1) * margin` pixels, so
/// `w = (size + margin) / (cw + margin)`; `.round()` snaps at the half-cell like
/// [`px_to_cell`], and the floor of 1 keeps a resize from ever proposing a
/// zero-cell surface. This is the geometry FG4's resize-drag snaps against.
pub fn px_to_span(size: (f64, f64), config: &GridConfig, container_width: f64) -> (u32, u32) {
    let cw = col_width(config, container_width);
    let w = ((size.0 + config.margin.0) / (cw + config.margin.0))
        .round()
        .max(1.0) as u32;
    let h = ((size.1 + config.margin.1) / (config.row_height + config.margin.1))
        .round()
        .max(1.0) as u32;
    (w, h)
}

/// The rows a fitted surface's measured pixel height needs: the smallest span whose
/// [`item_to_px`] rectangle is at least `height_px` tall.
///
/// This is [`px_to_span`]'s height half rounded **up** rather than to the nearest cell,
/// and the difference is the whole point. A resize-drag snapping to the nearest cell is a
/// gesture reading the user's intent; a content fit landing a cell short is content clipped
/// away by `.panel`'s `overflow: hidden`, with no scrollbar left to recover it.
///
/// [`FIT_SLACK_PX`] of overshoot is forgiven before a row is charged for it: a measured box
/// is a real DOM height with sub-pixel fractions in it, and a hairline of a stat row's
/// bottom leading is not content worth a whole 28px row.
///
/// Floored at two rows. An expanded surface draws a header *and* a body, so one row — which
/// is exactly what a folded one occupies — can never be the right answer for one that isn't.
pub fn rows_for_px(height_px: f64, config: &GridConfig) -> u32 {
    let pitch = config.row_height + config.margin.1;
    let rows = ((height_px + config.margin.1 - FIT_SLACK_PX) / pitch).ceil();
    (rows.max(0.0) as u32).max(MIN_EXPANDED_ROWS)
}

/// Sub-pixel overshoot a content fit absorbs rather than paying a row for. See
/// [`rows_for_px`].
const FIT_SLACK_PX: f64 = 1.0;

/// The shortest an expanded surface can be fitted to — its header plus a row of body.
const MIN_EXPANDED_ROWS: u32 = 2;

/// The cell a pixel point snaps to. `.round()` (not `.floor()`) snaps to the
/// nearest cell, so the drag ghost moves once the pointer is half a cell across
/// rather than a full cell — that half-cell threshold is what makes dragging
/// feel responsive.
pub fn px_to_cell(px: (f64, f64), config: &GridConfig, container_width: f64) -> (u32, u32) {
    let cw = col_width(config, container_width);
    let x = ((px.0 - config.padding.0) / (cw + config.margin.0))
        .round()
        .max(0.0) as u32;
    let y = ((px.1 - config.padding.1) / (config.row_height + config.margin.1))
        .round()
        .max(0.0) as u32;
    (x, y)
}

/// The container's derived pixel height. Absolute-positioned children don't
/// stretch their parent, so the grid's height is computed from the lowest item
/// edge rather than set by content.
///
/// Hidden surfaces are not drawn, so they set no edge: a stored rectangle far down the
/// grid would otherwise hold the container open over empty space no panel occupies.
pub fn container_height(items: &[GridItem], config: &GridConfig) -> f64 {
    let rows = items
        .iter()
        .filter(|i| !i.hidden)
        .map(|i| i.y + i.height())
        .max()
        .unwrap_or(0) as f64;
    if rows == 0.0 {
        return config.padding.1 * 2.0;
    }
    rows * config.row_height + (rows - 1.0) * config.margin.1 + config.padding.1 * 2.0
}

/// The tallest row extent whose [`container_height`] still fits `available_px` — the
/// inverse of that fn, and the one place the grid is told how much room it actually has.
///
/// Everything else here derives pixels from cells; this goes the other way, because the
/// default layout's column height is the one number that has to be answered by the window
/// rather than by the content or by the author. Floors at zero: a viewport too short for a
/// single row is a real answer, and the caller clamps it to something usable.
pub fn rows_that_fit(available_px: f64, config: &GridConfig) -> u32 {
    let pitch = config.row_height + config.margin.1;
    let usable = available_px - config.padding.1 * 2.0 + config.margin.1;
    (usable / pitch).floor().max(0.0) as u32
}
