//! Caster state — which form of itself the build is currently in.
//!
//! Three families of control, all of them build-wide and all of them derived from the export
//! rather than from a table of game names (Rule 0). The beta wrote its equivalent as
//! `STANCE_GROUPS`, a hand-authored list naming Bio Armor, Staff Fighting and Dual Pistols and
//! their sub-powers; nothing here names a power, a set or an archetype.
//!
//! * **Stances** — a power the build holds hands out two or more mutually exclusive
//!   sub-powers, and the build records which one is live in the parent pick's
//!   [`SelectedPower::active_sub_power`]. This is the one family that moves TOTALS: Pass 0
//!   expands the active sub-power like any toggle, and the modes it publishes through
//!   `setsModes` are what satisfy the `<mode> Source.Mode?` gates on the rest of the set's
//!   atoms.
//! * **Caster modes** — while a mode is live the game's PowerRedirector fires a different
//!   record entirely (a Kheldian attack in Nova form, a Primalist attack in Hunter Form),
//!   which the display resolver reads from [`CombatContext::active_modes`]. Display only.
//! * **Global mechanics** — the `scope: "global"` conditional effects the build's powers
//!   declare (Domination, ammo, insight, combo levels), read by the same display resolver
//!   from [`CombatContext::global_conditionals`]. Display only.
//!
//! Every list is scoped to the powers the build actually holds, so a control appears exactly
//! when it can change something the build shows — and, where a conditional forks on the caster's
//! archetype, to the classes the entry names ([`conditional_for_class`], COND-4).
//!
//! [`CombatContext::active_modes`]: crate::CombatContext::active_modes
//! [`CombatContext::global_conditionals`]: crate::CombatContext::global_conditionals

use crate::granted_powers::{granted_by, MIN_STANCE_OPTIONS};
use crate::{CharacterState, Power, PowerDatabase, Powerset, SelectedPower};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Shown for the no-stance state when the set publishes no conditional naming the parent —
/// i.e. when clearing the stance leaves the build in no named form at all.
const CLEARED_FALLBACK: &str = "None";

/// One selectable form of a stance group: a sub-power the parent hands out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanceOption {
    /// The sub-power's `internalName` — the value written to
    /// [`SelectedPower::active_sub_power`].
    pub internal_name: String,
    /// Display name.
    pub name: String,
    /// The `conditionalEffects` ids this option answers to, as the set's own powers spell
    /// them ([`conditional_ids`]). Empty when the set gates nothing on this form.
    pub conditional_ids: Vec<String>,
}

/// A parent power's mutually exclusive granted forms, and which one the build is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanceGroup {
    /// Set id of the parent pick — half of the address [`set_stance`] writes through.
    pub powerset: String,
    /// The parent's `internalName`.
    pub parent: String,
    /// The parent's display name; the group's heading.
    pub parent_name: String,
    /// Label for the cleared state. Some parents name that state themselves (a Dual Pistols
    /// build with no ammo loaded is firing Standard rounds, which the set's own conditionals
    /// label), so it is read from the data where the data has it and falls back to
    /// [`CLEARED_FALLBACK`] where it does not.
    pub cleared_label: String,
    /// The conditional ids the CLEARED state answers to — the parent's own, since a parent
    /// that publishes a mode is publishing the build's default form.
    pub cleared_conditional_ids: Vec<String>,
    /// The live option's `internalName`, or `None` for the cleared state.
    pub active: Option<String>,
    /// The alternatives, in the set's own power order.
    pub options: Vec<StanceOption>,
}

impl StanceGroup {
    /// Every conditional id this group owns, across all its states.
    fn conditional_ids(&self) -> impl Iterator<Item = &String> {
        self.cleared_conditional_ids.iter().chain(
            self.options
                .iter()
                .flat_map(|option| &option.conditional_ids),
        )
    }
}

