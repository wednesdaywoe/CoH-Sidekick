//! Perma-tracker calc. A power is *perma* when its effective recharge is ≤ its own duration, so
//! the moment its buff (or pet, or self-penalty window) expires it can be re-fired and the
//! caster-side state never drops. Pure numbers over the export bag (`Power.extra`), the Rust of
//! the beta `perma.ts`; read at the edge by the power card's ring, testable with plain `#[test]`
//! over the contract.
//!
//! The slotted-recharge and global-recharge inputs come from the same seams the dashboard uses
//! (the per-power [`crate::apply::power_enhancement`] and the finalized [`crate::CalculatedTotals`]),
//! so the ring can never disagree with the recharge number the stats dashboard shows.

use crate::projection::StrengthBounds;
use coh_data::{AtomicEffect, Power, Stacking, ToWho};
use serde_json::{Map, Value};

/// Effect-bag keys a *caster-side* buff can occupy, the beta `hasSelfStateToKeepUp` self-buff set.
/// How long the caster holds one comes from the key's own entry in `durations`
/// ([`self_state_window_from_atoms`]), and a key with no entry there is instantaneous, not a state to keep
/// up. Schema field names only, never a power proper noun (Rule 0).
///
/// These slots are NOT a self-side partition, and presence alone doesn't name the key as the
/// caster's, because a foe-directed effect lands in them too. Sonic Melee's `debuffResistance` is
/// `Target kTarget` in the authored def, reducing the FOE's resistance to debuffs, and a foe slow
/// lands in `movement`. The bag can't tell you which: the converter's buff branches emit
/// `{scale: |scale|, table}` and stamp `toWho` only on the self-penalty branches, so not one of
/// these slots carries a target, and the `Math.abs` drops the sign besides.
/// [`is_self_directed_effect`] therefore can't be applied to them the way `SELF_PENALTY_KEYS` does.
///
/// The export isn't missing the answer, only this projection of it. The atom array carries
/// `toWho` on every atom, and [`caster_holds_state_at`] joins back to it on the duration, which
/// keeps a foe's clock from becoming the caster's window.
pub const SELF_BUFF_KEYS: &[&str] = &[
    "tohitBuff",
    "damageBuff",
    "defenseBuff",
    "defenseBuffSuppressible",
    "rechargeBuff",
    "recoveryBuff",
    "regenBuff",
    // The `*Unenhanced` twins are the same caster state as their base key, split off by the
    // one discriminator the bag could not hold: an IgnoreStrength row lands in the twin and
    // nowhere else, so a self-buff whose only copy ignores strength had no slot here at all.
    // The list carried `regenBuffUnenhanced` alone until 2026-09-04, which is the shape the
    // skill's trap catalogue names — one axis, re-invented per family, and picked up in one of
    // four places. Adding the other three moves 18 windows corpus-wide (the four accolades on
    // every fork, Cross Punch and Swirl on Thunderspy) and one eligibility (PERMA-2's census).
    "regenBuffUnenhanced",
    "maxHPBuffUnenhanced",
    "recoveryBuffUnenhanced",
    "tohitBuffUnenhanced",
    "speedBuff",
    "enduranceBuff",
    "enduranceGain",
    "maxHPBuff",
    "maxEndBuff",
    "rangeBuff",
    "enduranceDiscount",
    "perceptionBuff",
    "specialBuff",
    "absorb",
    "defense",
    "resistance",
    "elusivity",
    "movement",
    "stealth",
    "debuffResistance",
    "mezResistance",
    "protection",
    "untouchable",
    "fly",
];

/// How re-firing the power while its caster-side window is still running combines with the
/// copy already up — the answer to "does casting early double the buff, or just restart the
/// clock". Read off the atoms' authored stacking flavour, never inferred from durations.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum RecastBehavior {
    /// The new application replaces or refreshes the running one (`kReplace`/`kRefresh`);
    /// magnitudes never overlap, however early the recast.
    Refreshes,
    /// Applications accumulate up to the authored stack limit (`kStack`/`kRefreshToCount`
    /// with room to grow); an early recast holds two copies for the overlap.
    Stacks,
}

