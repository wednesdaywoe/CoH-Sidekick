//! Atom-side mirror of the converter's bag projection, scoped to what the window consumers
//! read: which slot keys exist, the per-key `durations` entry, the `toWho: Self` marks on the
//! self-penalty slots, and the `buffDuration` / `effectDuration` power-level fallbacks. The
//! chain timeline ([`crate::chain_build`]) and the perma ring ([`crate::perma`]) both ask these
//! questions of the `effects` bag today; this module answers them from `Power::atoms` so the
//! bag can leave the contract.
//!
//! This is a per-key mirror of `projectAtomsToEffects` (`scripts/convert-powerset.cjs:6224`),
//! not a key-name→EffectType guess. The first attempt at this migration (2026-08-16, reverted)
//! built a hand map from the key names and moved ~35% of the chain surface: a bag key carries
//! sign and audience that the EffectType drops, so `tohitDebuff` and `tohitBuff` both matched
//! `EffectType::ToHit` and foe debuffs read as self-buff windows. Every branch below is read
//! from the converter, and `chain_window_atom_bag_parity` / `perma_window_atom_parity` hold the
//! result to the bag it replaces over every click power of all three bundles.
//!
//! What the converter tests on raw strings, the wire spells as typed fields; the transfer rules
//! come from `bridgeAttrib` (`scripts/_atomic-effect.ts`):
//!   * raw attrib families arrive as `effect_type` + `sub_type`. Two folds matter here: a bare
//!     damage/position attrib on a `*Res*` table is `Resistance` at EVERY aspect (so
//!     `Resistance` + `Str` is the converter's specialBuff branch, and `Resistance` + `Cur` is
//!     its defense branch), and mez/control/knockback attribs share `Mez`/`MezResist`/
//!     `Enhancement` split by aspect, with the converter's own key map re-derived from the sub
//!     type.
//!   * raw `aspect` maps 1:1 (`Strength` ⟺ `Str`, …); absent stays `Unspecified` and fails
//!     every aspect test, exactly like the raw empty string.
//!   * `modifier_table` is the raw table byte-for-byte, so the `.includes('debuff')` /
//!     `.includes('slow')` / `.includes('res_boolean')` tests transfer after lowercasing.
//!   * the bag's atom subset is `!gated` AND no `caster_archetypes` (`_bagTemplates` drops
//!     archetype-forked templates; everything else the bag never saw is stamped `gated`), minus
//!     `OnDeactivate` rows, which the routing loop skips. That is [`window_slots`], and it is an
//!     UNDER-approximation of the bag: the converter follows it with `_addUnanimousForkedSlots`,
//!     which restores the slots the fork turned out not to fork. [`bag_slots`] is the whole
//!     mirror, and the display consumers want it.
//!   * wire `duration` is `0.0` where the bag's parse said `null`; both sides record only
//!     positive durations, so `> 0.0` is the shared test.
//!
//! The projection is not the last word on a slot. `mergeStackingPatches` runs after
//! `extractEffects` returns and REBUILDS the slots the per-foe pass touched, dropping every mark
//! the projection wrote there ([`State::per_foe_patches`], PERFOE-1). It reads its own subset,
//! keys through its own coarser classifier, and merges by its own branch order — so it is a
//! separate pass here too, after `finish`'s post-passes, and not a case in the router.
//!
//! One ordering is faithful to the converter and not currently distinguishable by the corpus:
//! the two Thunderspy target-trap guards run on the FINISHED power (`convert-powerset.cjs:8940`),
//! after the per-archetype merge, so [`bag_slots`] projects its arms unstripped and guards once
//! at the end. 49 Thunderspy powers carry a forked atom and 2 of those also lose a key to a
//! guard, but on both the per-arm and post-merge orders agree — so the shape here comes from the
//! converter's call site rather than from evidence, and `display_slot_presence_atom_bag_parity`
//! stays green either way.
//!
//! Deliberately NOT mirrored, each measured corpus-vacuous against the shipped bundles or
//! handled by the parity guards:
//!   * the `durations` suffix-variant lookup (`key_*`): zero suffixed keys exist on any fork.
//!   * the Dual Pistols ammo strip: zero `dual_pistols` powers reach the player bundles.
//!   * resistible-twin coalescing and the AttackType marker group scan need the template
//!     grouping the wire doesn't carry; the twin routes to the same key with the same duration
//!     (so skipping the sibling changes nothing here), and a marker row is approximated
//!     per-atom (`Str` aspect, zero scale, zero magnitude).

use crate::granted::{PET_CLASS_KEY, SCALE_TERMS_KEY};
use coh_data::{
    expression_text, lands_on_caster, Aspect, AtomicEffect, AttribType, DatasetId, EffectType,
    Power, PvMode, Stacking, SubType, ToWho,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The window-relevant view of one power's would-be effects bag, derived from its atoms.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowSlots {
    /// Slot keys the projection created — the bag's `effects.contains_key` surface.
    pub present: BTreeSet<&'static str>,
    /// Each created slot's value object, as the converter authors it. A key in
    /// [`present`](Self::present) and absent here is one this projection states without a value:
    /// `summon` alone, whose value `extractSummon` builds from the template outside
    /// `projectAtomsToEffects` (ENT-14).
    pub values: Obj,
    /// The bag's `effects.durations` map: per key, the surviving recorded duration.
    pub durations: BTreeMap<&'static str, f64>,
    /// Self-penalty slots (`damageDebuff` / `rechargeDebuff` / `tohitDebuff` /
    /// `accuracyDebuff`) whose surviving entry carries `toWho: Self`.
    pub self_marked: BTreeSet<&'static str>,
    /// Any `slow[axis]` entry whose surviving write was the self-directed branch.
    pub slow_self: bool,
    /// The bag's `buffDuration`: plurality vote over `durations`, ties to the larger value.
    pub buff_duration: Option<f64>,
    /// The bag's `effectDuration`: the last routed mez magnitude row's duration.
    pub effect_duration: Option<f64>,
}

impl WindowSlots {
    /// Every bag key this projection would have written, as the bag itself spells them.
    ///
    /// [`present`](Self::present) is the slot set, and two more keys live as fields of their own
    /// because the window consumers read them as fallbacks rather than as contributions:
    /// `buffDuration` and `effectDuration` are ordinary keys in the bag, so a consumer asking
    /// "which keys does this project" has to see them. `durations` is deliberately NOT here —
    /// it is the per-key duration map, not a contribution of its own.
    pub fn keys(&self) -> BTreeSet<&'static str> {
        let mut keys = self.present.clone();
        if self.buff_duration.is_some() {
            keys.insert("buffDuration");
        }
        if self.effect_duration.is_some() {
            keys.insert("effectDuration");
        }
        keys
    }

    /// The bag-shape window resolve (`chain_build`'s `window_duration`, minus the
    /// corpus-vacuous suffix lookup): for the first present key with a positive `durations`
    /// entry, that entry; else the fallbacks in order; 0 when nothing positive is found.
    /// Presence without any duration anywhere is still 0.
    pub fn window_duration(&self, keys: &[&str], fallbacks: &[Fallback]) -> f64 {
        let present: Vec<&&str> = keys.iter().filter(|k| self.present.contains(**k)).collect();
        if present.is_empty() {
            return 0.0;
        }
        for key in &present {
            if let Some(d) = self.durations.get(**key).filter(|d| **d > 0.0) {
                return *d;
            }
        }
        for fb in fallbacks {
            let d = match fb {
                Fallback::BuffDuration => self.buff_duration,
                Fallback::EffectDuration => self.effect_duration,
            };
            if let Some(d) = d.filter(|d| *d > 0.0) {
                return d;
            }
        }
        0.0
    }

    /// The bag's `hasSelfDirectedPenalty`: a debuff the caster suffers, on the four penalty
    /// slots or any `slow` axis.
    pub fn has_self_directed_penalty(&self) -> bool {
        !self.self_marked.is_empty() || self.slow_self
    }
}

/// The two power-level duration fallbacks a window resolve may consult.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fallback {
    BuffDuration,
    EffectDuration,
}

/// Project one power's atoms to its window slots. `dataset` gates the Thunderspy-only routing
/// rules the converter keys on `datasetId`.
pub fn window_slots(power: &Power, dataset: DatasetId) -> WindowSlots {
    slots_inner(power, dataset, bag_subset, false, true)
}

/// `_bagTemplates`: ungated, unforked, minus the `OnDeactivate` rows the routing loop skips.
///
/// Public because a consumer that joins a slot back to the atom that CREATED it has to ask
/// over the same subset — a row this predicate drops routed nowhere, so it can never be the
/// evidence behind a slot ([`crate::perma::self_state_window_from_atoms`]).
pub fn bag_subset(a: &AtomicEffect) -> bool {
    !a.is_gated() && a.caster_archetypes.is_none() && !a.is_deactivation_burst()
}

/// The WHOLE bag mirror: [`window_slots`] plus every slot the archetype fork turned out not to
/// fork (`_addUnanimousForkedSlots`, `scripts/convert-powerset.cjs:5179`).
///
/// `_bagTemplates` drops a forked template because a shared bag has nowhere to say "for a
/// Peacebringer", and that is right when the value depends on the archetype and wrong for the
/// commoner case where a power enumerates the roster in its gate and hands every arm the same
/// value. Rebirth Combat Jumping forks only to carve out a Kheldian hover clause and both arms
/// buff defense .25, so the narrow subset has those pool powers describing themselves as doing
/// nothing. The converter answers by projecting once per archetype and stating a slot only when
/// every arm agrees; this is that pass, over the same roster.
///
/// `classes` is the dataset's own player class tokens (`PowerDatabase::class_name_of` over the
/// archetype catalog), and it has to be the real roster rather than the tokens the power's own
/// gate happens to name: an archetype no arm names sees exactly the unforked subset, which is
/// the base bag, which by construction lacks the key — so a fork that does not cover the roster
/// restores nothing, and only a full roster read can tell those two cases apart. An empty
/// `classes` therefore degrades to [`window_slots`], which is the answer for a dataset with no
/// forks rather than a silent skip.
///
/// The window consumers ([`crate::chain_build`], [`crate::perma`]) still call [`window_slots`],
/// and `unanimous_fork_restores_only_toggles` measures why that is not a gap: every power this
/// pass moves on any fork is a toggle or an auto, and both consumers reject those before asking
/// for a window.
///
/// Value equality is compared over the surface [`WindowSlots`] carries — key presence, the
/// `durations` map, and the two duration fields — where the converter compares each slot's whole
/// value object. That is an over-approximation exactly where the router does not yet hold values:
/// two arms agreeing on presence and differing on a scale would be restored here and declined
/// there. It tightens on its own as the value work fills the slots in.
pub fn bag_slots(power: &Power, dataset: DatasetId, classes: &[&str]) -> WindowSlots {
    let mut base = slots_inner(power, dataset, bag_subset, false, false);
    let forked = power.atoms.iter().any(|a| a.caster_archetypes.is_some());
    if forked && !classes.is_empty() {
        let arms: Vec<WindowSlots> = classes
            .iter()
            .map(|class| {
                slots_inner(
                    power,
                    dataset,
                    |a| !a.is_gated() && !a.is_deactivation_burst() && a.applies_to_class(class),
                    false,
                    false,
                )
            })
            .collect();

        let base_keys = base.keys();
        let candidates: BTreeSet<&'static str> = arms.iter().flat_map(|arm| arm.keys()).collect();
        for key in candidates {
            if base_keys.contains(key) {
                continue;
            }
            let first = slot_value(&arms[0], key);
            if first.is_none() || !arms.iter().all(|arm| slot_value(arm, key) == first) {
                continue;
            }
            match key {
                "buffDuration" => base.buff_duration = arms[0].buff_duration,
                "effectDuration" => base.effect_duration = arms[0].effect_duration,
                _ => {
                    base.present.insert(key);
                    if let Some((value, _)) = &first {
                        if !value.is_null() {
                            base.values.insert(key.to_owned(), value.clone());
                        }
                    }
                }
            }
        }

        // `durations` is ONE key of the bag, so it is restored whole or not at all. A base that
        // recorded any duration already states the key, and the fork's map is not its to
        // overrule — which is why a slot restored above can arrive without its clock.
        if base.durations.is_empty() {
            let first = &arms[0].durations;
            if !first.is_empty() && arms.iter().all(|arm| arm.durations == *first) {
                base.durations = first.clone();
            }
        }
    }
    thunderspy_strips(&mut base, power, dataset);
    base
}

/// One arm's value for a bag key: `None` when the arm does not state the key at all, which is
/// the converter's `bag[key] ?? null` declining unanimity.
///
/// The comparison is the converter's own — the whole value object, plus the recorded duration,
/// which lives in the `durations` map rather than on the slot. A key the projection states
/// without a value (`summon`) compares as `Value::Null`, and null is exactly what the converter
/// declines to restore, so a forked summon is left to the arms rather than merged.
fn slot_value(slots: &WindowSlots, key: &str) -> Option<(Value, Option<f64>)> {
    match key {
        "buffDuration" => slots.buff_duration.map(|d| (num(d), None)),
        "effectDuration" => slots.effect_duration.map(|d| (num(d), None)),
        _ => slots.present.contains(key).then(|| {
            (
                slots.values.get(key).cloned().unwrap_or(Value::Null),
                slots.durations.get(key).copied(),
            )
        }),
    }
}