/// A caster mode the build can switch on, and whether it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeToggle {
    /// The mode token, as `modeVariants` is keyed by it — the value stored in
    /// [`CombatContext::active_modes`](crate::CombatContext::active_modes).
    pub key: String,
    /// The token, spaced for reading.
    pub label: String,
    /// Display name of the build's own power that publishes this mode through `setsModes`,
    /// when exactly one does. A mode with no publisher in the build is still offered — the
    /// game has modes no power declares (a weapon's momentum window) — but one with a
    /// publisher reads better beside the power that turns it on.
    pub source: Option<String>,
    pub active: bool,
}

/// A `scope: "global"` conditional the build's powers declare, and whether it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MechanicToggle {
    /// The conditional's own id — the [`CombatContext::global_conditionals`] key.
    ///
    /// [`CombatContext::global_conditionals`]: crate::CombatContext::global_conditionals
    pub id: String,
    /// The converter's own label for it.
    pub label: String,
    /// Whether it resolves on for this build — the build's toggle, or the entry's
    /// `defaultActive` when the build has never touched it.
    pub active: bool,
    /// The mutual-exclusion group the export puts it in, when it puts it in one.
    ///
    /// The caster is at ONE combo level, holds ONE count of Tidal Power — the group is how the
    /// data says so. Three of them (Street Justice's combo levels, Water Blast's tidal stacks,
    /// Wind Control's pressure stacks) reach this list rather than a stance selector, because
    /// they are earned in combat rather than handed out as sub-powers, so nothing else in the
    /// build enforces the exclusion.
    pub group: Option<String>,
}

/// Every stance group the build holds a parent for.
///
/// A group is a pick whose set contains two or more granted powers gated on holding that
/// pick. Both halves are read from the export: the game marks a handed-out power with
/// `autoIssue` ([`Power::is_auto_issued`]), and the grant gate is the sub-power's own
/// `requires`.
pub fn stance_groups(state: &CharacterState, db: &PowerDatabase) -> Vec<StanceGroup> {
    let class_name = caster_class_name(state, db);
    state
        .all_selected()
        .filter_map(|selection| stance_group(state, selection, db, class_name))
        .collect()
}

/// The stance group one pick heads, if it heads one — what a power card asks to draw its own
/// choice strip, without deriving every other group in the build.
pub fn stance_group_for(
    state: &CharacterState,
    db: &PowerDatabase,
    powerset: &str,
    parent: &str,
) -> Option<StanceGroup> {
    let selection = state
        .all_selected()
        .find(|selection| selection.powerset == powerset && selection.internal_name == parent)?;
    stance_group(state, selection, db, caster_class_name(state, db))
}

fn stance_group(
    state: &CharacterState,
    selection: &SelectedPower,
    db: &PowerDatabase,
    class_name: Option<&str>,
) -> Option<StanceGroup> {
    let set = db.find_powerset(&selection.powerset)?;
    let parent = set
        .powers
        .iter()
        .find(|power| power.ident() == selection.internal_name)?;
    let forms: Vec<&Power> = set
        .powers
        .iter()
        .filter(|power| power.is_auto_issued() && granted_by(power, parent.ident()))
        .collect();
    if forms.len() < MIN_STANCE_OPTIONS {
        return None;
    }
    let scope = held_scope(state, db, set);
    let options: Vec<StanceOption> = forms
        .iter()
        .map(|power| {
            let mut ids = conditional_ids(&scope, power, class_name);
            ids.extend(kept_claim_ids(&scope, power, &forms, class_name));
            ids.sort();
            ids.dedup();
            StanceOption {
                internal_name: power.ident().to_string(),
                name: power.name.clone(),
                conditional_ids: ids,
            }
        })
        .collect();
    let cleared_conditional_ids = conditional_ids(&scope, parent, class_name);
    Some(StanceGroup {
        powerset: selection.powerset.clone(),
        parent: parent.ident().to_string(),
        parent_name: parent.name.clone(),
        cleared_label: cleared_conditional_ids
            .first()
            .and_then(|id| conditional_label(&scope, id, class_name))
            .unwrap_or_else(|| CLEARED_FALLBACK.to_string()),
        cleared_conditional_ids,
        active: selection.active_sub_power.clone(),
        options,
    })
}

