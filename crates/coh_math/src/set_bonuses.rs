//! IO set bonuses + the Rule of 5, a faithful port of the beta
//! `src/utils/calculations/set-bonuses.ts` (`calculateSetBonuses`).
//!
//! Two normalization layers, exactly as the beta:
//!   1. THIS module (layer 1) walks each power's slotted set pieces, counts pieces
//!      per set per power, fires every tier whose threshold is met, normalizes each
//!      effect's snake-case stat to an internal [`SetBonusStat`] key, expands `resAll`
//!      and mirrors the S/L·F/C·E/N paired stats, then applies the Rule of 5
//!      (identical `(stat, value)` bonuses stack at most five times, build-wide).
//!      Output is keyed by internal stat, the surface the `set_bonus_gate` grades.
//!   2. [`apply_set_bonuses_to_global`] (layer 2) routes those internal keys into
//!      `GlobalBonuses` fields (the beta `applySetBonusesToGlobal` + `STAT_TO_GLOBAL`);
//!      graded both directly (the set-bonus gate replays it) and end-to-end (the totals
//!      gate, once [`crate::recalculate`] folds it in after Pass 1).
//!
//! Pure calc: build + catalog in, per-stat numbers out, no I/O.
//!
//! ## The load-bearing subtlety
//! The Rule-of-5 dedup key is `(stat, value)`, NOT set identity. Two different sets
//! each granting +9% Accuracy share one cap bucket (correct CoH behavior). Piece
//! counting for tier unlocking is strictly PER POWER; the Rule of 5 is build-wide.
//!
//! Traversal order (`primary → secondary → pools → epic → inherents`, via
//! [`CharacterState::all_selected`]) matches the beta's. It decides which sources
//! land in the accepted vs. rejected list on a capped bucket, but NOT the numeric
//! total or the accepted count (both are order-independent: count = `min(N, 5)`),
//! so the gate grades count + `capped` + the summed value, never source order.

use coh_data::{EnhancementKind, IoSetCatalog, Level, SelectedPower};
use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use crate::totals::{route_closed, CalcError, GlobalBonuses};

/// The set-bonus stat vocabulary, the single source of truth shared with the beta (PROD6A).
/// Embedded from the exact file the build step copies into the beta
/// (`CoH-Sidekick/scripts/build-engine.mjs` → its `normalizeStatName` / `getPairedStat`), so
/// the two formerly hand-maintained tables ([`map_stat_name`] here and the beta `STAT_NAME_MAP`)
/// can no longer drift (the FLAGS-2 hazard: two self-consistent tables that silently diverge).
/// See `hand-data/set-bonus-stat-vocab.json`.
const STAT_VOCAB_JSON: &str = include_str!("../../../hand-data/set-bonus-stat-vocab.json");

/// Every raw stat the vocab maps to `null`: the reason the calc doesn't spend it, and the stats
/// its label nonetheless NAMES. The list IS the guard: a `null` the vocab grows without an entry
/// here panics at build, so "the calc drops this" can never again be a default nobody argued for.
///
/// Only one shape is left, a stat that IS spent, just not from here. The other, modelled
/// nowhere, is empty since SETSTAT-1: `knockback_strength` got a `GlobalBonuses` field of its
/// own, and `endurance_drain_resistance` turned out not to need one, being the same
/// `kEndurance`/`aspect=Res` attrib that `debuffResistEndurance` already carried.
///
/// The `names` column exists because "the calc spends this elsewhere" and "the set does not grant
/// this" are different facts, and only the first is true here. A reader searching the catalog for
/// Melee Defense wants Steadfast Protection in the answer ([`stats_named`]); a calc pass adding
/// its value a second time would double-count it ([`calculate_set_bonuses`]). One table states
/// both, so the two readings can't drift apart.
const UNSPENT_STATS: &[(&str, &str, &[SetBonusStat])] = &[
    // Applied elsewhere. The +3% Def uniques (Steadfast Protection piece 2, Gladiator's Armor
    // piece 6) reach the totals through the piece/global proc path, where proc-data carries
    // them as `Defense`/`All` and `procs::apply_single_proc_effect` expands them across the
    // eleven vectors. Spending the set-bonus row too would double-count them.
    (
        "defense_(all)",
        "applied via the piece/global proc path",
        &ALL_DEF_TYPES,
    ),
];

/// `statNameMap` from [`STAT_VOCAB_JSON`], parsed once into the raw-string → [`StatMapping`]
/// lookup [`map_stat_name`] serves. A `null` value is [`StatMapping::Unspent`] carrying its
/// [`UNSPENT_STATS`] reason, `"resAll"` is [`StatMapping::ResAll`], `"mezresist"` is
/// [`StatMapping::MezAll`], any other value an internal key resolved to its [`SetBonusStat`].
/// A malformed vocab (bad JSON, an internal key with no [`SetBonusStat`], or a `null` with no
/// stated reason) is a checked-in-data invariant proven by
/// [`tests::vocab_file_builds_and_covers_every_stat`], so the build-time `expect`/`panic` here
/// can only fire on a broken commit, never on calc input.
static STAT_NAME_MAP: LazyLock<HashMap<String, StatMapping>> = LazyLock::new(|| {
    #[derive(serde::Deserialize)]
    struct Vocab {
        #[serde(rename = "statNameMap")]
        stat_name_map: BTreeMap<String, Option<String>>,
    }
    let vocab: Vocab =
        serde_json::from_str(STAT_VOCAB_JSON).expect("set-bonus-stat-vocab.json is valid JSON");
    vocab
        .stat_name_map
        .into_iter()
        .map(|(raw, key)| {
            let mapping = match key.as_deref() {
                None => UNSPENT_STATS
                    .iter()
                    .find(|(stat, _, _)| *stat == raw)
                    .map(|(_, reason, names)| StatMapping::Unspent { reason, names })
                    .unwrap_or_else(|| {
                        panic!(
                            "set-bonus-stat-vocab.json: {raw:?} maps to null with no entry in \
                             UNSPENT_STATS — say why the calc does not spend it, and what its \
                             label names"
                        )
                    }),
                Some("resAll") => StatMapping::ResAll,
                // The beta's scalar mez-resist-all key. It routes to a `GlobalBonuses` field
                // the beta writes and never reads; here it expands per type instead, so the
                // vocab's one key becomes an expansion rather than a stat (MEZRES-1).
                Some("mezresist") => StatMapping::MezAll,
                Some(k) => StatMapping::Stat(SetBonusStat::from_beta_key(k).unwrap_or_else(|| {
                    panic!("set-bonus-stat-vocab.json: unknown internal key {k:?} for raw {raw:?}")
                })),
            };
            (raw, mapping)
        })
        .collect()
});

