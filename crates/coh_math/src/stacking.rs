//! `adjustForStacking` / `adjustForPerTarget` (`character-totals.ts:547` / `:567`) — the
//! per-power stacking transform applied to a bag value BEFORE it is resolved.
//!
//! Two mechanics share one input, the per-power targets-hit count:
//!
//! * **AoE per-target** — an effect carrying `perTarget` grows with the number of targets the
//!   power hit: `scale + perTarget × (N − 1)`. Soul Drain's +Damage per foe.
//! * **Linear self-stacking** — an effect listed in the power's `stacksLinear` is multiplied by
//!   the stack count (capped): Build Up ×2, Siphon Speed's +Recharge ×2. Here the targets-hit
//!   input doubles as a STACK-COUNT input, which is why the beta comment calls it "the
//!   targets-hit slider doubles as a stack-count slider".
//!
//! Never both: `perTarget` short-circuits `stacksLinear`. The beta justifies this as a guard
//! against N² scaling on powers said to carry both (`character-totals.ts:562` names Soul Drain).
//! That justification does NOT hold for the shipped corpus: NO power in any dataset carries
//! `perTarget` on a `stacksLinear` effect key — Soul Drain has `perTarget` and no `stacksLinear`
//! entry at all (`scripts/survey-stacking.ts`, deleted 2026-09-25 with the `src/` tree it read;
//! DATA-GAP STACK-2). The short-circuit is ported
//! because it is the beta's behavior, but it is unreachable from corpus data, so no fixture can
//! grade it — only the unit test below does.
//!
//! The two paths read an ABSENT `targets_hit` differently, mirroring the beta:
//!
//! * per-target — absent means 0 foes (`targetsHit ?? 0`, `character-totals.ts:561`). The UI
//!   renders an untouched slider as "Off", so the calc must agree; an AoE buff that credits a
//!   phantom first target while the slider reads Off is the beta's own "defaults to 1-target
//!   values despite showing Off" bug.
//! * linear self-stacking — absent keeps the base 1-stack value. A recast self-buff is already
//!   applied once when the power is active; the slider only adds stacks 2..cap on top. Only an
//!   EXPLICIT 0 zeroes it.

use crate::projection::{extra_object, object_number};
use coh_data::atom::lands_on_caster;
use coh_data::slot_value::Scaled;
use coh_data::{AtomicEffect, EffectType, Power, Stacking, SubType};
use serde_json::{Map, Value};

/// Does this atom self-stack, and to what depth? — the converter's `detectSelfStacking` test
/// (`scripts/convert-powerset.cjs:7004`) read off the atom instead of the template it was
/// derived from. The converter qualifies on `target === 'Self'`, `stack ∈ {Stack,
/// RefreshToCount}` and `stack_limit > 1`; all three ride the wire as `toWho`, `stacking` and
/// `stackCap`, which is what retires the bag's `stacksLinear` / `maxStacks` / `stackCaps`
/// (ATOM-BAG-2). Measured against those slots corpus-wide: the largest qualifying cap
/// equals the bag's `maxStacks` on 92 / 142 / 134 powers with zero disagreements.
///
/// The recipient test is [`lands_on_caster`], which is WIDER than the converter's literal
/// `target === 'Self'` — it admits `SelfAndPets` and `TargetAndPets`, the reading TARGETS-2
/// settled as canonical. The census measured what that widening costs before it was adopted
/// here: the 20 / 4 / 3 powers the converter misses carry `GrantPower`/`Meta`/`Mez`/`Stealth`
/// atoms that reach no stacking consumer, so the wider test changes no total today and stops
/// being wrong the moment one of those families gains a reader.
fn self_stacks(atom: &AtomicEffect) -> bool {
    // A per-target atom is on the AoE path, and two independent rules say so: the converter
    // removes those keys from `stacksLinear` through its `excludeKeys` argument, and
    // [`adjust_for_stacking`] short-circuits a per-target value before the stack multiply can
    // reach it. Skipping them here makes the family agree with both instead of relying on the
    // second to undo what it claimed — Consume, Drain Psyche and the per-foe absorbs are the
    // corpus population.
    // A `redirect_base` atom is caster-side by the converter's construction — the redirect walk
    // collects only templates that land on the caster — even though its own recipient is the
    // redirect's `Target`. Siphon Power's +damage is one: `Stack` to 2, and with only the
    // anchored-recipient test its stack slider moved nothing.
    !atom.per_target.is_some_and(|increment| increment != 0.0)
        && (lands_on_caster(atom) || atom.redirect_base.is_some_and(|base| base != 0.0))
        && matches!(
            atom.stacking,
            Some(Stacking::Stack | Stacking::RefreshToCount)
        )
        && atom.stack_cap.is_some_and(|cap| cap > 1.0)
}

