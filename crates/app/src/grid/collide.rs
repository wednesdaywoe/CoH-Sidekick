//! Collision, compaction, push-on-collide, and resize — the free grid's real
//! logic, and the part most likely to be subtly wrong. Pure functions over
//! `Vec<GridItem>`, no Dioxus, so the bugs COULD be found by `#[test]` rather
//! than by dragging panels around with a mouse.
//!
//! ALMOST NOTHING TESTS THEM, and the count is given rather than the adjective.
//! Measured 2026-09-26 this crate held zero `#[test]` and zero
//! `cfg(test)` across 100 files and 48,482 lines, and the only match for
//! `#[test]` in it was the sentence above, which claimed the coverage in the
//! present tense. Item 31 wrote the first 14 the next day. Exactly ONE of them
//! reaches this file: `default_layout_is_already_compact` in `super::model`,
//! which pins [`compact`] as a no-op over every authored default. It also
//! exercises [`validate_layout`] over the same defaults. Three more, added
//! 2026-10-01 in this file's own `tests`, cover [`reconcile_layout`] seating
//! new dashboards, which also runs the push-on-collide pass.
//!
//! So [`move_item`], [`resize_item`], [`place_item`], [`clamp_resize`],
//! [`fit_item`], [`toggle_collapse`] and [`set_hidden`] are still found wrong by dragging panels
//! around with a mouse. That is a statement of what is missing, not a plan: the
//! gap is named so the next person picks it up deliberately.

use std::collections::HashSet;

use super::model::{GridConfig, GridItem, PanelKind, BAND_SEED_ROWS};

/// Standard AABB overlap, negated. Sharing an edge (`<=`) counts as touching,
/// not colliding — two items flush against each other are a valid layout.
///
/// Vertical extent is [`GridItem::height`], not `h`: a folded surface occupies its header
/// row alone, so the rows its collapsed body would have covered are free for a neighbour.
///
/// A hidden surface occupies nothing at all. Reporting it here is the one place the whole
/// inert contract has to hold: push, compaction, and `validate_layout`'s overlap check are
/// all written in terms of this fn, so a hidden surface that collided would block a cell
/// nothing is drawing in — and fail validation the moment a neighbour floated into it.
pub fn collides(a: &GridItem, b: &GridItem) -> bool {
    if a.panel == b.panel || a.hidden || b.hidden {
        return false;
    }
    !(a.x + a.w <= b.x || a.x >= b.x + b.w || a.y + a.height() <= b.y || a.y >= b.y + b.height())
}

// Ahead of its consumer — no gesture queries "what would this land on?" yet, but
// the helper belongs with `collides`. Drops the allow when a caller appears.
#[allow(dead_code)]
pub fn first_collision<'a>(items: &'a [GridItem], target: &GridItem) -> Option<&'a GridItem> {
    items.iter().find(|i| collides(i, target))
}

/// Float every item up until it rests on another item or the top edge — the
/// grid's "gravity". Ordering is the whole algorithm: an item can only float
/// past items already placed, so upper-left items must be resolved first.
///
/// A hidden surface is skipped rather than floated: its rectangle is the cell it will
/// come back to, and gravity would drag every hidden surface to the top of the grid,
/// so showing one again would land it somewhere it has never been.
///
/// Runs on commit, never per frame (it's `O(n²)` worst case).
pub fn compact(items: &mut [GridItem]) {
    items.sort_by_key(|i| (i.y, i.x));

    let mut placed: Vec<GridItem> = Vec::with_capacity(items.len());
    for item in items.iter_mut() {
        if item.hidden {
            continue;
        }
        if !item.pinned {
            while item.y > 0 {
                let mut probe = *item;
                probe.y -= 1;
                if placed.iter().any(|p| collides(p, &probe)) {
                    break;
                }
                item.y -= 1;
            }
        }
        placed.push(*item);
    }
}

/// Move `panel` to `(new_x, new_y)`, pushing anything it lands on downward, then
/// compacting. `new_x` is clamped so the item can't cross the right edge; that
/// invariant is enforced here, not checked afterward.
///
/// Resolve *then* compact, always in that order: resolve pushes items down to
/// eliminate overlap, compact floats everything back up to close the gaps.
/// Compacting an still-overlapping layout produces nonsense.
pub fn move_item(
    items: &mut [GridItem],
    panel: PanelKind,
    new_x: u32,
    new_y: u32,
    config: &GridConfig,
) {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return;
    };
    if items[idx].pinned {
        return;
    }

    items[idx].x = new_x.min(config.columns.saturating_sub(items[idx].w));
    items[idx].y = new_y;

    let mut moved = HashSet::new();
    moved.insert(panel);
    resolve_collisions(items, panel, &mut moved);
    compact(items);
}