/// An internal set-bonus stat key, the closed universe of `STAT_NAME_MAP` outputs
/// (the beta's normalized keys). A raw stat that maps to none of these is unknown
/// (fail-loud: recorded in the error channel, dropped from the total, matching the
/// beta's warn-and-skip while staying visible). Exhaustive by
/// construction: a new mappable stat must be added here or the build breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetBonusStat {
    Damage,
    Accuracy,
    ToHit,
    Recharge,
    Endrdx,
    Range,
    DefMelee,
    DefRanged,
    DefAoE,
    DefSmashing,
    DefLethal,
    DefFire,
    DefCold,
    DefEnergy,
    DefNegative,
    DefPsionic,
    DefToxic,
    ResSmashing,
    ResLethal,
    ResFire,
    ResCold,
    ResEnergy,
    ResNegative,
    ResPsionic,
    ResToxic,
    Recovery,
    Regeneration,
    MaxHp,
    MaxEnd,
    RunSpeed,
    FlySpeed,
    JumpSpeed,
    JumpHeight,
    MezResistHold,
    MezResistStun,
    MezResistImmobilize,
    MezResistSleep,
    MezResistConfuse,
    MezResistFear,
    /// Repel resistance, which the six above do not cover and knockback's key does not reach.
    /// Its only set-bonus carriers are the PvP tiers' `Repel_Resist_*` bonus powers; the
    /// character stat behind it is the one MEZRES-3 gave a field of its own.
    MezResistRepel,
    ImmobilizeDuration,
    HoldDuration,
    StunDuration,
    SleepDuration,
    ConfuseDuration,
    TerrorDuration,
    KbProtection,
    KbResistance,
    HealOther,
    DebuffResistRecharge,
    DebuffResistSlow,
    PerceptionRadius,
    KnockbackStrength,
    DebuffResistEndurance,
}

impl SetBonusStat {
    /// Every variant, the closed internal-key universe. Public because it lets a consumer
    /// enumerate the breakdown keys a build can actually produce ([`Self::breakdown_keys`])
    /// instead of guessing: a UI keyed to a breakdown name no set bonus ever writes is a cue
    /// that silently never fires, which reads exactly like "this build has no capped bonuses".
    /// The round-trip test below proves the list total.
    pub const ALL: &'static [SetBonusStat] = {
        use SetBonusStat::*;
        &[
            Damage,
            Accuracy,
            ToHit,
            Recharge,
            Endrdx,
            Range,
            DefMelee,
            DefRanged,
            DefAoE,
            DefSmashing,
            DefLethal,
            DefFire,
            DefCold,
            DefEnergy,
            DefNegative,
            DefPsionic,
            DefToxic,
            ResSmashing,
            ResLethal,
            ResFire,
            ResCold,
            ResEnergy,
            ResNegative,
            ResPsionic,
            ResToxic,
            Recovery,
            Regeneration,
            MaxHp,
            MaxEnd,
            RunSpeed,
            FlySpeed,
            JumpSpeed,
            JumpHeight,
            MezResistHold,
            MezResistStun,
            MezResistImmobilize,
            MezResistSleep,
            MezResistConfuse,
            MezResistFear,
            MezResistRepel,
            ImmobilizeDuration,
            HoldDuration,
            StunDuration,
            SleepDuration,
            ConfuseDuration,
            TerrorDuration,
            KbProtection,
            KbResistance,
            HealOther,
            DebuffResistRecharge,
            DebuffResistSlow,
            PerceptionRadius,
            KnockbackStrength,
            DebuffResistEndurance,
        ]
    };
}