/// Which of a power's atoms a stacking question is about — the atom-native replacement for
/// passing a bag SLOT KEY to [`coh_data::slot_value::Bag::stacks_linear`] / [`coh_data::slot_value::Bag::stack_cap`].
///
/// The key list is gone rather than ported. `stacksLinear` membership was produced by
/// `classifyTemplateForStacking (`convert-powerset.cjs:7517`), a second atom→slot-key projection
/// running parallel to the converter's main one, and reproducing it here would have meant
/// maintaining that twin in a second language. It is not needed: an atom carries its own cap, so
/// the question "does this family stack, and how far" is answered by the family's own atoms.
/// [`Self::cap`] returning `Some` IS the membership answer.
///
/// The result is finer than the slots it replaces. `stackCaps[key] ?? maxStacks` reconstructs
/// per-key caps from two slots and needs the converter to have split them; the atoms state each
/// cap directly. Homecoming's Fortify_Mind is the one corpus power where those disagree — its
/// absorb caps at 2 while its debuff-resistance reaches 3 — and every arm below reads that off
/// the atoms with no `stackCaps` map involved.
///
/// **The variants are the appliers' own partition, not a tidier one.** A first cut keyed on
/// effect type alone and moved 434 / 442 (power, key) pairs against the bag, because two
/// discriminators the bag's key names encode were being collapsed: Build Up's +damage buff is an
/// `aspect: Str` atom (so excluding `Str` from a buff face silently unstacked it), and the
/// enhanceable and IgnoreStrength halves of ToHit / Regeneration / Recovery are separate slots
/// that differ on no field but `ignoreStrength`. `stacking_atom_census`'s A/B section is what
/// caught both, and it stays runnable so the next widening has to answer to it too.
///
/// **The third discriminator is the sub type, carried by `Buff` as an optional narrowing
/// (STACK-4).** A whole-family `cap` takes the MAX over every admitted atom, so one cap covered a
/// family whose axes disagreed — Thunderspy's Time Wall states its Run axis `Stack` (cap 2) and
/// its Fly / Jump axes `Replace`, and the planner multiplied all three by 2. The narrowing is
/// applied only where a consumer asks a per-sub-type question: the movement axes, the defense
/// positions and the resistance types each own a row, and they each pass their own sub type.
/// Every single-slot consumer passes `None` and reads exactly what it read before — a sub type's
/// cap is a max over a subset, so the finer key can only NARROW, and the census's STACK-4 A/B
/// section is the standing record of what it narrowed on each fork.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum StackFamily {
    /// The buff face of one effect type, optionally narrowed to one enhancement half.
    ///
    /// Defined as the leftovers, so the three variants partition the atom stream by construction
    /// instead of by three filters that have to be kept consistent by hand. An atom of this type
    /// is its buff unless one of the other two variants has already claimed it.
    ///
    /// Both exclusions are the bag's own partition rather than a tidying, and both were measured
    /// wrong in a simpler spelling first. A `ToHit|Res` atom is toHit-DEBUFF-resistance — but a
    /// `Resistance|Res` atom is ordinary damage resistance, so "exclude Res" unstacked five
    /// armour tier-9s. An `Absorb|Str` atom is a `specialBuff` strength meta — but a
    /// `DamageBuff|Str` atom is Build Up's +damage, because the converter's `specialBuff` block
    /// skips the `_dmg`, accuracy and rechargetime attribs (`convert-powerset.cjs:5020`), so
    /// "admit Str" and "exclude Str" each get one of those two wrong.
    ///
    /// That second exclusion is a carve-out with a carve-out, and the converter authored both:
    /// its `specialBuff` block skips the `_dmg`, accuracy and rechargetime strength attribs
    /// (`convert-powerset.cjs:5020`) so they fall through to their own buff key. So Build Up's
    /// `DamageBuff|Str` +damage belongs HERE, while Power Boost's `Absorb|Str` and `Movement|Str`
    /// belong to `SpecialBuff` — and a flat "exclude Str" or "admit Str" rule gets one of those
    /// two wrong. Both spellings were measured wrong by the census A/B before this one stood.
    ///
    /// The `Option<SubType>` is the STACK-4 narrowing: `None` asks about the whole family, and
    /// `Some(sub)` asks about the row that sub type owns. An atom answers the narrowed question
    /// when it names the sub type itself or `All` — the export's own spelling of "every one of
    /// them": Homecoming states Personal Force Field as eleven typed defense rows where the two
    /// Parse6 forks state the identical value once as `Defense/All`, so an `All` atom IS the
    /// typed atom of each position it covers. An atom naming NO sub type answers only the whole-
    /// family question — it owns no row.
    Buff(EffectType, Half, Option<SubType>),
    /// The bag's `debuffResistance` — one slot and one cap across a power's defense-debuff-
    /// resistance and its recharge-debuff-resistance alike. Membership is
    /// [`crate::appliers::debuff_resistance::is_debuff_resistance_atom`], the applier's own
    /// eight-route list, NOT `aspect: Res` on its own.
    DebuffResistance,
    /// The bag's `specialBuff`: the +Strength self-buffs, restricted to the effect types that
    /// actually reach a `specialBuff` key ([`crate::strength::is_special_buff_atom`]). The
    /// restriction is the converter's, not a tightening invented here — an `aspect: Str` atom on
    /// a type with no `specialBuff` key is the strength meta-template METHOD-2 filters, and
    /// crediting it would stack a value no consumer reads.
    SpecialBuff,
}

/// Which enhancement half of a buff face a stacking question is about — the discriminator behind
/// the bag's twinned slot names (`tohitBuff` vs `tohitBuffUnenhanced`).
///
/// [`Half::Either`] is for the families the bag never split, and it is the honest spelling for
/// them: asking for a half that does not exist would answer `None` for every power.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Half {
    /// `ignoreStrength` absent or false — the enhanceable slot.
    Enhanceable,
    /// `ignoreStrength: true` — the `*Unenhanced` twin.
    Unenhanced,
    /// The family has one slot; take both halves.
    Either,
}

impl StackFamily {
    fn admits(self, atom: &AtomicEffect) -> bool {
        match self {
            Self::Buff(effect_type, half, sub_type) => {
                atom.effect_type == Some(effect_type)
                    && !Self::DebuffResistance.admits(atom)
                    && !crate::strength::is_special_buff_atom(atom)
                    // A Defiance rider is a `DamageBuff` atom the damage applier rejects — the
                    // scaling-with-health mechanic, not a buff the power confers — so it must not
                    // decide whether that power's +damage stacks either. Every plain attack in
                    // the corpus carries one, which is why leaving it in widened `damageBuff`
                    // across most of the attack corpus in the census A/B.
                    && !crate::appliers::damage::is_defiance(atom)
                    // The STACK-4 narrowing. `All` admits everywhere the whole family does —
                    // see the variant doc — while a sub type with no atom naming it (and no
                    // `All` standing in) answers `None` from [`Self::cap`], which is the row
                    // not stacking, not a cap of zero.
                    && match sub_type {
                        Some(sub) => atom.sub_type == Some(sub) || atom.sub_type == Some(SubType::All),
                        None => true,
                    }
                    && match half {
                        Half::Enhanceable => atom.ignore_strength != Some(true),
                        Half::Unenhanced => atom.ignore_strength == Some(true),
                        Half::Either => true,
                    }
            }
            Self::DebuffResistance => {
                crate::appliers::debuff_resistance::is_debuff_resistance_atom(atom)
            }
            Self::SpecialBuff => crate::strength::is_special_buff_atom(atom),
        }
    }

