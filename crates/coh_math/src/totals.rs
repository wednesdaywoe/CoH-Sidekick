//! `GlobalBonuses`, the totals accumulator, ported (shape, not name) from the beta
//! `GlobalBonuses` (`character-totals.ts:83`). Every field is `f64`, zero-initialized; each
//! pass writes the fields it owns. The struct GROWS pass by pass (M3 steps 3–9): it carries
//! only the fields a landed pass populates, so a zero here always means "unpopulated", never
//! "populated to zero by a pass that doesn't exist yet".
//!
//! Pass 1 (strength) is the first to write it, the `strength*` fractions (Power Boost family).
//! See [`crate::strength`].

/// A gap found during recalculation: data the interpreter met but couldn't derive a value from,
/// (fail loud). Collected into [`GlobalBonuses::errors`] and surfaced by the
/// UI as a per-item error marker. It is NOT fatal: one underivable contribution must never blank
/// a whole build, nor be silently replaced by zero or a stale hardcoded constant.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalcError {
    /// The contribution that failed: the stat or mechanic a maintainer/user should look at
    /// (e.g. `"Fury"`).
    pub context: Box<str>,
    /// What was unrecognized or missing, in maintainer-readable terms: the value the interpreter
    /// couldn't derive, and why.
    pub detail: Box<str>,
}

impl CalcError {
    pub fn new(context: impl Into<Box<str>>, detail: impl Into<Box<str>>) -> Self {
        CalcError {
            context: context.into(),
            detail: detail.into(),
        }
    }
}

