//! PROD6C-3k, the effective power: which power a display surface is actually showing.
//!
//! Every other display transform is a pure function of one power object. This one decides
//! WHICH power that is. A snipe shows its fast in-combat form; a from-Hide opener shows its
//! slow animation only while hidden; a power with mode-gated contributions shows them merged
//! in when their mode is on. The beta resolves this as a chain of memos in `InfoPanel`
//! (single-sourced as `resolveEffectivePower`), and the projection has to resolve the same
//! chain before it reads a single stat. The values it emits are keyed to the base power's
//! identity but describe the effective one.
//!
//! Five transforms, applied in the beta's order (the third is the rebuild's own, since the beta
//! had no mechanism for it):
//! 1. mode redirect (`applyModeRedirect`). While a caster mode is live the game's
//!    PowerRedirector fires a different record entirely: a Kheldian attack in Nova form, a Titan
//!    Weapons attack under Momentum, Seismic Blast under Seismic Power. The converter carries the
//!    binary's whole `Redirect` table as `modeVariants`, keyed by the mode that selects it, each
//!    entry carrying its own ATOMS along with its display fields. The variant is a different
//!    record, so the projection has to resolve its damage from its own list (MODEVAR-1).
//! 2. form variant ([`with_form_variant`]). A redirect table whose branches are selected by a
//!    condition rather than by a mode or by dropping an interrupt: Energy Transfer's charged
//!    pair, Water Jet's and Rending Flurry's combo-stack gates, the Arachnos Soldier attacks that
//!    fire as a Crab or Bane weapon. Each branch carries its condition verbatim and the engine
//!    evaluates it, first match wins, as the game's redirector does (CHAIN-1).
//! 3. quick snipe (`applyQuickSnipe`). A `quickSnipe` power swaps in the fast cast/range and the
//!    fast form's damage, both the bag key the card renders and the ATOMS the projection resolves
//!    the number from, whenever that form's OWN carried gate holds for the build. Each fork
//!    authors a different gate (Homecoming's combat/Marksman test, the forks' ToHit threshold),
//!    so this is evaluated, never assumed. See [`fast_form_selected`].
//! 4. mid-combat cast (`InfoPanel`'s `formAdjustedPower`). When NOT hidden, a power with a
//!    `midCombatCast` casts at that time instead of its from-Hide animation.
//! 5. active conditionals (`selectActiveConditionals` + `applyActiveConditionals`). The
//!    mode-gated contributions whose toggle is on merge onto `effects` and `damage` — and onto
//!    `stats`, for a toggle that selects a redirect branch with its own recharge and endurance
//!    ([`conditional_stats`]). That last is the rebuild's own: the beta had no channel for it, and
//!    a mode mechanic modelled here rather than as a record swap otherwise loses its whole timing.
//!
//! Two deliberate boundaries, both matching what the beta surfaces do with the result:
//! * The additive-collision instances (`applyActiveConditionals`'s `extraInstances`, a second
//!   simultaneous mez the merge refuses to fold into one stronger row) are dropped. The beta
//!   renders them from a separate prop, not from the bag, so no projected row carries them.
//! * A power's slotting and perma stay keyed to the base power, as both beta surfaces keep them:
//!   enhancement reads `allowedEnhancements`, which no transform here touches, and the perma
//!   tracker has never read the merged power.

use crate::expr::{eval_bool, SourceContext};
use crate::window_slots::slots_over;
use coh_data::{AtomicEffect, CharacterState, DatasetId, Power, PowerDatabase};
use serde_json::{Map, Value};
use std::borrow::Cow;

/// Timing metadata the from-Hide animation owns and the mid-combat one doesn't, so a swapped
/// cast must not leave them standing (the beta clears both). Nothing in the projection reads
/// them today; they're cleared so the effective power is the object the beta produces.
const FROM_HIDE_TIMING_KEYS: [&str; 2] = ["interruptTime", "timeToRoot"];