    /// The stack depth this family reaches on `power`, or `None` when nothing in it self-stacks
    /// — the same answer `stacksLinear.includes(key)` used to give, sourced from the atoms
    /// instead of from a list the converter wrote.
    ///
    /// MAX rather than sum or first: two atoms of one family disagreeing on their cap is the
    /// power stacking to the deeper of them, and taking the max is what reproduced `maxStacks`
    /// exactly across all three forks.
    pub fn cap(self, power: &Power) -> Option<f64> {
        power
            .atoms
            .iter()
            .filter(|atom| self.admits(atom) && self_stacks(atom))
            .filter_map(|atom| atom.stack_cap)
            .reduce(f64::max)
    }

    /// Which family a BAG SLOT KEY names, or `None` for a slot no stacking question is about.
    ///
    /// The totals path needs no such map — each call site knows its own family and passes it. The
    /// display path does: it transforms a JSON bag keyed by slot name, one row at a time, with no
    /// atom in hand to ask. So the question "does this row stack" can only be answered by naming
    /// which atoms the row is made of.
    ///
    /// **This is not `classifyTemplateForStacking` ported.** That is the converter's atom→key
    /// projection over five attrib maps, and reproducing it in a second language is what the
    /// migration declined. This runs the other way — key→family, two dozen static keys — and most
    /// of it is the pairing `stacking_atom_census` was already asserting as a literal of its own.
    /// The census now imports this map rather than keeping that copy, so its A/B grades what
    /// production reads.
    ///
    /// The keys the census had no reason to carry are the ones with no `stack()` call site on the
    /// totals path, which only the display reaches because only the display iterates every row.
    /// `enduranceGain` and `maxHPBuff` are 51 / 58 / 48 and 17 / 19 / 11 powers of the corpus
    /// `stacksLinear`, so a map assembled from the migrated call sites would have looked complete
    /// while unstacking a fifth of the population without a word. They were found by measuring the
    /// key vocabulary, not by reading the call sites, and `display_stacking_atom_native` keeps that
    /// check standing.
    ///
    /// The movement axes appear under both spellings the display bag carries. `buildDisplayEffects`
    /// FLATTENS the nested `movement` object into top-level `runSpeed` / `fly` / `jumpSpeed` /
    /// `jumpHeight` keys and leaves the nested one standing beside them, and the converter only
    /// ever listed `movement` — so the rows the surface actually renders were the ones not
    /// stacking. All of them take `Half::Either`, matching the single `Buff(Movement, Either)` the
    /// totals path stacks the whole map with: a lone `IgnoreStrength` movement entry keeps the
    /// plain key (MOVEMAP-1), so an enhanceable/unenhanced split read off the key NAME would be
    /// wrong for exactly the powers that carry one.
    ///
    /// The four axis keys carry their own sub type (STACK-4) — the row `runSpeed` renders is the
    /// Run axis' row, and it stacks to the Run axis' depth, not the family's. The nested
    /// `movement` key keeps the whole family: it owns no axis of its own, and its children are
    /// re-resolved per row by [`Self::for_nested_key`] instead of inheriting the parent's cap.
    /// The defense and resistance keys keep the whole family for the same reason — their children
    /// are the per-position / per-type rows, and the re-resolution happens on the child, where the
    /// key names the sub type.
    pub fn for_bag_key(key: &str) -> Option<Self> {
        Some(match key {
            "tohitBuff" => Self::Buff(EffectType::ToHit, Half::Enhanceable, None),
            "tohitBuffUnenhanced" => Self::Buff(EffectType::ToHit, Half::Unenhanced, None),
            "damageBuff" => Self::Buff(EffectType::DamageBuff, Half::Either, None),
            "defenseBuff" => Self::Buff(EffectType::Defense, Half::Either, None),
            "resistance" => Self::Buff(EffectType::Resistance, Half::Either, None),
            "regenBuff" => Self::Buff(EffectType::Regeneration, Half::Enhanceable, None),
            "regenBuffUnenhanced" => Self::Buff(EffectType::Regeneration, Half::Unenhanced, None),
            "recoveryBuff" => Self::Buff(EffectType::Recovery, Half::Enhanceable, None),
            "recoveryBuffUnenhanced" => Self::Buff(EffectType::Recovery, Half::Unenhanced, None),
            "rechargeBuff" => Self::Buff(EffectType::RechargeTime, Half::Either, None),
            "accuracyBuff" => Self::Buff(EffectType::Accuracy, Half::Either, None),
            "rangeBuff" => Self::Buff(EffectType::Range, Half::Either, None),
            "absorb" => Self::Buff(EffectType::Absorb, Half::Either, None),
            "maxHPBuff" => Self::Buff(EffectType::MaxHp, Half::Enhanceable, None),
            "maxHPBuffUnenhanced" => Self::Buff(EffectType::MaxHp, Half::Unenhanced, None),
            "enduranceGain" => Self::Buff(EffectType::Endurance, Half::Either, None),
            // Conserve Power / Conserve Energy: an IgnoreStrength `Stack / limit 2` self row,
            // and the bag gives the discount one key rather than an enhanceable/unenhanced pair,
            // so `Either` is the whole family. It reached this map only when STACK-6 took the
            // key off the router — the converter's own stacking classifier had been naming
            // `specialBuff` for it, which is a slot the power has no value under.
            "enduranceDiscount" => Self::Buff(EffectType::EnduranceDiscount, Half::Either, None),
            "debuffResistance" => Self::DebuffResistance,
            "specialBuff" => Self::SpecialBuff,
            "movement" => Self::Buff(EffectType::Movement, Half::Either, None),
            "runSpeed" | "runSpeedUnenhanced" => {
                Self::Buff(EffectType::Movement, Half::Either, Some(SubType::Run))
            }
            "fly" => Self::Buff(EffectType::Movement, Half::Either, Some(SubType::Fly)),
            "jumpSpeed" => Self::Buff(EffectType::Movement, Half::Either, Some(SubType::Jump)),
            "jumpHeight" => Self::Buff(
                EffectType::Movement,
                Half::Either,
                Some(SubType::JumpHeight),
            ),
            _ => return None,
        })
    }