/// The internal totals accumulator. Projected to the display `CharacterStats` by Pass 8
/// (`finalize`, M3 step 9); consumers read the projection, not this. Field names are the
/// Rust snake_case of the beta's camelCase; [`GlobalBonuses::get`] bridges the two for the
/// totals gate, which asserts against the TS field names verbatim.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct GlobalBonuses {
    // +Strength buffs (Power Boost family), stored as FRACTIONS (e.g. 0.787 = +78.7%
    // strength), not percentages. Non-ED multipliers on the caster's OWN matching output;
    // consumed by Pass 2 apply. See the beta's field-doc at character-totals.ts:178.
    pub strength_defense: f64,
    pub strength_to_hit: f64,
    pub strength_heal: f64,
    pub strength_absorb: f64,
    pub strength_end_mod: f64,
    pub strength_movement: f64,
    pub strength_mez: f64,

    // Pass 8 (finalize, Step 9.5): the purple-patch combat projections, written onto the
    // accumulator (mirroring the beta, which stores them on `globalBonuses`, NOT on the
    // projected `CharacterStats`; character-totals.ts:4452-4454). `base_to_hit` and
    // `hit_chance` are FRACTIONS in [0.05, 0.95]; `combat_modifier` is a raw table
    // multiplier. Populated by [`crate::finalize::project_combat`] from the caster's
    // `combat.enemy_level_offset` (M3 has no level shift). Left at their zero default when
    // the purple-patch table is empty (an unloaded db). In that case a `CalcError` is recorded
    // rather than fabricating the beta's 0.75 / 1.0 default (Rule 1). See [`crate::finalize`].
    pub base_to_hit: f64,
    pub hit_chance: f64,
    pub combat_modifier: f64,
    // Pass 2a (apply): the atom-native families accumulated per active power, stored
    // as PERCENTAGES (`resolveScaledEffect × 100`), the display convention the beta
    // `GlobalBonuses` uses for these fields (character-totals.ts:88-145). See
    // [`crate::apply`].
    pub damage: f64,
    pub to_hit: f64,
    pub max_hp: f64,
    pub regeneration: f64,
    pub recovery: f64,
    // Pass 2b (apply, atom-native, ATOM1): +Recharge (Hasten, Quickness, Speed Boost). A PERCENTAGE,
    // but read via the beta's `extractScaleValue × 100` (the raw `scale`, NOT AT-table
    // resolved, since recharge buffs carry the final fraction directly); the self-directed
    // `rechargeDebuff` crash (Granite −65%) subtracts. See [`crate::apply`].
    pub recharge: f64,
    // Pass 2b (apply, atom-native, ATOM8): +Max Endurance (Physical Perfection, Superior Conditioning,
    // Call to the Depths). Absolute endurance POINTS, not a percentage: the scale IS the point
    // value (`resolveScaledEffect × enhMultiplier`, NO ×100). See [`crate::apply`].
    pub max_endurance: f64,
    // Pass 2b (apply, atom-native, ATOM9): +Accuracy (Combat Training: Offensive, Focused Accuracy,
    // Tactics-adjacent). A PERCENTAGE (`resolveScaledEffect × 100`); NOT enhanced. Distinct
    // from `to_hit`: accuracy is a multiplier on the attack roll, to_hit an additive term.
    pub accuracy: f64,
    // Pass 2b (apply, atom-native, ATOM5): +Perception radius (Tactics, Focused Accuracy, Rise to the
    // Challenge-adjacent). A PERCENTAGE (`resolveScaledEffect × 100`); NOT enhanced. Beta gates
    // on `val > 0`, so only positive +Perception aggregates here. See [`crate::apply`].
    pub perception_radius: f64,
    // Pass 2b (apply, atom-native, ATOM7): Endurance Discount / EndDisc (Conserve Power, Body Mastery).
    // A PERCENTAGE (`resolveScaledEffect × 100`); NOT enhanced. Beta gates on `discount > 0` and
    // routes to the canonical `global.endurance` EndDisc accumulator (exposed as the `endrdx`
    // stat) so toggle-cost math and the dashboard read one unified sum; the vestigial beta
    // `enduranceDiscount` field is never accumulated. See [`crate::apply`].
    pub endurance: f64,
    // Pass 9.7: endurance per second the build's ACTIVE toggles drain, and the net rate after
    // recovery pays for them (the beta `toggleEndCost` / `netEndPerSec`, its Step 9.7 +
    // the net-endurance close). Both are ABSOLUTE end/sec, not percentages, and both are
    // filled AFTER the projection, since the drain sums the projected per-power
    // `endurance_cost.final` values and those need every discount source already aggregated.
    // See [`crate::projection::toggle_endurance_total`]. There's deliberately no
    // `endurance_discount` twin: the beta field of that name is vestigial and never
    // accumulated. `endurance` above IS the EndDisc sum.
    pub toggle_end_cost: f64,
    pub net_end_per_sec: f64,
    // Pass 2b (apply, atom-native, ATOM6): +Range (Boost Range, Aim's +Range). A PERCENTAGE
    // (`resolveScaledEffect × 100`); NOT enhanced. SELF-targeted only: the same `rangeBuff` slot
    // on a Foe-targeted attack (every snipe: Blazing Bolt, Moonbeam) is the per-power Fast Snipe
    // range bump, NOT a persistent caster buff, so it must not feed this total. Beta gates the
    // resolved value on `> 0`. See [`crate::apply`].
    pub range: f64,
    // Pass 2b (apply, bag-sourced, ATOM10 residual): +Absorb shield (Frigid Protection, Reaction
    // Time, Rime, Soul Transfer). ABSOLUTE HP, not a percentage: a Heal table resolved to HP ×
    // (1 + enhBonuses.heal + strengthBuffs.absorb), boosted like healing. The MaxHP-FRACTION half
    // (Wild Bastion, `maxHPFraction` set or a `_ones` table) resolves against the final Max HP in
    // [`crate::lib`] Step 9.2: the fraction VALUE itself stays bag-read until an Expression
    // evaluator exists (ATOM10 residual). Beta gates hp > 0. See [`crate::apply`].
    pub absorb: f64,

    // Per-type +Defense (the eleven standard globals: three positions + eight damage
    // types). Keyed additively; `add_defense` routes a lowercase type name here.
    pub defense_melee: f64,
    pub defense_ranged: f64,
    pub defense_aoe: f64,
    pub defense_smashing: f64,
    pub defense_lethal: f64,
    pub defense_fire: f64,
    pub defense_cold: f64,
    pub defense_energy: f64,
    pub defense_negative: f64,
    pub defense_psionic: f64,
    pub defense_toxic: f64,

    // Per-type +Resistance (the eight standard damage types). `add_resistance` routes a
    // lowercase type name here; a self-directed −Res penalty subtracts.
    pub resistance_smashing: f64,
    pub resistance_lethal: f64,
    pub resistance_fire: f64,
    pub resistance_cold: f64,
    pub resistance_energy: f64,
    pub resistance_negative: f64,
    pub resistance_psionic: f64,
    pub resistance_toxic: f64,

    // Pass 2b (apply, atom-native, ATOM11 for the six MEZ types; knockback/knockup stay
    // bag-read, fold divergence): mez PROTECTION magnitudes (Acrobatics KB protection,
    // armor mez protection). Flat magnitudes (NOT percentages): the pool/epic `protection`
    // object adds its number directly, and the curated-armor path adds `|scale| × table[50]`.
    // Knockback and Knockup fold into `protection_knockback` (the same physical stat); repel
    // does NOT, and has its own field below. See [`crate::apply`]. [[skeptical-borrowing]]
    pub protection_hold: f64,
    pub protection_stun: f64,
    pub protection_immobilize: f64,
    pub protection_sleep: f64,
    pub protection_confuse: f64,
    pub protection_fear: f64,
    pub protection_knockback: f64,

    // Repel PROTECTION: protection against the continuous push (Repel, Hurricane), not knockback.
    // Its own field for the same reason `mez_resist_repel` is one (MEZRES-3's corpus split), so
    // the knockup fold above does not extend to it: four of the five builds that state it pair
    // `protRepel` 10 with `protKnockback` 10, but Thunderspy's Power Surge Brute pairs 10 with
    // 100, and folding would put 110 in a field the oracle says holds 100. The converter files
    // repel under `KNOCKBACK_TYPES`, so the slot is the ACCUMULATE fold `kb_protection_value`
    // reproduces, not the six-mez max fold `mez_protection_value` owns. The port took MEZRES-3
    // for mez resistance and missed it here; `totals_replay.rs` found the gap by replaying whole
    // builds, and no per-power `{scale, table}` record could have — those grade what an applier
    // RETURNS, not which total it lands in.
    pub protection_repel: f64,

    // Pass 2b (apply, atom-native, ATOM13): DEBUFF RESISTANCE, per-type (Foresight, Tactical Training,
    // Combat Training: Offensive, Mental Training). PERCENTAGES (`resolveScaledEffect × 100`),
    // AT-table resolved. Unlike the regen/recovery buffs these RESOLVE a `Res_Boolean`
    // table (the debuff-resistance boolean IS the value, not a skip signal). The bag `movement`
    // key routes to `debuff_resist_slow` (the slow/-recharge axis). `debuffResistDefense` alone
    // is boosted by the Defense enhancement (`enhBonuses.defense`, the corpus-zero slotting
    // seam, ×1.0); the rest are unenhanced. See [`crate::apply`]. [[skeptical-borrowing]]
    pub debuff_resist_slow: f64,
    pub debuff_resist_defense: f64,
    pub debuff_resist_recharge: f64,
    pub debuff_resist_endurance: f64,
    pub debuff_resist_recovery: f64,
    pub debuff_resist_to_hit: f64,
    pub debuff_resist_regeneration: f64,
    pub debuff_resist_perception: f64,
    // The last two of the family to get a total (DEBUFFRES-1). The frozen oracle has no field
    // for either, so they're the rebuild's, not a port: `debuffResMapping` never named them and
    // its `key in global` guard dropped both. Nothing about the encoding is special. `fAccuracy`
    // and `fRange` are ordinary `CharacterAttributes` members carrying a resistance face like
    // every other, and `mod_Process` resists an incoming debuff by `fMag *= (1 - fRes)` off the
    // attrib's own offset, so a stated Res(Accuracy) is applied by the game exactly as a stated
    // Res(ToHit) is. Carriers are thin but real: Brainstorm's Light Affinity states accuracy on
    // Lightfield/Spotlight, and Regeneration's Revive/Dismiss Pain state range on Homecoming and
    // Brainstorm alike. Unenhanced, like every type but `defense`.
    pub debuff_resist_accuracy: f64,
    pub debuff_resist_range: f64,

    // Per-type mez RESISTANCE (duration reduction) from the bag `mezResistance` object
    // (`character-totals.ts:1369`), a PERCENTAGE (`resolveScaledEffect × 100 × enhMult`).
    // Distinct from mez PROTECTION (`protection_*`, a magnitude that resists mez outright):
    // resistance shortens a mez that lands. Each type is enhanced by its OWN mez enhancement
    // (`1 + enh.get(type)`; ×1.0 on the unslotted corpus). The beta's `mezResMapping` routes
    // ONLY `knockback` of the KB family and has no taunt/placate entry, so its object carried
    // five keys that reached no total; this calc routes all but `knockup`, declared with its
    // evidence in [`UNSPENT_MEZ_RESISTANCE`]. `teleport` routes too, but only for the carriers
    // that protect the CASTER. The reader splits them upstream (MEZRES-3). See [`crate::apply`].
    // Atom-native: the `MezResist` Res atoms are read `?? bag`, atom==bag proven corpus-wide.
    // [[skeptical-borrowing]]
    pub mez_resist_hold: f64,
    pub mez_resist_stun: f64,
    pub mez_resist_immobilize: f64,
    pub mez_resist_sleep: f64,
    pub mez_resist_confuse: f64,
    pub mez_resist_fear: f64,
    pub mez_resist_knockback: f64,

    // Repel resistance: resistance to the continuous push (Repel, Hurricane, Detention Field),
    // NOT knockback. It rides the same `mezResistance` slot and the same KB overwrite fold as
    // `mez_resist_knockback`, but it's a separate stat and the corpus proves it: Rooted grants
    // knockback 100 / repel 10 on all three forks, Thunderspy's Unyielding grants knockback 12 /
    // repel 100, and Homecoming's Sheer Willpower grants repel with no knockback at all. That's
    // why the `knockup` warrant (always paired with knockback at an equal scale, so the knockback
    // key already carries it) does NOT extend to repel (MEZRES-3).
    pub mez_resist_repel: f64,

    // Teleport resistance: resistance to being teleported (Static Shield, Power Surge, Personal
    // Force Field, Entropic Aura). Rides the same `mezResistance` slot and the six MEZ types'
    // accumulate fold, but it's the ONE key whose carriers are filtered upstream, at the reader:
    // roughly half the corpus's `MezResist`/`Teleport` atoms are the immunity a teleported entity
    // gets so it can't be chain-yanked, and the caster must not be credited with the foe's
    // (MEZRES-3). See `appliers::mez_resistance::teleport_protects_caster`.
    pub mez_resist_teleport: f64,

    // Taunt / Placate RESISTANCE (Leadership: Assault, World of Pain, Tactical Training:
    // Assault), the beta's separate "Additional mez resistance" pair (`character-totals.ts:164`).
    // Read from the TOP-LEVEL bag slots `effects.taunt` / `effects.placate` (NOT the `taunt`/
    // `placate` keys inside a `mezResistance` object, which drop). A PERCENTAGE (`|scale| ×
    // getTableValue × 100`). Credited ONLY when the slot is an OBJECT (a bare number drops) AND
    // its table is `Res_Boolean`, i.e. taunt/placate RESISTANCE, not the taunt EFFECT (auras on
    // InherentTaunt/Ones tables, which drop here). See [`crate::apply`]. Atom-native: the
    // `Mez/Taunt|Placate` atoms are read `?? bag`, atom==bag proven on all three datasets
    // (two HC taunt-EFFECT residuals stay `?? bag`).
    pub mez_resist_taunt: f64,
    pub mez_resist_placate: f64,

    // Pass 2b (apply, atom-native): stealth RADIUS, in FEET (a distance: the two radii resolve
    // with NO `× 100`, unlike every percentage family above). Unenhanced, unstacked.
    //
    // NOT accumulated per power like the rest of Pass 2: powers sharing a binary suppress
    // group (`stackKey`, e.g. Homecoming's `StealthToggle`) do not stack (only the group's
    // largest radius applies), so a source's contribution is unknowable until every source
    // is. The apply loop collects `StealthContribution`s and [`crate::stealth`] ASSIGNS these
    // two fields once. The PvE/PvP split is per-axis: a power can win its group on one axis
    // and lose on the other. See [`crate::stealth`]. Source: the typed `Stealth/RadiusPvE`/
    // `RadiusPvP` atoms ([`crate::appliers::stealth`]), `?? bag`, atom-native since ATOM3
    // (unblocked by BRIDGE-2, which typed the two axes; ATOMIC-STATE-AUDIT ATOM3).
    // [[skeptical-borrowing]]
    pub stealth_radius_pve: f64,
    pub stealth_radius_pvp: f64,

    // Pass 6 (incarnates): the incarnate level shift (Alpha T4 / Destiny T4 / Lore T3+), a
    // non-negative integer count (0/1/2), NOT a percentage. Summed by
    // [`crate::incarnates::apply_incarnate_bonuses`]; `recalculate` nets it into the purple-patch
    // `effective_level_diff`. Zero when no incarnate contributes one.
    pub level_shift: f64,

    // The travel-buff totals (run/fly/jump speed + jump height), PERCENTAGES. Written by set
    // bonuses, the active-power movement resolve (Pass 7, [`crate::movement`]: travel suppress-group
    // max + additive, plus self-directed `slow` debuffs), procs, and incarnates (Incandescence
    // Radial Destiny's runSpeed writes all three speed axes). These are the FINAL buff-percent
    // totals and stay percentages here because the beta reads them straight and the totals gate
    // grades them. `finalize` projects them onto the class's own travel band to reach the speeds
    // the dashboard shows ([`crate::finalize::CharacterStats::run_speed`]) without touching these.
    pub run_speed: f64,
    pub fly_speed: f64,
    pub jump_speed: f64,
    pub jump_height: f64,

    // The two remaining self-directed `slow` axes, both PERCENTAGES and both written only by the
    // self-penalty path in [`crate::apply`] (no set bonus, proc or incarnate touches either), so
    // they're negative or zero on every build the corpus can produce. `movement_control` is air
    // control, how much steering you keep while airborne; `movement_friction` is ground grip.
    // Homecoming spends them on Super Speed alone (−10% each); Rebirth and Thunderspy also put
    // them on the Teleport family, alongside the `flySpeed` penalty those powers already write
    // through this same path (MOVE-1).
    pub movement_control: f64,
    pub movement_friction: f64,

    // Pass 6 (incarnates): Healing RECEIVED buff (Incandescence Destiny's Res(Heal)), a PERCENTAGE.
    // Positive = more healing received. Distinct from `strength_heal` (heal OUTPUT strength): this
    // boosts healing the caster RECEIVES, not casts.
    pub heal_received: f64,

    // M4 set-bonus (and, later, proc) targets: the `GlobalBonuses` fields the set-bonus layer-2
    // routing (`set_bonuses::apply_set_bonuses_to_global`, beta `STAT_TO_GLOBAL`) writes that no
    // M3 per-power pass owned. Zero on any build without the corresponding set bonus (or proc).
    // All PERCENTAGES.
    //
    // `mez_resist` is the beta's SCALAR mez-resist-all (`mezResist`), which the beta writes and
    // never reads. NOTHING in this calc writes it: both paths that claim an all-types mez
    // resistance (set bonuses and always-on procs) expand into the per-type `mez_resist_*` fields
    // above, from the types the export states (DATA-GAP-REGISTER MEZRES-1). The field stays
    // because it's part of the oracle's field vocabulary, which `get`/`add` bridge by name for
    // the gates; the two gates declare the expansion as their one divergence. The six
    // `*_duration` fields are OFFENSIVE control duration (they lengthen the mez YOU apply), NOT
    // the defensive `protection_*`/`mez_resist_*` families. `heal_other` is +Healing strength on
    // heals the caster casts (also read as a heal enhancement multiplier at
    // character-totals.ts:1604).
    pub mez_resist: f64,
    pub heal_other: f64,
    // OFFENSIVE knockback magnitude (how far YOU throw a foe), the one knockback field
    // here that isn't defensive; keeping it distinct from `protection_knockback` and
    // `mez_resist_knockback` is the whole point of the name (SETSTAT-1). No `effects` slot
    // carries it, so only a set bonus moves it off zero. It's not inert: `granted`'s
    // `GLOBAL_BONUS_ASPECTS` reads it under the `knockback` aspect, which scales a displayed
    // knockback DISTANCE, the same coupling the six mez `*_duration` fields have.
    pub knockback_strength: f64,
    pub immobilize_duration: f64,
    pub hold_duration: f64,
    pub stun_duration: f64,
    pub sleep_duration: f64,
    pub confuse_duration: f64,
    pub terror_duration: f64,

    /// Fail-loud channel: contributions the interpreter could NOT derive from
    /// the data, e.g. Brute Fury were a dataset's export to drop `Rage_Buff`'s magnitude
    /// expression (all three carry it today; INHERENT-2 closed). Recorded here instead of being
    /// silently defaulted to zero or a stale constant, so the UI can mark the affected stat while
    /// every derivable total still lands. Empty on a clean build. Not an f64 field, so [`Self::get`]
    /// (and the totals gate, which reads named f64 fields) ignores it.
    pub errors: Vec<CalcError>,
}

