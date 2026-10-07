//! Pass 8 — caps + projection. Ports the beta's Step 9.5 (purple-patch combat projection,
//! `character-totals.ts:4446-4454`) and `convertToCharacterStats`
//! (`character-totals.ts:3724-3776`): the raw [`GlobalBonuses`] accumulator becomes the
//! projected, capped [`CharacterStats`] every consumer reads.
//!
//! Two beta facts shape this port:
//!
//! 1. **`convertToCharacterStats` is a pure projector** — straight copies plus the
//!    combine-by-`max` of the S/L, F/C, E/N pairs, with NO caps, NO base offsets, NO
//!    scaling. It is faithfully reproducible field-for-field, so the totals gate grades it.
//! 2. **Caps clamp at the display layer in the beta, not in the projection** (resistance /
//!    HP / damage at three scattered sites). D4 folds them into `finalize` so
//!    [`CharacterStats`] is the single capped truth — a DELIBERATE deviation. It is graded
//!    by the unit tests here, not the totals gate, and the totals gate stays a
//!    faithful-reproduction check because the synthetic corpus stays under the binding caps
//!    (the one cap that would bind, the defense softcap, is a THRESHOLD not a clamp — see
//!    below). That is measured, not assumed: the 98 synthetic builds reach 400%
//!    regeneration, 250% recovery, +40pp ToHit and 47.5% melee defense against ceilings of
//!    +1900%, +400%, +125.35pp and 175%, so every clamped stat is exercised and none binds.
//!
//! **The defense softcap is a threshold, not a clamp.** It is the defense value that reaches
//! ~95% avoidance vs a given-level enemy — a reference line defense legitimately exceeds
//! (the Homecoming synthetic corpus reaches 77.5% against a 45% softcap). The game never
//! clamps defense *to it*; the beta renders it cosmetically. So the softcap is exposed as a
//! separate [`CharacterStats::defense_softcap`] value for the UI to compare against, and it
//! clamps nothing.
//!
//! Defense does have a real clamp, three to five times higher up:
//! `CLAMP_CUR(fDefenseType[..])` against the class's own per-level row
//! (`ArchetypeCaps::defense_ceiling_table`, DATA-GAP-REGISTER CAPS-1). It binds at ~175-225%
//! rather than at 45%, so it never contradicts the softcap and never moves a real build's
//! number — but it exists, and this module used to state that it did not.
//!
//! The genuine ceilings that clamp here: resistance, the absolute-HP cap, absorb, the travel
//! axes, and — since CAPS-1 — ToHit, regeneration, recovery, defense and the absolute
//! endurance pool. Regeneration and recovery are the two a real build reaches without any
//! what-if help, so those two were live wrong numbers before they were clamped. The
//! damage-strength cap is per-power (beta `damage.ts:648`), not a clamp on the global
//! damage-buff total, so it is NOT applied here; [`crate::damage`] binds it per power (RB5).

use crate::movement::{self, MovementCapBumps, MovementStat, ProjectedMovement};
use crate::purple_patch::{self, ContentMode};
use crate::totals::{CalcError, GlobalBonuses};
use coh_data::{ArchetypeCaps, PurplePatch};

