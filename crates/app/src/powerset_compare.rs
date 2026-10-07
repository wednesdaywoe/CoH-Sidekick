//! Powerset compare — two powersets, role-paired, one metric at a time.
//!
//! The last of the beta's analysis surfaces. Pick an archetype and a powerset on each side and
//! the modal lists both sets' powers paired by what they DO, each pair drawn as two bars of the
//! chosen metric. Tap two rows to re-pair them by hand where the automatic pairing reads the
//! sets differently from how you do.
//!
//! # Every number is the engine's, for a powerset the build does not hold
//!
//! The beta computes its own: `calculateDamageWithATTable(scale, table, at, level)` per damage
//! entry, DPS as `damage / (cast + recharge)`. That is a second damage calculator beside the
//! real one, and the two answer differently the moment either changes.
//!
//! Here each side is projected through [`coh_math::recalculate_projecting`] against a SYNTHETIC
//! build — an empty character of the chosen archetype — with the whole powerset passed as
//! `extra` power refs. That is the same door the info tooltip already uses for a power you are
//! merely hovering, so an unheld power's numbers are computed by the engine exactly like a held
//! one's. Unslotted, because a powerset comparison is about the sets and not about anyone's
//! slotting: every figure below is the `base` tier.
//!
//! # Two inputs are READ from the live build rather than offered again
//!
//! **The level**, and **who you are hitting.** Both are already on screen — the level in the
//! header, the target in the Combat panel — and a second copy of either inside this modal is
//! two controls that can disagree about one fact. The beta ships its own 1–50 level box for
//! exactly that reason and it is the wrong trade here: changing the header's level is one click
//! away, undoable, and moves the whole app together.
//!
//! **The target is not a nicety.** An attack does not carry "a damage number" — it carries
//! several `Damage` atoms whose gates disagree about who is being hit, and the main row of
//! nearly every attack is gated on `enttype target> critter eq`. Measured on Homecoming's Fire
//! Blast, an unanswered fork left Flares and Fire Blast both reading 22.16 — the same stray DoT
//! component — against their real 66.58 and 84.73. A bar chart built on that is not slightly
//! off, it is a different ranking. So an unanswered row is COUNTED and SAID (Rule 1), never
//! folded into a total that looks complete.
//!
//! That measurement is history rather than current behaviour, and the fix is where the damage
//! is wrong to leave unstated: the PvE/PvP fork is now always answered, because which side of it
//! a build plans against is a switch with a default rather than a question. What a target still
//! withholds is its RANK, which forks 137 of Homecoming's 1120 powers (the crit tables) and
//! leaves the other 983 fully answered — so the caption names the rank rather than warning that
//! nothing resolved.
//!
//! # What a power IS comes from its atoms, not from its numbers
//!
//! Roles are classified from the export's own `powerType` and `effectArea` plus the power's
//! atom list — a `Damage` atom makes it an attack, an `EntCreate` atom a summon, a `Mez` atom
//! aimed at the target a control. Deliberately NOT from the projection: the projection depends
//! on the target, and a taxonomy that moved when you changed enemy rank would regroup the whole
//! list under you. The beta reads the transitional `effects` bag for the same job, which is the
//! one-value-per-slot shape this codebase has spent the year migrating off.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use coh_data::{CharacterState, EffectType, Power, PowerDatabase, Powerset, ToWho};
use coh_math::projection::{PowerProjection, PowerRef};
use dioxus::prelude::*;

// ============================================================
// The vocabulary the export owns.
// ============================================================

/// How a power is executed, as `powerType` spells it.
///
/// An enum rather than a string test so a fork that ships a fourth kind breaks the build here
/// instead of landing in whichever arm happens to be last (Rule 1). All three forks carry
/// exactly these; the beta additionally tests for a `Passive` kind that no fork's export has,
/// so its passive bucket can never fill.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PowerKind {
    Click,
    Toggle,
    Auto,
}

impl PowerKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "Click" => Some(PowerKind::Click),
            "Toggle" => Some(PowerKind::Toggle),
            "Auto" => Some(PowerKind::Auto),
            _ => None,
        }
    }
}