/// The recast verdict for the caster-side window at `seconds`: every atom that times that
/// window and lands on the caster is classified by its authored stacking flavour, and only a
/// unanimous family yields a verdict. A mix — or a flavour whose recast semantics this repo
/// has not proven (`Extend` lengthens, `Overlap`/`Maximize`/`Ignore`/`Suppress`/
/// `StackThenIgnore`/`Continuous` each mean something else) — yields `None`: a single badge
/// would misdescribe at least one row, and an absent verdict is "unstated", never a claim.
///
/// The duration join and the caster filter are [`caster_holds_state_at`]'s, for the same
/// reason it needs them: the bag's slots carry no target, so the atoms are the only place the
/// export records *whose* clock the flavour governs.
pub fn recast_verdict(def: &Power, seconds: f64) -> Option<RecastBehavior> {
    let mut refreshes = false;
    let mut stacks = false;
    for atom in &def.atoms {
        let Some(duration) = atom.duration else {
            continue;
        };
        if (duration - seconds).abs() > DURATION_MATCH_TOLERANCE {
            continue;
        }
        if !reaches_caster_for_perma(atom, def) {
            continue;
        }
        match atom.stacking {
            Some(Stacking::Replace) | Some(Stacking::Refresh) => refreshes = true,
            // The converter's own self-stacking rule (`detectSelfStacking`): these two
            // accumulate only with room above one copy. A capped-at-one or cap-less read
            // is unproven, so it refuses a verdict rather than guessing.
            Some(Stacking::Stack) | Some(Stacking::RefreshToCount) => {
                if atom.stack_cap.is_some_and(|cap| cap > 1.0) {
                    stacks = true;
                } else {
                    return None;
                }
            }
            Some(
                Stacking::Extend
                | Stacking::Overlap
                | Stacking::Maximize
                | Stacking::Ignore
                | Stacking::Suppress
                | Stacking::StackThenIgnore
                | Stacking::Continuous
                | Stacking::Yes
                | Stacking::No,
            )
            | None => return None,
        }
    }
    match (refreshes, stacks) {
        (true, false) => Some(RecastBehavior::Refreshes),
        (false, true) => Some(RecastBehavior::Stacks),
        // No atom timed the window, or the families disagree across rows.
        (false, false) | (true, true) => None,
    }
}

/// Everything the ring needs to draw and label the perma state of one power.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct PermaInfo {
    /// Base recharge time in seconds (before enhancement).
    pub base_recharge: f64,
    /// The caster-side window in seconds ([`self_state_window_from_atoms`]): the longest-lived state the
    /// caster holds, whether that's a self buff, a self-directed penalty, or a pet's lifetime.
    pub duration: f64,
    /// Recharge after slotting + global recharge: `baseRecharge / (1 + totalRecharge)`.
    pub effective_recharge: f64,
    /// The +recharge fraction required to perma (`baseRecharge/duration − 1`; 2.75 = +275%).
    pub recharge_needed: f64,
    /// The current +recharge fraction in hand (slotted + global).
    pub total_recharge: f64,
    /// Progress toward perma, 0–100 (capped).
    pub perma_percent: f64,
    /// True when the power can be kept up permanently (`effectiveRecharge ≤ duration`).
    pub is_perma: bool,
    /// Whether recasting inside the window stacks or merely refreshes ([`recast_verdict`]
    /// at this window's duration); `None` when the atoms don't state a single answer.
    pub recast: Option<RecastBehavior>,
}

fn power_type_lower(def: &Power) -> Option<String> {
    def.extra
        .get("powerType")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
}