    /// The family a CHILD key of a per-type bag object narrows to, or `None` when the parent owns
    /// no per-sub-type partition and the child inherits its parent's reading instead.
    ///
    /// The parent's own [`Self::for_bag_key`] family is the whole family on purpose — `movement`,
    /// `defenseBuff` and `resistance` name no sub type — so carrying that cap down to the children
    /// would answer Time Wall's Fly row with the Run axis' cap. The child key is where the sub
    /// type lives: the movement object's children are the axis keys (the applier's own
    /// spellings, `flySpeed` included — the display's flattened `fly` rename does not reach into
    /// the nested object), and the defense / resistance children are lowercase wire sub types.
    pub fn for_nested_key(parent: &str, child: &str) -> Option<Self> {
        match parent {
            "movement" => Self::movement_axis_sub_type(child)
                .map(|sub| Self::Buff(EffectType::Movement, Half::Either, Some(sub))),
            "defenseBuff" => SubType::from_wire_lower(child)
                .map(|sub| Self::Buff(EffectType::Defense, Half::Either, Some(sub))),
            "resistance" => SubType::from_wire_lower(child)
                .map(|sub| Self::Buff(EffectType::Resistance, Half::Either, Some(sub))),
            _ => None,
        }
    }

    /// The sub type a movement axis key names — both spellings, the applier's (`flySpeed`) and
    /// the display's flattened rename (`fly`). A key naming no axis is `None`, which the display
    /// reads as "inherit the parent's reading" rather than as an unstackable row.
    fn movement_axis_sub_type(axis: &str) -> Option<SubType> {
        match axis {
            "runSpeed" => Some(SubType::Run),
            "fly" | "flySpeed" => Some(SubType::Fly),
            "jumpSpeed" => Some(SubType::Jump),
            "jumpHeight" => Some(SubType::JumpHeight),
            _ => None,
        }
    }
}

/// The deepest stack any of a power's atoms reaches — the atom-native `effects.maxStacks`.
///
/// Unfiltered by family on purpose: `maxStacks` is a power-wide claim, and the census measured
/// this exact reduction against the shipped slot at **92 / 142 / 134 powers, zero disagreements**.
/// The only powers it drops are the ones with no self-stacking atom at all, which is ATOM-BAG-3's
/// four: the bag's `maxStacks 7` there counts a DELAY SCHEDULE — one `Replace` shield re-applied
/// every few seconds — and no clock has a slot on the bag to be recorded in.
pub fn max_stack_cap(power: &Power) -> Option<f64> {
    power
        .atoms
        .iter()
        .filter(|atom| self_stacks(atom))
        .filter_map(|atom| atom.stack_cap)
        .reduce(f64::max)
}

/// Is the caster himself one of the entities his own AoE counts — so that N is never 0?
///
/// Both terms are the ones `computeAoePerTargetPatches` reads to mint the stamp in the first
/// place, and reading them back is what keeps the floor and the value it floors on one story:
///
/// * The geometry — `effectArea` is a sphere or a cone with a bounded `maxTargets` — is that
///   pass's own `isAoEWithTargets` gate. It matters because `per_target` also reaches atoms from
///   the `Execute_Power` redirect branch, where the increment counts something else entirely
///   (Reactive Regeneration's stacks count how recently you were hit, on a `SingleTarget` toggle),
///   and a floor there would assert a combat state rather than a seat in a sphere.
/// * The recipient list — the converter's `selfIsCountedTarget`, [`Power::affects_caster`] — is
///   what makes the caster one of those seats. It is the same term `firstTargetExcluded` uses to
///   decide the value AT one target, so the two halves of "N = 1 is solo" now answer from one
///   field instead of one of them being left unsaid.
///
/// A power failing either term keeps the foe-aura reading: it reaches its caster only through a
/// target, so an absent count is genuinely nobody hit and the buff does not fire (PROD6B-2d)
/// — unless [`aim_guarantees_a_target`] answers the same question from the other side.
pub fn caster_occupies_a_target_slot(power: &Power) -> bool {
    power.affects_caster() && bounded_aoe_entity_count(power)
}

/// The geometry half of both floors: `effectArea` is a sphere or a cone and `stats.maxTargets` is
/// bounded above one — `computeAoePerTargetPatches`' own `isAoEWithTargets` gate, read back.
///
/// It is what makes N an entity count rather than something else wearing the same field.
/// `per_target` also reaches atoms from the `Execute_Power` redirect branch, where the increment
/// counts how recently the caster was hit (Reactive Regeneration, a `SingleTarget` toggle), and a
/// floor there would assert a combat state instead of a seat in a sphere.
fn bounded_aoe_entity_count(power: &Power) -> bool {
    // A redirect's sphere counts the foes instead of the power's own (Fulcrum Shift): still an
    // entity count, just stated one hop away.
    if redirect_targets_per_cast(power).is_some() {
        return true;
    }
    let area = power.extra.get("effectArea").and_then(Value::as_str);
    if !matches!(area, Some("AoE" | "Cone")) {
        return false;
    }
    object_number(extra_object(power, "stats"), "maxTargets")
        .is_some_and(|max| max > 1.0 && max != UNBOUNDED_MAX_TARGETS)
}