impl SetBonusStat {
    /// The beta's internal-key string for this stat, the exact key the aggregated
    /// map and the `set_bonus_gate` fixtures are keyed by.
    pub fn as_beta_key(self) -> &'static str {
        use SetBonusStat::*;
        match self {
            Damage => "damage",
            Accuracy => "accuracy",
            ToHit => "tohit",
            Recharge => "recharge",
            Endrdx => "endrdx",
            Range => "range",
            DefMelee => "defMelee",
            DefRanged => "defRanged",
            DefAoE => "defAoE",
            DefSmashing => "defSmashing",
            DefLethal => "defLethal",
            DefFire => "defFire",
            DefCold => "defCold",
            DefEnergy => "defEnergy",
            DefNegative => "defNegative",
            DefPsionic => "defPsionic",
            DefToxic => "defToxic",
            ResSmashing => "resSmashing",
            ResLethal => "resLethal",
            ResFire => "resFire",
            ResCold => "resCold",
            ResEnergy => "resEnergy",
            ResNegative => "resNegative",
            ResPsionic => "resPsionic",
            ResToxic => "resToxic",
            Recovery => "recovery",
            Regeneration => "regeneration",
            MaxHp => "maxhp",
            MaxEnd => "maxend",
            RunSpeed => "runspeed",
            FlySpeed => "flyspeed",
            JumpSpeed => "jumpspeed",
            JumpHeight => "jumpheight",
            // The six the beta collapses into its scalar `mezresist` (MEZRES-1). They are
            // this calc's keys, not the beta's, so they take the camelCase spelling of the
            // accumulator field rather than a beta internal key that doesn't exist.
            MezResistHold => "mezResistHold",
            MezResistStun => "mezResistStun",
            MezResistImmobilize => "mezResistImmobilize",
            MezResistSleep => "mezResistSleep",
            MezResistConfuse => "mezResistConfuse",
            MezResistFear => "mezResistFear",
            MezResistRepel => "mezResistRepel",
            ImmobilizeDuration => "immobilizeDuration",
            HoldDuration => "holdDuration",
            StunDuration => "stunDuration",
            SleepDuration => "sleepDuration",
            ConfuseDuration => "confuseDuration",
            TerrorDuration => "terrorDuration",
            KbProtection => "kbprotection",
            KbResistance => "kbresistance",
            HealOther => "healOther",
            DebuffResistRecharge => "debuffresistrecharge",
            DebuffResistSlow => "debuffresistslow",
            PerceptionRadius => "perceptionradius",
            KnockbackStrength => "knockbackstrength",
            DebuffResistEndurance => "debuffresistendurance",
        }
    }

    /// Inverse of [`as_beta_key`](Self::as_beta_key): resolve an internal-key string back to its
    /// [`SetBonusStat`]. Used to build the [`STAT_NAME_MAP`] lookup from the vocab file's key
    /// strings. `None` for a string outside the closed internal-key universe.
    pub fn from_beta_key(key: &str) -> Option<SetBonusStat> {
        use SetBonusStat::*;
        Some(match key {
            "damage" => Damage,
            "accuracy" => Accuracy,
            "tohit" => ToHit,
            "recharge" => Recharge,
            "endrdx" => Endrdx,
            "range" => Range,
            "defMelee" => DefMelee,
            "defRanged" => DefRanged,
            "defAoE" => DefAoE,
            "defSmashing" => DefSmashing,
            "defLethal" => DefLethal,
            "defFire" => DefFire,
            "defCold" => DefCold,
            "defEnergy" => DefEnergy,
            "defNegative" => DefNegative,
            "defPsionic" => DefPsionic,
            "defToxic" => DefToxic,
            "resSmashing" => ResSmashing,
            "resLethal" => ResLethal,
            "resFire" => ResFire,
            "resCold" => ResCold,
            "resEnergy" => ResEnergy,
            "resNegative" => ResNegative,
            "resPsionic" => ResPsionic,
            "resToxic" => ResToxic,
            "recovery" => Recovery,
            "regeneration" => Regeneration,
            "maxhp" => MaxHp,
            "maxend" => MaxEnd,
            "runspeed" => RunSpeed,
            "flyspeed" => FlySpeed,
            "jumpspeed" => JumpSpeed,
            "jumpheight" => JumpHeight,
            "mezResistHold" => MezResistHold,
            "mezResistStun" => MezResistStun,
            "mezResistImmobilize" => MezResistImmobilize,
            "mezResistSleep" => MezResistSleep,
            "mezResistConfuse" => MezResistConfuse,
            "mezResistFear" => MezResistFear,
            "mezResistRepel" => MezResistRepel,
            "immobilizeDuration" => ImmobilizeDuration,
            "holdDuration" => HoldDuration,
            "stunDuration" => StunDuration,
            "sleepDuration" => SleepDuration,
            "confuseDuration" => ConfuseDuration,
            "terrorDuration" => TerrorDuration,
            "kbprotection" => KbProtection,
            "kbresistance" => KbResistance,
            "healOther" => HealOther,
            "debuffresistrecharge" => DebuffResistRecharge,
            "debuffresistslow" => DebuffResistSlow,
            "perceptionradius" => PerceptionRadius,
            "knockbackstrength" => KnockbackStrength,
            "debuffresistendurance" => DebuffResistEndurance,
            _ => return None,
        })
    }

    /// The paired damage type that also receives a bonus applied to this stat (the
    /// beta `PAIRED_STATS`): the S/L, F/C, E/N couplings for defense and resistance.
    /// Positional defense, typed Psionic/Toxic defense, and every non-defense stat
    /// have no pair.
    pub fn paired(self) -> Option<SetBonusStat> {
        use SetBonusStat::*;
        Some(match self {
            ResSmashing => ResLethal,
            ResLethal => ResSmashing,
            ResFire => ResCold,
            ResCold => ResFire,
            ResEnergy => ResNegative,
            ResNegative => ResEnergy,
            ResPsionic => ResToxic,
            ResToxic => ResPsionic,
            DefSmashing => DefLethal,
            DefLethal => DefSmashing,
            DefFire => DefCold,
            DefCold => DefFire,
            DefEnergy => DefNegative,
            DefNegative => DefEnergy,
            _ => return None,
        })
    }

    /// The `GlobalBonuses` field (beta camelCase, an [`GlobalBonuses::add_by_camel_name`] key)
    /// this stat routes into, the beta `STAT_TO_GLOBAL`. TOTAL: every tracked stat has a target
    /// field, so there's no "routes nowhere" state to represent. The two stats whose routing is
    /// NOT a plain 1:1 add are handled by [`apply_set_bonuses_to_global`], not here:
    /// [`KbProtection`](SetBonusStat::KbProtection) (×0.01 scale) and
    /// [`DebuffResistRecharge`](SetBonusStat::DebuffResistRecharge), which also feeds Slow via
    /// the beta `PAIRED_STATS`.
    fn global_field(self) -> &'static str {
        use SetBonusStat::*;
        match self {
            Damage => "damage",
            Accuracy => "accuracy",
            ToHit => "toHit",
            Recharge => "recharge",
            Endrdx => "endurance",
            Range => "range",
            DefMelee => "defMelee",
            DefRanged => "defRanged",
            DefAoE => "defAoE",
            DefSmashing => "defSmashing",
            DefLethal => "defLethal",
            DefFire => "defFire",
            DefCold => "defCold",
            DefEnergy => "defEnergy",
            DefNegative => "defNegative",
            DefPsionic => "defPsionic",
            DefToxic => "defToxic",
            ResSmashing => "resSmashing",
            ResLethal => "resLethal",
            ResFire => "resFire",
            ResCold => "resCold",
            ResEnergy => "resEnergy",
            ResNegative => "resNegative",
            ResPsionic => "resPsionic",
            ResToxic => "resToxic",
            Recovery => "recovery",
            Regeneration => "regeneration",
            MaxHp => "maxHP",
            MaxEnd => "maxEndurance",
            RunSpeed => "runSpeed",
            FlySpeed => "flySpeed",
            // The beta's `STAT_TO_GLOBAL` has no `jumpspeed` key, so a JumpSpeed set bonus routed
            // nowhere there. It routes here: `jumpSpeed` is a real `GlobalBonuses` field that the
            // movement pass and the self-slow path both already write, so the beta's omission is a
            // table gap, not a statement about the game. A deliberate divergence from the frozen
            // oracle, and a free one, since no dataset ships a jumpspeed set bonus (measured, all
            // three). Left unrouted it would have been a MEZRES-2 waiting to happen: a live stat
            // with a working target field, dropped by an inherited gap.
            JumpSpeed => "jumpSpeed",
            JumpHeight => "jumpHeight",
            MezResistHold => "mezResistHold",
            MezResistStun => "mezResistStun",
            MezResistImmobilize => "mezResistImmobilize",
            MezResistSleep => "mezResistSleep",
            MezResistConfuse => "mezResistConfuse",
            MezResistFear => "mezResistFear",
            MezResistRepel => "mezResistRepel",
            ImmobilizeDuration => "immobilizeDuration",
            HoldDuration => "holdDuration",
            StunDuration => "stunDuration",
            SleepDuration => "sleepDuration",
            ConfuseDuration => "confuseDuration",
            TerrorDuration => "terrorDuration",
            KbProtection => "protKnockback",
            KbResistance => "mezResistKnockback",
            HealOther => "healOther",
            DebuffResistRecharge => "debuffResistRecharge",
            DebuffResistSlow => "debuffResistSlow",
            PerceptionRadius => "perceptionRadius",
            KnockbackStrength => "knockbackStrength",
            // The raw stat is spelled `endurance_drain_resistance`, but it isn't a new stat:
            // resistance to endurance drain IS `kEndurance` on an `aspect=Res` template, the
            // same attrib every `debuffResistance.endurance` power authors (Grounded, Murky
            // Cloud, Static Shield). Routing it anywhere else would split one character stat
            // across two dashboard rows. That's SETSTAT-2's shape, a live stat with a working
            // target field. Its siblings `DebuffResistRecharge`/`DebuffResistSlow` route the
            // same way.
            DebuffResistEndurance => "debuffResistEndurance",
        }
    }

    /// The dashboard breakdown-map key(s) this stat's sources surface under, the beta's
    /// Step-3 routing `PAIRED_STATS[normalized] ?? [STAT_TO_GLOBAL[normalized] || stat]`
    /// (`character-totals.ts:4290`). For every tracked stat `STAT_TO_GLOBAL` equals
    /// [`global_field`](Self::global_field), so this is that camelCase target, except for
    /// the one the beta fans: [`DebuffResistRecharge`](Self::DebuffResistRecharge) surfaces under
    /// BOTH `debuffResistRecharge` and `debuffResistSlow` (the beta `PAIRED_STATS`). The beta's
    /// `|| stat` fallback had exactly one subject, `JumpSpeed`, which now has a real target.
    pub fn breakdown_keys(self) -> Vec<&'static str> {
        match self {
            SetBonusStat::DebuffResistRecharge => vec!["debuffResistRecharge", "debuffResistSlow"],
            other => vec![other.global_field()],
        }
    }
}