/// Base recharge: `stats.recharge` (beta `getRecharge`). The transitional `effects.recharge`
/// was a second spelling of this same field, so reading it here is redundant once the bag
/// leaves the contract — a census over all three bundles finds zero powers where a positive
/// `effects.recharge` differs from `stats.recharge`, and none where it is positive while
/// `stats.recharge` is absent.
fn get_recharge(def: &Power) -> f64 {
    def.extra
        .get("stats")
        .and_then(Value::as_object)
        .and_then(|stats| stats.get("recharge"))
        .and_then(Value::as_f64)
        .filter(|r| *r > 0.0)
        .unwrap_or(0.0)
}

/// Debuff slots the converter's self-penalty branches tag, and so the only ones whose VALUE can
/// say that the penalty is the CASTER's. Each still has to prove it (`toWho: "Self"`), since the
/// same slot on another power is an ordinary foe debuff. The per-type `slow` map is handled
/// separately, being a map rather than a slot.
///
/// This is a list of slots that can carry the answer, NOT the list of penalties a caster can
/// take: everything else the converter routes to a `*Debuff` slot loses its recipient on the way
/// in, which is a default worth having and a closed list worth refusing. `caster_takes_penalty_at`
/// is the open half, asked of the atoms (PERMA-4).
const SELF_PENALTY_KEYS: &[&str] = &[
    "damageDebuff",
    "rechargeDebuff",
    "tohitDebuff",
    "accuracyDebuff",
];

/// The bag rounds a duration out of the same `"N seconds"` string the atom parses, so the two
/// agree exactly today; the tolerance is here so a rounding change on one side degrades into a
/// missed veto rather than a wrong window.
const DURATION_MATCH_TOLERANCE: f64 = 1e-3;

/// Does a `Target`-anchored atom reach the caster? `toWho: Target` names whoever the power is
/// aimed at, which on a team buff is a teammate with the caster among them, so this is a
/// question about a recipient LIST — and two lists can answer it.
///
/// **One rule over both: is the caster named among the recipients.** An absent list is the only
/// exception, and it keeps the window: this veto DELETES a window rather than crediting a value,
/// so missing evidence must never manufacture one (and why this is not
/// [`Power::affects_caster`], which answers `false` for an absent list as readily as for one
/// naming somebody else).
///
/// **Which list.** [`AtomicEffect::owner_targets`] is stamped by the collectors exactly when a
/// template was pulled out of ANOTHER power's file, and then it — not the shell's aim — says
/// whom the row lands on (TARGETS-3, the order [`coh_data::reaches_caster`] already reads them
/// in). Fulcrum Shift is the case: `targetsAffected: ['Foe']` on the parent, and its eight
/// `+Damage` rows arrive from the buff sub-power it executes, each stamped `['Friend', 'Self']`.
/// Reading the parent vetoed all eight and collapsed a 45s window the caster demonstrably holds
/// — the archetypal Kinetics perma, `win 0.0, rust false` on every Kinetics copy of Homecoming
/// and Brainstorm (Controller and Defender spell it `Kinetic_Transfer`), with no ring drawn.
/// PERMA-4 fixed that on 2026-09-05.
///
/// **Why a union answers a per-row question.** This is the half that stayed wrong until PERMA-5
/// (2026-09-05). The power's `targetsAffected` is a union over the whole power, and the old read
/// concluded from that it could only be read leniently: no `Self` proves nothing, only a foe
/// entry is evidence against. That inverts the one direction a union is sound in. A union is a
/// SUPERSET of every row's recipients, so `Self` in it does not put this row on the caster —
/// but `Self` missing from it puts NO row of the power on the caster, this one included. The
/// weak direction is the positive one; the negative direction is a proof.
///
/// The corpus agrees, and it is the same field that separates the two populations. The
/// archetypal perma team buffs spell the caster out — Accelerate Metabolism and Chrono Shift
/// `['Friend', 'Self']`, Farsight `['Teammate', 'Self']` — and keep their windows untouched,
/// which is what the old comment's fear (that requiring `Self` would delete their rings) was
/// about. The ally-only buffs name `Friend`, `DeadPlayerFriend` or `DeadOrAliveLeaguemate` and
/// never the caster, and the lenient read handed him their clocks: 47 power entries over 22
/// (fork, power) pairs drew a perma ring off a window nobody holds — Adrenalin Boost,
/// Painbringer, Amp Up, Fortify Pack, Experimental Injection, Mutation, Elixir of Life,
/// Thunderspy's Motivate/Discipline Allies and its two teleports — with another 330 census rows
/// on ally buffs whose recharge never let the window matter (Fortitude, Speed Boost, Clear Mind,
/// the single-target shields).
///
/// PERMA-3: the power half of this read `targetType.starts_with("Foe")` until 2026-09-04, which
/// is the wrong FIELD and, within it, a prefix rather than a vocabulary. `targetType` is where
/// the power is aimed; `EntsAffected` is who its effects land on, and only the second answers
/// this question. A control aimed at `Any` or cast on `Self` — Detention Field and Sonic Cage on
/// the forks spelling them `Any`, every PBAoE, Recall Friend at `Any` — read as foe-free, so the
/// veto went inert and the FOE's 30s intangibility (or a teleport target's 1.5s translucency)
/// became the caster's own window, ring and all. 12 power entries drew a ring nobody could keep
/// up. The foe vocabulary that replaced it is gone again with PERMA-5: naming a foe is no longer
/// how the caster is ruled out, not being named is.
fn target_recipients_reach_caster(atom: &AtomicEffect, def: &Power) -> bool {
    let stamped = atom
        .owner_targets
        .as_deref()
        .filter(|list| !list.is_empty());
    match stamped {
        Some(targets) => targets.iter().any(|t| &**t == "Self"),
        // No power in any of the four bundles omits `targetsAffected` (0 of 14,449 on
        // 2026-09-05), so the absent arm is unreachable on the corpus and no census or parity
        // sweep can grade it. `an_ally_only_recipient_list_times_the_allys_clock` states it.
        None => match def.targets_affected() {
            Some(targets) if !targets.is_empty() => targets.contains(&"Self"),
            _ => true,
        },
    }
}