/// Resize `panel` to `(new_w, new_h)` cells, clamped to the surface's minimums,
/// its maximums, and the right edge; then push and compact exactly as a move
/// does. This is the pure core FG4's resize-drag drives.
pub fn resize_item(
    items: &mut [GridItem],
    panel: PanelKind,
    new_w: u32,
    new_h: u32,
    config: &GridConfig,
) {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return;
    };
    if items[idx].pinned {
        return;
    }

    let (w, h) = clamp_resize(&items[idx], new_w, new_h, config);
    items[idx].w = w;
    items[idx].h = h;

    let mut moved = HashSet::new();
    moved.insert(panel);
    resolve_collisions(items, panel, &mut moved);
    compact(items);
}

/// Fold or unfold `panel`, then push and compact exactly as a move or resize does.
///
/// Collapsing IS a size change — a folded surface occupies one row instead of `h` — so the
/// layout has to reflow around it. Without the compaction, folding a panel would leave a
/// hole exactly the size of the body it just hid, which is the opposite of what folding is
/// for; without the push, unfolding one would land it on top of its neighbour.
///
/// Unlike [`resize_item`] this consults no minimum: `min_height` is the smallest size at
/// which a surface's *content* stays usable, and a folded surface is drawing none of it.
/// `h` is left alone, so unfolding returns the surface to the size it was folded at.
pub fn toggle_collapse(items: &mut [GridItem], panel: PanelKind) {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return;
    };
    items[idx].collapsed = !items[idx].collapsed;

    let mut moved = HashSet::new();
    moved.insert(panel);
    resolve_collisions(items, panel, &mut moved);
    compact(items);
}

/// Take `panel` off the grid or put it back, then push and compact exactly as a fold does.
///
/// Both directions reflow for the same reason folding does — the surface stops or starts
/// occupying cells — and both run the same two steps, because which one does the work
/// depends on the direction: hiding has no victims (a hidden surface collides with nothing),
/// so the compaction closes the hole; showing lands the surface back on its remembered
/// rectangle, where the push clears whatever floated into it while it was gone.
pub fn set_hidden(items: &mut [GridItem], panel: PanelKind, hidden: bool) {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return;
    };
    items[idx].hidden = hidden;

    let mut moved = HashSet::new();
    moved.insert(panel);
    resolve_collisions(items, panel, &mut moved);
    compact(items);
}

/// Set `panel`'s full rectangle — position *and* size together — then push and
/// compact exactly as a move or resize does. This is the keyboard-move commit
/// (FG6): a keyboard session can both relocate and resize a surface before placing
/// it, so neither [`move_item`] (size fixed) nor [`resize_item`] (origin fixed)
/// alone expresses it. `x` is clamped inside the grid and `(w, h)` through
/// [`clamp_resize`] against that clamped `x`, so `x + w <= columns` holds without a
/// post-check.
pub fn place_item(
    items: &mut [GridItem],
    panel: PanelKind,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    config: &GridConfig,
) {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return;
    };
    if items[idx].pinned {
        return;
    }

    items[idx].x = x.min(config.columns.saturating_sub(1));
    items[idx].y = y;
    let (w, h) = clamp_resize(&items[idx], w, h, config);
    items[idx].w = w;
    items[idx].h = h;

    let mut moved = HashSet::new();
    moved.insert(panel);
    resolve_collisions(items, panel, &mut moved);
    compact(items);
}

/// The `(w, h)` a resize request for `item` clamps to — its own minimums and
/// maximums, and the room left to the right edge (the origin stays put on a
/// south-east resize, so width is capped rather than `x` shifted). Shared by the
/// commit ([`resize_item`]) and FG4's live preview so the placeholder shows
/// exactly where the surface will land, before any collision handling.
pub fn clamp_resize(item: &GridItem, new_w: u32, new_h: u32, config: &GridConfig) -> (u32, u32) {
    let max_width = item.max_width.unwrap_or(config.columns);
    let max_height = item.max_height.unwrap_or(u32::MAX);
    let w = clamp_dim(new_w, item.min_width, max_width).min(config.columns - item.x);
    // A surface that fits its content has no height for a gesture to propose: whatever the
    // drag asks for, the next measurement overrules it, so honouring the ask would show a
    // surface springing back the instant the pointer left it. Held here rather than at each
    // gesture, so the pointer resize, its placeholder and the keyboard's Shift+arrows all
    // get the same answer and cannot drift apart.
    let h = if item.panel.fits_content() {
        item.h
    } else {
        clamp_dim(new_h, item.min_height, max_height)
    };
    (w, h)
}