/// The same projection over an arbitrary subset of the power's atoms.
///
/// [`window_slots`] is this with the bag's own subset (`_bagTemplates`: ungated, unforked,
/// minus the `OnDeactivate` rows the routing loop skips). The conditional adjusters
/// ([`crate::adjusters`]) need the projection over ONE entry's atoms instead — a gated set,
/// selected by [`AtomicEffect::conditional_id`] — and the router below is indifferent to which
/// atoms it is handed. Splitting the filter out rather than copying the router is the whole
/// point: a second router is the drift this module's header exists to warn about.
///
/// The `finish` post-passes — the resource fold, the `damageBuff` headline vote, the
/// `buffDuration` plurality — run over the selected subset alone, which is what the converter
/// does for a conditional group too: `extractConditionalEffects` calls `extractEffects` on the
/// GROUP's templates, so each entry's bag is projected in its own scope and never against the
/// power's base.
///
/// The two Thunderspy target-trap guards do NOT run here, and that is the difference between a
/// subset and the power. They read `power.effects` at `convert-powerset.cjs:8940` — the finished
/// BASE bag — and never walk `conditionalEffects`, so an entry's bag keeps a key the base loses.
/// Stripping here took Pack Frenzy's hunter-mode `rechargeBuff` (and its clock, and the
/// `buffDuration` derived from it) off an entry the converter leaves whole.
pub fn slots_over(
    power: &Power,
    dataset: DatasetId,
    select: impl FnMut(&AtomicEffect) -> bool,
) -> WindowSlots {
    slots_inner(power, dataset, select, true, false)
}

/// The router, with the two Thunderspy target-trap guards optional. They belong to the finished
/// power rather than to a projection (`convert-powerset.cjs:8940` runs them after the
/// per-archetype merge), so [`bag_slots`] projects its arms with `strip: false` and guards once
/// at the end.
fn slots_inner(
    power: &Power,
    dataset: DatasetId,
    mut select: impl FnMut(&AtomicEffect) -> bool,
    per_foe_entry: bool,
    strip: bool,
) -> WindowSlots {
    let selected: Vec<&AtomicEffect> = power.atoms.iter().filter(|a| select(a)).collect();
    let mut s = State {
        absorb_fraction: absorb_max_hp_fraction(&selected),
        absorb_stack_count: absorb_stack_count(&selected),
        ..Default::default()
    };
    let twins = twin_roles(&selected);
    let targets = power.targets_affected();
    for (i, (a, twin)) in selected.iter().zip(twins).enumerate() {
        s.cursor = i;
        route(&mut s, a, dataset, twin, targets.as_deref());
    }
    let scope = if per_foe_entry {
        PerFoeScope::Entry(&selected)
    } else {
        PerFoeScope::Base
    };
    s.finish(power, dataset, scope, strip)
}

/// The converter's Expression MaxHP-fraction absorb pre-scan, over the same selected subset the
/// router is about to walk.
///
/// An absorb whose magnitude is an Expression at `aspect: Max` carries no scale×table the
/// converter can evaluate, so the routing branch emits a duration-only queue entry and the
/// magnitude arrives from here instead: `effects.absorb = { maxHPFraction }`. Mirroring only the
/// queue is what left the slot unmarked on 12 Homecoming and 9 Thunderspy powers — the clock was
/// recorded and the key never appeared, because a duration-only resource entry deliberately does
/// not mark presence.
///
/// One fraction or nothing, exactly as the converter: a power with two distinct evaluable
/// fractions, or one shape [`parse_absorb_max_hp_fraction`] declines, leaves every absorb here
/// duration-only. Gated conditional groups scan in their own scope through
/// [`slots_over`]'s subset, which is how Ablative's 0.3 and 0.09 each see one fraction.
fn absorb_max_hp_fraction(atoms: &[&AtomicEffect]) -> Option<(f64, bool)> {
    let mut fraction: Option<(f64, bool)> = None;
    for a in atoms {
        if a.effect_type != Some(EffectType::Absorb)
            || a.aspect != Some(Aspect::Max)
            || a.attrib_type != Some(AttribType::Expression)
        {
            continue;
        }
        let expr = expression_text(a.magnitude_expression.as_deref());
        if expr.trim().is_empty() {
            // Empty = the placeholder/PvP variant, which states no magnitude to recover.
            continue;
        }
        let parsed = parse_absorb_max_hp_fraction(
            &expr,
            a.scale,
            a.modifier_table.as_deref(),
            a.ignore_strength == Some(true),
        )?;
        match fraction {
            None => fraction = Some(parsed),
            // The converter collects the fractions in a Set and the strength flag as an OR, so
            // a repeat of the same fraction agrees and either row's `@Strength` reaches it.
            Some((prev, strength)) if r4(prev) == r4(parsed.0) => {
                fraction = Some((prev, strength || parsed.1));
            }
            Some(_) => return None,
        }
    }
    fraction
}

/// The converter's absorb-stack pre-scan (`convert-powerset.cjs:5970`): an absorb the power
/// applies N identical times states ONE stack's worth in the slot, so the epilogue divides.
///
/// The converter's `g.length === 1` is "the template carries one attrib", which a per-atom read
/// cannot ask; an Absorb template in the corpus carries the absorb attrib alone, so every
/// matching atom is taken as its own template here. Whether that approximation costs anything is
/// what the value gate measures — an over-count divides a slot no other reader divides.
fn absorb_stack_count(atoms: &[&AtomicEffect]) -> usize {
    let applies: Vec<&&AtomicEffect> = atoms
        .iter()
        .filter(|a| {
            a.effect_type == Some(EffectType::Absorb)
                && a.aspect == Some(Aspect::Cur)
                && matches!(a.attrib_type, None | Some(AttribType::Magnitude))
        })
        .collect();
    if applies.len() <= 1 {
        return 0;
    }
    let first = applies[0];
    let uniform = applies.iter().all(|a| {
        (a.scale.unwrap_or(0.0) - first.scale.unwrap_or(0.0)).abs() < 1e-6
            && a.modifier_table == first.modifier_table
            && a.to_who == first.to_who
    });
    if uniform {
        applies.len()
    } else {
        0
    }
}

/// Read a MaxHP-fraction absorb Expression as its fraction of the caster's Max HP, mirroring
/// `parseAbsorbMaxHPFraction` (`scripts/convert-powerset.cjs:6122`). Two shapes carry one:
///
///   * `Max.kHitPoints source> C * [@Strength *]` — the fraction is the literal `C`.
///   * `Max.kHitPoints source> @StdResult *` — the fraction is the template's own standard
///     result, which is a bare fraction only on a `_ones` table (elsewhere it multiplies through
///     a real table and this declines).
///
/// `None` for anything else, and the caller leaves that power's absorbs duration-only. Master
/// Brawler's expression reads live HP and endurance, which is the shape this exists to turn away.
///
/// The converter matches its two regexes against the joined expression, so this splits the same
/// joined text on whitespace: the regexes' only captures are a numeric literal and `@Strength`,
/// neither of which can contain a space, so a token walk and a `\s+` regex agree here. The
/// numeric test is the regex's own `[\d.]+` character class rather than a general float parse,
/// which is why a signed or exponent form declines instead of being read.
///
/// Returns the fraction and `appliesStrength` — whether +Absorb strength (Power Boost, Clarion,
/// slotted Heal) grows it. Strength reaches the value only where the template allows it at all:
/// `fStr` stays 1.0 when AllowStrength is off, which the export spells as `IgnoreStrength`.
fn parse_absorb_max_hp_fraction(
    expr: &str,
    scale: Option<f64>,
    table: Option<&str>,
    ignores_strength: bool,
) -> Option<(f64, bool)> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    let ["Max.kHitPoints", "source>", rest @ ..] = parts.as_slice() else {
        return None;
    };
    let strength_reaches = !ignores_strength;
    let literal_ok =
        |lit: &str| !lit.is_empty() && lit.chars().all(|c| c.is_ascii_digit() || c == '.');
    match rest {
        [literal, "*"] if literal_ok(literal) => literal.parse::<f64>().ok().map(|f| (f, false)),
        [literal, "*", "@Strength", "*"] if literal_ok(literal) => {
            literal.parse::<f64>().ok().map(|f| (f, strength_reaches))
        }
        ["@StdResult", "*"] => {
            let ones = table.is_some_and(|t| t.to_ascii_lowercase().ends_with("_ones"));
            scale
                .filter(|s| ones && *s > 0.0)
                .map(|s| (s, strength_reaches))
        }
        _ => None,
    }
}

/// A bag value object, in the shape `_sortKeysDeep` emits: `serde_json`'s map, because the
/// consumers this half exists for read a bag. `display_effects` and
/// [`crate::effective::with_active_conditionals`] merge and render `effects` as JSON today, so
/// the honest product of a value mirror is that same object built from atoms — a typed
/// intermediate would be decoded straight back into one at every call site. The typed surface
/// stays where a consumer wants a fact rather than a bag: [`WindowSlots::durations`] and the
/// two duration fields answer the window consumers.
pub type Obj = Map<String, Value>;

fn num(n: f64) -> Value {
    serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
}

/// One queued resource atom, in `addOrAccumulate`'s entry shape
/// (`scripts/convert-powerset.cjs:6538`). `duration_only` entries are the deferred Expression
/// absorbs, which carry a clock and no value.
#[derive(Clone, Default)]
struct ResourceEntry {
    duration_only: bool,
    scale: f64,
    table: Option<String>,
    is_debuff: bool,
    twin: bool,
    ignore_strength: bool,
    /// The template's `Replace` stack mode, carried by the converter on the two maxHP slots
    /// alone so every other queue folds always-sum.
    replace: bool,
    /// `!reachesCaster` — set only on the four regen/recovery buff slots, exactly as the
    /// converter sets it.
    not_on_caster: bool,
    duration: Option<f64>,
}