/// Whether an atom's effect reaches the caster. The two signals carry different weight:
/// `notOnCaster` is explicit and decides on its own whoever the power is aimed at, while a
/// target-anchored recipient resolves through its recipient list
/// ([`target_recipients_reach_caster`]). An absent or `Unspecified` target proves nothing and
/// reads as "reaches the caster", so missing evidence can never manufacture a veto.
///
/// Deliberately NOT [`coh_data::lands_on_caster`], despite the near-identical question.
/// That one defaults an unstated recipient to "no", which is right where it is asked —
/// it decides whether to CREDIT a value, and crediting on a guess fabricates a total.
/// This one decides whether to VETO one, so the same default would delete a real buff on
/// no evidence. Same question, opposite burden of proof; a shared implementation would
/// have to pick one and be wrong at the other door.
fn reaches_caster_for_perma(atom: &AtomicEffect, def: &Power) -> bool {
    if atom.not_on_caster == Some(true) {
        return false;
    }
    match atom.to_who {
        Some(ToWho::Target | ToWho::TargetOnly | ToWho::TargetOnlyAndPets) => {
            target_recipients_reach_caster(atom, def)
        }
        // A marker mod attaches to a map marker entity, never to a character.
        Some(ToWho::Marker) => false,
        Some(ToWho::Self_ | ToWho::SelfAndPets | ToWho::TargetAndPets | ToWho::Unspecified)
        | None => true,
    }
}