/// Set `panel`'s height to a measured content fit of `rows`, then push and compact exactly
/// as a resize does. Returns whether the layout actually changed — a measurement that
/// agrees with the current height is the common case and must not churn the signal it
/// would otherwise be written back into.
///
/// Unlike [`resize_item`] this consults no minimum and no maximum. `min_height` is the floor
/// a *drag* may not cross, and a fit is not a drag: the content is the authority on how tall
/// the surface has to be, and clamping its answer upward would leave the surface holding
/// space it isn't drawing into, while clamping downward would reintroduce the scrollbar the
/// fit exists to remove. `rows` is trusted, and [`rows_for_px`](super::math::rows_for_px)
/// owns the floor.
pub fn fit_item(items: &mut [GridItem], panel: PanelKind, rows: u32) -> bool {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return false;
    };
    if items[idx].pinned || items[idx].h == rows {
        return false;
    }

    items[idx].h = rows;

    let mut moved = HashSet::new();
    moved.insert(panel);
    resolve_collisions(items, panel, &mut moved);
    compact(items);
    true
}

/// Whether a deserialized layout is structurally sound: every surface present
/// exactly once, every item inside the column count with a positive span, and no
/// two items overlapping. Corrupt or stale persisted state fails this and the
/// caller falls back to the default. Run [`reconcile_layout`] first — it completes
/// a layout that merely predates a surface, which would otherwise fail the count.
///
/// `expected` is the roster the layout is checked against, handed in rather than read off a
/// const, because the roster is no longer constant: a dashboard panel is something the user
/// builds, so which surfaces SHOULD be present is a question only the caller can answer. The
/// grid is the wrong place to know it, and `PanelKind::ALL` was the shape of knowing it.
///
/// A hidden surface is still *present* — that is what keeps hiding one from reading as
/// a corrupt save — and its rectangle is still checked for a positive in-grid span,
/// because that rectangle is where showing it again puts it. Only the overlap check
/// passes over it, through [`collides`].
pub fn validate_layout(items: &[GridItem], config: &GridConfig, expected: &[PanelKind]) -> bool {
    if items.len() != expected.len() {
        return false;
    }
    for panel in expected.iter().copied() {
        if items.iter().filter(|i| i.panel == panel).count() != 1 {
            return false;
        }
    }
    for item in items {
        if item.w == 0 || item.h == 0 || item.x + item.w > config.columns {
            return false;
        }
    }
    for i in 0..items.len() {
        for j in (i + 1)..items.len() {
            if collides(&items[i], &items[j]) {
                return false;
            }
        }
    }
    true
}

/// Make a persisted layout agree with the roster it is being loaded against: drop any surface
/// the roster no longer has, append any it has gained below the existing content at its default
/// size, then compact the whole layout.
///
/// Mirrors [`crate::mobile_order::MobileOrder::reconcile`]: `validate` requires every surface
/// present and no others, so a layout saved before a surface existed would be rejected without
/// this. Compaction is idempotent on an already-committed layout (every move/resize ends
/// compacted), so this never disturbs a full one.
///
/// **The drop half is new, and it is what makes two stores safe to keep apart.** The rectangles
/// live in `sk-layout` and the dashboard roster in `sk-stats-config`, written by different paths
/// at different moments, so they will disagree — a panel made in one tab, a layout saved in
/// another. The roster is authoritative and this is where that is enforced. While every surface
/// was a variant of a closed enum there was nothing to enforce: a stored item naming a surface
/// the build did not have could not deserialize in the first place, so an unexpected item was
/// not a state this function could be handed.
///
/// A dropped surface takes its rectangle with it, and a re-created panel gets a fresh id
/// precisely so it cannot inherit one (see [`DashboardId`](crate::grid::model::DashboardId)).
pub fn reconcile_layout(mut items: Vec<GridItem>, expected: &[PanelKind]) -> Vec<GridItem> {
    // Dashboards leaving on screen, kept for the space they free up. A roster that drops some
    // dashboards and gains others in one change is a replacement ("Panels by category" is
    // one), and the new panels belong where the old ones were, not under everything else.
    let vacated: Vec<GridItem> = items
        .iter()
        .filter(|i| !expected.contains(&i.panel) && i.panel.dashboard().is_some() && !i.hidden)
        .copied()
        .collect();
    items.retain(|item| expected.contains(&item.panel));

    // Limits are derived from the panel kind, never authored by the user, so a
    // retuned min_size/max_size must win over the value frozen into the save.
    for item in &mut items {
        item.refresh_limits();
    }

    // Below the visible content, not below every stored rectangle: a hidden surface keeps
    // the cell it was hidden from, which sits under whatever floated up over it, so
    // measuring against it would append the new surface into a band of empty grid.
    let mut next_y = items
        .iter()
        .filter(|i| !i.hidden)
        .map(|i| i.y + i.height())
        .max()
        .unwrap_or(0);

    let arrived: Vec<PanelKind> = expected
        .iter()
        .copied()
        .filter(|panel| panel.dashboard().is_some() && !items.iter().any(|i| i.panel == *panel))
        .collect();
    if !vacated.is_empty() && !arrived.is_empty() {
        seat_in_vacated(&mut items, &arrived, &vacated);
    }

    for panel in expected.iter().copied() {
        if items.iter().any(|i| i.panel == panel) {
            continue;
        }
        let (width, height) = (panel.default_width() as u32, panel.min_size().1);
        items.push(GridItem::new(panel, 0, next_y, width, height));
        next_y += height;
    }
    compact(&mut items);
    items
}