/// The geometry of what a power hits, as `effectArea` spells it. Absent on the ~40 powers per
/// fork that state no area — a real state, not a missing value, so it has no variant here and
/// rides as `None`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reach {
    SingleTarget,
    Cone,
    Aoe,
    Location,
    /// Homecoming only; the other two forks author no chain power.
    Chain,
}

impl Reach {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "SingleTarget" => Some(Reach::SingleTarget),
            "Cone" => Some(Reach::Cone),
            "AoE" => Some(Reach::Aoe),
            "Location" => Some(Reach::Location),
            "Chain" => Some(Reach::Chain),
            _ => None,
        }
    }
}

/// What a power is FOR — the axis the two sets are paired along.
///
/// The variant order is the order on screen and the order of pairing: attacks first, narrowest
/// reach first, then the things that are not attacks. This ordering is the one authored thing
/// in the module; every input that decides which variant a power lands in is read from the
/// export.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Role {
    SingleTargetAttack,
    ConeAttack,
    AoeAttack,
    LocationAttack,
    ChainAttack,
    /// An attack whose `effectArea` the export does not state.
    UnplacedAttack,
    Control,
    Summon,
    DefensiveToggle,
    OtherToggle,
    Auto,
    Support,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::SingleTargetAttack => "Single-target attacks",
            Role::ConeAttack => "Cone attacks",
            Role::AoeAttack => "AoE attacks",
            Role::LocationAttack => "Location attacks",
            Role::ChainAttack => "Chain attacks",
            Role::UnplacedAttack => "Attacks with no stated area",
            Role::Control => "Control",
            Role::Summon => "Pets and summons",
            Role::DefensiveToggle => "Defensive toggles",
            Role::OtherToggle => "Other toggles",
            Role::Auto => "Auto powers",
            Role::Support => "Buffs and utility",
        }
    }
}

/// Read a power's string field out of the transitional `extra` map.
fn extra_str<'a>(power: &'a Power, key: &str) -> Option<&'a str> {
    power.extra.get(key).and_then(serde_json::Value::as_str)
}

/// Which role a power belongs to, from its execution kind and its own atoms.
///
/// `None` when `powerType` names a kind this build does not know — surfaced as a row that says
/// so rather than swept into a default bucket, because a whole execution kind arriving unseen
/// is a data change and not a display edge case.
fn classify(power: &Power) -> Option<Role> {
    let kind = PowerKind::parse(extra_str(power, "powerType")?)?;
    let reach = extra_str(power, "effectArea").and_then(Reach::parse);

    let has = |wanted: EffectType| {
        power
            .atoms
            .iter()
            .any(|atom| atom.effect_type == Some(wanted))
    };

    if has(EffectType::Damage) {
        return Some(match reach {
            Some(Reach::SingleTarget) => Role::SingleTargetAttack,
            Some(Reach::Cone) => Role::ConeAttack,
            Some(Reach::Aoe) => Role::AoeAttack,
            Some(Reach::Location) => Role::LocationAttack,
            Some(Reach::Chain) => Role::ChainAttack,
            None => Role::UnplacedAttack,
        });
    }
    if has(EffectType::EntCreate) {
        return Some(Role::Summon);
    }
    // Aimed at the target, because a Mez atom pointed at the caster is mez PROTECTION — the
    // discriminator is `to_who`, and reading the effect type alone files every armour set's
    // status protection under Control.
    let mezzes_a_target = power.atoms.iter().any(|atom| {
        atom.effect_type == Some(EffectType::Mez) && atom.to_who == Some(ToWho::Target)
    });
    if mezzes_a_target {
        return Some(Role::Control);
    }
    Some(match kind {
        PowerKind::Auto => Role::Auto,
        PowerKind::Toggle => {
            let shields = has(EffectType::Defense)
                || has(EffectType::Resistance)
                || has(EffectType::MezResist);
            if shields {
                Role::DefensiveToggle
            } else {
                Role::OtherToggle
            }
        }
        PowerKind::Click => Role::Support,
    })
}