/// Can this power have been used at all without reaching anybody — or does its own aim rule out
/// the empty count before the geometry is consulted?
///
/// [`Power::aim_requires_an_entity`] is the other way a per-foe count cannot be zero, and it is
/// the CLICK shape rather than the aura one. A foe-aimed power is refused at activation with no
/// target entity, and a cone range-checks that entity as well, so a build saying "I use this"
/// is saying at least one foe was in front of it. Guarded Spin is the corpus case: a Staff
/// Fighting cone whose +Def(Melee, Lethal) is one `kStackType_Stack` mod aimed at the caster,
/// which the game applies once per foe the cone lands on — so the growth is real, and zero was
/// the one count the power could not be at (PERFOE-4).
///
/// The geometry term is the same one [`caster_occupies_a_target_slot`] asks, and for the same
/// reason: without it this would floor a count that is not counting entities.
pub fn aim_guarantees_a_target(power: &Power) -> bool {
    power.aim_requires_an_entity() && bounded_aoe_entity_count(power)
}

/// Is zero a count this power can be at? The union of the two reasons it cannot be — the caster
/// holding a seat in his own sphere ([`caster_occupies_a_target_slot`]) and the aim refusing to
/// fire at nobody ([`aim_guarantees_a_target`]) — and the one predicate every apply site asks.
pub fn per_target_count_cannot_be_zero(power: &Power) -> bool {
    caster_occupies_a_target_slot(power) || aim_guarantees_a_target(power)
}

/// Homecoming's over-cap block (`stats.overCapTrigger` / `overCapMultiplier` /
/// `overCapExponential`): past the trigger, each further target's per-target effects are
/// scaled by the multiplier — or by the multiplier raised to how far past the trigger it is,
/// when exponential. The Tanker AoEs use it to hit past the old cap at a third of the effect.
///
/// It reaches effects aimed at the caster too, which is the one place the planner can see it.
/// Radiation Therapy's help text is the witness: trigger 1, multiplier 0.3 on a 9.46s per-foe
/// recharge row is "the first target hit will increase its recharge by 9.5 with additional
/// targets adding 2.8 seconds". The exponential reading (`multiplier ^ excess`) has no such
/// witness: Aura of Insanity is the only power carrying it, and nothing here reads its effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverCap {
    pub trigger: u32,
    pub multiplier: f64,
    pub exponential: bool,
}