/// `splitResourceRecipients` (`convert-powerset.cjs:6011`): where any row reaches the caster,
/// the rows that do not are dropped, because those four slots claim what the power adds to the
/// CASTER's own regen/recovery. Where NO row reaches him the queue is left alone — a wholly
/// ally-facing slot is display data, and deleting it would lose Speed Boost's +Recovery from
/// the power card (ATOM-BAG-5).
fn split_resource_recipients(entries: &[ResourceEntry]) -> Vec<ResourceEntry> {
    if !entries.iter().any(|e| !e.not_on_caster && !e.duration_only) {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter(|e| !e.not_on_caster)
        .cloned()
        .collect()
}

/// `foldResourceSlot` (`convert-powerset.cjs:6026`): the slot value as a function of its queue.
///
/// Same table accumulates, a table change resets, a duration-distinct DEBUFF splits into a
/// primary and `durationVariants`, and `ignoreStrength` survives only while every contributing
/// entry carries it (ENT-4 — a folded scale standing for several templates has no honest answer
/// for a mixed slot, so the mark is dropped rather than guessed).
fn fold_resource_slot(entries: &[ResourceEntry]) -> (Option<Obj>, Option<f64>) {
    let mut cur: Option<Obj> = None;
    let mut cur_table: Option<String> = None;
    let mut cur_dur: Option<f64> = None;
    let mut cur_ignores = false;
    for e in entries {
        if e.duration_only {
            if let Some(d) = e.duration {
                cur_dur = Some(d);
            }
            continue;
        }
        let same_table = cur.is_some() && cur_table.as_deref() == e.table.as_deref();
        if same_table && cur_ignores != e.ignore_strength {
            cur_ignores = false;
            cur.as_mut()
                .expect("same_table implies a current entry")
                .remove("ignoreStrength");
        }
        if same_table {
            let obj = cur.as_mut().expect("same_table implies a current entry");
            let split = e.is_debuff
                && match (cur_dur, e.duration) {
                    (Some(p), Some(d)) => (p - d).abs() > 0.001,
                    _ => false,
                };
            if split {
                let prev_scale = obj.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
                let prev_dur = cur_dur.unwrap_or(0.0);
                let new_dur = e.duration.unwrap_or(0.0);
                let variant = if new_dur > prev_dur {
                    variant_obj(prev_scale, prev_dur)
                } else {
                    variant_obj(e.scale.abs(), new_dur)
                };
                push_variant(obj, variant);
                if new_dur > prev_dur {
                    obj.insert("scale".into(), num(e.scale.abs()));
                    cur_dur = Some(new_dur);
                }
                continue;
            }
            let prev = obj.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
            let next = if e.replace {
                prev.max(e.scale.abs())
            } else {
                prev + e.scale.abs()
            };
            obj.insert("scale".into(), num(next));
        } else {
            let mut o = Obj::new();
            o.insert("scale".into(), num(e.scale.abs()));
            if let Some(t) = &e.table {
                o.insert("table".into(), Value::String(t.clone()));
            }
            if e.twin {
                o.insert("unresistable".into(), Value::Bool(true));
            }
            cur_ignores = e.ignore_strength;
            if cur_ignores {
                o.insert("ignoreStrength".into(), Value::Bool(true));
            }
            cur = Some(o);
            cur_table = e.table.clone();
        }
        if let Some(d) = e.duration {
            cur_dur = Some(d);
        }
    }
    (cur, cur_dur)
}

fn variant_obj(scale: f64, duration: f64) -> Value {
    let mut o = Obj::new();
    o.insert("scale".into(), num(scale));
    o.insert("duration".into(), num(duration));
    Value::Object(o)
}

/// Was this atom's template pulled out of another power's file? `owner_targets` is stamped by
/// the collectors exactly there (`_stampOwnerScalars`; the encoder emits only a non-empty
/// list), so its presence IS the collection provenance — the fact the CONTROL branch's
/// own-beats-redirect preference keys on (TAUNT-1).
fn redirect_collected(a: &AtomicEffect) -> bool {
    a.owner_targets.as_deref().is_some_and(|t| !t.is_empty())
}

fn push_variant(obj: &mut Obj, variant: Value) {
    match obj
        .entry("durationVariants")
        .or_insert_with(|| Value::Array(Vec::new()))
    {
        Value::Array(list) => list.push(variant),
        _ => unreachable!("durationVariants is only ever written as an array"),
    }
}

#[derive(Default)]
struct State {
    present: BTreeSet<&'static str>,
    /// The projected bag: every slot key's value object. `present` is its key set plus the
    /// keys that exist without one — `summon` alone, which `extractSummon` owns outside this
    /// projection (ENT-14).
    bag: Obj,
    durations: BTreeMap<&'static str, f64>,
    self_mark: BTreeMap<&'static str, bool>,
    slow_axis_self: BTreeMap<&'static str, bool>,
    /// Last routed `damageBuff` |scale|, for the headline post-pass.
    damage_scale: Option<f64>,
    /// Every `damageBuff` instance: (|scale| rounded to 4 places, duration-or-0, damage type).
    damage_instances: Vec<(f64, f64, Option<SubType>)>,
    resources: BTreeMap<&'static str, Vec<ResourceEntry>>,
    effect_duration: Option<f64>,
    /// The subset's one recoverable Expression absorb fraction, from [`absorb_max_hp_fraction`].
    absorb_fraction: Option<(f64, bool)>,
    /// The absorb-stack pre-scan's count, for the epilogue's divide.
    absorb_stack_count: usize,
    /// Control slots an OWN row has written (TAUNT-1) — a redirect-collected control row
    /// must not displace one. See [`Self::slot_ctrl`].
    ctrl_own: BTreeSet<&'static str>,
    /// Which atom the router is currently on, so a write can be attributed back to it.
    cursor: usize,
    /// Every `(atom index, slot key, sub-key)` the router wrote, in write order. The per-foe
    /// post-pass ([`State::per_foe_patches`]) needs the slot a `per_target`-stamped atom
    /// reached, and asking the router is the only way to get it that does not re-derive the
    /// routing — `classifyTemplateForStacking` is a second, coarser classifier, and a copy of
    /// it here would be exactly the drift this module's header warns about.
    writes: Vec<(usize, &'static str, Option<Box<str>>)>,
}

fn r4(n: f64) -> f64 {
    (n * 1e4).round() / 1e4
}

impl State {
    /// Attribute a slot write to the atom the router is currently on. Cheap and unconditional:
    /// a slot written by no stamped atom is simply never asked about.
    fn mark(&mut self, key: &'static str, sub: Option<&str>) {
        self.writes.push((self.cursor, key, sub.map(Box::from)));
    }

    fn record(&mut self, key: &'static str, dur: Option<f64>) {
        if let Some(d) = dur.filter(|d| *d > 0.0) {
            self.durations.insert(key, d);
        }
    }

    /// A whole-slot write: `effects[key] = value`, the converter's plain assignment.
    fn slot(&mut self, key: &'static str, dur: Option<f64>, value: Value) {
        self.mark(key, None);
        self.present.insert(key);
        self.bag.insert(key.to_owned(), value);
        self.record(key, dur);
    }

    /// The CONTROL-slot write (TAUNT-1): the power's own row beats a redirect-collected one
    /// (`owner_targets` presence is the collection provenance), and among rows of one
    /// provenance last write wins. A skipped redirect row records nothing — the converter's
    /// branch skips its `recordDuration` too.
    fn slot_ctrl(
        &mut self,
        key: &'static str,
        dur: Option<f64>,
        value: Value,
        from_redirect: bool,
    ) {
        if from_redirect && self.ctrl_own.contains(key) {
            return;
        }
        self.slot(key, dur, value);
        if !from_redirect {
            self.ctrl_own.insert(key);
        }
    }

    /// A by-type write: `effects[key] ||= {}; effects[key][sub] = value`. A slot the converter
    /// last wrote as a scalar (`base_defense`'s branch) is replaced whole, which is what the
    /// `typeof !== 'object'` reset does there.
    /// The converter's `writeMovementBuff` (FLYPOOL-1): the enhanceable row keeps the
    /// movement axis, a PAIRED IgnoreStrength row moves to `<axis>Unenhanced`, a lone
    /// IgnoreStrength row keeps the plain key, and same-kind collisions keep last-write.
    fn movement_buff_slot(&mut self, axis: &str, dur: Option<f64>, value: Value, is: bool) {
        let cur_is = self.bag.get("movement").and_then(|m| m.get(axis)).map(|e| {
            e.get("ignoreStrength")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        let twin = format!("{axis}Unenhanced");
        match cur_is {
            Some(false) if is => self.slot_sub("movement", &twin, dur, value),
            Some(true) if !is => {
                let cur = self
                    .bag
                    .get("movement")
                    .and_then(|m| m.get(axis))
                    .cloned()
                    .expect("cur_is proved the entry exists");
                self.slot_sub("movement", &twin, dur, cur);
                self.slot_sub("movement", axis, dur, value);
            }
            _ => self.slot_sub("movement", axis, dur, value),
        }
    }

    fn slot_sub(&mut self, key: &'static str, sub: &str, dur: Option<f64>, value: Value) {
        self.mark(key, Some(sub));
        self.present.insert(key);
        let entry = self
            .bag
            .entry(key.to_owned())
            .or_insert_with(|| Value::Object(Obj::new()));
        if !entry.is_object() {
            *entry = Value::Object(Obj::new());
        }
        if let Value::Object(map) = entry {
            map.insert(sub.to_owned(), value);
        }
        self.record(key, dur);
    }

    /// The same-table accumulate the mez / knockback / control-resistance branches do:
    /// a second row on the table already recorded adds its |scale| instead of replacing it.
    fn accumulate_sub(&mut self, key: &'static str, sub: &str, dur: Option<f64>, value: Value) {
        let table = value
            .get("table")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let prev_scale = self
            .bag
            .get(key)
            .and_then(|v| v.get(sub))
            .filter(|v| v.get("table").and_then(Value::as_str) == table.as_deref())
            .and_then(|v| v.get("scale"))
            .and_then(Value::as_f64);
        match prev_scale {
            Some(prev) => {
                let add = value.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(Value::Object(map)) = self.bag.get_mut(key) {
                    if let Some(Value::Object(entry)) = map.get_mut(sub) {
                        entry.insert("scale".into(), num(prev + add));
                    }
                }
                self.mark(key, Some(sub));
                self.present.insert(key);
                self.record(key, dur);
            }
            None => self.slot_sub(key, sub, dur, value),
        }
    }

    /// The knockback family's whole-slot accumulate: same shape as [`accumulate_sub`] one level
    /// up, because `effects[kbType]` is a scalar slot rather than a by-type map.
    fn accumulate_slot(&mut self, key: &'static str, dur: Option<f64>, value: Value) {
        let table = value
            .get("table")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let prev_scale = self
            .bag
            .get(key)
            .filter(|v| v.get("table").and_then(Value::as_str) == table.as_deref())
            .and_then(|v| v.get("scale"))
            .and_then(Value::as_f64);
        match prev_scale {
            Some(prev) => {
                let add = value.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(Value::Object(entry)) = self.bag.get_mut(key) {
                    entry.insert("scale".into(), num(prev + add));
                }
                self.mark(key, None);
                self.present.insert(key);
                self.record(key, dur);
            }
            None => self.slot(key, dur, value),
        }
    }

    /// A penalty slot write: last-write-wins on the `toWho: Self` mark, like the converter's
    /// plain re-assignment of the slot object.
    fn penalty_slot(&mut self, key: &'static str, is_self: bool, dur: Option<f64>, value: Value) {
        self.slot(key, dur, value);
        self.self_mark.insert(key, is_self);
    }

    fn queue_resource(&mut self, key: &'static str, entry: ResourceEntry) {
        self.mark(key, None);
        self.resources.entry(key).or_default().push(entry);
    }

    /// `accumulateBuffSlot` for `tohitBuff`: first write or a table change resets and records;
    /// a duration-distinct instance mints a variant (the burst/tail pair); otherwise the new
    /// scale wins and the enhanceability mark follows it rather than the value it replaced.
    fn accumulate_tohit(&mut self, value: Value, dur: Option<f64>) {
        let table = value
            .get("table")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let reset = match self.bag.get("tohitBuff") {
            Some(Value::Object(prev)) => {
                prev.get("table").and_then(Value::as_str) != table.as_deref()
            }
            _ => true,
        };
        if reset {
            self.slot("tohitBuff", dur, value);
            return;
        }
        let prev_dur = self.durations.get("tohitBuff").copied();
        let scale = value.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
        let unresistable = value.get("unresistable").is_some();
        let ignores = value.get("ignoreStrength").is_some();
        let split = match (prev_dur, dur) {
            (Some(p), Some(d)) => (p - d).abs() > 0.001,
            _ => false,
        };
        if split {
            let p = prev_dur.unwrap_or(0.0);
            let d = dur.unwrap_or(0.0);
            let prev_scale = self
                .bag
                .get("tohitBuff")
                .and_then(|v| v.get("scale"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let variant = if d > p {
                variant_obj(prev_scale, p)
            } else {
                variant_obj(scale, d)
            };
            if let Some(Value::Object(prev)) = self.bag.get_mut("tohitBuff") {
                push_variant(prev, variant);
                if d > p {
                    prev.insert("scale".into(), num(scale));
                }
            }
            if d > p {
                self.durations.insert("tohitBuff", d);
            }
            self.mark("tohitBuff", None);
            self.present.insert("tohitBuff");
            return;
        }
        if let Some(Value::Object(prev)) = self.bag.get_mut("tohitBuff") {
            prev.insert("scale".into(), num(scale));
            if unresistable {
                prev.insert("unresistable".into(), Value::Bool(true));
            }
            if ignores {
                prev.insert("ignoreStrength".into(), Value::Bool(true));
            } else {
                prev.remove("ignoreStrength");
            }
        }
        self.mark("tohitBuff", None);
        self.record("tohitBuff", dur);
        self.present.insert("tohitBuff");
    }

    fn finish(
        mut self,
        power: &Power,
        dataset: DatasetId,
        scope: PerFoeScope<'_>,
        strip: bool,
    ) -> WindowSlots {
        // Resource fold: the slot value is a function of its queue, computed after the loop.
        let resources = std::mem::take(&mut self.resources);
        for (key, entries) in resources {
            let (effect, duration) = fold_resource_slot(&split_resource_recipients(&entries));
            if let Some(obj) = effect {
                self.present.insert(key);
                self.bag.insert(key.to_owned(), Value::Object(obj));
            }
            if let Some(d) = duration {
                self.durations.insert(key, d);
            }
        }

        self.damage_buff_headline();
        self.damage_buff_variants();
        self.absorb_stack_epilogue();
        self.per_foe_patches(power, dataset, scope);

        // buffDuration: plurality over the recorded durations, ties to the larger value.
        let buff_duration = derive_buff_duration(&self.durations);

        let mut slots = WindowSlots {
            present: self.present,
            values: self.bag,
            durations: self.durations,
            self_marked: self
                .self_mark
                .iter()
                .filter(|(_, is_self)| **is_self)
                .map(|(k, _)| *k)
                .collect(),
            slow_self: self.slow_axis_self.values().any(|v| *v),
            buff_duration,
            effect_duration: self.effect_duration,
        };
        if strip {
            thunderspy_strips(&mut slots, power, dataset);
        }
        slots
    }

    /// The `damageBuff` headline: the slot takes the value the most damage types share (ties to
    /// the longer-lived group), and when that differs from the last-written scale the recorded
    /// duration follows the winning group.
    fn damage_buff_headline(&mut self) {
        if self.damage_instances.is_empty() {
            return;
        }
        let Some(last) = self.damage_scale else {
            return;
        };
        if self
            .bag
            .get("damageBuff")
            .and_then(|v| v.get("scale"))
            .is_none()
        {
            return;
        }
        let Some((best_scale, _, best_dur)) = self.best_damage_group() else {
            return;
        };
        if r4(last) == best_scale {
            return;
        }
        if let Some(Value::Object(slot)) = self.bag.get_mut("damageBuff") {
            slot.insert("scale".into(), num(best_scale));
        }
        if self.durations.contains_key("damageBuff") {
            self.durations.insert("damageBuff", best_dur);
        }
    }

    /// The winning (scale, type count, duration) group of the `damageBuff` instances.
    fn best_damage_group(&self) -> Option<(f64, usize, f64)> {
        let mut by_scale: Vec<(f64, Vec<Option<SubType>>, f64)> = Vec::new();
        for (scale, duration, ty) in &self.damage_instances {
            match by_scale.iter_mut().find(|(k, _, _)| k == scale) {
                Some((_, types, dur)) => {
                    if !types.contains(ty) {
                        types.push(*ty);
                    }
                    *dur = dur.max(*duration);
                }
                None => by_scale.push((*scale, vec![*ty], *duration)),
            }
        }
        let mut best: Option<(f64, usize, f64)> = None;
        for (scale, types, dur) in &by_scale {
            let take = match best {
                None => true,
                Some((_, bn, bd)) => types.len() > bn || (types.len() == bn && *dur > bd),
            };
            if take {
                best = Some((*scale, types.len(), *dur));
            }
        }
        best
    }

    /// The `damageBuff` burst/tail `durationVariants`, display-only: a (scale, duration) group
    /// covering the SAME number of damage types as the primary survives as a variant, which
    /// admits Inner Light's all-types burst and skips Embrace of Fire's single-type buff.
    fn damage_buff_variants(&mut self) {
        if self.damage_instances.is_empty() {
            return;
        }
        let Some(prim_scale) = self
            .bag
            .get("damageBuff")
            .and_then(|v| v.get("scale"))
            .and_then(Value::as_f64)
            .map(r4)
        else {
            return;
        };
        let prim_dur = self.durations.get("damageBuff").copied();
        let mut groups: Vec<(f64, f64, Vec<Option<SubType>>)> = Vec::new();
        for (scale, duration, ty) in &self.damage_instances {
            let key = (r4(*scale), *duration);
            match groups.iter_mut().find(|(s, d, _)| (*s, *d) == key) {
                Some((_, _, types)) => {
                    if !types.contains(ty) {
                        types.push(*ty);
                    }
                }
                None => groups.push((key.0, key.1, vec![*ty])),
            }
        }
        let Some(prim_count) = groups
            .iter()
            .find(|(s, d, _)| *s == prim_scale && Some(*d) == prim_dur)
            .map(|(_, _, types)| types.len())
        else {
            return;
        };
        let mut variants: Vec<(f64, f64)> = groups
            .iter()
            .filter(|(_, d, types)| Some(*d) != prim_dur && *d > 0.0 && types.len() == prim_count)
            .map(|(s, d, _)| (*s, *d))
            .collect();
        if variants.is_empty() {
            return;
        }
        variants.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal))
        });
        let list: Vec<Value> = variants.iter().map(|(s, d)| variant_obj(*s, *d)).collect();
        if let Some(Value::Object(slot)) = self.bag.get_mut("damageBuff") {
            slot.insert("durationVariants".into(), Value::Array(list));
        }
    }

    /// The per-foe post-pass (PERFOE-1): `computeAoePerTargetPatches` + `mergeStackingPatches`,
    /// which run in the converter AFTER `extractEffects` has returned and REBUILD the slots they
    /// touch as `{scale, table, perTarget}` — the pass's own scale wins and every mark the
    /// projection wrote (`ignoreStrength`, `unresistable`, `durationVariants`) is gone, because
    /// the new object never had them.
    ///
    /// The mirror does not recompute the AoE geometry the converter reads, and does not need to:
    /// the converter stamps `_perTargetIncrement` only on the DOMINANT-table increments it
    /// selected, and that stamp rides the wire as [`AtomicEffect::per_target`]. So the stamp
    /// already carries the dominant-table verdict, the stack-flavour filter and the Defiance
    /// exclusion, and Σ of the distinct stamps IS `sumDistinctScale(domStacks)`. What the
    /// geometry is still asked for is which BRANCH produced a stamp, below — not what it is
    /// worth.
    ///
    /// Which slot a stamped atom belongs to comes from the router's own write ledger
    /// ([`State::writes`]) rather than from a port of `classifyTemplateForStacking`. That
    /// classifier is a second, coarser mapping over the same templates, and a copy of it here
    /// would be a second router — the drift this module's header exists to warn about.
    fn per_foe_patches(&mut self, power: &Power, dataset: DatasetId, scope: PerFoeScope<'_>) {
        let selected: Vec<&AtomicEffect> = match scope {
            PerFoeScope::Base => power.atoms.iter().filter(|a| per_foe_subset(a)).collect(),
            PerFoeScope::Entry(atoms) => atoms.to_vec(),
        };
        let entry_scope = matches!(scope, PerFoeScope::Entry(_));
        // The stacked-increment branch and the Execute_Power redirect branch reach the wire as
        // the same `per_target` stamp, and they merge differently: the AoE branch patches a
        // computed scale, the redirect branch patches whatever its own BASE arm summed — 0 when
        // that arm found nothing, and then the projection's value stands. Only the power's own
        // geometry separates them — a stamp on a power that is not an AoE with a target count
        // cannot have come from `computeAoePerTargetPatches` at all.
        let aoe = is_aoe_with_targets(power);
        // The scale/table half of the pass reads stamps, so no stamp means nothing to merge —
        // but the MaxHP-fraction absorb increment is the half that carries no stamp at all (its
        // magnitude rides an Expression, so the converter recovers the fraction rather than a
        // scale), and it must still run. Parasitic Aura's and Parasitic Leech's Defensive-stance
        // entries are exactly that: an `absorb.maxHPFractionPerTarget` and not one `per_target`
        // atom beside it.
        if !selected
            .iter()
            .any(|a| a.per_target.is_some() || a.redirect_base.is_some())
        {
            self.absorb_fraction_per_target(&selected, aoe);
            return;
        }

        // Every atom that reached each slot. The ledger is the router's own, taken over the
        // per-foe subset rather than the bag's — which is why it is a second routing pass and
        // not `self.writes`.
        let mut groups: BTreeMap<(&'static str, Option<Box<str>>), Vec<usize>> = BTreeMap::new();
        for (idx, key, sub) in write_ledger(power, dataset, &selected) {
            groups.entry((key, sub)).or_default().push(idx);
        }

        for ((key, sub), members) in groups {
            // `mergeStackingPatches` declines both of these outright: the strength containers
            // are keyed maps that a flat patch would clobber, and so is any slot the projection
            // left as a by-type map when the patch arrived without a sub-key.
            if key == "specialBuff" || key == "specialDebuff" {
                continue;
            }
            // A patch the converter's classifier writes FLAT lands on a slot the router split
            // by type, and `mergeStackingPatches` declines it there (`!('scale' in existing)`).
            // `debuffResistance` is the live case: one template carries -Regen, -Recovery,
            // -Endurance and -Recharge resistance at once, the classifier folds all four onto
            // the bare container key, and the container has no scale for a flat patch to reach.
            if sub.is_some() && !PATCH_SUB_KEYED.contains(&key) {
                continue;
            }
            // `_remapUnenhancedPatchKeys`: the classifier keys every patch on the BASE slot,
            // and the converter moves it to the `*Unenhanced` twin only where the projection
            // stated that twin and not the base. The router keys on the atom's own
            // `ignoreStrength` instead, so normalise to the classifier's key first and then
            // apply the converter's move — otherwise an unenhanceable increment whose slot the
            // resource fold merged back into the base patches a key nobody wrote.
            let base = key.strip_suffix("Unenhanced").unwrap_or(key);
            let key = if self.bag.contains_key(base) {
                base
            } else {
                self.present
                    .iter()
                    .copied()
                    .find(|k| k.strip_suffix("Unenhanced") == Some(base))
                    .unwrap_or(base)
            };
            let stamped: Vec<&AtomicEffect> = members
                .iter()
                .filter_map(|i| selected.get(*i).copied())
                .filter(|a| a.per_target.is_some())
                .collect();
            // The redirect branch's BASE arm (PERFOE-2), stamped onto this power's atoms by the
            // same signature replay that carries the per-foe arm. A slot can hold one arm, the
            // other, or both — Siphon Power is base-only, Fulcrum Shift is both — so this is
            // gathered beside `stamped` rather than inside it.
            let redirect_base: Vec<&AtomicEffect> = members
                .iter()
                .filter_map(|i| selected.get(*i).copied())
                .filter(|a| a.redirect_base.is_some())
                .collect();
            if stamped.is_empty() && redirect_base.is_empty() {
                continue;
            }
            // A helper's atoms resolve under the helper's class (`AtomicEffect::pet_class`), so a
            // slot whose increments come from two classes is two values, not one. Thunderspy's
            // Fulcrum Shift is the case: its +5 base runs as the player, its 1.6 per foe as
            // `minion_pets`.
            let mut classes: Vec<Option<&str>> = Vec::new();
            for a in stamped.iter().chain(redirect_base.iter()) {
                if !classes.contains(&a.pet_class.as_deref()) {
                    classes.push(a.pet_class.as_deref());
                }
            }
            if classes.len() > 1 {
                let terms = classes
                    .iter()
                    .map(|class| {
                        let of_class = |a: &&&AtomicEffect| a.pet_class.as_deref() == *class;
                        let stamped: Vec<&AtomicEffect> =
                            stamped.iter().filter(of_class).copied().collect();
                        let redirect_base: Vec<&AtomicEffect> =
                            redirect_base.iter().filter(of_class).copied().collect();
                        let (scale, table, per_target) =
                            redirect_patch(power, &stamped, &redirect_base);
                        let mut term = rebuilt(scale, table, per_target);
                        if let (Some(class), Value::Object(obj)) = (class, &mut term) {
                            obj.insert(PET_CLASS_KEY.into(), Value::String((*class).to_owned()));
                        }
                        term
                    })
                    .collect();
                let mut value = Obj::new();
                value.insert(SCALE_TERMS_KEY.into(), Value::Array(terms));
                match sub.as_deref() {
                    Some(sub) => self.slot_sub_raw(key, sub, Value::Object(value)),
                    None => {
                        self.present.insert(key);
                        self.bag.insert(key.to_owned(), Value::Object(value));
                    }
                }
                continue;
            }
            let table = stamped
                .iter()
                .chain(redirect_base.iter())
                .find_map(|a| a.modifier_table.as_deref())
                .map(str::to_owned);
            let per_target = sum_distinct(stamped.iter().map(|a| {
                (
                    a.per_target.unwrap_or(0.0).abs(),
                    a.modifier_table.as_deref(),
                )
            }));

            // `scale` is the value at ONE target hit. The Replace rows are the always-on base;
            // the increment joins them there unless the caster occupies the N=1 slot and the
            // increment's own gate excludes the caster from it (Phalanx Fighting).
            // `entry.scale` on the redirect branch is what its base arm summed, and nothing
            // else: the entry starts at `{scale: 0, perTarget: 0}` and never picks up the
            // projection's own value. Distinct stamps, like every other sum here — a by-type
            // base is ONE contribution spread over N attribs.
            let redirect_scale = sum_distinct(redirect_base.iter().map(|a| {
                (
                    a.redirect_base.unwrap_or(0.0).abs(),
                    a.modifier_table.as_deref(),
                )
            }));
            let scale = if !redirect_base.is_empty() {
                // The value at ONE foe also carries that foe's own increment when the
                // increment's recipients include the caster — Fulcrum Shift's per-foe
                // `KineticTransferBuff` is a `["Friend", "Self"]` sphere, so one foe hit is
                // 4 + 2, not 4. `mergeStackingPatches` stopped at the base and left the card
                // one increment short of the totals (TARGETS-3); the atom applier
                // (`per_target_from_group`) adds it the same way.
                redirect_scale
                    + sum_distinct(
                        stamped
                            .iter()
                            .filter(|a| coh_data::atom::reaches_caster(a, power))
                            .map(|a| {
                                (
                                    a.per_target.unwrap_or(0.0).abs(),
                                    a.modifier_table.as_deref(),
                                )
                            }),
                    )
            } else if aoe && stamped.iter().all(|a| is_increment(a)) {
                let replace = sum_distinct(
                    members
                        .iter()
                        .filter_map(|i| selected.get(*i).copied())
                        .filter(|a| {
                            a.to_who == Some(ToWho::Self_)
                                && a.stacking == Some(Stacking::Replace)
                                && (entry_scope || !is_defiance(a))
                                && a.modifier_table.as_deref() == table.as_deref()
                        })
                        .map(|a| (a.scale.unwrap_or(0.0).abs(), a.modifier_table.as_deref())),
                );
                let first_target_excluded = !entry_scope
                    && power.affects_caster()
                    && stamped.iter().all(|a| excludes_self(a));
                if first_target_excluded {
                    replace
                } else {
                    replace + per_target
                }
            } else {
                0.0
            };

            self.apply_patch(key, sub.as_deref(), scale, table.as_deref(), per_target);
            if let Some(Some(class)) = classes.first() {
                let slot = match sub.as_deref() {
                    Some(sub) => self.bag.get_mut(key).and_then(|v| v.get_mut(sub)),
                    None => self.bag.get_mut(key),
                };
                if let Some(Value::Object(obj)) = slot {
                    obj.insert(PET_CLASS_KEY.into(), Value::String((*class).to_owned()));
                }
            }
        }

        self.absorb_fraction_per_target(&selected, aoe);
    }

    /// One patch's merge, `mergeStackingPatches`'s own branch order.
    ///
    /// A patch does not annotate its slot, it REPLACES it — which is the whole of PERFOE-1. The
    /// one exception is the redirect branch's `scale: 0` against a slot that already states a
    /// non-zero scale: there the projection's object survives and only gains `perTarget`.
    fn apply_patch(
        &mut self,
        key: &'static str,
        sub: Option<&str>,
        scale: f64,
        table: Option<&str>,
        per_target: f64,
    ) {
        let existing = match sub {
            Some(sub) => self.bag.get(key).and_then(|v| v.get(sub)).cloned(),
            None => self.bag.get(key).cloned(),
        };
        // A flat patch onto a by-type map is the `!('scale' in existing)` decline.
        if sub.is_none() {
            if let Some(Value::Object(map)) = &existing {
                if !map.contains_key("scale") {
                    return;
                }
            }
        }
        let existing_scale = existing.as_ref().and_then(|v| {
            v.get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())
        });

        let patched = match &existing {
            // `patchValue.scale === 0 && existing.scale` — the object stands, marks and all.
            Some(Value::Object(prev))
                if scale == 0.0 && existing_scale.is_some_and(|s| s != 0.0) =>
            {
                let mut obj = prev.clone();
                obj.insert("perTarget".into(), num(per_target));
                Value::Object(obj)
            }
            Some(Value::Object(prev)) => {
                let table = table
                    .map(str::to_owned)
                    .or_else(|| prev.get("table").and_then(Value::as_str).map(str::to_owned));
                rebuilt(scale, table.as_deref(), per_target)
            }
            _ if scale > 0.0 => rebuilt(scale, table, per_target),
            // A scalar slot (or none at all) wrapped by an increment that has somewhere to go.
            Some(_) if per_target != 0.0 => {
                rebuilt(existing_scale.unwrap_or(0.0), table, per_target)
            }
            _ => return,
        };

        match sub {
            Some(sub) => self.slot_sub_raw(key, sub, patched),
            None => {
                self.present.insert(key);
                self.bag.insert(key.to_owned(), patched);
            }
        }
    }

    /// `effects[key][sub] = value` without the presence/duration bookkeeping a routed write does
    /// — a patch reaches a slot the router already stated.
    fn slot_sub_raw(&mut self, key: &'static str, sub: &str, value: Value) {
        self.present.insert(key);
        let entry = self
            .bag
            .entry(key.to_owned())
            .or_insert_with(|| Value::Object(Obj::new()));
        if !entry.is_object() {
            *entry = Value::Object(Obj::new());
        }
        if let Value::Object(map) = entry {
            map.insert(sub.to_owned(), value);
        }
    }

    /// The MaxHP-fraction absorb's per-foe companion (PROD6C-3j): the increment has no scale to
    /// grow, because the magnitude rides an Expression whose Scale is the `@StdResult`
    /// placeholder. Each foe hit re-applies that same Expression, so the increment IS the
    /// recovered fraction, and it lands beside `maxHPFraction` rather than as a `perTarget` on a
    /// scale that means nothing. Only where the pre-scan actually recovered a fraction: a
    /// duration-only absorb has no magnitude for an increment to grow.
    fn absorb_fraction_per_target(&mut self, selected: &[&AtomicEffect], aoe: bool) {
        if !aoe {
            return;
        }
        if self
            .bag
            .get("absorb")
            .is_none_or(|v| v.get("maxHPFraction").is_none())
        {
            return;
        }
        let Some(fraction) = selected
            .iter()
            .filter(|a| {
                a.to_who == Some(ToWho::Self_)
                    && is_increment(a)
                    && a.effect_type == Some(EffectType::Absorb)
                    && a.aspect == Some(Aspect::Max)
                    && a.attrib_type == Some(AttribType::Expression)
            })
            .find_map(|a| {
                parse_absorb_max_hp_fraction(
                    &expression_text(a.magnitude_expression.as_deref()),
                    a.scale,
                    a.modifier_table.as_deref(),
                    a.ignore_strength == Some(true),
                )
                .map(|(f, _)| f)
            })
        else {
            return;
        };
        if let Some(Value::Object(slot)) = self.bag.get_mut("absorb") {
            slot.insert("maxHPFractionPerTarget".into(), num(fraction));
        }
    }

    /// The absorb-stack epilogue: an absorb the power applies N identical times states one
    /// stack's worth, not the whole pile. The stacking metadata it also writes
    /// (`maxStacks` / `stacksLinear` / `stackInterval`) is a def field sharing the map and is
    /// not projected here — the value it changes is `absorb.scale`.
    fn absorb_stack_epilogue(&mut self) {
        if self.absorb_stack_count <= 1 {
            return;
        }
        let count = self.absorb_stack_count as f64;
        if let Some(Value::Object(slot)) = self.bag.get_mut("absorb") {
            if let Some(scale) = slot.get("scale").and_then(Value::as_f64) {
                slot.insert("scale".into(), num(scale / count));
            }
        }
    }
}

/// Whose templates the per-foe pass reads.
///
/// The converter runs `computeAoePerTargetPatches` twice with different inputs. For the base bag
/// it reads the power's own templates minus the conditional-surfaced ones; for a conditional
/// entry `extractConditionalEffects` hands it the GROUP's templates, and hands them over with no
/// tags and no gate text at all (`convert-powerset.cjs:4215`) — so on that route neither the
/// Defiance filter nor the caster-excluded term can fire, whatever the atoms carry.
#[derive(Clone, Copy)]
enum PerFoeScope<'a> {
    Base,
    Entry(&'a [&'a AtomicEffect]),
}