// ============================================================
// The metric.
// ============================================================

/// What the bars are measuring. Every one is read off [`PowerProjection`]'s `base` tier — this
/// compares powersets, not slottings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Metric {
    Damage,
    DamagePerSecondCast,
    DamagePerCycle,
    DamagePerEndurance,
    Recharge,
    Range,
    Endurance,
    CastTime,
}

impl Metric {
    pub const ALL: [Metric; 8] = [
        Metric::Damage,
        Metric::DamagePerSecondCast,
        Metric::DamagePerCycle,
        Metric::DamagePerEndurance,
        Metric::Recharge,
        Metric::Range,
        Metric::Endurance,
        Metric::CastTime,
    ];

    fn id(self) -> &'static str {
        match self {
            Metric::Damage => "damage",
            Metric::DamagePerSecondCast => "dpa",
            Metric::DamagePerCycle => "dps",
            Metric::DamagePerEndurance => "dpe",
            Metric::Recharge => "recharge",
            Metric::Range => "range",
            Metric::Endurance => "endurance",
            Metric::CastTime => "cast",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Metric::Damage => "Damage",
            Metric::DamagePerSecondCast => "Damage per second of cast",
            Metric::DamagePerCycle => "Damage per cycle second",
            Metric::DamagePerEndurance => "Damage per endurance",
            Metric::Recharge => "Recharge",
            Metric::Range => "Range",
            Metric::Endurance => "Endurance cost",
            Metric::CastTime => "Cast time",
        }
    }

    /// Whether a longer bar is a better power. Stated because the bars cannot say it: four of
    /// these are costs, and a reader who takes length for merit reads them exactly backwards.
    fn longer_is_better(self) -> bool {
        match self {
            Metric::Damage
            | Metric::DamagePerSecondCast
            | Metric::DamagePerCycle
            | Metric::DamagePerEndurance
            | Metric::Range => true,
            Metric::Recharge | Metric::Endurance | Metric::CastTime => false,
        }
    }

    fn hint(self) -> &'static str {
        if self.longer_is_better() {
            "longer bar is better"
        } else {
            "longer bar is worse — this is a cost"
        }
    }

    /// This metric for one power, or `None` when the power has no such stat. `None` is a blank
    /// row and never a zero: a toggle has no cast-time bar, which is not a cast time of zero.
    fn read(self, projection: &PowerProjection) -> Option<f64> {
        // ArcanaTime, not the authored cast: the animation lock is what a rotation actually
        // pays, and it is what `coh_math::chain` schedules against. Using the raw cast here and
        // the arcana figure in the chain modal would have the two surfaces disagree about the
        // same power.
        let cast = projection.arcana_time;
        let damage = self.damage(projection);
        match self {
            Metric::Damage => damage,
            Metric::DamagePerSecondCast => Some(damage? / positive(cast?)?),
            Metric::DamagePerCycle => {
                let recharge = projection.recharge.as_ref().map_or(0.0, |tier| tier.base);
                Some(damage? / positive(cast? + recharge)?)
            }
            Metric::DamagePerEndurance => {
                let endurance = projection.endurance_cost.as_ref()?.base;
                Some(damage? / positive(endurance)?)
            }
            Metric::Recharge => projection.recharge.as_ref().map(|tier| tier.base),
            Metric::Range => projection.range.as_ref().map(|tier| tier.base),
            Metric::Endurance => projection.endurance_cost.as_ref().map(|tier| tier.base),
            Metric::CastTime => cast,
        }
    }

    /// The power's own damage, or `None` when this context resolved none of it.
    ///
    /// The test is on the RESOLVED components, not on `PowerDamage::is_empty`, and the two
    /// differ in the case that matters: a Peacebringer's attacks carry no ungated component at
    /// all, so with no target chosen every row is unanswered and the sum over what is left is
    /// `-0.0`. Printed, that reads as "this attack does no damage" — a confident wrong answer
    /// where the honest one is "not known here" (Rule 1). A blank plus the row's `+N?` marker
    /// says the second thing.
    ///
    /// A power that deals damage only through a summoned entity (a rain patch) lands here too,
    /// with nothing unresolved: no components, no marker, no bar.
    fn damage(self, projection: &PowerProjection) -> Option<f64> {
        let damage = &projection.damage;
        if damage.components.is_empty() {
            return None;
        }
        Some(damage.base)
    }

    fn render(self, value: f64) -> String {
        match self {
            Metric::Damage | Metric::DamagePerSecondCast | Metric::DamagePerCycle => {
                format!("{value:.1}")
            }
            Metric::DamagePerEndurance => format!("{value:.2}"),
            Metric::Recharge | Metric::CastTime => format!("{value:.2}s"),
            Metric::Range => format!("{value:.0}ft"),
            Metric::Endurance => format!("{value:.2}"),
        }
    }
}