impl OverCap {
    /// The power's over-cap, or `None` when it states no trigger.
    pub fn of(power: &Power) -> Option<Self> {
        let stats = extra_object(power, "stats");
        let trigger = object_number(stats, "overCapTrigger").filter(|t| *t >= 1.0)? as u32;
        Some(OverCap {
            trigger,
            multiplier: object_number(stats, "overCapMultiplier").unwrap_or(1.0),
            exponential: stats
                .and_then(|stats| stats.get("overCapExponential"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// How much of its per-target effect the `k`-th target (1-based) receives.
    fn weight(&self, k: u32) -> f64 {
        if k <= self.trigger {
            1.0
        } else if self.exponential {
            self.multiplier.powi((k - self.trigger) as i32)
        } else {
            self.multiplier
        }
    }
}

/// `n` targets, counted as the per-target effects see them: `n` itself with no over-cap, and the
/// sum of each target's [`OverCap::weight`] with one.
pub fn weighted_count(over_cap: Option<OverCap>, n: u32) -> f64 {
    match over_cap {
        Some(over_cap) => (1..=n).map(|k| over_cap.weight(k)).sum(),
        None => f64::from(n),
    }
}

/// How a power counts the targets its per-target effects grow with: whether zero is a count it
/// can be at ([`per_target_count_cannot_be_zero`]), how targets past its over-cap count, and —
/// for a power whose foes are counted by a redirect's sphere — how many one cast can reach.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TargetCount {
    pub cannot_be_zero: bool,
    pub over_cap: Option<OverCap>,
    /// [`redirect_targets_per_cast`]: set, a count above it is a second cast.
    pub per_cast: Option<u32>,
}

/// [`TargetCount`] for `power` — what every per-target apply site passes.
pub fn target_count(power: &Power) -> TargetCount {
    TargetCount {
        cannot_be_zero: per_target_count_cannot_be_zero(power),
        over_cap: OverCap::of(power),
        per_cast: redirect_targets_per_cast(power),
    }
}

/// The per-foe count's ceiling for ONE cast, when the foes are counted by an `Execute_Power`
/// redirect's sphere rather than the power's own geometry — the converter's
/// `perTargetMaxTargets` stamp.
///
/// Fulcrum Shift is the population: a single-target shell, so it states no `maxTargets`, whose
/// `Redirects.Kinetics.KineticTransfer` sphere hits up to 10 foes and runs the +damage buff once
/// for each. The fan-out lives in another power's file, so only the converter can read it.
pub fn redirect_targets_per_cast(power: &Power) -> Option<u32> {
    power
        .extra
        .get("perTargetMaxTargets")
        .and_then(Value::as_f64)
        .filter(|max| *max > 1.0)
        .map(|max| max as u32)
}

/// How many casts of a redirect-counted power can be standing at once: the deepest stack its
/// per-foe atoms state. Fulcrum Shift's per-foe buff is `Stack` with a limit of 2, so a second
/// cast inside the buff's duration adds a second set of foe buffs and a second base.
fn redirect_cast_depth(power: &Power) -> u32 {
    power
        .atoms
        .iter()
        .filter(|atom| atom.per_target.is_some_and(|increment| increment != 0.0))
        .filter(|atom| {
            matches!(
                atom.stacking,
                Some(Stacking::Stack | Stacking::RefreshToCount)
            )
        })
        .filter_map(|atom| atom.stack_cap)
        .reduce(f64::max)
        .map_or(1, |cap| cap.max(1.0) as u32)
}

/// AoE per-target growth (`character-totals.ts:547`). `per_target` is the caller's already
/// non-zero increment. `N = 1` is the unmodified value; `N ≥ 2` adds `per_target × (N − 1)`;
/// `N = 0` means the sphere landed on nobody → the buff does not fire (scale 0).
///
/// An ABSENT count is 0, not the identity (the beta's `targetsHit ?? 0`). A foe aura reaches the
/// caster only THROUGH a target: its `EntsAffected` lists foes alone, so with nobody in radius no
/// effect block runs at all — Invincibility's authored help says as much ("the first foe you
/// engage in melee grants the highest Defense bonus"), and its base `kReplace` mod sits in the
/// same foe-driven block as the `kContinuous` increment. Treating absent as the N=1 value
/// credited a phantom first target on every such power (PROD6B-2d).
///
/// `count_cannot_be_zero` is [`per_target_count_cannot_be_zero`] — whether N = 0 is a state this
/// power can be in at all. Two shapes say it is not, and both floor the count at one target:
///
/// * The caster holds a seat in his own sphere. Phalanx Fighting is the case
///   (`EntsAffected kLeaguemate, kCaster`, base `Replace` 0.5 beside a `target ≠ source` increment
///   of 0.3): with no ally in radius the game still gives its 5% melee/ranged/AoE defence, and
///   reading the untouched slider as zero deleted that base (PERFOE-3).
/// * The aim refuses to fire at nobody. Guarded Spin is the case: a foe-aimed Staff Fighting cone
///   whose +Def(Melee, Lethal) the game stacks once per foe the cone lands on, showing nothing
///   whatever the owner did with its toggle because the slider it needed starts at "Off"
///   (PERFOE-4).
fn adjust_for_per_target(
    value: &Scaled,
    per_target: f64,
    targets_hit: Option<u32>,
    count: TargetCount,
) -> Scaled {
    let n = targets_hit
        .unwrap_or(0)
        .max(u32::from(count.cannot_be_zero));
    if n == 0 {
        return Scaled {
            scale: 0.0,
            ..value.clone()
        };
    }
    if n == 1 {
        return value.clone();
    }
    // A redirect-counted power reaches at most `per_cast` foes per cast, so a count past it is a
    // second cast standing beside the first — and each cast brings its own base as well as its
    // own foe buffs. The count is read as the fewest casts that reach it (12 foe buffs at 10 per
    // cast is two casts). `scale - per_target` is one cast's base: `scale` is the value at one
    // foe, and that one foe's buff is already in it.
    if let Some(per_cast) = count.per_cast.filter(|per_cast| n > *per_cast) {
        let casts = f64::from(n.div_ceil(per_cast));
        return Scaled {
            scale: casts * (value.scale - per_target) + per_target * f64::from(n),
            ..value.clone()
        };
    }
    Scaled {
        scale: value.scale + per_target * (weighted_count(count.over_cap, n) - 1.0),
        ..value.clone()
    }
}

/// The combined stacking adjustment (`character-totals.ts:567`). `stacks_linear` and `stack_cap`
/// are this value's already-resolved metadata, and every caller now derives both from
/// [`StackFamily::cap`] — `Some(cap)` is the membership answer and the depth at once. The pair
/// survives as two parameters because the beta's shape has them, and because the uncapped branch
/// below is only reachable by a caller that could answer "stacks" without a depth.
///
/// A `perTarget` effect takes the AoE path and IGNORES `stacks_linear` — the beta's
/// short-circuit against N² scaling. Otherwise a `stacks_linear` effect is multiplied by the
/// stack count capped at `stack_cap`; everything else is returned unchanged.
///
/// `count` reaches the AoE path alone. Its floor is NOT a floor on a stack depth: N there counts
/// applications of a self-buff, and a click the build has not fired is at zero stacks however
/// the power addresses or aims at anybody.
///
/// The beta's `typeof value !== 'object'` guard (a bare number is exempt from the stack
/// multiply) is NOT reproduced: [`Scaled`] flattens a bare number to `{scale, table:None}`, and
/// no corpus data reaches that branch — ZERO bare-number values exist for any `stacksLinear`
/// effect key in any dataset (`scripts/survey-stacking.ts`; DATA-GAP STACK-1).
pub fn adjust_for_stacking(
    value: &Scaled,
    targets_hit: Option<u32>,
    stacks_linear: bool,
    stack_cap: Option<f64>,
    count: TargetCount,
) -> Scaled {
    // `!!value.perTarget` — a zero/absent perTarget is falsy and falls through to stacking.
    if let Some(per_target) = value.per_target.filter(|p| *p != 0.0) {
        return adjust_for_per_target(value, per_target, targets_hit, count);
    }
    // Explicit 0 = no stacks active (the power whiffed) — but only for a stacking effect;
    // otherwise 0 falls through the beta's `!targetsHit` guard and returns the value as-is.
    if targets_hit == Some(0) && stacks_linear {
        return Scaled {
            scale: 0.0,
            ..value.clone()
        };
    }
    let Some(n) = targets_hit else {
        return value.clone();
    };
    if n <= 1 || !stacks_linear {
        return value.clone();
    }
    let capped = match stack_cap {
        Some(cap) if cap > 0.0 => f64::from(n).min(cap),
        _ => f64::from(n),
    };
    Scaled {
        scale: value.scale * capped,
        ..value.clone()
    }
}

// ---------------------------------------------------------------------------
// the DISPLAY half — `buildDisplayEffects.ts`'s `withTargetsHit` (PROD6C-3b)
// ---------------------------------------------------------------------------
//
// The totals path above transforms one bag VALUE as the accumulator reaches it. The display
// transforms the whole DISPLAY BAG before `RegistryEffectsDisplay` resolves it, and reads the
// same targets-hit input through two extra gates the totals path has no equivalent of:
//
// * The power must SHOW a stacking slider ([`has_stacking_slider`]) — a power whose data
//   carries `perTarget` but whose `maxTargets` is absent or unbounded renders no slider, so
//   the display leaves its rows alone however the (non-unique) internalName key was set.
// * `targetsHit > 1` is the identity. The totals path instead reads absent/0 as ZERO foes and
//   zeroes a `perTarget` buff (PROD6B-2d); the display never zeroes a row, it only grows one.
//   The asymmetry is the beta's, on the surface it is being graded against.

/// `maxTargets` sentinel for an unbounded AoE — no slider, since there is no axis to drag.
const UNBOUNDED_MAX_TARGETS: f64 = 255.0;

/// Does this power render a stacking slider (the beta `getStackingInfo`)? Per-target scaling
/// needs a bounded `maxTargets` to be an axis; otherwise a stack depth makes the slider a stack
/// count.
///
/// The stack arm reads [`max_stack_cap`] rather than the bag's `maxStacks`, which is the same
/// writer that put the phantom in `stacksLinear`: `absorbStackCount` records a delay schedule as
/// a depth, so the four ATOM-BAG-3 powers offered a seven-position slider for a shield that never
/// doubles. Migrating the rows without the gate would have left the slider standing and made it
/// scale nothing, which is the worse of the two states.
///
/// The per-target arm reads the ATOMS, and it is the same field [`crate::window_slots`] mints the
/// bag's `perTarget` from — so the gate and the value it gates now answer from one place. It read
/// the authored `effects` object until STACKINFO-1, which was sound only while that object was
/// the display bag's source: the seed moved to the atoms (ENGLAG-1) and then the strip emptied the
/// authored bag, leaving a gate that consulted a witness with nothing to say. It said no on every
/// power of every fork — `perTarget` survives in ZERO authored bags corpus-wide — so a per-foe row
/// grew only where the power ALSO carried a self-stacking atom and fell through to the arm below.
/// Soul Drain has one and worked by accident; the 18 / 21 / 25 / 18 powers that do not (Invincibility,
/// Against All Odds, Phalanx Fighting, Consume, Drain Psyche, …) rendered a one-target row at every
/// slider setting.
fn has_stacking_slider(power: &Power) -> bool {
    stacking_slider(power).is_some()
}

/// What a power's targets-hit input counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderKind {
    /// Foes (or allies) the AoE landed on — the per-target path. Absent reads as zero.
    Targets,
    /// Applications of a self-stacking buff — the linear path. Absent reads as one stack.
    Stacks,
}

/// The targets-hit input a power offers: what it counts, and its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackingSlider {
    pub kind: SliderKind,
    /// The lowest count the power can be at — 1 where [`per_target_count_cannot_be_zero`]
    /// says nobody-hit is not a state it can be in, 0 otherwise.
    pub min: u32,
    pub max: u32,
    /// Foes one cast reaches, when [`Self::max`] spans more than one cast
    /// ([`redirect_targets_per_cast`]); `None` for every other slider.
    pub per_cast: Option<u32>,
}

impl StackingSlider {
    /// The count the engine reads for an input of `targets_hit`, so a control shows the value
    /// that is actually applied rather than the raw (possibly absent) input.
    pub fn effective(&self, targets_hit: Option<u32>) -> u32 {
        let default = match self.kind {
            SliderKind::Targets => 0,
            SliderKind::Stacks => 1,
        };
        targets_hit.unwrap_or(default).clamp(self.min, self.max)
    }
}

/// The targets-hit input this power offers, or `None` when the count moves nothing on it (the
/// beta `getStackingInfo`). Per-target scaling needs a bounded `maxTargets` to be an axis;
/// otherwise a self-stacking depth makes the input a stack count.
pub fn stacking_slider(power: &Power) -> Option<StackingSlider> {
    if carries_per_target(power) {
        // A redirect-counted power (Fulcrum Shift) counts foe buffs across every cast that can
        // stand at once: 10 foes per cast, two casts deep, is 20.
        if let Some(per_cast) = redirect_targets_per_cast(power) {
            return Some(StackingSlider {
                kind: SliderKind::Targets,
                min: u32::from(per_target_count_cannot_be_zero(power)),
                max: per_cast * redirect_cast_depth(power),
                per_cast: Some(per_cast),
            });
        }
        // The slider's bound comes from the power's own `stats`, which is where the beta reads
        // it — NOT the display bag, whose `maxTargets` the stats merge also fills for a
        // pool/epic power that carries no `stats` at all.
        // An absent bound is falsy on the beta's own `maxTargets &&` guard, so it reads as no
        // axis rather than as one target.
        let max_targets = object_number(extra_object(power, "stats"), "maxTargets").unwrap_or(0.0);
        if max_targets > 1.0 && max_targets != UNBOUNDED_MAX_TARGETS {
            return Some(StackingSlider {
                kind: SliderKind::Targets,
                min: u32::from(per_target_count_cannot_be_zero(power)),
                max: max_targets as u32,
                per_cast: None,
            });
        }
    }
    // A power whose recharge grows per foe hit has the same axis even with no per-foe buff
    // (Cinders, Synaptic Overload): the count moves its recharge.
    if let Some(rule) = crate::adaptive_recharge::adaptive_recharge(power) {
        return Some(StackingSlider {
            kind: SliderKind::Targets,
            min: 0,
            max: rule.max_targets,
            per_cast: None,
        });
    }
    max_stack_cap(power).map(|cap| StackingSlider {
        kind: SliderKind::Stacks,
        min: 0,
        max: cap as u32,
        per_cast: None,
    })
}

/// Does this power carry a per-foe increment at all — the whole-power half of [`has_per_target`],
/// asked of the atoms rather than of a bag.
///
/// A zero increment is not one: `apply_patch` writes `perTarget: 0` onto a slot it rebuilt for
/// some other reason, so bag PRESENCE and a real per-foe axis are different questions. The corpus
/// does not currently separate them — every power with a `per_target` stamp has a non-zero one on
/// all four forks — which is exactly why the stricter test is the one to hold: it cannot start
/// being wrong the way the looser one would.
fn carries_per_target(power: &Power) -> bool {
    power
        .atoms
        .iter()
        .any(|atom| atom.per_target.is_some_and(|increment| increment != 0.0))
}

/// The beta `hasPerTargetField`: the value carries `perTarget`, or one of its immediate
/// by-type sub-objects does. One level deep, as the beta checks it — a `perTarget` nested
/// deeper is not what suppresses the stack multiply.
///
/// A MaxHP-fraction absorb spells the same field `maxHPFractionPerTarget` (PROD6C-3j): its
/// magnitude is an Expression with no scale for a `perTarget` to ride, and the beta — whose
/// converter recovers that magnitude as a `_ones` scale instead — carries a plain `perTarget`
/// on the very same slot. Reading only one spelling would render a slider on one surface and
/// not the other for one power.
fn has_per_target(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    // A zero is not an increment: `apply_patch` writes `perTarget: 0` onto a slot the redirect
    // branch rebuilt for its base alone (Siphon Power), and reading that as per-foe kept the
    // card's +damage from stacking while the totals did — [`carries_per_target`]'s rule.
    fn carries(object: &Map<String, Value>) -> bool {
        ["perTarget", "maxHPFractionPerTarget"]
            .iter()
            .any(|key| object.get(*key).is_some_and(|v| v.as_f64() != Some(0.0)))
    }
    carries(object) || object.values().filter_map(Value::as_object).any(carries)
}

/// Apply the per-power targets-hit input to a DISPLAY bag in place (the beta
/// `withTargetsHit` / `adjustEffectsForTargets`, `buildDisplayEffects.ts`). Each registered
/// effect's value grows by its own `perTarget` increment, or — for an effect the power lists in `stacksLinear` and that carries
/// no `perTarget` at all — is multiplied by the capped stack count.
///
/// The stacking metadata comes from the power's ATOMS, through the row's own
/// [`StackFamily::for_bag_key`], and no longer from the authored bag's `stacksLinear` /
/// `stackCaps` / `maxStacks`. Those three were the totals path's source until ATOM-BAG-2 replaced
/// them, and this was the last reader — a KNOWN-wrong one since ATOM-BAG-3, because the writer
/// behind them counts a delay schedule as a stack depth. So Hoarfrost, Particle Shielding, Spirit
/// Ward and Sonic Haven no longer offer a ×7 slider on a shield that never doubles, and the
/// display and the totals now answer the same question from the same place.
///
/// It cuts the other way too, and the corpus population is larger: the display bag flattens
/// `movement` into per-axis rows, the converter only ever listed the nested key, and so the rows
/// the surface renders were the ones the multiply missed.
///
/// The rows a per-type object carries are re-resolved per child through
/// [`StackFamily::for_nested_key`] (STACK-4): the parent's whole-family cap is the max over its
/// atoms, and Time Wall's Run axis reaches 2 while its Fly / Jump axes do not stack at all. A
/// child that names no sub type inherits its parent's reading, so a parent with no per-sub-type
/// partition is transformed exactly as before.
pub fn adjust_display_bag(bag: &mut Map<String, Value>, power: &Power, targets_hit: Option<u32>) {
    let Some(count) = targets_hit.filter(|count| *count > 1) else {
        return;
    };
    if !has_stacking_slider(power) {
        return;
    }
    for (effect_key, value) in bag.iter_mut() {
        // A `perTarget` value takes the AoE path alone — applying both would scale it by N
        // twice (the beta's own guard, keyed on the whole value rather than the leaf).
        let cap = (!has_per_target(value))
            .then(|| StackFamily::for_bag_key(effect_key)?.cap(power))
            .flatten();
        adjust_display_value(value, count, cap.is_some(), cap, effect_key, power);
    }
}

/// One bag value: a scaled leaf takes [`adjust_for_stacking`]; a by-type object recurses into
/// its entries. A bare number is left alone — the beta's `typeof value !== 'object'` guard,
/// which [`Scaled`]'s flattening cannot express, so it lives here where the raw JSON still can.
///
/// `parent_key` is the bag key this value sits under, and it is what lets the recursion ask the
/// STACK-4 question one level down: a per-type parent (`movement`, `defenseBuff`, `resistance`)
/// owns a whole-family cap it must not hand to its children, because each child key names its
/// own sub type and stacks to that sub type's depth — Time Wall's `movement` object carries the
/// Run axis' cap, and its `flySpeed` child is a `Replace` axis that stacks nowhere.
fn adjust_display_value(
    value: &mut Value,
    count: u32,
    stacks_linear: bool,
    cap: Option<f64>,
    parent_key: &str,
    power: &Power,
) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    // A MaxHP-fraction absorb grows its FRACTION, the only magnitude it has (PROD6C-3j). No
    // corpus power spells an absorb both ways or lists a fraction-form absorb in `stacksLinear`,
    // so the fraction only ever takes the per-target path here.
    if let Some(fraction) = object.get("maxHPFraction").and_then(Value::as_f64) {
        let leaf = Scaled {
            scale: fraction,
            table: None,
            per_target: object.get("maxHPFractionPerTarget").and_then(Value::as_f64),
        };
        let adjusted =
            adjust_for_stacking(&leaf, Some(count), stacks_linear, cap, target_count(power)).scale;
        if let Some(number) = serde_json::Number::from_f64(adjusted) {
            object.insert("maxHPFraction".to_string(), Value::Number(number));
        }
        return;
    }
    let Some(scale) = object.get("scale").and_then(Value::as_f64) else {
        for (child_key, nested) in object.iter_mut() {
            // A per-type child re-resolves its own family instead of inheriting the parent's
            // whole-family cap; every other parent passes its reading down unchanged.
            let (child_linear, child_cap) = StackFamily::for_nested_key(parent_key, child_key)
                .map(|family| {
                    let cap = family.cap(power);
                    (cap.is_some(), cap)
                })
                .unwrap_or((stacks_linear, cap));
            adjust_display_value(nested, count, child_linear, child_cap, child_key, power);
        }
        return;
    };
    let leaf = Scaled {
        scale,
        table: None,
        per_target: object.get("perTarget").and_then(Value::as_f64),
    };
    let adjusted =
        adjust_for_stacking(&leaf, Some(count), stacks_linear, cap, target_count(power)).scale;
    if let Some(number) = serde_json::Number::from_f64(adjusted) {
        object.insert("scale".to_string(), Value::Number(number));
    }
}
