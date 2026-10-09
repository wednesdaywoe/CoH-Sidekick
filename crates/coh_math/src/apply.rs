//! Pass 2a — apply-per-power, atom-native families. A faithful port of the
//! atom-sourced half of `applyActivePowerBonuses` (`character-totals.ts:1056`), the
//! ~1000-line loop that walks each active power and accumulates its per-family
//! contribution into [`GlobalBonuses`].
//!
//! SCOPE: every family this loop accumulates now reads its contribution through an atom
//! applier (`appliers::*`). Nothing here reads the `effects` bag.
//!
//! It used to. The beta wrote each family as `atomHelper(power) ?? effects.<slot>`, and this
//! port carried the same seam through the ATOM1-15 migrations while the atom half was still
//! filling in. The measurement that closed it is
//! `coh_math/examples/bag_removal_census.rs`, which runs the real gather and apply over every
//! power of all three forks twice, once as shipped and once with every `effects` object
//! deleted, and diffs the totals. It reports zero movers on all three at 15 archetypes each,
//! so every one of those arms was dead corpus-wide. The bag reader itself
//! (`coh_data::Bag`) is still what the `*_atom_bag_parity` guards compare against; it just
//! has no caller in the calc.
//!
//! Two of the slots were dead in a stronger sense than "the atom answered first": the flat
//! pool/epic `protection` object and `elusivity` have zero carriers on the wire, on every
//! fork. Their blocks went with the arms rather than being kept over a slot nothing writes.
//!
//! The per-family reading, fold and gate live at each block below. In brief, and with the
//! parity guard that pins each against the bag it replaced:
//!
//! - Recharge buff and its self-crash (ATOM1): `RechargeTime Str`, scale read directly.
//! - Mez resistance (ATOM2): per-type `MezResist Res` atoms, accumulate fold, knockback
//!   overwrite. Teleport carries a caster gate no other key needs.
//! - Stealth radius (ATOM3, after BRIDGE-2 typed the axes `Stealth/RadiusPvE|RadiusPvP`):
//!   feeds the COLLECT step, committed by `crate::stealth`. ATOM-BAG-7 retired its arm and
//!   the misread behind it.
//! - Perception (ATOM5) and accuracy (ATOM9): buff face, gate `aspect != Res` plus
//!   `!isDebuff`, overwrite fold. `perception_atom_bag_parity` (HC 25/25, Reb 20/20),
//!   `accuracy_atom_bag_parity` (HC 7/7, Reb 4/4, tspy 2/2). Accuracy was the first family
//!   Thunderspy fed atoms for; its `aspect != Res` gate is converter-exact rather than
//!   tightened, since Rebirth Whirlwind is `aspect=Cur`.
//! - Range (ATOM6): gate non-`Res`, non-debuff, and `toWho == Self` or `aspect == Str`.
//!   `range_atom_bag_parity` (HC 13/13, Reb 1/1). The separate `> 0` and `is_self(power)`
//!   consumer gates are unchanged.
//! - Endurance discount (ATOM7): unconditional gate, the simplest in the series, overwrite
//!   fold. `endurance_discount_atom_bag_parity` (HC 8/8, Reb 9/9). Routes to the canonical
//!   `endurance` accumulator, not the vestigial one.
//! - Max endurance (ATOM8): `endurance@maximum` bridge-folded to `MaxEndurance`, resource-sum
//!   fold, twin collapsed, self-debuffs kept and foe-facing ones dropped.
//!   `max_endurance_atom_bag_parity` (HC 6/6, Reb 3/3).
//! - Absorb (ATOM10), mez protection (ATOM11 six types plus ATOM15 KB/KU), taunt/placate
//!   resistance (ATOM12), debuff resistance (ATOM13), and the movement readers: documented at
//!   their blocks.
//!
//! The resource twins (regen, recovery) and the self `damageDebuff` crash are atom-native too;
//! Gamma Boost's HP-scaling Expression reads through `hp_scaling_resource` (PASS2B-2) rather
//! than being punted.
//! The `slow` bag slot is NOT an M3 family:
//! it writes only the movement globals (`runSpeed`/`flySpeed`/`jumpSpeed`/`jumpHeight`) via the
//! same gather-then-resolve as the travel powers, which is M4's movement-resolve pass.
//!
//! NOT a seam any more: **per-power enhancement strength** (`enhBonuses`). Each enhanced family
//! below multiplies its base by `1 + enh.get(<aspect>)` (+ any `strengthBuffs.*` term for ToHit
//! and Defense) — the per-power aggregation ([`crate::enhancement::calculate_power_enhancement_bonuses`],
//! the beta `calculatePowerEnhancementBonuses`) computed once per power at the top of the walk from
//! its `ActivePower::slots`, the dataset's IO-set catalog + curves, and the exemplar preview. The
//! sum-then-one-ED order is graded by the enhancement gate's sweep; the wiring end-to-end by the
//! slotted totals-gate builds. The unslotted synthetic corpus keeps every `enh.get(*)` at 0, so the
//! multiplier reduces to `1 + strengthBuffs` there (the port stays exact-f64). When an active Alpha
//! is equipped, [`power_enhancement`] swaps that plain aggregation for Alpha's ED-bypass split
//! ([`crate::enhancement::combine_with_alpha_ed`] / [`crate::enhancement::filter_alpha_by_allowed_enhancements`],
//! INCARNATE-1); every non-Alpha build — the whole totals-gate corpus included — keeps the plain
//! aggregation unchanged.
//!
//! DOCUMENTED SEAMS still carried here, each zero across the M3 synthetic corpus (so the port
//! stays exact-f64) and named where a later pass consumes it:
//!
//! * The **`?? effects.<slot>` bag fallback** and the bag-only sibling contributors that
//!   write into an atom-native total — the `effects.defense` pet-aura/override that takes
//!   precedence over the atom half — are step 6 (Pass 2b).
//!
//! Suppressible defense is gated on `combat.in_combat`, which the synthetic corpus does
//! exercise.
//!
//! NOT a seam any more: **stacking** landed with Pass 2b wave 15
//! ([`crate::stacking::adjust_for_stacking`]). It is driven by the PER-POWER
//! [`ActivePower::targets_hit`], not a shared combat-context foe count, and is applied to the
//! COMBINED `atomValue ?? bagValue` — the beta stacks after the `??`
//! (`character-totals.ts:1188`), so stacking only the bag half would skip every atom-native
//! power. `targets_hit: None` — what most of the corpus carries — is the identity on the linear
//! path but reads as 0 targets on the per-target one (see [`crate::stacking`]).
//!
//! Every family the beta wraps must be wrapped here too. Defense and resistance were NOT, so
//! their per-foe auras (Invincibility, Evolving Armor) ignored the count entirely; because an
//! absent count was also treated as the one-target value, they sat at a phantom first foe no
//! matter what the slider said. Both are fixed and gated per-family by the PROD6B-2d walk in
//! walk, which drives the count instead of leaving it absent.
//!
//! Absorb was the family NEITHER side wrapped (PROD6C-3j) — Parasitic Aura's shield grows with
//! the foes hit exactly as its regen/recovery siblings in the same block do, and both engine
//! and beta reported the one-foe value at every count. Fixed on both sides at once, which is
//! why that walk had to grow an absorb reader and reach Click powers to see it at all.

use crate::appliers::{
    absorb::{
        absorb_flat_value, absorb_max_hp_fraction_per_target, absorb_max_hp_fraction_value,
        states_max_hp_fraction,
    },
    accuracy::accuracy_buff_value,
    damage::{
        damage_buff_is_defiance_only, damage_buff_value, helper_damage_buff_values,
        self_damage_debuff_value,
    },
    debuff_resistance::debuff_resistance_value,
    defense::{defense_buff_suppressible_value, defense_buff_value, defense_self_debuff_value},
    endurance_discount::endurance_discount_value,
    hp_scaling_resource::hp_scaling_resource_value,
    maxhp::{max_hp_buff_unenhanced_value, max_hp_buff_value},
    mez_protection::{kb_protection_value, mez_protection_value},
    mez_resistance::mez_resistance_value,
    movement::{
        carries_combat_debuff, movement_buff_value, movement_cap_bump_value,
        self_movement_cap_debuff_value, self_slow_value,
    },
    perception::perception_buff_value,
    range::range_buff_value,
    recharge::{recharge_buff_value, recharge_self_debuff_value},
    resistance::{
        resistance_buff_value, resistance_self_debuff_resistible, resistance_self_debuff_value,
    },
    resources::{
        max_endurance_buff_value, recovery_buff_unenhanced_value, recovery_buff_value,
        regen_buff_unenhanced_value, regen_buff_value,
    },
    stealth::stealth_contribution,
    taunt_placate::taunt_placate_value,
    to_hit::{to_hit_buff_unenhanced_value, to_hit_buff_value},
    TypedValue,
};
use crate::enhancement::{
    calculate_power_enhancement_bonuses, combine_with_alpha_ed,
    filter_alpha_by_allowed_enhancements, EnhancementBonuses,
};
use crate::gather::{ActivePower, PowerSourceKind};
use crate::incarnates::AlphaEnhancement;
use crate::movement::{MovementCapContribution, MovementContribution, MovementStat};
use crate::scaled::{resolve_scaled_effect, resolve_scaled_effect_for};
use crate::stacking::{adjust_for_stacking, Half, StackFamily};
use crate::stealth::StealthContribution;
use crate::strength::StrengthBuffs;
use crate::totals::{route_closed, CalcError, GlobalBonuses, TypeRoute};
use coh_data::slot_value::Scaled;
use coh_data::{
    CombatContext, EffectType, Enhancement, Level, Power, PowerDatabase, SubType, TableScope,
};

/// The beta's per-power defense enhancement term `enhBonuses.defense || enhBonuses.defenseBuff`:
/// the `defenseBuff` fraction only when `defense` is zero (JS truthiness). Shared by the
/// Defense, Elusivity, and defense-debuff-resistance seams.
pub(crate) fn defense_enh(enh: &EnhancementBonuses) -> f64 {
    let defense = enh.get("defense");
    if defense != 0.0 {
        defense
    } else {
        enh.get("defenseBuff")
    }
}

/// maxHP atoms convert their raw `scale` to a percentage with a ×10 factor —
/// the beta's maxHP-specific convention, unlike the ×100 the other Pass-2a
/// families use (`max_hp` is stored as a percentage; see [`crate::totals`]).
const MAX_HP_SCALE_TO_PERCENT: f64 = 10.0;

/// Turn a per-type router verdict on a DATA-fed key into an error when nothing claimed it.
/// [`TypeRoute::Unspent`] is a declared non-contribution and stays quiet — the declaration and
/// its evidence live at the router. [`TypeRoute::Unknown`] is a type key the export grew and
/// this calc cannot place: Rule 1 wants that visible, never a number that silently never
/// arrives.
fn surface_route(
    route: TypeRoute,
    family: &str,
    type_key: &str,
    source: &str,
    errors: &mut Vec<CalcError>,
) {
    if route == TypeRoute::Unknown {
        errors.push(CalcError::new(
            family,
            format!("{source}: unroutable {family} type {type_key:?}"),
        ));
    }
}

/// The curated-armor mez-protection slots, each paired with the mez type it routes to
/// (`add_mez_protection`). Ports the beta's `mezProtTypes` (`character-totals.ts:1810`):
/// `knockup` routes to `knockback` — the same physical stat — and the apply loop folds a
/// single power's Knockback/Knockup pair to `max` before accumulating.
///
/// `repel` keeps its OWN key rather than joining that fold. It reads through the same
/// [`kb_protection_value`] accumulate fold (the converter files all three under
/// `KNOCKBACK_TYPES`), but it is a separate stat: Thunderspy's Power Surge Brute records
/// `protKnockback` 100 against `protRepel` 10, so folding the pair would overstate knockback
/// protection by the repel magnitude. That much IS graded — folding them reddens all five of the
/// frozen builds that state `protRepel`, as does dropping this entry. Added after
/// `totals_replay.rs` graded its absence as a declared drop on those five.
///
/// Repel counts as a self-KB atom below, so its non-`Res_Boolean` magnitude takes the Knockback
/// enhancement like knockback's does. UNGRADED: all five builds are unslotted on the repel
/// carrier, so the multiplier is ×1.0 on every record that reaches it. Uniform with the other two
/// because the converter's KB branch makes no distinction between them.
const MEZ_PROT_TYPES: [(&str, &str); 9] = [
    ("hold", "hold"),
    ("stun", "stun"),
    ("immobilize", "immobilize"),
    ("sleep", "sleep"),
    ("confuse", "confuse"),
    ("fear", "fear"),
    ("knockback", "knockback"),
    ("knockup", "knockback"),
    ("repel", "repel"),
];

