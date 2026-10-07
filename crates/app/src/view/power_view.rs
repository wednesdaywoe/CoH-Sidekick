//! `PowerView` — the ONE display projection of a power. The hover tooltip, the info
//! panel, and any future surface render from the same instance, so a power can never
//! show one thing in one window and another elsewhere (the beta's window-drift bug
//! class, designed out).
//!
//! Every number here comes from [`coh_math::projection::PowerProjection`] — the engine's
//! per-power resolve, already three-tiered against this build's slotting and globals. The view
//! chooses labels, units and order; it computes nothing. A surface that re-derived a magnitude
//! beside the engine is exactly how the two drift, which is the whole reason the projection
//! owns the math (PROD6B).

use crate::panels::stat_registry::StatFamily;
use crate::shell::{Db, Selection};
use coh_data::{CharacterState, Power};
use coh_math::damage::{DamageApplication, DamageComponent, PowerDamage};
use coh_math::effect_registry::{self, EffectCategory, EffectFormat, SummaryGate};
use coh_math::granted::{GrantedMagnitude, GrantedQuantity};
use coh_math::perma::PermaInfo;
use coh_math::projection::{PowerProjection, PowerRef as ProjectionRef, ThreeTier};
use coh_math::CalculatedTotals;
use std::collections::BTreeSet;

/// Resolve a power definition, whichever partition of the dataset holds it: the powersets,
/// the pool/epic partitions, or — for a granted inherent, which carries the synthetic
/// `Inherent` set rather than a real one — by ident across all three.
///
/// Every surface that turns a `(set_id, ident)` pair back into a def goes through here. The
/// powerset lookup alone is not enough: pool and epic powers live in flat partitions tagged
/// by the aggregate that owns them, so a pool power resolved by powerset comes back absent.
pub fn resolve_power_def<'a>(
    database: &'a Db,
    set_id: &str,
    power_ident: &str,
) -> Option<&'a Power> {
    if set_id == coh_data::INHERENT_SET {
        return database.find_granted_power(power_ident);
    }
    database
        .find_power(set_id, power_ident)
        .or_else(|| database.find_partition_power(set_id, power_ident))
}

/// The engine's projection for one power against the live build.
///
/// A power the build HOLDS is already projected — [`coh_math::recalculate`] walks every
/// selection — so it is read out of the shared totals rather than recomputed, and it carries
/// its slotting. A power the build does not hold (hovered in the picker, previewed in the pool
/// modal) has no entry, so it is projected on demand through
/// [`coh_math::recalculate_projecting`], unslotted but still taking Alpha and the build-wide
/// globals.
///
/// That second path re-runs the whole pipeline for one power, which is why it is NOT folded
/// into the shell's `BuildTotals` memo: every stat panel reads that memo, and hovering a power
/// in the picker would recompute the dashboard. Here it sits behind a memo keyed to the
/// selection, so it runs once per power pointed at.
pub fn projection_for(
    database: &Db,
    build: &CharacterState,
    totals: &CalculatedTotals,
    powerset_id: &str,
    ident: &str,
) -> Option<PowerProjection> {
    let matches = |projection: &PowerProjection| {
        projection.power_set == powerset_id && projection.power_internal_name == ident
    };
    if let Some(held) = totals.power_projection.iter().find(|p| matches(p)) {
        return Some(held.clone());
    }
    let requested = [ProjectionRef {
        powerset: powerset_id.to_string(),
        internal_name: ident.to_string(),
        targets_hit: None,
    }];
    coh_math::recalculate_projecting(build, database, &requested)
        .power_projection
        .into_iter()
        .find(matches)
}

/// The full view of one power — its definition and its projection, resolved together.
/// `None` when the dataset does not carry the power at all.
pub fn view_for(
    database: &Db,
    build: &CharacterState,
    totals: &CalculatedTotals,
    powerset_id: &str,
    ident: &str,
) -> Option<PowerView> {
    let power = resolve_power_def(database, powerset_id, ident)?;
    let projection = projection_for(database, build, totals, powerset_id, ident);
    Some(PowerView::build(
        power,
        projection.as_ref(),
        totals.damage_ceiling,
    ))
}

/// Resolve the current selection against the current database — the one derivation the
/// Totals and Info memos share. Identity re-resolves against the CURRENT database (see
/// `PowerRef`): `Err(power)` is a selection this dataset doesn't carry — rendered as a
/// note, never an index panic. `None` = nothing selected.
pub fn resolve_selected_view(
    database: &Db,
    build: &CharacterState,
    totals: &CalculatedTotals,
    selection: &Selection,
) -> Option<Result<PowerView, String>> {
    selection.as_ref().map(|selected| {
        view_for(
            database,
            build,
            totals,
            &selected.powerset_id,
            &selected.power,
        )
        .ok_or_else(|| selected.power.clone())
    })
}

/// One resolved effect, rendered. Every string is already in its display unit; the view holds
/// no raw scale or table name, because an unresolved `scale × table` is engine internals, not
/// something a player can read a build against.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectRow {
    pub key: String,
    pub label: String,
    /// The stat-colour token for this row's family — the view speaks in tokens, never colors,
    /// so themes re-skin it for free.
    pub token: &'static str,
    /// The mez rank a duration row applies at (`Mag 3`), when the effect is a mez.
    pub magnitude: Option<String>,
    /// The effective duration of a MAGNITUDE-valued mez row, in seconds — the number the
    /// tier cannot carry, restored from the engine row (`GrantedMagnitude::duration`) now that
    /// the display bag's `durations` map is gone (ENGLAG-2). Rendered as `(8s)` beside the
    /// rank, matching the pre-strip annotation. `None` on every row whose tier already IS the
    /// seconds (a `MezDuration` row, or any row without a recorded duration), so a row the
    /// second number never prints a second number twice.
    pub duration: Option<String>,
    /// The mechanic a damage component belongs to, in the export's own words — see
    /// [`mechanic_names`]. Its own field rather than part of [`Self::magnitude`] because it
    /// names WHAT the row is, where the annotation beside it only qualifies how it lands, and
    /// the two earn different weight. `None` on every row that is not a damage component.
    pub mechanic: Option<String>,
    pub base: String,
    /// `None` for an effect that declares no enhancement aspect: the beta shows an em dash in
    /// both enhanced columns rather than repeating the base, so a row that slotting cannot
    /// move never looks like one that slotting happened not to move.
    pub enhanced: Option<String>,
    pub enhanced_final: Option<String>,
    /// Slotting moved this row (the enhanced column earns emphasis).
    pub enhanced_changed: bool,
    /// A build-wide global moved it beyond the slotting (the final column earns emphasis).
    pub final_changed: bool,
}

/// A group of effect rows under one heading — the beta's MEZ / BUFFS / DEBUFFS / SPECIAL
/// sections, which collapse several registry categories each so a player's protection and
/// travel buffs sit with their +damage buffs instead of under half-empty subheads.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectSection {
    pub title: &'static str,
    pub rows: Vec<EffectRow>,
}