/// The projected, capped display stats — the beta `CharacterStats`
/// (`src/utils/calculations/stats.ts:14`) in Rust snake_case, plus the two finalize-only
/// capped-truth fields D4 adds ([`Self::max_hp_absolute`], [`Self::defense_softcap`]). Field
/// names bridge to the beta camelCase via [`Self::get`] for the totals gate.
///
/// Value conventions match the beta: buff totals are PERCENTAGES (`damage`, the `def*` /
/// `res*` / `debuff_resist*` fields), `to_hit` is the ToHit BUFF only (base 75 added at
/// display), and `max_hp` is the HP BUFF PERCENT (base HP applied at display) — the absolute,
/// capped HP is the separate [`Self::max_hp_absolute`].
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CharacterStats {
    // Offense (straight copies; note the beta renames `toHit`→`tohit`, `endurance`→`endrdx`).
    pub damage: f64,
    pub accuracy: f64,
    pub to_hit: f64,
    pub recharge: f64,
    pub endurance_reduction: f64,

    // Defense positional (straight copies). UNCAPPED — the softcap is a threshold, not a clamp.
    pub def_melee: f64,
    pub def_ranged: f64,
    pub def_aoe: f64,

    // Defense typed — combine the S/L, F/C, E/N pairs by `max`; Psionic/Toxic unpaired.
    // UNCAPPED (softcap is a threshold).
    pub def_sl: f64,
    pub def_fc: f64,
    pub def_en: f64,
    pub def_psionic: f64,
    pub def_toxic: f64,
    /// The six paired types one by one, under the same floor and ceiling as the pairs above. The
    /// pairs are what the beta projected and what the totals gate grades; these are what a
    /// reader needs whenever the two halves differ — Fiery Aura resists far more Fire than Cold,
    /// and `max` reports only the Fire.
    pub def_smashing: f64,
    pub def_lethal: f64,
    pub def_fire: f64,
    pub def_cold: f64,
    pub def_energy: f64,
    pub def_negative: f64,

    // Resistance typed — same combine-by-`max`, THEN clamped to the AT resistance cap (a real
    // ceiling). The clamp does not bind on the synthetic corpus (max 42.5% < 75/90% cap), so
    // these still equal the beta's uncapped projection at the gate; the binding case is
    // unit-tested here.
    pub res_sl: f64,
    pub res_fc: f64,
    pub res_en: f64,
    pub res_psionic: f64,
    pub res_toxic: f64,
    /// The six paired types one by one, clamped to the same AT cap — see [`Self::def_smashing`].
    pub res_smashing: f64,
    pub res_lethal: f64,
    pub res_fire: f64,
    pub res_cold: f64,
    pub res_energy: f64,
    pub res_negative: f64,

    // Recovery & HP.
    pub recovery: f64,
    pub regeneration: f64,
    /// HP BUFF PERCENT (`= global.maxHP`), matching the beta `stats.maxhp`. The absolute
    /// capped HP is [`Self::max_hp_absolute`].
    pub max_hp: f64,
    pub max_end: f64,

    // Debuff resistance (straight copies).
    pub debuff_resist_slow: f64,
    pub debuff_resist_defense: f64,
    pub debuff_resist_recharge: f64,
    pub debuff_resist_endurance: f64,
    pub debuff_resist_recovery: f64,
    pub debuff_resist_to_hit: f64,
    pub debuff_resist_regeneration: f64,
    pub debuff_resist_perception: f64,

    // ---- finalize-only (D4 capped truth; NOT beta `CharacterStats` fields, so NOT
    // gate-graded — unit-tested here) ----
    /// Absolute capped HP: `min(baseHP·(1 + maxHP/100), hpCap_at_level)`. The single
    /// capped-HP truth (D4). The beta computes this at the display layer
    /// (`stat-definitions.ts:481`); folded here so every consumer reads one number.
    pub max_hp_absolute: f64,
    /// Defense softcap THRESHOLD (percent) for the caster's combat context — the reference
    /// value the UI compares `def*` against, NOT a clamp on defense. From the purple patch
    /// (`get_defense_softcap`).
    pub defense_softcap: f64,
    /// The archetype resistance ceiling (PERCENT, e.g. 75.0) the `res_*` fields above were
    /// clamped to. The clamp is applied here, so a clamped value is indistinguishable from an
    /// unclamped one by the time the UI reads it — the ceiling has to travel with the number
    /// for the dashboard to mark a resistance as capped. `0.0` when the db carries no caps for
    /// the archetype (the same no-caps branch that leaves `res_*` raw), which reads as
    /// "unknown", never as "capped at zero".
    pub resistance_cap: f64,
    /// The per-level absolute HP ceiling [`Self::max_hp_absolute`] was clamped to, carried for
    /// the same reason as [`Self::resistance_cap`]. `0.0` when uncapped or when the db carries
    /// no caps.
    pub max_hp_cap: f64,
    /// Absorb shield in ABSOLUTE HP, clamped to [`Self::absorb_cap`] — the number to display.
    /// The raw, unclamped sum stays on the accumulator (`GlobalBonuses::absorb`), which the
    /// totals gate grades against the beta's uncapped `effects.absorb`.
    pub absorb: f64,
    /// The per-level absorb ceiling [`Self::absorb`] was clamped to, carried for the same
    /// reason as [`Self::resistance_cap`]. `0.0` when the db carries no caps.
    pub absorb_cap: f64,
    /// The ToHit BUFF ceiling (percentage points) [`Self::to_hit`] was clamped to — the
    /// class's per-level `CLAMP_CUR(fToHit)` row minus its base ToHit. Carried for the same
    /// reason as [`Self::resistance_cap`]: the clamp is applied here, so the ceiling has to
    /// travel with the number for a surface to mark it capped. `0.0` reads as "unknown".
    pub to_hit_cap: f64,
    /// The regeneration BUFF ceiling (percent over base) [`Self::regeneration`] was clamped
    /// to. +2900% for a Scrapper, i.e. the published 3000% total.
    pub regeneration_cap: f64,
    /// The recovery BUFF ceiling (percent over base) [`Self::recovery`] was clamped to.
    pub recovery_cap: f64,
    /// The real defense ceiling (percent) the `def_*` fields were clamped to — NOT
    /// [`Self::defense_softcap`], which is a threshold and clamps nothing. A surface marking
    /// defense "capped" wants the softcap; a surface asking why the number stopped climbing
    /// wants this.
    pub defense_ceiling: f64,
    /// Absolute endurance pool: `min(basePool + maxEndBuffPoints, maxEndCap_at_level)`. The
    /// [`Self::max_hp_absolute`] of endurance, and for the same reason — the buff is in
    /// POINTS, so the pool it produces is the only place a ceiling can bind.
    pub max_endurance_absolute: f64,
    /// The per-level endurance-pool ceiling [`Self::max_endurance_absolute`] was clamped to.
    pub max_endurance_cap: f64,
    /// The travel speeds the build actually reaches, in the units the game shows them in — mph
    /// for the three speeds, feet for jump height — each already clamped to its ceiling and
    /// carrying that ceiling, for the same reason [`Self::resistance_cap`] travels with `res_*`.
    /// The buff PERCENTAGES they were projected from stay on the accumulator
    /// (`GlobalBonuses::run_speed` …), which is what the totals gate grades.
    pub run_speed: ProjectedMovement,
    pub fly_speed: ProjectedMovement,
    pub jump_speed: ProjectedMovement,
    pub jump_height: ProjectedMovement,
}