/// The result of normalizing one raw set-bonus stat string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StatMapping {
    /// A concrete trackable stat.
    Stat(SetBonusStat),
    /// `damage_resistance_(all)`, which expands to all eight resistance types (no pairing;
    /// the eight are already the full set).
    ResAll,
    /// `mez_resistance_(all)`, which expands to the mez types the effect's own
    /// [`mez_types`](coh_data::SetBonusEffect::mez_types) names. Unlike [`ResAll`](Self::ResAll)
    /// the set isn't implied by the label: the binary encodes the bonus as one multi-attrib
    /// template and the converter carries those attribs onto the effect, so the expansion reads
    /// the data rather than restating what "all" ought to mean.
    MezAll,
    /// A stat this calc deliberately doesn't spend, carrying both columns of its
    /// [`UNSPENT_STATS`] row. Named for what it is rather than "ignored": a `null` in the vocab
    /// is either a value applied on another path or a value nothing models, and those are
    /// different facts. Collapsing them lost that distinction. `defense_(all)` reads as a
    /// dropped +3% Defense until you check the proc path.
    Unspent {
        reason: &'static str,
        /// What the label names, for the readers that describe the bonus rather than spend it.
        names: &'static [SetBonusStat],
    },
}

/// Normalize a raw set-bonus stat string to an internal key, the beta `normalizeStatName`.
/// `None` = unknown (not in the vocab): the caller records it in the error channel and drops
/// it (fail-loud). The mapping is data now, not code: it reads the shared
/// [`STAT_NAME_MAP`] built from `hand-data/set-bonus-stat-vocab.json`, the same file the beta
/// consumes, so there's one vocabulary rather than two hand-maintained twins. The CamelCase
/// aliases never appear in the exported contract (they're import-path keys) but are carried in
/// the vocab so a build imported from the beta's own data matches the beta verbatim.
fn map_stat_name(raw: &str) -> Option<StatMapping> {
    STAT_NAME_MAP.get(raw).copied()
}

/// How a raw set-bonus stat resolves for a per-effect tooltip row (the beta `normalizeStatName`
/// followed by the `x/5` lookup): whether it has a Rule-of-5 tracking bucket to surface a count
/// for, is known but carries no single tracking key (`resAll`, expanded to eight types, or an
/// explicitly-ignored stat, both of which render their description with no counter, matching the
/// beta's `getTotalBonusCount → 0`), or is unrecognized. Unknown drives a fail-loud marker while
/// the rest of the tooltip still renders. That's the tooltip's one departure
/// from the beta, which renders an unknown stat's description silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectStatKey {
    /// A trackable stat. Look its count/capped up with [`bonus_tracking_lookup`] under this
    /// [`SetBonusStat::as_beta_key`].
    Tracked(&'static str),
    /// Known to the calc but with no single tracking bucket (`resAll` / an ignored stat).
    KnownUntracked,
    /// Not in the stat map, surfaced as a visible marker (Rule 1).
    Unknown,
}

/// Resolve a raw set-bonus stat string to its [`EffectStatKey`] for the tooltip. This is the
/// display side of [`map_stat_name`], exposed so the UI shares the one stat vocabulary rather
/// than re-deriving it (the mapping lives in the data layer, not the component).
pub fn effect_stat_key(raw: &str) -> EffectStatKey {
    match map_stat_name(raw) {
        Some(StatMapping::Stat(stat)) => EffectStatKey::Tracked(stat.as_beta_key()),
        // Every type of one mez-resist-all bonus is tracked from the same effect, at the same
        // value and from the same sources, so their buckets carry identical counts and any one
        // of them reports the tier's Rule-of-5 count exactly. Naming one keeps the cue on the
        // game's most common set bonus, which `resAll`'s expansion gives up.
        Some(StatMapping::MezAll) => {
            EffectStatKey::Tracked(SetBonusStat::MezResistHold.as_beta_key())
        }
        Some(StatMapping::ResAll | StatMapping::Unspent { .. }) => EffectStatKey::KnownUntracked,
        None => EffectStatKey::Unknown,
    }
}