/// The atoms `computeAoePerTargetPatches` reads: the power's own templates minus the ones
/// `collectConditionalsGrouped` re-homes into a `conditionalEffects` entry, where their per-foe
/// scaling is recomputed in the entry's own scope.
///
/// This is NOT the bag's subset, and the difference is the whole reason the pass needs its own
/// ledger. `_bagTemplates` drops every gated atom; the per-foe pass drops only the gates the
/// conditional extractor SURFACES, so an untoggleable gate — an on-hit roll, a PvE/PvP split, a
/// `target ≠ source` clause — stays in base for it. Phalanx Fighting's per-ally increment is
/// exactly that atom, and reading the bag's subset here leaves it stamped, unrouted and unasked.
/// The surfaced set is on the wire as [`AtomicEffect::conditional_id`], stamped by the same
/// classifier, so this asks the stamp rather than re-classifying the gate.
/// The rest of the filter is `collectTemplatesWithMeta`'s own, which the pass inherits by
/// walking its output: a PvP-only group, a chance-0 group, a Containment-tagged one, and the
/// three gate texts it names (dead state, a charged meter, a die roll). Dropping only the
/// conditional stamp leaves the PvP twin of a defense buff in the base sum, where its Replace
/// row lands on the same slot as the PvE one and states the increment against a base 11× too
/// large (Impose Presence's melee defense, 0.075 read as 0.825).
fn per_foe_subset(a: &AtomicEffect) -> bool {
    if a.conditional_id.is_some() || a.caster_archetypes.is_some() || a.is_deactivation_burst() {
        return false;
    }
    if a.pv_mode == Some(PvMode::PvP) || a.base_probability == Some(0.0) {
        return false;
    }
    if a.tags
        .as_deref()
        .is_some_and(|t| t.split(',').any(|tag| tag.trim() == "Containment"))
    {
        return false;
    }
    let gate = a
        .requires_expression
        .as_deref()
        .map(|req| req.join(" "))
        .unwrap_or_default();
    !(gate.contains("kHitPoints == 0")
        || gate.contains("kMeter > 0")
        || gate.contains("kMeter >=")
        || gate.contains("rand()"))
}