/// Every power whose `conditionalEffects` a stance in `parent_set` can answer: the parent's own
/// set, plus every set the build holds.
///
/// The parent's set alone is not enough. Thunderspy issues Swap Ammo, Staff Mastery and Bio
/// Armor's Adaptation from the inherent set, while the attacks and armours whose conditionals
/// name the forms stay in the primary or secondary — so a selector scoped to its own set found
/// none of them, and its choice moved the totals while the power text stayed in the old form.
/// Held SETS rather than held picks, so a conditional on a power picked after the stance was
/// set is already in step with it.
fn held_scope<'a>(
    state: &CharacterState,
    db: &'a PowerDatabase,
    parent_set: &'a Powerset,
) -> Vec<&'a Power> {
    let mut scope: Vec<&Power> = parent_set.powers.iter().collect();
    for id in [&state.primary.id, &state.secondary.id]
        .into_iter()
        .flatten()
    {
        if let Some(set) = db.find_powerset(id).filter(|set| set.id != parent_set.id) {
            scope.extend(set.powers.iter());
        }
    }
    let partitions: Vec<&str> = state
        .pools
        .iter()
        .chain(state.epic_pool.iter())
        .map(|pool| pool.id.as_str())
        .collect();
    scope.extend(
        db.pool_powers
            .iter()
            .chain(db.epic_powers.iter())
            .filter(|entry| partitions.contains(&entry.set_id.as_str()))
            .map(|entry| &entry.power),
    );
    scope
}