/// Which stats one set-bonus effect NAMES: what a surface that *describes* the catalog needs,
/// as distinct from [`calculate_set_bonuses`], which decides what a build *receives*.
///
/// The two differ in exactly one place and it's deliberate: an [`UNSPENT_STATS`] row grants
/// nothing through this pass (its value arrives on another path, and adding it here would
/// double-count) but it's still a bonus the set carries, so it names its stats. Nothing that
/// spends a value may call this, which is why the type carries no magnitude.
///
/// The expansions are the calc's own, so a search groups a bonus under the same stats the
/// totals credit it to: `resAll` to the eight resistance types, `mez_resistance_(all)` to the
/// types the effect's own [`mez_types`](coh_data::SetBonusEffect::mez_types) names, and a plain
/// stat to itself plus its [`paired`](SetBonusStat::paired) twin.
pub fn stats_named(effect: &coh_data::SetBonusEffect) -> NamedStats {
    match map_stat_name(&effect.stat) {
        None => NamedStats::Unknown,
        Some(StatMapping::Stat(stat)) => {
            let mut named = vec![stat];
            named.extend(stat.paired());
            NamedStats::Stats(named)
        }
        Some(StatMapping::ResAll) => NamedStats::Stats(ALL_RES_TYPES.to_vec()),
        Some(StatMapping::Unspent { names, .. }) => NamedStats::Stats(names.to_vec()),
        // The label says "all" and the export named no types, so which types it covers isn't
        // stated anywhere, the same gap the calc refuses to guess past (MEZRES-1). A describing
        // surface has one more option than the calc does: say so, rather than drop the row.
        Some(StatMapping::MezAll) if effect.mez_types.is_empty() => NamedStats::UntypedMezAll,
        Some(StatMapping::MezAll) => {
            let (named, unknown): (Vec<_>, Vec<_>) = effect
                .mez_types
                .iter()
                .map(|type_key| (type_key, mez_resist_stat(type_key)))
                .partition(|(_, stat)| stat.is_some());
            if let Some((type_key, _)) = unknown.first() {
                return NamedStats::UnknownMezType((*type_key).clone());
            }
            NamedStats::Stats(named.into_iter().filter_map(|(_, stat)| stat).collect())
        }
    }
}

/// What [`stats_named`] made of one effect. Every non-`Stats` variant is a fault a describing
/// surface must show rather than swallow: a set bonus silently missing from a
/// catalog search reads exactly like a set that doesn't grant it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamedStats {
    /// The stats this effect's label names, either one or an expansion.
    Stats(Vec<SetBonusStat>),
    /// A `mez_resistance_(all)` effect carrying no mez types (MEZRES-1).
    UntypedMezAll,
    /// A mez type outside the six the calc routes.
    UnknownMezType(String),
    /// A raw stat string absent from the vocabulary.
    Unknown,
}

/// The Rule-of-5 `(count, capped)` for one `(stat, value)` in a build's projected tracking,
/// the beta `getTotalBonusCount` + `isBonusCapped` in one lookup (they share the bucket). `count`
/// is every instance that fired, accepted plus rejected (`sources + rejectedSources`), so a
/// capped stat still reports its true `6/5`, `7/5`, … total; `capped` flips once the sixth was
/// rejected. Missing stat or value bucket ⇒ `(0, false)`, exactly the beta's `?? 0` / `?? false`.
pub fn bonus_tracking_lookup(
    tracking: &[SetBonusStatTracking],
    stat_key: &str,
    value: f64,
) -> (u32, bool) {
    let key = value_key(value);
    tracking
        .iter()
        .find(|stat| stat.stat_key == stat_key)
        .and_then(|stat| stat.buckets.get(&key))
        .map(|bucket| {
            let total = (bucket.sources.len() + bucket.rejected_sources.len()) as u32;
            (total, bucket.capped)
        })
        .unwrap_or((0, false))
}

/// Whether THIS instance of a bonus was the one the Rule of 5 refused.
///
/// A different question from [`bonus_tracking_lookup`]'s `capped`: `capped` is a property of the
/// BUCKET, true for every copy once a sixth exists, so it answers "is this bonus at its cap".
/// Asking it of one power's copy and reading the answer as "this power's copy grants nothing"
/// strikes out all six and claims the build gets none of them, when it gets five. This one asks
/// the bucket which instances it actually rejected.
///
/// Keyed by the same (power, set, tier) address `BonusSourceRef` carries, because that's what
/// identifies one slotting's contribution: `internal_name` alone isn't unique across sets.
pub fn bonus_instance_rejected(
    tracking: &[SetBonusStatTracking],
    stat_key: &str,
    value: f64,
    power_internal_name: &str,
    power_set: &str,
    pieces: u8,
) -> bool {
    let key = value_key(value);
    tracking
        .iter()
        .find(|stat| stat.stat_key == stat_key)
        .and_then(|stat| stat.buckets.get(&key))
        .is_some_and(|bucket| {
            bucket.rejected_sources.iter().any(|source| {
                source.power_internal_name == power_internal_name
                    && source.power_set == power_set
                    && source.pieces == pieces
            })
        })
}

/// Whether ANY Rule-of-5 bucket feeding one dashboard stat rejected a bonus, the stat-level
/// question the dashboard asks, as opposed to [`bonus_tracking_lookup`]'s per-`(stat, value)` one.
/// A stat can carry several value buckets (a +5% and a +2.5% Defense bonus track separately) and
/// only one of them need cap for the displayed total to be smaller than the build's bonuses
/// nominally grant; that's exactly what the dashboard's warning cue reports.
///
/// Keyed by BREAKDOWN key ([`SetBonusStatTracking::breakdown_keys`], the camelCase dashboard
/// vocabulary) rather than [`SetBonusStatTracking::stat_key`], because the dashboard's stat rows
/// are the breakdown map's rows, and because `+Res(Recharge Debuff)` fans one tracked stat into
/// two of them, which a `stat_key` lookup couldn't express.
pub fn stat_has_capped_bonus(tracking: &[SetBonusStatTracking], breakdown_key: &str) -> bool {
    tracking
        .iter()
        .filter(|stat| stat.breakdown_keys.iter().any(|key| key == breakdown_key))
        .flat_map(|stat| stat.buckets.values())
        .any(|bucket| bucket.capped)
}

/// The tracked stat one `mez_types` entry names, the lowercase mez-type vocabulary shared with
/// [`GlobalBonuses::add_mez_resistance`], so a set bonus and a power's own mez resistance land in
/// the same accumulator. `None` for a type key outside that vocabulary, which the caller reports
/// rather than drops (Rule 1).
fn mez_resist_stat(type_key: &str) -> Option<SetBonusStat> {
    use SetBonusStat::*;
    Some(match type_key {
        "hold" => MezResistHold,
        "stun" => MezResistStun,
        "immobilize" => MezResistImmobilize,
        "sleep" => MezResistSleep,
        "confuse" => MezResistConfuse,
        "fear" => MezResistFear,
        _ => return None,
    })
}