/// The damage block of one power, ready to render.
///
/// Three kinds of row, kept apart because they are three different claims. `certain` is what
/// every landed hit deals and is the only thing the total sums. `conditional` is the rows that
/// may or may not land — the archetype hit-time mechanics (a critical, Scourge, Assassination)
/// and the components the export ships inert — each carrying the reason it is set aside, never
/// averaged into the total. `unresolved` is what nobody has said enough for: chiefly a
/// target-gated component with no target chosen, which is the whole block until one is picked.
///
/// Every one of them carries the mechanic's name where the export supplies one — see
/// [`mechanic_names`]. That is what lets this surface answer the question the beta answered with
/// five `is<Archetype>AttackPower` name checks, without knowing an archetype exists.
#[derive(Debug, Clone, PartialEq)]
pub struct DamageView {
    pub certain: Vec<EffectRow>,
    pub conditional: Vec<EffectRow>,
    /// Components whose condition nobody can answer ahead of the fight — the foe is held, it
    /// carries a debuff, it is below half health — stated as what they ADD when it holds. The
    /// gate decides whether, not how much, so the amount is a fact even while the condition is
    /// not; only the line whose amount is unknowable too falls through to `unresolved`.
    pub situational: Vec<EffectRow>,
    pub unresolved: Vec<String>,
    /// How many components are waiting only on a target rank. They are kept out of `unresolved`
    /// because the planner can answer them: the panel offers the rank instead of the gate text.
    pub waiting_on_rank: usize,
    /// The certain rows summed, three-tier. `None` when there is nothing to sum: no certain
    /// component at all, or exactly one — whose own row already is the total.
    pub total: Option<EffectRow>,
    /// The archetype's damage-strength cap bound the total.
    pub capped: bool,
    /// The same three tiers as NUMBERS — what a bar needs and a table of strings cannot give
    /// back. A ranged hit contributes its floor, the number [`Self::total`] states first, so
    /// the bar never draws a length the rows above it don't claim.
    pub tiers: ThreeTier,
    /// What a full bar means: the hardest hit this build's CHOSEN POWERSETS can produce with a
    /// power's slots filled for damage ([`coh_math::projection::damage_ceiling`]), or this
    /// power's own final where it somehow out-hits that.
    ///
    /// Shared across powers on purpose — a per-power reference (each power against its own
    /// ceiling) draws two attacks of different strength at the same length. But shared is not
    /// enough on its own: the build's own hardest PICK, which is what this used to be, is a
    /// maximum that is always attained, so exactly one power read full on every build and the
    /// bar's range went to the gap between the best and second-best pick. Measuring against the
    /// sets instead means a full bar is something a build can fail to reach, and so something
    /// worth reaching. `0.0` when nothing in reach deals damage, and then no bar is drawn.
    pub reference: f64,
    /// The hit this build actually lands, formatted — the headline number the block leads with.
    /// A ranged hit states both ends for the same reason [`Self::total`] does: its floor alone
    /// reads as the whole answer.
    pub headline: String,
    /// The same hit as the game ships it, unslotted and unbuffed. Printed beside the headline
    /// rather than under it because it is not a second number to read — it is the anchor that
    /// makes the first one mean something, and a build with nothing in the slots shows the two
    /// agreeing, which is the honest picture.
    pub base_text: String,
    /// [`Self::headline`] against [`Self::base_text`], as one percentage — the arithmetic a
    /// reader would otherwise do between the two. `None` when nothing moved the hit, which is
    /// when the figure would be `+0%`: a delta of nothing is noise, not a fact.
    ///
    /// One number for both halves of the gap on purpose. The split between what SLOTTING bought
    /// and what the BUILD's globals added is already drawn, in the bar's two inner fills, and
    /// saying it twice in two grammars makes the reader reconcile them.
    pub delta: Option<String>,
    /// The damage types the headline is MADE OF — the chips that ride beside it, sorted and
    /// distinct, read off the same components every other figure in this block is read off.
    ///
    /// Derived here rather than from [`PowerView::damage_types`] because the two answer
    /// different questions and only one of them is the question the chips ask. That field is a
    /// power-level classification off the power's UNGATED atoms, so it is blind to the build:
    /// a Dual Pistols attack with a special ammo loaded deals its ammo's element and NOT its
    /// Lethal base — the base row goes dormant — and the classification went on saying Lethal
    /// beside a Cold number (CHIPTYPE-1). A component that never lands is excluded for the same
    /// reason: the chip is the headline's own label, and a dormant rider is not in the headline.
    /// Its row is still listed below with the reason it is inert, so nothing is hidden.
    ///
    /// A power whose every component is inert lands no live type at all, which is a real state
    /// (damage waiting on an external buff). [`PowerView::build`] fills that case from the
    /// classification, so this is what the panel draws either way and the chips have exactly one
    /// source — two reads of one value can disagree, and on this value they did.
    pub types: Vec<String>,
    /// The top of a ranged hit, three-tier, or `None` for a fixed one. Kept as numbers so a
    /// metric other than raw damage can divide both ends.
    pub high: Option<ThreeTier>,
    /// What the per-second and per-endurance readings divide by.
    pub timing: DamageTiming,
}

/// The power's own costs, as the projection resolved them, for the damage metrics.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DamageTiming {
    /// ArcanaTime where the projection has it, else the authored cast. ArcanaTime is what a
    /// rotation actually pays, and it is what the chain builder and powerset compare divide by.
    pub cast: Option<f64>,
    pub recharge: Option<ThreeTier>,
    pub endurance: Option<ThreeTier>,
}

impl DamageTiming {
    fn of(projection: &PowerProjection) -> Self {
        Self {
            cast: projection
                .arcana_time
                .or(projection.cast_time.map(|tier| tier.base)),
            recharge: projection.recharge,
            endurance: projection.endurance_cost,
        }
    }
}

/// Which reading of a power's damage the hero line states. A display preference, not build
/// state: it changes no total, only how one is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DamageMetric {
    /// One activation's damage.
    #[default]
    Damage,
    /// Damage per second of animation — what the cast locks you out for.
    PerAnimation,
    /// Damage per second of the full cycle: animation plus recharge.
    PerSecond,
    /// Damage per point of endurance.
    PerEndurance,
}

impl DamageMetric {
    pub const ALL: [DamageMetric; 4] = [
        DamageMetric::Damage,
        DamageMetric::PerAnimation,
        DamageMetric::PerSecond,
        DamageMetric::PerEndurance,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DamageMetric::Damage => "DMG",
            DamageMetric::PerAnimation => "DPA",
            DamageMetric::PerSecond => "DPS",
            DamageMetric::PerEndurance => "DPE",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            DamageMetric::Damage => "Damage — what one activation deals",
            DamageMetric::PerAnimation => {
                "Damage per animation — damage ÷ activation time (ArcanaTime)"
            }
            DamageMetric::PerSecond => {
                "Damage per second — damage ÷ full cycle (activation + recharge)"
            }
            DamageMetric::PerEndurance => "Damage per endurance — damage ÷ endurance cost",
        }
    }
}

/// The hero line under one [`DamageMetric`].
#[derive(Debug, Clone, PartialEq)]
pub struct DamageHero {
    pub headline: String,
    pub base_text: String,
    pub delta: Option<String>,
}