/// Seat `arrived` in the span the `vacated` dashboards covered: rows across that span's width,
/// the width shared as evenly as whole cells allow, as many to a row as keeps each at its
/// two-cell floor. Rows past the first push whatever sits under the span down to make room.
///
/// The span is the vacated rectangles' horizontal extent, starting at the highest of them. A
/// band across the top stays a band; a panel the user kept in a narrow side column is replaced
/// by a stack in that column.
fn seat_in_vacated(items: &mut Vec<GridItem>, arrived: &[PanelKind], vacated: &[GridItem]) {
    const MIN_SEAT: u32 = 2;
    let left = vacated.iter().map(|i| i.x).min().unwrap_or(0);
    let right = vacated.iter().map(|i| i.x + i.w).max().unwrap_or(MIN_SEAT);
    let top = vacated.iter().map(|i| i.y).min().unwrap_or(0);
    let width = right - left;
    let per_row = (width / MIN_SEAT).max(1) as usize;

    let mut seated = HashSet::new();
    for (row, chunk) in arrived.chunks(per_row).enumerate() {
        let seats = chunk.len() as u32;
        let share = width / seats;
        let remainder = width % seats;
        let y = top + row as u32 * BAND_SEED_ROWS;
        let mut x = left;
        for (index, &panel) in chunk.iter().enumerate() {
            let w = share + u32::from((index as u32) < remainder);
            items.push(GridItem::new(panel, x, y, w, BAND_SEED_ROWS));
            seated.insert(panel);
            x += w;
        }
    }

    // Every new panel is an obstacle from the start, so pushing clears all of them and none
    // pushes a sibling.
    let mut moved = seated.clone();
    for panel in arrived {
        resolve_collisions(items, *panel, &mut moved);
    }
}

/// Push every item overlapping `panel` below it, then cascade into whatever
/// *those* items now overlap. `moved` is the safety rail: each item is displaced
/// at most once per pass, which is what stops two items pushing each other in a
/// cycle that never terminates. Pinned items are obstacles, never victims.
///
/// Victims land below the source *and* below anything else this pass has already displaced.
/// One source can overlap a whole stacked column at once — grow a stat panel down over its
/// three neighbours, or unfold one that was folded above them — and sending each of those to
/// the same `source.y + height` drops them on each other. The rail cannot sort that out
/// afterward, because reaching this point has already spent every one of their single
/// displacements; they have to clear each other as they are placed.
///
/// `moved` is what makes that shared: an item in it has taken its one displacement and will
/// not move again, so the set doubles as the list of fixed obstacles a later victim must
/// clear — including ones placed by a sibling branch of the recursion, which a floor kept
/// per call would miss.
fn resolve_collisions(items: &mut [GridItem], panel: PanelKind, moved: &mut HashSet<PanelKind>) {
    let Some(idx) = items.iter().position(|i| i.panel == panel) else {
        return;
    };
    let source = items[idx];

    let mut victims: Vec<PanelKind> = items
        .iter()
        .filter(|i| collides(i, &source) && !moved.contains(&i.panel) && !i.pinned)
        .map(|i| i.panel)
        .collect();
    // Reading order, so a column's own top-to-bottom sequence survives the push.
    victims.sort_by_key(|&v| {
        items
            .iter()
            .find(|i| i.panel == v)
            .map_or((0, 0), |i| (i.y, i.x))
    });

    // Place all of this source's victims before descending into any of them. Depth-first
    // would settle a victim's own cascade before the sibling below it in the same column
    // was placed at all, which lands that sibling underneath its descendants — a column
    // that reshuffles itself when one surface is dragged taller.
    for &victim in &victims {
        moved.insert(victim);
        let Some(vi) = items.iter().position(|i| i.panel == victim) else {
            continue;
        };
        items[vi].y = source.y + source.height();
        // Settles rather than spins: every obstacle here is stationary, and each pass moves
        // `y` strictly down past the lowest of them still in the way.
        while let Some(clear_of) = lowest_displaced_edge(items, vi, moved) {
            items[vi].y = clear_of;
        }
    }

    for victim in victims {
        resolve_collisions(items, victim, moved);
    }
}