/// A divisor that is not a divisor. Zero cast, zero cycle and zero cost all mean the stat is
/// absent rather than infinitesimal, so the metric is absent too.
fn positive(value: f64) -> Option<f64> {
    (value > 0.0).then_some(value)
}

// ============================================================
// Resolving one side.
// ============================================================

/// One power on one side, with the engine's numbers for it.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub name: String,
    pub unlock_level: u8,
    role: Option<Role>,
    projection: PowerProjection,
    /// Damage rows whose gate this context could not answer — chiefly, no target chosen. The
    /// count travels with the row so the bar can say it is short rather than reading as whole.
    unresolved_damage: usize,
}

/// Everything one side of the comparison contributes.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SideView {
    pub entries: Vec<Entry>,
}

/// Project every power of `powerset` as `archetype` would cast it, unslotted.
///
/// The synthetic build carries the live build's LEVEL and COMBAT context and nothing else: the
/// numbers have to be answerable (the target gates every attack's main damage row) and they
/// have to agree with the rest of the screen, and a build's picks and slotting are exactly what
/// a powerset comparison is trying to see past.
pub fn resolve_side(
    database: &PowerDatabase,
    live: &CharacterState,
    archetype_id: &str,
    powerset_id: &str,
) -> Option<SideView> {
    let powerset: &Powerset = database.find_powerset(powerset_id)?;

    let mut synthetic = CharacterState::empty(live.dataset);
    synthetic.archetype.id = Some(archetype_id.to_string());
    synthetic.level = live.level;
    synthetic.combat = live.combat.clone();

    let requests: Vec<PowerRef> = powerset
        .powers
        .iter()
        .filter_map(|power| power.internal_name.clone())
        .map(|internal_name| PowerRef {
            powerset: powerset_id.to_string(),
            internal_name,
            targets_hit: None,
        })
        .collect();

    let totals = coh_math::recalculate_projecting(&synthetic, database, &requests);

    let mut entries: Vec<Entry> = Vec::new();
    for power in &powerset.powers {
        let Some(ident) = power.internal_name.as_deref() else {
            continue;
        };
        let Some(projection) = totals
            .power_projection
            .iter()
            .find(|p| p.power_set == powerset_id && p.power_internal_name == ident)
        else {
            continue;
        };
        entries.push(Entry {
            name: if power.name.is_empty() {
                ident.to_string()
            } else {
                power.name.clone()
            },
            unlock_level: power.unlock_level(),
            role: classify(power),
            unresolved_damage: projection.damage.unresolved.len(),
            projection: projection.clone(),
        });
    }
    Some(SideView { entries })
}

// ============================================================
// Pairing.
// ============================================================

/// Which side of the comparison a cell belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    A,
    B,
}

/// One row of the chart: a role, and the power each side contributes to it. Either half can be
/// empty — the sets do not have to hold the same number of anything.
#[derive(Clone, PartialEq, Debug)]
pub struct Pair {
    role: Option<Role>,
    a: Option<usize>,
    b: Option<usize>,
}

/// A hand-made re-pairing: exchange the occupants of two cells.
pub type Swap = ((usize, Side), (usize, Side));