impl GlobalBonuses {
    /// Every camelCase name [`Self::get`] answers: the closed vocabulary a per-source
    /// provenance ledger walks to attribute one contributor's deltas.
    ///
    /// Written out rather than derived: the beta names aren't a mechanical transform of the
    /// Rust ones (`max_hp` is `maxHP`, `defense_melee` is `defMelee`), so `get`'s match is the
    /// only place the pairing exists. `breakdown_keys_cover_every_bonus_field` keeps this from
    /// drifting out of step with it. Add an f64 field without a key here and the count
    /// assertion turns red.
    pub const BREAKDOWN_KEYS: [&'static str; 91] = [
        "strengthDefense",
        "strengthToHit",
        "strengthHeal",
        "strengthAbsorb",
        "strengthEndMod",
        "strengthMovement",
        "strengthMez",
        "baseToHit",
        "hitChance",
        "combatModifier",
        "damage",
        "toHit",
        "maxHP",
        "regeneration",
        "recovery",
        "recharge",
        "maxEndurance",
        "accuracy",
        "perceptionRadius",
        "endurance",
        "toggleEndCost",
        "netEndPerSec",
        "range",
        "absorb",
        "defMelee",
        "defRanged",
        "defAoE",
        "defSmashing",
        "defLethal",
        "defFire",
        "defCold",
        "defEnergy",
        "defNegative",
        "defPsionic",
        "defToxic",
        "resSmashing",
        "resLethal",
        "resFire",
        "resCold",
        "resEnergy",
        "resNegative",
        "resPsionic",
        "resToxic",
        "protHold",
        "protStun",
        "protImmobilize",
        "protSleep",
        "protConfuse",
        "protFear",
        "protKnockback",
        "protRepel",
        "debuffResistSlow",
        "debuffResistDefense",
        "debuffResistRecharge",
        "debuffResistEndurance",
        "debuffResistRecovery",
        "debuffResistToHit",
        "debuffResistRegeneration",
        "debuffResistPerception",
        "debuffResistAccuracy",
        "debuffResistRange",
        "mezResistHold",
        "mezResistStun",
        "mezResistImmobilize",
        "mezResistSleep",
        "mezResistConfuse",
        "mezResistFear",
        "mezResistKnockback",
        "mezResistRepel",
        "mezResistTeleport",
        "mezResistTaunt",
        "mezResistPlacate",
        "stealthRadiusPvE",
        "stealthRadiusPvP",
        "levelShift",
        "runSpeed",
        "flySpeed",
        "jumpSpeed",
        "jumpHeight",
        "movementControl",
        "movementFriction",
        "healReceived",
        "mezResist",
        "healOther",
        "knockbackStrength",
        "immobilizeDuration",
        "holdDuration",
        "stunDuration",
        "sleepDuration",
        "confuseDuration",
        "terrorDuration",
    ];