/// The `conditionalEffects` ids in `scope` that name `power` — either the power itself or a mode
/// it publishes.
///
/// The converter derives a conditional's id by folding the token its gate names: a gate reading
/// the caster's mode (`kDefensiveAdaptation Source.Mode?`) yields the mode, a gate reading
/// whether the caster holds a power yields the power. So a form's conditional is found by
/// folding both spellings and looking for either — which is why the reconciliation covers a
/// stance whose sub-power and mode disagree (Efficient Adaptation publishes the legacy
/// `RestedAdaptation`) as well as one where they agree.
fn conditional_ids(scope: &[&Power], power: &Power, class_name: Option<&str>) -> Vec<String> {
    let named: Vec<String> = std::iter::once(power.ident())
        .chain(modes(power))
        .map(fold)
        .collect();
    let mut ids: Vec<String> = scope
        .iter()
        .flat_map(|power| conditional_effects(power, class_name))
        .filter(|conditional| named.contains(&fold(&conditional.id)))
        .map(|conditional| conditional.id)
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The ownership conditionals a form KEEPS: the ones whose claimed power a sibling form revokes
/// and this one doesn't.
///
/// A form that works through a granted power names no mode a gate could read. Staff Fighting's
/// forms are the case: each toggle makes the set's attacks build a Perfection temp power, and
/// the finishers gate on owning its third level (`…Perfection_of_Body_Level_3 source.ownPower?`).
/// The toggle's own record never names that power — it only revokes the OTHER two forms'
/// Perfection powers, in its `grantEdges`. That is enough: a claim every sibling form revokes
/// and this form leaves alone can only be built under this form. Selecting the form then
/// declares the finished stack, the state its finisher bonuses read.
fn kept_claim_ids(
    scope: &[&Power],
    form: &Power,
    forms: &[&Power],
    class_name: Option<&str>,
) -> Vec<String> {
    let own = revoked_paths(form);
    let by_siblings: BTreeSet<String> = forms
        .iter()
        .filter(|sibling| sibling.ident() != form.ident())
        .flat_map(|sibling| revoked_paths(sibling))
        .collect();
    scope
        .iter()
        .flat_map(|power| conditional_effects(power, class_name))
        .filter(|conditional| conditional.global)
        .filter_map(|conditional| {
            let (path, _) = conditional.owned_power.as_ref()?;
            let path = path.to_ascii_lowercase();
            (by_siblings.contains(&path) && !own.contains(&path)).then_some(conditional.id)
        })
        .collect()
}

/// The power paths a power's `grantEdges` revoke, lower-cased.
fn revoked_paths(power: &Power) -> BTreeSet<String> {
    power
        .extra
        .get("grantEdges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|edge| edge.get("op").and_then(Value::as_str) == Some("revoke"))
        .filter_map(|edge| edge.get("path").and_then(Value::as_str))
        .map(str::to_ascii_lowercase)
        .collect()
}

fn conditional_label(scope: &[&Power], id: &str, class_name: Option<&str>) -> Option<String> {
    scope
        .iter()
        .flat_map(|power| conditional_effects(power, class_name))
        .find(|conditional| conditional.id == id)
        .and_then(|conditional| conditional.label)
}

/// Point `group` at `option` (or clear it), reconciling the display conditionals it owns.
///
/// The stance is the single source of truth for the form a build is in, so selecting one
/// writes both consumers at once: the parent pick's `active_sub_power`, which the totals read,
/// and the `global_conditionals` entries the group's forms answer to, which the per-power
/// display reads. Writing only the first would leave a build whose totals are in one form and
/// whose power text describes another.
pub fn set_stance(state: &mut CharacterState, group: &StanceGroup, option: Option<&str>) {
    let Some(selection) = state.selected_power_mut(&group.powerset, &group.parent) else {
        return;
    };
    selection.active_sub_power = option.map(str::to_string);

    let live = match option {
        None => &group.cleared_conditional_ids,
        Some(chosen) => match group
            .options
            .iter()
            .find(|candidate| candidate.internal_name == chosen)
        {
            Some(chosen) => &chosen.conditional_ids,
            None => return,
        },
    };
    for id in group.conditional_ids() {
        state
            .combat
            .global_conditionals
            .insert(id.clone(), live.contains(id));
    }
}

/// The caster modes this build can switch on: the modes its own powers publish variants for.
///
/// Keyed off `modeVariants` rather than off `setsModes` because that is what the control can
/// change — a mode no power in the build redirects on has nothing to show for being switched
/// on, and the totals never read this state at all (they resolve modes from what is actually
/// running, `coh_math::gather`).
pub fn caster_modes(state: &CharacterState, db: &PowerDatabase) -> Vec<ModeToggle> {
    let held: Vec<&Power> = state
        .all_selected()
        .filter_map(|selection| db.resolve_power(&selection.powerset, &selection.internal_name))
        .collect();

    let keys: BTreeSet<&str> = held
        .iter()
        .flat_map(|power| mode_variant_keys(power))
        .collect();
    keys.into_iter()
        .map(|key| ModeToggle {
            key: key.to_string(),
            label: spaced(key),
            source: publisher(&held, key),
            active: state.combat.active_modes.contains(key),
        })
        .collect()
}

/// The caster modes that change WHAT THE BUILD CAN CAST: a mode one of its own powers sets,
/// and that some held click power's `modesRequired` or `modesDisallowed` names.
///
/// Distinct from [`caster_modes`], which is keyed off `modeVariants` — what a mode changes
/// about a power that is castable either way. A Kheldian's Nova and Dwarf forms are both
/// (they redirect the tier-one attacks AND lock out everything human); a Bio Armor adaptation
/// is only the latter; a mode nothing gates on is neither, and offering it as a form would be
/// offering a control with no consequence.
///
/// Both halves of the gate are read because neither alone answers the question. `modesRequired`
/// says which form an attack belongs to, but the human attacks do not carry it — they are
/// castable in no form, which only `modesDisallowed` states. The result is scoped to the modes
/// the build can actually enter, so a power gated on a form the build cannot take never
/// invents one.
pub fn form_modes(state: &CharacterState, db: &PowerDatabase) -> Vec<String> {
    let held: Vec<&Power> = state
        .all_selected()
        .filter_map(|selection| db.resolve_power(&selection.powerset, &selection.internal_name))
        .collect();

    let settable: BTreeSet<&str> = held.iter().flat_map(|power| modes(power)).collect();
    if settable.is_empty() {
        return Vec::new();
    }

    let mut gating: BTreeSet<&str> = BTreeSet::new();
    for power in &held {
        if !is_click(power) {
            continue;
        }
        let gates =
            string_array(power, "modesRequired").chain(string_array(power, "modesDisallowed"));
        gating.extend(gates.filter(|mode| settable.contains(mode)));
    }
    gating.into_iter().map(str::to_string).collect()
}

/// Whether `power` can be cast while `mode` is the build's live form (`None` = no form).
///
/// `form_modes` is the set of forms this build can be in, and it is what makes an unqualified
/// `modesRequired` answerable: a power requiring a mode the build can enter is castable only in
/// that mode, while one requiring a mode outside the build's forms (a weapon's momentum window,
/// a Domination window) is not form-gated at all and stays available.
pub fn castable_in_mode(power: &Power, mode: Option<&str>, form_modes: &[String]) -> bool {
    if let Some(live) = mode {
        if string_array(power, "modesDisallowed").any(|disallowed| disallowed == live) {
            return false;
        }
    }
    !string_array(power, "modesRequired")
        .any(|required| form_modes.iter().any(|form| form == required) && Some(required) != mode)
}

/// The display spelling of a mode key — `Peacebringer_Blaster_Mode` reads as
/// "Peacebringer Blaster Mode". The keys are the game's own tokens, so this is the only
/// naming any mode control does.
pub fn mode_label(key: &str) -> String {
    spaced(key)
}

/// The display spelling of a mutual-exclusion group id — `combo-levels` reads as "combo levels".
///
/// The export names the group but never labels it (unlike the conditionals inside it, which
/// carry their own `label`), so this is the one place a group's heading comes from. Separators
/// become spaces and nothing else changes: inventing a nicer name would be inventing.
pub fn group_label(group: &str) -> String {
    spaced(group)
}

fn is_click(power: &Power) -> bool {
    power
        .extra
        .get("powerType")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("click"))
}

/// The one held power publishing `key` through `setsModes`, or `None` when none or several do
/// — a label is only worth showing when it points somewhere unambiguous.
fn publisher(held: &[&Power], key: &str) -> Option<String> {
    let mut publishers = held
        .iter()
        .filter(|power| modes(power).any(|mode| mode == key));
    let first = publishers.next()?;
    publishers.next().is_none().then(|| first.name.clone())
}

/// The build-wide conditional mechanics its powers declare, minus the ones a stance selector
/// already owns — a form is chosen once, through the control that also moves the totals, not
/// twice through two controls that can disagree.
pub fn global_mechanics(state: &CharacterState, db: &PowerDatabase) -> Vec<MechanicToggle> {
    let owned: BTreeSet<String> = stance_groups(state, db)
        .iter()
        .flat_map(|group| group.conditional_ids().cloned().collect::<Vec<_>>())
        .collect();

    let class_name = caster_class_name(state, db);
    let mut toggles: BTreeMap<String, MechanicToggle> = BTreeMap::new();
    for selection in state.all_selected() {
        let Some(power) = db.resolve_power(&selection.powerset, &selection.internal_name) else {
            continue;
        };
        for conditional in conditional_effects(power, class_name).filter(|c| c.global) {
            if owned.contains(&conditional.id) || conditional.answered_by_picks(db) {
                continue;
            }
            let active = state
                .combat
                .global_conditionals
                .get(&conditional.id)
                .copied()
                .unwrap_or(conditional.default_active);
            toggles
                .entry(conditional.id.clone())
                .or_insert_with(|| MechanicToggle {
                    label: conditional
                        .label
                        .clone()
                        .unwrap_or_else(|| spaced(&conditional.id)),
                    id: conditional.id,
                    active,
                    group: conditional.group,
                });
        }
    }
    let mut toggles: Vec<MechanicToggle> = toggles.into_values().collect();
    toggles.sort_by(|a, b| a.label.cmp(&b.label).then_with(|| a.id.cmp(&b.id)));
    toggles
}

/// Switch a global mechanic on or off, clearing the group it excludes.
///
/// The sibling half is the point. A grouped mechanic is a state the caster is IN, one of
/// several — Street Justice's combo level, Water Blast's tidal stacks, Wind Control's pressure
/// — and switching each independently lets a build claim all three at once, which the display
/// then merges into a caster holding every stack simultaneously. Nothing else in the build
/// enforces it: the groups that ARE handed out as sub-powers reach [`set_stance`] instead, and
/// never arrive here at all.
///
/// One call is one act, so the whole choice lands in a single commit and a single undo step.
pub fn set_global_mechanic(
    state: &mut CharacterState,
    mechanics: &[MechanicToggle],
    id: &str,
    on: bool,
) {
    let group = mechanics
        .iter()
        .find(|mechanic| mechanic.id == id)
        .and_then(|mechanic| mechanic.group.as_deref())
        .filter(|_| on);

    let Some(group) = group else {
        state.combat.global_conditionals.insert(id.to_string(), on);
        return;
    };
    for sibling in mechanics
        .iter()
        .filter(|mechanic| mechanic.group.as_deref() == Some(group))
    {
        state
            .combat
            .global_conditionals
            .insert(sibling.id.clone(), sibling.id == id);
    }
}

/// One `conditionalEffects` entry, owned so it outlives the borrow of the power that
/// declared it.
struct Conditional {
    id: String,
    label: Option<String>,
    /// `scope: "global"` — caster state shared by every power, as against the per-power
    /// target state (a foe disintegrating) that belongs beside the power it tracks.
    global: bool,
    default_active: bool,
    group: Option<String>,
    /// The caster-ownership state this toggle asserts: `(path, count)` from the converter's
    /// `ownedPower`. Absent unless the gate behind the toggle is a power-presence test that
    /// states one count — see [`active_ownership_claims`].
    owned_power: Option<(String, f64)>,
}

/// Does this ownership claim name a power the player PICKS?
///
/// Then the build's picks already answer it, and a toggle could only repeat the pick or
/// contradict it. Both happened: Rebirth's Boxing, Kick and Cross Punch each scale their damage
/// by which of the other two you own (`Pool.Fighting.Kick source.ownPowerNum? .15 *` inside the
/// magnitude expression), and a "Kick" toggle beside a picked Kick moved nothing, while one
/// beside an unpicked Kick credited a power the build doesn't hold.
///
/// A charge, a combo level or a stack count is never a pick (its set is `Temporary_Powers` or
/// `Redirects`, which no build holds), and an auto-issued power isn't bought, so those keep
/// their toggles. A count above one is a stack, never a pick, whatever the path.
fn claim_is_pickable(path: &str, count: f64, db: &PowerDatabase) -> bool {
    if count != 1.0 {
        return false;
    }
    let Some((set_path, name)) = path.rsplit_once('.') else {
        return false;
    };
    let want = crate::pick_rules::normalize(set_path);
    let names_set = |candidate: Option<&str>| {
        candidate.is_some_and(|candidate| crate::pick_rules::normalize(candidate) == want)
    };
    let in_powerset = db
        .powersets
        .iter()
        .filter(|set| names_set(set.set_path.as_deref()))
        .flat_map(|set| set.powers.iter());
    let in_pool = db
        .pool_catalog
        .pools
        .iter()
        .chain(db.pool_catalog.epics.iter())
        .filter(|pool| names_set(pool.set_path.as_deref()))
        .flat_map(|pool| {
            db.pool_powers
                .iter()
                .chain(db.epic_powers.iter())
                .filter(move |entry| entry.set_id == pool.id)
                .map(|entry| &entry.power)
        });
    in_powerset
        .chain(in_pool)
        .find(|power| power.ident().eq_ignore_ascii_case(name))
        .is_some_and(|power| !power.is_auto_issued())
}

/// Whether the build owns the power a `conditionalEffects` entry's claim names, when that power
/// is one the player picks ([`claim_is_pickable`]); `None` when the entry is a real toggle.
///
/// The display reads this in place of the toggle, so a power's numbers follow the picks the
/// same way its damage does.
pub fn picks_answer(entry: &Value, state: &CharacterState, db: &PowerDatabase) -> Option<bool> {
    let claim = entry.get("ownedPower")?;
    let path = claim.get("path").and_then(Value::as_str)?;
    let count = claim.get("count").and_then(Value::as_f64)?;
    claim_is_pickable(path, count, db)
        .then(|| crate::pick_rules::owned_power_count(state, &db.set_paths, path) > 0.0)
}

impl Conditional {
    fn answered_by_picks(&self, db: &PowerDatabase) -> bool {
        self.owned_power
            .as_ref()
            .is_some_and(|(path, count)| claim_is_pickable(path, *count, db))
    }
}

/// Every `conditionalEffects` id this dataset spells.
///
/// The vocabulary a `.skif` file's `stances` map is keyed by, which is what lets
/// [`crate::skif::decode`] tell a key the dataset no longer spells from one it still does —
/// the question the conditional-id respelling migration turns on.
///
/// Reaches past [`PowerDatabase::all_powers`] into the basic inherents as well, because a build
/// runs those too and [`global_mechanics`] offers their toggles like any other power's.
/// Deliberately unfiltered by archetype, unlike every other reader here: this is the DATASET's
/// vocabulary, not one build's controls, and a key must stay recognizable to the migration on a
/// build whose class the entry does not name.
pub fn declared_conditional_ids(db: &PowerDatabase) -> BTreeSet<String> {
    db.all_powers()
        .chain(db.inherent_powers.iter())
        .flat_map(conditional_entries)
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// The build's own class token (`"Class_Blaster"`), or `None` before an archetype is chosen or
/// where the dataset states none for it.
///
/// The one answer to "which class is casting", shared by everything that resolves an archetype
/// fork — the atom filter in `coh_math::gather` and the toggle filter below read the same
/// question, and two spellings of it could disagree.
pub fn caster_class_name<'a>(state: &CharacterState, db: &'a PowerDatabase) -> Option<&'a str> {
    db.class_name_of(state.archetype.id.as_deref()?)
}