impl CharacterStats {
    /// Read a projected field by its BETA (camelCase) name — the bridge the totals gate uses
    /// to compare against the TS `stats` dump. Only the beta `CharacterStats` fields are
    /// exposed; the finalize-only fields ([`Self::max_hp_absolute`], [`Self::defense_softcap`])
    /// have no beta counterpart and are covered by unit tests, not the gate. The movement
    /// projections are absent for a different reason — they are pairs, not scalars, and what the
    /// beta's `stats` dump carries for movement is the buff percentage, which lives on
    /// [`GlobalBonuses`]. Returns `None` for an unknown name.
    pub fn get(&self, beta_field: &str) -> Option<f64> {
        Some(match beta_field {
            "damage" => self.damage,
            "accuracy" => self.accuracy,
            "tohit" => self.to_hit,
            "recharge" => self.recharge,
            "endrdx" => self.endurance_reduction,

            "defMelee" => self.def_melee,
            "defRanged" => self.def_ranged,
            "defAoE" => self.def_aoe,
            "defSL" => self.def_sl,
            "defFC" => self.def_fc,
            "defEN" => self.def_en,
            "defPsionic" => self.def_psionic,
            "defToxic" => self.def_toxic,

            "resSL" => self.res_sl,
            "resFC" => self.res_fc,
            "resEN" => self.res_en,
            "resPsionic" => self.res_psionic,
            "resToxic" => self.res_toxic,

            "recovery" => self.recovery,
            "regeneration" => self.regeneration,
            "maxhp" => self.max_hp,
            "maxend" => self.max_end,

            "debuffResistSlow" => self.debuff_resist_slow,
            "debuffResistDefense" => self.debuff_resist_defense,
            "debuffResistRecharge" => self.debuff_resist_recharge,
            "debuffResistEndurance" => self.debuff_resist_endurance,
            "debuffResistRecovery" => self.debuff_resist_recovery,
            "debuffResistToHit" => self.debuff_resist_to_hit,
            "debuffResistRegeneration" => self.debuff_resist_regeneration,
            "debuffResistPerception" => self.debuff_resist_perception,

            _ => return None,
        })
    }
}