/// The power the display surfaces render for `power`, given the build's combat state and the
/// `gate` context its data-carried form conditions are evaluated against.
///
/// `db` answers two things the conditionals need. One is the build's class token
/// ([`coh_data::caster_class_name`]): a conditional can fork on the caster's archetype, and a
/// Scrapper holding Cross Punch must not get its Dominator-only bonus laid over the displayed
/// power. `None` (no archetype chosen) drops every forked entry, matching the totals (COND-4).
/// The other is whether a conditional's ownership claim names a power the player picks
/// ([`coh_data::picks_answer`]), which the picks decide instead of a toggle.
///
/// Borrowed when no transform applies, the common case since most powers carry no `quickSnipe`,
/// no `midCombatCast`, and no active conditional.
pub fn effective_power<'a>(
    power: &'a Power,
    state: &CharacterState,
    gate: &SourceContext,
    db: &PowerDatabase,
) -> Cow<'a, Power> {
    let caster_class = coh_data::caster_class_name(state, db);
    let mut effective = Cow::Borrowed(power);
    if let Some(swapped) = with_mode_variant(&effective, state) {
        effective = Cow::Owned(swapped);
    }
    if let Some(swapped) = with_form_variant(&effective, gate) {
        effective = Cow::Owned(swapped);
    }
    if let Some(swapped) = with_quick_snipe(&effective, gate) {
        effective = Cow::Owned(swapped);
    }
    if !state.combat.hidden {
        if let Some(swapped) = with_mid_combat_cast(&effective) {
            effective = Cow::Owned(swapped);
        }
    }
    let active = active_conditionals(&effective, state, caster_class, db);
    if !active.is_empty() {
        effective = Cow::Owned(with_active_conditionals(&effective, state.dataset, &active));
    }
    effective
}

/// The variant the game redirects to while one of the build's modes is live. `None` when the
/// power has no mode table or no live mode selects an entry.
///
/// Slots, enhancements and picker entry live on the base and every mode shares them, so the base
/// keeps its identity and only the display fields the variant publishes are replaced. When more
/// than one live mode has a variant the power's own table order decides, which is the order the
/// binary lists its redirects in (`modeVariants` is emitted in that order and serde's map
/// preserves it).
///
/// Both damage routes move together, as they do for the fast form ([`with_quick_snipe`]): the
/// variant's display fields, and the ATOMS the projection resolves its number from. A swap that
/// moved only the display left the variant's cast and area standing over the base record's
/// damage, which the forks disagree with in both directions: Homecoming's Stalagmite states 2.92
/// under Seismic Power against a base 0.75, its Crushing Blow 1.32 against 1.64 (MODEVAR-1).
fn with_mode_variant(power: &Power, state: &CharacterState) -> Option<Power> {
    if state.combat.active_modes.is_empty() {
        return None;
    }
    let variants = power.extra.get("modeVariants").and_then(Value::as_object)?;
    let (mode, variant) = variants
        .iter()
        .find(|(mode, _)| state.combat.active_modes.contains(*mode))
        .and_then(|(mode, variant)| Some((mode, variant.as_object()?)))?;
    let mut swapped = power.clone();
    for (key, value) in variant {
        // `internalName` identifies the variant for evidence; the effective power keeps the base
        // power's identity, which every projected value is keyed to. `atoms` are carried decoded
        // on the power itself and taken from there below, so the raw tuples aren't copied into
        // `extra`. Copying them would leave a second, undecoded copy of the same list that
        // nothing would read.
        if key == "internalName" || key == "atoms" {
            continue;
        }
        swapped.extra.insert(key.clone(), value.clone());
    }
    // Unconditional for the same reason the fast form's is: a power that DECODED with a
    // `modeVariants` entry always carries that entry's atoms
    // (`PowerDecodeError::ModeVariantWithoutAtoms`), so the empty fallback is reachable only from
    // a `Power` literal assembled in Rust, whose base list is empty too.
    swapped.atoms = power
        .mode_variant_atoms
        .get(mode)
        .cloned()
        .unwrap_or_default();
    Some(swapped)
}