/// The defense vectors a `defense_(all)` label names. Every defense stat the vocabulary carries,
/// three positional and five typed, because "all" is what the label says and the proc path that
/// actually spends this bonus expands it across the same universe
/// (`procs::apply_single_proc_effect`). Read by [`stats_named`] only; the set-bonus pass spends
/// nothing here ([`UNSPENT_STATS`]).
const ALL_DEF_TYPES: [SetBonusStat; 8] = [
    SetBonusStat::DefMelee,
    SetBonusStat::DefRanged,
    SetBonusStat::DefAoE,
    SetBonusStat::DefSmashing,
    SetBonusStat::DefLethal,
    SetBonusStat::DefFire,
    SetBonusStat::DefCold,
    SetBonusStat::DefEnergy,
];

/// The eight resistance types `resAll` expands to.
const ALL_RES_TYPES: [SetBonusStat; 8] = [
    SetBonusStat::ResSmashing,
    SetBonusStat::ResLethal,
    SetBonusStat::ResFire,
    SetBonusStat::ResCold,
    SetBonusStat::ResEnergy,
    SetBonusStat::ResNegative,
    SetBonusStat::ResPsionic,
    SetBonusStat::ResToxic,
];

/// The categories immune to exemplar suppression: Purple, ATO, PvP, and Winter/Event
/// sets keep their bonuses at any exemplar level (beta `EXEMPLAR_IMMUNE_CATEGORIES`).
fn is_exemplar_immune(category: &str) -> bool {
    matches!(category, "purple" | "ato" | "pvp" | "event")
}

/// One accepted or rejected set-bonus instance in a Rule-of-5 bucket, the facts the
/// beta's source string is built from (`${set.name} (${pieces}pc in ${power.name})`).
/// The engine owns the accept/reject decision; the output mapper owns the display label,
/// resolving the power's display name from the build (the engine's
/// [`SelectedPower`](coh_data::SelectedPower) carries only `internal_name`/`power_set`)
/// and formatting the string. Keeping the raw refs here, not a pre-baked string, keeps
/// display vocabulary out of the pure calc.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BonusSourceRef {
    /// The slotting power's `internal_name`, half the key the mapper resolves to a
    /// display name (internal names aren't unique across sets, so `power_set` disambiguates).
    pub power_internal_name: String,
    /// The slotting power's set (beta `powerSet`).
    pub power_set: String,
    /// The IO set's display name (already known here from the catalog).
    pub set_name: String,
    /// The tier threshold that fired (the beta `bonus.pieces`, e.g. `4` → "4pc").
    pub pieces: u8,
}

/// One Rule-of-5 bucket for a single `(stat, value)` pair. `value` is the FIRST
/// value that opened the bucket. The beta stores it once and never updates it, and
/// the aggregate sum is `value × count` (so two raw values that round to the same
/// 2-dp key still sum off the first). `count` is capped at 5;
/// `capped` flips true once a sixth-plus identical bonus is rejected. `sources` holds
/// the `count` accepted instances; `rejected_sources` the sixth-plus (the beta
/// `sources` / `rejectedSources`), feeding the tooltip rows and the over-cap ring.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ValueBucket {
    pub value: f64,
    pub count: u32,
    pub capped: bool,
    pub sources: Vec<BonusSourceRef>,
    pub rejected_sources: Vec<BonusSourceRef>,
}

/// Rule-of-5 tracking: per stat, a map from the 2-dp value key to its bucket. Mirrors
/// the beta `BonusTracking` (`{ [stat]: { [valueKey]: ValueTracking } }`).
pub type BonusTracking = BTreeMap<SetBonusStat, BTreeMap<String, ValueBucket>>;

/// The set-bonus contribution of a build: the Rule-of-5-capped per-stat totals, the
/// tracking that produced them (for the "x/5" / capped UI and the gate), and any
/// fail-loud errors (unknown stats, unresolved set ids).
#[derive(Debug, Clone, PartialEq)]
pub struct SetBonusResult {
    /// Per-stat total, `Σ value × min(count, 5)`, the beta `AggregatedBonuses`.
    pub aggregated: BTreeMap<SetBonusStat, f64>,
    pub tracking: BonusTracking,
    pub errors: Vec<CalcError>,
}

/// The serializable projection of one tracked stat's Rule-of-5 buckets, the shape the
/// output mapper reshapes into the beta `bonusTracking` (keyed by [`stat_key`](Self::stat_key))
/// and the set-bonus `breakdown` sources (keyed by [`breakdown_keys`](Self::breakdown_keys)).
/// The internal [`BonusTracking`] is keyed by the [`SetBonusStat`] enum, which can't serialize
/// as the beta string map key, so [`tracking_out`] re-keys it here.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SetBonusStatTracking {
    /// The beta internal stat key ([`SetBonusStat::as_beta_key`]), the `bonusTracking` map key
    /// the UI looks up via `getTotalBonusCount` / `isBonusCapped`.
    pub stat_key: String,
    /// The camelCase dashboard breakdown-map key(s) ([`SetBonusStat::breakdown_keys`]).
    pub breakdown_keys: Vec<String>,
    /// The 2-dp value key → bucket map (the beta `{ [valueKey]: ValueTracking }`).
    pub buckets: BTreeMap<String, ValueBucket>,
}

/// Re-key the internal [`BonusTracking`] into the serializable per-stat projection the output
/// mapper consumes. The engine owns every accept/reject decision recorded here; the mapper only
/// reshapes and resolves display names.
pub fn tracking_out(tracking: &BonusTracking) -> Vec<SetBonusStatTracking> {
    tracking
        .iter()
        .map(|(&stat, buckets)| SetBonusStatTracking {
            stat_key: stat.as_beta_key().to_string(),
            breakdown_keys: stat
                .breakdown_keys()
                .into_iter()
                .map(String::from)
                .collect(),
            buckets: buckets.clone(),
        })
        .collect()
}

/// One set-bonus contribution as the detailed breakdown shows it: which set granted it, at which
/// tier, how much it moved the field, and whether the Rule of 5 took it.
///
/// `value` is already ROUTED, scaled the way [`apply_set_bonuses_to_global`] scales it into the
/// target field (KB protection's ×0.01), so a row means the same number the total does.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SetBonusBreakdownSource {
    pub breakdown_key: String,
    pub set_name: String,
    pub power_internal_name: String,
    pub power_set: String,
    /// The tier that granted it: 4 for a 4-piece bonus.
    pub pieces: u8,
    pub value: f64,
    /// Rejected by the Rule of 5: the sixth-plus identical bonus, which the total does NOT
    /// include. Kept as a row because a build that slotted it should be told it's paying for
    /// nothing, the same reason a superseded travel buff keeps its row.
    pub rejected: bool,
}

