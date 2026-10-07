//! Incarnate crafting — the typed view of the contract's `incarnate-crafting`
//! section: every craft recipe the dataset ships (the salvage and prerequisite
//! POWERS consumed, the ability granted) plus the salvage catalog those recipes
//! reference, priced from the dataset's own conversion-store rows.
//!
//! Binary-sourced from `baserecipes.bin` on all three forks
//! (`tools/bin-crawler/bin_crawler/parser/_recipes.py` →
//! `export_incarnate_recipes.py` → `emit-contract.cjs`). The Homecoming decode
//! is oracle-gated against the community-collected tables the beta vendored
//! which also document the two
//! places the binary is MORE complete: the Demons lore tree the hand tables
//! lack, and the tier-3 radial Total/Partial pair those tables carried in
//! visual (mirror) order — a swap the beta's own modal displayed as fact.
//!
//! The craft ladder is stated by the recipes themselves — a Tier 2 recipe
//! consumes the Tier 1 POWER, a Tier 4 consumes two Rares — so [`craft_tree`]
//! is assembly, not derivation: it never invents an edge the data doesn't
//! state. The one choice it makes is WHICH recipe variant roots a Very Rare
//! (the game ships all six two-of-four-Rares pairs as separate recipes, same
//! salvage each); it takes the same-branch pair, the default
//! [`crate::incarnate_catalog::IncarnateTreeView::path_to`] shows.

use serde::Deserialize;
use serde_json::Value;

use crate::incarnate_catalog::{IncarnateSlotCatalog, RareDepth};

/// Which authored family a recipe belongs to, from its def file (the converter
/// maps the source basename; an unknown basename stops the emit, so this enum
/// is exhaustive over what can reach the contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum RecipeFamily {
    /// The live thread-salvage path — what the in-game creation tab crafts.
    #[serde(rename = "current")]
    Current,
    /// The original shard-component path (Homecoming authors it for Alpha).
    #[serde(rename = "legacyShard")]
    LegacyShard,
    /// PvP-zone grants: no salvage, no prerequisites, gated by map.
    #[serde(rename = "pvp")]
    Pvp,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SalvageAmount {
    pub id: String,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerAmount {
    pub full_name: String,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftRecipe {
    pub name: String,
    pub family: RecipeFamily,
    /// Full name of the incarnate ability this recipe creates.
    pub reward_power: String,
    /// The resolved crafting-UI tab, `Slot|Tree|Rarity` (`Alpha|Vigor|Uncommon`).
    pub tab: String,
    pub salvage: Vec<SalvageAmount>,
    pub power_components: Vec<PowerAmount>,
}

/// Rarity as salvage.bin states it. `InfiniteTessellation` reads `uncommon`
/// here while the community registry says very-rare — exported as-is (Rule 0)
/// and pinned by the oracle gate; only legacy-family recipes reference it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub enum SalvageRarity {
    #[serde(rename = "common")]
    Common,
    #[serde(rename = "uncommon")]
    Uncommon,
    #[serde(rename = "rare")]
    Rare,
    #[serde(rename = "very-rare")]
    VeryRare,
}

impl SalvageRarity {
    /// The tier token a rule keys its colour on (`--tier-*` in tokens.css) —
    /// the same vocabulary [`crate::incarnate_catalog::IncarnateTier`] uses.
    pub fn icon_token(self) -> &'static str {
        match self {
            SalvageRarity::Common => "common",
            SalvageRarity::Uncommon => "uncommon",
            SalvageRarity::Rare => "rare",
            SalvageRarity::VeryRare => "veryrare",
        }
    }
}