/// The record a redirect table selects by an evaluated condition: the third form mechanism,
/// beside the mode variant above and the fast form below (CHAIN-1).
///
/// The game's PowerRedirector walks the table in order and fires the FIRST branch whose condition
/// holds, so this does the same, in the order the converter emitted and serde preserves. Energy
/// Transfer is the shape that needed it: two complementary branches,
/// `…Energy_Store source.ownPower?` and the same negated, with no unconditional default for
/// either existing detector to work from.
///
/// Only a definite `Ok(true)` selects, the same posture [`fast_form_selected`] takes: a condition
/// this build can't answer (a costume token, a target test) leaves the base record standing
/// rather than guessing at a form. That is still most of the corpus. The branches selected by
/// PICKS resolve, and so do the ones naming transient combat state the build has DECLARED
/// (`gather::owned_powers` folds the conditional toggles' claims in), and
/// the unanswerable population is counted, a measured number rather than a silence.
///
/// The walk STOPS at an indeterminate branch rather than skipping it (ASFORM-1): the game's
/// redirector fires the first true branch of concrete values, so selecting a later branch here
/// asserts every earlier one is false, which an unanswerable condition precisely cannot say.
/// The meter tables are where it bites — Assassin's Strike is `kMeter < .9` over a
/// constant-true Stealth fallback, and a build whose meter is unbound must keep its base
/// record (with its gates reported unresolved), not be shown the fallback as if the meter
/// were known high.
fn with_form_variant(power: &Power, gate: &SourceContext) -> Option<Power> {
    let variants = power.extra.get("formVariants").and_then(Value::as_array)?;
    let mut selected = None;
    for (index, variant) in variants.iter().enumerate() {
        let Some(condition) = crate::expr::json_tokens(variant.get("condition")) else {
            continue;
        };
        match eval_bool(&condition, gate) {
            Ok(true) => {
                selected = Some((index, variant));
                break;
            }
            Ok(false) => continue,
            Err(_) => return None,
        }
    }
    let (index, variant) = selected?;
    let variant = variant.as_object()?;

    let mut swapped = power.clone();
    for (key, value) in variant {
        // `condition` selected the variant and `internalName` identifies it for evidence; the
        // effective power keeps the base power's identity, which every projected value is keyed
        // to. `atoms` are carried decoded on the power itself and taken from there below.
        if matches!(key.as_str(), "condition" | "internalName" | "atoms") {
            continue;
        }
        swapped.extra.insert(key.clone(), value.clone());
    }
    // A variant that DECODED always carries atoms (`PowerDecodeError::FormVariantWithoutAtoms`),
    // so the empty fallback is reachable only from a `Power` assembled in Rust, whose base list
    // is empty too.
    swapped.atoms = power
        .form_variant_atoms
        .get(index)
        .cloned()
        .unwrap_or_default();
    Some(swapped)
}

/// The `extra` object under `key`, cloned so it can be edited into the effective power.
fn object(power: &Power, key: &str) -> Option<Map<String, Value>> {
    power.extra.get(key).and_then(Value::as_object).cloned()
}

/// Replace (or remove, for `None`) an `extra` key on an owned power.
fn set(power: &mut Power, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            power.extra.insert(key.to_string(), value);
        }
        None => {
            power.extra.remove(key);
        }
    }
}

/// Whether the build satisfies the fast form's OWN gate, the redirect condition the converter
/// carried through verbatim, evaluated here rather than re-derived.
///
/// This used to be `state.combat.in_combat`, which is Homecoming's mechanic and only
/// Homecoming's: it gates on `kEngaged Source.Mode? … Experienced_Marksman source.ownPower? ||`,
/// while Rebirth and Thunderspy fork from before that change and still author the original
/// `cur.kToHit source> .97 >=`. Firing the fork's fast form off an In-Combat toggle would show a
/// form the fork's own data says isn't available (SNIPE-2). `kEngaged` now binds from `in_combat`
/// in [`crate::gather::live_modes`], so Homecoming's answer still comes out of the toggle,
/// through the gate the export states, not around it.
///
/// Only a definite `Ok(true)` selects the fast form. An Indeterminate gate leaves the slow one
/// standing, the same conservative posture [`crate::gather::active_conditional_powers`] and
/// [`crate::inherents`] take; an absent or malformed gate can only come from a stale or broken
/// bundle, which is forbidden corpus-wide because this function has no
/// error channel to report it through.
fn fast_form_selected(snipe: &Map<String, Value>, gate: &SourceContext) -> bool {
    let Some(condition) = crate::expr::json_tokens(snipe.get("condition")) else {
        return false;
    };
    matches!(eval_bool(&condition, gate), Ok(true))
}

