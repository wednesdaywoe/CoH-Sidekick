//! Mobile panel ordering — independent from desktop's 2D layout.
//! A flat list with ▲/▼ affordance and drag reorder, persisted separately.

use crate::grid::{move_to_index, PanelKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobileOrder(pub Vec<PanelKind>);

impl MobileOrder {
    /// The stack in roster order — which is `surfaces`' order, and deliberately so: the
    /// mobile stack has no cells to arrange, so the only thing it can inherit from the desktop
    /// is the sequence, and reading the same list keeps the two from disagreeing about it.
    pub fn default_order(expected: &[PanelKind]) -> Self {
        MobileOrder(expected.to_vec())
    }

    /// Every surface in the roster exactly once, and nothing that is not in it.
    ///
    /// `expected` is handed in for the reason
    /// [`validate_layout`](crate::grid::collide::validate_layout) takes it: a dashboard panel is
    /// user-built, so the roster is not a const any more.
    pub fn validate(&self, expected: &[PanelKind]) -> bool {
        if self.0.len() != expected.len() {
            return false;
        }
        for panel in expected.iter().copied() {
            if self.0.iter().filter(|&&p| p == panel).count() != 1 {
                return false;
            }
        }
        true
    }

    pub fn missing_panels(&self, expected: &[PanelKind]) -> Vec<PanelKind> {
        expected
            .iter()
            .filter(|p| !self.0.contains(p))
            .copied()
            .collect()
    }

    /// Drop what the roster no longer has, then append what it has gained.
    ///
    /// **The prune is not optional, and leaving it out fails loudly in the wrong place.** This
    /// used to only append, which was complete while every surface was a variant of a closed
    /// enum — an entry naming a surface the build lacked could not deserialize at all. A
    /// dashboard id is data, so it deserializes fine and outlives the panel it names. One such
    /// entry would survive reconcile, [`Self::validate`] would then fail on the length, and
    /// `load_mobile` would answer `None` — silently replacing the user's whole hand-built order
    /// because of a single stale line in it.
    ///
    /// When the same change drops some panels and gains others, the gained ones take the place
    /// of the first dropped one rather than going to the end: that is a replacement, and the
    /// user put what was replaced where they wanted it.
    pub fn reconcile(mut self, expected: &[PanelKind]) -> MobileOrder {
        let first_dropped = self.0.iter().position(|panel| !expected.contains(panel));
        let at = first_dropped.map(|index| {
            self.0[..index]
                .iter()
                .filter(|panel| expected.contains(panel))
                .count()
        });
        self.0.retain(|panel| expected.contains(panel));
        let missing = self.missing_panels(expected);
        match at {
            Some(at) => {
                self.0.splice(at..at, missing);
            }
            None => self.0.extend(missing),
        }
        self
    }

    pub fn move_to_index(&self, panel: PanelKind, target_index: usize) -> MobileOrder {
        let reordered = move_to_index(&self.0, panel, target_index);
        MobileOrder(reordered)
    }

    pub fn move_by(&self, panel: PanelKind, delta: i32) -> MobileOrder {
        if let Some(current_pos) = self.0.iter().position(|&p| p == panel) {
            let target = if delta > 0 {
                (current_pos as i32 + delta)
                    .min(self.0.len() as i32 - 1)
                    .max(0) as usize
            } else {
                (current_pos as i32 + delta).max(0) as usize
            };
            self.move_to_index(panel, target)
        } else {
            self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::model::DashboardId;

    #[test]
    fn a_replacement_lands_where_the_replaced_panel_was() {
        let dash = |id| PanelKind::Dashboard(DashboardId(id));
        let order = MobileOrder(vec![PanelKind::Powers, dash(1), PanelKind::Info]);
        let expected = [PanelKind::Powers, dash(2), dash(3), PanelKind::Info];

        let after = order.reconcile(&expected);

        assert_eq!(after.0, expected);
    }
}