    /// The keys whose value is NOT the sum of attributed contributions, and so can carry no
    /// per-source breakdown. Naming them lets [`Self::is_attributable`] be a closed question
    /// rather than a judgement call at each render site.
    ///
    /// Two shapes, neither of them an accumulation:
    ///
    /// - **Baselines**: `baseToHit` (0.75), `hitChance` (0.95), `combatModifier` (1.0) and the
    ///   seven `strength*` multipliers are starting values the calc reads, not totals it builds
    ///   up. A "sources" list for a baseline is a category error: nothing contributed it.
    /// - **Derived rates**: `netEndPerSec` is `((100 + maxEndurance)/60) × (1 + recovery/100) −
    ///   toggleEndCost`, a formula over three other fields. Its inputs each have their own
    ///   provenance; the rate itself has none, and per-power rows under it would be fabricated.
    ///
    /// Every other key IS an accumulation and must reconcile exactly against its ledgers.
    /// `every_attributable_key_reconciles_on_every_fork` holds that line.
    pub const UNATTRIBUTABLE_KEYS: [&'static str; 11] = [
        "strengthDefense",
        "strengthToHit",
        "strengthHeal",
        "strengthAbsorb",
        "strengthEndMod",
        "strengthMovement",
        "strengthMez",
        "baseToHit",
        "hitChance",
        "combatModifier",
        "netEndPerSec",
    ];