/// Whether this power's fast form is one a change in the CASTER's to-hit can select, i.e.
/// whether its gate reads `cur.kToHit` off the source.
///
/// The forks' snipes gate on `cur.kToHit source> .97 >=`, so a team to-hit buff genuinely
/// switches the form and every number the rotation reads off it; Homecoming's gate names no
/// to-hit term at all, so the same buff reaches nothing. Which of those a build is looking at is
/// a property of the condition the export ships, never of the fork, so the chain-side control
/// asks this rather than branching on a dataset (CHAIN-1).
///
/// The condition is postfix ([`crate::expr::eval`]), and an attribute becomes the caster's own
/// only as the operand of a source-side reader. `cur.kToHit target>` reads the TARGET's to-hit,
/// which no build-side buff moves. So the read is structural, not textual: the pair is what
/// counts, not the bare symbol appearing somewhere.
pub fn fast_form_reads_caster_to_hit(power: &Power) -> bool {
    let Some(condition) = crate::expr::json_tokens(
        power
            .extra
            .get("quickSnipe")
            .and_then(Value::as_object)
            .and_then(|snipe| snipe.get("condition")),
    ) else {
        return false;
    };
    condition.windows(2).any(|pair| {
        &*pair[0] == crate::expr::CURRENT_TO_HIT && matches!(&*pair[1], "source>" | "Source>")
    })
}

/// The fast (uninterruptible) redirect form, the beta `applyQuickSnipe`. `None` when the power
/// has no fast form, or when the build doesn't satisfy that form's gate.
///
/// The fast form's damage REPLACES the slow form's rather than adding to it (it's the same
/// attack fired differently), and its stats overwrite the base's on the keys it publishes.
///
/// Both damage routes move together: the `damage` bag key the display surfaces render, and the
/// ATOMS the projection resolves the number from. Swapping only the bag left the fast cast
/// beside the slow form's charged hit, and because Homecoming trades roughly half the damage
/// for the shorter cast, its snipes read about 2× (SNIPE-3).
fn with_quick_snipe(power: &Power, gate: &SourceContext) -> Option<Power> {
    let snipe = power.extra.get("quickSnipe").and_then(Value::as_object)?;
    if !fast_form_selected(snipe, gate) {
        return None;
    }
    let snipe_stats = snipe.get("stats").and_then(Value::as_object);
    let mut swapped = power.clone();
    // Unconditional: a loaded power with a `quickSnipe` always carries the form's atoms (the
    // bundle can't say otherwise: `PowerDecodeError::FastFormWithoutAtoms`), and a power with
    // none is one a test built by hand, whose base list is empty too.
    swapped.atoms = power.quick_snipe_atoms.clone();

    // A power with no `stats` object keeps none. The beta leaves it untouched rather than
    // synthesizing one from the snipe form.
    if let Some(mut stats) = object(power, "stats") {
        if let Some(snipe_stats) = snipe_stats {
            for (key, value) in snipe_stats {
                stats.insert(key.clone(), value.clone());
            }
        }
        // The fast form has no interruptible channel; the slow form's must not carry over.
        stats.remove("interruptTime");
        set(&mut swapped, "stats", Some(Value::Object(stats)));
    }

    set(&mut swapped, "damage", snipe.get("damage").cloned());

    // The bag half of this swap is gone. It published the fast form's stats
    // into `effects` for the pool/epic partition, which used to carry its execution stats there
    // and stopped on 2026-09-03. This also runs BEFORE [`with_active_conditionals`], the only
    // thing that puts a bag on a power now, so the `if let` never bound on any fork.

    Some(swapped)
}

/// The uninterruptible mid-combat animation, the beta `InfoPanel` `formAdjustedPower`. `None`
/// when the power has no separate mid-combat cast.
fn with_mid_combat_cast(power: &Power) -> Option<Power> {
    let cast_time = power.extra.get("midCombatCast")?.as_f64()?;
    let mut swapped = power.clone();
    // The beta spreads over a possibly-absent `stats`, so a power with none gains one here.
    let mut stats = object(power, "stats").unwrap_or_default();
    stats.insert("castTime".to_string(), Value::from(cast_time));
    for key in FROM_HIDE_TIMING_KEYS {
        stats.remove(key);
    }
    set(&mut swapped, "stats", Some(Value::Object(stats)));
    Some(swapped)
}