/// Ally-only target types the caster never benefits from (Speed Boost, Fortitude —
/// "you cannot use this power on yourself"). Ported from `ALLY_ONLY_TARGET_TYPES`
/// (`character-totals.ts:737`); compared case-insensitively against `Power.targetType`.
const ALLY_ONLY_TARGET_TYPES: [&str; 7] = [
    "ally",
    "ally (alive)",
    "teammate",
    "dead teammate",
    "friend",
    "deadplayerfriend",
    "deadoraliveleaguemate",
];

fn target_type(p: &Power) -> Option<&str> {
    p.extra.get("targetType").and_then(|v| v.as_str())
}

fn is_ally_only(p: &Power) -> bool {
    target_type(p).is_some_and(|t| {
        let t = t.to_ascii_lowercase();
        ALLY_ONLY_TARGET_TYPES.contains(&t.as_str())
    })
}

/// A Self-targeted power — the gate the beta's `rangeBuff` block applies
/// (`power.targetType?.toLowerCase() === 'self'`, `character-totals.ts:1989`). A power with no
/// `targetType` is NOT Self (the beta's `?.` short-circuits to `undefined !== 'self'`).
fn is_self(p: &Power) -> bool {
    target_type(p).is_some_and(|t| t.eq_ignore_ascii_case("self"))
}

/// The per-power enhancement multiplier source — the beta's per-power `enhBonuses` selection
/// (`character-totals.ts:1098`). With NO active Alpha it is the plain slotted aggregation
/// ([`calculate_power_enhancement_bonuses`]), the unchanged path every non-Alpha build takes.
/// With an active Alpha it is the ED split: a slotted power (a non-empty slot array) goes through
/// [`combine_with_alpha_ed`] (Alpha's ED-subject slice joins the raw IO sum before the single ED
/// pass, its bypass slice lands after); a slot-less power goes through
/// [`filter_alpha_by_allowed_enhancements`] (Alpha is then the only enhancement source, added
/// without an ED pass) — mirroring the beta's `power.slots?.length > 0` branch. `allowed` is the
/// power's `allowed_enhancements` gate (absent ⇒ every aspect). Fail-loud when a slotted power has
/// no enhancement curves (Rule 1); real datasets always carry them (SW6).
///
/// Takes the pick's `slots` directly (not the whole [`ActivePower`]) so the perma ring can reuse
/// the exact same per-power enhancement the aggregate uses — one source for "the recharge this
/// power's slotting grants," Alpha split included, so the ring and the totals can never disagree.
pub fn power_enhancement(
    slots: &[Option<Enhancement>],
    power: &Power,
    alpha: &AlphaEnhancement,
    level: i32,
    combat: &CombatContext,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> EnhancementBonuses {
    let exemplar = combat.exemplar_level;
    let allowed = power.allowed_enhancements.as_deref();
    let no_curves_error = || {
        CalcError::new(
            "enhancement",
            format!(
                "{} is slotted but this dataset loaded no enhancement curves — its enhancement is unmodeled",
                power.name
            ),
        )
    };
    // A slot-less power's only enhancement is Alpha, added without an ED split —
    // and it is the one path that never reads the build level, so it answers
    // before the level is resolved.
    if alpha.active && slots.is_empty() {
        return filter_alpha_by_allowed_enhancements(&alpha.bonuses, allowed);
    }
    let Some(global_io_level) = Level::from_i64(i64::from(level)) else {
        errors.push(CalcError::new(
            "enhancement",
            format!(
                "{} enhanced at level {level}, which is not a level",
                power.name
            ),
        ));
        return EnhancementBonuses::default();
    };
    if alpha.active {
        return match db.enhancement_curves.as_ref() {
            Some(curves) => combine_with_alpha_ed(
                slots,
                allowed,
                global_io_level,
                db.io_sets.as_ref(),
                &alpha.bonuses,
                &alpha.ed_bypass,
                exemplar,
                curves,
                errors,
            ),
            None => {
                errors.push(no_curves_error());
                // No curves ⇒ no ED split possible; Alpha still applies undiminished.
                filter_alpha_by_allowed_enhancements(&alpha.bonuses, allowed)
            }
        };
    }
    match db.enhancement_curves.as_ref() {
        Some(curves) => calculate_power_enhancement_bonuses(
            slots,
            global_io_level,
            db.io_sets.as_ref(),
            exemplar,
            curves,
            errors,
        ),
        None => {
            if slots.iter().any(Option::is_some) {
                errors.push(no_curves_error());
            }
            EnhancementBonuses::default()
        }
    }
}

/// Pass 2a. Walk each already-gathered active power (Pass 0 handled auto/toggle,
/// stance expansion and mode suppression) and add its atom-native family contributions
/// to `g`. `strength` is Pass 1's `StrengthBuffs`; only ToHit and Defense receive it in
/// this family set (the beta multiplies `damage`/`resistance`/`regen`/`recovery`/`maxHP`
/// by enhancement strength alone — none of them by a `strengthBuffs.*` term).
/// `stealth_contributions` is an OUT-parameter, not a total: stealth's suppress groups make a
/// per-power value meaningless in isolation, so the walk collects and
/// [`crate::stealth::resolve_stealth_radius`] commits. It accumulates across calls because the
/// beta resolves once over every source (`character-totals.ts:4294`). `movement_contribs` is the
/// same kind of OUT-parameter collector for the travel buffs (suppress-group max + additive), committed
/// by [`crate::movement::resolve_movement_totals`]; self-directed `slow` debuffs add to `g` inline
/// (no suppress group). `alpha` is the equipped
/// Alpha incarnate's virtual-enhancement inputs (Pass 6's [`crate::incarnates::alpha_enhancement`]);
/// when it is active each power's enhancement runs through the ED split ([`power_enhancement`]).
// The signature mirrors the beta's wide `applyActivePowerBonuses` calc inputs (archetype, level,
// strength, combat context, Alpha inputs, the stealth collector, the db) — kept as explicit
// parameters so each maps 1:1 to the ported source rather than hiding behind a context struct.
/// One deferred self-directed −Resistance penalty, collected during the per-power walk and
/// resolved by [`resolve_res_self_debuffs`] once every resistance source has summed.
///
/// Carries its own identity for the same reason [`AbsorbFractionContribution`] does: the resolve
/// runs outside the walk, so the snapshot bracket that attributes every other family has closed
/// by then and the penalty would reach the total with no row explaining it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResSelfDebuff {
    /// Lowercase damage type (`"smashing"`).
    pub damage_type: String,
    /// The penalty as it is DISPLAYED — already negative.
    pub nominal: f64,
    /// Whether the caster's own resistance to this type mitigates it.
    pub resistible: bool,
    pub power_internal_name: String,
    pub power_set: String,
    pub kind: PowerSourceKind,
}

/// Apply the deferred self −Resistance penalties (beta Step 9.3).
///
/// CoH reduces a resistible −Res by the caster's own resistance to that type:
/// `effective = nominal × (1 − R)`, while the displayed magnitude stays nominal. R is
/// snapshotted per type BEFORE any penalty lands, so several penalties of one type all resist
/// against the same pre-debuff total rather than cascading in application order. The factor is
/// clamped at 0 so an over-100% raw resistance cannot flip a penalty into a buff.
pub fn resolve_res_self_debuffs(
    debuffs: &[ResSelfDebuff],
    g: &mut GlobalBonuses,
    power_breakdown: &mut Vec<PowerBreakdownSource>,
    errors: &mut Vec<CalcError>,
) {
    if debuffs.is_empty() {
        return;
    }
    let mut snapshot: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    for d in debuffs {
        snapshot
            .entry(d.damage_type.as_str())
            .or_insert_with(|| g.resistance_of(&d.damage_type).unwrap_or(0.0));
    }
    for d in debuffs {
        let r = snapshot.get(d.damage_type.as_str()).copied().unwrap_or(0.0);
        let factor = if d.resistible {
            (1.0 - r / 100.0).max(0.0)
        } else {
            1.0
        };
        // Bracketed like the walk's own attribution rather than filed from `nominal`: what
        // reaches the total is the MITIGATED penalty, and a fully-resisted one moves nothing at
        // all. Measuring the accumulator is the only description that cannot drift from the
        // arithmetic, and `deltas_since` drops the no-op case for free.
        let before = g.clone();
        surface_route(
            g.add_resistance(&d.damage_type, d.nominal * factor),
            "Resistance",
            &d.damage_type,
            "self −Resistance penalty",
            errors,
        );
        PowerBreakdownSource::record_deltas(
            &before,
            g,
            &d.power_internal_name,
            &d.power_set,
            d.kind,
            power_breakdown,
        );
    }
}

/// One power's effect on one breakdown key — the rows the detailed-totals panel groups by
/// source and sums. Deliberately a MEASURED delta rather than a value each applier reports: the
/// apply pass writes `GlobalBonuses` from a dozen family appliers with their own folds, caps and
/// self-penalty resolves, and asking each to also describe itself would be a second description
/// of the same arithmetic — free to drift from the first. Diffing the accumulator around one
/// power cannot drift, because it IS the arithmetic ([`GlobalBonuses::deltas_since`]).
///
/// Every contributor addressed as a power files rows here, whichever pass applied it: the apply
/// walk's active powers and accolades, and Pass 3's archetype inherents
/// ([`crate::inherents::apply_archetype_inherents`], whose Vigilance / Rage_Buff are extracted
/// power defs in the `Inherent` set like any other). The incarnate loadout is NOT a power — it
/// carries no powerset and is addressed by slot — so Pass 6 files
/// [`crate::incarnates::IncarnateBreakdownSource`] instead.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PowerBreakdownSource {
    /// The beta camelCase field name ([`GlobalBonuses::BREAKDOWN_KEYS`]).
    pub breakdown_key: String,
    pub power_internal_name: String,
    pub power_set: String,
    /// How much this contributor moved that field. Signed — a self-penalty (Granite's −Recharge)
    /// is a contribution like any other and must read as the negative it is.
    pub value: f64,
    pub kind: PowerSourceKind,
}

impl PowerBreakdownSource {
    /// Attribute everything `g` moved since `before` to one power, as one row per touched key.
    /// The recorder every pass that applies a power shares, so a contributor cannot be described
    /// one way here and another way there.
    pub fn record_deltas(
        before: &GlobalBonuses,
        g: &GlobalBonuses,
        power_internal_name: &str,
        power_set: &str,
        kind: PowerSourceKind,
        out: &mut Vec<Self>,
    ) {
        out.extend(g.deltas_since(before).into_iter().map(|(key, value)| Self {
            breakdown_key: key.to_string(),
            power_internal_name: power_internal_name.to_string(),
            power_set: power_set.to_string(),
            value,
            kind,
        }));
    }
}

/// One MaxHP-FRACTION absorb source, gathered during the apply walk and resolved to absolute HP
/// at Step 9.2 once every +MaxHP source has summed.
///
/// Carries its own identity because the resolve happens outside the walk, where the snapshot
/// bracket that attributes every other family is long closed. Without it the fraction half of
/// absorb reaches the total with no row explaining it — which on a Wild Bastion build is most of
/// the number.
#[derive(Debug, Clone, PartialEq)]
pub struct AbsorbFractionContribution {
    /// A fraction of final Max HP, post-enhancement. Resolved to HP by Step 9.2.
    pub fraction: f64,
    pub power_internal_name: String,
    pub power_set: String,
    pub kind: PowerSourceKind,
}

/// Attribute everything `g` gained since `before` to `active`.
fn record_power_deltas(
    active: &ActivePower,
    before: &GlobalBonuses,
    g: &GlobalBonuses,
    out: &mut Vec<PowerBreakdownSource>,
) {
    PowerBreakdownSource::record_deltas(
        before,
        g,
        active.def.ident(),
        active.power_set,
        active.kind,
        out,
    );
}