/// Whether a `conditionalEffects` entry is a control THIS build has.
///
/// Most entries name no archetype and belong to whoever holds the power. The ones that do are
/// the pool and epic groups whose gate forks on the caster: Cross Punch, Intimidate and Invoke
/// Panic each carry a Domination bonus that exists for a Dominator and for nobody else, and
/// Mace Blast's crit branch sits under a Stalker-only parent. The entry names them because a
/// bag has one value per slot and no room to say for whom, which is AT-FORK-1's shape one level
/// up: there the atom says who an effect applies to, here the entry says who the CONTROL is for
/// (DATA-GAP-REGISTER COND-4).
///
/// An unknown class drops the entry, matching what `coh_math::gather` does with a forked atom
/// on the same build — one build state, one answer. Offering the toggle instead would let a
/// build switch on a bonus its class does not get.
pub fn conditional_for_class(entry: &Value, class_name: Option<&str>) -> bool {
    let Some(named) = entry.get("casterArchetypes").and_then(Value::as_array) else {
        return true;
    };
    let Some(class_name) = class_name else {
        return false;
    };
    named
        .iter()
        .filter_map(Value::as_str)
        .any(|token| token.eq_ignore_ascii_case(class_name))
}

fn conditional_entries(power: &Power) -> impl Iterator<Item = &Value> + '_ {
    power
        .extra
        .get("conditionalEffects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// The `conditionalEffects` a build of `class_name` has, parsed.
///
/// Every reader that answers for ONE build goes through this, so the archetype fork is resolved
/// in one place rather than at each surface (COND-4).
fn conditional_effects<'a>(
    power: &'a Power,
    class_name: Option<&'a str>,
) -> impl Iterator<Item = Conditional> + 'a {
    conditional_entries(power)
        .filter(move |entry| conditional_for_class(entry, class_name))
        .filter_map(|entry| {
            Some(Conditional {
                id: entry.get("id").and_then(Value::as_str)?.to_string(),
                label: entry
                    .get("label")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                global: entry.get("scope").and_then(Value::as_str) == Some("global"),
                default_active: entry
                    .get("defaultActive")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                group: entry
                    .get("group")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                owned_power: entry.get("ownedPower").and_then(|claim| {
                    Some((
                        claim.get("path").and_then(Value::as_str)?.to_string(),
                        claim.get("count").and_then(Value::as_f64)?,
                    ))
                }),
            })
        })
}