/// Which slot each atom of `selected` reaches, from the router itself.
///
/// A second routing pass rather than a port of `classifyTemplateForStacking`: that classifier is
/// a coarser second mapping over the same templates, and a copy of it here would be the second
/// router this module's header exists to warn about. The pass runs into a throwaway state and
/// only its write ledger is kept.
fn write_ledger(
    power: &Power,
    dataset: DatasetId,
    selected: &[&AtomicEffect],
) -> Vec<(usize, &'static str, Option<Box<str>>)> {
    let mut scratch = State {
        absorb_fraction: absorb_max_hp_fraction(selected),
        absorb_stack_count: absorb_stack_count(selected),
        ..Default::default()
    };
    let twins = twin_roles(selected);
    let targets = power.targets_affected();
    for (i, (a, twin)) in selected.iter().zip(twins).enumerate() {
        scratch.cursor = i;
        route(&mut scratch, a, dataset, twin, targets.as_deref());
    }
    scratch.writes
}

/// The slot keys a per-foe patch carries a SUB-KEY for. `classifyTemplateForStacking` sub-keys
/// exactly the three by-type families and writes every other key flat — including the two keyed
/// containers, whose flat patch then meets a by-type map and is declined.
const PATCH_SUB_KEYED: &[&str] = &["resistance", "defenseBuff", "movement"];

/// `{ scale, table, perTarget }` — the object a patch rebuilds its slot as, in the converter's
/// own key order.
/// The redirect branch's `{scale, table, perTarget}` from its two arms, the same sums
/// [`State::per_foe_patches`] takes for a single-class slot: `scale` is the base arm plus the
/// increments that reach the caster at one foe.
fn redirect_patch<'a>(
    power: &Power,
    stamped: &[&'a AtomicEffect],
    redirect_base: &[&'a AtomicEffect],
) -> (f64, Option<&'a str>, f64) {
    let distinct = |atoms: &[&'a AtomicEffect], value: fn(&AtomicEffect) -> Option<f64>| {
        sum_distinct(
            atoms
                .iter()
                .map(|a| (value(a).unwrap_or(0.0).abs(), a.modifier_table.as_deref())),
        )
    };
    let reaching: Vec<&AtomicEffect> = stamped
        .iter()
        .copied()
        .filter(|a| coh_data::atom::reaches_caster(a, power))
        .collect();
    let table = stamped
        .iter()
        .chain(redirect_base.iter())
        .find_map(|a| a.modifier_table.as_deref());
    (
        distinct(redirect_base, |a| a.redirect_base) + distinct(&reaching, |a| a.per_target),
        table,
        distinct(stamped, |a| a.per_target),
    )
}

fn rebuilt(scale: f64, table: Option<&str>, per_target: f64) -> Value {
    let mut obj = Obj::new();
    obj.insert("scale".into(), num(scale));
    if let Some(table) = table {
        obj.insert("table".into(), Value::String(table.to_owned()));
    }
    obj.insert("perTarget".into(), num(per_target));
    Value::Object(obj)
}

/// `sumDistinctScale`: a by-type buff is ONE logical increment applied across N damage types,
/// and a burst+tail pair repeats the same `(scale, table)` at two durations. Summing raw rows
/// inflates the per-foe value N× on the forks that encode one attrib per template; deduping by
/// `(scale, table)` collapses that to the one real increment, while genuinely distinct
/// increments differ in scale and still sum.
fn sum_distinct<'a>(entries: impl Iterator<Item = (f64, Option<&'a str>)>) -> f64 {
    let mut seen: BTreeSet<(u64, Option<&str>)> = BTreeSet::new();
    let mut total = 0.0;
    for (scale, table) in entries {
        if seen.insert((scale.to_bits(), table)) {
            total += scale;
        }
    }
    total
}

/// Is this power an AoE or Cone with a real target count — the gate
/// `computeAoePerTargetPatches` opens on. 255 is the team-wide sentinel and not a count.
///
/// Read off the export's own def fields rather than the projected bag. A power carries
/// `effectArea` at the top level and `maxTargets` under `stats` — both spellings are emitted by
/// the emitters, so both must be asked (asking only the first reads every ability as
/// single-target, a whole pass declining to run rather than a value that looks slightly off). The
/// transitional `effects` fallback for pseudo-pet geometry was never exercised: a census over all
/// three bundles finds zero powers carrying either field in `effects`, so it is dropped here —
/// the 2-address variant reproduces the 3-address one exactly (0 / 0 / 0 diverge).
fn is_aoe_with_targets(power: &Power) -> bool {
    let def = |field: &str| {
        power
            .extra
            .get(field)
            .or_else(|| power.extra.get("stats").and_then(|s| s.get(field)))
    };
    if !matches!(
        def("effectArea").and_then(Value::as_str),
        Some("AoE") | Some("Cone")
    ) {
        return false;
    }
    def("maxTargets")
        .and_then(Value::as_f64)
        .is_some_and(|n| n > 1.0 && n != 255.0)
}

/// A per-target increment's stack flavour: `Stack`, `Continuous` and `RefreshToCount` accumulate
/// per foe hit, `Replace` is the always-on base.
fn is_increment(a: &AtomicEffect) -> bool {
    matches!(
        a.stacking,
        Some(Stacking::Stack) | Some(Stacking::Continuous) | Some(Stacking::RefreshToCount)
    ) && a.to_who == Some(ToWho::Self_)
}

/// The `Defiance` tag, which the pass excludes from both sides of the sum: Rebirth strips it
/// from a Blaster's flat-table per-foe row, so the tag test is the only thing keeping that row
/// out of the dominant-table group it does not belong to.
fn is_defiance(a: &AtomicEffect) -> bool {
    a.tags
        .as_deref()
        .is_some_and(|t| t.to_ascii_lowercase().contains("defiance"))
}