/// Per-iteration state for [`apply_active_power_bonuses`], gathered once per power so each
/// family method takes `&self` rather than the fourteen identifiers the original body bound
/// inline. The family methods are literal copies of the original per-family blocks (block order
/// preserved exactly for f64 parity); each is a literal copy of the original body with the
/// per-power identifiers bound to `self`.
struct WalkCtx<'a, 'b> {
    power: &'a Power,
    active: &'b ActivePower<'a>,
    archetype: &'a str,
    level: i32,
    db: &'a PowerDatabase,
    errors: &'a mut Vec<CalcError>,
    enh: &'a EnhancementBonuses,
    strength: &'a StrengthBuffs,
    combat: &'a CombatContext,
    targets_hit: Option<u32>,
}

impl<'a, 'b> WalkCtx<'a, 'b> {
    fn new(
        power: &'a Power,
        active: &'b ActivePower<'a>,
        archetype: &'a str,
        level: i32,
        db: &'a PowerDatabase,
        errors: &'a mut Vec<CalcError>,
        enh: &'a EnhancementBonuses,
        strength: &'a StrengthBuffs,
        combat: &'a CombatContext,
        targets_hit: Option<u32>,
    ) -> Self {
        Self {
            power,
            active,
            archetype,
            level,
            db,
            errors,
            enh,
            strength,
            combat,
            targets_hit,
        }
    }

    /// `typed_to_scaled`: the beta wraps each stackable bag value in `adjustForStacking` BEFORE
    /// resolving it (`character-totals.ts:1139-1990`).
    fn to_scaled(&self, tv: TypedValue) -> Scaled {
        Scaled {
            scale: tv.scale,
            table: tv.table.map(|t| t.to_string()),
            per_target: tv.per_target,
        }
    }

    /// `stack`: the [`StackFamily`] over the power's OWN atoms, not a bag slot name (ATOM-BAG-2).
    /// `Some(cap)` is the `stacksLinear` membership and the `stackCaps[key] ?? maxStacks` depth at
    /// once, read off the atoms that already carry them.
    ///
    /// The last argument answers whether an absent targets-hit count means zero targets or one.
    /// It is one where the power cannot be at zero: the caster fills one of his own AoE seats, so
    /// the untouched slider must not delete a base the game gives with nobody else in radius
    /// (PERFOE-3), or the power is aimed at an entity the game insists on before it will fire, so
    /// a build that says it uses the power is saying it reached somebody (PERFOE-4).
    fn stack(&self, value: Scaled, family: StackFamily) -> Scaled {
        let cap = family.cap(self.power);
        let count = crate::stacking::target_count(self.power);
        adjust_for_stacking(&value, self.targets_hit, cap.is_some(), cap, count)
    }

    /// Resolve a [`Scaled`] through the AT table like the beta does (`resolve_scaled_effect ×` the
    /// family's multiplier). Shared by the percentage families.
    fn resolve(&mut self, scaled: &Scaled) -> f64 {
        resolve_scaled_effect(
            scaled.scale,
            scaled.table.as_deref(),
            self.archetype,
            self.level,
            self.db,
            self.errors,
        )
    }

    /// Resolve a [`TypedValue`] through stack then the AT table (`stacked_percent`): the way the
    /// beta wraps each entry before `resolveScaledEffect` (`character-totals.ts:1249/1272/1297`),
    /// used by the per-foe defense/resistance families that grow with the targets-hit count.
    fn stacked_percent(&mut self, tv: &TypedValue, family: StackFamily) -> f64 {
        let s = self.stack(self.to_scaled(tv.clone()), family);
        self.resolve(&s) * 100.0
    }