/// Does the caster hold anything for `seconds`? `Some(false)` only when the power's atoms
/// positively account for that duration and every one of them lands on the target; `None` when no
/// atom carries it, the "no evidence" case rather than a negative answer.
///
/// This is the target check the bag can't answer for [`SELF_BUFF_KEYS`]: those slots ship no
/// `toWho`, but the atom array does carry one, so the duration is the join key back to it. A
/// power whose self buff and foe debuff happen to share a duration passes the veto. The check can
/// fail to remove a foe window, never remove a caster's own.
fn caster_holds_state_at(def: &Power, seconds: f64) -> Option<bool> {
    let mut accounted = false;
    let mut caster = false;
    for atom in &def.atoms {
        let Some(duration) = atom.duration else {
            continue;
        };
        if (duration - seconds).abs() > DURATION_MATCH_TOLERANCE {
            continue;
        }
        accounted = true;
        caster |= reaches_caster_for_perma(atom, def);
    }
    accounted.then_some(caster)
}

/// [`self_state_window`] read from the power's atoms instead of the effects bag, through the
/// [`crate::window_slots`] per-key converter mirror. Same four routes, same veto discipline:
/// the mirror answers what the bag answered (which keys exist, each key's clock, the
/// self-penalty marks), and the atoms keep the veto they already had over whose clock it is.
///
/// The summon lifetime is read from the atoms too, off
/// [`AtomicEffect::summon_window`] — the field the converter's own summon resolution stamps on
/// the row it read, because which `EntCreate` row IS the power's window is a question no rule
/// over `duration` can answer (ENT-14, and the field's own doc for the three shapes). Gated
/// rows count here: Soul Extraction's ghosts are tier-gated and exactly one materializes, so
/// the stamp is the admission and the gate is not.
pub fn self_state_window_from_atoms(def: &Power, dataset: coh_data::DatasetId) -> f64 {
    let slots = crate::window_slots::window_slots(def, dataset);
    let summon_lifetime = def
        .atoms
        .iter()
        .filter_map(|atom| atom.summon_window)
        .filter(|d| *d > 0.0)
        .fold(0.0f64, f64::max);

    if slots.durations.is_empty() {
        if !has_caster_state_atoms(def, &slots) {
            return summon_lifetime;
        }
        let power_level = slots.buff_duration.filter(|d| *d > 0.0).unwrap_or(0.0);
        return summon_lifetime.max(power_level);
    }

    slots
        .durations
        .iter()
        .filter(|(_, seconds)| **seconds > 0.0)
        .filter(|(key, seconds)| times_a_caster_state_atoms(def, &slots, key, **seconds))
        .map(|(_, seconds)| *seconds)
        .fold(summon_lifetime, f64::max)
}

/// [`times_a_caster_state`] over the mirror instead of the bag: same key sets, same routes,
/// with the penalty marks and the slow map read from [`crate::window_slots::WindowSlots`].
fn times_a_caster_state_atoms(
    def: &Power,
    slots: &crate::window_slots::WindowSlots,
    key: &str,
    seconds: f64,
) -> bool {
    if key == "absorb" {
        return caster_holds_state_at(def, seconds).unwrap_or(true);
    }
    if SELF_BUFF_KEYS.contains(&key) {
        return slots.present.contains(key) && caster_holds_state_at(def, seconds).unwrap_or(true);
    }
    if SELF_PENALTY_KEYS.contains(&key) {
        return slots.self_marked.contains(key);
    }
    if key == "slow" {
        return slots.slow_self;
    }
    caster_takes_penalty_at(def, seconds)
}