/// `requiresExcludesSelf`: the RPN clause `entref target> entref source> eq !` — "target ≠
/// source". Phalanx Fighting's per-ally increment carries it, so the buff accrues from nearby
/// allies and never from the caster's own slot.
fn excludes_self(a: &AtomicEffect) -> bool {
    let Some(req) = a.requires_expression.as_deref() else {
        return false;
    };
    let joined = req.join(" ");
    joined.contains("entref target>")
        && joined.contains("entref source>")
        && joined.contains("eq !")
}

/// The two Thunderspy target-trap guards, in the converter's own call order and at the converter's
/// own call SITE: `convert-powerset.cjs:8940` runs them on the finished power, after
/// `_addUnanimousForkedSlots` has merged the per-archetype arms. So they take a built
/// [`WindowSlots`] rather than the router's state, and [`bag_slots`] runs them once on the merged
/// result instead of once per arm.
fn thunderspy_strips(slots: &mut WindowSlots, power: &Power, dataset: DatasetId) {
    if dataset != DatasetId::Thunderspy {
        return;
    }
    if strip_thunderspy_ones_buffs(slots, power) {
        // The Ones-buffs guard re-derives buffDuration from the surviving map (or clears it).
        slots.buff_duration = derive_buff_duration(&slots.durations);
    }
    strip_thunderspy_applied_mez(slots, power);
}

/// `guardThunderspyOnesBuffs`: the recharge/resource target-trap strip. The binary drops
/// aspect and per-template target, so a recovered "buff" on a foe/pet-facing power is kept
/// only when the power's own shortHelp advertises it. Returns the converter's `changed`.
fn strip_thunderspy_ones_buffs(slots: &mut WindowSlots, power: &Power) -> bool {
    // Every arm below decides by what the shortHelp ADVERTISES, so a record carrying no
    // shortHelp field at all gives the guard nothing to weigh, `advertises` answers false to
    // each pattern, and the guard degrades into an unconditional strip. That is absence of
    // evidence read as disproof, and the population it lands on is pet abilities: their
    // records carry neither shortHelp nor targetType, which cost Thunderspy all 13 of its pet
    // +Recharge buffs while every other fork kept theirs (ENT-21). The converter's
    // `guardThunderspyOnesBuffs` is the original and never sees this case — it runs only on
    // the player path, where shortHelp is present on 2873 of 2877 powers — so the drift is
    // this mirror being asked about a population the original was never run over. Decline.
    //
    // Absent, not blank: a present-but-empty shortHelp genuinely advertises nothing, so the
    // strip is faithful there. The four Thunderspy player powers with a missing or empty
    // shortHelp carry none of these keys, so the distinction moves nothing on that side.
    let Some(short_help) = power
        .extra
        .get("shortHelp")
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    let advertises = |pattern: &str| regex_lite_test(short_help, pattern);
    let mut changed = false;
    let mut drop = |slots: &mut WindowSlots, key: &'static str| {
        if slots.present.remove(key) {
            slots.durations.remove(key);
            slots.values.remove(key);
            changed = true;
        }
    };
    if slots.present.contains("rechargeBuff") && !advertises("rech") {
        drop(slots, "rechargeBuff");
    }
    let target_type = power
        .extra
        .get("targetType")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if matches!(target_type, "Foe" | "Location" | "DeadFoe") {
        if slots.present.contains("recoveryBuff") && !advertises_recovery(short_help) {
            drop(slots, "recoveryBuff");
        }
        if slots.present.contains("regenBuff") && !advertises("regen") {
            drop(slots, "regenBuff");
        }
    }
    let ta = power.targets_affected().unwrap_or_default();
    let pet_only = !ta.is_empty() && ta.iter().all(|t| *t == "MyPet");
    if pet_only {
        let has_self = word_boundary_contains(short_help, "self");
        if slots.present.contains("recoveryBuff") && !(has_self && advertises_recovery(short_help))
        {
            drop(slots, "recoveryBuff");
        }
        if slots.present.contains("regenBuff") && !(has_self && advertises("regen")) {
            drop(slots, "regenBuff");
        }
        if slots.present.contains("enduranceGain") && !(has_self && advertises("end")) {
            drop(slots, "enduranceGain");
        }
        if slots.present.contains("defenseBuff") && !(has_self && advertises("def")) {
            drop(slots, "defenseBuff");
        }
    }
    changed
}

/// `/\bself\b/i`: the word with non-word characters (or the string edge) on both sides.
fn word_boundary_contains(text: &str, word: &str) -> bool {
    let lower = text.to_lowercase();
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut start = 0;
    while let Some(pos) = lower[start..].find(word) {
        let at = start + pos;
        let before_ok = lower[..at].chars().next_back().is_none_or(|c| !is_word(c));
        let after_ok = lower[at + word.len()..]
            .chars()
            .next()
            .is_none_or(|c| !is_word(c));
        if before_ok && after_ok {
            return true;
        }
        start = at + 1;
    }
    false
}

/// The guard's `/\+\s*rech/i`-family test: a `+` followed by optional space and the stem,
/// case-insensitive.
fn regex_lite_test(text: &str, stem: &str) -> bool {
    let lower = text.to_lowercase();
    let stem = stem.to_lowercase();
    let bytes = lower.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'+' {
            continue;
        }
        let rest = lower[i + 1..].trim_start();
        if rest.starts_with(stem.as_str()) {
            return true;
        }
    }
    false
}

/// `/\+\s*rec(?:overy|\b)/i`: `+Recovery` or `+Rec` at a word boundary, without matching
/// `+Recharge`.
fn advertises_recovery(text: &str) -> bool {
    let lower = text.to_lowercase();
    let bytes = lower.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'+' {
            continue;
        }
        let rest = lower[i + 1..].trim_start();
        if rest.starts_with("recovery") {
            return true;
        }
        if let Some(after) = rest.strip_prefix("rec") {
            let boundary = after
                .chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
            if boundary {
                return true;
            }
        }
    }
    false
}

/// `guardThunderspyAppliedMez`: on a power whose `targets_affected` names no foe, the recovered
/// applied-control keys are index artifacts, kept only where a negative-magnitude `Mez` atom at
/// aspect `Cur` says the key is real protection.
fn strip_thunderspy_applied_mez(slots: &mut WindowSlots, power: &Power) {
    const APPLIED: &[(&str, &[SubType])] = &[
        ("hold", &[SubType::Held]),
        ("stun", &[SubType::Stunned]),
        ("immobilize", &[SubType::Immobilized]),
        ("sleep", &[SubType::Sleep]),
        ("confuse", &[SubType::Confused]),
        ("fear", &[SubType::Terrorized, SubType::Afraid]),
        ("knockback", &[SubType::Knockback]),
        ("knockup", &[SubType::Knockup]),
    ];
    let ta = power.targets_affected().unwrap_or_default();
    if ta.is_empty() {
        return;
    }
    if ta
        .iter()
        .any(|t| matches!(*t, "Foe" | "DeadFoe" | "DeadOrAliveFoe" | "Any"))
    {
        return;
    }
    // Protection-backed keys read the FULL atom list, gated rows included, matching the
    // converter (it scans the encoded wire atoms).
    let protected: BTreeSet<&str> = APPLIED
        .iter()
        .filter(|(_, subs)| {
            power.atoms.iter().any(|a| {
                // The converter's three spellings of protection (TSPY-8): signed scale on the
                // Res_Boolean armors, signed magnitude on Duration-typed mez, and an Expression
                // magnitude whose sign never reaches the wire. Mirrors
                // `protectionBackedMezKeys` — keep the arms in step or this guard strips keys
                // the converter kept.
                let protection = a.scale.is_some_and(|s| s < 0.0)
                    || a.magnitude.is_some_and(|m| m < 0.0)
                    || a.attrib_type == Some(AttribType::Expression);
                a.effect_type == Some(EffectType::Mez)
                    && protection
                    && a.aspect == Some(Aspect::Cur)
                    && a.sub_type.is_some_and(|sub| subs.contains(&sub))
            })
        })
        .map(|(k, _)| *k)
        .collect();
    let mut changed = false;
    for (key, _) in APPLIED {
        if protected.contains(key) {
            continue;
        }
        if slots.present.remove(key) {
            slots.values.remove(*key);
            changed = true;
        }
        changed |= slots.durations.remove(key).is_some();
    }
    // The converter's META_ONLY cleanup deletes the whole bag when nothing substantive
    // remains; every downstream read then sees nothing.
    if changed && slots.present.is_empty() {
        slots.values.clear();
        slots.durations.clear();
        slots.buff_duration = None;
        slots.effect_duration = None;
        slots.self_marked.clear();
        slots.slow_self = false;
    }
}

fn derive_buff_duration(durations: &BTreeMap<&'static str, f64>) -> Option<f64> {
    if durations.is_empty() {
        return None;
    }
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for d in durations.values() {
        match counts.iter_mut().find(|(v, _)| v == d) {
            Some((_, c)) => *c += 1,
            None => counts.push((*d, 1)),
        }
    }
    let mut best: Option<(f64, usize)> = None;
    for (d, c) in counts {
        let take = match best {
            None => true,
            Some((bd, bc)) => c > bc || (c == bc && d > bd),
        };
        if take {
            best = Some((d, c));
        }
    }
    best.map(|(d, _)| d).filter(|d| *d > 0.0)
}

/// The converter's `MEZ_TYPES` / `KNOCKBACK_TYPES` / `CONTROL_TYPES` key maps, re-keyed from
/// the wire sub type (the bridge's `MEZ_SUBTYPE` covers all three families).
fn mez_key(sub: SubType) -> Option<&'static str> {
    Some(match sub {
        SubType::Held => "hold",
        SubType::Stunned => "stun",
        SubType::Sleep => "sleep",
        SubType::Immobilized => "immobilize",
        SubType::Confused => "confuse",
        SubType::Terrorized | SubType::Afraid => "fear",
        _ => return None,
    })
}

fn kb_key(sub: SubType) -> Option<&'static str> {
    Some(match sub {
        SubType::Knockback => "knockback",
        SubType::Knockup => "knockup",
        SubType::Repel => "repel",
        _ => return None,
    })
}

fn control_key(sub: SubType) -> Option<&'static str> {
    Some(match sub {
        SubType::Taunt => "taunt",
        SubType::Placate => "placate",
        SubType::Untouchable => "untouchable",
        SubType::OnlyAffectsSelf => "onlyAffectsSelf",
        SubType::Teleport => "teleport",
        _ => return None,
    })
}

fn movement_key(sub: SubType) -> Option<&'static str> {
    Some(match sub {
        SubType::Run => "runSpeed",
        SubType::Fly => "flySpeed",
        SubType::Jump => "jumpSpeed",
        SubType::JumpHeight => "jumpHeight",
        SubType::FlyMode => "fly",
        SubType::Control => "movementControl",
        SubType::Friction => "movementFriction",
        _ => return None,
    })
}

/// The converter's `DAMAGE_TYPES` / `DEFENSE_POSITIONS` / `ELUSIVITY_TYPES` sub-keys, as the bag
/// lowercases them, re-keyed from the wire sub type. `All` is the bare `base_defense` /
/// `ElusivityBase` dimension, which those two families spell differently — the caller picks.
fn type_key(sub: SubType) -> Option<&'static str> {
    Some(match sub {
        SubType::Smashing => "smashing",
        SubType::Lethal => "lethal",
        SubType::Fire => "fire",
        SubType::Cold => "cold",
        SubType::Energy => "energy",
        SubType::Negative => "negative",
        SubType::Psionic => "psionic",
        SubType::Toxic => "toxic",
        SubType::Special => "special",
        SubType::Melee => "melee",
        SubType::Ranged => "ranged",
        SubType::AoE => "aoe",
        SubType::All => "all",
        _ => return None,
    })
}

/// The converter's `STEALTH_TYPES` sub-keys.
fn stealth_key(sub: SubType) -> Option<&'static str> {
    Some(match sub {
        SubType::RadiusPvE => "stealthPvE",
        SubType::RadiusPvP => "stealthPvP",
        SubType::Translucency => "translucency",
        _ => return None,
    })
}

/// Which half of a resistible/unresistable twin an atom is, or neither.
#[derive(Clone, Copy, PartialEq)]
enum TwinRole {
    None,
    /// The resistible member: it routes, tagged `unresistable`, standing for both.
    Res,
    /// The `IgnoreResistance` sibling: skipped, since its twin already represents it.
    Unres,
}