    /// Whether `key`'s value is the sum of contributions a ledger can attribute.
    ///
    /// The display side asks this before offering to expand a stat: a breakdown that opens onto
    /// nothing reads as "this build has no sources for it", which for a baseline is false and
    /// for a derived rate is meaningless. Unknown keys answer `false`: a key outside
    /// [`Self::BREAKDOWN_KEYS`] has no accumulator field behind it at all.
    pub fn is_attributable(key: &str) -> bool {
        Self::BREAKDOWN_KEYS.contains(&key) && !Self::UNATTRIBUTABLE_KEYS.contains(&key)
    }

    /// Every breakdown key this accumulator moved since `before`, paired with how far.
    ///
    /// The measurement behind every per-source provenance ledger (the apply walk's
    /// [`crate::apply::PowerBreakdownSource`], Pass 3's inherent rows, Pass 6's
    /// [`crate::incarnates::IncarnateBreakdownSource`]): a contributor is bracketed by a
    /// snapshot and whatever moved between them IS its contribution. Deltas are measured
    /// rather than reported because a pass that also described itself would be a second
    /// description of the same arithmetic, free to drift from the first.
    ///
    /// The compare is exact, not an epsilon: these fields are only ever accumulated onto, so
    /// an untouched one is bit-identical, and an epsilon would silently drop the smallest real
    /// contributions.
    pub fn deltas_since(&self, before: &Self) -> Vec<(&'static str, f64)> {
        Self::BREAKDOWN_KEYS
            .iter()
            .filter_map(|key| {
                let (was, now) = (before.get(key)?, self.get(key)?);
                (was != now).then_some((*key, now - was))
            })
            .collect()
    }

    /// Read a field by its BETA (camelCase) name, the bridge the totals gate uses to
    /// compare Rust output against the TS `globalBonuses` dump field-for-field. Returns
    /// `None` for a name this struct doesn't yet carry (a field owned by an M4 pass, or
    /// a typo in `M3_FIELDS`), which the gate treats as "not an M3 field".
    pub fn get(&self, beta_field: &str) -> Option<f64> {
        Some(match beta_field {
            "strengthDefense" => self.strength_defense,
            "strengthToHit" => self.strength_to_hit,
            "strengthHeal" => self.strength_heal,
            "strengthAbsorb" => self.strength_absorb,
            "strengthEndMod" => self.strength_end_mod,
            "strengthMovement" => self.strength_movement,
            "strengthMez" => self.strength_mez,

            "baseToHit" => self.base_to_hit,
            "hitChance" => self.hit_chance,
            "combatModifier" => self.combat_modifier,

            "damage" => self.damage,
            "toHit" => self.to_hit,
            "maxHP" => self.max_hp,
            "regeneration" => self.regeneration,
            "recovery" => self.recovery,
            "recharge" => self.recharge,
            "maxEndurance" => self.max_endurance,
            "accuracy" => self.accuracy,
            "perceptionRadius" => self.perception_radius,
            "endurance" => self.endurance,
            "toggleEndCost" => self.toggle_end_cost,
            "netEndPerSec" => self.net_end_per_sec,
            "range" => self.range,
            "absorb" => self.absorb,

            "defMelee" => self.defense_melee,
            "defRanged" => self.defense_ranged,
            "defAoE" => self.defense_aoe,
            "defSmashing" => self.defense_smashing,
            "defLethal" => self.defense_lethal,
            "defFire" => self.defense_fire,
            "defCold" => self.defense_cold,
            "defEnergy" => self.defense_energy,
            "defNegative" => self.defense_negative,
            "defPsionic" => self.defense_psionic,
            "defToxic" => self.defense_toxic,

            "resSmashing" => self.resistance_smashing,
            "resLethal" => self.resistance_lethal,
            "resFire" => self.resistance_fire,
            "resCold" => self.resistance_cold,
            "resEnergy" => self.resistance_energy,
            "resNegative" => self.resistance_negative,
            "resPsionic" => self.resistance_psionic,
            "resToxic" => self.resistance_toxic,

            "protHold" => self.protection_hold,
            "protStun" => self.protection_stun,
            "protImmobilize" => self.protection_immobilize,
            "protSleep" => self.protection_sleep,
            "protConfuse" => self.protection_confuse,
            "protFear" => self.protection_fear,
            "protKnockback" => self.protection_knockback,
            "protRepel" => self.protection_repel,

            "debuffResistSlow" => self.debuff_resist_slow,
            "debuffResistDefense" => self.debuff_resist_defense,
            "debuffResistRecharge" => self.debuff_resist_recharge,
            "debuffResistEndurance" => self.debuff_resist_endurance,
            "debuffResistRecovery" => self.debuff_resist_recovery,
            "debuffResistToHit" => self.debuff_resist_to_hit,
            "debuffResistRegeneration" => self.debuff_resist_regeneration,
            "debuffResistPerception" => self.debuff_resist_perception,
            "debuffResistAccuracy" => self.debuff_resist_accuracy,
            "debuffResistRange" => self.debuff_resist_range,

            "mezResistHold" => self.mez_resist_hold,
            "mezResistStun" => self.mez_resist_stun,
            "mezResistImmobilize" => self.mez_resist_immobilize,
            "mezResistSleep" => self.mez_resist_sleep,
            "mezResistConfuse" => self.mez_resist_confuse,
            "mezResistFear" => self.mez_resist_fear,
            "mezResistKnockback" => self.mez_resist_knockback,
            "mezResistRepel" => self.mez_resist_repel,
            "mezResistTeleport" => self.mez_resist_teleport,

            "mezResistTaunt" => self.mez_resist_taunt,
            "mezResistPlacate" => self.mez_resist_placate,
            "stealthRadiusPvE" => self.stealth_radius_pve,
            "stealthRadiusPvP" => self.stealth_radius_pvp,

            "levelShift" => self.level_shift,
            "runSpeed" => self.run_speed,
            "flySpeed" => self.fly_speed,
            "jumpSpeed" => self.jump_speed,
            "jumpHeight" => self.jump_height,
            "movementControl" => self.movement_control,
            "movementFriction" => self.movement_friction,
            "healReceived" => self.heal_received,

            "mezResist" => self.mez_resist,
            "healOther" => self.heal_other,
            "knockbackStrength" => self.knockback_strength,
            "immobilizeDuration" => self.immobilize_duration,
            "holdDuration" => self.hold_duration,
            "stunDuration" => self.stun_duration,
            "sleepDuration" => self.sleep_duration,
            "confuseDuration" => self.confuse_duration,
            "terrorDuration" => self.terror_duration,

            _ => return None,
        })
    }