/// Does an atom of the power state that the CASTER takes a lasting penalty for `seconds`?
///
/// The open half of [`SELF_PENALTY_KEYS`]. The mirror's four tagged slots are the only ones whose
/// VALUE records a recipient, so every other debuff route arrives here having lost the one fact
/// that decides whose state it is — and reading that loss as "the foe's" is right as a default
/// and wrong as a closed list. The atom never lost it, so this asks the atom.
///
/// PERMA-4: EM Pulse leaves the caster with a 15s `Recovery −10 toWho Self`, the endurance crash
/// the power is famous for, and `recoveryDebuff` is not one of the four. It read `win 0.0` on
/// every fork that carries the row while the atom sat right there.
///
/// Three terms, and the burden is the opposite of [`caster_holds_state_at`]'s. That one vetoes a
/// window the mirror already called the caster's, so absent evidence must not remove it; this one
/// CREATES a window off a slot whose default reading is the foe's, so absent evidence must not
/// add one:
///
/// * [`coh_data::lands_on_caster`] — the anchored recipients only, so an unstated `toWho` proves
///   nothing here. A `Target` row of a power that also names `Self` is the foe's debuff on every
///   power in this class, and crediting it would hand the caster every foe's clock.
/// * [`is_caster_state_family`] — a penalty the caster is IN, not one applied to him. A self
///   `Damage` row is the crash a nuke charges you, over the instant it lands.
/// * [`crate::window_slots::bag_subset`] — the same rows the mirror routed. A gated or forked row
///   created no slot, so it cannot be the evidence behind one.
fn caster_takes_penalty_at(def: &Power, seconds: f64) -> bool {
    def.atoms
        .iter()
        .filter(|atom| crate::window_slots::bag_subset(atom))
        .filter(|atom| {
            atom.duration
                .is_some_and(|d| (d - seconds).abs() <= DURATION_MATCH_TOLERANCE)
        })
        .filter(|atom| coh_data::lands_on_caster(atom))
        .any(|atom| is_penalty_row(atom) && is_caster_state_family(atom))
}

/// Is this atom a penalty rather than a buff? Spelled two ways in the export and both have to be
/// read: a negative scale, and the table. A slow is a POSITIVE scale on a `*_Slow` table (Spin's
/// `0.2` on `Melee_Slow`), so reading only the sign would miss half of them.
fn is_penalty_row(atom: &AtomicEffect) -> bool {
    let table = atom
        .modifier_table
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    atom.scale.is_some_and(|scale| scale < 0.0)
        || table.contains("debuff")
        || table.contains("slow")
}

/// Effect families a lasting caster-side state can live in — [`SELF_BUFF_KEYS`] asked one layer
/// up, where the export states it, and the beta `perma.ts`'s `SELF_STATE_TYPES`. Deliberately not
/// the whole enum: applied control, damage and the instantaneous heal are things that happen TO
/// someone rather than states anybody holds.
///
/// Exhaustive on purpose: a new effect type has to be classified here rather
/// than falling into "not a state" on the compiler's silence.
fn is_caster_state_family(atom: &AtomicEffect) -> bool {
    use coh_data::EffectType as E;
    let Some(effect_type) = atom.effect_type else {
        return false;
    };
    match effect_type {
        E::Absorb
        | E::Accuracy
        | E::DamageBuff
        | E::Defense
        | E::Elusivity
        | E::Endurance
        | E::EnduranceDiscount
        | E::Enhancement
        | E::MaxEndurance
        | E::MaxHp
        | E::MezResist
        | E::Movement
        | E::Perception
        | E::Range
        | E::RechargeTime
        | E::Recovery
        | E::Regeneration
        | E::Resistance
        | E::Stealth
        | E::ToHit => true,
        // Mez at aspect `Res` is protection, a state the caster is in; applied mez is not.
        E::Mez => atom.aspect == Some(coh_data::Aspect::Res),
        // Healing STRENGTH holds a window (Field Medic's 60s). The heal itself is `Abs`, a
        // heal-over-time tick rather than a state.
        E::Heal | E::HealResistance => atom.aspect != Some(coh_data::Aspect::Abs),
        E::Damage
        | E::EntCreate
        | E::ExecutePower
        | E::GlobalChanceMod
        | E::GrantPower
        | E::Meta
        | E::RechargePower
        | E::ThreatLevel
        | E::Unmapped => false,
    }
}

/// [`has_caster_state`] over the mirror: a named self-shaped slot or penalty mark, with the
/// same atom veto.
fn has_caster_state_atoms(def: &Power, slots: &crate::window_slots::WindowSlots) -> bool {
    let named = SELF_BUFF_KEYS.iter().any(|key| slots.present.contains(key))
        || slots.has_self_directed_penalty();
    named
        && (def.atoms.is_empty()
            || def
                .atoms
                .iter()
                .any(|atom| reaches_caster_for_perma(atom, def)))
}

