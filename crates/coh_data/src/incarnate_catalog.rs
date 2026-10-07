//! Incarnate catalog — the typed view of the contract's `incarnate-catalog`
//! section: the per-slot pick lists the incarnate picker renders (slot display
//! name/icon + each power's identity/display/help), emitted from each dataset's
//! OWN binary-export slot indices (never the beta's vendored HC-shaped copies in
//! `src/data/incarnate-indices/`, which are shared across datasets).
//!
//! Structure (tree / tier / branch) is DERIVED from the powers' display names,
//! not shipped: the game authors the ladder into the name itself — `<Tree>
//! [Total|Partial] [Core|Radial] <suffix>` with a tier-4 marker word (Paragon /
//! Final / Flawless / Superior / Epiphany / Embodiment) — and the derivation is
//! corpus-pinned: every slot of every
//! dataset groups into trees of exactly 9 powers, each with exactly one Common
//! root. The beta derived the same way (`inferTierFromPowerName` /
//! `inferBranchFromPowerName` over `displayName`) but needed a separate Genesis
//! ladder and a hand prefix map for lore trees; deriving the tree from the
//! display tokens BEFORE the first structural keyword needs neither ("Banished
//! Pantheon Ally" and "Melee Genome" both group without a table), and the
//! corpus carries no "Genesis"-named power outside the genesis slot, so the
//! beta's genesis-as-tier-4-keyword special case is vestigial and not ported.
//!
//! Which slots are OFFERED is not decided here: the catalog mirrors the export,
//! and all three forks ship a genesis powerset — dormant off-Rebirth (GENESIS-1,
//! empty effect tables). [`crate::database::PowerDatabase::offered_incarnate_slots`]
//! gates on the matching effects table being non-empty.

use serde::Deserialize;
use serde_json::Value;

/// Rarity tier of an incarnate power, in craft order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncarnateTier {
    Common,
    Uncommon,
    Rare,
    VeryRare,
}

impl IncarnateTier {
    pub fn label(self) -> &'static str {
        match self {
            IncarnateTier::Common => "Common",
            IncarnateTier::Uncommon => "Uncommon",
            IncarnateTier::Rare => "Rare",
            IncarnateTier::VeryRare => "Very Rare",
        }
    }

    /// The tier token in the synthesized icon filename
    /// (`incarnate_<slot>_<tree>_<tier>.png`).
    pub fn icon_token(self) -> &'static str {
        match self {
            IncarnateTier::Common => "common",
            IncarnateTier::Uncommon => "uncommon",
            IncarnateTier::Rare => "rare",
            IncarnateTier::VeryRare => "veryrare",
        }
    }
}

/// Core / Radial branch of an incarnate power; the Common root is `Base`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncarnateBranch {
    Base,
    Core,
    Radial,
}

/// How deep into its branch a Rare-tier power sits. The game authors exactly two Rares
/// per branch and says which is which in the name; BOTH are crafted from that branch's
/// own Uncommon (each Rare recipe's `PowerComponent` in `baserecipes.bin` names it —
/// see [`crate::incarnate_crafting`]). Total is drawn as the outer rung and Partial the
/// inner one, which is the order the craft ladder renders in.
///
/// `None` on every other tier: only Rare splits this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RareDepth {
    Total,
    Partial,
}

/// Words that mark tier/branch structure in a display name. Everything BEFORE
/// the first of these is the tree name.
const TIER4_WORDS: [&str; 6] = [
    "paragon",
    "final",
    "flawless",
    "superior",
    "epiphany",
    "embodiment",
];
const TIER3_WORDS: [&str; 2] = ["total", "partial"];
const BRANCH_WORDS: [&str; 2] = ["core", "radial"];

fn is_structural(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    TIER4_WORDS.contains(&w.as_str())
        || TIER3_WORDS.contains(&w.as_str())
        || BRANCH_WORDS.contains(&w.as_str())
}