/// The caster-ownership states this build's ACTIVE conditional toggles assert, as
/// `(power path, count)` pairs.
///
/// A granted charge, a combo-meter stack and a lockout are not PICKS, so
/// [`crate::pick_rules::owned_power_count`] cannot answer them and
/// `coh_math::gather::owned_powers` leaves them absent — honest for a build not in that state,
/// but it also means the ownership gates reading them answer the same way for every build.
/// The toggles are the layer that knows, and this is where they say so: flipping
/// `tidal_power-3` on is the build declaring three tidal stacks, which is what selects Water
/// Jet's enhanced form.
///
/// Read off `global_conditionals` directly rather than off [`global_mechanics`], because that
/// list deliberately withholds the ids a stance selector owns — those are still live state,
/// written into the same map by [`set_stance`], and a build in Defensive Adaptation owns that
/// state whichever control set it.
pub fn active_ownership_claims(state: &CharacterState, db: &PowerDatabase) -> Vec<(String, f64)> {
    let class_name = caster_class_name(state, db);
    state
        .all_selected()
        .filter_map(|selection| db.resolve_power(&selection.powerset, &selection.internal_name))
        .flat_map(move |power| conditional_effects(power, class_name))
        .filter(|conditional| conditional.global)
        // The picks answer these in `coh_math::gather::owned_powers`, and a stale saved toggle
        // must not claim a power the build doesn't hold.
        .filter(|conditional| !conditional.answered_by_picks(db))
        .filter(|conditional| {
            state
                .combat
                .global_conditionals
                .get(&conditional.id)
                .copied()
                .unwrap_or(conditional.default_active)
        })
        .filter_map(|conditional| conditional.owned_power)
        .collect()
}