/// A debuff the *caster* suffers: the object-shaped `ScaledEffect` (`scale` + `table`) carrying
/// `toWho: "Self"` (beta `isSelfDirectedEffect`). A bare-number debuff slot is foe/display-only by
/// construction, so only the object form can be self-directed.
fn is_self_directed_effect(value: Option<&Value>) -> bool {
    value.and_then(Value::as_object).is_some_and(|obj| {
        obj.contains_key("scale")
            && obj.contains_key("table")
            && obj.get("toWho").and_then(Value::as_str) == Some("Self")
    })
}

/// True when a power carries any self-directed penalty, a debuff the caster actually takes
/// (Granite Armor's `-damage`, Defensive Adaptation's `-recharge`). Scans exactly the slots the
/// converter's self-penalty branches tag: the damage/recharge/tohit/accuracy debuffs plus the
/// per-type `slow` map (beta `hasSelfDirectedPenalty`). Shared with the toggle-pill gate. Both
/// ask "does this land a persistent state on the caster?", so the game rule lives once here.
pub fn has_self_directed_penalty(effects: &Map<String, Value>) -> bool {
    if SELF_PENALTY_KEYS
        .iter()
        .any(|key| is_self_directed_effect(effects.get(*key)))
    {
        return true;
    }
    effects
        .get("slow")
        .and_then(Value::as_object)
        .is_some_and(|slow| slow.values().any(|v| is_self_directed_effect(Some(v))))
}

/// The power carries `damage`, the beta's `!!power.damage` truthiness. A damaging attack's
/// duration is typically a hold/mez roughly matching its recharge, so [`is_perma_eligible`] holds
/// it to a stricter recharge ≥ 2× duration bar than a plain buff.
fn has_damage(def: &Power) -> bool {
    def.extra
        .get("damage")
        .is_some_and(|damage| !damage.is_null())
}

/// Whether a power should show a perma ring at all, meaning keeping it up permanently has to be
/// both meaningful and achievable (beta `isPermaEligible`):
/// - a Click power (toggles re-apply on a per-tick cadence, autos fire on their own, and neither
///   has a perma gap to close);
/// - with a positive recharge *and* a caster-side state to keep up, one test and not two, since
///   [`self_state_window`] is zero exactly when no such state exists. Mez and foe debuffs don't
///   count: those expire on the target after the cast, and the planner doesn't model refreshing
///   them by re-firing on cadence. For [`SELF_BUFF_KEYS`] that exclusion is the atoms' to enforce
///   rather than the bag's, since those slots carry no target of their own;
/// - whose cycle still fits inside that window at the archetype's *maximum* recharge strength,
///   the cheapest statement of "perma is reachable at all", since `bounds.cap` is the most the
///   game will ever divide by (Rest, Build Up and Category Five asymptote well short of it, and a
///   ring that can never fill only confuses);
/// - and clears the recharge bar for its kind: a damaging attack needs recharge ≥ 2× duration
///   (its duration is usually an incidental hold), a buff/summon just needs recharge > duration.
///
/// `bounds` is the archetype's RechargeTime `ClampStrength` interval, as in
/// [`calculate_perma_info`]; `None` leaves the reachability test out rather than inventing a
/// ceiling, matching how an absent cap leaves the divisor unclamped.
pub fn is_perma_eligible(
    def: &Power,
    bounds: Option<StrengthBounds>,
    dataset: coh_data::DatasetId,
) -> bool {
    if matches!(
        power_type_lower(def).as_deref(),
        Some("toggle") | Some("auto")
    ) {
        return false;
    }
    let recharge = get_recharge(def);
    let duration = self_state_window_from_atoms(def, dataset);
    if recharge <= 0.0 || duration <= 0.0 {
        return false;
    }
    if bounds.is_some_and(|bounds| recharge / bounds.cap > duration) {
        return false;
    }
    if has_damage(def) {
        recharge >= duration * 2.0
    } else {
        recharge > duration
    }
}

