//! The planner grid.
//!
//! The **free grid** ([`model`]/[`math`]/[`collide`]/[`view`]) is the live model:
//! absolute `{x,y,w,h}` cells with collision-push and compaction, rendered and
//! dragged by `view` through the pure engine, and persisted as `GridItem`s by
//! [`crate::layout_store`].
//!
//! The legacy **`FlowLayout`** (rows of panels with widths) and the older
//! **`GridLayout`** (1-based CSS-Grid rects) no longer render, drag, or persist
//! anything. They survive here as plain data — deserialize-only targets so
//! `layout_store` can migrate a layout saved in either older format up to the
//! current `GridItem` store. Their behavior lives in `layout_store`'s migration,
//! not on the structs.
//!
//! Mobile is an independent `MobileOrder` (flat list), untouched by the rewrite:
//! its reorder menu and the shared `move_to_index` primitive stay here.

use serde::{Deserialize, Serialize};

pub mod collide;
pub mod math;
pub mod model;
pub mod view;

// `PanelKind` keeps its short `crate::grid::PanelKind` path (imported across the
// shell, mobile, and the grid `view`). `GridItem`/`GridConfig` are reached through
// their `grid::model` path.
pub use model::PanelKind;

/// Generic reorder primitive: move an item to a target index, clamping to list bounds.
pub fn move_to_index<T: Copy + PartialEq>(items: &[T], item: T, target_index: usize) -> Vec<T> {
    let mut result = items.to_vec();
    if let Some(current_pos) = result.iter().position(|&x| x == item) {
        let val = result.remove(current_pos);
        let clamped = target_index.min(result.len());
        result.insert(clamped, val);
    }
    result
}

/// One panel in the legacy row-packed layout: a surface plus its column width.
/// Deserialize-only — `layout_store` reads a stored `FlowLayout` and migrates it
/// to `GridItem`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowItem {
    pub panel: PanelKind,
    pub width: u8,
}

/// The legacy row-packed layout: panels wrapped into rows under a 12-column cap,
/// height always one row. Deserialize-only (see [`FlowItem`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowLayout {
    pub rows: Vec<Vec<FlowItem>>,
}

/// One cell in the oldest layout format: a 1-based CSS-Grid rectangle.
/// Deserialize-only migration target (see [`GridLayout`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GridCell {
    pub panel: PanelKind,
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
}

/// The oldest persisted layout: a flat list of 1-based rects. Deserialize-only —
/// `layout_store` migrates a stored `GridLayout` to `GridItem`s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridLayout {
    pub cells: Vec<GridCell>,
}