/// The modes a power publishes while it runs.
fn modes(power: &Power) -> impl Iterator<Item = &str> {
    string_array(power, "setsModes")
}

/// The modes a power carries a redirect variant for.
fn mode_variant_keys(power: &Power) -> impl Iterator<Item = &str> {
    power
        .extra
        .get("modeVariants")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(key, _)| key.as_str())
}

fn string_array<'a>(power: &'a Power, key: &str) -> impl Iterator<Item = &'a str> {
    power
        .extra
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// Fold a token to the spelling the converter's conditional ids are written in: lower case,
/// separators dropped. `Defensive_Adaptation` and `kDefensiveAdaptation`'s mode both fold onto
/// the id `defensiveadaptation`.
pub(crate) fn fold(token: &str) -> String {
    token
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

/// A wire token as a phrase: underscores become spaces, and so does each camel-case seam.
fn spaced(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 4);
    let mut previous: Option<char> = None;
    for character in token.chars() {
        if character == '_' || character == '-' {
            out.push(' ');
            previous = None;
            continue;
        }
        let seam = previous.is_some_and(|previous| {
            previous.is_ascii_lowercase() && character.is_ascii_uppercase()
        });
        if seam {
            out.push(' ');
        }
        out.push(character);
        previous = Some(character);
    }
    out
}