/// The fully-recalculated build totals: the raw [`GlobalBonuses`] accumulator (uncapped —
/// the gate grades it, and the UI reads a few fields the projection drops, e.g. `hitChance`)
/// plus the projected, capped [`CharacterStats`]. `recalculate` returns this so
/// `CharacterStats` is the single capped truth (D4).
///
/// `set_bonus_tracking` carries the set-bonus Rule-of-5 provenance the totals themselves don't
/// need but the UI does — the `(x/5)` counters, capped-strikethrough, over-cap rings/banner, and
/// per-stat tooltip set-bonus rows. It is the engine's OWN bucket decisions (the same tracking
/// that produced the summed totals), projected serializable ([`set_bonuses::tracking_out`]);
/// empty when no dataset catalog is loaded. Proc, active-power and accolade provenance ride their
/// own fields below; INHERENT and INCARNATE provenance is still absent, because those passes write
/// the accumulator outside the per-power walk that measures the rest.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CalculatedTotals {
    pub bonuses: GlobalBonuses,
    pub stats: CharacterStats,
    pub set_bonus_tracking: Vec<crate::set_bonuses::SetBonusStatTracking>,
    /// The always-on proc contributions to the dashboard breakdown — the beta `type:'proc'`
    /// sources (LotG +Recharge, Steadfast +Def, Performance Shifter, …), including the
    /// Rule-of-5-capped ones. Folded into the breakdown map by the output mapper. Empty when no
    /// proc database is loaded. Stealth-radius proc rows ride the stealth/movement provenance
    /// (PROD6), like the active-power rows.
    pub proc_breakdown: Vec<crate::procs::ProcBreakdownSource>,
    /// The toggled-on buff-pet aura contributions to the dashboard breakdown — one row per
    /// (pet aura, stat). Labelled with the SUMMONING power's display name by the output mapper,
    /// which is how the beta labels its synthetic per-pet power. Empty unless a build enabled a
    /// pet's toggle.
    pub buff_pet_breakdown: Vec<crate::buff_pets::BuffPetBreakdownSource>,
    /// The travel-buff contributions to the dashboard breakdown — one row per (movement source,
    /// axis), each flagged `suppressed` when it lost its suppress group or combat mode dropped
    /// it. Distinct from `capped`: travel suppression is ordinary game mechanics and must NOT
    /// feed the Rule-of-5 warning.
    pub movement_breakdown: Vec<crate::movement::MovementBreakdownSource>,
    /// The stealth-radius contributions to the dashboard breakdown — one row per (stealth
    /// source, axis), each flagged `superseded` when it lost its suppress group. The stealth
    /// twin of `movement_breakdown`, and separate from `power_breakdown` for the same reason:
    /// both totals are grouped-max resolves committed after the walk, so a source's contribution
    /// is not known while the snapshot bracket that attributes every other family is open.
    /// Carries the proc pass's stealth-IO rows as well as the walk's, which is what the
    /// `proc_breakdown` note above means by stealth rows riding this provenance.
    pub stealth_breakdown: Vec<crate::stealth::StealthBreakdownSource>,
    /// PROD6B-1 — the per-power **non-DPS execution + perma** projection: one entry per
    /// selected power carrying the resolved three-tier recharge / endurance / accuracy /
    /// cast-time / range + ArcanaTime + perma the per-power display surfaces read. Keyed to the
    /// power by `power_internal_name` / `power_set` like the breakdowns above. Empty on a build
    /// with no selected powers. The buff/debuff granted magnitudes (defense / resistance / mez /
    /// heal / absorb …) land in PROD6B-2; DPS stays Tier-3 Deferred.
    pub power_projection: Vec<crate::projection::PowerProjection>,
    /// The per-POWER contributions — one row per (contributor, breakdown key), carrying the
    /// beta's `active-power`, `accolade` and `inherent` source groups. Measured by diffing the
    /// accumulator around each contributor, so a row cannot disagree with the total it helped
    /// make. Two passes file here: the apply walk (active powers + accolades) and Pass 3
    /// ([`crate::inherents`], whose Vigilance / Fury are extracted defs in the `Inherent` set),
    /// which is why the row's `kind` — not the field it arrived in — names the source group.
    pub power_breakdown: Vec<crate::apply::PowerBreakdownSource>,
    /// Pass 6's incarnate contributions — one row per (slot contributor, breakdown key), for the
    /// beta's `incarnate` source group. Separate from `power_breakdown` because an incarnate is
    /// addressed by slot rather than by powerset, and one equipped power can contribute more than
    /// once (its stat block, its level shift, its below-45 exemplar buff).
    pub incarnate_breakdown: Vec<crate::incarnates::IncarnateBreakdownSource>,
    /// The hardest hit this build's chosen powersets can produce with a power's own slots filled
    /// for damage — the scale a damage bar is read against, from
    /// [`crate::projection::damage_ceiling`]. Carried on the totals rather than derived at the
    /// render site because it is a property of the BUILD (its sets, its level, its damage buffs),
    /// not of the power being shown, and a per-power derivation would re-scan the catalogue once
    /// per card. `None` when nothing in reach resolves to damage — including the ordinary case of
    /// no target chosen, where no damage number exists to scale either.
    pub damage_ceiling: Option<f64>,
    /// What the what-if TEAM-BUFF layer moved in producing these totals — the provenance a
    /// surface needs to mark a number as SIMULATED rather than as the build's own.
    ///
    /// Measured by the injection rather than re-read from the build for the same reason every
    /// other breakdown here is measured: a second answer to "what did the layer do" could
    /// disagree with the totals it claims to describe. Empty on every build with no what-if,
    /// which is the common case and the one a marker must never appear on.
    pub what_if: crate::what_if::Applied,
}