/// Every set-bonus row feeding `breakdown_key`, routed exactly as the totals route it.
///
/// This is the ONE derivation of "what did set bonuses contribute to this field", shared by the
/// detailed-totals modal, the set-bonus surface, and the reconciliation gate that grades them.
/// Written here rather than in the UI because it has to reproduce [`apply_set_bonuses_to_global`]'s
/// two special routings (the KB-protection ×0.01 scale and the `DebuffResistRecharge` fan-out to
/// Slow), and a second copy of that in a component is exactly the drift this crate keeps paying for.
///
/// Accepted rows carry their bucket's value; rejected ones carry it too but are flagged, so a
/// caller summing the LIVE rows reproduces the field and a caller showing all of them can still
/// explain why the build's nominal bonuses exceed its actual ones.
pub fn breakdown_sources_for_key(
    tracking: &[SetBonusStatTracking],
    breakdown_key: &str,
) -> Vec<SetBonusBreakdownSource> {
    let mut rows = Vec::new();
    for stat in tracking {
        if !stat.breakdown_keys.iter().any(|key| key == breakdown_key) {
            continue;
        }
        // The scale is the stat's, not the key's: KB protection is tracked as a percentage of
        // Mag (400 = Mag 4.0) and routed ×0.01, so its rows must be scaled to match or they'd
        // overstate the field a hundredfold.
        let scale = match SetBonusStat::from_beta_key(&stat.stat_key) {
            Some(SetBonusStat::KbProtection) => 0.01,
            _ => 1.0,
        };
        for bucket in stat.buckets.values() {
            let row = |source: &BonusSourceRef, rejected: bool| SetBonusBreakdownSource {
                breakdown_key: breakdown_key.to_string(),
                set_name: source.set_name.clone(),
                power_internal_name: source.power_internal_name.clone(),
                power_set: source.power_set.clone(),
                pieces: source.pieces,
                value: bucket.value * scale,
                rejected,
            };
            rows.extend(bucket.sources.iter().map(|source| row(source, false)));
            rows.extend(
                bucket
                    .rejected_sources
                    .iter()
                    .map(|source| row(source, true)),
            );
        }
    }
    rows
}

/// The Rule-of-5 bucket key for a value, a faithful `Number.prototype.toFixed(2)`.
/// This is the dedup granularity, NOT a display detail: two bonuses share a cap
/// bucket iff their 2-dp values match, so it must round exactly as the beta does.
///
/// Rust's `{:.2}` and JS `toFixed(2)` both round to nearest and agree on every value
/// EXCEPT an exact 2-dp halfway point, where JS rounds toward the larger integer and
/// Rust rounds to even. The only doubles that ARE exact 2-dp halfway points are odd
/// eighths (`x.125`, `x.375`, `x.625`, `x.875`), for which `value × 8` is an odd
/// integer (a power-of-two multiply, so exact). For those (all positive in set data)
/// round half-away-from-zero; everything else defers to `{:.2}`, which is already
/// exact. Multiplying by 100 to detect ties is unsafe (it turns `2.5249999…` into a
/// false `252.5`); the `× 8` exactness test is what keeps `2.525` out of the tie branch.
pub(crate) fn value_key(value: f64) -> String {
    let eighths = value * 8.0;
    let is_exact_tie = eighths.fract() == 0.0 && (eighths as i64) % 2 != 0;
    if is_exact_tie {
        // `value × 100` is exact for an eighth, so `.round()` (half away from zero)
        // reproduces JS's round-up for the positive ties that occur here.
        let hundredths = (value * 100.0).round() as i64;
        format!("{}.{:02}", hundredths / 100, (hundredths % 100).abs())
    } else {
        format!("{value:.2}")
    }
}

/// Record one emitted bonus against the Rule of 5 (the beta `trackBonus`): bucket by
/// `(stat, value.toFixed(2))`, accept the first five, reject the rest. The `source` is
/// the same for a stat and its paired/`resAll` expansions (one tier fires them together),
/// so the caller builds it once and lends it to each `track_bonus` call.
fn track_bonus(
    tracking: &mut BonusTracking,
    stat: SetBonusStat,
    value: f64,
    source: &BonusSourceRef,
) {
    let value_key = value_key(value);
    let bucket = tracking
        .entry(stat)
        .or_default()
        .entry(value_key)
        .or_insert(ValueBucket {
            value,
            count: 0,
            capped: false,
            sources: Vec::new(),
            rejected_sources: Vec::new(),
        });
    if bucket.count < 5 {
        bucket.count += 1;
        bucket.sources.push(source.clone());
    } else {
        bucket.capped = true;
        bucket.rejected_sources.push(source.clone());
    }
}

/// The effective character level for suppression: the exemplar level when
/// exemplared, else the build level. That's the beta's `exemplarLevel || buildLevel`,
/// whose falsy-`0` half [`Level`] spells as `None`.
fn effective_level(exemplar_level: Option<Level>, build_level: Level) -> Level {
    exemplar_level.unwrap_or(build_level)
}