/// The active subset of the power's `conditionalEffects`, the beta `selectActiveConditionals`.
///
/// A `global`-scoped entry reads its bare id from [`CombatContext::global_conditionals`]; a
/// `per-power` one reads `"<internalName>:<id>"` from `per_power_conditionals`. An untouched
/// toggle falls back to the entry's own `defaultActive`.
///
/// The beta routes one id (Domination) to the Header's own mechanic state instead of the
/// toggle maps. That binding stays on the surface side, which writes the resolved state into
/// the global map before it reaches here. The engine reads the data's scope, not a curated
/// list of mechanic names (Rule 0).
fn active_conditionals<'a>(
    power: &'a Power,
    state: &CharacterState,
    caster_class: Option<&str>,
    db: &PowerDatabase,
) -> Vec<&'a Value> {
    let Some(list) = power
        .extra
        .get("conditionalEffects")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let internal_name = power.internal_name.as_deref().unwrap_or_default();
    list.iter()
        .filter(|conditional| coh_data::conditional_for_class(conditional, caster_class))
        .filter(|conditional| {
            let Some(id) = conditional.get("id").and_then(Value::as_str) else {
                return false;
            };
            // A claim on a power the player picks follows the picks, not a toggle.
            if let Some(owned) = coh_data::picks_answer(conditional, state, db) {
                return owned;
            }
            let default = conditional
                .get("defaultActive")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let global = conditional.get("scope").and_then(Value::as_str) == Some("global");
            let toggle = if global {
                state.combat.global_conditionals.get(id)
            } else {
                state
                    .combat
                    .per_power_conditionals
                    .get(&format!("{internal_name}:{id}"))
            };
            toggle.copied().unwrap_or(default)
        })
        .collect()
}

/// Where [`with_active_conditionals`] records what the live conditionals CONTRIBUTED, as
/// opposed to what the merged bag ends up holding.
///
/// Engine-owned and never on the wire: no converter writes it and nothing round-trips it. It
/// exists because the two questions came apart. `effects` answers "what does this power's bag
/// hold now", which the display rebuilds from atoms and does not need; this answers "what did
/// the toggles add", which the display cannot rebuild at all, because a conditional's atoms are
/// `gated` on every fork and the bag mirror's subset ([`crate::window_slots`]) drops gated rows
/// by construction.
pub const CONDITIONAL_DELTA_KEY: &str = "activeConditionalEffects";

/// Layer the active conditionals onto the power, the beta `applyActiveConditionals`.
///
/// `effects` is the beta's own merged object and stays that, so the parity oracle keeps
/// comparing the same artifact. The delta is stamped BESIDE it rather than derived back out of
/// it, because the subtraction is not recoverable: a `replace` conditional overwrites a base key
/// with its own value and the result is indistinguishable from a base that always read that way.
fn with_active_conditionals(power: &Power, dataset: DatasetId, active: &[&Value]) -> Power {
    let mut merged = power.clone();
    set(&mut merged, "damage", merged_damage(power, active));
    let delta = conditional_delta(power, dataset, active);
    if let Some(effects) = merged_effects(&delta) {
        set(&mut merged, "effects", Some(Value::Object(effects)));
    }
    if !delta.is_empty() {
        set(
            &mut merged,
            CONDITIONAL_DELTA_KEY,
            Some(Value::Object(delta)),
        );
    }
    let stats_patch = conditional_stats(active);
    if !stats_patch.is_empty() {
        // Spread over a possibly-absent `stats`, as [`with_mid_combat_cast`] does: a power
        // carrying none gains one here rather than dropping the patch.
        let mut stats = object(power, "stats").unwrap_or_default();
        for (key, value) in stats_patch {
            stats.insert(key, value);
        }
        set(&mut merged, "stats", Some(Value::Object(stats)));
    }
    merged
}

/// The execution stats the active conditionals state for themselves — the TIMING half of a
/// caster-mode mechanic (`conditionalEffects[].stats`).
///
/// A modal redirect table's branches are separate power records, and their recharge and endurance
/// can differ while their effects are identical. Dual Pistols' Suppressive Fire is that table
/// whole: 20s and 10.192 end on standard rounds, 8s and 8.53 with any special ammo loaded, and
/// nothing else between the two records at all. The mechanic is modelled as `conditionalEffects`
/// rather than as a record swap ([`with_form_variant`] declines a wholly modal table on purpose),
/// so the branch's numbers ride on the toggle that selects it and are laid over `stats` here.
///
/// Only `stats` is written, which is enough: every reader takes `stats.<key>` over the bag's copy
/// when it is truthy ([`crate::projection::truthy_stat`]), and the converter emits a patch key only
/// where the branch states a real value.
///
/// First active entry wins a key, the same precedence [`conditional_delta`] applies to the bag —
/// one policy for one merge rather than a second one here. Two of these CAN be on at once: the ammo
/// toggles are plain `global` booleans and the converter's exclusive-group annotation covers only
/// `adaptation` and `combo` ids, so nothing stops a build declaring Fire and Ice ammo together even
/// though the game loads one. What makes the tie-break safe is narrower and measured: every toggle
/// carrying stats on one power carries the SAME values, so whichever wins states the same number.
/// Nothing in this repo re-checks that. If a fork ever gives two ammos different recharges, this
/// policy becomes a real decision rather than a formality, and nothing will say so.
fn conditional_stats(active: &[&Value]) -> Map<String, Value> {
    let mut patch = Map::new();
    for conditional in active {
        let Some(stats) = conditional.get("stats").and_then(Value::as_object) else {
            continue;
        };
        for (key, value) in stats {
            if !patch.contains_key(key) {
                patch.insert(key.clone(), value.clone());
            }
        }
    }
    patch
}