impl CalculatedTotals {
    /// How many of the build's slotted bonuses the Rule of 5 refused — the count a build-wide
    /// alert is about.
    ///
    /// Both pools, because the rule has two of them and a build hits either: set bonuses cap by
    /// `(stat, value)` in [`crate::set_bonuses`], and always-on proc globals keep a SEPARATE
    /// pool of their own in [`crate::procs`], so six Luck of the Gambler +Recharge pieces are
    /// over the cap without a single set bonus being.
    ///
    /// Counted as distinct SLOTTED THINGS — a tier in a power, a proc piece in a power — not as
    /// rejected `(stat, value)` rows. One rejected tier writes a rejection under every stat it
    /// grants, so a `damage_resistance_(all)` bonus would otherwise report as eight and a paired
    /// defense bonus as two. The number has to be one a reader can check against their own build.
    ///
    /// Deliberately not folded in: a suppressed travel buff and a superseded stealth radius also
    /// fail to reach the total, but they are ordinary game mechanics rather than a cap the build
    /// is paying for and getting nothing from ([`crate::movement`] says the same beside its own
    /// flag). Counting them here would make the alert fire on builds that have wasted nothing.
    pub fn rule_of_five_rejections(&self) -> usize {
        let set_bonuses: std::collections::BTreeSet<(&str, u8, &str, &str)> = self
            .set_bonus_tracking
            .iter()
            .flat_map(|stat| stat.buckets.values())
            .flat_map(|bucket| bucket.rejected_sources.iter())
            .map(|source| {
                (
                    source.set_name.as_str(),
                    source.pieces,
                    source.power_set.as_str(),
                    source.power_internal_name.as_str(),
                )
            })
            .collect();
        let procs: std::collections::BTreeSet<(&str, &str, &str, &str)> = self
            .proc_breakdown
            .iter()
            .filter(|source| source.capped)
            .map(|source| {
                (
                    source.set_name.as_str(),
                    source.proc_name.as_str(),
                    source.power_set.as_str(),
                    source.power_internal_name.as_str(),
                )
            })
            .collect();
        set_bonuses.len() + procs.len()
    }
}

/// Step 9.5 — write the purple-patch combat projections onto the accumulator
/// (`character-totals.ts:4446-4454`). `effective_level_diff` is signed (positive = the
/// target out-levels the caster). Returns `Err` with the fields left untouched when the
/// purple-patch table is empty (an unloaded db) — the caller records it and the display marks
/// the gap, rather than fabricating the beta's 0.75 / 1.0 default (Rule 1).
pub fn project_combat(
    bonuses: &mut GlobalBonuses,
    purple_patch: &PurplePatch,
    effective_level_diff: i32,
    content_mode: ContentMode,
) -> Result<(), CalcError> {
    let _ = content_mode; // combat mode gates only the defense softcap (a threshold), not these.
    let (Some(base_to_hit), Some(combat_modifier)) = (
        purple_patch::get_base_to_hit(purple_patch, effective_level_diff),
        purple_patch::get_combat_modifier(purple_patch, effective_level_diff),
    ) else {
        return Err(CalcError::new(
            "hitChance",
            "purple-patch tables are empty — cannot project base ToHit / combat modifier",
        ));
    };

    // finalToHit = clamp(baseToHit + toHit/100, 0.05, 0.95); the toHit/accuracy globals are
    // PERCENTAGES. The [0.05, 0.95] clamp is applied twice — once on ToHit, once on the
    // ToHit×accuracy product — exactly as the beta does.
    let final_to_hit = (base_to_hit + bonuses.to_hit / 100.0).clamp(0.05, 0.95);
    let accuracy_mult = 1.0 + bonuses.accuracy / 100.0;
    bonuses.base_to_hit = base_to_hit;
    bonuses.hit_chance = (final_to_hit * accuracy_mult).clamp(0.05, 0.95);
    bonuses.combat_modifier = combat_modifier;
    Ok(())
}