    /// Add `value` to the field named by a BETA (camelCase) name, the mutable twin of
    /// [`Self::get`]. Ports the beta's generic `if (key in global) global[key] += value`
    /// (`applyHybridStatBlock`'s fallback), and is the loosest coupling in the calc: it routes by
    /// arbitrary field NAME, so it's the one place any stat key the export grows can arrive.
    ///
    /// Returns the same [`TypeRoute`] verdict the per-type `add_*` family does. It used to
    /// return a bare `bool` that all nine call sites discarded, which made an unrecognized name
    /// indistinguishable from a routed one, the `_ => {}` of MEZRES-2 wearing a return type.
    /// Declares no unspent names: the corpus feeds it nothing it can't place (measured across
    /// incarnate stat blocks, set-bonus fields, self-slow axes, buff-pet auras and the movement
    /// axes, all three datasets), so anything unrecognized is [`TypeRoute::Unknown`].
    pub fn add_by_camel_name(&mut self, beta_field: &str, value: f64) -> TypeRoute {
        match beta_field {
            "strengthDefense" => self.strength_defense += value,
            "strengthToHit" => self.strength_to_hit += value,
            "strengthHeal" => self.strength_heal += value,
            "strengthAbsorb" => self.strength_absorb += value,
            "strengthEndMod" => self.strength_end_mod += value,
            "strengthMovement" => self.strength_movement += value,
            "strengthMez" => self.strength_mez += value,

            "damage" => self.damage += value,
            "toHit" => self.to_hit += value,
            "maxHP" => self.max_hp += value,
            "regeneration" => self.regeneration += value,
            "recovery" => self.recovery += value,
            "recharge" => self.recharge += value,
            "maxEndurance" => self.max_endurance += value,
            "accuracy" => self.accuracy += value,
            "perceptionRadius" => self.perception_radius += value,
            "endurance" => self.endurance += value,
            "range" => self.range += value,
            "absorb" => self.absorb += value,

            "defMelee" => self.defense_melee += value,
            "defRanged" => self.defense_ranged += value,
            "defAoE" => self.defense_aoe += value,
            "defSmashing" => self.defense_smashing += value,
            "defLethal" => self.defense_lethal += value,
            "defFire" => self.defense_fire += value,
            "defCold" => self.defense_cold += value,
            "defEnergy" => self.defense_energy += value,
            "defNegative" => self.defense_negative += value,
            "defPsionic" => self.defense_psionic += value,
            "defToxic" => self.defense_toxic += value,

            "resSmashing" => self.resistance_smashing += value,
            "resLethal" => self.resistance_lethal += value,
            "resFire" => self.resistance_fire += value,
            "resCold" => self.resistance_cold += value,
            "resEnergy" => self.resistance_energy += value,
            "resNegative" => self.resistance_negative += value,
            "resPsionic" => self.resistance_psionic += value,
            "resToxic" => self.resistance_toxic += value,

            "protHold" => self.protection_hold += value,
            "protStun" => self.protection_stun += value,
            "protImmobilize" => self.protection_immobilize += value,
            "protSleep" => self.protection_sleep += value,
            "protConfuse" => self.protection_confuse += value,
            "protFear" => self.protection_fear += value,
            "protKnockback" => self.protection_knockback += value,
            "protRepel" => self.protection_repel += value,

            "debuffResistSlow" => self.debuff_resist_slow += value,
            "debuffResistDefense" => self.debuff_resist_defense += value,
            "debuffResistRecharge" => self.debuff_resist_recharge += value,
            "debuffResistEndurance" => self.debuff_resist_endurance += value,
            "debuffResistRecovery" => self.debuff_resist_recovery += value,
            "debuffResistToHit" => self.debuff_resist_to_hit += value,
            "debuffResistRegeneration" => self.debuff_resist_regeneration += value,
            "debuffResistPerception" => self.debuff_resist_perception += value,
            "debuffResistAccuracy" => self.debuff_resist_accuracy += value,
            "debuffResistRange" => self.debuff_resist_range += value,

            "mezResistHold" => self.mez_resist_hold += value,
            "mezResistStun" => self.mez_resist_stun += value,
            "mezResistImmobilize" => self.mez_resist_immobilize += value,
            "mezResistSleep" => self.mez_resist_sleep += value,
            "mezResistConfuse" => self.mez_resist_confuse += value,
            "mezResistFear" => self.mez_resist_fear += value,
            "mezResistKnockback" => self.mez_resist_knockback += value,
            "mezResistRepel" => self.mez_resist_repel += value,
            "mezResistTeleport" => self.mez_resist_teleport += value,

            "mezResistTaunt" => self.mez_resist_taunt += value,
            "mezResistPlacate" => self.mez_resist_placate += value,

            "levelShift" => self.level_shift += value,
            "runSpeed" => self.run_speed += value,
            "flySpeed" => self.fly_speed += value,
            "jumpSpeed" => self.jump_speed += value,
            "jumpHeight" => self.jump_height += value,
            "movementControl" => self.movement_control += value,
            "movementFriction" => self.movement_friction += value,
            "healReceived" => self.heal_received += value,

            "mezResist" => self.mez_resist += value,
            "healOther" => self.heal_other += value,
            "knockbackStrength" => self.knockback_strength += value,
            "immobilizeDuration" => self.immobilize_duration += value,
            "holdDuration" => self.hold_duration += value,
            "stunDuration" => self.stun_duration += value,
            "sleepDuration" => self.sleep_duration += value,
            "confuseDuration" => self.confuse_duration += value,
            "terrorDuration" => self.terror_duration += value,

            _ => return TypeRoute::Unknown,
        }
        TypeRoute::Routed
    }