/// One purchase route the dataset's own store states: `amount` of `currency`
/// (a salvage display name — "Incarnate Thread", "Empyrean Merit") buys one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BuyOption {
    pub currency: String,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftSalvage {
    pub id: String,
    pub display_name: String,
    pub rarity: SalvageRarity,
    pub icon: String,
    /// Single-currency store rows only; a salvage with no row is not
    /// purchasable outright (the legacy drops) and prices as nothing.
    pub buy: Vec<BuyOption>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct IncarnateCrafting {
    pub recipes: Vec<CraftRecipe>,
    pub salvage: Vec<CraftSalvage>,
}

/// One rung of a goal ability's craft tree: the recipe crafting `power`, and
/// beneath it the tree of everything that recipe consumes.
#[derive(Debug, Clone, PartialEq)]
pub struct CraftNode {
    /// Full name of the ability this node crafts.
    pub power: String,
    /// Goal-independent checklist identity: `child@parent` (root: `goal@`).
    /// The parent qualifier matters because a Very Rare's tree holds the SAME
    /// Tier 2 twice — once under each consumed Rare — and the two instances
    /// are separate crafts marked off separately; being parent-relative rather
    /// than goal-relative is what lets a mark survive the goal tier changing.
    pub key: String,
    pub salvage: Vec<SalvageAmount>,
    pub children: Vec<CraftNode>,
}

impl IncarnateCrafting {
    /// Parse the contract's `incarnate-crafting` section. Malformed ≠ absent
    /// (Rule 1, matching [`crate::incarnate_catalog`]): an ABSENT section
    /// degrades to no crafting data (the modal then offers nothing), but a
    /// PRESENT section that doesn't parse is an error, never the default.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(IncarnateCrafting::default());
        };
        serde_json::from_value(section.clone())
            .map_err(|e| format!("incarnate-crafting section: {e}"))
    }

    /// Every recipe creating `reward_power`, case-insensitively (the loadout
    /// stores what the export gave it, which is not always this casing).
    pub fn recipes_for(&self, reward_power: &str) -> Vec<&CraftRecipe> {
        self.recipes
            .iter()
            .filter(|r| r.reward_power.eq_ignore_ascii_case(reward_power))
            .collect()
    }

    pub fn salvage(&self, id: &str) -> Option<&CraftSalvage> {
        self.salvage.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    /// The full craft tree for `goal_power` on the CURRENT family — the path
    /// the in-game creation tab crafts. `Err` carries what was missing or
    /// ambiguous, never a partial tree (Rule 1).
    pub fn craft_tree(
        &self,
        slot: &IncarnateSlotCatalog,
        goal_power: &str,
    ) -> Result<CraftNode, String> {
        self.node(slot, goal_power, "", 0)
    }

    fn node(
        &self,
        slot: &IncarnateSlotCatalog,
        power: &str,
        parent: &str,
        depth: usize,
    ) -> Result<CraftNode, String> {
        // The authored ladder is four tiers; anything deeper means the recipe
        // data acquired a cycle or a new shape, and recursing on would hang.
        if depth > 6 {
            return Err(format!(
                "craft chain under {power} exceeds the authored ladder"
            ));
        }
        let recipe = self.current_recipe(slot, power)?;
        let mut children = Vec::new();
        for component in &recipe.power_components {
            children.push(self.node(slot, &component.full_name, power, depth + 1)?);
        }
        Ok(CraftNode {
            power: recipe.reward_power.clone(),
            key: format!("{}@{parent}", recipe.reward_power),
            salvage: recipe.salvage.clone(),
            children,
        })
    }

    /// The one current-family recipe this tree descends through for `power`.
    /// A Very Rare ships six pair variants; the same-branch pair — both
    /// consumed Rares on the goal's own branch — is the default the picker's
    /// ladder highlights, resolved through the catalog's derived branch/depth
    /// rather than any name table.
    fn current_recipe(
        &self,
        slot: &IncarnateSlotCatalog,
        power: &str,
    ) -> Result<&CraftRecipe, String> {
        let candidates: Vec<&CraftRecipe> = self
            .recipes_for(power)
            .into_iter()
            .filter(|r| r.family == RecipeFamily::Current)
            .collect();
        match candidates.as_slice() {
            [] => Err(format!("no current-family recipe crafts {power}")),
            [only] => Ok(only),
            _ => {
                let goal = slot
                    .find_power(power.rsplit('.').next().unwrap_or(power))
                    .ok_or_else(|| format!("{power}: not in this slot's catalog"))?;
                let branch = goal.branch();
                let same_branch = |r: &&CraftRecipe| {
                    let mut depths = Vec::new();
                    for c in &r.power_components {
                        let internal = c.full_name.rsplit('.').next().unwrap_or(&c.full_name);
                        match slot.find_power(internal) {
                            Some(p) if p.branch() == branch => depths.push(p.rare_depth()),
                            _ => return false,
                        }
                    }
                    depths.contains(&Some(RareDepth::Total))
                        && depths.contains(&Some(RareDepth::Partial))
                };
                candidates
                    .iter()
                    .find(|r| same_branch(r))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "{power}: {} recipe variants but no same-branch Total+Partial pair",
                            candidates.len()
                        )
                    })
            }
        }
    }
}

impl CraftNode {
    /// Salvage still needed across this subtree: a node is skipped (already
    /// crafted, its ingredients spent) when it — or any ancestor — is obtained.
    /// Totals accumulate into `needed` as (salvage id → count).
    pub fn remaining_salvage(
        &self,
        is_obtained: &dyn Fn(&str) -> bool,
        needed: &mut std::collections::BTreeMap<String, u32>,
    ) {
        if is_obtained(&self.key) {
            return; // an obtained node's whole subtree was consumed making it
        }
        for s in &self.salvage {
            *needed.entry(s.id.clone()).or_insert(0) += s.amount;
        }
        for child in &self.children {
            child.remaining_salvage(is_obtained, needed);
        }
    }
}