/// Absolute, capped Max HP: `min(baseHealth·(1 + maxHpBuffPct/100), maxHealth)`, or uncapped
/// when the cap is 0 — the beta `getBaselineHealth` + display-layer clamp
/// ([stat-definitions.ts:481](../../../CoH-Sidekick/src/utils/calculations/stat-definitions.ts#L481),
/// character-totals.ts:4422). One definition shared by [`convert_to_character_stats`] (the
/// [`CharacterStats::max_hp_absolute`] field) and the absorb-fraction resolve, so the fraction a
/// power grants (a % of final Max HP) is taken against exactly the HP the dashboard shows.
/// `max_hp_buff_pct` is the `+MaxHP` buff PERCENT (`GlobalBonuses::max_hp`).
pub fn absolute_max_hp(caps: &ArchetypeCaps, max_hp_buff_pct: f64, level: i32) -> f64 {
    let (base, cap) = caps.baseline_health(level);
    let buffed = base * (1.0 + max_hp_buff_pct / 100.0);
    if cap > 0.0 {
        buffed.min(cap)
    } else {
        buffed
    }
}

/// `convertToCharacterStats` + D4 caps. Projects `bonuses` into [`CharacterStats`]: straight
/// copies, the combine-by-`max` pairs, the resistance-cap and absolute-HP clamps, and the
/// defense-softcap threshold. `caps`/`defense_softcap` are `Option` so a hand-built db
/// without cap data fails loud (a [`CalcError`] per missing input) instead of clamping to a
/// fabricated ceiling; the returned errors are appended to the accumulator by the caller.
pub fn convert_to_character_stats(
    bonuses: &GlobalBonuses,
    caps: Option<&ArchetypeCaps>,
    defense_softcap: Option<f64>,
    level: i32,
    movement_cap_bumps: &MovementCapBumps,
) -> (CharacterStats, Vec<CalcError>) {
    let mut errors = Vec::new();
    let mut stats = CharacterStats {
        // Offense.
        damage: bonuses.damage,
        accuracy: bonuses.accuracy,
        to_hit: bonuses.to_hit,
        recharge: bonuses.recharge,
        endurance_reduction: bonuses.endurance,

        // Defense positional — uncapped.
        def_melee: bonuses.defense_melee,
        def_ranged: bonuses.defense_ranged,
        def_aoe: bonuses.defense_aoe,

        // Defense typed — combine-by-max, uncapped.
        def_sl: bonuses.defense_smashing.max(bonuses.defense_lethal),
        def_fc: bonuses.defense_fire.max(bonuses.defense_cold),
        def_en: bonuses.defense_energy.max(bonuses.defense_negative),
        def_psionic: bonuses.defense_psionic,
        def_toxic: bonuses.defense_toxic,
        def_smashing: bonuses.defense_smashing,
        def_lethal: bonuses.defense_lethal,
        def_fire: bonuses.defense_fire,
        def_cold: bonuses.defense_cold,
        def_energy: bonuses.defense_energy,
        def_negative: bonuses.defense_negative,

        // Resistance typed — combine-by-max, clamped below once the cap is known.
        res_sl: bonuses.resistance_smashing.max(bonuses.resistance_lethal),
        res_fc: bonuses.resistance_fire.max(bonuses.resistance_cold),
        res_en: bonuses.resistance_energy.max(bonuses.resistance_negative),
        res_psionic: bonuses.resistance_psionic,
        res_toxic: bonuses.resistance_toxic,
        res_smashing: bonuses.resistance_smashing,
        res_lethal: bonuses.resistance_lethal,
        res_fire: bonuses.resistance_fire,
        res_cold: bonuses.resistance_cold,
        res_energy: bonuses.resistance_energy,
        res_negative: bonuses.resistance_negative,

        // Recovery & HP.
        recovery: bonuses.recovery,
        regeneration: bonuses.regeneration,
        max_hp: bonuses.max_hp,
        max_end: bonuses.max_endurance,

        // Debuff resistance.
        debuff_resist_slow: bonuses.debuff_resist_slow,
        debuff_resist_defense: bonuses.debuff_resist_defense,
        debuff_resist_recharge: bonuses.debuff_resist_recharge,
        debuff_resist_endurance: bonuses.debuff_resist_endurance,
        debuff_resist_recovery: bonuses.debuff_resist_recovery,
        debuff_resist_to_hit: bonuses.debuff_resist_to_hit,
        debuff_resist_regeneration: bonuses.debuff_resist_regeneration,
        debuff_resist_perception: bonuses.debuff_resist_perception,

        max_hp_absolute: 0.0,
        defense_softcap: 0.0,
        resistance_cap: 0.0,
        max_hp_cap: 0.0,
        // Clamped below once the cap is known; uncapped without AT caps, like `res_*`.
        absorb: bonuses.absorb,
        absorb_cap: 0.0,
        to_hit_cap: 0.0,
        regeneration_cap: 0.0,
        recovery_cap: 0.0,
        defense_ceiling: 0.0,
        max_endurance_absolute: 0.0,
        max_endurance_cap: 0.0,
        // Projected below; without AT caps there is no base scale to project FROM, so these stay
        // at their zero default rather than reporting a speed derived from a fabricated base.
        run_speed: ProjectedMovement::default(),
        fly_speed: ProjectedMovement::default(),
        jump_speed: ProjectedMovement::default(),
        jump_height: ProjectedMovement::default(),
    };

    // Hard caps (D4). Resistance clamps to `resistanceCap × 100` (a real ceiling); the
    // absolute HP clamps to the per-level HP cap; absorb clamps to the per-level absorb
    // ceiling. All need the AT caps.
    //
    // Not an `if let`: the empty `None` arm below carries the reasoning for doing nothing
    // when an archetype has no caps, and the comment at the `bind` closure above points at
    // it by name. Collapsing the match deletes the only place that decision is written.
    #[allow(clippy::single_match)]
    match caps {
        Some(caps) => {
            let res_cap = caps.resistance_cap * 100.0;
            for res in [
                &mut stats.res_sl,
                &mut stats.res_fc,
                &mut stats.res_en,
                &mut stats.res_psionic,
                &mut stats.res_toxic,
                &mut stats.res_smashing,
                &mut stats.res_lethal,
                &mut stats.res_fire,
                &mut stats.res_cold,
                &mut stats.res_energy,
                &mut stats.res_negative,
            ] {
                *res = res.min(res_cap);
            }
            // Absolute HP: base HP scaled by the +MaxHP buff percent, clamped to the cap. Shared
            // with the absorb-fraction resolve (lib.rs) so both read one definition (Rule 8).
            stats.max_hp_absolute = absolute_max_hp(caps, bonuses.max_hp, level);
            stats.resistance_cap = res_cap;
            stats.max_hp_cap = caps.hp_cap_at_level(level);
            // Absorb clamps to the per-level AttribMaxMax Absorb ceiling — the game's
            // `ClampMax` (`Common/entity/character_attribs.c`) clamps `attrMax.fAbsorb`
            // between AttribMin (0 for every class) and `pclass->pattrMaxMax[combatLevel]`.
            // The ceiling is the CLASS TABLE value, so +MaxHP buffs (accolades, Dull Pain)
            // raise Max HP without raising how much absorb can sit on top of it.
            let absorb_cap = caps.absorb_cap_at_level(level);
            stats.absorb_cap = absorb_cap;
            if absorb_cap > 0.0 {
                stats.absorb = stats.absorb.clamp(0.0, absorb_cap);
            }
            // Travel: each axis's buff percentage against the class's own base scale and its
            // per-level ceiling, plus whatever the build's active travel powers raised that
            // ceiling by. The percentages the projection reads stay untouched on `bonuses`.
            let project = |stat: MovementStat, buff_percent: f64| {
                movement::project_axis(stat, buff_percent, caps, movement_cap_bumps, level)
            };
            stats.run_speed = project(MovementStat::RunSpeed, bonuses.run_speed);
            stats.fly_speed = project(MovementStat::FlySpeed, bonuses.fly_speed);
            stats.jump_speed = project(MovementStat::JumpSpeed, bonuses.jump_speed);
            stats.jump_height = project(MovementStat::JumpHeight, bonuses.jump_height);

            // The CAPS-1 attribute ceilings: `ClampCur` bounds each of these against the
            // class's own per-level row. Each ceiling travels with its value for the same
            // reason `resistance_cap` does. An absent table leaves the stat RAW and the
            // ceiling 0.0 ("unknown"), never clamped to a fabricated number — the same
            // no-caps posture as the `None` arm below, one attribute at a time.
            let bind = |ceiling: Option<f64>, value: &mut f64, carried: &mut f64| {
                if let Some(ceiling) = ceiling {
                    *carried = ceiling;
                    *value = value.min(ceiling);
                }
            };
            bind(
                caps.to_hit_buff_cap_at_level(level),
                &mut stats.to_hit,
                &mut stats.to_hit_cap,
            );
            bind(
                caps.regeneration_buff_cap_at_level(level),
                &mut stats.regeneration,
                &mut stats.regeneration_cap,
            );
            bind(
                caps.recovery_buff_cap_at_level(level),
                &mut stats.recovery,
                &mut stats.recovery_cap,
            );
            // Both bounds of `CLAMP_CUR(fDefenseType[..])`. The FLOOR is the half MOVEMIN-1's
            // sibling gap left out (ATTRMIN-1): the game writes "your defense is negated" as a
            // saturating magnitude, not as a switch — Thunderspy's Organic Armor states
            // `Defense −500` on every typed slot while Defensive Adaptation is up — so a
            // defense total has no defined value at all until something bounds it below.
            //
            // Floor and ceiling apply in that order, and the floor applies even when the
            // ceiling is missing: they are separate rows, so an absent ceiling withdraws
            // itself and not the floor (the same posture `movement::project_axis` takes).
            let defense_floor = caps.defense_floor_percent();
            let defense_ceiling = caps.defense_ceiling_at_level(level);
            if let Some(ceiling) = defense_ceiling {
                stats.defense_ceiling = ceiling;
            }
            for def in [
                &mut stats.def_melee,
                &mut stats.def_ranged,
                &mut stats.def_aoe,
                &mut stats.def_sl,
                &mut stats.def_fc,
                &mut stats.def_en,
                &mut stats.def_psionic,
                &mut stats.def_toxic,
                &mut stats.def_smashing,
                &mut stats.def_lethal,
                &mut stats.def_fire,
                &mut stats.def_cold,
                &mut stats.def_energy,
                &mut stats.def_negative,
            ] {
                *def = def.max(defense_floor);
                if let Some(ceiling) = defense_ceiling {
                    *def = def.min(ceiling);
                }
            }
            // Endurance's buff is in POINTS, so unlike the percentage stats above there is
            // no ceiling to put on `max_end` itself — the clamp lands on the pool it makes.
            if let Some((base_pool, pool_cap)) = caps.max_endurance_at_level(level) {
                stats.max_endurance_cap = pool_cap;
                stats.max_endurance_absolute = (base_pool + bonuses.max_endurance).min(pool_cap);
            }
        }
        // No caps: leave resistance raw and absolute HP 0 rather than fabricating a
        // baseline (Rule 1). Deliberately NOT an error here — this function cannot tell a
        // build that has no archetype yet (the app's own boot state, where uncapped and 0
        // are the right answers) from an archetype the dataset has no caps for. Only the
        // caller knows which, so `recalculate` raises it there.
        None => {}
    }

    match defense_softcap {
        Some(softcap) => stats.defense_softcap = softcap,
        None => errors.push(CalcError::new(
            "defenseSoftcap",
            "purple-patch defense-softcap table is empty — softcap threshold unknown",
        )),
    }

    (stats, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pair keeps reporting the larger half, and each half now reports itself under the same
    /// clamp — the Fiery Aura shape, where Fire runs into the cap and Cold stays well under it.
    #[test]
    fn paired_types_project_one_by_one_under_the_pairs_clamp() {
        let bonuses = GlobalBonuses {
            resistance_fire: 90.0,
            resistance_cold: 20.0,
            defense_fire: 12.0,
            defense_cold: 3.0,
            ..GlobalBonuses::default()
        };
        let caps = ArchetypeCaps {
            resistance_cap: 0.75,
            ..ArchetypeCaps::default()
        };
        let (stats, _) = convert_to_character_stats(
            &bonuses,
            Some(&caps),
            Some(45.0),
            50,
            &MovementCapBumps::default(),
        );

        assert_eq!(stats.res_fire, 75.0, "fire clamps to the AT cap");
        assert_eq!(stats.res_cold, 20.0);
        assert_eq!(stats.res_fc, 75.0, "the pair is still the larger half");
        assert_eq!(stats.def_fire, 12.0);
        assert_eq!(stats.def_cold, 3.0);
        assert_eq!(stats.def_fc, 12.0);
    }
}