/// Merge each active conditional's damage onto the power's base.
///
/// A base row names the toggles it's mutex with in `displacedBy`, the converter's join between a
/// gated row and the conditional whose predicate its own gate negates. An active toggle takes
/// that row's place instead of stacking on it; everything else concatenates, since the calc sums
/// components the way it sums a multi-component attack.
///
/// `mode` is deliberately not consulted here, and this is the half of PAR2 the beta closed first.
/// Reading `mode: "replace"` as "drop the whole base array" deletes real damage: Psi Blade's
/// Insight is tagged replace off a negated gate on its GrantPower atom while its own damage is a
/// genuinely extra DoT, so swapping the array wholesale loses the base strike. The rows that
/// should go are exactly the ones that say so.
fn merged_damage(power: &Power, active: &[&Value]) -> Option<Value> {
    let active_ids: Vec<&str> = active
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str))
        .collect();
    let mut entries: Vec<Value> = damage_entries(power.extra.get("damage"))
        .into_iter()
        .filter(|row| !displaced_by_active(row, &active_ids))
        .collect();
    for conditional in active {
        entries.extend(damage_entries(conditional.get("damage")));
    }
    match entries.len() {
        0 => None,
        1 => Some(entries.remove(0)),
        _ => Some(Value::Array(entries)),
    }
}

/// Whether one base damage row is displaced by a toggle that's currently on.
fn displaced_by_active(row: &Value, active_ids: &[&str]) -> bool {
    let Some(displaced) = row.get("displacedBy").and_then(Value::as_array) else {
        return false;
    };
    displaced
        .iter()
        .filter_map(Value::as_str)
        .any(|id| active_ids.contains(&id))
}

/// A `damage` field as a list of entries: an array as-is, a single object as one entry, absent
/// as none.
fn damage_entries(damage: Option<&Value>) -> Vec<Value> {
    match damage {
        Some(Value::Array(entries)) => entries.clone(),
        Some(Value::Null) | None => Vec::new(),
        Some(value) => vec![value.clone()],
    }
}

/// `delta` as the bag the effective power carries, or `None` when there is no delta.
///
/// This is the beta's `applyActiveConditionals` object and exists to stay that, so the parity
/// oracle keeps comparing the same artifact. The rules that decide what is IN the delta are
/// [`conditional_delta`]'s; the only thing left here is the empty case.
///
/// **The base half is gone.** It read the power's own `effects` bag, and no
/// fork has carried one since 2026-09-03. Nothing writes one before this point either — the
/// mode, form, quick-snipe and mid-combat swaps above all leave `effects` untouched, and this
/// merge is the only writer left in the crate. The fold had nothing to fold onto, so the
/// result was always the delta itself.
fn merged_effects(delta: &Map<String, Value>) -> Option<Map<String, Value>> {
    // No delta means no bag. Materializing an empty object here is a diff the base power does
    // not have, which would make a control whose whole contribution was dropped look like it
    // moved the power.
    if delta.is_empty() {
        return None;
    }
    Some(delta.clone())
}