/// Pair the two sides by role, then by unlock level within a role.
///
/// Level order rather than the powerset's own listing order, because the sets being compared
/// disagree about listing order and the question a reader is asking is "what does each set give
/// me at roughly the same point". A role one side has and the other does not still produces
/// rows, with the missing half blank — a set that has no cones is a finding, not a gap to hide.
fn auto_pair(a: &SideView, b: &SideView) -> Vec<Pair> {
    let indexed = |side: &SideView, role: Option<Role>| {
        let mut hits: Vec<usize> = side
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.role == role)
            .map(|(index, _)| index)
            .collect();
        hits.sort_by_key(|index| side.entries[*index].unlock_level);
        hits
    };

    // Every role either side names, in variant order, with the unclassifiable rows last.
    let mut roles: Vec<Option<Role>> = a
        .entries
        .iter()
        .chain(b.entries.iter())
        .map(|entry| entry.role)
        .collect();
    roles.sort_by_key(|role| (role.is_none(), *role));
    roles.dedup();

    let mut pairs = Vec::new();
    for role in roles {
        let left = indexed(a, role);
        let right = indexed(b, role);
        for slot in 0..left.len().max(right.len()) {
            pairs.push(Pair {
                role,
                a: left.get(slot).copied(),
                b: right.get(slot).copied(),
            });
        }
    }
    pairs
}

/// The pairing with the reader's own exchanges applied, in the order they were made.
fn paired(a: &SideView, b: &SideView, swaps: &[Swap]) -> Vec<Pair> {
    let mut pairs = auto_pair(a, b);
    for ((from_index, from_side), (to_index, to_side)) in swaps {
        if *from_index >= pairs.len() || *to_index >= pairs.len() {
            continue;
        }
        let from = cell(&pairs[*from_index], *from_side);
        let to = cell(&pairs[*to_index], *to_side);
        set_cell(&mut pairs[*from_index], *from_side, to);
        set_cell(&mut pairs[*to_index], *to_side, from);
    }
    pairs
}

fn cell(pair: &Pair, side: Side) -> Option<usize> {
    match side {
        Side::A => pair.a,
        Side::B => pair.b,
    }
}

fn set_cell(pair: &mut Pair, side: Side, value: Option<usize>) {
    match side {
        Side::A => pair.a = value,
        Side::B => pair.b = value,
    }
}

/// The largest value any bar will draw, which every bar is scaled against. `None` when nothing
/// on either side reads — the chart then draws no bars rather than dividing by zero.
fn scale_of(a: &SideView, b: &SideView, pairs: &[Pair], metric: Metric) -> Option<f64> {
    let mut max: Option<f64> = None;
    for pair in pairs {
        for value in [
            pair.a.and_then(|i| metric.read(&a.entries[i].projection)),
            pair.b.and_then(|i| metric.read(&b.entries[i].projection)),
        ]
        .into_iter()
        .flatten()
        {
            max = Some(max.map_or(value, |current: f64| current.max(value)));
        }
    }
    max.filter(|value| *value > 0.0)
}

// ============================================================
// The modal.
// ============================================================

/// The compare's open state, held at the shell root for the containment reason every modal here
/// shares: a `fixed` backdrop is contained by the grid's `transform`ed surfaces (see
/// [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct PowersetCompareOpen(pub Signal<bool>);

/// Mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn PowersetCompareHost(database: Option<Db>) -> Element {
    let mut open = use_context::<PowersetCompareOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Compare powersets".to_string(),
            // Wide: the chart is two label columns and two bar tracks abreast, and a narrow
            // card turns the bars into stubs that cannot be compared by eye.
            size: ModalSize::Xl,
            on_close: move |_| open.set(false),
            PowersetCompareBody { database }
        }
    }
}