    /// STACK-4: stack a per-ROW value to the depth its OWN sub type reaches, then resolve.
    /// A key that names no sub type is unrecognized data (Rule 1): surface it and land the row at
    /// its base value (`× multiplier`). Shared by defense and resistance per-type loops.
    fn per_type_stacked_percent(
        &mut self,
        tv: &TypedValue,
        effect_type: EffectType,
        key: &str,
        multiplier: f64,
    ) -> f64 {
        match SubType::from_wire_lower(key) {
            Some(sub) => {
                let family = StackFamily::Buff(effect_type, Half::Either, Some(sub));
                self.stacked_percent(tv, family) * multiplier
            }
            None => {
                self.errors.push(CalcError::new(
                    format!("{} {key}", self.power.name),
                    format!("a {effect_type:?} total row's key names no sub type"),
                ));
                resolve_scaled_effect(
                    tv.scale,
                    tv.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * 100.0
                    * multiplier
            }
        }
    }

    /// Resolve a value that ignores the AT table entirely — the recharge buff reads its `scale`
    /// directly (`× 100`, no table) because a recharge buff carries the final fraction on a
    /// `*_Ones` table (`character-totals.ts:1188`).
    #[allow(dead_code)]
    fn resolve_direct(&mut self, scaled: &Scaled) -> f64 {
        scaled.scale * 100.0
    }

    fn apply_to_hit(&mut self, g: &mut GlobalBonuses) {
        // ToHit buff — atom-native; the `?? effects.tohitBuff` bag fallback (lowercase `h`) is
        // a corpus-vacuous seam (parallel maxHP, unit-test-proven only). The atom helper filters
        // `aspect:Res` toHit (a toHit-debuff-resistance → debuffResistTohit, not a buff), and
        // every real +ToHit buff carries a non-Res atom, so no corpus power falls to the bag —
        // Combat Training: Offensive did until PASS2B-5 retired its stale `effects.tohitBuff`
        // override. Enhanced by ToHit enhancement (`enh.tohit`, corpus-zero seam) AND global
        // +ToHit strength.
        let to_hit_slot: Option<Scaled> = to_hit_buff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| {
                self.stack(
                    s,
                    StackFamily::Buff(EffectType::ToHit, Half::Enhanceable, None),
                )
            });
        if let Some(s) = to_hit_slot {
            let enh_multiplier = 1.0 + self.enh.get("tohit") + self.strength.to_hit;
            g.to_hit += resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0
                * enh_multiplier;
        }
        // ToHit buff that ignores strength (IgnoreStrength) — no enh, no +ToHit. Same bag
        // fallback (`?? effects.tohitBuffUnenhanced`).
        let to_hit_unenhanced: Option<Scaled> = to_hit_buff_unenhanced_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| {
                self.stack(
                    s,
                    StackFamily::Buff(EffectType::ToHit, Half::Unenhanced, None),
                )
            });
        if let Some(s) = to_hit_unenhanced {
            g.to_hit += resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
        }
    }

    fn apply_damage(&mut self, g: &mut GlobalBonuses) {
        // Damage buff (Assault, Build Up, Fulcrum Shift) — atom-native, else the bag
        // `?? effects.damageBuff` for an atom-less legacy power. NOT enhanced — there is no
        // "+Damage buff" enhancement in CoH.
        //
        // A power whose whole +damage buff is Defiance takes NEITHER path. The atom read
        // already rejects the tagged atoms, but the bag slot holds the same value and an
        // empty atom read is indistinguishable from the atom-less legacy case the fallback
        // serves — so the rejection has to be spoken here too or it is undone one line later
        // (`damage_buff_is_defiance_only`).
        //
        // A spawned helper's share is its own value, read under the helper's class
        // (`AtomicEffect::pet_class`): Rebirth's Fulcrum Shift is nothing else. One value is
        // stacked then resolved, as always. Several — Thunderspy's Fulcrum Shift, whose +5 base
        // runs as the player and whose 1.6 per foe runs as `minion_pets` — are each resolved
        // through their own table first and stacked as one, because the per-foe count belongs
        // to the power and a second cast brings the base along whichever class reads it. The
        // stacking is linear in both scale and increment, so the order changes no number.
        let family = StackFamily::Buff(EffectType::DamageBuff, Half::Either, None);
        let mut parts: Vec<(TableScope<'_>, Scaled)> = Vec::new();
        if !damage_buff_is_defiance_only(self.power) {
            if let Some(value) = damage_buff_value(self.power) {
                parts.push((TableScope::Archetype(self.archetype), self.to_scaled(value)));
            }
        }
        for (class, value) in helper_damage_buff_values(self.power) {
            parts.push((TableScope::Pet(class), self.to_scaled(value)));
        }
        let damage_buff_percent = match parts.as_slice() {
            [] => None,
            [(scope, value)] => {
                let s = self.stack(value.clone(), family);
                Some(
                    resolve_scaled_effect_for(
                        s.scale,
                        s.table.as_deref(),
                        *scope,
                        self.level,
                        self.db,
                        self.errors,
                    ) * 100.0,
                )
            }
            _ => {
                let mut combined = Scaled {
                    scale: 0.0,
                    table: None,
                    per_target: None,
                };
                for (scope, value) in &parts {
                    let rate = resolve_scaled_effect_for(
                        1.0,
                        value.table.as_deref(),
                        *scope,
                        self.level,
                        self.db,
                        self.errors,
                    );
                    combined.scale += value.scale * rate;
                    if let Some(increment) = value.per_target.filter(|p| *p != 0.0) {
                        *combined.per_target.get_or_insert(0.0) += increment * rate;
                    }
                }
                Some(self.stack(combined, family).scale * 100.0)
            }
        };
        if let Some(percent) = damage_buff_percent {
            g.damage += percent;
        }
        // Self-directed `damageDebuff` crash (Granite Armor −30%, Bio Defensive Adaptation
        // −25%). Applied ONLY when the power has no damage BUFF: a co-present buff means the
        // debuff is a transient crash effect (Rage) and must not count as sustained damage.
        // Atom-native, `?? bag` for the atom-less residue; both halves already gate on
        // `toWho:Self`. Stored as a positive magnitude, subtracts here (`× -100`). Unenhanceable.
        if let Some(s) = self_damage_debuff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .filter(|_| damage_buff_percent.is_none())
        {
            g.damage -= resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
        }

        // Resolve a per-type value through the stacking transform first, the way the beta wraps
        // each entry before `resolveScaledEffect` (`character-totals.ts:1249/1272/1297`). The
        // per-foe families route through here: Invincibility's +Def and Evolving Armor's +Res
        // both grow with the targets-hit count, and skipping the wrap credited their full
        // one-target value no matter what the slider said (PROD6B-2d).
    }

    fn apply_defense(&mut self, g: &mut GlobalBonuses) {
        // Defense — always-on, enhanced by Defense enhancement + global +Defense strength.
        // `effects.defense` pet-aura precedence is a step-6 bag concern. A team-only buff
        // (Grant Cover) is already gone: the applier drops the caster-excluded rows.
        if let Some(entries) = defense_buff_value(self.power) {
            let enh_multiplier = 1.0 + defense_enh(self.enh) + self.strength.defense;
            for (ty, tv) in &entries {
                // STACK-4: each position stacks to its OWN sub type's depth. Rebirth's
                // Burnout self-stacks Melee / AoE / Lethal / Cold and `Replace`s the other
                // six, and one family cap used to double all ten at the slider.
                let percent =
                    self.per_type_stacked_percent(tv, EffectType::Defense, ty, enh_multiplier);
                surface_route(
                    g.add_defense(ty, percent),
                    "Defense",
                    ty,
                    &self.power.name,
                    self.errors,
                );
            }
        }
        // Suppressible defense (Hide, Stealth, Cloaking Device) — applied only out of
        // combat. Same enhancement + strength multiplier as the always-on half.
        if let Some(entries) =
            defense_buff_suppressible_value(self.power).filter(|_| !self.combat.in_combat)
        {
            let enh_multiplier = 1.0 + defense_enh(self.enh) + self.strength.defense;
            for (ty, tv) in &entries {
                let percent =
                    self.per_type_stacked_percent(tv, EffectType::Defense, ty, enh_multiplier);
                surface_route(
                    g.add_defense(ty, percent),
                    "Defense",
                    ty,
                    &self.power.name,
                    self.errors,
                );
            }
        }
        // Self-directed −Defense penalty (Thunderspy Organic Armor's Defensive Adaptation,
        // the two accolades) — the sibling the defense family lacked, so a power that
        // negates its own caster's defense reached no total at all (DEFDEBUFF-1). The
        // applier already restricts to self-directed, undelayed atoms and returns
        // `|scale|`; negate here. Unenhanced, for the reason the applier gives.
        //
        // Applied inline, unlike the resistance twin: −Res is mitigated by the caster's own
        // resistance and so has to wait for the completed totals, while a −Def has no such
        // mitigation — the game clamps it against the class floor instead, and `finalize`
        // does that after every contributor is in. Saturating magnitudes are the norm in
        // this family (Organic Armor states −500, which resolves to −5000% on a Tanker), so
        // the floor is what makes the number defined; it is not arithmetic here.
        //
        // NOT wrapped in `stack`, matching the resistance twin and the two damage/recharge
        // crashes: the beta has no `adjustForStacking` call site for a self-penalty, and
        // there is no bag slot behind this one whose `stackCaps` entry could answer.
        if let Some(entries) = defense_self_debuff_value(self.power) {
            for (ty, tv) in &entries {
                let percent = resolve_scaled_effect(
                    tv.scale,
                    tv.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * 100.0;
                surface_route(
                    g.add_defense(ty, -percent),
                    "Defense",
                    ty,
                    &self.power.name,
                    self.errors,
                );
            }
        }
    }

    fn apply_resistance(
        &mut self,
        g: &mut GlobalBonuses,
        res_self_debuffs: &mut Vec<ResSelfDebuff>,
    ) {
        // Resistance — enhanced by Resistance enhancement (no strength term exists).
        if let Some(entries) = resistance_buff_value(self.power) {
            let enh_multiplier = 1.0 + self.enh.get("resistance");
            for (ty, tv) in &entries {
                let percent =
                    self.per_type_stacked_percent(tv, EffectType::Resistance, ty, enh_multiplier);
                surface_route(
                    g.add_resistance(ty, percent),
                    "Resistance",
                    ty,
                    &self.power.name,
                    self.errors,
                );
            }
        }
        // Self-directed −Resistance penalty (Bio Armor Offensive Adaptation). The applier
        // already restricts to self-directed atoms and returns `|scale|`; negate here.
        // Unenhanced.
        //
        // COLLECTED, not applied: CoH mitigates a resistible −Res by the caster's own
        // resistance to that type (`effective = nominal × (1 − R)`), and R must include every
        // source — powers, set bonuses, procs, accolades, incarnates — so it cannot be known
        // during this walk. `resolve_res_self_debuffs` applies them once the totals are
        // complete (beta Step 9.3). Applying inline charged the full nominal, which read as a
        // bigger resistance loss than the game shows.
        if let Some(entries) = resistance_self_debuff_value(self.power) {
            let resistible = resistance_self_debuff_resistible(self.power);
            for (ty, tv) in &entries {
                // The beta's `resolveScaledEffect(...) * 100 * -1` — the self penalty is
                // stored as a positive magnitude and subtracts here.
                let nominal = -(resolve_scaled_effect(
                    tv.scale,
                    tv.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * 100.0);
                res_self_debuffs.push(ResSelfDebuff {
                    damage_type: ty.clone(),
                    nominal,
                    resistible: resistible
                        .iter()
                        .find(|(t, _)| t == ty)
                        .is_none_or(|(_, r)| *r),
                    power_internal_name: self.power.ident().to_string(),
                    power_set: self.active.power_set.to_string(),
                    kind: self.active.kind,
                });
            }
        }
    }

    fn apply_resources(&mut self, g: &mut GlobalBonuses) {
        // Regeneration — enhanced by Healing enhancement + +Healing strength (the beta's
        // `global.healOther` set-bonus accumulator, 0 in the M3 corpus). Atom-native, with NO bag
        // fallback: ATOM-BAG-5 measured the `?? effects.regenBuff` / `?? effects.regenBuffUnenhanced`
        // arms over all three forks and they answered for nothing but phantoms — Disrupting
        // Torrent's foe -Regen read as +100% regen on the caster, and after the reader learned to
        // ask who an atom lands on, Temporal Bomb's and Rally The Militia's would have fallen
        // through to the same arm. The bag routes resources on the aspect and the sign of `scale`
        // and never asks the recipient, so it is the wrong side of this seam by construction; the
        // atom-less powers the fallback was kept for (Rest, Stamina) went atom-native with
        // ATOM-BAG-6. The enhanceable and IgnoreStrength halves
        // co-apply; a `Res_Boolean` table on the enhanceable half is regen-debuff-resistance,
        // not a regen buff, and skips BOTH halves (the beta's `if/else if` — the twin only
        // stands alone when the enhanceable half is absent). Each half falls back independently.
        // The enhanceable half is multiplied by `1 + enh.get("heal")` (Healing enhancement); the
        // `+ healOther/100` set-bonus term stays a seam (set bonuses are not modeled yet, 0 here).
        //
        // The HP-scaling regen Expression (Gamma Boost) is evaluated atom-native from
        // `magnitude_expression` given `combat.hit_points_percent`. It is the FINAL enhanceable
        // magnitude (already × @StdResult), so it is never re-run through `resolve_scaled_effect`
        // (PASS2B-2), and it ADMITS ITSELF: `regen_buff_value` punts on Expression atoms by
        // design, so until ATOM-BAG-5 this half ran only when the bag carried a `regenBuff`
        // slot. The bag was a pure admission ticket there — its scale never reached the total
        // and its table only fed the `Res_Boolean` guard below — so deleting the slot computed
        // Gamma Boost's magnitude and threw it away.
        let atom_regen_expression = hp_scaling_resource_value(
            self.power,
            coh_data::EffectType::Regeneration,
            self.archetype,
            self.level,
            self.combat,
            self.db,
            self.errors,
        );
        let regen_slot: Option<(f64, Option<String>)> = regen_buff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| {
                self.stack(
                    s,
                    StackFamily::Buff(EffectType::Regeneration, Half::Enhanceable, None),
                )
            })
            .map(|s| (s.scale, s.table));
        let regen_unenhanced: Option<(f64, Option<String>)> =
            regen_buff_unenhanced_value(self.power)
                .map(|__tbs| self.to_scaled(__tbs))
                .map(|s| {
                    self.stack(
                        s,
                        StackFamily::Buff(EffectType::Regeneration, Half::Unenhanced, None),
                    )
                })
                .map(|s| (s.scale, s.table));
        // The enhanceable half as (magnitude, the table its `Res_Boolean` guard reads). The
        // guard keeps reading the SLOT's table wherever there is one — that is the table it has
        // always graded — and falls back to the Expression atom's own where the slot is gone.
        let regen_enhanceable: Option<(f64, Option<String>)> = match atom_regen_expression {
            Some(expr) => Some((
                expr.value,
                regen_slot
                    .as_ref()
                    .and_then(|(_, t)| t.clone())
                    .or(expr.table),
            )),
            None => regen_slot.as_ref().map(|(scale, table)| {
                (
                    resolve_scaled_effect(
                        *scale,
                        table.as_deref(),
                        self.archetype,
                        self.level,
                        self.db,
                        self.errors,
                    ),
                    table.clone(),
                )
            }),
        };
        if let Some((magnitude, table)) = &regen_enhanceable {
            if !table_is_res_boolean(table.as_deref()) {
                let value = magnitude * 100.0 * (1.0 + self.enh.get("heal"));
                let unenh = regen_unenhanced.as_ref().map_or(0.0, |(s, t)| {
                    resolve_scaled_effect(
                        *s,
                        t.as_deref(),
                        self.archetype,
                        self.level,
                        self.db,
                        self.errors,
                    ) * 100.0
                });
                g.regeneration += value + unenh;
            }
        } else if let Some((scale, table)) = &regen_unenhanced {
            g.regeneration += resolve_scaled_effect(
                *scale,
                table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
        }

        // Recovery — the regen twin. Enhanced by Endurance Modification enhancement.
        // Atom-native, and its two bag arms are gone for the same measured reason (ATOM-BAG-5):
        // the only power in three forks that reached `?? effects.recoveryBuffUnenhanced` was
        // Defibrillate, whose foe -Recovery row put a `scale: 30` DURATION multiplier in the
        // caster's magnitude slot and reported +3000% recovery.
        // Same `Res_Boolean` skip on the enhanceable half (endurance-drain resistance,
        // not a recovery buff), read off the resolved slot's table (atom or bag). The two halves
        // are separate blocks in the beta, not combined like regen. The enhanceable half is
        // multiplied by `1 + enh.get("enduranceMod")` (Endurance Modification enhancement); the
        // unenhanced half is not.
        //
        // The HP-scaling recovery Expression (Gamma Boost) — the recovery twin of the regen block
        // above, self-admitting for the same reason (PASS2B-2, ATOM-BAG-5).
        let atom_recovery_expression = hp_scaling_resource_value(
            self.power,
            coh_data::EffectType::Recovery,
            self.archetype,
            self.level,
            self.combat,
            self.db,
            self.errors,
        );
        let recovery_slot: Option<(f64, Option<String>)> = recovery_buff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| {
                self.stack(
                    s,
                    StackFamily::Buff(EffectType::Recovery, Half::Enhanceable, None),
                )
            })
            .map(|s| (s.scale, s.table));
        let recovery_enhanceable: Option<(f64, Option<String>)> = match atom_recovery_expression {
            Some(expr) => Some((
                expr.value,
                recovery_slot
                    .as_ref()
                    .and_then(|(_, t)| t.clone())
                    .or(expr.table),
            )),
            None => recovery_slot.as_ref().map(|(scale, table)| {
                (
                    resolve_scaled_effect(
                        *scale,
                        table.as_deref(),
                        self.archetype,
                        self.level,
                        self.db,
                        self.errors,
                    ),
                    table.clone(),
                )
            }),
        };
        if let Some((magnitude, table)) = recovery_enhanceable {
            if !table_is_res_boolean(table.as_deref()) {
                g.recovery += magnitude * 100.0 * (1.0 + self.enh.get("enduranceMod"));
            }
        }
        let recovery_unenhanced_slot: Option<(f64, Option<String>)> =
            recovery_buff_unenhanced_value(self.power)
                .map(|__tbs| self.to_scaled(__tbs))
                .map(|s| {
                    self.stack(
                        s,
                        StackFamily::Buff(EffectType::Recovery, Half::Unenhanced, None),
                    )
                })
                .map(|s| (s.scale, s.table));
        if let Some((scale, table)) = recovery_unenhanced_slot {
            g.recovery += resolve_scaled_effect(
                scale,
                table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
        }

        // Recharge buff (Hasten, Quickness, Speed Boost) — atom-native (ATOM1 /
        // DATA-GAP-REGISTER PASS2B-3), else the `?? bag.rechargeBuff` fallback. The
        // `RechargeTime Str` atoms ARE the +recharge buffs; the bag only stays load-bearing for
        // HC Entropic Aura (a pure per-target increment with no matching atom) and every
        // Thunderspy recharge power (TSPY-3 `Unmapped`). Behavior-preserving: Σ|atom.scale|
        // equals the bag scale exactly across the corpus (measured HC 28/28 · Reb 20/20). NOT
        // enhanced (recharge enhancements cut a power's own recharge time, not a +recharge buff)
        // and read `scale` DIRECTLY × 100 — a recharge buff carries the final fraction on a
        // `*_Ones` table, so NO AT-table resolution (the beta's `extractScaleValue`).
        // `adjustForStacking` is applied after the `??` (the corpus-zero no-op seam).
        let recharge_slot: Option<Scaled> = recharge_buff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| {
                self.stack(
                    s,
                    StackFamily::Buff(EffectType::RechargeTime, Half::Either, None),
                )
            });
        if let Some(s) = recharge_slot {
            g.recharge += s.scale * 100.0;
        }
        // Self-directed `rechargeDebuff` crash (Granite Armor −65%, Reaction Time −40%) —
        // atom-native, else `?? bag.self_recharge_debuff`. Self-facing only (`toWho:Self`); a
        // foe-facing −recharge is an enemy debuff, never the caster's total. Unlike the buff,
        // this half IS AT-table resolved (`resolveScaledEffect × -100`) and subtracts. The atom
        // carries the crash as a negative `scale`; the reader returns `|scale|`, resolved and
        // negated here — exact match to the bag (measured 2/2 both datasets). Unenhanceable.
        let recharge_crash: Option<Scaled> =
            recharge_self_debuff_value(self.power).map(|__tbs| self.to_scaled(__tbs));
        if let Some(s) = recharge_crash {
            g.recharge -= resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
        }

        // Max Endurance — atom-native (ATOM8), else the bag `?? effects.maxEndBuff` for the
        // atom-less residue (all Thunderspy — TSPY-3 `Unmapped`, zero `MaxEndurance` atoms — and any
        // HC/Reb power with none). Physical Perfection, Superior Conditioning, Power of the Depths'
        // team +MaxEnd, Burnout's self −MaxEnd. The reader ([`appliers::resources::max_endurance_buff_value`])
        // reads the `endurance@maximum` atoms (bridge-folded to `MaxEndurance`) with the RESOURCE-SUM
        // fold, twin collapsed, self-debuffs kept, foe-facing debuffs dropped — equal to the bag value
        // exactly across the corpus (census HC 6/6 · Reb 3/3, pinned by the
        // `max_endurance_atom_bag_parity` guard), so this is a behavior-preserving source swap: the
        // beta has no atom reader and reads the bag directly (character-totals.ts:1728). Enhanced by
        // Endurance Modification (`1 + enh.get("enduranceMod")`).
        // Absolute endurance POINTS, NOT a percentage: the beta resolves the scale via AT table but
        // does NOT × 100 (scale 5 = +5 end).
        let max_end_slot: Option<Scaled> =
            max_endurance_buff_value(self.power).map(|__tbs| self.to_scaled(__tbs));
        if let Some(s) = max_end_slot {
            let enh_multiplier = 1.0 + self.enh.get("enduranceMod");
            g.max_endurance += resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * enh_multiplier;
        }
    }

    fn apply_affinity_buffs(&mut self, g: &mut GlobalBonuses) {
        // Accuracy — atom-native (ATOM9), else the bag `?? effects.accuracyBuff` for the atom-less
        // residue. Combat Training: Offensive, Focused Accuracy, Targeting Drone. The reader
        // ([`appliers::accuracy::accuracy_buff_value`]) mirrors the converter's `accuracyBuff` gate
        // (`aspect != Res` + `!isDebuff`, OVERWRITE fold) and equals the bag value exactly across the
        // corpus (census HC 7/7 · Reb 4/4 · tspy 2/2, pinned by the `accuracy_atom_bag_parity` guard),
        // so this is a behavior-preserving source swap: the beta has no atom reader and reads the bag
        // directly (character-totals.ts:1179). UNLIKE ATOM5-8, Thunderspy carries real `Accuracy`
        // atoms (Conditioning) — the first atom-fed tspy family, not a TSPY-3 residual. A PERCENTAGE
        // (`resolveScaledEffect × 100`), AT-table resolved; NOT enhanced (accuracy enhancements boost
        // the attack roll, not a +Accuracy buff power). The `stack()` (`adjustForStacking`) consumer
        // seam stays at the call site, applied to whichever source. Distinct from ToHit (a multiplier,
        // not an additive term).
        let accuracy_slot: Option<Scaled> = accuracy_buff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| {
                self.stack(
                    s,
                    StackFamily::Buff(EffectType::Accuracy, Half::Either, None),
                )
            });
        if let Some(s) = accuracy_slot {
            g.accuracy += resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
        }

        // Perception radius — atom-native (ATOM5), else the bag `?? effects.perceptionBuff` for
        // the atom-less residue (all Thunderspy — TSPY-3 `Unmapped`, zero `Perception` atoms — and
        // any HC/Reb power with no `Perception` atom). Tactics, Focused Accuracy, Clear Mind,
        // +Perception auras. The reader mirrors the converter's `perceptionBuff` gate (`aspect != Res`,
        // `!isDebuff`, OVERWRITE fold) and equals the bag value exactly across the corpus (census
        // HC 25/25 · Reb 20/20, pinned by the `perception_atom_bag_parity` guard), so this is a
        // behavior-preserving source swap: the beta has no atom reader and reads the bag directly
        // (character-totals.ts:1971). A PERCENTAGE (`resolveScaledEffect × 100`), AT-table resolved;
        // NOT enhanced, NOT stacked. Beta gates on `value > 0`: only a positive +Perception aggregates
        // here — a negative value is a −Perception debuff owned by a different (M4) path, so the guard
        // is not vacuous even though the corpus buffs are all positive.
        let perception_slot: Option<Scaled> =
            perception_buff_value(self.power).map(|__tbs| self.to_scaled(__tbs));
        if let Some(s) = perception_slot {
            let value = resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
            if value > 0.0 {
                g.perception_radius += value;
            }
        }

        // Endurance Discount / EndDisc — atom-native (ATOM7), else the bag `?? effects.enduranceDiscount`
        // for the atom-less residue (all Thunderspy — TSPY-3 `Unmapped`, zero `EnduranceDiscount` atoms —
        // and any HC/Reb power with none). Conserve Power, Body Mastery, Force Affinity. The reader
        // mirrors the converter's UNCONDITIONAL `enduranceDiscount` write (no aspect/debuff/self gate —
        // simplest in the series; the endurance DDR is a different `Endurance`-attrib slot, ATOM13) and
        // equals the bag value exactly across the corpus (census HC 8/8 · Reb 9/9, pinned by the
        // `endurance_discount_atom_bag_parity` guard), so this is a behavior-preserving source swap: the
        // beta has no atom reader and reads the bag directly (character-totals.ts:1757). A PERCENTAGE
        // (`resolveScaledEffect × 100`), AT-table resolved; NOT enhanced. The `discount > 0` consumer
        // gate stays here (SEPARATE, applied to whichever source), routing to the canonical `endurance`
        // EndDisc accumulator (the `endrdx` stat), NOT the vestigial `enduranceDiscount` field — so
        // toggle-cost math and the dashboard read one unified sum.
        let endurance_discount_slot: Option<Scaled> =
            endurance_discount_value(self.power).map(|__tbs| self.to_scaled(__tbs));
        if let Some(s) = endurance_discount_slot {
            let discount = resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
            if discount > 0.0 {
                g.endurance += discount;
            }
        }

        // Range buff — atom-native (ATOM6), else the bag `?? effects.rangeBuff` for the atom-less
        // residue (all Thunderspy — TSPY-3 `Unmapped`, zero `Range` atoms — and any HC/Reb power
        // with no buff-face `Range` atom). Boost Range, Aim's +Range, team +Range. The reader
        // mirrors the converter's `rangeBuff` gate (`aspect != Res`, `!isDebuff`, `toWho == Self ||
        // aspect == Str`, OVERWRITE fold) and equals the bag value exactly across the corpus
        // (census HC 13/13 · Reb 1/1, pinned by the `range_atom_bag_parity` guard), so this is a
        // behavior-preserving source swap: the beta has no atom reader and reads the bag directly
        // (character-totals.ts:1989). A PERCENTAGE (`resolveScaledEffect × 100`), AT-table resolved;
        // NOT enhanced. `adjustForStacking` is the corpus-zero no-op seam. Two SEPARATE consumer
        // gates stay here: beta gates the resolved value on `> 0`, and the self-gate
        // (`is_self(power)`, character-totals.ts:1989) is load-bearing — the same `rangeBuff` slot
        // on a Foe-targeted attack (every snipe — Blazing Bolt, Moonbeam) is the per-power Fast
        // Snipe range bump, gated in-game on a ≥22% ToHit buff, NOT a persistent caster buff, so it
        // must not feed the character Range total. `is_self` reads the POWER's `targetType`, not the
        // atom's `toWho`, so it applies identically to the atom and bag sources.
        let range_slot: Option<Scaled> = range_buff_value(self.power)
            .map(|__tbs| self.to_scaled(__tbs))
            .map(|s| self.stack(s, StackFamily::Buff(EffectType::Range, Half::Either, None)))
            .filter(|_| is_self(self.power));
        if let Some(s) = range_slot {
            let value = resolve_scaled_effect(
                s.scale,
                s.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0;
            if value > 0.0 {
                g.range += value;
            }
        }
    }

    fn apply_shields_and_hp(
        &mut self,
        g: &mut GlobalBonuses,
        absorb_fraction_contribs: &mut Vec<AbsorbFractionContribution>,
    ) {
        // Absorb shield. Two magnitude forms share one multiplier
        // (character-totals.ts:1746). `enhMultiplier = applyStrength ? 1 + enh.get("heal") (Healing
        // enhancement) + strengthBuffs.absorb : 1`, where `applyStrength = maxHPFraction == null ||
        // appliesStrength !== false` (only an explicit `appliesStrength: false` — an ATO proc — opts
        // out).
        //  • FLAT-HP form (real Heal table, no maxHPFraction: Frigid Protection, Rime, Soul
        //    Transfer): resolve the Heal table to ABSOLUTE HP (NO ×100 — an HP amount), add to
        //    `absorb` inline.
        //  • MaxHP-FRACTION form (`maxHPFraction` set — Wild Bastion; OR a `_ones` table —
        //    Ablative, Parasitic Aura): a fraction of the caster's FINAL Max HP. `baseFraction =
        //    maxHPFraction ?? resolveScaledEffect(_ones ⇒ scale)`; push `baseFraction ×
        //    enhMultiplier` into `absorb_fraction_contribs`, resolved to HP against the final Max
        //    HP once every +MaxHP source has summed (lib.rs, beta step 9.2 `:4420`). Both gate `> 0`.
        // The base fraction is read ATOM-NATIVE where the power carries an `Absorb`/`aspect=Max`/
        // `type=Expression` atom (ATOM10 fraction): its `magnitude_expression` is evaluated to the
        // coefficient (`absorb_max_hp_fraction_value`), replacing the bag's converter-recovered
        // `maxHPFraction`; `?? bag` keeps atom-less powers on the bag.
        // An absorb inside a foe-targeted AoE grows with the foes hit like every other self-buff
        // in its block (PROD6C-3j) — `adjustForStacking` reaches BOTH forms below, and the
        // fraction's own increment (`maxHPFractionPerTarget`) stands in where the Expression
        // magnitude leaves no scale for `perTarget` to ride.
        // The flat triple — scale, table and the AoE increment — is read ATOM-NATIVE
        // (ATOM-BAG-3), with `?? bag` for a power that routes no flat atom. `maxHPFraction`,
        // `appliesStrength` stays the bag's; `maxHPFractionPerTarget` no longer does. That
        // companion field was the last of the slot with no atom behind it (PROD6C-3j), and every
        // input the converter derives it from rides the wire — the power's own `effectArea` and
        // `stats.maxTargets`, and the ceiling atom's `toWho`/`aspect`/`attribType`/`stacking`
        // ([`absorb_max_hp_fraction_per_target`]). Graded per power against the EFFECTIVE
        // increment (`maxHPFractionPerTarget ?? perTarget`, since the forks spell one increment
        // on two different slots): 2 / 1 / 1 powers agree, nothing moved, neither side one-sided.
        // **The atoms ADMIT the block** (ATOM-BAG-3's remainder). It used to open with
        // `if let Some(absorb) = bag.absorb()`, which made the bag an admission ticket rather
        // than a fallback: a mode-gated absorb reaches the walk as a synthetic power whose bag
        // `gather::active_conditional_powers` drops on purpose, so Organic Armor's Inexhaustible
        // and the Bio Armor stance increments contributed nothing (ABSORB-3, closed then by an
        // `else` arm that handled the fraction form alone). What kept the gate afterwards was
        // the fear that admitting the FLAT form on atoms would double-count, since a shield's
        // `Absorb/Cur` points row sits beside its `Absorb/Max` Expression ceiling. It does not:
        // the pair is one shield stated twice and the `_ones` table routes it to the fraction
        // arm exactly once ([`states_max_hp_fraction`] carries the reading and the evidence).
        // Dropping the bag side entirely is corpus-neutral for the same reason removing the
        // gate was — over three forks, zero powers carried absorb atoms without the bag slot
        // and zero carried the slot without atoms (`absorb_atom_census` §5, measured before
        // atom1-13 deleted both the slot and the census; the reading stands, it cannot be re-run).
        // `stack()` now reaches those too, where the old arm skipped it: a synthetic's
        // `targets_hit` is `None` (lib.rs) and no gated absorb atom in any fork carries a
        // `per_target`, so both stacking paths are the identity there today.
        let atom_fraction = absorb_max_hp_fraction_value(self.power);
        let atom_flat = absorb_flat_value(self.power);
        if atom_fraction.is_some() || atom_flat.is_some() {
            let scale = atom_flat.as_ref().map_or(0.0, |v| v.scale);
            let table = atom_flat.as_ref().and_then(|v| v.table.clone());
            let per_target = atom_flat.as_ref().and_then(|v| v.per_target);
            let is_ones = states_max_hp_fraction(table.as_deref());
            // Strength always applies. The one thing that could withhold it was the bag's
            // `appliesStrength: false`, and no power carried it without an atom stating the
            // same — the removal census would have moved on any that did, since dropping the
            // bag turns the multiplier back on.
            let enh_multiplier = {
                // `absorb`, not `heal`: Absorb is the SECOND attrib of the Healing boost, so
                // every Heal-boosting enhancement writes both aspects at the same value
                // (`mirror_heal_to_absorb` in enhancement.rs guarantees it for the sources whose
                // data lists only "Healing") — reading either gives the same number for slotted
                // enhancements. It has to be `absorb` for the boosts that hit Absorb ALONE:
                // the Cardiac/Resilient Radial Alpha's +33% Absorb must not inflate heals.
                1.0 + self.enh.get("absorb") + self.strength.absorb
            };
            // The scale half stacks BEFORE the table resolve, as the beta stacks it — growing the
            // resolved amount instead would scale the increment by the table.
            let stacked = self.stack(
                Scaled {
                    scale,
                    table: table.as_deref().map(String::from),
                    per_target,
                },
                StackFamily::Buff(EffectType::Absorb, Half::Either, None),
            );
            if atom_fraction.is_some() || is_ones {
                let base_fraction = match atom_fraction {
                    // A fraction the converter (or the atom's Expression) already carries as a
                    // fraction takes its increment from whichever field the export authored it
                    // on: `maxHPFractionPerTarget` beside a `maxHPFraction`, the ordinary
                    // `perTarget` beside a `_ones` scale the same power spells both ways.
                    Some(fraction) => {
                        self.stack(
                            Scaled {
                                scale: fraction,
                                table: None,
                                per_target: absorb_max_hp_fraction_per_target(self.power)
                                    .or(per_target),
                            },
                            StackFamily::Buff(EffectType::Absorb, Half::Either, None),
                        )
                        .scale
                    }
                    None => resolve_scaled_effect(
                        stacked.scale,
                        stacked.table.as_deref(),
                        self.archetype,
                        self.level,
                        self.db,
                        self.errors,
                    ),
                };
                let fraction = base_fraction * enh_multiplier;
                if fraction > 0.0 {
                    absorb_fraction_contribs.push(AbsorbFractionContribution {
                        fraction,
                        power_internal_name: self.active.def.ident().to_string(),
                        power_set: self.active.power_set.to_string(),
                        kind: self.active.kind,
                    });
                }
            } else {
                let hp = resolve_scaled_effect(
                    stacked.scale,
                    stacked.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * enh_multiplier;
                if hp > 0.0 {
                    g.absorb += hp;
                }
            }
        }

        // Max HP — enhanced by Healing enhancement. Reads `.scale` DIRECTLY (×10, no table
        // resolution): every +MaxHP power's value carries no per-target and the displayed buff
        // is a flat 10% per scale point (Dull Pain scale 2 → +20%). Atom-native: every corpus
        // +MaxHP power carries a real Max-aspect atom, and unlike the Expression-punted
        // regen/recovery twins none of them is punted, so the `?? effects.maxHPBuff` /
        // `?? effects.maxHPBuffUnenhanced` fallback that used to sit here
        // (character-totals.ts:1669, 1690) never answered for any build. The enhanceable half is
        // multiplied by `1 + enh.get("heal")` (Healing enhancement); the IgnoreStrength half has
        // NO such multiplier.
        if let Some(scale) = max_hp_buff_value(self.power).map(|v| v.scale) {
            g.max_hp += scale * MAX_HP_SCALE_TO_PERCENT * (1.0 + self.enh.get("heal"));
        }
        if let Some(scale) = max_hp_buff_unenhanced_value(self.power).map(|v| v.scale) {
            g.max_hp += scale * MAX_HP_SCALE_TO_PERCENT;
        }
    }

    fn apply_mez_protection(&mut self, g: &mut GlobalBonuses) {
        // === Pass 2b: mez PROTECTION (atom-native — ATOM11 for the six MEZ types, ATOM15 for
        // KB/KU) ===
        // Writes flat magnitudes into `prot*`, NOT percentages.
        //
        // The beta had a second path here: a flat pool/epic `protection` object
        // (`{ hold: 1, knockback: 4 }`) read straight off the bag. No power on any fork carries
        // that slot — zero carriers over all three — so it read nothing and went with the bag.

        // Curated-armor mez protection — `effects.hold/stun/…/knockback/knockup` with a
        // `Res_Boolean` table (armor protection), OR (MEZPROT-2) the winning atom is
        // protection-spelled (negative scale / magnitude / Expression) on a power that affects
        // no foe (the converter's `protectionBackedMezKeys` + `TSPY_MEZ_FOE_TARGETS`
        // discriminator, now graded on the fold winner) — e.g. Bane Spider Armor's six,
        // Grounded's / Minerals' protections on `Melee_Ones` — OR a self-affecting SingleTarget
        // Knockback/Knockup (Acrobatics-style KB protection on a `Melee_Ones` table). The
        // magnitude is `|scale| × table[level]`. KB/KU fold to `max` per power.
        //
        // Atom-native for the SIX MEZ types (ATOM11 / PASS2B-10): each `effects.<mezType>` slot is
        // the `Mez/<subType>` atom (aspect ∉ {Res, Str}), read via [`mez_protection_value`] with the
        // converter's max-magnitude / PvE-preferred fold, where the frozen oracle reads the bag slot
        // directly (character-totals.ts:1810). Behavior-preserving — the atom slot equals the bag
        // slot for every credited case across the corpus (census HC/Reb 0 credited bag-only / phantom
        // / divergence, pinned by the `mez_protection_atom_bag_parity` guard). The `?? bag` that
        // used to carry the atom-less residue is gone with TSPY-3, which recovered Thunderspy's
        // typing and left the residue empty.
        //
        // Atom-native for KNOCKBACK/KNOCKUP too (ATOM15 / PASS2B-1): [`kb_protection_value`]
        // accumulates the SELF-directed KB protection atoms (`Mez` + `MezResist`, the converter's
        // branches 2a/3), reproducing the bag on the legit self-protection powers while EXCLUDING the
        // foe-attack branch the old `effectArea + powerType` proxy miscredited. This is a deliberate
        // value change (foe-KB attacks → 0 `protKnockback`), landed on both the oracle and here, not a
        // behavior-preserving migration. The downstream Res_Boolean gate + table resolution + KB
        // max-fold are all untouched.
        //
        // The non-Res_Boolean self-KB path multiplies its magnitude by `1 + enh.get("knockback")`
        // (Knockback enhancement boosts Acrobatics' KB protection — the beta's
        // `kbIsSelfAtom && !isResBoolean` gate). The unslotted corpus keeps it ×1.0.
        let mut knockback_protection = 0.0_f64;
        for (field, routed_ty) in MEZ_PROT_TYPES {
            let is_kb = matches!(field, "knockback" | "knockup" | "repel");
            // KB/KU protection is atom-native (ATOM15 / PASS2B-1): `kb_protection_value` accumulates
            // ONLY the power's self-directed KB protection atoms (foe-attack knockback excluded,
            // `MezResist` included), so a self-atom result is caster protection by construction — the
            // old `effectArea + powerType` proxy (which miscredited SingleTarget foe attacks) is
            // retired. A power with no self-KB atom contributes nothing here.
            let (mez, kb_is_self_atom) = if is_kb {
                (kb_protection_value(self.power, field), true)
            } else {
                (mez_protection_value(self.power, field), false)
            };
            let Some(mez) = mez else { continue };
            let table_lower = mez.table.to_ascii_lowercase();
            let is_res_boolean = table_lower.contains("res_boolean");
            // MEZPROT-2: a slot is credited when the table is `Res_Boolean` (the classic armor
            // protection), when it is a self-KB atom (KB/KU protection, Acrobatics/Throwing
            // knives — owns its own fold), or when the winning atom is protection-SPELLED and
            // the power does not affect a foe. The three-spelling test (`is_protection`, set by
            // `mez_protection_value`) plus the recipient is the converter's own discriminator
            // (`protectionBackedMezKeys` + TSPY_MEZ_FOE_TARGETS), now graded on the fold winner
            // instead of being lost to the `|scale|` abs fold. A spelled non-Res_Boolean winner
            // on `Melee_Ones`/`Ranged_Ones` (Bane Spider Armor's six, Grounded's immobilize,
            // Minerals' confuse) is caster armor — a real total the old Res_Boolean-only gate
            // owed nothing. The recipient term keeps a foe control that happens to be spelled
            // (Defibrillate's revenge sleep) out of the caster's protection.
            let protection = is_res_boolean
                || kb_is_self_atom
                || (mez.is_protection && !self.power.affects_foe());
            if !protection {
                continue;
            }
            // Read at the build level, like every other table read in this pass. The beta
            // pinned 50 here on the claim that "protection magnitude is a fixed value that
            // doesn't scale down during leveling"; the claim is false on both oracles —
            // `Res_Boolean` appears nowhere in the game source (no code branch special-cases
            // it, so `mod_Fill` resolves it at `iEffCombatLevel` like any other template), and
            // the tables themselves vary by level (`melee_res_boolean` runs 0.120 → 0.277
            // across 1–50), so the pin roughly doubled a level-10 build's protection.
            let Some(table_value) =
                self.db
                    .at_tables
                    .get_table_value(self.archetype, &table_lower, self.level)
            else {
                continue;
            };
            let mut magnitude = mez.scale.abs() * table_value;
            // Knockback enhancements boost non-Res_Boolean self-KB protection (per the
            // Acrobatics description) — the beta's `kbIsSelfAtom && !isResBoolean` gate.
            if kb_is_self_atom && !is_res_boolean {
                magnitude *= 1.0 + self.enh.get("knockback");
            }
            if routed_ty == "knockback" {
                knockback_protection = knockback_protection.max(magnitude);
            } else {
                route_closed(g.add_mez_protection(routed_ty, magnitude), routed_ty);
            }
        }
        if knockback_protection > 0.0 {
            g.protection_knockback += knockback_protection;
        }
    }

    fn apply_mez_resistance(&mut self, g: &mut GlobalBonuses) {
        // === Pass 2b: MEZ RESISTANCE, per-type — atom-native (ATOM2 / PASS2B-13) ===
        // Vengeance, Sonic/Thermal shields, Acrobatics, etc. (`character-totals.ts:1369`).
        // Per-type mez RESISTANCE (duration reduction) — distinct from mez PROTECTION above
        // (which resists a mez landing at all). Atom-native: the `MezResist aspect=Res` atoms ARE
        // the per-type resistance (subType names the type), else the `?? bag.mez_resistance`
        // fallback for the atom-less residue — ALL of Thunderspy (TSPY-3 `Unmapped`). Behavior-
        // preserving: the atom fold equals the bag value exactly across HC + Rebirth (measured,
        // every routed type, 0 phantoms / 0 divergences — see the ATOM2 spec). The reader folds
        // the converter's TWO shapes: the six MEZ types ACCUMULATE (`Σ|scale|`, mirroring
        // `+= |scale|`), knockback OVERWRITES (single value, self-facing + non-`Res_Boolean`
        // only). Downstream is unchanged: `resolveScaledEffect × 100 × enhMultiplier`, `Res_Boolean`
        // tables RESOLVE here (unlike the regen/recovery buffs), no `> 0` gate.
        //
        // Enhanced by `1 + enh.get(type)` — each type is boosted by its OWN mez enhancement (Hold
        // resistance by Hold enhancement, etc.); the unslotted corpus keeps it
        // ×1.0. `add_mez_resistance` maps hold/stun/immobilize/sleep/confuse/fear/knockback plus
        // taunt/placate (MEZRES-2 — the beta's `mezResMapping` had no entry for the pair, so its
        // `key && key in global` guard dropped the only encoding the Leadership pool's Assault
        // has) and `repel` (MEZRES-3), and declares `knockup` unspent with its reason. The reader
        // still emits knockup so the decision happens in one place, at the router. `teleport` is
        // the exception it cannot make that way: it is held back at the READER, because only
        // there is the atom's target still visible (see `appliers::mez_resistance`).
        let mez_resistance_entries: Option<Vec<(String, Scaled)>> =
            mez_resistance_value(self.power).map(|es| {
                es.into_iter()
                    .map(|(ty, tv)| (ty, self.to_scaled(tv)))
                    .collect()
            });
        if let Some(entries) = mez_resistance_entries {
            for (ty, s) in entries {
                // Each type is boosted by its OWN mez enhancement (`enhBonuses[type]`), e.g.
                // Hold resistance by Hold enhancement. `ty` is a normalized aspect key.
                //
                // Taunt/placate are the exception: they are UNENHANCED, because the ATOM12 path
                // that credits the very same two totals from the `effects.taunt|placate` slot
                // applies no multiplier, and one stat must not enhance differently depending on
                // which of its two encodings a power happens to use.
                let enh_multiplier = match ty.as_str() {
                    "taunt" | "placate" => 1.0,
                    _ => 1.0 + self.enh.get(&ty),
                };
                let percent = resolve_scaled_effect(
                    s.scale,
                    s.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * 100.0
                    * enh_multiplier;
                surface_route(
                    g.add_mez_resistance(&ty, percent),
                    "Mez resistance",
                    &ty,
                    &self.power.name,
                    self.errors,
                );
            }
        }
    }

    fn apply_debuff_resistance(&mut self, g: &mut GlobalBonuses) {
        // === Pass 2b: DEBUFF RESISTANCE, per-type — atom-native (ATOM13 / PASS2B-11) ===
        // Foresight, Tactical Training: Leadership, Combat Training: Offensive, Mental
        // Training. Each type resolves `resolveScaledEffect × 100 × enhMultiplier` into the
        // field `add_debuff_resistance` routes it to (bag `movement` → `debuff_resist_slow`).
        // Unlike the regen/recovery buffs, a `Res_Boolean` table is RESOLVED here, not skipped —
        // the boolean IS the value.
        //
        // Atom-native: each debuff-resistance type is the resisted attribute's own
        // `<EffectType> aspect=Res` atom ([`appliers::debuff_resistance::debuff_resistance_value`],
        // OVERWRITE fold), read where the frozen oracle reads the bag directly
        // (character-totals.ts:1323). Behavior-preserving — the atom value equals the bag value
        // exactly across the corpus for every routed type (census HC 192/192 · Reb 142/142, 0
        // bag-only / 0 phantom / 0 divergence, pinned by the `debuff_resistance_atom_bag_parity`
        // guard), `?? bag` for the atom-less residue (ALL of Thunderspy — TSPY-3 `Unmapped`, zero
        // aspect=Res debuff-resistance atoms). Whole-map fallback (the ATOM2 shape): the reader
        // reproduces the ENTIRE routed bag map, so the atom side is used wholesale or not at all.
        // `accuracy` and `range` are in that map since DEBUFFRES-1; the oracle has no field for
        // either, so those two are the one place this pass reads higher than the frozen calc.
        //
        // Only `debuffResistDefense` carries an enhancement multiplier — `× (1 + enh.get("defense"))`
        // (Defense enhancement boosts defense-debuff resistance; the beta's `debuffResEnhMapping`);
        // every other type is unenhanced. NOT boosted by +Strength(Defense) — the beta uses
        // `enhBonuses`, not `strengthBuffs`, here. `adjustForStacking` is the corpus-zero no-op seam
        // (default combat context).
        let debuff_resistance_entries: Option<Vec<(String, Scaled)>> =
            debuff_resistance_value(self.power).map(|es| {
                es.into_iter()
                    .map(|(ty, tv)| (ty, self.to_scaled(tv)))
                    .collect()
            });
        if let Some(entries) = debuff_resistance_entries {
            for (ty, s) in entries {
                let s = self.stack(s, StackFamily::DebuffResistance);
                // Only defense-debuff-resistance is enhanced — by Defense enhancement (the beta's
                // `debuffResEnhMapping = { defense: 'defense' }`); every other type is unenhanced.
                // Not boosted by +Strength(Defense): the beta uses `enhBonuses`, not `strengthBuffs`.
                let enh_multiplier = if ty == "defense" {
                    1.0 + self.enh.get("defense")
                } else {
                    1.0
                };
                let percent = resolve_scaled_effect(
                    s.scale,
                    s.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * 100.0
                    * enh_multiplier;
                surface_route(
                    g.add_debuff_resistance(&ty, percent),
                    "Debuff resistance",
                    &ty,
                    &self.power.name,
                    self.errors,
                );
            }
        }
    }

    fn apply_mez_resistance_extra(&mut self, g: &mut GlobalBonuses) {
        // === Pass 2b: TAUNT / PLACATE RESISTANCE — atom-native (ATOM12 / PASS2B-14) ===
        // World of Pain, Tactical Training: Assault (`character-totals.ts:1907` / `:1927`) — the
        // beta's separate "Additional mez resistance" pair. The `{scale, table}` source is now the
        // `Mez/Taunt|Placate` atom (aspect != Res — [`taunt_placate_value`], OVERWRITE fold),
        // read where the frozen oracle reads the top-level `effects.taunt`/`effects.placate` bag slot
        // directly. The `?? bag` that carried the atom-less residue is gone; its only members were
        // HC `Entropic Aura`/`Geode`, a taunt EFFECT on a non-`Res_Boolean` table the gate below
        // drops anyway, so nothing it fed was ever credited. Behavior-preserving: the atom equals the
        // bag slot exactly across the corpus (census 0 phantom / 0 divergence all 3, pinned by the
        // `taunt_placate_atom_bag_parity` guard). UNLIKE ATOM5-8, Thunderspy carries real
        // Taunt/Placate atoms, so it is atom-fed too.
        //
        // This is ONE of the stat's two encodings. The other — an `aspect=Res` `MezResist` atom in
        // the `mezResistance` slot — reaches the same two totals through `add_mez_resistance`
        // (MEZRES-2). They sum rather than collide because no power carries both, pinned corpus-wide
        // by `route_sweep::taunt_placate_resistance_encodings_are_disjoint`.
        //
        // The downstream gate is UNCHANGED: credited ONLY when the atom reader yields a
        // `{scale, table}` pair (the beta gated the bag slot the same way, on
        // `typeof !== 'number'`) AND its table is
        // `Res_Boolean` (taunt/placate RESISTANCE; the taunt EFFECT rides InherentTaunt/Ones tables
        // and drops here). mag = `|scale| × getTableValue × 100`, the table read at the BUILD
        // `level`; a table miss drops. `Math.abs` because a
        // −scale still grants +resistance (the atom scale is negative; the reader already abs'd it).
        let taunt_placate_resistance = |sub: SubType| -> f64 {
            let Some(mez) = taunt_placate_value(self.power, sub) else {
                return 0.0;
            };
            let table_lower = mez.table.to_ascii_lowercase();
            if !table_lower.contains("res_boolean") {
                return 0.0;
            }
            match self
                .db
                .at_tables
                .get_table_value(self.archetype, &table_lower, self.level)
            {
                Some(table_value) => mez.scale.abs() * table_value * 100.0,
                None => 0.0,
            }
        };
        g.mez_resist_taunt += taunt_placate_resistance(SubType::Taunt);
        g.mez_resist_placate += taunt_placate_resistance(SubType::Placate);
    }

    fn apply_stealth(&mut self, stealth_contributions: &mut Vec<StealthContribution>) {
        // === Pass 2b: STEALTH RADIUS — COLLECTED, not accumulated (atom-native, ATOM3) ===
        // Stealth, Superior Invisibility, Arctic Fog, Super Speed (`character-totals.ts:1934`).
        // Each radius resolves in FEET (no `× 100` — it is a distance), unenhanced and
        // unstacked (the beta wraps neither axis in `adjustForStacking`).
        //
        // The `(pve, pvp, stack_key)` source is the typed `Stealth/RadiusPvE|RadiusPvP` atoms
        // (BRIDGE-2/ATOM3). The `?? bag.stealth()` arm behind it is gone (ATOM-BAG-7): its one
        // remaining carrier corpus-wide was a hand override pinning Street Justice's Assassin's
        // Strike at +1 ft on both axes, which was the `Str`-face reveal row read as a radius.
        // The bag-removal census now reports zero movers on all three forks.
        //
        // This is the one Pass 2 family that cannot add into `g` here: powers sharing a
        // suppress group don't stack, so no contribution's value is known until every source
        // is. `stealth_contributions` is committed by `crate::stealth::resolve_stealth_radius`
        // once the walk finishes. Only positive axes push, so a zero/zero stealth slot
        // (`translucency`-only powers, both radii absent) never becomes a contribution.
        if let Some(stealth) = stealth_contribution(self.power) {
            let mut radius = |axis: Option<Scaled>| {
                axis.map_or(0.0, |s| {
                    resolve_scaled_effect(
                        s.scale,
                        s.table.as_deref(),
                        self.archetype,
                        self.level,
                        self.db,
                        self.errors,
                    )
                })
            };
            let pve = radius(stealth.pve);
            let pvp = radius(stealth.pvp);
            if pve > 0.0 || pvp > 0.0 {
                stealth_contributions.push(StealthContribution {
                    stack_key: stealth.stack_key,
                    pve,
                    pvp,
                    power_name: self.power.name.clone(),
                });
            }
        }
    }

    fn apply_movement(
        &mut self,
        movement_contribs: &mut Vec<MovementContribution>,
        movement_cap_contribs: &mut Vec<MovementCapContribution>,
        g: &mut GlobalBonuses,
    ) {
        // === Pass 7: MOVEMENT — COLLECTED, resolved together (suppress-group max + additive) ===
        // Committed by [`crate::movement::resolve_movement_totals`] once the walk finishes, because
        // travel buffs sharing a `stackKey` (kTravelBuff) take their strongest member instead of
        // summing. Every axis is `resolveScaledEffect × 100` (a percentage). Push order matches the
        // beta (`character-totals.ts:1423-1503`).
        //
        // The movement MAP (Super Speed, Fly, Combat Jumping, …) — atom-native `movementBuffValue`,
        // `?? bag` for an atom-less map. The bag arm still answers for the teleports and three
        // Thunderspy travel buffs (1 / 4 / 9 powers), on their control/friction axes. Skipped when the power
        // carries a tohit/damage DEBUFF: that marks a foe-targeting aura (Time's Juncture) whose
        // movement is a foe slow, not a self-buff (`character-totals.ts:1472`).
        //
        // Read off the ATOMS, not `slot_present("tohitDebuff") || slot_present("damageDebuff")`
        // (ATOM-BAG-4d). The bag-presence form gated the atom arm on a bag key, so deleting the
        // slot did not merely lose the bag's own answers — it disarmed the guard and MINTED
        // movement that had been correctly withheld (Granite Armor +50 JumpSpeed on Rebirth and
        // Thunderspy). The guard had to move to atoms before any movement slot could go.
        if carries_combat_debuff(self.power) {
            // foe-aura movement is not the caster's buff
        } else {
            // The reader's axis keys are `&'static str`; they used to be copied to `String` so
            // they could share a type with the bag map this fell back to.
            let entries = movement_buff_value(self.power).unwrap_or_default();
            let axis_counts: std::collections::HashMap<&str, usize> =
                entries
                    .iter()
                    .fold(std::collections::HashMap::new(), |mut m, (a, _)| {
                        *m.entry(*a).or_default() += 1;
                        m
                    });
            for (axis, movement) in &entries {
                let Some((stat, aspect, sub_type)) = movement_axis(axis) else {
                    continue;
                };
                // STACK-4: the axis stacks to the depth its OWN sub type reaches. Time Wall's
                // Run is `Stack` (cap 2) while its Fly / Jump are `Replace` — one family cap
                // used to multiply all three axes by 2.
                let stacked = self.stack(
                    Scaled {
                        scale: movement.scale,
                        table: movement.table.as_ref().map(|t| t.to_string()),
                        per_target: movement.per_target,
                    },
                    StackFamily::Buff(EffectType::Movement, Half::Either, Some(sub_type)),
                );
                let base = resolve_scaled_effect(
                    stacked.scale,
                    stacked.table.as_deref(),
                    self.archetype,
                    self.level,
                    self.db,
                    self.errors,
                ) * 100.0;
                // An `IgnoreStrength` entry is the half of a two-template axis the
                // caster's Run/Fly/Jump enhancements do not touch — the same rule
                // the converter's `IgnoreStrength` flag encodes on the atom.
                //
                // Only where the axis actually HOLDS a pair. A lone `IgnoreStrength`
                // entry has been multiplied by the aspect since the map existed, and
                // 26 / 20 / 10 of them ship across the three forks — Super Speed's
                // jump ride-along, Enforced Morale, Rooted, Granite, most of the
                // flight family. Reading the flag for them too is very likely right
                // and is certainly a different change from this one, so they keep
                // the answer they have. MOVEMAP-1.
                let paired = axis_counts.get(axis).copied().unwrap_or(0) > 1;
                let multiplier = if movement.ignore_strength && paired {
                    1.0
                } else {
                    1.0 + self.enh.get(aspect)
                };
                movement_contribs.push(MovementContribution {
                    stat,
                    value: base * multiplier,
                    stack_key: movement.stack_key.as_ref().map(|k| k.to_string()),
                    suppressible: movement.suppressible,
                    power_name: self.power.name.clone(),
                });
            }
        }

        // The travel CEILING raises (`aspect=Maximum` movement templates — Super Speed, Fly,
        // Afterburner). Collected beside the buffs and resolved by
        // [`crate::movement::resolve_cap_bumps`], because these share suppress groups the same way.
        // Resolved through the AT table like any other scaled effect rather than read raw: every
        // cap bump on all three datasets sits on a `*_Ones` table (flat 1.0), so this agrees with
        // the beta's raw-`scale` read everywhere it can be measured, and stays right if one ever
        // lands on a real curve. NOT enhanceable — a Run enhancement makes the character faster,
        // not the ceiling higher.
        //
        // Atom-native as of ATOM-BAG-4(b), the last of the four movement readers to go live. It
        // was held back while Reaction Time's self `Maximum` row looked like a cap raise the bag
        // had never carried; MOVEMAP-6 found that row to be the toggle's `OnDeactivate` mirror,
        // which the bag declines to route and the atom now declines with it. The two sides agree
        // exactly on all three forks with nothing exempted, so the swap moves no total —
        // `movement_cap_atom_bag_parity` is what says so, in both directions.
        let atom_cap_bump = movement_cap_bump_value(self.power).map(|v| {
            v.into_iter()
                .map(|(axis, m)| (axis.to_string(), m))
                .collect::<Vec<_>>()
        });
        for (axis, bump) in atom_cap_bump.unwrap_or_default() {
            let Some((stat, _, _)) = movement_axis(&axis) else {
                continue;
            };
            let scale = resolve_scaled_effect(
                bump.scale,
                bump.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            );
            movement_cap_contribs.push(MovementCapContribution {
                stat,
                scale,
                stack_key: bump.stack_key.map(|k| k.to_string()),
                suppressible: bump.suppressible,
            });
        }

        // Self-directed movement slow (Granite Armor −70% run, Hibernate) — a self penalty that
        // writes the movement globals DIRECTLY (no suppress group), unenhanced, `× -100`
        // (`character-totals.ts:1519`). Atom-native `self_slow_value`, `?? bag` for an atom-less
        // power. Both arms restrict to `toWho:Self` entries.
        //
        // The atom arm keys the axis exactly as the movement map above keys it, and that is the
        // point rather than a tidiness: a travel power's plus and its minus on one axis are two
        // halves of one authored pair that part company between the two maps. Rebirth's Group Fly
        // states `+0.5 / −0.5` and again `+0.5 / −0.5 IgnoreStrength`, and while both maps held one
        // value per axis they cancelled by luck. Split one and not the other and the plus is
        // counted without its minus. MOVEMAP-1.
        //
        // `movementCapDebuff` is spent HERE rather than through the cap projection above: ENT-5
        // moved the Maximum-aspect rows into their own slot so a cap debuff would stop
        // overwriting the speed debuff on the same axis, and these are the entries that were
        // reaching this total from inside `slow` before the split. Spending them here keeps the
        // totals where they were; the split is a display fix. It is atom-native as of
        // ATOM-BAG-4(c) — the reader agrees with the slot exactly, 1 / 0 / 0 carriers and zero
        // disagreements either way (`movement_cap_atom_bag_parity`).
        let to_scaled = |v: Vec<(&'static str, coh_data::slot_value::MovementValue)>| {
            v.into_iter()
                .map(|(field, m)| {
                    (
                        field,
                        Scaled {
                            scale: m.scale,
                            table: m.table.as_ref().map(|t| t.to_string()),
                            per_target: None,
                        },
                    )
                })
                .collect::<Vec<_>>()
        };
        let atom_slow = self_slow_value(self.power).map(to_scaled);
        let atom_cap_debuff = self_movement_cap_debuff_value(self.power).map(to_scaled);
        let self_movement_debuffs = atom_slow.into_iter().chain(atom_cap_debuff).flatten();
        for (field, scaled) in self_movement_debuffs {
            let value = -(resolve_scaled_effect(
                scaled.scale,
                scaled.table.as_deref(),
                self.archetype,
                self.level,
                self.db,
                self.errors,
            ) * 100.0);
            route_closed(g.add_by_camel_name(field, value), field);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn apply_active_power_bonuses(
    powers: &[ActivePower],
    g: &mut GlobalBonuses,
    archetype: &str,
    level: i32,
    strength: &StrengthBuffs,
    combat: &CombatContext,
    alpha: &AlphaEnhancement,
    stealth_contributions: &mut Vec<StealthContribution>,
    absorb_fraction_contribs: &mut Vec<AbsorbFractionContribution>,
    movement_contribs: &mut Vec<MovementContribution>,
    movement_cap_contribs: &mut Vec<MovementCapContribution>,
    res_self_debuffs: &mut Vec<ResSelfDebuff>,
    power_breakdown: &mut Vec<PowerBreakdownSource>,
    db: &PowerDatabase,
) {
    // Sink for named-AT-table lookup misses (crate::scaled) — corpus-unreachable, but a
    // typo'd/dropped table must land in `g.errors`, not resolve at a fabricated rate.
    let mut scaled_errors: Vec<CalcError> = Vec::new();
    let errors = &mut scaled_errors;
    // Attribution is DEFERRED by one iteration: the snapshot is taken here and diffed at the top
    // of the next pass (and once after the loop). An end-of-body diff would be equivalent TODAY —
    // every `continue` in the body belongs to a nested loop, so nothing short-circuits this one —
    // and measurably so: swapping this for the naive form leaves the corpus reconciliation green.
    // It is written this way because that equivalence is an accident of the current body rather
    // than a property anything enforces. An outer-level `continue` added later would make the
    // naive form drop that power's contributions silently, and silently is the failure mode this
    // ledger exists to prevent.
    let mut pending: Option<(&ActivePower, GlobalBonuses)> = None;
    for active in powers {
        if let Some((previous, before)) = pending.take() {
            record_power_deltas(previous, &before, g, power_breakdown);
        }
        pending = Some((active, g.clone()));
        // This power as THIS build has it. A power with no archetype fork — nearly all of
        // them — borrows through untouched; the two-armed pool defences narrow to the arm
        // this archetype gets, which is what stops Rebirth's Tough contributing both
        // (AT-FORK-1). The narrowing lives here rather than in the gather because every
        // family below re-reads `power.atoms` off the power itself.
        let narrowed = active.def.for_caster_class(db.class_name_of(archetype));
        let power: &Power = &narrowed;
        // The per-power stacking input (targets hit / stacks active). `None` — every M3
        // synthetic build bar the stacking probe — is the identity on the linear path and 0
        // targets on the per-target one ([`crate::stacking`]).
        // Ally-only buffs never touch the caster's own totals.
        if is_ally_only(power) {
            continue;
        }
        // Per-power enhancement multiplier source (the beta's per-power `enhBonuses`): the plain
        // slotted aggregation, or — when an active Alpha is equipped — the ED split that folds
        // Alpha's virtual enhancement into it (see [`power_enhancement`]).
        let enh = power_enhancement(active.slots, power, alpha, level, combat, db, errors);
        // The whole per-power state the families below share is gathered into one `WalkCtx` so
        // each family method takes `&self` rather than fourteen arguments. The family blocks are
        // extracted verbatim (each method is a literal copy of the original body with the
        // per-power identifiers bound to `self`); block order is load-bearing for exact f64
        // parity and is preserved exactly.
        let mut ctx = WalkCtx::new(
            power,
            active,
            archetype,
            level,
            db,
            errors,
            &enh,
            strength,
            combat,
            active.targets_hit,
        );

        // ToHit buff (enhanced + IgnoreStrength-unenhanced). See [`WalkCtx::apply_to_hit`].
        ctx.apply_to_hit(g);
        // Damage buff and its self-crash. See [`WalkCtx::apply_damage`].
        ctx.apply_damage(g);

        // Defense (always-on + suppressible + self-debuff). See [`WalkCtx::apply_defense`].
        ctx.apply_defense(g);
        // Resistance (buff + collected self-debuff). See [`WalkCtx::apply_resistance`].
        ctx.apply_resistance(g, res_self_debuffs);
        // Resources (regen, recovery, recharge, max-endurance). See [`WalkCtx::apply_resources`].
        ctx.apply_resources(g);
        // Affinity buffs (accuracy, perception, endurance-discount, range). See
        // [`WalkCtx::apply_affinity_buffs`].
        ctx.apply_affinity_buffs(g);
        // Absorb shield and Max HP. See [`WalkCtx::apply_shields_and_hp`].
        ctx.apply_shields_and_hp(g, absorb_fraction_contribs);
        // mez PROTECTION (atom-native — ATOM11 for the six MEZ types, ATOM15 for KB/KU). See
        // [`WalkCtx::apply_mez_protection`].
        ctx.apply_mez_protection(g);
        // mez RESISTANCE, per-type — atom-native (ATOM2 / PASS2B-13). See
        // [`WalkCtx::apply_mez_resistance`].
        ctx.apply_mez_resistance(g);
        // DEBUFF RESISTANCE, per-type — atom-native (ATOM13 / PASS2B-11). See
        // [`WalkCtx::apply_debuff_resistance`].
        ctx.apply_debuff_resistance(g);
        // === Pass 2b (bag-only): ELUSIVITY → Defense Debuff Resistance ===
        // A faithful mirror of the frozen calc's `elusivity → debuffResistDefense` routing
        // (`character-totals.ts:1399`): every entry — the KEY IS IGNORED — resolves
        // `resolveScaledEffect × 100 × enhMultiplier` and ACCUMULATES into `debuff_resist_defense`
        // (the same field Wave 11's `debuffResistance.defense` feeds), gated on `> 0`.
        // CORPUS-VACUOUS: the only `effects.elusivity` entries ever present were a hand-override
        // on Foresight + the Widow-Teamwork Elude that DUPLICATED
        // their `debuffResistance.defense` (`Base_Defense@Resistance`) value — a 2× DDR double-count.
        // That override was retired (the binary grants DDR once), so no dataset carries
        // `effects.elusivity` — zero carriers over all three forks. The mirror is gone with the
        // bag rather than kept over a slot nothing writes. The real `Elusivity`(aspect=Str) atoms
        // are a distinct PvP-defense stat, correctly unconsumed; should a fork ever author the
        // routing again it comes back as an atom reader, not a bag slot.

        // TAUNT / PLACATE RESISTANCE and STEALTH RADIUS. See [`WalkCtx::apply_mez_resistance_extra`].
        ctx.apply_mez_resistance_extra(g);
        ctx.apply_stealth(stealth_contributions);
        // MOVEMENT (buff map, travel ceiling, self-debuffs). See [`WalkCtx::apply_movement`].
        ctx.apply_movement(movement_contribs, movement_cap_contribs, g);
    }
    if let Some((previous, before)) = pending {
        record_power_deltas(previous, &before, g, power_breakdown);
    }
    g.errors.append(&mut scaled_errors);
}

/// The movement-map axis key → its [`MovementStat`] and the enhancement aspect that scales it
/// (`movementKeyMap` / `movementAspectMap`, `character-totals.ts:1479`). Unmapped keys (e.g. the
/// `fly` flight-mode grant) return `None` and contribute nothing.
/// The stat and enhancement aspect a movement axis key maps to, plus the axis' own sub type —
/// the STACK-4 discriminator the per-axis stack cap is narrowed by. A key naming no axis is
/// `None` and its entry is skipped by both loops, as before.
fn movement_axis(axis: &str) -> Option<(MovementStat, &'static str, SubType)> {
    match axis {
        "runSpeed" => Some((MovementStat::RunSpeed, "run", SubType::Run)),
        "flySpeed" => Some((MovementStat::FlySpeed, "fly", SubType::Fly)),
        "jumpHeight" => Some((MovementStat::JumpHeight, "jump", SubType::JumpHeight)),
        "jumpSpeed" => Some((MovementStat::JumpSpeed, "jump", SubType::Jump)),
        _ => None,
    }
}

/// The beta's `table.toLowerCase().includes('res_boolean')` guard — a resource buff on a
/// `Res_Boolean` table is a debuff-resistance boolean, not an actual regen/recovery buff,
/// and is skipped. Over a raw table name: the regen/recovery fallbacks normalize atom and
/// bag slots to `Option<&str>` before the check.
fn table_is_res_boolean(table: Option<&str>) -> bool {
    table.is_some_and(|t| t.to_ascii_lowercase().contains("res_boolean"))
}
