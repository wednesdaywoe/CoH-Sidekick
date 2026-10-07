//! Movement appliers, ported from `movementBuffValue` / `selfSlowValue`
//! (atom-query.ts): the atom-native `effects.movement` and `effects.slow` maps —
//! the self/current movement BUFF and the self movement PENALTY, per axis.
//! `FlyMode` (the kFly flight-mode grant) is a distinct axis and falls out
//! structurally — reading its mode magnitude as a speed buff double-counts Fly by
//! +200%.
//!
//! Mirrors the bag's MOVEMENT routing chain: aspect `Res` → debuff-resistance;
//! self+`Str` → strength; self+`Max`+scale>0 → the travel-CAP bump (the split
//! that stopped Super Speed reporting 1.938×Melee_Ones); `Max`+slow → the cap
//! debuff; self+slow → `slow`; then keep self or `Cur`-aspect only. `stackKey`
//! (only with `stacking: Suppress`) and `suppressible` ride along per entry as
//! travel-suppression metadata.
//!
//! The two maps are read on the same terms and keyed the same way for one
//! reason: a power's plus and its minus on one axis are two halves of one
//! authored pair, and they part company here — positives land in `movement`,
//! negatives in `slow`. Key the two differently and one half is counted without
//! the other. See [`movement_buff_value`] and [`self_slow_value`].

use super::{base_atoms_of_type, is_debuff_atom, is_gated};
use coh_data::slot_value::MovementValue;
use coh_data::{reaches_caster, Aspect, AtomicEffect, EffectType, Power, Stacking, SubType};

/// Movement axis → the bag's `effects.movement` key. Deliberately partial:
/// `fly` (FlyMode), `movementControl`, `movementFriction` add zero to totals on
/// both sides and are excluded structurally.
fn axis_key(sub: Option<SubType>) -> Option<&'static str> {
    match sub? {
        SubType::Run => Some("runSpeed"),
        SubType::Fly => Some("flySpeed"),
        SubType::Jump => Some("jumpSpeed"),
        SubType::JumpHeight => Some("jumpHeight"),
        _ => None,
    }
}

/// Movement axis → the global the bag's `effects.slow` key routes to
/// (`Bag::self_slow`'s `slowKeyMap`). Wider than [`axis_key`] by the two axes
/// that carry a modelled global on the penalty side but contribute nothing on
/// the buff side (MOVE-1).
fn slow_axis_key(sub: Option<SubType>) -> Option<&'static str> {
    match sub? {
        SubType::Control => Some("movementControl"),
        SubType::Friction => Some("movementFriction"),
        other => axis_key(Some(other)),
    }
}

/// A movement atom the bag routes to `slow` rather than `movement`.
fn is_slow_atom(a: &AtomicEffect) -> bool {
    is_debuff_atom(a)
        || a.modifier_table
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("slow")
}

/// An atom from a `chance: 0` group that names no mode to be gated on.
///
/// A chance-0 group is a mode-gate sentinel rather than a literal 0% (METHOD-1),
/// and the converter's collector deliberately keeps one that carries a payload:
/// dropping it wholesale would delete Evasive Maneuvers' fly speed and Rooted's
/// run penalty, which are real effects that apply in their mode. So these arrive
/// here unstamped, inside the base list.
///
/// The corpus splits them in two, cleanly. Of the 24 that reach the caster's
/// movement routing, 8 carry a group `Tag` naming the mode — `FlightActive` on
/// Evasive Maneuvers and Quantum Maneuvers, `GraniteRoot` on Rooted, all
/// Homecoming — and 16 carry no tag, no `Requires` and no special case at all.
/// Those 16 are every Rebirth fly power, and a sentinel that names no mode
/// cannot be a gate on one.
///
/// Dropping them moves no total on its own: each was either overwritten by a
/// later write into the same axis slot, or its whole slot goes empty and the bag
/// fallback answers with the same numbers. It exists for the split, which is
/// what exposes them — with the fly axis split and these left in, Rebirth's Fly
/// reads −5.2% where the game gives +161%.
fn is_unmoded_sentinel(a: &AtomicEffect) -> bool {
    a.base_probability == Some(0.0)
        && a.tags.as_deref().is_none_or(str::is_empty)
        && a.requires_expression.as_deref().is_none_or(<[_]>::is_empty)
        && a.special_case.is_none()
}

/// Which of the four movement slots this atom belongs to, or `None` when an
/// earlier branch of the routing chain claims it (debuff-resistance, strength)
/// or nothing does.
///
/// One function because the chain is one chain: the branches are ordered, and a
/// reader that reproduces only its own branch re-derives the ones above it and
/// drifts from them. The two CAP branches were `None` here until ATOM-BAG-4, and
/// naming them is the whole difference between the four readers below and four
/// separate re-derivations of the same ordering.
#[derive(Clone, Copy, PartialEq)]
enum MovementSlot {
    Movement,
    Slow,
    CapBump,
    CapDebuff,
}