/// `twinRole` (`convert-powerset.cjs:6056`), approximated per atom.
///
/// HC splits many foe debuffs into two templates identical but for the `IgnoreResistance`
/// flag — one the target's debuff resistance reduces, one that bypasses it. Both apply, and
/// the single-valued bag slot represents them as ONE value tagged `unresistable`.
///
/// The converter signs a twin by the whole TEMPLATE (its sorted attrib list, aspect, table,
/// scale, duration); the wire carries no template index, so this signs each atom by its own
/// `(effect_type, sub_type, aspect, table, scale, duration)`. Within a genuine pair the two
/// templates carry the same attribs, so atom-for-atom the signatures still match, and adding
/// the sub type keeps two templates with DIFFERENT attrib sets apart where a template-blind
/// signature would have merged them. What remains is one over-approximation: two templates
/// whose attrib sets differ but overlap, agreeing on scale, table and duration, and disagreeing
/// on the flag, would be called twins here and not there — `display_slot_value_atom_bag_parity`
/// is what measures whether the corpus has any.
fn twin_roles(atoms: &[&AtomicEffect]) -> Vec<TwinRole> {
    let sig = |a: &AtomicEffect| {
        (
            a.effect_type,
            a.sub_type,
            a.aspect,
            a.modifier_table.as_deref().map(str::to_owned),
            r4(a.scale.unwrap_or(0.0)),
            a.duration.map(r4),
        )
    };
    let is_twin_debuff = |a: &AtomicEffect| {
        a.scale.unwrap_or(0.0) < 0.0
            || a.modifier_table
                .as_deref()
                .is_some_and(|t| t.to_ascii_lowercase().contains("debuff"))
    };
    let mut seen: Vec<(_, bool, bool)> = Vec::new();
    for a in atoms {
        if !is_twin_debuff(a) {
            continue;
        }
        let key = sig(a);
        let resistible = a.resistible != Some(false);
        match seen.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, res, unres)) => {
                *res |= resistible;
                *unres |= !resistible;
            }
            None => seen.push((key, resistible, !resistible)),
        }
    }
    atoms
        .iter()
        .map(|a| {
            if !is_twin_debuff(a) {
                return TwinRole::None;
            }
            let key = sig(a);
            let paired = seen
                .iter()
                .any(|(k, res, unres)| *k == key && *res && *unres);
            if !paired {
                TwinRole::None
            } else if a.resistible != Some(false) {
                TwinRole::Res
            } else {
                TwinRole::Unres
            }
        })
        .collect()
}

/// `gateExcludesCaster (`convert-powerset.cjs:6177`): does this row's gate say the target is
/// somebody the caster is not? Three clause families, asked of the JOINED expression and never
/// re-split (COND-8).
fn gate_excludes_caster(a: &AtomicEffect) -> bool {
    let Some(requires) = a.requires_expression.as_deref() else {
        return false;
    };
    if requires.is_empty() {
        return false;
    }
    let squashed = requires.join(" ");
    const IDENTITY: [&str; 4] = [
        "entref target> entref source> eq !",
        "entref source> entref target> eq !",
        "entref target> entref source> == !",
        "entref source> entref target> == !",
    ];
    IDENTITY.iter().any(|clause| squashed.contains(clause))
        || squashed.contains("enttype target> critter eq")
        || squashed.contains("target.isFriend? !")
}

/// `reachesCaster (`convert-powerset.cjs:6207`): does this row land on the CASTER once the
/// power resolves the pronoun in it? Only the four regen/recovery buff slots ask, and
/// deliberately — a `target ≠ source` row summed into the caster's own slot states a number
/// nobody receives (ATOM-BAG-5).
///
/// `AnyAffected` names nobody on its own, so the answer needs the power's target list. The
/// converter throws when that list is absent rather than reading it as "no Self", which would
/// silently delete a real self-buff; here the same case leaves the row unmarked, which is the
/// converter's behaviour for every OTHER recipient and keeps the projection total. The corpus
/// gate counts the powers that reach it.
fn reaches_caster(a: &AtomicEffect, targets: Option<&[&str]>) -> bool {
    if !matches!(a.to_who, Some(ToWho::Target) | Some(ToWho::TargetOnly)) {
        return lands_on_caster(a);
    }
    match targets {
        Some(ta) => ta.contains(&"Self") && !gate_excludes_caster(a),
        None => true,
    }
}

/// The per-atom facts the converter's `makeEffect` / `makeMezEffect` closures capture, built
/// once per row so every branch below writes the object those closures would.
struct Row<'a> {
    scale: f64,
    table: Option<&'a str>,
    magnitude: f64,
    unresistable: bool,
    ignore_strength: bool,
    is_self: bool,
    stack_key: Option<&'a str>,
    suppress_stack: bool,
    suppressible: bool,
    /// The atom's own [`AttribType`], for the mez shape alone — which of its two numbers
    /// `scale × table` computes (MEZDUR-1). This projection is the fourth producer of a
    /// `{mag, scale, table}` value (after the powerset converter, its pseudo-pet branch and the
    /// pet-entity table), and the display reader routes every one of them off this field.
    attrib_type: Option<AttribType>,
}

impl Row<'_> {
    /// `makeEffect()`: the slot's magnitude is |scale|, since the sign lives in the slot NAME.
    fn effect(&self) -> Value {
        let mut o = Obj::new();
        o.insert("scale".into(), num(self.scale.abs()));
        if let Some(t) = self.table {
            o.insert("table".into(), Value::String(t.to_owned()));
        }
        if self.unresistable {
            o.insert("unresistable".into(), Value::Bool(true));
        }
        if self.ignore_strength {
            o.insert("ignoreStrength".into(), Value::Bool(true));
        }
        Value::Object(o)
    }

    /// `makeMezEffect()`: a mez carries its magnitude, its scale, and the discriminator saying
    /// which of the two the product is (MEZDUR-1). The scale rides SIGNED and the self
    /// recipient rides as `toWho` (MEZFACE-1), the same face the display reads off a parent
    /// power's row.
    fn mez(&self) -> Value {
        let mut o = Obj::new();
        o.insert("mag".into(), num(self.magnitude));
        o.insert("scale".into(), num(self.scale));
        if let Some(t) = self.table {
            o.insert("table".into(), Value::String(t.to_owned()));
        }
        if let Some(attrib_type) = self.attrib_type {
            o.insert(
                "attribType".into(),
                Value::String(attrib_type.as_wire().to_owned()),
            );
        }
        if self.ignore_strength {
            o.insert("ignoreStrength".into(), Value::Bool(true));
        }
        Value::Object(o)
    }

    /// The `toWho: 'Self'` stamp the caster-penalty slots carry: a self-directed debuff is a
    /// penalty the caster suffers, not something the power inflicts.
    fn self_marked(&self, value: Value) -> Value {
        if self.is_self {
            self.marked_self(value)
        } else {
            value
        }
    }

    /// The same stamp, unconditional — the `slow` branch marks its self entries outright.
    fn marked_self(&self, mut value: Value) -> Value {
        if let Value::Object(o) = &mut value {
            o.insert("toWho".into(), Value::String("Self".into()));
        }
        value
    }

    /// `attachTravelMeta`: the binary suppress group active powers share, and the in-combat
    /// switch-off. Only the movement slots carry them.
    fn travel(&self, mut value: Value) -> Value {
        if let Value::Object(o) = &mut value {
            if self.suppress_stack {
                if let Some(k) = self.stack_key {
                    o.insert("stackKey".into(), Value::String(k.to_owned()));
                }
            }
            if self.suppressible {
                o.insert("suppressible".into(), Value::Bool(true));
            }
        }
        value
    }
}