/// One archetype's selectable powersets: its primaries, its secondaries, and the branch sets a
/// VEAT swaps in — which live in neither list and would otherwise be unreachable here.
fn powerset_options(database: &PowerDatabase, archetype_id: &str) -> Vec<(String, String)> {
    let Ok(archetypes) = database.archetypes() else {
        return Vec::new();
    };
    let Some(archetype) = archetypes.get(archetype_id) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = archetype
        .primary_sets
        .iter()
        .chain(archetype.secondary_sets.iter())
        .cloned()
        .collect();
    for branch in &archetype.branches {
        ids.extend(branch.primary_set.clone());
        ids.extend(branch.secondary_set.clone());
    }
    let mut options: Vec<(String, String)> = ids
        .into_iter()
        .map(|id| {
            let name = database
                .find_powerset(&id)
                .map(|set| set.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| id.clone());
            (id, name)
        })
        .collect();
    options.sort_by(|left, right| left.1.cmp(&right.1));
    options.dedup_by(|left, right| left.0 == right.0);
    options
}

#[component]
fn PowersetCompareBody(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();

    // The reader's choices. Side A opens on the build's own primary when it has one — the
    // comparison almost always starts from "what I am playing against what I might".
    let opening = use_hook(|| {
        let build = session.build.read();
        (build.archetype.id.clone(), build.primary.id.clone())
    });
    let mut archetype_a = use_signal(|| opening.0.clone().unwrap_or_default());
    let mut powerset_a = use_signal(|| opening.1.clone().unwrap_or_default());
    let mut archetype_b = use_signal(String::new);
    let mut powerset_b = use_signal(String::new);
    let mut metric = use_signal(|| Metric::Damage);
    // A cell awaiting its partner, and the exchanges already made. Both are cleared whenever a
    // selector moves: a swap names a row by index, and the rows are about to be different ones.
    let mut pending = use_signal(|| Option::<(usize, Side)>::None);
    let mut swaps = use_signal(Vec::<Swap>::new);

    let loading = database.is_none();
    let sides = use_memo({
        let database = database.clone();
        move || {
            let Some(database) = database.as_ref() else {
                return (None, None);
            };
            let build = session.build.read();
            let resolve = |archetype: String, powerset: String| {
                (!archetype.is_empty() && !powerset.is_empty())
                    .then(|| resolve_side(&database.0, &build, &archetype, &powerset))
                    .flatten()
            };
            (
                resolve(archetype_a(), powerset_a()),
                resolve(archetype_b(), powerset_b()),
            )
        }
    });

    // The level and the named target every bar is read against. A memo because naming the
    // target means scanning every atom's gate for the rank vocabulary — cheap once, not once
    // per render of a caption.
    let context = use_memo({
        let database = database.clone();
        move || {
            let build = session.build.read();
            // Two halves, and only one of them can be missing. The entity half always answers,
            // so the caption always names SOMETHING to read the bars against; the rank half is
            // what an unstated target withholds, and saying so beats a bare "no target".
            let ranked = build.combat.target_class.as_ref().map(|class| {
                database.as_ref().map_or_else(
                    || class.clone(),
                    |db| crate::panels::combat::target_label(&db.0, class),
                )
            });
            let named = match (ranked, build.combat.target_is_player) {
                (Some(label), true) => format!("a player ({label})"),
                (Some(label), false) => label,
                (None, true) => "a player, rank unstated".to_string(),
                (None, false) => "an enemy, rank unstated".to_string(),
            };
            (build.level, named, build.combat.target_class.is_some())
        }
    });

    if loading {
        return rsx! {
            div { class: "load-state", "Loading the dataset…" }
        };
    }
    let Some(database) = database else {
        return rsx! {};
    };

    let archetype_options: Vec<(String, String)> = database
        .0
        .archetypes()
        .map(|archetypes| {
            let mut all: Vec<(String, String)> = archetypes
                .all()
                .iter()
                .map(|archetype| (archetype.id.clone(), archetype.name.clone()))
                .collect();
            all.sort_by(|left, right| left.1.cmp(&right.1));
            all
        })
        .unwrap_or_default();

    let mut reset_pairing = move || {
        swaps.write().clear();
        pending.set(None);
    };

    rsx! {
        div { class: "ps-compare",
            div { class: "ps-compare__pickers",
                SidePicker {
                    side: Side::A,
                    archetype_options: archetype_options.clone(),
                    powerset_options: powerset_options(&database.0, &archetype_a()),
                    archetype: archetype_a(),
                    powerset: powerset_a(),
                    on_archetype: move |id: String| {
                        archetype_a.set(id);
                        powerset_a.set(String::new());
                        reset_pairing();
                    },
                    on_powerset: move |id: String| {
                        powerset_a.set(id);
                        reset_pairing();
                    },
                }
                SidePicker {
                    side: Side::B,
                    archetype_options: archetype_options.clone(),
                    powerset_options: powerset_options(&database.0, &archetype_b()),
                    archetype: archetype_b(),
                    powerset: powerset_b(),
                    on_archetype: move |id: String| {
                        archetype_b.set(id);
                        powerset_b.set(String::new());
                        reset_pairing();
                    },
                    on_powerset: move |id: String| {
                        powerset_b.set(id);
                        reset_pairing();
                    },
                }
            }

            div { class: "ps-compare__controls",
                label { class: "ps-compare__control",
                    span { class: "ps-compare__control-label", "Metric" }
                    select {
                        class: "select-compact",
                        onchange: move |evt: Event<FormData>| {
                            let chosen = evt.value();
                            if let Some(found) = Metric::ALL.iter().find(|m| m.id() == chosen) {
                                metric.set(*found);
                            }
                        },
                        for choice in Metric::ALL {
                            option {
                                key: "{choice.id()}",
                                value: "{choice.id()}",
                                selected: choice == metric(),
                                "{choice.label()}"
                            }
                        }
                    }
                }
                span { class: "ps-compare__hint", "{metric().hint()}" }
                // The level and the target are the live build's, and both are edited where they
                // live. Named here because every number below moves with them and neither
                // control is visible from inside this card.
                span { class: "ps-compare__context",
                    "Level {context().0} · "
                    span { "vs {context().1}" }
                    // A note rather than the warning this used to be. Without a rank the bars
                    // are right for every power whose damage does not fork on one, and the rows
                    // that do fork report themselves unresolved in place — so this says what
                    // picking a rank would buy, not that the chart is unreadable.
                    if !context().2 {
                        span { class: "faint", " · crit rows need a rank (pick one in Combat)" }
                    }
                }
                if !swaps.read().is_empty() {
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| reset_pairing(),
                        "Reset pairing"
                    }
                }
            }

            Chart {
                sides: sides(),
                metric: metric(),
                pending: pending(),
                swaps: swaps(),
                on_tap: move |cell: (usize, Side)| {
                    match pending() {
                        None => pending.set(Some(cell)),
                        Some(first) if first == cell => pending.set(None),
                        Some(first) => {
                            swaps.write().push((first, cell));
                            pending.set(None);
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn SidePicker(
    side: Side,
    archetype_options: Vec<(String, String)>,
    powerset_options: Vec<(String, String)>,
    archetype: String,
    powerset: String,
    on_archetype: EventHandler<String>,
    on_powerset: EventHandler<String>,
) -> Element {
    let slug = match side {
        Side::A => "a",
        Side::B => "b",
    };
    let title = match side {
        Side::A => "Set A",
        Side::B => "Set B",
    };
    rsx! {
        div { class: "ps-compare__side ps-compare__side--{slug}",
            span { class: "ps-compare__side-title", "{title}" }
            // The chosen option carries `selected` rather than the `<select>` carrying `value`.
            // A select's `value` is a PROPERTY, not an attribute, and setting it in the same
            // render that creates the options finds no option to match yet — the control comes
            // up blank while the signal behind it holds a real id. Which is exactly what Set A
            // opening on the build's own powerset looked like: right state, empty control.
            select {
                class: "select-compact",
                "aria-label": "{title} archetype",
                onchange: move |evt: Event<FormData>| on_archetype.call(evt.value()),
                option { value: "", selected: archetype.is_empty(), "Choose an archetype…" }
                for (id, name) in archetype_options.iter() {
                    option { key: "{id}", value: "{id}", selected: *id == archetype, "{name}" }
                }
            }
            select {
                class: "select-compact",
                disabled: archetype.is_empty(),
                "aria-label": "{title} powerset",
                onchange: move |evt: Event<FormData>| on_powerset.call(evt.value()),
                option { value: "", selected: powerset.is_empty(), "Choose a powerset…" }
                for (id, name) in powerset_options.iter() {
                    option { key: "{id}", value: "{id}", selected: *id == powerset, "{name}" }
                }
            }
        }
    }
}

#[component]
fn Chart(
    sides: (Option<SideView>, Option<SideView>),
    metric: Metric,
    pending: Option<(usize, Side)>,
    swaps: Vec<Swap>,
    on_tap: EventHandler<(usize, Side)>,
) -> Element {
    let (Some(a), Some(b)) = (&sides.0, &sides.1) else {
        return rsx! {
            p { class: "ps-compare__empty", "Choose a powerset on each side to compare them." }
        };
    };

    let pairs = paired(a, b, &swaps);
    let scale = scale_of(a, b, &pairs, metric);
    let unresolved: usize = pairs
        .iter()
        .flat_map(|pair| {
            [
                pair.a.map(|i| a.entries[i].unresolved_damage),
                pair.b.map(|i| b.entries[i].unresolved_damage),
            ]
        })
        .flatten()
        .sum();

    rsx! {
        div { class: "ps-compare__chart",
            if unresolved > 0 {
                p { class: "ps-compare__unresolved",
                    "{unresolved} damage rows could not be resolved against the current target, so those bars are short."
                }
            }
            if pending.is_some() {
                p { class: "ps-compare__hint", "Tap another power to pair it with the one you picked." }
            }
            // A role heading is drawn when the role changes, so the rows read as sections
            // without the pair list having to be a nested structure.
            for (index, pair) in pairs.iter().enumerate() {
                {
                    let opens_a_section = index == 0 || pairs[index - 1].role != pair.role;
                    rsx! {
                        if opens_a_section {
                            h3 { class: "ps-compare__role",
                                match pair.role {
                                    Some(role) => role.label(),
                                    None => "Unrecognized execution kind",
                                }
                            }
                        }
                        div { class: "ps-compare__pair",
                            BarRow {
                                side: Side::A,
                                index,
                                entry: pair.a.map(|i| a.entries[i].clone()),
                                metric,
                                scale,
                                selected: pending == Some((index, Side::A)),
                                on_tap,
                            }
                            BarRow {
                                side: Side::B,
                                index,
                                entry: pair.b.map(|i| b.entries[i].clone()),
                                metric,
                                scale,
                                selected: pending == Some((index, Side::B)),
                                on_tap,
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BarRow(
    side: Side,
    index: usize,
    entry: Option<Entry>,
    metric: Metric,
    scale: Option<f64>,
    selected: bool,
    on_tap: EventHandler<(usize, Side)>,
) -> Element {
    let slug = match side {
        Side::A => "a",
        Side::B => "b",
    };
    let Some(entry) = entry else {
        return rsx! {
            div { class: "ps-compare__bar ps-compare__bar--{slug} is-absent",
                span { class: "ps-compare__name", "—" }
                div { class: "ps-compare__track" }
            }
        };
    };

    let value = metric.read(&entry.projection);
    let width = match (value, scale) {
        (Some(value), Some(scale)) if value > 0.0 => (value / scale) * 100.0,
        _ => 0.0,
    };
    let class = if selected {
        format!("ps-compare__bar ps-compare__bar--{slug} is-selected")
    } else {
        format!("ps-compare__bar ps-compare__bar--{slug}")
    };

    rsx! {
        button {
            class: "{class}",
            r#type: "button",
            title: "{entry.name} — unlocks at level {entry.unlock_level}",
            onclick: move |_| on_tap.call((index, side)),
            span { class: "ps-compare__name", "{entry.name}" }
            div { class: "ps-compare__track",
                div { class: "ps-compare__fill", style: "width: {width}%;" }
                span { class: "ps-compare__value",
                    match value {
                        Some(value) => metric.render(value),
                        None => "—".to_string(),
                    }
                }
            }
            if entry.unresolved_damage > 0 {
                span { class: "ps-compare__short", "+{entry.unresolved_damage}?" }
            }
        }
    }
}