/// The two axes the converter's cap-bump branch refuses even at `Max` aspect, so
/// they fall through to the ordinary maps (`convert-powerset.cjs:6797`). They
/// have no travel ceiling to raise — a control/friction ceiling is not a speed
/// cap — and the exclusion is theirs alone, not a property of `Max`.
///
/// UNMEASURED, and said so rather than left to look load-bearing: no power on
/// any fork states a self, positive, `Max`-aspect control or friction row, so
/// deleting this test moves nothing and the corpus cannot grade it. It is here
/// because the converter branch it mirrors has it, which is the only warrant
/// available — and the day such a row is authored, the two sides agreeing is
/// what keeps the atom reader and the slot from parting company.
fn is_capless_axis(sub: Option<SubType>) -> bool {
    matches!(sub, Some(SubType::Control) | Some(SubType::Friction))
}

fn route(a: &AtomicEffect, power: &Power) -> Option<MovementSlot> {
    if is_unmoded_sentinel(a) {
        return None;
    }
    // A toggle's shutdown burst is not a standing effect — it fires when the power
    // turns OFF. The bag's routing pass has skipped these since it was written
    // (`convert-powerset.cjs:5759`) and this is the same rule on the atom side, held
    // here rather than at each reader so all four slots get it from one place.
    //
    // Reaction Time is the whole population that reaches a movement slot (MOVEMAP-6):
    // it states its aura at `AnyAffected` and mirrors it sign-for-sign at `Self` on
    // deactivation, so the mirror rows read as a standing self slow and a standing
    // +1.0 run-cap RAISE. `is_cancelled_pair` used to catch three of the six by
    // matching `+X` against `−X`; the field states what that inferred, and states it
    // for the `Max` row the arithmetic could not reach.
    if a.is_deactivation_burst() {
        return None;
    }
    let self_directed = reaches_caster(a, power);
    if a.aspect == Some(Aspect::Res) {
        return None; // → debuffResistance.movement
    }
    if self_directed && a.aspect == Some(Aspect::Str) {
        return None; // → specialBuff.movement
    }
    if self_directed
        && a.aspect == Some(Aspect::Max)
        && a.scale.is_some_and(|s| s > 0.0)
        && !is_capless_axis(a.sub_type)
    {
        return Some(MovementSlot::CapBump);
    }
    if a.aspect == Some(Aspect::Max) && is_slow_atom(a) {
        return Some(MovementSlot::CapDebuff);
    }
    if is_slow_atom(a) {
        // Only the caster's own penalty reaches a caster total; a foe slow shares
        // the map (`Bag::self_slow` filters on `toWho`) and is not ours.
        return self_directed.then_some(MovementSlot::Slow);
    }
    self_directed.then_some(MovementSlot::Movement)
}