    /// Add `value` to the `defense<Type>` field named by a LOWERCASE defense type (the key
    /// the atom appliers emit: `"melee"`, `"aoe"`, `"smashing"`, …). The eleven standard
    /// globals; the corpus feeds this router nothing else (measured, all three datasets), so
    /// it declares no unspent keys and anything unrecognized is [`TypeRoute::Unknown`].
    pub fn add_defense(&mut self, type_lowercase: &str, value: f64) -> TypeRoute {
        match type_lowercase {
            "melee" => self.defense_melee += value,
            "ranged" => self.defense_ranged += value,
            "aoe" => self.defense_aoe += value,
            "smashing" => self.defense_smashing += value,
            "lethal" => self.defense_lethal += value,
            "fire" => self.defense_fire += value,
            "cold" => self.defense_cold += value,
            "energy" => self.defense_energy += value,
            "negative" => self.defense_negative += value,
            "psionic" => self.defense_psionic += value,
            "toxic" => self.defense_toxic += value,
            _ => return TypeRoute::Unknown,
        }
        TypeRoute::Routed
    }

    /// Add `value` to the `resistance<Type>` field named by a LOWERCASE resistance type, the
    /// eight standard damage types. Declares no unspent keys, for the same measured reason as
    /// [`Self::add_defense`].
    pub fn add_resistance(&mut self, type_lowercase: &str, value: f64) -> TypeRoute {
        match type_lowercase {
            "smashing" => self.resistance_smashing += value,
            "lethal" => self.resistance_lethal += value,
            "fire" => self.resistance_fire += value,
            "cold" => self.resistance_cold += value,
            "energy" => self.resistance_energy += value,
            "negative" => self.resistance_negative += value,
            "psionic" => self.resistance_psionic += value,
            "toxic" => self.resistance_toxic += value,
            _ => return TypeRoute::Unknown,
        }
        TypeRoute::Routed
    }

    /// The accumulated resistance to a LOWERCASE damage type, the read twin of
    /// [`Self::add_resistance`]. `None` for a name that isn't one of the eight standard
    /// types, the same drop-unknown set the adder uses.
    pub fn resistance_of(&self, type_lowercase: &str) -> Option<f64> {
        Some(match type_lowercase {
            "smashing" => self.resistance_smashing,
            "lethal" => self.resistance_lethal,
            "fire" => self.resistance_fire,
            "cold" => self.resistance_cold,
            "energy" => self.resistance_energy,
            "negative" => self.resistance_negative,
            "psionic" => self.resistance_psionic,
            "toxic" => self.resistance_toxic,
            _ => return None,
        })
    }

    /// Add `mag` to the mez-protection field named by a LOWERCASE mez type. Ports the beta's
    /// `protMapping` (`character-totals.ts:1771`): both `knockback` and `knockup` route to
    /// `protection_knockback`. They're the same physical stat (a power grants the pair at equal
    /// magnitude), and the apply loop already folds a single power's pair to `max` before
    /// calling this. Declares no unspent keys (measured, all three datasets).
    pub fn add_mez_protection(&mut self, type_lowercase: &str, magnitude: f64) -> TypeRoute {
        match type_lowercase {
            "hold" => self.protection_hold += magnitude,
            "stun" => self.protection_stun += magnitude,
            "immobilize" => self.protection_immobilize += magnitude,
            "sleep" => self.protection_sleep += magnitude,
            "confuse" => self.protection_confuse += magnitude,
            "fear" => self.protection_fear += magnitude,
            "knockback" | "knockup" => self.protection_knockback += magnitude,
            // Repel is not knockback's pair, so it does not join the fold above. See the field.
            "repel" => self.protection_repel += magnitude,
            _ => return TypeRoute::Unknown,
        }
        TypeRoute::Routed
    }

    /// Add `value` to the debuff-resistance field named by a LOWERCASE bag type. Ports the
    /// beta's `debuffResMapping` (`character-totals.ts:1326`): the bag `movement` key routes
    /// to `debuff_resist_slow` (the −slow/−recharge axis is displayed as "Slow Resistance").
    ///
    /// Declares no unspent keys, and since DEBUFFRES-1 has none left to declare: `accuracy` and
    /// `range` were the two bag keys that fell through here, and both now have a field. See the
    /// struct fields for why the game applies them.
    pub fn add_debuff_resistance(&mut self, type_lowercase: &str, value: f64) -> TypeRoute {
        match type_lowercase {
            "movement" => self.debuff_resist_slow += value,
            "defense" => self.debuff_resist_defense += value,
            "recharge" => self.debuff_resist_recharge += value,
            "endurance" => self.debuff_resist_endurance += value,
            "recovery" => self.debuff_resist_recovery += value,
            "tohit" => self.debuff_resist_to_hit += value,
            "regeneration" => self.debuff_resist_regeneration += value,
            "perception" => self.debuff_resist_perception += value,
            "accuracy" => self.debuff_resist_accuracy += value,
            "range" => self.debuff_resist_range += value,
            _ => return TypeRoute::Unknown,
        }
        TypeRoute::Routed
    }