impl DamageView {
    /// The hero line read as `metric`, or why it cannot be (a toggle has no cast to divide by,
    /// an inherent costs no endurance). Each tier divides by its OWN tier of the cost: the base
    /// hit by the base cycle, the final hit by the cycle this build's recharge leaves.
    pub fn hero(&self, metric: DamageMetric) -> Result<DamageHero, &'static str> {
        let base_div = self.divisor(metric, |tier| tier.base)?;
        let final_div = self.divisor(metric, |tier| tier.r#final)?;
        let base = self.tiers.base / base_div;
        let r#final = self.tiers.r#final / final_div;
        Ok(DamageHero {
            headline: span_text(r#final, self.high.map(|high| high.r#final / final_div)),
            base_text: span_text(base, self.high.map(|high| high.base / base_div)),
            delta: damage_delta(base, r#final),
        })
    }

    /// A slotted proc's average damage per activation, read as `metric`. Divided by the FINAL
    /// tier of the cost, because the procs ride on the cast this build actually makes.
    pub fn proc_reading(&self, metric: DamageMetric, per_cast: f64) -> Option<f64> {
        let div = self.divisor(metric, |tier| tier.r#final).ok()?;
        Some(per_cast / div)
    }

    /// What `metric` divides a hit by, at the tier `tier` picks out of each cost.
    fn divisor(
        &self,
        metric: DamageMetric,
        tier: fn(&ThreeTier) -> f64,
    ) -> Result<f64, &'static str> {
        let timing = self.timing;
        let positive = |value: f64| (value > 0.0).then_some(value);
        match metric {
            DamageMetric::Damage => Ok(1.0),
            DamageMetric::PerAnimation => {
                timing.cast.and_then(positive).ok_or("no activation time")
            }
            DamageMetric::PerSecond => {
                let cast = timing.cast.unwrap_or(0.0);
                let recharge = timing.recharge.as_ref().map_or(0.0, tier);
                positive(cast + recharge).ok_or("no cycle time")
            }
            DamageMetric::PerEndurance => timing
                .endurance
                .as_ref()
                .map(tier)
                .and_then(positive)
                .ok_or("no endurance cost"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerView {
    pub name: String,
    pub internal_name: Option<String>,
    /// The def's `icon` FILENAME, not a resolved URL — the same field the picked card and the
    /// Available rail read. Kept as the export states it so the URL stays derived at render
    /// (`view::icons`), which is the rule that module opens with.
    pub icon: Option<String>,
    pub short_help: Option<String>,
    /// The authored description, one paragraph per authored break. Game markup is stripped
    /// rather than rendered, and the breaks it carries become real paragraphs — the export's
    /// `<br>`/`<color>` tags are the game client's own, and Dioxus escapes text, so leaving
    /// them in would print the tags at the reader.
    pub description: Vec<String>,
    /// Power type · target type · effect area, as the export states them.
    pub tags: Vec<String>,
    /// The power's damage-type CLASSIFICATION — its set of types read off its own ungated atoms
    /// (Thunderspy from the converter's `damageTypes`). Power-level, never per-atom, and blind to
    /// the build by construction, which is why it is no longer what the chips beside a damage
    /// figure are drawn from: that is [`DamageView::types`], read off the components the figure
    /// itself is read off (CHIPTYPE-1).
    ///
    /// Two readers remain, and both are cases where there is no live component to answer with: a
    /// power the converter typed but the engine resolved no damage for, whose chips the panel
    /// draws on its own, and a power whose damage is wholly inert, where [`Self::build`] feeds
    /// this into `DamageView::types` so the block still carries a label.
    ///
    /// `Err` when the converter field is malformed (Rule 1: rendered as a visible
    /// marker for this power; the rest of the view still renders).
    pub damage_types: Result<Vec<String>, String>,
    /// The damage this power deals against the build's chosen target, one row per resolved
    /// component plus a total (RB5), and the tiers as numbers for the bar. Empty for a power
    /// that deals none.
    pub damage: Option<DamageView>,
    /// What the power costs to fire and where it reaches, grouped by what the groups MEAN —
    /// see [`execution_group`]. This used to be two lists split on whether slotting could move
    /// a row, which is a fact about the enhancement system wearing a heading: "Stats" and a
    /// trailing pairs list, with cast time filed away from the recharge it is read against.
    pub execution_groups: Vec<EffectSection>,
    pub sections: Vec<EffectSection>,
    /// The rows the power's own authored summary names, flattened out of [`Self::sections`] —
    /// what a surface with limited room leads with. Empty when the summary named nothing this
    /// registry knows: every row still sits on the full face, so declining to classify costs a
    /// promotion, never a row.
    ///
    /// Flat rather than sectioned because this is the short list by construction — a summary
    /// naming one debuff and one mez earns two headings over two rows, and a heading per row is
    /// chrome outweighing what it introduces.
    pub headline_rows: Vec<EffectRow>,
    pub perma: Option<PermaInfo>,
    /// This power's post-ED slotted bonuses, aspect label → percent string.
    pub enhancement_bonuses: Vec<(String, String)>,
    pub allowed_enhancements: Vec<String>,
    /// Set when the engine could not project this power though the dataset carries its
    /// definition — the two resolvers disagreeing is a defect, so it shows rather than
    /// leaving a card with an unexplained absence of numbers (Rule 1).
    pub fault: Option<String>,
}

impl PowerView {
    pub fn build(
        power: &Power,
        projection: Option<&PowerProjection>,
        damage_ceiling: Option<f64>,
    ) -> Self {
        let string_field = |key: &str| {
            power
                .extra
                .get(key)
                .and_then(|v| v.as_str())
                .map(String::from)
        };
        let tags = ["powerType", "targetType", "effectArea"]
            .iter()
            .filter_map(|key| string_field(key))
            .collect();

        let damage_types = power.damage_types().map(|set| {
            set.iter()
                .map(|s| s.as_wire().to_string())
                .collect::<Vec<_>>()
        });

        let short_help = string_field("shortHelp");
        // The designers' own one-line answer to "what is this power for", read as a set of
        // effect keys. Open — they named nothing we recognise, or wrote "Special" — promotes
        // nothing rather than everything: the rows stay on the full face where they already
        // live, so a gate that cannot classify makes the panel no denser than it is today.
        let headline_keys = match short_help.as_deref().map(effect_registry::summary_gate) {
            Some(SummaryGate::Named(keys)) => keys,
            _ => BTreeSet::new(),
        };
        let (execution, sections, headline_rows, perma, enhancement_bonuses) = match projection {
            Some(projection) => (
                execution_rows(projection),
                effect_sections(&projection.granted_magnitudes, None),
                effect_sections(&projection.granted_magnitudes, Some(&headline_keys))
                    .into_iter()
                    .flat_map(|section| section.rows)
                    .collect(),
                // The display gate the card's ring uses (the beta InfoPanel's
                // isPermaEligible guard): an un-permable power keeps its computed
                // numbers on the projection but shows no tracker.
                projection.perma.filter(|_| projection.perma_eligible),
                enhancement_bonus_rows(projection),
            ),
            None => (Vec::new(), Vec::new(), Vec::new(), None, Vec::new()),
        };

        PowerView {
            name: power.name.clone(),
            internal_name: power.internal_name.clone(),
            icon: string_field("icon"),
            short_help,
            description: string_field("description")
                .map(|text| paragraphs(&text))
                .unwrap_or_default(),
            tags,
            damage: projection
                .and_then(|projection| {
                    damage_view(
                        &projection.damage,
                        damage_ceiling,
                        DamageTiming::of(projection),
                    )
                })
                .map(|mut view| {
                    // The one case the live read cannot answer, resolved HERE rather than in the
                    // panel: a power whose damage is wholly inert has no live type to name, and
                    // the power-level classification is the only thing left to say. Doing it here
                    // keeps `DamageView::types` the single thing a chip is ever drawn from.
                    if view.types.is_empty() {
                        view.types = damage_types.clone().unwrap_or_default();
                    }
                    view
                }),
            damage_types,
            execution_groups: execution_groups(execution),
            sections,
            headline_rows,
            perma,
            enhancement_bonuses,
            allowed_enhancements: power.allowed_enhancements.clone().unwrap_or_default(),
            fault: projection
                .is_none()
                .then(|| format!("{} could not be projected against this build.", power.name)),
        }
    }
}

/// The execution block: every `execution`-category row the projection resolved, in registry
/// priority order, plus ArcanaTime — which is derived from the cast time rather than authored,
/// so it has no registry key of its own.
fn execution_rows(projection: &PowerProjection) -> Vec<EffectRow> {
    // The registry declares this order and this block is its only consumer, so the sort lives
    // here rather than in the engine. `granted_magnitudes` arrives in resolution order, which
    // is not the declared one: it put Max Targets (priority 9) above Arc (8) on every cone in
    // the game. A key that declares no priority sorts last — an absent priority is not zero.
    let mut ordered: Vec<&GrantedMagnitude> = projection
        .granted_magnitudes
        .iter()
        .filter(|magnitude| magnitude.category == EffectCategory::Execution)
        .collect();
    ordered.sort_by(|a, b| {
        let rank = |magnitude: &GrantedMagnitude| magnitude.priority.unwrap_or(f64::INFINITY);
        rank(a)
            .partial_cmp(&rank(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut rows: Vec<EffectRow> = ordered
        .into_iter()
        .map(effect_row)
        .map(|mut row| {
            if let Some(short) = tile_label(&row.key) {
                row.label = short.to_string();
            }
            row
        })
        .collect();
    // Hit chance sits beside the accuracy it is computed from. The accuracy tile reads the power
    // against an even-level target with no ToHit; this one reads it against the target level and
    // level shift the combat panel is set to, so the label names that gap.
    if let Some(hit) = projection.hit_chance {
        let accuracy_token = rows
            .iter()
            .find(|row| row.key == "accuracy")
            .map(|row| row.token)
            .unwrap_or_else(|| StatFamily::Neutral.token());
        let row = EffectRow {
            key: "hitChance".to_string(),
            label: format!("hit vs {}", level_gap_label(hit.level_diff)),
            token: accuracy_token,
            magnitude: None,
            duration: None,
            mechanic: None,
            base: format!("{}%", format_precision(hit.chance * 100.0, 1)),
            enhanced: None,
            enhanced_final: None,
            enhanced_changed: false,
            final_changed: false,
        };
        let at = rows
            .iter()
            .position(|row| row.key == "accuracy")
            .map_or(rows.len(), |index| index + 1);
        rows.insert(at, row);
    }
    if let Some(arcana) = projection.arcana_time {
        rows.push(EffectRow {
            key: "arcanaTime".to_string(),
            label: "arcana".to_string(),
            token: StatFamily::Neutral.token(),
            magnitude: None,
            duration: None,
            mechanic: None,
            base: format!("{}s", format_precision(arcana, 2)),
            enhanced: None,
            enhanced_final: None,
            enhanced_changed: false,
            final_changed: false,
        });
    }
    rows
}

/// The target's level relative to the caster, as a player says it: `+4`, `−1`, `even`.
fn level_gap_label(level_diff: i32) -> String {
    match level_diff {
        0 => "even".to_string(),
        diff if diff > 0 => format!("+{diff}"),
        diff => format!("−{}", diff.unsigned_abs()),
    }
}

/// The execution rows grouped under the headings a player reads them for, in the order those
/// headings appear.
///
/// A group with no rows never renders, and a row whose key [`execution_group`] does not place
/// lands under "Other" rather than being dropped or folded into whichever group is first — an
/// unplaced row is a gap in this mapping, and Rule 1's habit is to show it where someone will
/// see it.
fn execution_groups(rows: Vec<EffectRow>) -> Vec<EffectSection> {
    const ORDER: [&str; 3] = ["Cost and timing", "Reach", "Other"];
    ORDER
        .iter()
        .filter_map(|title| {
            let rows: Vec<EffectRow> = rows
                .iter()
                .filter(|row| execution_group(&row.key) == *title)
                .cloned()
                .collect();
            (!rows.is_empty()).then_some(EffectSection { title, rows })
        })
        .collect()
}

/// Which heading an execution row belongs under.
///
/// Grouped by what a player is asking when they look — *what does this cost me to fire* versus
/// *who can I hit with it* — rather than by whether an enhancement aspect reaches the row, which
/// is what the block used to split on. The old split put cast time in a different list from the
/// recharge it is read against, and named its groups after the enhancement system rather than
/// after anything the reader wanted.
///
/// Lives here rather than in `hand-data/effect-registry.json` for the same reason
/// [`section_title`] does: this is a choice about how one surface arranges rows, not a rule for
/// resolving them, and the registry is the home of the latter. It keys on registry effect keys —
/// schema vocabulary, never power or set names (Rule 0).
fn execution_group(key: &str) -> &'static str {
    match key {
        "enduranceCost" | "recharge" | "castTime" | "arcanaTime" | "buffDuration"
        | "effectDuration" => "Cost and timing",
        "accuracy" | "hitChance" | "range" | "radius" | "arc" | "maxTargets" => "Reach",
        _ => "Other",
    }
}

/// The word under an execution tile, where the registry's own label is wrong for the shape.
///
/// The registry speaks Mids' abbreviations — `End Cost`, `Rech Time`, `Activation`, `Pwr Range` —
/// which were chosen for a dense label column where the word has to disambiguate itself from
/// eleven neighbours. Under a number, with a heading already saying which group it is in, the
/// qualifier is the part that has stopped earning its space: "Cost and timing / 5.2 end" says
/// everything "End Cost" did, and the shorter word lets the number be the thing read first.
///
/// `None` keeps the registry's label, which is the right answer for every row this list does not
/// name — declining to abbreviate costs nothing, and inventing a word for a key nobody has looked
/// at is how a label drifts from what the registry says the row is.
fn tile_label(key: &str) -> Option<&'static str> {
    match key {
        "enduranceCost" => Some("end"),
        "recharge" => Some("recharge"),
        "castTime" => Some("cast"),
        "accuracy" => Some("accuracy"),
        "range" => Some("range"),
        "radius" => Some("radius"),
        "arc" => Some("arc"),
        "maxTargets" => Some("max targets"),
        "buffDuration" => Some("duration"),
        "effectDuration" => Some("effect dur"),
        _ => None,
    }
}

/// The engine's per-power damage as display rows. `None` for a power with nothing to say about
/// damage at all — no component resolved and none left unresolved, which is a power that deals
/// none rather than one whose damage is unknown.
fn damage_view(
    damage: &PowerDamage,
    ceiling: Option<f64>,
    timing: DamageTiming,
) -> Option<DamageView> {
    if damage.components.is_empty() && damage.unresolved.is_empty() {
        return None;
    }
    // Folding this power in is the safety valve, not the scale: the ceiling already covers every
    // power in the build's own sets, so a hit that exceeds it came from somewhere the catalogue
    // does not reach (a power whose damage the ceiling pass could not resolve). Widening rather
    // than clamping means nothing is ever shown a full bar that is really an overflow.
    let reference = ceiling.unwrap_or(0.0).max(damage.r#final).max(0.0);
    let (certain, conditional): (Vec<_>, Vec<_>) = damage
        .components
        .iter()
        .partition(|component| component.is_certain());

    // A single certain component IS the total, and repeating its numbers on a second row reads
    // as a second hit rather than as the same one summed. A ranged hit's total states both
    // ends — the floor alone would read as the whole answer.
    let total = (certain.len() > 1).then(|| EffectRow {
        key: "damageTotal".to_string(),
        label: "Total".to_string(),
        token: StatFamily::for_effect("damage", EffectCategory::Damage).token(),
        magnitude: None,
        duration: None,
        mechanic: None,
        base: span_text(damage.base, damage.high.map(|high| high.base)),
        enhanced: Some(span_text(
            damage.enhanced,
            damage.high.map(|high| high.enhanced),
        )),
        enhanced_final: Some(span_text(
            damage.r#final,
            damage.high.map(|high| high.r#final),
        )),
        enhanced_changed: (damage.enhanced - damage.base).abs() > 0.001,
        final_changed: (damage.r#final - damage.enhanced).abs() > 0.001,
    });

    // The hero line: the hit this build lands, the hit the game ships, and the gap between them
    // as one figure. Every piece is read off the SAME tiers the bar draws, so the words and the
    // lengths can never disagree.
    let headline = span_text(damage.r#final, damage.high.map(|high| high.r#final));
    let base_text = span_text(damage.base, damage.high.map(|high| high.base));
    let delta = damage_delta(damage.base, damage.r#final);

    // The chips, off the same components the rows and the bar are: a type earns one by being on
    // a component that can actually land. `Special` is excluded here as it is in the
    // classification — it names no element and has no hue to draw.
    let mut types: Vec<String> = damage
        .components
        .iter()
        .filter(|component| component.application != DamageApplication::Dormant)
        .map(|component| component.damage_type.clone())
        .filter(|damage_type| damage_type != "Special")
        .collect();
    types.sort();
    types.dedup();

    Some(DamageView {
        headline,
        base_text,
        delta,
        types,
        high: damage.high,
        timing,
        tiers: ThreeTier {
            base: damage.base,
            enhanced: damage.enhanced,
            r#final: damage.r#final,
        },
        reference,
        certain: certain
            .iter()
            .enumerate()
            .map(|(index, c)| damage_row(index, c, ""))
            .collect(),
        conditional: conditional
            .iter()
            .enumerate()
            .map(|(index, component)| damage_row(index, component, &conditional_note(component)))
            .collect(),
        situational: damage
            .unresolved
            .iter()
            .filter(|unresolved| !unresolved.waits_on_rank)
            .filter_map(|unresolved| unresolved.if_it_lands.as_ref())
            .enumerate()
            .map(|(index, component)| {
                // Signed, because the row is added on top of the hit above it — an unsigned
                // number beside the headline reads as a second, separate hit.
                let mut row = damage_row(index, component, &conditional_note(component));
                row.base = format!("+{}", row.base);
                for value in [&mut row.enhanced, &mut row.enhanced_final]
                    .into_iter()
                    .flatten()
                {
                    value.insert(0, '+');
                }
                row
            })
            .collect(),
        waiting_on_rank: damage
            .unresolved
            .iter()
            .filter(|unresolved| unresolved.waits_on_rank)
            .count(),
        unresolved: damage
            .unresolved
            .iter()
            .filter(|unresolved| !unresolved.waits_on_rank && unresolved.if_it_lands.is_none())
            // The gate stays in the line, and the mechanic's name joins it rather than replacing
            // it. Both are load-bearing and neither substitutes for the other: the name says
            // WHICH component this is, and the gate is the only thing telling two components of
            // the same mechanic apart — Char's two Containment components differ solely in
            // whether their gate names a critter or a player, so a line carrying the name alone
            // prints the same sentence twice.
            .map(|unresolved| {
                let named = mechanic_names(&unresolved.damage_type, &unresolved.tags)
                    .map(|mechanic| format!(" ({mechanic})"))
                    .unwrap_or_default();
                match unresolved.gate.is_empty() {
                    true => format!(
                        "{} damage{} is unresolved — {}",
                        unresolved.damage_type, named, unresolved.reason
                    ),
                    false => format!(
                        "{} damage{} where “{}” is unresolved — {}",
                        unresolved.damage_type, named, unresolved.gate, unresolved.reason
                    ),
                }
            })
            .collect(),
        total,
        capped: damage.capped,
    })
}

/// The damage headline's delta — the final hit against the shipped one, as one percentage.
///
/// `None` where there is no gap to state (nothing moved the hit, so the figure would be `+0%`,
/// which is noise) and where there is no gap to state it AGAINST: a zero base has no percentage,
/// and dividing by it would print `inf%`.
///
/// The sign is written rather than left to the formatter, so a debuffed hit reads `−18%` with
/// the same typographic minus the rest of the panel uses rather than the ASCII hyphen a
/// negative number carries.
fn damage_delta(base: f64, r#final: f64) -> Option<String> {
    if base <= 0.0 || (r#final - base).abs() <= 0.001 {
        return None;
    }
    let ratio = (r#final / base - 1.0) * 100.0;
    Some(format!(
        "{}{}%",
        if ratio >= 0.0 { "+" } else { "−" },
        format_precision(ratio.abs(), 0)
    ))
}

/// Why a component sits outside the total: a roll it has to win, or the export shipping it
/// inert. Stated on the row itself, because a damage number a reader cannot tell apart from a
/// certain one is worse than no row.
fn conditional_note(component: &DamageComponent) -> String {
    match component.application {
        DamageApplication::Always => String::new(),
        DamageApplication::Chance(probability) => {
            format!("{}% chance", format_precision(probability * 100.0, 1))
        }
        DamageApplication::Dormant => "inactive".to_string(),
    }
}

/// The mechanic a component belongs to, as the game names it — the effect group's authored tags,
/// printed verbatim. A bare "10% chance" says a roll happens and nothing about what it is;
/// `CritLarge · ScrapperCrit_ST` says which roll, and `FieryEmbrace` on an inert component says
/// what would wake it.
///
/// Nothing decides which tags deserve showing: the export labels flavour (`Bleed`) with the same
/// field it labels mechanics, and a list of the ones that "count" is a table of game proper nouns
/// (Rule 0). The one omission is a tag that repeats the row's own damage-type label — `Lethal` on
/// the Lethal row is the label printed twice, not a second fact.
///
/// Empty on Rebirth and Thunderspy, whose parser has no effect group to carry a tag: those rows
/// read exactly as they did before rather than naming a mechanic inferred from the archetype.
fn mechanic_names(damage_type: &str, tags: &[String]) -> Option<String> {
    let named: Vec<&str> = tags
        .iter()
        .map(String::as_str)
        .filter(|tag| !tag.eq_ignore_ascii_case(damage_type))
        .collect();
    (!named.is_empty()).then(|| named.join(" · "))
}

/// A tier's display text: one number, or "low–high" when the component's magnitude program
/// ranges over a circumstance register.
fn span_text(low: f64, high: Option<f64>) -> String {
    match high {
        Some(high) => format!("{}–{}", format_precision(low, 2), format_precision(high, 2)),
        None => format_precision(low, 2),
    }
}

/// One damage component as a row. A ticking component reads in its own terms — the per-tick
/// figure with the tick count beside it — because a fire patch stating only its total looks
/// like a hit that never lands. A ranged component states both endpoints in every tier and
/// names the register they range over, in the export's own word.
///
/// `index` is the row's place in its list, and it leads the key: nothing else on a component is
/// unique. The Lotus Drops carries two Fiery Embrace Fire rows, a hit and its tick, alike in
/// type, table and mechanic, and a repeated key took down the renderer.
fn damage_row(index: usize, component: &DamageComponent, note: &str) -> EffectRow {
    let mut annotations: Vec<String> = Vec::new();
    if !note.is_empty() {
        annotations.push(note.to_string());
    }
    if let Some(spread) = &component.spread {
        annotations.push(format!("varies with {}", spread.over));
    }
    if let Some(over_time) = component.over_time {
        annotations.push(format!(
            "{} × {} over {}s",
            span_text(
                component.base,
                component.spread.as_ref().map(|spread| spread.base)
            ),
            format_precision(over_time.expected_ticks, 2),
            format_precision(over_time.duration, 2),
        ));
        if let Some(chance) = over_time.tick_chance {
            annotations.push(format!(
                "{}% per tick of {}{}",
                format_precision(chance * 100.0, 1),
                format_precision(over_time.nominal_ticks, 0),
                if over_time.cancel_on_miss {
                    ", stopping on a miss"
                } else {
                    ""
                },
            ));
        }
    }
    let ThreeTier {
        base,
        enhanced,
        r#final,
    } = component.total;
    let high = component.spread.as_ref().map(|spread| spread.total);
    // The mechanic is part of the key: an attack's base component and its critical read the same
    // damage type off the same table, and against a player Beheader ships both.
    let mechanic = mechanic_names(&component.damage_type, &component.tags);
    EffectRow {
        key: format!(
            "damage:{index}:{}:{}:{}",
            component.damage_type,
            component.table,
            mechanic.as_deref().unwrap_or_default()
        ),
        label: component.damage_type.clone(),
        token: StatFamily::for_effect("damage", EffectCategory::Damage).token(),
        magnitude: (!annotations.is_empty()).then(|| annotations.join(" · ")),
        duration: None,
        mechanic,
        base: span_text(base, high.map(|high| high.base)),
        enhanced: Some(span_text(enhanced, high.map(|high| high.enhanced))),
        enhanced_final: Some(span_text(r#final, high.map(|high| high.r#final))),
        enhanced_changed: (enhanced - base).abs() > 0.001,
        final_changed: (r#final - enhanced).abs() > 0.001,
    }
}

/// The remaining rows, grouped into the four player-facing sections.
fn effect_sections(
    magnitudes: &[GrantedMagnitude],
    headline: Option<&BTreeSet<&'static str>>,
) -> Vec<EffectSection> {
    // Order is the sections' own, not the registry's: mez first because it decides whether a
    // power lands at all, special last because it is the least comparable.
    const ORDER: [&str; 5] = ["Mez", "Buffs", "Debuffs", "Damage", "Special"];
    ORDER
        .iter()
        .filter_map(|title| {
            let rows: Vec<EffectRow> = magnitudes
                .iter()
                .filter(|magnitude| section_title(magnitude.category) == Some(title))
                // `None` is the full face. `Some` is the authored summary's own list, and an
                // empty one means it named nothing — which promotes nothing, not everything.
                .filter(|magnitude| {
                    headline.is_none_or(|keys| keys.contains(magnitude.effect_key.as_str()))
                })
                .map(effect_row)
                .collect();
            (!rows.is_empty()).then_some(EffectSection { title, rows })
        })
        .collect()
}

/// Which section a registry category renders under. `None` is the execution block, which
/// [`execution_rows`] owns. Exhaustive — a new category has to be placed, not defaulted.
fn section_title(category: EffectCategory) -> Option<&'static str> {
    match category {
        EffectCategory::Execution => None,
        EffectCategory::Control => Some("Mez"),
        EffectCategory::Buff | EffectCategory::Protection | EffectCategory::Movement => {
            Some("Buffs")
        }
        EffectCategory::Debuff => Some("Debuffs"),
        EffectCategory::Damage => Some("Damage"),
        EffectCategory::Special => Some("Special"),
    }
}

fn effect_row(granted: &GrantedMagnitude) -> EffectRow {
    let config = effect_registry::lookup(&granted.effect_key);
    // A DURATION-valued mez row's tiers are SECONDS — the magnitude rides beside them in
    // `quantity`, not in the value — so it reads in its own unit rather than the `mag` its
    // registry format names. A MAGNITUDE-valued one keeps the registry's `Mag`, because there
    // the tiers ARE the rank. Nothing else overrides: the row's format already wins over the
    // config's where the engine set one (an absorb authored as a fraction of Max HP displays
    // as a percent).
    let unit = match granted.quantity {
        GrantedQuantity::MezDuration { .. } => EffectFormat::Duration,
        GrantedQuantity::Value
        | GrantedQuantity::MezMagnitude
        | GrantedQuantity::MezExpression
        | GrantedQuantity::MezConstant
        | GrantedQuantity::MezUnstated
        | GrantedQuantity::Distance => granted.format,
    };
    let precision = config
        .and_then(|config| config.precision)
        .unwrap_or_else(|| unit.default_precision());
    let label = match &granted.by_type_label {
        Some(types) => format!("{} ({types})", granted.label),
        None => granted.label.clone(),
    };
    // A mez's duration and a knock's distance are scaled by the bonus the EFFECT key names, so
    // they are enhanceable even though their configs declare no aspect (mag-format effects
    // don't). Everything else is enhanceable exactly when its config says so — unless the row's
    // own source template ignores Strength, which overrides the key's rule for that row alone
    // (ENT-4). The em dash the two enhanced columns then show is the honest rendering: the game
    // writes "Ignores Buffs and Enhancements" under such a row rather than repeating the number.
    let enhanceable = !granted.ignores_strength
        && match granted.quantity {
            GrantedQuantity::MezDuration { .. }
            | GrantedQuantity::MezMagnitude
            | GrantedQuantity::Distance => true,
            GrantedQuantity::MezExpression
            | GrantedQuantity::MezConstant
            | GrantedQuantity::MezUnstated => false,
            GrantedQuantity::Value => config
                .map(|config| config.enhancement_aspect.is_some())
                .unwrap_or(false),
        };
    let format = |value: f64| format_effect_value(value, unit, precision);
    let ThreeTier {
        base,
        enhanced,
        r#final,
    } = granted.value;

    EffectRow {
        key: granted.row_key.clone(),
        label,
        token: StatFamily::for_effect(&granted.effect_key, granted.category).token(),
        magnitude: match granted.quantity {
            GrantedQuantity::MezDuration { magnitude } => {
                Some(format!("Mag {}", format_precision(magnitude, 1)))
            }
            GrantedQuantity::Value
            | GrantedQuantity::MezMagnitude
            | GrantedQuantity::MezExpression
            | GrantedQuantity::MezConstant
            | GrantedQuantity::MezUnstated
            | GrantedQuantity::Distance => None,
        },
        // The effective duration, when the engine row carries one and the tier is not already
        // the seconds. `MezDuration` rows are excluded by the engine itself (their tier IS the
        // rank's seconds), and Distance rows with a `durations` record are as absent, so this
        // renders `(8s)` beside exactly the rows whose rank left the seconds out (ENGLAG-2) and
        // stays silent everywhere the number is already the value.
        duration: match granted.quantity {
            GrantedQuantity::MezDuration { .. } => None,
            _ => granted
                .duration
                .map(|d| format!("({}s)", format_precision(d, 1))),
        },
        mechanic: None,
        // Three mez rows carry no number, and they say so differently on purpose. An
        // expression-valued one is working as designed — Inner Will's magnitude is whatever
        // is mezzing you — so it reads as a state a player can accept. An unstated one is a
        // converter regression, and reading like a defect is the point (MEZDUR-1). A
        // constant-valued one is a game state we have never seen on a mez row and therefore
        // do not resolve; it reads as a defect for the same reason, because a mez row
        // reaching here means the population the gate pins at zero has grown one.
        base: match granted.quantity {
            GrantedQuantity::MezExpression => "Varies".to_string(),
            GrantedQuantity::MezConstant => "constant".to_string(),
            GrantedQuantity::MezUnstated => "unstated".to_string(),
            _ => format(base),
        },
        enhanced: enhanceable.then(|| format(enhanced)),
        enhanced_final: enhanceable.then(|| format(r#final)),
        enhanced_changed: (enhanced - base).abs() > 0.001,
        final_changed: (r#final - enhanced).abs() > 0.001,
    }
}

/// The post-ED slotted bonuses, as the beta's summary reads them: aspect name, percent.
fn enhancement_bonus_rows(projection: &PowerProjection) -> Vec<(String, String)> {
    projection
        .enhancement_bonuses
        .iter()
        .filter(|(_, bonus)| **bonus != 0.0)
        .map(|(aspect, bonus)| {
            (
                capitalized(aspect),
                format!("+{}%", format_precision(bonus * 100.0, 2)),
            )
        })
        .collect()
}

/// Split an authored description into paragraphs, dropping the game client's markup.
///
/// The bins carry the description exactly as the client renders it — `<br>` breaks and
/// `<color #fcfc95>` runs — and the converter resolves that markup before the wire
/// (`scripts/_display-text.cjs` `helpText`), so a break normally arrives as a newline. The
/// tag arm stays because `<br>` is what this field held for as long as the converter passed
/// it through, and a wire still carrying one must not read as a single run. A `<br><br>` and
/// a blank line are the same authored paragraph break; every other tag is presentation.
fn paragraphs(description: &str) -> Vec<String> {
    /// Accumulate text, ending the current paragraph at every newline in it.
    fn push_text(paragraphs: &mut Vec<String>, current: &mut String, text: &str) {
        let mut lines = text.split('\n');
        if let Some(first) = lines.next() {
            current.push_str(first);
        }
        for line in lines {
            paragraphs.push(std::mem::take(current));
            current.push_str(line);
        }
    }

    let mut paragraphs = Vec::new();
    let mut current = String::new();
    let mut rest = description;

    while let Some(open) = rest.find('<') {
        push_text(&mut paragraphs, &mut current, &rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            // An unterminated `<` is text, not a tag — keep it rather than eating the tail.
            push_text(&mut paragraphs, &mut current, &rest[open..]);
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + close];
        if tag.trim_end_matches('/').trim().eq_ignore_ascii_case("br") {
            paragraphs.push(std::mem::take(&mut current));
        }
        rest = &rest[open + close + 1..];
    }
    push_text(&mut paragraphs, &mut current, rest);
    paragraphs.push(current);

    paragraphs
        .into_iter()
        .map(|paragraph| paragraph.trim().to_string())
        .filter(|paragraph| !paragraph.is_empty())
        .collect()
}

/// An aspect key as a label — the beta's `capitalize` on the same keys.
fn capitalized(aspect: &str) -> String {
    let mut chars = aspect.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Round to `max_decimals`, then strip trailing zeros — the beta `formatPrecision`.
///
/// Rounds halves away from zero (`f64::round`) rather than to even, because the beta's
/// `toFixed` does and CoH scales are full of exact halves at display precision (a `0.125`
/// would otherwise read `0.12` here and `0.13` there).
fn format_precision(value: f64, max_decimals: u8) -> String {
    let factor = 10f64.powi(max_decimals as i32);
    let rounded = (value * factor).round() / factor;
    // JS `parseFloat().toString()` normalizes -0 to "0"; Rust's Display keeps the sign.
    if rounded == 0.0 {
        return "0".to_string();
    }
    format!("{rounded}")
}

/// A resolved value in its display unit — the beta `formatEffectValueForConfig`.
fn format_effect_value(value: f64, format: EffectFormat, precision: u8) -> String {
    match format {
        EffectFormat::Percent => format!("{}%", format_precision(value, precision)),
        EffectFormat::Duration => format!("{}s", format_precision(value, precision)),
        EffectFormat::Mag => format!("Mag {}", format_precision(value, precision)),
        EffectFormat::Scale => format!("{} scale", format_precision(value, precision)),
        EffectFormat::Degrees => format!("{}°", format_precision(value, 0)),
        // A distance reads in whole feet, and the registry is what says a key IS one (PR8).
        // This used to be `Value if label == "Range" || label == "Radius"` — a display rule
        // hiding in a string comparison against a label, which is the half of the registry
        // that gets respelled. `range`'s label is `Pwr Range`, so it never matched and a
        // power's range printed a bare `80` beside a radius printing `20ft`; the comment on
        // the test pinning that called it deliberate.
        EffectFormat::Distance => format!("{}ft", format_precision(value, 0)),
        EffectFormat::Value | EffectFormat::Damage | EffectFormat::Custom => {
            format_precision(value, precision)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use coh_data::{DatasetId, PowerDatabase, PowersetSelection, SelectedPower};
    use std::path::PathBuf;

    fn database(dataset: DatasetId) -> PowerDatabase {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contract")
            .join(dataset.as_str())
            .join("bundle.json.gz");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        PowerDatabase::from_gz_bytes(&bytes).expect("bundle loads")
    }

    const FORKS: [DatasetId; 4] = [
        DatasetId::Homecoming,
        DatasetId::Rebirth,
        DatasetId::Thunderspy,
        DatasetId::Brainstorm,
    ];

    /// A build holding one whole powerset, which is what makes a set's caster-state controls
    /// reachable — a stance group is published by a picked PARENT power, so picking the set
    /// piecemeal would leave half the groups unfound.
    fn whole_set(dataset: DatasetId, set: &coh_data::Powerset) -> Option<CharacterState> {
        let archetype = set.archetype.clone()?;
        let mut state = CharacterState::empty(dataset);
        state.level = 50;
        state.archetype.id = Some(archetype);
        state.primary = PowersetSelection {
            id: Some(set.id.clone()),
            powers: set
                .powers
                .iter()
                .map(|power| SelectedPower::picked(power.ident().to_string(), &set.id, 1))
                .collect(),
            ..PowersetSelection::default()
        };
        Some(state)
    }

    /// One graded power: what the card would DRAW as chips, what its live components actually
    /// are, and the power-level classification the drawn set falls back to.
    struct Chips {
        ident: String,
        drawn: Vec<String>,
        live: Vec<String>,
        classified: Vec<String>,
    }

    /// Every power in `state`'s set that resolved damage, read through [`PowerView::build`] —
    /// the same call the panel makes, so the fallback is graded where it actually happens rather
    /// than re-derived here.
    fn chips_by_power(
        state: &CharacterState,
        db: &PowerDatabase,
        set: &coh_data::Powerset,
    ) -> Vec<Chips> {
        let totals = coh_math::recalculate(state, db);
        let mut out = Vec::new();
        for power in set.powers.iter() {
            let ident = power.ident().to_string();
            let Some(projection) = totals
                .power_projection
                .iter()
                .find(|row| row.power_internal_name == ident)
            else {
                continue;
            };
            let view = PowerView::build(power, Some(projection), None);
            let Some(damage) = view.damage else { continue };
            let mut live: Vec<String> = projection
                .damage
                .components
                .iter()
                .filter(|component| component.application != DamageApplication::Dormant)
                .map(|component| component.damage_type.clone())
                .filter(|damage_type| damage_type != "Special")
                .collect();
            live.sort();
            live.dedup();
            out.push(Chips {
                ident,
                drawn: damage.types,
                live,
                classified: view.damage_types.unwrap_or_default(),
            });
        }
        out
    }

    /// The chips name the damage the headline is MADE of, on every power of every fork
    /// (CHIPTYPE-1).
    ///
    /// **What this grades.** That [`DamageView::types`] is the live components' own type set —
    /// so a chip can never name an element the number beside it does not contain, and can never
    /// omit one it does. The defect it replaces read the power's UNGATED atoms instead, a
    /// classification that is blind to the build: with a Dual Pistols ammo loaded the card read
    /// "Lethal 68.4" over a hit that was entirely Cold.
    ///
    /// **What it cannot see.** It consumes the contract and the engine's own component list, so
    /// a component carrying the WRONG type is invisible here — this grades agreement between two
    /// surfaces, not either one's truth.
    ///
    /// **Tier.** Per-run. Four bundle loads and one recalculate per powerset.
    #[test]
    fn the_chips_name_what_the_headline_is_made_of() {
        for dataset in FORKS {
            let db = database(dataset);
            let mut graded = 0usize;
            for set in db.powersets.iter() {
                let Some(state) = whole_set(dataset, set) else {
                    continue;
                };
                for power in chips_by_power(&state, &db, set) {
                    if power.live.is_empty() {
                        continue;
                    }
                    graded += 1;
                    assert_eq!(
                        power.drawn, power.live,
                        "{dataset:?} {}/{}: the chips should name the components the headline is \
                         made of",
                        set.id, power.ident,
                    );
                }
            }
            // Non-vacuity, per fork: a sweep that graded nothing passes for the wrong reason.
            assert!(
                graded > 1000,
                "{dataset:?}: only {graded} powers reached the assertion",
            );
        }
    }

    /// A power whose damage is ALL inert keeps the power-level classification, so a card is
    /// never left with a number and no label (CHIPTYPE-1).
    ///
    /// Every fork ships these — damage waiting on something outside the attack — and they are
    /// the one case the live read cannot answer, which is why the fallback exists at all. Graded
    /// by the property (no live component) rather than by naming a power.
    #[test]
    fn a_wholly_inert_power_falls_back_to_the_classification() {
        let mut graded = 0usize;
        for dataset in FORKS {
            let db = database(dataset);
            for set in db.powersets.iter() {
                let Some(state) = whole_set(dataset, set) else {
                    continue;
                };
                for power in chips_by_power(&state, &db, set) {
                    if !power.live.is_empty() {
                        continue;
                    }
                    graded += 1;
                    assert_eq!(
                        power.drawn, power.classified,
                        "{dataset:?} {}/{}: a power with no live component should draw the \
                         power-level classification rather than nothing",
                        set.id, power.ident,
                    );
                }
            }
        }
        assert!(
            graded > 20,
            "only {graded} wholly-inert powers reached the assertion",
        );
    }

    /// Loading a caster mode that swaps a power's damage element swaps its chips with it
    /// (CHIPTYPE-1) — the reported symptom, graded end to end.
    ///
    /// Selected by the PROPERTY: a stance option whose conditional changes which damage types
    /// land. No powerset, power or ammo is named here; the sweep finds them, and the assertion
    /// messages print whatever the data gave.
    #[test]
    fn a_caster_mode_that_swaps_the_damage_element_swaps_the_chips() {
        let mut moved = 0usize;
        for dataset in FORKS {
            let db = database(dataset);
            for set in db.powersets.iter() {
                let Some(base_state) = whole_set(dataset, set) else {
                    continue;
                };
                let groups = coh_data::caster_state::stance_groups(&base_state, &db);
                if groups.is_empty() {
                    continue;
                }
                let before = chips_by_power(&base_state, &db, set);
                for group in &groups {
                    for option in &group.options {
                        let mut state = base_state.clone();
                        coh_data::caster_state::set_stance(
                            &mut state,
                            group,
                            Some(&option.internal_name),
                        );
                        for power in chips_by_power(&state, &db, set) {
                            if power.live.is_empty() {
                                continue;
                            }
                            // The invariant holds under every caster state, not just the default.
                            assert_eq!(
                                power.drawn, power.live,
                                "{dataset:?} {}/{} under {}: the chips should follow the \
                                 components",
                                set.id, power.ident, option.internal_name,
                            );
                            let was = before
                                .iter()
                                .find(|row| row.ident == power.ident)
                                .map(|row| row.drawn.clone())
                                .unwrap_or_default();
                            if was != power.drawn && !was.is_empty() {
                                moved += 1;
                            }
                        }
                    }
                }
            }
        }
        // Non-vacuity, and the reported defect's own population: the mechanism has to be
        // REACHED, not merely consistent where it never fires.
        assert!(
            moved > 0,
            "no caster mode changed any power's chips — the swap this grades was not reached",
        );
        println!("chips moved with a caster mode on {moved} power/stance pairs");
    }

    /// Every list the card draws keys its rows uniquely, on every power of every fork.
    ///
    /// The renderer diffs these lists by key. A repeated key is a debug-build assertion and,
    /// in a release build, a corrupted diff that takes the whole app down the next time the list
    /// changes; every later click still commits and saves, but nothing redraws. The Lotus Drops
    /// hit it with two Fiery Embrace Fire rows that agreed in type, table and mechanic.
    ///
    /// **What it cannot see.** Lists outside [`PowerView`] (the slotted procs, the combat
    /// modes) are keyed elsewhere and are not graded here.
    #[test]
    fn every_drawn_list_keys_its_rows_uniquely() {
        for dataset in FORKS {
            let db = database(dataset);
            let mut graded = 0usize;
            for set in db.powersets.iter() {
                let Some(state) = whole_set(dataset, set) else {
                    continue;
                };
                let totals = coh_math::recalculate(&state, &db);
                for power in set.powers.iter() {
                    let ident = power.ident();
                    let projection = totals
                        .power_projection
                        .iter()
                        .find(|row| row.power_internal_name == ident);
                    let view = PowerView::build(power, projection, None);
                    let mut lists: Vec<(&str, &[EffectRow])> = Vec::new();
                    if let Some(damage) = &view.damage {
                        lists.push(("damage", &damage.certain));
                        lists.push(("on a roll", &damage.conditional));
                        lists.push(("situational", &damage.situational));
                    }
                    for section in view.sections.iter().chain(&view.execution_groups) {
                        lists.push((section.title, &section.rows));
                    }
                    for (title, rows) in lists {
                        graded += rows.len();
                        let mut seen = std::collections::BTreeSet::new();
                        for row in rows {
                            assert!(
                                seen.insert(row.key.as_str()),
                                "{dataset:?} {}/{ident}: the {title} list repeats the key {:?}",
                                set.id,
                                row.key,
                            );
                        }
                    }
                }
            }
            assert!(
                graded > 1000,
                "{dataset:?}: only {graded} rows reached the assertion"
            );
        }
    }

    /// A +Max End row reads in endurance points, the unit the totals sheet shows Max Endurance
    /// in. The atoms are absolute points on a `*_Ones` table, so the registry's old `percent`
    /// format multiplied them by 100: Personal Force Field's +5 drew as `500%`.
    ///
    /// **What it cannot see.** Whether the points themselves are right — that is the engine's
    /// answer, graded by the totals replay.
    #[test]
    fn max_end_reads_in_points() {
        let mut graded = 0usize;
        for dataset in FORKS {
            let db = database(dataset);
            for set in db.powersets.iter() {
                let Some(state) = whole_set(dataset, set) else {
                    continue;
                };
                let totals = coh_math::recalculate(&state, &db);
                for power in set.powers.iter() {
                    let Some(projection) = totals
                        .power_projection
                        .iter()
                        .find(|row| row.power_internal_name == power.ident())
                    else {
                        continue;
                    };
                    let view = PowerView::build(power, Some(projection), None);
                    for row in view.sections.iter().flat_map(|section| &section.rows) {
                        if row.key != "maxEndBuff" {
                            continue;
                        }
                        graded += 1;
                        let points: f64 = row.base.parse().unwrap_or_else(|_| {
                            panic!(
                                "{dataset:?} {}/{}: +Max End should read in points, drew {:?}",
                                set.id,
                                power.ident(),
                                row.base,
                            )
                        });
                        // The whole pool is 100 points; no single power adds more than that.
                        assert!(
                            points.abs() <= 100.0,
                            "{dataset:?} {}/{}: +Max End {points} is past the whole pool",
                            set.id,
                            power.ident(),
                        );
                    }
                }
            }
        }
        assert!(graded > 0, "no +Max End row reached the assertion");
        eprintln!("graded {graded} +Max End rows");
    }
}