/// Calculate a build's set bonuses with the Rule of 5 applied, the beta
/// `calculateSetBonuses`. `powers` is every selected power (the caller passes
/// [`coh_data::CharacterState::all_selected`], whose order matches the beta's
/// `buildPowers` traversal); `exemplar_level`/`build_level` drive exemplar
/// suppression (pass `state.combat.exemplar_level` and `state.level`).
pub fn calculate_set_bonuses<'a>(
    powers: impl IntoIterator<Item = &'a SelectedPower>,
    catalog: &IoSetCatalog,
    exemplar_level: Option<Level>,
    build_level: Level,
    pvp: bool,
) -> SetBonusResult {
    let effective = i64::from(effective_level(exemplar_level, build_level));
    let mut tracking: BonusTracking = BTreeMap::new();
    let mut errors: Vec<CalcError> = Vec::new();

    for power in powers {
        // Which sets are slotted in THIS power, and how many active pieces of each.
        let mut sets_in_power: BTreeMap<&str, usize> = BTreeMap::new();

        for enhancement in power.slots.iter().flatten() {
            let EnhancementKind::IoSet {
                set_id,
                piece_num: _,
                ..
            } = &enhancement.kind
            else {
                continue;
            };
            let set = catalog.get(set_id);
            let active =
                piece_bonuses_active(set, enhancement.attuned, enhancement.level, effective);
            if active {
                *sets_in_power.entry(set_id.as_str()).or_insert(0) += 1;
            }
        }

        for (set_id, piece_count) in sets_in_power {
            let Some(set) = catalog.get(set_id) else {
                // A slotted set id with no catalog entry contributes nothing (beta
                // `if (!set) return`): surfaced loud, dropped numerically (Rule 1).
                errors.push(CalcError::new(
                    "Set bonus",
                    format!(
                        "unknown set id {set_id:?} slotted in {}",
                        power.internal_name
                    ),
                ));
                continue;
            };

            for bonus in &set.bonuses {
                // A tier with no threshold never fires (beta `if (!bonus?.pieces)`).
                if bonus.pieces == 0 || piece_count < usize::from(bonus.pieces) {
                    continue;
                }
                // One source per fired tier, shared by every effect it emits (and every
                // paired / resAll expansion of those), matching the beta's per-`bonus.pieces`
                // source string.
                let source = BonusSourceRef {
                    power_internal_name: power.internal_name.clone(),
                    power_set: power.powerset.clone(),
                    set_name: set.name.clone(),
                    pieces: bonus.pieces,
                };
                for effect in &bonus.effects {
                    // A PvP tier is the `isPVPMap?` arm of the set's `Requires`, so it applies
                    // on a PvP map and nowhere else (BONUS-REQ-1). Its PvE sibling at the same
                    // piece count is a separate, unflagged effect in this same list.
                    if effect.pvp && !pvp {
                        continue;
                    }
                    match map_stat_name(&effect.stat) {
                        None => errors.push(CalcError::new(
                            "Set bonus",
                            format!("unknown stat {:?} in set {:?}", effect.stat, set.name),
                        )),
                        Some(StatMapping::Unspent { .. }) => {}
                        Some(StatMapping::ResAll) => {
                            for res in ALL_RES_TYPES {
                                track_bonus(&mut tracking, res, effect.value, &source);
                            }
                        }
                        Some(StatMapping::MezAll) if effect.mez_types.is_empty() => {
                            // The label says "all" but the export named no types, so there's
                            // nothing to spend it into and no warrant to assume the usual six
                            // (Rule 1: a visible break beats a soft-wrong number). Empty on
                            // the shipped corpus: the one tier that used to land here was a
                            // unique-piece global re-encoded as a bonus, removed at the
                            // extractor. DATA-GAP-REGISTER MEZRES-1.
                            errors.push(CalcError::new(
                                "Set bonus",
                                format!(
                                    "{:?} {}pc: mez_resistance_(all) names no mez types",
                                    set.name, bonus.pieces
                                ),
                            ));
                        }
                        Some(StatMapping::MezAll) => {
                            for type_key in &effect.mez_types {
                                match mez_resist_stat(type_key) {
                                    Some(stat) => {
                                        track_bonus(&mut tracking, stat, effect.value, &source)
                                    }
                                    None => errors.push(CalcError::new(
                                        "Set bonus",
                                        format!(
                                            "{:?} {}pc: unknown mez type {type_key:?}",
                                            set.name, bonus.pieces
                                        ),
                                    )),
                                }
                            }
                        }
                        Some(StatMapping::Stat(stat)) => {
                            track_bonus(&mut tracking, stat, effect.value, &source);
                            if let Some(paired) = stat.paired() {
                                track_bonus(&mut tracking, paired, effect.value, &source);
                            }
                        }
                    }
                }
            }
        }
    }

    let aggregated = aggregate(&tracking);
    SetBonusResult {
        aggregated,
        tracking,
        errors,
    }
}

/// Layer 2: route a build's Rule-of-5-capped set-bonus aggregate into the [`GlobalBonuses`]
/// accumulator (the beta `applySetBonusesToGlobal` + `STAT_TO_GLOBAL`). Every contribution
/// ADDS to its target field. Two stats route specially, exactly as the beta:
/// [`KbProtection`](SetBonusStat::KbProtection) is stored as a percentage of Mag (400 = Mag 4.0),
/// so it scales ×0.01 into `protKnockback`; a `+Res(Recharge Debuff)` bonus
/// ([`DebuffResistRecharge`](SetBonusStat::DebuffResistRecharge)) also grants Slow resistance (the
/// beta `PAIRED_STATS`), so it feeds BOTH `debuffResistRecharge` and `debuffResistSlow`. Every
/// other stat is a plain 1:1 add by field name, and every tracked stat HAS such a field. The
/// beta's `key && key in global` guard had one subject here, JumpSpeed, and this calc routes it.
/// The calc's fail-loud `errors` fold into the accumulator so an unknown stat surfaces through the
/// same channel.
pub fn apply_set_bonuses_to_global(global: &mut GlobalBonuses, result: &SetBonusResult) {
    for (&stat, &value) in &result.aggregated {
        match stat {
            // A +Res(Recharge Debuff) set bonus also grants Slow resistance (beta `PAIRED_STATS`
            // `debuffresistrecharge`).
            SetBonusStat::DebuffResistRecharge => {
                global.debuff_resist_recharge += value;
                global.debuff_resist_slow += value;
            }
            // IO-set KB protection is stored as a percentage of Mag (400 = Mag 4.0), so ×0.01.
            SetBonusStat::KbProtection => global.protection_knockback += value * 0.01,
            _ => {
                let field = stat.global_field();
                route_closed(global.add_by_camel_name(field, value), field);
            }
        }
    }
    global.errors.extend(result.errors.iter().cloned());
}

/// Whether a slotted piece's set bonuses are active at the effective level (beta's
/// per-slot exemplar-suppression check). Immune categories are always active; an
/// attuned piece survives down to `set.minLevel − 3`; a level-based piece down to
/// `ioLevel − 3`. A piece whose set is unknown falls back to the beta defaults
/// (minLevel 1, IO level 50); it contributes nothing anyway (its set is skipped when
/// emitting), so the fallback only avoids diverging mid-count.
fn piece_bonuses_active(
    set: Option<&coh_data::IoSet>,
    attuned: bool,
    io_level: Option<Level>,
    effective: i64,
) -> bool {
    if set
        .map(|s| is_exemplar_immune(&s.category))
        .unwrap_or(false)
    {
        return true;
    }
    if attuned {
        let set_min_level = set.map(|s| s.min_level).filter(|&l| l != 0).unwrap_or(1);
        effective >= set_min_level - 3
    } else {
        let level = io_level.map(i64::from).unwrap_or(50);
        effective >= level - 3
    }
}

/// Sum the tracking into per-stat totals, the beta `getAggregatedFromTracking`
/// (`Σ value × count`, count already Rule-of-5-capped at 5).
fn aggregate(tracking: &BonusTracking) -> BTreeMap<SetBonusStat, f64> {
    tracking
        .iter()
        .map(|(&stat, buckets)| {
            let total = buckets.values().map(|b| b.value * f64::from(b.count)).sum();
            (stat, total)
        })
        .collect()
}