/// One power in a slot's pick list — identity plus display, straight from the
/// export index. Tier/branch/tree are derived views over `display_name`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncarnateCatalogPower {
    pub full_name: String,
    pub internal_name: String,
    pub display_name: String,
    #[serde(default)]
    pub short_help: String,
}

impl IncarnateCatalogPower {
    pub fn tier(&self) -> IncarnateTier {
        let words: Vec<String> = self
            .display_name
            .split_whitespace()
            .map(|w| w.to_ascii_lowercase())
            .collect();
        if words.iter().any(|w| TIER4_WORDS.contains(&w.as_str())) {
            IncarnateTier::VeryRare
        } else if words.iter().any(|w| TIER3_WORDS.contains(&w.as_str())) {
            IncarnateTier::Rare
        } else if words.iter().any(|w| BRANCH_WORDS.contains(&w.as_str())) {
            IncarnateTier::Uncommon
        } else {
            IncarnateTier::Common
        }
    }

    pub fn branch(&self) -> IncarnateBranch {
        let lower = self.display_name.to_ascii_lowercase();
        if lower.split_whitespace().any(|w| w == "radial") {
            IncarnateBranch::Radial
        } else if lower.split_whitespace().any(|w| w == "core") {
            IncarnateBranch::Core
        } else {
            IncarnateBranch::Base
        }
    }

    /// Which of its branch's two Rares this is, or `None` on any other tier.
    /// Read from the same `total`/`partial` word [`IncarnateTier`] uses to place
    /// the power on the Rare rung in the first place.
    pub fn rare_depth(&self) -> Option<RareDepth> {
        if self.tier() != IncarnateTier::Rare {
            return None;
        }
        let words: Vec<String> = self
            .display_name
            .split_whitespace()
            .map(|w| w.to_ascii_lowercase())
            .collect();
        if words.iter().any(|w| w == "total") {
            Some(RareDepth::Total)
        } else if words.iter().any(|w| w == "partial") {
            Some(RareDepth::Partial)
        } else {
            None
        }
    }

    /// The short label for a tree-node button: the display name minus its tree prefix, with
    /// each tier/branch word cut to an initial — "Cardiac Total Core Revamp" → "T.C. Revamp",
    /// "Cardiac Core Paragon" → "C. Paragon", "Cardiac Boost" → "Boost".
    ///
    /// The words that get cut are exactly the ones [`Self::tier`] and [`Self::branch`] read,
    /// so the label can't disagree with the tier/branch the node is placed by. Nothing is
    /// lost: a node's rung already states its tier and its column already states its branch
    /// and depth — the initials are there to keep those readable, not to carry them.
    ///
    /// Tier-4 words are NOT cut (the beta doesn't either): "Paragon" is the whole of what
    /// distinguishes a Very Rare's name, and there is only ever one word left beside it.
    pub fn node_label(&self) -> String {
        let tree = self.tree_name();
        let rest: Vec<&str> = self
            .display_name
            .strip_prefix(tree.as_str())
            .unwrap_or(&self.display_name)
            .split_whitespace()
            .collect();
        // A name that IS its bare tree name leaves nothing to label with; keep it whole
        // rather than rendering an empty node.
        if rest.is_empty() {
            return self.display_name.clone();
        }

        let mut initials = String::new();
        let mut words: Vec<&str> = Vec::new();
        for word in rest {
            let lower = word.to_ascii_lowercase();
            let structural =
                TIER3_WORDS.contains(&lower.as_str()) || BRANCH_WORDS.contains(&lower.as_str());
            match (structural, word.chars().next()) {
                (true, Some(first)) => {
                    initials.push(first);
                    initials.push('.');
                }
                _ => words.push(word),
            }
        }
        match (initials.is_empty(), words.is_empty()) {
            (true, _) => words.join(" "),
            (false, true) => initials,
            (false, false) => format!("{initials} {}", words.join(" ")),
        }
    }