/// The bottom edge of the lowest already-displaced item still overlapping `items[vi]`, or
/// `None` once it overlaps none of them.
fn lowest_displaced_edge(items: &[GridItem], vi: usize, moved: &HashSet<PanelKind>) -> Option<u32> {
    let probe = items[vi];
    items
        .iter()
        .filter(|i| i.panel != probe.panel && moved.contains(&i.panel) && collides(i, &probe))
        .map(|i| i.y + i.height())
        .max()
}

/// Clamp an integer cell dimension to `[min, max]`. `min` wins if the range is
/// degenerate, so a below-minimum request never produces a zero-sized item.
fn clamp_dim(value: u32, min: u32, max: u32) -> u32 {
    value.clamp(min, max.max(min))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::model::DashboardId;

    fn dashboards(ids: std::ops::RangeInclusive<u32>) -> Vec<PanelKind> {
        ids.map(|id| PanelKind::Dashboard(DashboardId(id)))
            .collect()
    }

    fn roster(dash: &[PanelKind]) -> Vec<PanelKind> {
        let mut roster = vec![PanelKind::Powers, PanelKind::Available, PanelKind::Pools];
        roster.extend_from_slice(dash);
        roster.extend([PanelKind::SetBonuses, PanelKind::Info]);
        roster
    }

    /// A default layout seated for one dashboard — the state these reconcile cases start from,
    /// built explicitly now that a fresh install seats four.
    fn one_dashboard_layout() -> Vec<GridItem> {
        GridItem::default_layout_sized(
            crate::grid::model::DEFAULT_COLUMN_ROWS,
            &roster(&dashboards(1..=1)),
        )
    }

    fn item(items: &[GridItem], panel: PanelKind) -> GridItem {
        *items.iter().find(|i| i.panel == panel).unwrap()
    }

    #[test]
    fn a_replacement_takes_the_band_the_old_dashboard_held() {
        let before = one_dashboard_layout();
        let old = item(&before, PanelKind::Dashboard(DashboardId(1)));
        let fresh = dashboards(2..=5);

        let after = reconcile_layout(before.clone(), &roster(&fresh));

        assert!(validate_layout(
            &after,
            &GridConfig::default(),
            &roster(&fresh)
        ));
        let seated: Vec<_> = fresh.iter().map(|p| item(&after, *p)).collect();
        assert!(seated.iter().all(|i| i.y == old.y));
        assert_eq!(seated[0].x, old.x);
        assert_eq!(seated.iter().map(|i| i.w).sum::<u32>(), old.w);
        assert_eq!(
            item(&after, PanelKind::Powers).y,
            item(&before, PanelKind::Powers).y
        );
    }

    #[test]
    fn a_replacement_too_wide_for_one_row_wraps_and_pushes_the_columns_down() {
        let before = one_dashboard_layout();
        let fresh = dashboards(2..=9);

        let after = reconcile_layout(before.clone(), &roster(&fresh));

        assert!(validate_layout(
            &after,
            &GridConfig::default(),
            &roster(&fresh)
        ));
        let lowest = fresh
            .iter()
            .map(|p| item(&after, *p))
            .map(|i| i.y + i.height())
            .max()
            .unwrap();
        assert!(lowest > item(&before, PanelKind::Powers).y);
        assert_eq!(item(&after, PanelKind::Powers).y, lowest);
    }

    #[test]
    fn a_panel_added_without_one_removed_still_goes_below() {
        let before = one_dashboard_layout();
        let mut both = dashboards(1..=1);
        both.push(PanelKind::Dashboard(DashboardId(2)));

        let after = reconcile_layout(before.clone(), &roster(&both));

        let added = item(&after, PanelKind::Dashboard(DashboardId(2)));
        assert!(added.y > item(&before, PanelKind::Dashboard(DashboardId(1))).y);
    }
}