/// Compute perma info for a power from its already-resolved recharge inputs: `slotted_recharge`
/// (the post-ED per-power recharge fraction from [`crate::apply::power_enhancement`]) and
/// `global_recharge` (the build's global +recharge as a fraction, i.e. `stats.recharge / 100`).
/// `None` for a toggle/auto or a power lacking a recharge/duration (beta `calculatePermaInfo`).
///
/// `StrengthsDisallowed('RechargeTime')` powers take no recharge strength at all, so Hasten, set
/// bonuses, and slotted IOs are ignored (Rune of Protection, the armor T9s); `GlobalStrengths-
/// Disallowed` keeps slotted enhancement but ignores globals (Kuji-In Rin). Server-side `.powers`
/// data, HC only.
///
/// `bounds` is the archetype's RechargeTime `ClampStrength` interval. Unlike the endurance
/// sibling, its +400% cap is REACHABLE: a perma build stacking Hasten, set bonuses and Ageless
/// lives near it. Recharge past the cap therefore has to stop buying anything, both in the
/// effective recharge and in how close to perma the power reads. `None` leaves it unclamped
/// (see [`crate::projection::ReductionClamps`]).
pub fn calculate_perma_info(
    def: &Power,
    targets_hit: Option<u32>,
    slotted_recharge: f64,
    global_recharge: f64,
    bounds: Option<StrengthBounds>,
    dataset: coh_data::DatasetId,
) -> Option<PermaInfo> {
    if matches!(
        power_type_lower(def).as_deref(),
        Some("toggle") | Some("auto")
    ) {
        return None;
    }
    // An adaptive-recharge power waits longer the more foes it hit, so its perma reading
    // follows the same count as its recharge row.
    let base_recharge = match crate::adaptive_recharge::adaptive_recharge(def) {
        Some(rule) => rule.base_for(targets_hit),
        None => get_recharge(def),
    };
    let duration = self_state_window_from_atoms(def, dataset);
    if base_recharge <= 0.0 || duration <= 0.0 {
        return None;
    }

    let recharge_locked = disallows_recharge(def, "strengthsDisallowed");
    let global_locked = recharge_locked || disallows_recharge(def, "globalStrengthsDisallowed");
    let slotted = if recharge_locked {
        0.0
    } else {
        slotted_recharge
    };
    let total_recharge = slotted + if global_locked { 0.0 } else { global_recharge };

    // Both numbers below read the CLAMPED strength, because the game has only the clamped one:
    // `total_recharge` stays raw for display (it's what the build actually carries), but a build
    // past the cap doesn't recharge faster and doesn't get closer to perma either.
    let net_strength = match bounds {
        Some(bounds) => (1.0 + total_recharge).clamp(bounds.floor, bounds.cap),
        None => 1.0 + total_recharge,
    };
    let effective_recharge = base_recharge / net_strength;
    let recharge_needed = base_recharge / duration - 1.0;
    let perma_percent = if recharge_needed > 0.0 {
        ((net_strength - 1.0) / recharge_needed * 100.0).min(100.0)
    } else {
        100.0
    };

    Some(PermaInfo {
        base_recharge,
        duration,
        effective_recharge,
        recharge_needed,
        total_recharge,
        perma_percent,
        is_perma: effective_recharge <= duration,
        recast: recast_verdict(def, duration),
    })
}

/// The power's `field` array (a `StrengthsDisallowed` list) names `RechargeTime`.
pub(crate) fn disallows_recharge(def: &Power, field: &str) -> bool {
    def.extra
        .get(field)
        .and_then(Value::as_array)
        .is_some_and(|list| list.iter().any(|v| v.as_str() == Some("RechargeTime")))
}