    /// The tree this power belongs to: the display words before the first
    /// structural keyword. The Common root has no structural word, so its tree
    /// is everything but the trailing slot-suffix word ("Cardiac Boost" →
    /// "Cardiac", "Banished Pantheon Ally" → "Banished Pantheon").
    pub fn tree_name(&self) -> String {
        let words: Vec<&str> = self.display_name.split_whitespace().collect();
        if let Some(cut) = words.iter().position(|w| is_structural(w)) {
            words[..cut].join(" ")
        } else {
            words[..words.len().saturating_sub(1)].join(" ")
        }
    }
}

/// One incarnate slot's catalog: identity/display plus its pick list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncarnateSlotCatalog {
    /// Slot id in the beta's vocabulary: `alpha` … `genesis`.
    pub id: String,
    /// The export's powerset key, e.g. `Incarnate.Alpha`.
    pub key: String,
    pub display_name: String,
    pub icon: String,
    pub powers: Vec<IncarnateCatalogPower>,
}

/// A derived tree within a slot: the powers sharing one tree name, sorted
/// tier-then-branch (the picker's render order).
#[derive(Debug, Clone, PartialEq)]
pub struct IncarnateTreeView<'a> {
    pub name: String,
    pub powers: Vec<&'a IncarnateCatalogPower>,
}

/// Cells per row in a tree's craft grid. The game's ladder is symmetric — one Common root
/// that forks into a Core and a Radial side, each of which reaches two Rares and one Very
/// Rare — so the widest rung (both branches' Rares) is four, and an odd count is what lets
/// the single root sit centred under them.
pub const TREE_GRID_COLUMNS: usize = 5;

/// One rung of a tree's craft grid: the tier, and which power (if any) occupies each of the
/// five columns. Columns run Core-outer → Core-inner → centre → Radial-inner → Radial-outer,
/// so a branch reads as one vertical line down its own side of the grid and the two branches
/// mirror around the centre column the Common root sits in.
#[derive(Debug, Clone, PartialEq)]
pub struct IncarnateTreeRow<'a> {
    pub tier: IncarnateTier,
    pub cells: [Option<&'a IncarnateCatalogPower>; TREE_GRID_COLUMNS],
}

impl IncarnateSlotCatalog {
    /// Group this slot's powers into trees, alphabetical by tree name (the
    /// beta's sidebar order), each tree's powers sorted tier-then-branch.
    pub fn trees(&self) -> Vec<IncarnateTreeView<'_>> {
        let mut trees: Vec<IncarnateTreeView<'_>> = Vec::new();
        for power in &self.powers {
            let name = power.tree_name();
            match trees.iter_mut().find(|t| t.name == name) {
                Some(tree) => tree.powers.push(power),
                None => trees.push(IncarnateTreeView {
                    name,
                    powers: vec![power],
                }),
            }
        }
        for tree in &mut trees {
            tree.powers.sort_by_key(|p| (p.tier(), p.branch()));
        }
        trees.sort_by(|a, b| a.name.cmp(&b.name));
        trees
    }

    pub fn find_power(&self, internal_name: &str) -> Option<&IncarnateCatalogPower> {
        self.powers
            .iter()
            .find(|p| p.internal_name.eq_ignore_ascii_case(internal_name))
    }
}

impl<'a> IncarnateTreeView<'a> {
    /// The tree as the craft ladder it is: four rungs, Very Rare at the top down to the
    /// Common root, each [`TREE_GRID_COLUMNS`] wide. Every cell is placed from the power's
    /// own derived tier/branch/depth — nothing here is a table of names.
    ///
    /// A rung with no powers still comes back, as a row of `None`s: a slot whose export is
    /// missing a tier should read as a gap in the ladder, not as a ladder one rung shorter
    /// (Rule 1 — the absence is the thing worth seeing).
    pub fn grid_rows(&self) -> Vec<IncarnateTreeRow<'a>> {
        use IncarnateBranch::{Base, Core, Radial};
        use IncarnateTier::{Common, Rare, Uncommon, VeryRare};
        use RareDepth::{Partial, Total};