    /// Add `value` to the mez-RESISTANCE field named by a LOWERCASE mez type. Ports the beta's
    /// `mezResMapping` (`character-totals.ts:1370`), plus `taunt`/`placate`, which the beta's
    /// map lacked, so its `key && key in global` guard dropped them (MEZRES-2). The three keys
    /// that still do not reach a total are declared in [`UNSPENT_MEZ_RESISTANCE`].
    pub fn add_mez_resistance(&mut self, type_lowercase: &str, value: f64) -> TypeRoute {
        match type_lowercase {
            "hold" => self.mez_resist_hold += value,
            "stun" => self.mez_resist_stun += value,
            "immobilize" => self.mez_resist_immobilize += value,
            "sleep" => self.mez_resist_sleep += value,
            "confuse" => self.mez_resist_confuse += value,
            "fear" => self.mez_resist_fear += value,
            "knockback" => self.mez_resist_knockback += value,
            // Repel is its own stat, not knockback's pair. The corpus separates the two on every
            // fork (Rooted grants knockback 100 / repel 10, Thunderspy's Unyielding knockback 12
            // / repel 100, Sheer Willpower repel alone), so the knockup warrant below doesn't
            // reach it and routing here adds a value nothing else carries (MEZRES-3).
            "repel" => self.mez_resist_repel += value,
            // Taunt/placate resistance reaches this router from the `mezResistance` slot, the
            // ONLY encoding the Leadership pool's Assault has, along with the VEAT Tactical
            // Training pair and the Sheer Willpower accolade. The `effects.taunt|placate` slot
            // the ATOM12 path reads is a DIFFERENT encoding carried by a disjoint set of powers
            // (measured: 0 carry both, all three datasets), so routing here adds the missing
            // half rather than double-counting the credited one.
            "taunt" => self.mez_resist_taunt += value,
            "placate" => self.mez_resist_placate += value,
            // Teleport arrives already filtered: the reader drops the carriers whose protection
            // belongs to the entity the power moved, because only there is the atom's target
            // still visible (MEZRES-3). Every key that reaches this arm is caster-facing, so
            // there's nothing left to decide here.
            "teleport" => self.mez_resist_teleport += value,
            _ => return unspent_or_unknown(UNSPENT_MEZ_RESISTANCE, type_lowercase),
        }
        TypeRoute::Routed
    }
}

/// What a per-type router did with one type key. Every key resolves to exactly one of these
/// (the five `add_*` routers have no fallthrough left), so a key the data grows becomes a
/// visible error rather than a contribution that quietly never arrives.
///
/// The `_ => {}` this replaces hid MEZRES-2: the Leadership pool's Assault grants taunt
/// and placate resistance, the router dropped both, and every gate stayed green because the
/// frozen oracle dropped them too. That's the FLAGS-2 shape, where a drop shared by both engines
/// is invisible to a comparison between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "an unhandled route is exactly the silent drop this type exists to prevent"]
pub enum TypeRoute {
    /// Added to a [`GlobalBonuses`] field.
    Routed,
    /// Recognized and deliberately not added; carries why.
    Unspent(&'static str),
    /// No arm claimed it.
    Unknown,
}

/// Mez-resistance keys the corpus carries that deliberately do not reach a total, each with the
/// evidence for why. Declared rather than dropped: the corpus sweep guard asserts the live path
/// produces exactly this set, so both a new key AND a reason that's gone stale turn a test
/// red instead of vanishing.
const UNSPENT_MEZ_RESISTANCE: &[(&str, &str)] = &[
    // Every corpus power granting knockup resistance grants knockback resistance in the same
    // slot at an identical scale (74/74 across the three datasets, guarded by the sweep), so
    // the knockback key already carries the pair's value and adding knockup would double it.
    // The same physical-stat pairing `add_mez_protection` folds with `max`.
    (
        "knockup",
        "granted only paired with knockback at equal scale — the knockback key carries it",
    ),
];

/// A key no router arm claimed: the declared reason if the table names it, else
/// [`TypeRoute::Unknown`].
fn unspent_or_unknown(declared: &[(&str, &'static str)], key: &str) -> TypeRoute {
    match declared
        .iter()
        .find(|(declared_key, _)| *declared_key == key)
    {
        Some((_, why)) => TypeRoute::Unspent(why),
        None => TypeRoute::Unknown,
    }
}

/// Consume the verdict for a key that came from a CLOSED list in this crate rather than from
/// data. The list is itself the proof the key routes, so a miss is a typo in the list, not a
/// data gap a player could act on. Assert instead of surfacing it.
#[track_caller]
pub fn route_closed(route: TypeRoute, key: &str) {
    debug_assert_eq!(
        route,
        TypeRoute::Routed,
        "closed-list key {key:?} did not route"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The guard [`GlobalBonuses::BREAKDOWN_KEYS`]'s doc has named since it was written, and which
    /// did not exist until `protection_repel` was added and nothing went red.
    ///
    /// The Rust field names and the beta camelCase names are paired in exactly two hand-written
    /// places — that list and [`GlobalBonuses::get`]'s match — because the pairing is not a
    /// mechanical transform (`max_hp` is `maxHP`, `defense_melee` is `defMelee`). Nothing in the
    /// type system holds them together, so an f64 field added with no key is invisible: it
    /// accumulates, and every reader that walks the key list — the provenance ledgers, the
    /// what-if control list, the totals replay's whole-struct comparison — silently skips it.
    ///
    /// The field COUNT is measured by serializing rather than by reading the struct, so adding a
    /// field is enough to turn it red; no list has to be kept here too. `errors` is the one
    /// non-numeric member and drops out of the count on its own.
    #[test]
    fn breakdown_keys_cover_every_bonus_field() {
        let g = GlobalBonuses::default();
        let serde_json::Value::Object(map) =
            serde_json::to_value(&g).expect("GlobalBonuses serializes")
        else {
            panic!("GlobalBonuses did not serialize as an object");
        };
        let numeric = map.values().filter(|v| v.is_number()).count();
        assert_eq!(
            numeric,
            GlobalBonuses::BREAKDOWN_KEYS.len(),
            "GlobalBonuses carries {numeric} f64 fields and BREAKDOWN_KEYS names {}. A field with \
             no key is accumulated and then skipped by every reader that walks the list.",
            GlobalBonuses::BREAKDOWN_KEYS.len(),
        );
        let unread: Vec<&str> = GlobalBonuses::BREAKDOWN_KEYS
            .into_iter()
            .filter(|key| g.get(key).is_none())
            .collect();
        assert!(
            unread.is_empty(),
            "BREAKDOWN_KEYS names {unread:?}, which GlobalBonuses::get cannot answer. A key with \
             no arm reads as a field this struct does not carry."
        );
    }
}