/// What the active conditionals ADD to the power, and nothing else — the merge's own half,
/// with the base's left out.
///
/// Split out of [`merged_effects`] because the base half and the added half now have different
/// consumers. The base bag is on its way off the wire, and the display already rebuilds it from
/// the atoms ([`crate::granted::display_effects`]); what the display cannot rebuild is this,
/// because every conditional's atoms are `gated` on all four forks and the bag mirror's subset
/// drops them by construction. So the merged bag stopped being the display's source and this
/// delta became it, carried on the effective power under [`CONDITIONAL_DELTA_KEY`].
///
/// The collision rules are the merge's, unchanged. `mode: "replace"` shallow-merges with the
/// conditional winning, since it's mutually exclusive with a base sibling (a stance replacing
/// the baseline self-buff, a drowning debuff replacing the not-drowning one). The default
/// `additive` mode contributes only keys the base LACKS: a colliding key means two simultaneous
/// instances, and overwriting the base with one of them would display a single misleadingly
/// stronger effect. The beta records those collisions as `extraInstances` for a separate row;
/// nothing in the projection carries them.
fn conditional_delta(power: &Power, dataset: DatasetId, active: &[&Value]) -> Map<String, Value> {
    let mut delta = Map::new();
    // STRIP-1: the collision check reads the ATOM-PROJECTED base surface, not the literal base
    // bag, which the strip emptied for ordinary powerset powers. Key PRESENCE under the
    // pre-strip bag is that surface — the same `window_slots` [`crate::adjusters`] keys its
    // collision classification on. Without this, a colliding conditional inserts its weaker copy
    // into the empty bag and changes the effective power exactly like a real contribution, while
    // the control surface says it shows nothing (the Time_Stop `time_crawl_debuff` shape).
    // `durations` lives as its own map, so its collision is presence of the base map, not a
    // key in [`WindowSlots::keys`].
    let base_slots = crate::window_slots::window_slots(power, dataset);
    for conditional in active {
        let Some(from) = conditional_effects(power, dataset, conditional) else {
            continue;
        };
        let replace = conditional.get("mode").and_then(Value::as_str) == Some("replace");
        for (key, value) in from {
            // Key PRESENCE decides, not truthiness: an authored zero or null is still a base
            // value the additive merge must not overwrite.
            let base_collides = base_slots.keys().contains(key.as_str())
                || (key == "durations" && !base_slots.durations.is_empty());
            if replace || !delta.contains_key(&key) {
                if !replace && base_collides {
                    continue;
                }
                delta.insert(key, value);
            }
        }
    }
    delta
}

/// One active conditional's contribution, projected from the ATOMS it claims.
///
/// The join is [`coh_data::AtomicEffect::conditional_id`], the converter stamp the adjusters
/// derivation already reads ([`crate::adjusters`]) — an entry's atoms are the gated rows the
/// group it was built from encoded, so projecting exactly those through the bag mirror rebuilds
/// the block the wire used to carry. `extractConditionalEffects` calls `extractEffects` on the
/// group's own templates, which is what [`slots_over`]'s subset scope reproduces: each entry's
/// bag is projected against itself and never against the power's base.
///
/// `summon` is the one key taken from the authored block, because it is the one key with no atom
/// value behind it: `extractSummon` builds it from the template's pet parameters outside the
/// projection (ENT-14), so the mirror states the key and not its object.
///
/// An entry whose atoms are absent projects nothing, which is the honest answer for the
/// payload-less entries the export declares and matches [`crate::adjusters`]'s `is_inert`.
fn conditional_effects(
    power: &Power,
    dataset: DatasetId,
    conditional: &Value,
) -> Option<Vec<(String, Value)>> {
    let authored = conditional.get("effects").and_then(Value::as_object);
    let id = conditional.get("id").and_then(Value::as_str)?;
    let slots = slots_over(power, dataset, |atom: &AtomicEffect| {
        atom.conditional_id.as_deref() == Some(id)
    });
    let mut out: Vec<(String, Value)> = slots
        .values
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    if !slots.durations.is_empty() {
        let durations: Map<String, Value> = slots
            .durations
            .iter()
            .map(|(key, seconds)| ((*key).to_owned(), Value::from(*seconds)))
            .collect();
        out.push(("durations".to_owned(), Value::Object(durations)));
    }
    if let Some(seconds) = slots.buff_duration {
        out.push(("buffDuration".to_owned(), Value::from(seconds)));
    }
    if let Some(seconds) = slots.effect_duration {
        out.push(("effectDuration".to_owned(), Value::from(seconds)));
    }
    if slots.present.contains("summon") {
        if let Some(summon) = authored.and_then(|block| block.get("summon")) {
            out.push(("summon".to_owned(), summon.clone()));
        }
    }
    (!out.is_empty()).then_some(out)
}