        // Column per (tier, branch, rare depth), reading Core-outer → Radial-outer.
        let veryrare = [Some((Core, None)), None, None, None, Some((Radial, None))];
        let rare = [
            Some((Core, Some(Total))),
            Some((Core, Some(Partial))),
            None,
            Some((Radial, Some(Partial))),
            Some((Radial, Some(Total))),
        ];
        let uncommon = [None, Some((Core, None)), None, Some((Radial, None)), None];
        let common = [None, None, Some((Base, None)), None, None];

        [
            (VeryRare, veryrare),
            (Rare, rare),
            (Uncommon, uncommon),
            (Common, common),
        ]
        .into_iter()
        .map(|(tier, layout)| IncarnateTreeRow {
            tier,
            cells: layout.map(|slot| {
                let (branch, depth) = slot?;
                self.powers
                    .iter()
                    .copied()
                    .find(|p| p.tier() == tier && p.branch() == branch && p.rare_depth() == depth)
            }),
        })
        .collect()
    }

    /// The powers `power` is crafted THROUGH — its prerequisite chain down to the Common
    /// root, `power` itself excluded. This is the game's craft ladder read backwards: an
    /// Uncommon comes from the root, a Rare from its branch's Uncommon, and a Very Rare
    /// from both of its branch's Rares.
    ///
    /// A Very Rare can in fact be crafted from any two of the tree's four Rares; this
    /// returns the same-branch pair, which is the default path and the only one the picker
    /// shows. Empty for the Common root, which has nothing beneath it.
    pub fn path_to(&self, power: &IncarnateCatalogPower) -> Vec<&'a IncarnateCatalogPower> {
        let branch = power.branch();
        let mut path: Vec<&'a IncarnateCatalogPower> = Vec::new();
        let mut push = |tier: IncarnateTier, branch: IncarnateBranch, depth: Option<RareDepth>| {
            if let Some(found) = self
                .powers
                .iter()
                .copied()
                .find(|p| p.tier() == tier && p.branch() == branch && p.rare_depth() == depth)
            {
                path.push(found);
            }
        };

        match power.tier() {
            IncarnateTier::Common => return path,
            IncarnateTier::Uncommon => push(IncarnateTier::Common, IncarnateBranch::Base, None),
            IncarnateTier::Rare => {
                push(IncarnateTier::Common, IncarnateBranch::Base, None);
                push(IncarnateTier::Uncommon, branch, None);
            }
            IncarnateTier::VeryRare => {
                push(IncarnateTier::Common, IncarnateBranch::Base, None);
                push(IncarnateTier::Uncommon, branch, None);
                push(IncarnateTier::Rare, branch, Some(RareDepth::Total));
                push(IncarnateTier::Rare, branch, Some(RareDepth::Partial));
            }
        }
        path
    }
}

/// The whole `incarnate-catalog` section: every slot the dataset's export
/// carries, in picker tab order.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct IncarnateCatalog {
    pub slots: Vec<IncarnateSlotCatalog>,
}

impl IncarnateCatalog {
    /// Parse the contract's `incarnate-catalog` section. Malformed ≠ absent
    /// (Rule 1, matching [`crate::incarnate_effects`]): an ABSENT section
    /// degrades to an empty catalog (a hand-constructed `PowerDatabase` needs
    /// none — the picker then offers nothing), but a PRESENT section that
    /// doesn't parse is an error, never silently the default.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(IncarnateCatalog::default());
        };
        serde_json::from_value(section.clone())
            .map_err(|e| format!("incarnate-catalog section: {e}"))
    }

    pub fn slot(&self, id: &str) -> Option<&IncarnateSlotCatalog> {
        self.slots.iter().find(|s| s.id == id)
    }
}