/// One entry per (axis, `ignoreStrength`, `suppressible`), in first-seen order.
///
/// Two atoms are the same entry when they agree on the axis AND on the two
/// things that change how the axis reads them: whether the caster's enhancements
/// multiply the value, and whether it drops in combat. Sprint's two
/// `RunningSpeed 0.5 Melee_Ones` halves differ on the first and nothing else;
/// Thunderspy folds a third, suppressible travel row onto the same axis. Keying
/// on the axis alone made each of those the last one written, which is how
/// Sprint came to report +50% run where the game gives +100%.
///
/// The key is deliberately no finer. Adding the modifier table separates nothing
/// the corpus states — measured over both maps and all four axes, zero slots
/// change — and dropping the dedup entirely splits twelve more that no oracle
/// has been asked about (Homecoming's Group Fly and the Dwarf Steps, Thunderspy's
/// Speed Boost).
fn keyed_entries<'a>(
    atoms: impl IntoIterator<Item = &'a AtomicEffect>,
    key: fn(Option<SubType>) -> Option<&'static str>,
) -> Vec<(&'static str, MovementValue)> {
    let mut out: Vec<(&'static str, MovementValue)> = Vec::new();
    for a in atoms {
        let Some(axis) = key(a.sub_type) else {
            continue;
        };
        let value = MovementValue {
            scale: a.scale.map_or(f64::NAN, f64::abs),
            table: a.modifier_table.clone(),
            stack_key: if a.stacking == Some(Stacking::Suppress) {
                a.stack_key.clone().filter(|k| !k.is_empty())
            } else {
                None
            },
            suppressible: a.suppressible == Some(true),
            ignore_strength: a.ignore_strength == Some(true),
            per_target: a.per_target,
        };
        match out.iter_mut().find(|(k, v)| {
            *k == axis
                && v.ignore_strength == value.ignore_strength
                && v.suppressible == value.suppressible
        }) {
            Some((_, v)) => *v = value,
            None => out.push((axis, value)),
        }
    }
    out
}

/// The atom-native `effects.movement` — the self/current movement BUFF per axis,
/// for the four axes that reach a character total (Run/Fly/Jump/JumpHeight →
/// runSpeed/flySpeed/jumpSpeed/jumpHeight).
///
/// Every axis splits, fly included. It was held back once, and both reasons on
/// record for holding it turned out not to be reasons:
///
///   * A Parse6 `Fly`/`FlyMode` conflation — measured false. All three exports
///     name the flight-mode grant `Fly` and the speed buff `FlyingSpeed`, with no
///     other spelling in the corpus; the axis map has always sent them to
///     `FlyMode` and `Fly`, and `axis_key` drops `FlyMode` before it reaches here.
///   * The ± pairs on Rebirth and Thunderspy — real, but they are held together
///     by [`self_slow_value`] keying the minus exactly as this keys the plus, not
///     by refusing to split. Refusing cost Combat Flight −51% where the game
///     gives −1%, and Rebirth's Fly −18% where it gives +161%.
pub fn movement_buff_value(power: &Power) -> Option<Vec<(&'static str, MovementValue)>> {
    let movement = base_atoms_of_type(power, EffectType::Movement);
    if movement.is_empty() {
        return None;
    }
    let atoms = movement
        .into_iter()
        .filter(|a| route(a, power) == Some(MovementSlot::Movement));
    Some(keyed_entries(atoms, axis_key))
}

/// Whether this power is a combat-debuff power, so the movement map must stand aside for it
/// — the atom mirror of the bag-presence test `slot_present("tohitDebuff") ||
/// slot_present("damageDebuff")` (`character-totals.ts:1472`), which marks a foe-targeting aura
/// (Time's Juncture) whose movement rows are a foe slow rather than the caster's buff.
///
/// A PRESENCE test, not a value read, so it mirrors the two converter lines that WRITE those
/// slots rather than the readers that spend them: `tohitDebuff` for a `toHit` attrib at neither
/// `Res` nor `Str` aspect with `isDebuff` (`convert-powerset.cjs:6338`), `damageDebuff` for a
/// `*_dmg` attrib at `Str` aspect with `isDebuff` (`:5898`). Both facings count on both slots —
/// the converter writes the key either way and only TAGS `toWho: 'Self'` afterwards — which is
/// why Granite Armor's self −damage crash trips this guard as surely as Time's Juncture's foe
/// −ToHit does. That is not incidental to preserve: the crash is what holds back Granite's
/// phantom +50 JumpSpeed on the Parse6 forks.
///
/// Measured against the slot it replaces over all three bundles: **zero bag-only**, so the guard
/// never stops firing where it fired before. The 7 / 7 / 8 atom-only powers are one family, the
/// Dual Pistols attacks, and they are a converter key RELOCATION rather than a disagreement:
/// the redirect-payload pass moves `damageDebuff` out of base into the Chemical Ammo
/// conditional after base effects are built (`convert-powerset.cjs:1945`), and no wire field
/// move. They contribute nothing either way — every movement row Dual Pistols states is
/// foe-facing, and [`route`] admits only what reaches the caster — so the divergence is inert on
/// the totals, which the corpus gates hold.
pub fn carries_combat_debuff(power: &Power) -> bool {
    power.atoms.iter().any(|a| {
        if is_gated(a) || !is_debuff_atom(a) {
            return false;
        }
        match a.effect_type {
            Some(EffectType::ToHit) => {
                a.aspect != Some(Aspect::Res) && a.aspect != Some(Aspect::Str)
            }
            Some(EffectType::DamageBuff) => a.aspect == Some(Aspect::Str),
            _ => false,
        }
    })
}

/// The atom-native `effects.slow`, self-directed entries only (`Bag::self_slow`)
/// — a movement penalty the caster inflicts on itself. Granite Armor's −70% run
/// and Hibernate are the plain cases; on the Parse6 forks it is also the minus
/// half of a travel power's ± pair.
///
/// Keyed exactly as [`movement_buff_value`] keys the plus, because the pair is
/// one authored thing split across two maps: Rebirth's Group Fly states
/// `+0.5 / −0.5` and again `+0.5 / −0.5 IgnoreStrength`, and a map holding one
/// value per axis kept one of each — which cancelled, by luck. Split the plus
/// alone and the cancel breaks.
///
/// Wider than the buff side by `movementControl` / `movementFriction`, which
/// carry a modelled global on this side (MOVE-1) and none on the other.
pub fn self_slow_value(power: &Power) -> Option<Vec<(&'static str, MovementValue)>> {
    let movement = base_atoms_of_type(power, EffectType::Movement);
    if movement.is_empty() {
        return None;
    }
    let atoms = movement
        .iter()
        .copied()
        .filter(|a| route(a, power) == Some(MovementSlot::Slow));
    Some(keyed_entries(atoms, slow_axis_key))
}

/// The atom-native `effects.movementCapBump` — a travel power's `aspect=Maximum`
/// CEILING raise (Super Speed's run cap +1.938, Fly's +2.0475, Afterburner's
/// +1.0 on top). ATOM-BAG-4(b).
///
/// A separate slot from the speed buff because they are separate attributes: the
/// two used to share one axis key and the cap row clobbered the real
/// Current-aspect buff, which is how Super Speed came to report `1.938 ×
/// Melee_Ones` instead of `1.0 × Melee_SpeedRunning`. [`route`] holds that split;
/// this reader only says which side of it to take.
///
/// Keyed one entry per axis, LAST atom wins, mirroring the converter's plain
/// `effects.movementCapBump[axis] = …` assignment rather than
/// [`movement_buff_value`]'s finer (axis, ignoreStrength, suppressible) key. The
/// two agree on the corpus — no power carries two cap-bump atoms on one axis, on
/// any fork — so the coarser key is the converter's rule, not a collapse this
/// reader chose.
pub fn movement_cap_bump_value(power: &Power) -> Option<Vec<(&'static str, MovementValue)>> {
    cap_entries(power, MovementSlot::CapBump, axis_key, false)
}

/// The atom-native self-directed `effects.movementCapDebuff` — the DEBUFF
/// direction of the same split, which lowers the ceiling rather than the speed
/// (Granite Armor's −1.7851 jump height). ATOM-BAG-4(c).
///
/// Self-directed only, mirroring [`coh_data::Bag::self_movement_cap_debuff`]'s
/// `toWho == "Self"` filter: the converter writes this key for a foe cap debuff
/// too and only TAGS the self ones, so the facing test belongs to the reader
/// rather than to [`route`]. Axis-keyed as wide as the slow side, since a
/// control/friction ceiling reaches a modelled global here (MOVE-1) even though
/// the cap-BUMP branch refuses those two axes outright.
pub fn self_movement_cap_debuff_value(power: &Power) -> Option<Vec<(&'static str, MovementValue)>> {
    cap_entries(power, MovementSlot::CapDebuff, slow_axis_key, true)
}

/// Shared body of the two cap readers — one function for the same reason
/// [`route`] is one function, and because they differ only in which side of the
/// split they take, how wide their axis map is, and whether the facing filter
/// applies.
fn cap_entries(
    power: &Power,
    slot: MovementSlot,
    key: fn(Option<SubType>) -> Option<&'static str>,
    self_only: bool,
) -> Option<Vec<(&'static str, MovementValue)>> {
    let movement = base_atoms_of_type(power, EffectType::Movement);
    if movement.is_empty() {
        return None;
    }
    let atoms = movement
        .into_iter()
        .filter(|a| route(a, power) == Some(slot) && (!self_only || reaches_caster(a, power)));
    let mut out: Vec<(&'static str, MovementValue)> = Vec::new();
    for a in atoms {
        let Some(axis) = key(a.sub_type) else {
            continue;
        };
        let value = MovementValue {
            scale: a.scale.map_or(f64::NAN, f64::abs),
            table: a.modifier_table.clone(),
            stack_key: if a.stacking == Some(Stacking::Suppress) {
                a.stack_key.clone().filter(|k| !k.is_empty())
            } else {
                None
            },
            suppressible: a.suppressible == Some(true),
            ignore_strength: a.ignore_strength == Some(true),
            per_target: a.per_target,
        };
        match out.iter_mut().find(|(k, _)| *k == axis) {
            Some((_, v)) => *v = value,
            None => out.push((axis, value)),
        }
    }
    Some(out)
}

// `is_cancelled_pair` stood here until MOVEMAP-6. It recognised Reaction Time's
// deactivation mirror by matching a `+X` slow row against a `−X` one on the same axis
// and table, because the field that states the fact — `application_type` —
// stopped at the bag and never reached the atom. Two things were wrong with inferring
// it. It could only see the rows whose mirror is an exact negation, so it caught the
// three `Cur` slows and missed the `Max` run-cap row that routes to `CapBump`, which
// is the whole of MOVEMAP-6. And it had to be taught, in prose, which look-alike pair
// NOT to match — Thunderspy's Increase Density, whose `0.05` self row is half its
// `0.1` aura row rather than a negation, and is a real self penalty. `route` now asks
// the export instead, so both cases fall out of the same test with nothing to exempt.