/// Route one atom the way `projectAtomsToEffects`'s loop does, writing the slot's presence, its
/// recorded duration, and its authored value object.
fn route(
    s: &mut State,
    a: &AtomicEffect,
    dataset: DatasetId,
    twin: TwinRole,
    targets: Option<&[&str]>,
) {
    // The `IgnoreResistance` sibling of a twin pair is skipped: its resistible twin routes to
    // the same slot and represents both.
    if twin == TwinRole::Unres {
        return;
    }
    let Some(et) = a.effect_type else { return };
    let scale = a.scale.unwrap_or(0.0);
    let table_raw = a.modifier_table.as_deref().unwrap_or("");
    let table = table_raw.to_lowercase();
    let is_debuff = scale < 0.0 || table.contains("debuff");
    let is_self = lands_on_caster(a);
    let dur = a.duration.filter(|d| *d > 0.0);
    let suppressed = a.suppressible == Some(true);
    let ignores = a.ignore_strength == Some(true);
    let asp = a.aspect;
    // The converter's `a.magnitude || 1`: a zero magnitude is JS-falsy and reads as one.
    let magnitude = match a.magnitude.unwrap_or(0.0) {
        0.0 => 1.0,
        m => m,
    };
    let row = Row {
        scale,
        table: a.modifier_table.as_deref(),
        magnitude,
        unresistable: twin == TwinRole::Res,
        ignore_strength: ignores,
        is_self,
        stack_key: a.stack_key.as_deref(),
        suppress_stack: a.stacking == Some(Stacking::Suppress),
        suppressible: suppressed,
        attrib_type: a.attrib_type,
    };

    // The AttackType marker approximation: a Strength-aspect zero-scale zero-magnitude row of
    // the damage family is a tagging template, not an effect (the converter skips the whole
    // 7-attrib group; the wire has no group to scan).
    let marker = scale == 0.0
        && a.magnitude.unwrap_or(0.0) == 0.0
        && matches!(et, EffectType::DamageBuff | EffectType::Enhancement);

    // Resource-family inert Expression rows: the group chance is 0, so the effect never fires.
    let inert_expression = a.attrib_type == Some(AttribType::Expression)
        && a.base_probability == Some(0.0)
        && matches!(
            et,
            EffectType::MaxHp
                | EffectType::MaxEndurance
                | EffectType::Endurance
                | EffectType::Recovery
                | EffectType::Regeneration
        );
    if inert_expression {
        return;
    }

    // One resource queue entry, with the fields the converter carries per slot.
    let resource = |not_on_caster: bool, replace: bool| ResourceEntry {
        duration_only: false,
        scale,
        table: a.modifier_table.as_deref().map(str::to_owned),
        is_debuff,
        twin: twin == TwinRole::Res,
        ignore_strength: ignores,
        replace,
        not_on_caster,
        duration: dur,
    };

    match et {
        // `<type>_dmg` at Cur/Abs/Max: a defense table routes to the defense slots, anything
        // else is actual damage and owns no window slot.
        EffectType::Damage => {
            if table.contains("buff_def") || table.contains("debuff_def") {
                let Some(sub) = a.sub_type.and_then(type_key) else {
                    return;
                };
                if is_debuff {
                    s.slot_sub("defenseDebuff", sub, dur, row.self_marked(row.effect()));
                } else if suppressed {
                    s.slot_sub("defenseBuffSuppressible", sub, dur, row.effect());
                } else {
                    s.slot_sub("defenseBuff", sub, dur, row.effect());
                }
            }
        }
        // `<type>_dmg` at Strength: the damage buff/debuff pair.
        EffectType::DamageBuff => {
            if marker {
                return;
            }
            if is_debuff {
                s.penalty_slot("damageDebuff", is_self, dur, row.self_marked(row.effect()));
            } else {
                s.slot("damageBuff", dur, row.effect());
                s.damage_instances
                    .push((r4(scale.abs()), dur.unwrap_or(0.0), a.sub_type));
                s.damage_scale = Some(scale.abs());
            }
        }
        // A damage/position stat: `Res` aspect is the resistance pair, `Str` is the converter's
        // specialBuff branch (the bridge folds a bare stat on a `*Res*` table here at every
        // aspect), `Cur` is its defense branch.
        EffectType::Resistance | EffectType::HealResistance => {
            let aspect_res = et == EffectType::HealResistance || asp == Some(Aspect::Res);
            // `Heal_Dmg`'s resistance face is healing RECEIVED, and the bag files it under the
            // `heal` sub-key of the same map.
            let sub = if et == EffectType::HealResistance {
                "heal"
            } else {
                let Some(sub) = a.sub_type.and_then(type_key) else {
                    return;
                };
                sub
            };
            if aspect_res {
                if is_debuff {
                    s.slot_sub("resistanceDebuff", sub, dur, row.self_marked(row.effect()));
                } else {
                    s.slot_sub("resistance", sub, dur, row.effect());
                }
            } else if asp == Some(Aspect::Str) {
                if is_debuff {
                    s.slot_sub("specialDebuff", sub, dur, row.effect());
                } else {
                    s.slot_sub("specialBuff", sub, dur, row.effect());
                }
            } else if asp == Some(Aspect::Cur) {
                if is_debuff {
                    s.slot_sub("defenseDebuff", sub, dur, row.self_marked(row.effect()));
                } else if suppressed {
                    s.slot_sub("defenseBuffSuppressible", sub, dur, row.effect());
                } else {
                    s.slot_sub("defenseBuff", sub, dur, row.effect());
                }
            }
        }
        EffectType::Defense => {
            // `base_defense` IS the defense characteristic rather than one dimension of it, so
            // its buff/debuff face is the SLOT, where a positional row is one key inside it.
            let bare = !matches!(a.sub_type, Some(sub) if sub != SubType::All);
            let sub = if bare {
                "defense"
            } else {
                let Some(sub) = a.sub_type.and_then(type_key) else {
                    return;
                };
                sub
            };
            if asp == Some(Aspect::Str) {
                if is_debuff {
                    s.slot_sub("specialDebuff", sub, dur, row.effect());
                } else {
                    s.slot_sub("specialBuff", sub, dur, row.effect());
                }
            } else if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", sub, dur, row.effect());
            } else {
                let (key, value) = if is_debuff {
                    ("defenseDebuff", row.self_marked(row.effect()))
                } else if suppressed {
                    ("defenseBuffSuppressible", row.effect())
                } else {
                    ("defenseBuff", row.effect())
                };
                if bare {
                    s.slot(key, dur, value);
                } else {
                    s.slot_sub(key, sub, dur, value);
                }
            }
        }
        // The bridge's Strength bucket spans four converter families; the sub type says which
        // branch would have run.
        EffectType::Enhancement => {
            if let Some(sub) = a.sub_type {
                if let Some(mez) = mez_key(sub) {
                    s.slot_sub("specialBuff", mez, dur, row.effect());
                } else if let Some(kb) = kb_key(sub) {
                    if is_self || scale > 0.0 {
                        s.accumulate_slot(kb, dur, row.effect());
                    }
                } else if control_key(sub).is_some() {
                    // ATOM-BAG-9. An `Enhancement`-face control row buffs the recipient's
                    // taunt/placate STRENGTH; the `taunt`/`placate` slots are read as
                    // RESISTANCE and have no axis to record which face they came from. The
                    // converter stopped writing them here too, so this router must agree or
                    // the presence gate reports the atom side minting a key the bag dropped.
                    // Geode and Light Affinity's Spotlight are the whole population.
                } else if !marker {
                    // Damage types, positions and `All` (base_defense) at Strength.
                    let key = if sub == SubType::All {
                        "defense"
                    } else {
                        let Some(key) = type_key(sub) else { return };
                        key
                    };
                    if is_debuff {
                        s.slot_sub("specialDebuff", key, dur, row.effect());
                    } else {
                        s.slot_sub("specialBuff", key, dur, row.effect());
                    }
                }
            }
        }
        EffectType::Mez => {
            let Some(sub) = a.sub_type else { return };
            if let Some(key) = mez_key(sub) {
                if dataset == DatasetId::Thunderspy && scale < 0.0 && !table.contains("res_boolean")
                {
                    return;
                }
                // A PvE row outranks a PvP one on the same slot; between two of a kind the
                // larger magnitude wins.
                let new = row.self_marked(row.mez());
                let new_pvp = table.contains("pvp");
                let take = match s.bag.get(key) {
                    None => true,
                    Some(cur) => {
                        let cur_pvp = cur
                            .get("table")
                            .and_then(Value::as_str)
                            .is_some_and(|t| t.to_ascii_lowercase().contains("pvp"));
                        if cur_pvp != new_pvp {
                            cur_pvp
                        } else {
                            magnitude > cur.get("mag").and_then(Value::as_f64).unwrap_or(0.0)
                        }
                    }
                };
                if take {
                    s.slot(key, dur, new);
                } else {
                    s.present.insert(key);
                    s.record(key, dur);
                }
                if let Some(d) = dur {
                    s.effect_duration = Some(d);
                }
            } else if let Some(kb) = kb_key(sub) {
                if is_self || scale > 0.0 {
                    s.accumulate_slot(kb, dur, row.effect());
                }
            } else if let Some(ctrl) = control_key(sub) {
                s.slot_ctrl(ctrl, dur, row.effect(), redirect_collected(a));
            }
        }
        EffectType::MezResist => {
            let Some(sub) = a.sub_type else { return };
            if let Some(mez) = mez_key(sub) {
                s.accumulate_sub("mezResistance", mez, dur, row.effect());
            } else if let Some(ctrl) = control_key(sub) {
                s.accumulate_sub("mezResistance", ctrl, dur, row.effect());
            } else if let Some(kb) = kb_key(sub) {
                if is_self {
                    if table.contains("res_boolean") {
                        s.accumulate_slot(kb, dur, row.effect());
                    } else {
                        s.slot_sub("mezResistance", kb, dur, row.effect());
                    }
                }
            }
        }
        EffectType::Movement => {
            let Some(axis) = a.sub_type.and_then(movement_key) else {
                return;
            };
            let is_slow = is_debuff || scale < 0.0 || table.contains("slow");
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "movement", dur, row.effect());
            } else if is_self && asp == Some(Aspect::Str) {
                s.slot_sub("specialBuff", "movement", dur, row.effect());
            } else if is_self
                && asp == Some(Aspect::Max)
                && scale > 0.0
                && axis != "movementControl"
                && axis != "movementFriction"
            {
                s.slot_sub("movementCapBump", axis, dur, row.travel(row.effect()));
            } else if a.sub_type == Some(SubType::FlyMode) && is_slow {
                // MOVEMAP-7: the kFly mode row isn't a slow magnitude. The converter
                // skips it in the debuff display slots, so the mirror does too.
            } else if asp == Some(Aspect::Max) && is_slow {
                s.slot_sub(
                    "movementCapDebuff",
                    axis,
                    dur,
                    row.self_marked(row.effect()),
                );
            } else if is_self && is_slow {
                s.slot_sub("slow", axis, dur, row.marked_self(row.effect()));
                s.slow_axis_self.insert(axis, true);
            } else if is_self {
                let value = row.travel(row.effect());
                s.movement_buff_slot(axis, dur, value, a.ignore_strength == Some(true));
            } else if is_slow {
                s.slot_sub("slow", axis, dur, row.effect());
                s.slow_axis_self.insert(axis, false);
            } else if asp == Some(Aspect::Cur) {
                let value = row.travel(row.effect());
                s.movement_buff_slot(axis, dur, value, a.ignore_strength == Some(true));
            }
        }
        EffectType::MaxHp => {
            if asp == Some(Aspect::Max) {
                if is_debuff {
                    return;
                }
                let key = if ignores {
                    "maxHPBuffUnenhanced"
                } else {
                    "maxHPBuff"
                };
                let replace = a.stacking == Some(Stacking::Replace);
                s.queue_resource(key, resource(false, replace));
            } else {
                s.queue_resource("healing", resource(false, false));
            }
        }
        EffectType::MaxEndurance => s.queue_resource("maxEndBuff", resource(false, false)),
        EffectType::Endurance => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "endurance", dur, row.effect());
            } else if asp == Some(Aspect::Str) {
                s.slot_sub("specialBuff", "endurance", dur, row.effect());
            } else if is_debuff || scale < 0.0 {
                s.queue_resource("enduranceDrain", resource(false, false));
            } else {
                s.queue_resource("enduranceGain", resource(false, false));
            }
        }
        EffectType::Recovery => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "recovery", dur, row.effect());
            } else if is_debuff || scale < 0.0 {
                s.queue_resource("recoveryDebuff", resource(false, false));
            } else {
                let off_caster = !reaches_caster(a, targets);
                let key = if ignores {
                    "recoveryBuffUnenhanced"
                } else {
                    "recoveryBuff"
                };
                s.queue_resource(key, resource(off_caster, false));
            }
        }
        EffectType::Regeneration => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "regeneration", dur, row.effect());
            } else if is_debuff || scale < 0.0 {
                s.queue_resource("regenDebuff", resource(false, false));
            } else if a.stack_by_attrib_and_key == Some(true)
                && matches!(a.stacking, Some(Stacking::Stack | Stacking::Continuous))
            {
                // The converter's StackByAttribAndKey skip, asked of the flag itself: these
                // are the per-target increments the perTarget pipeline folds elsewhere, and
                // routing them here too would double-count them. The `Stack`/`Continuous`
                // half is load-bearing — the flag alone means ordinary refresh semantics
                // (Icy Bastion's lingering +4), which this branch must NOT swallow.
                //
                // The slot is still ATTRIBUTED, with no value queued: the row this branch
                // declines to route is the very row the per-foe pass then patches, and a skip
                // that recorded nothing would leave its stamp with no slot to land in.
                s.mark(
                    if ignores {
                        "regenBuffUnenhanced"
                    } else {
                        "regenBuff"
                    },
                    None,
                );
            } else {
                let off_caster = !reaches_caster(a, targets);
                let key = if ignores {
                    "regenBuffUnenhanced"
                } else {
                    "regenBuff"
                };
                s.queue_resource(key, resource(off_caster, false));
            }
        }
        EffectType::Absorb => {
            if asp == Some(Aspect::Max) && a.attrib_type == Some(AttribType::Expression) {
                // The recovered fraction IS the slot's value, so the key exists even though the
                // queue entry below is duration-only. The first Expression row to reach a
                // fraction owns the slot; the resource fold's own effect overwrites it when a
                // non-Expression absorb also queued.
                if let Some((fraction, applies_strength)) = s.absorb_fraction {
                    if !s.bag.contains_key("absorb") {
                        let mut o = Obj::new();
                        o.insert("maxHPFraction".into(), num(fraction));
                        if applies_strength {
                            o.insert("appliesStrength".into(), Value::Bool(true));
                        }
                        if let Some(t) = row.table {
                            o.insert("table".into(), Value::String(t.to_owned()));
                        }
                        s.present.insert("absorb");
                        s.bag.insert("absorb".into(), Value::Object(o));
                    }
                }
                if let Some(d) = dur {
                    s.queue_resource(
                        "absorb",
                        ResourceEntry {
                            duration_only: true,
                            duration: Some(d),
                            ..ResourceEntry::default()
                        },
                    );
                }
            } else if asp == Some(Aspect::Str) {
                s.slot_sub("specialBuff", "absorb", dur, row.effect());
            } else {
                s.queue_resource("absorb", resource(false, false));
            }
        }
        EffectType::ToHit => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "tohit", dur, row.effect());
            } else if asp == Some(Aspect::Str) {
                s.slot_sub("specialBuff", "tohit", dur, row.effect());
            } else if is_debuff {
                s.penalty_slot("tohitDebuff", is_self, dur, row.self_marked(row.effect()));
            } else if ignores {
                s.slot("tohitBuffUnenhanced", dur, row.effect());
            } else {
                s.accumulate_tohit(row.effect(), dur);
            }
        }
        EffectType::Accuracy => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "accuracy", dur, row.effect());
            } else if is_debuff || scale < 0.0 {
                s.penalty_slot(
                    "accuracyDebuff",
                    is_self,
                    dur,
                    row.self_marked(row.effect()),
                );
            } else {
                s.slot("accuracyBuff", dur, row.effect());
            }
        }
        EffectType::RechargeTime => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "recharge", dur, row.effect());
            } else if is_debuff || scale < 0.0 || table.contains("slow") {
                s.penalty_slot(
                    "rechargeDebuff",
                    is_self,
                    dur,
                    row.self_marked(row.effect()),
                );
            } else {
                s.slot("rechargeBuff", dur, row.effect());
            }
        }
        EffectType::ThreatLevel => {
            if is_debuff || scale < 0.0 {
                s.slot("threatDebuff", dur, row.effect());
            } else {
                s.slot("threatBuff", dur, row.effect());
            }
        }
        EffectType::Range => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "range", dur, row.effect());
            } else if is_debuff || scale < 0.0 {
                if is_self {
                    s.slot("rangeDebuff", dur, row.marked_self(row.effect()));
                }
            } else if reaches_caster(a, targets) {
                // ATOM-BAG-9. The slot is the CASTER's range, so the question is the power's
                // recipients, not the row's aspect. The `Aspect::Str` disjunct this replaces
                // was written for Power of the Depths, whose `["Friend", "Self"]` reaches the
                // caster anyway; Brainstorm's Magnify is `["Friend"]` alone and does not.
                s.slot("rangeBuff", dur, row.effect());
            }
        }
        EffectType::EnduranceDiscount => s.slot("enduranceDiscount", dur, row.effect()),
        EffectType::Perception => {
            if asp == Some(Aspect::Res) {
                s.slot_sub("debuffResistance", "perception", dur, row.effect());
            } else if is_debuff || scale < 0.0 {
                s.slot("perceptionDebuff", dur, row.effect());
            } else {
                s.slot("perceptionBuff", dur, row.effect());
            }
        }
        EffectType::Stealth => {
            let Some(axis) = a.sub_type.and_then(stealth_key) else {
                return;
            };
            if asp != Some(Aspect::Cur) {
                return;
            }
            s.slot_sub("stealth", axis, dur, row.effect());
            // The suppress group is a sibling of the axes rather than one of them, and the
            // sentinel spellings are not a group.
            if row.suppress_stack {
                if let Some(key) = row.stack_key.filter(|k| *k != "0" && *k != "4294967295") {
                    s.slot_sub("stealth", "stackKey", None, Value::String(key.to_owned()));
                }
            }
        }
        EffectType::Elusivity => {
            let Some(sub) = a.sub_type.and_then(type_key) else {
                return;
            };
            // No `recordDuration` — the converter's elusivity branch records none.
            s.slot_sub("elusivity", sub, None, row.effect());
        }
        EffectType::Heal => {
            if asp == Some(Aspect::Str) {
                s.slot_sub("specialBuff", "heal", dur, row.effect());
            }
        }
        // A created entity is the bag's `summon` slot. Its VALUE is `extractSummon`'s, built
        // from the template's pet parameters outside this projection, so the key is stated here
        // without one (ENT-14).
        EffectType::EntCreate => {
            s.present.insert("summon");
        }
        EffectType::ExecutePower
        | EffectType::GlobalChanceMod
        | EffectType::GrantPower
        | EffectType::Meta
        | EffectType::RechargePower
        | EffectType::Unmapped => {}
    }
}
