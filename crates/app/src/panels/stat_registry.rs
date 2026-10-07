//! The dashboard's stat vocabulary — the one place a stat's identity, label, colour family,
//! section, formatting, and engine read are written down. The stat panels
//! ([`crate::panels::stats`]), the config modal ([`crate::panels::stats_config`]), and the
//! persisted visibility set ([`crate::stats_store`]) all derive from this, so moving a stat
//! between sections or retuning its colour is a one-line change here rather than three files
//! kept in sync by hand — the beta's own lesson, single-sourced there as `STAT_SECTIONS` /
//! `STAT_CATEGORY` / `STAT_COLORS` after the same drift.
//!
//! **This is display vocabulary, not game data.** Nothing here decides a number; every
//! [`StatDef::read`] pulls a field the engine already computed off [`CalculatedTotals`]. No stat
//! is named in a conditional anywhere else in the UI — surfaces iterate [`StatDef::ALL`].
//!
//! Two deliberate absences, both engine gaps rather than choices — a toggle that always renders
//! zero is a soft-wrong number presented as authoritative, so these stay out
//! of the vocabulary until the engine projects them:
//!
//! - **Threat** has no `GlobalBonuses` field at all: nothing in the engine accumulates a threat
//!   level. (End Cost and Net End were absent for the neighbouring reason — no summed toggle
//!   cost to discount — until Step 9.7 landed the pass; both are in the vocabulary now.)
//!
//! Teleport resistance was listed here for the same reason until MEZRES-3 closed: roughly half
//! its carriers aim it at the power's victim rather than the caster (Wormhole's is the foe's
//! post-yank immunity), and the field that separates the two, `targets_affected`, was parsed but
//! never emitted onto the contract. It is emitted now, the reader withholds the victim-facing
//! carriers, and the row shows the caster's own protection. Repel resistance, `movementControl`
//! and `movementFriction` were absent for the plainer reason — no field to read — and all four
//! are in the vocabulary now.
//!
//! Movement reads in mph (feet for jump height), the units the game itself shows. The bases and
//! per-level ceilings come from each archetype's own class table and the projection lives in
//! `coh_math::movement`; the TypeScript reaches the same numbers through a hand-written table of
//! constants (`src/data/core/movement-constants.ts`), which is the hardcode Rule 0 forbids.
//!
//! That table said fly 0 and jump 21 mph, both wrong — jump 21 is the ft/s figure wearing mph's
//! label, which inflates every jump readout by 21/14.32 ≈ 1.47×. This comment used to pin those
//! two numbers on the BETA. Measured 2026-09-11 (FORK-7): the beta fixed both on 2026-07-27
//! against a Longbow Jetpack reading, and the copy still carrying them is canonical's own
//! vendored oracle, which ships to nobody. Naming the wrong repo is the same failure the numbers
//! were — a convention stated without measuring it.

use coh_math::movement::MovementStat;
use coh_math::{set_bonuses, CalculatedTotals};

/// Which colour family a stat belongs to. Hue answers "what KIND of number is this" — never
/// "is it good or bad", which rides on the non-colour cues in [`crate::panels::stats`]. Each
/// maps to a `--stat-*` token so a theme can remap the whole code without touching a component.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatFamily {
    Damage,
    ToHit,
    Defense,
    Resistance,
    Health,
    Endurance,
    Movement,
    Mez,
    DebuffResist,
    Special,
    Neutral,
}

impl StatFamily {
    /// The CSS custom property carrying this family's hue.
    pub fn token(self) -> &'static str {
        match self {
            StatFamily::Damage => "var(--stat-damage)",
            StatFamily::ToHit => "var(--stat-tohit)",
            StatFamily::Defense => "var(--stat-defense)",
            StatFamily::Resistance => "var(--stat-resistance)",
            StatFamily::Health => "var(--stat-heal)",
            StatFamily::Endurance => "var(--stat-endurance)",
            StatFamily::Movement => "var(--stat-movement)",
            StatFamily::Mez => "var(--stat-mez)",
            StatFamily::DebuffResist => "var(--stat-ddr)",
            StatFamily::Special => "var(--stat-special)",
            StatFamily::Neutral => "var(--stat-neutral)",
        }
    }

    /// The family a per-power effect row belongs to — the same hues the dashboard uses, so a
    /// build's defense numbers look the same in the Info panel as in the Defense panel. The
    /// beta kept this as a `colorClass` per registry entry; the engine drops those as
    /// presentation-only ([`coh_math::effect_registry`]), so the choice lands here, beside the
    /// dashboard's, rather than in the Info panel's own file.
    ///
    /// Keyed on registry effect keys — schema vocabulary, not power names (Rule 0). The
    /// category is the fallback for keys the list doesn't name, which is why every arm below is
    /// one whose family its category can't imply.
    pub fn for_effect(
        effect_key: &str,
        category: coh_math::effect_registry::EffectCategory,
    ) -> Self {
        use coh_math::effect_registry::EffectCategory;
        match effect_key {
            "damageBuff" | "damageDebuff" => StatFamily::Damage,
            "tohitBuff" | "tohitDebuff" | "accuracy" | "accuracyBuff" | "accuracyDebuff"
            | "range" | "rangeBuff" | "perceptionBuff" | "perceptionDebuff" => StatFamily::ToHit,
            "defense" | "defenseBuff" | "defenseBuffSuppressible" | "defenseDebuff" => {
                StatFamily::Defense
            }
            "resistance" | "resistanceDebuff" => StatFamily::Resistance,
            "healing" | "absorb" | "maxHPBuff" | "regenBuff" | "regenDebuff" => StatFamily::Health,
            "enduranceCost" | "enduranceDiscount" | "enduranceGain" | "enduranceDrain"
            | "enduranceCrash" | "maxEndBuff" | "recoveryBuff" | "recoveryDebuff" => {
                StatFamily::Endurance
            }
            "speedBuff" | "slow" => StatFamily::Movement,
            "elusivity" | "debuffResistance" => StatFamily::DebuffResist,
            "protection" | "mezResistance" => StatFamily::Mez,
            "specialBuff" | "specialDebuff" | "summon" => StatFamily::Special,
            _ => match category {
                EffectCategory::Damage => StatFamily::Damage,
                EffectCategory::Control => StatFamily::Mez,
                EffectCategory::Protection => StatFamily::Mez,
                EffectCategory::Movement => StatFamily::Movement,
                EffectCategory::Special => StatFamily::Special,
                // Recharge, duration, area, and the rest of the execution block carry no family
                // of their own — the beta's neutral slate.
                EffectCategory::Execution | EffectCategory::Buff | EffectCategory::Debuff => {
                    StatFamily::Neutral
                }
            },
        }
    }
}

/// Which family of stat this is — the taxonomy, not a place on the screen.
///
/// It used to be both. `StatSection::panel()` mapped each variant one-to-one onto a
/// `PanelKind`, so "which kind of number is this" and "which grid surface draws it" were the
/// same fact, and a stat's surface was therefore decided at compile time. Dashboard panels are
/// user-built now — any panel holds any stats — so the second meaning moved out to
/// [`crate::panels::dashboards`], where it is user data, and this kept the first.
///
/// Which is the meaning every consumer outside the dashboard was already using: the detailed
/// totals sheet, the set-bonus totals and finder, the proc settings, the what-if modal and the
/// forum export all group BY family and head their groups with [`Self::title`]. None of them
/// ever wanted a panel. Splitting the two is what kept an arbitrary-panels change out of six
/// surfaces that have nothing to do with the grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatSection {
    Offense,
    Defense,
    Resistance,
    Survival,
    Movement,
    StatusProtection,
    StatusResistance,
    DebuffResistance,
}

impl StatSection {
    /// Every section, in dashboard order. The exhaustive match makes adding a section a
    /// compile error here rather than a silently missing block in the config modal.
    pub const ALL: [StatSection; 8] = {
        match StatSection::Offense {
            StatSection::Offense
            | StatSection::Defense
            | StatSection::Resistance
            | StatSection::Survival
            | StatSection::Movement
            | StatSection::StatusProtection
            | StatSection::StatusResistance
            | StatSection::DebuffResistance => {}
        }
        [
            StatSection::Offense,
            StatSection::Defense,
            StatSection::Resistance,
            StatSection::Survival,
            StatSection::Movement,
            StatSection::StatusProtection,
            StatSection::StatusResistance,
            StatSection::DebuffResistance,
        ]
    };

    /// The section's display name — the heading every surface that groups by family writes
    /// above the group.
    ///
    /// Eight literals of its own, where this used to borrow `self.panel().title()`. Borrowing
    /// was the right call while a section WAS a panel: one string, no way for the two to
    /// disagree. It stops being available the moment a panel's name is something the user
    /// typed, and these are the strings that borrowing produced.
    pub fn title(self) -> &'static str {
        match self {
            StatSection::Offense => "Offense",
            StatSection::Defense => "Defense",
            StatSection::Resistance => "Resistance",
            StatSection::Survival => "Survival",
            StatSection::Movement => "Movement",
            StatSection::StatusProtection => "Status Protection",
            StatSection::StatusResistance => "Status Resistance",
            StatSection::DebuffResistance => "Debuff Resistance",
        }
    }

    /// The section's key in markup — a DOM id to scroll to, a class to hang a rule off.
    ///
    /// Same story as [`Self::title`]: this used to be reached through `panel().slug()`. A
    /// section needs a stable key whether or not anything on the grid shares its name, and
    /// after the split nothing does.
    pub fn slug(self) -> &'static str {
        match self {
            StatSection::Offense => "offense",
            StatSection::Defense => "defense",
            StatSection::Resistance => "resistance",
            StatSection::Survival => "survival",
            StatSection::Movement => "movement",
            StatSection::StatusProtection => "status-protection",
            StatSection::StatusResistance => "status-resistance",
            StatSection::DebuffResistance => "debuff-resistance",
        }
    }
}

/// How a stat's number becomes text. Percentages dominate, but the engine's fields are not all
/// percentages — mez protection is a magnitude, HP and absorb are hit points, max endurance is
/// endurance POINTS, and stealth is a radius in feet. Formatting each as `%` would be a
/// legible-looking lie about the unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatFormat {
    /// `12.5%` — the plain percentage.
    Percent,
    /// `+12.5%` — a buff percentage, where the sign carries meaning and zero is the baseline.
    SignedPercent,
    /// `170%` — the CoH speed-multiplier convention Mids calls "Haste": base recharge is 100%
    /// and bonuses add on top, so a `+70%` total displays as `170%`. A display convention only;
    /// every formula still reads the underlying `+70`.
    HastePercent,
    /// `Mag 8.7` — a mez protection magnitude.
    Magnitude,
    /// `1338` — hit points (Max HP, absorb), integerised the way the game integerises them:
    /// truncated, never rounded. [`hit_points`] carries the citation.
    ///
    /// **Absorb is decided here rather than inherited.** The client never separates the two —
    /// `uiCombatNumbers.c:193` prints current HP and absorb through one `%.2f`, and where either
    /// becomes an integer it is the same `(int)` cast. No window shows HP whole and absorb
    /// fractional, so the planner does not invent one (STATFMT-1).
    HitPoints,
    /// `105` — endurance points.
    Points,
    /// `35 ft` — a radius.
    Feet,
    /// `41.5 mph` — a travel speed, as the game's own readout gives it.
    MilesPerHour,
    /// `4.32 ft` — a jump height. Separate from [`Self::Feet`] because a stealth radius is a
    /// whole number of feet while a jump height lives in its decimals.
    FeetPrecise,
    /// `0.52/s` — endurance per second, the unit the toggle drain is already in. Two decimals
    /// rather than [`trim`]'s trailing-zero strip: these numbers live in a band where the second
    /// decimal is the whole difference between a build that sustains and one that stalls.
    EndurancePerSecond,
    /// `+1.15/s` — endurance per second where the SIGN is the answer. Net endurance going
    /// negative is the single fact this row exists to report, so the `+` is what makes its
    /// absence read as a warning rather than as a smaller number.
    SignedEndurancePerSecond,
    /// `+1` — the incarnate level shift, a small non-negative count.
    LevelShift,
    /// `66.67% dur` — status RESISTANCE shown the way the in-game combat monitor shows it: the
    /// residual mez DURATION `100 / (1 + resistance/100)`, not the raw resistance. 50%
    /// resistance leaves 66.67% duration, NOT 50% — the common "cut in half" misreading. Lower
    /// is better, and 0% resistance reads as a full `100% dur`.
    MezDuration,
}

impl StatFormat {
    pub fn render(self, value: f64) -> String {
        match self {
            StatFormat::Percent => format!("{}%", trim(value)),
            StatFormat::SignedPercent => {
                format!("{}{}%", if value >= 0.0 { "+" } else { "" }, trim(value))
            }
            StatFormat::HastePercent => format!("{}%", trim(100.0 + value)),
            StatFormat::Magnitude => format!("Mag {value:.1}"),
            StatFormat::HitPoints => hit_points(value),
            StatFormat::Points => format!("{value:.0}"),
            StatFormat::Feet => format!("{value:.0} ft"),
            StatFormat::MilesPerHour => format!("{} mph", trim(value)),
            StatFormat::FeetPrecise => format!("{} ft", trim(value)),
            StatFormat::EndurancePerSecond => format!("{value:.2}/s"),
            StatFormat::SignedEndurancePerSecond => {
                format!("{}{value:.2}/s", if value >= 0.0 { "+" } else { "" })
            }
            StatFormat::LevelShift => {
                if value > 0.0 {
                    format!("+{value:.0}")
                } else {
                    "0".to_string()
                }
            }
            StatFormat::MezDuration => format!("{}% dur", trim(100.0 / (1.0 + value / 100.0))),
        }
    }

    /// A what-if MAGNITUDE rather than a total: always signed, and never through a display
    /// convention that only makes sense for a total.
    ///
    /// [`Self::HastePercent`] is the reason this exists. It shows recharge as `170%` because
    /// base recharge IS 100% — but a layer entry is the `+70` itself, so rendering it through
    /// that convention would present a +70% team buff as a 170-point one. [`Self::MezDuration`]
    /// inverts for the same reason: the residual-duration convention describes a total
    /// resistance, not an increment to one.
    pub fn render_delta(self, value: f64) -> String {
        let sign = if value > 0.0 { "+" } else { "" };
        match self {
            StatFormat::Percent
            | StatFormat::SignedPercent
            | StatFormat::HastePercent
            | StatFormat::MezDuration => format!("{sign}{}%", trim(value)),
            StatFormat::Magnitude => format!("{sign}{} Mag", trim(value)),
            StatFormat::HitPoints => format!("{sign}{}", hit_points(value)),
            StatFormat::Points | StatFormat::LevelShift => {
                format!("{sign}{value:.0}")
            }
            StatFormat::Feet | StatFormat::FeetPrecise => format!("{sign}{} ft", trim(value)),
            StatFormat::MilesPerHour => format!("{sign}{} mph", trim(value)),
            StatFormat::EndurancePerSecond | StatFormat::SignedEndurancePerSecond => {
                format!("{sign}{value:.2}/s")
            }
        }
    }
}

/// Hit points as the game itself makes them whole: a C truncating cast, never a round.
///
/// The client converts a hit-point `float` to an integer in exactly one way and nowhere else —
/// `Game/src/UI/Hybrid/uiRegister.c:763` prints max HP as
/// `itoa_with_commas_static((int)e->pchar->attrMax.fHitPoints)`, and the `kAttribStyle_Integer`
/// arm at `uiCombatNumbers.c:202` is the same `(int)`. No `floor()`, `roundf()`, `ceil()` or
/// `+0.5f` touches HP anywhere in it. So a 1621.91 total reads 1621 in game, and a planner
/// rounding it to 1622 shows a number the game never would.
///
/// `trunc()` rather than `floor()` because that is what the cast does on BOTH sides of zero, and
/// a what-if delta is the one place a hit-point value here goes negative. It is also what made
/// the two renderers disagree: `render` floored while `render_delta` sent the same stat through
/// `{:.0}`, which rounds half-to-even — so 1.5 read 2 and 2.5 read 2, and a delta in (-1, 0)
/// read `-0` (STATFMT-1).
fn hit_points(value: f64) -> String {
    let whole = value.trunc();
    // A truncated negative fraction is a zero like any other; `-0.0` would format as "-0".
    format!("{:.0}", if whole == 0.0 { 0.0 } else { whole })
}

/// Round to 2 decimals and strip trailing zeros: `0 → "0"`, `30 → "30"`, `30.5 → "30.5"`,
/// `30.55 → "30.55"`. Character totals are the planner's 2-decimal tier (the beta
/// `formatPrecision`); padding a clean 30 to "30.00" adds noise, and truncating 30.55 hides
/// precision a veteran is reading for.
fn trim(value: f64) -> String {
    let text = format!("{value:.2}");
    if !text.contains('.') {
        return text;
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Which ceiling a stat is measured against, for the at-cap cue. The engine owns every ceiling
/// (they are archetype and level dependent), so this names WHICH one applies rather than
/// carrying a number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatCap {
    /// No ceiling this stat can reach.
    None,
    /// The defense softcap. A THRESHOLD, not a clamp (D4) — defense legitimately exceeds it,
    /// so reaching it is worth marking precisely because the number keeps climbing past it.
    DefenseSoftcap,
    /// The archetype resistance ceiling, a real clamp.
    ResistanceCap,
    /// The per-level absolute HP ceiling, a real clamp.
    MaxHpCap,
    /// The per-level absorb ceiling, a real clamp.
    AbsorbCap,
    /// The travel ceiling for one axis — the class's own per-level row plus whatever the build's
    /// active travel powers raise it by. A real clamp: the engine has already applied it, so the
    /// cue is what tells the user the number stopped climbing.
    TravelCap(MovementStat),
}

impl StatCap {
    /// The ceiling's value for this build, or `None` when the stat has no cap or the engine
    /// could not supply one (a db with no archetype caps yields `0.0`, which means "unknown"
    /// — never "capped at zero").
    fn ceiling(self, totals: &CalculatedTotals) -> Option<f64> {
        let value = match self {
            StatCap::None => return None,
            StatCap::DefenseSoftcap => totals.stats.defense_softcap,
            StatCap::ResistanceCap => totals.stats.resistance_cap,
            StatCap::MaxHpCap => totals.stats.max_hp_cap,
            StatCap::AbsorbCap => totals.stats.absorb_cap,
            StatCap::TravelCap(stat) => match stat {
                MovementStat::RunSpeed => totals.stats.run_speed.cap,
                MovementStat::FlySpeed => totals.stats.fly_speed.cap,
                MovementStat::JumpSpeed => totals.stats.jump_speed.cap,
                MovementStat::JumpHeight => totals.stats.jump_height.cap,
            },
        };
        (value > 0.0).then_some(value)
    }
}

/// One dashboard stat: its identity, where it shows, how it looks, and how to read it off the
/// engine's output.
///
pub struct StatDef {
    /// Stable identity — the persisted key and the DOM id the config modal's deep-link scrolls
    /// to. Kept as the beta's slug so a config exported from either planner means the same
    /// thing.
    pub id: &'static str,
    pub label: &'static str,
    pub family: StatFamily,
    pub section: StatSection,
    pub format: StatFormat,
    pub cap: StatCap,
    /// The camelCase dashboard breakdown key(s) this stat's set-bonus contributions track
    /// under, for the Rule-of-5 cue — and, read backwards through [`rows_for_bonus_stat`], for
    /// filing a bonus under the row it would move. EMPTY for a stat no set bonus can feed — which is most of
    /// them: the set-bonus vocabulary ([`set_bonuses::SetBonusStat`]) is a much smaller universe
    /// than the dashboard's, and naming a key it never writes would be a cue that silently never
    /// fires, indistinguishable from "this build has no capped bonuses".
    ///
    /// A list rather than one key so a row projected from several fields could name each of them;
    /// a Rule-of-5 cap on any one is a cap on the number shown.
    pub breakdown_keys: &'static [&'static str],
    /// The [`coh_math::GlobalBonuses::BREAKDOWN_KEYS`] this stat's value is built from — what the
    /// detailed-totals breakdown walks the provenance ledgers for.
    ///
    /// A SECOND list rather than a widening of [`Self::breakdown_keys`], because the two are
    /// genuinely different vocabularies and one field cannot honestly serve both. Most rows show
    /// it: the seven `prot*`, the ten `debuffResist*`, `mezResistTaunt`/`Placate`, the two stealth
    /// radii and `absorb` all name a ledger key and NO set-bonus key, because no set bonus grants
    /// them — they have a per-source explanation but no Rule-of-5 bucket. Folding the two into one
    /// list would have to claim a cue that could never fire.
    ///
    /// Empty means the stat has no per-source explanation, which is a claim the gates check
    /// rather than a default: `netEndPerSec` is a formula over three other fields and the
    /// baselines are not accumulations at all
    /// ([`coh_math::GlobalBonuses::is_attributable`]). Every key here must be attributable —
    /// `ledger_keys_are_attributable` is what stops a stat offering an expansion that opens onto
    /// nothing, which reads as "no sources" rather than as "not that kind of number".
    pub ledger_keys: &'static [&'static str],
    /// The unit the LEDGER rows are in, which is not always the unit the face is in: Max HP
    /// shows absolute hit points while `maxHP` accumulates a percentage, the travel rows are
    /// buff percentages behind an mph face, and Recharge's face adds the implicit 100%. Naming
    /// it per stat keeps the rows honest without a second key→unit table to drift
    /// (the PROD6A lesson). Equal to [`Self::format`] wherever the units already agree.
    pub ledger_format: StatFormat,
    /// Read the stat off the engine's output. Every one of these is a field access: the
    /// registry reports what the engine computed, it never computes.
    pub read: fn(&CalculatedTotals) -> f64,
    /// An extra sentence for the row's tooltip. Two things earn one: a face that is a PROJECTION
    /// of some other engine field, where the input is worth seeing; and a row whose place in its
    /// section would otherwise imply a relationship to its neighbours that the engine does not
    /// have. `None` for the stats that show the engine's own number and stand alone in their
    /// section, which is most of them.
    pub annotate: Option<fn(&CalculatedTotals) -> String>,
}

/// A stat IS its [`id`](StatDef::id) — that is what the persisted config stores and what
/// [`by_id`] resolves, so two defs are the same stat exactly when their ids match. Written out
/// rather than derived because the derive would compare the `read`/`annotate` function pointers,
/// which the compiler warns is not a meaningful comparison. Lets a `&'static StatDef` travel as a
/// value through a Dioxus prop.
impl PartialEq for StatDef {
    fn eq(&self, other: &StatDef) -> bool {
        self.id == other.id
    }
}

impl std::fmt::Debug for StatDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("StatDef").field(&self.id).finish()
    }
}

impl StatDef {
    /// The stat's value, its formatted text, and the two status cues — everything a row needs,
    /// resolved in one place so the panel body and any future surface agree by construction.
    pub fn resolve(&self, totals: &CalculatedTotals) -> ResolvedStat {
        let value = (self.read)(totals);
        ResolvedStat {
            text: self.format.render(value),
            note: self.annotate.map(|note| note(totals)).unwrap_or_default(),
            at_cap: self
                .cap
                .ceiling(totals)
                .is_some_and(|ceiling| value >= ceiling),
            rule_of_five_capped: self
                .breakdown_keys
                .iter()
                .any(|key| set_bonuses::stat_has_capped_bonus(&totals.set_bonus_tracking, key)),
            // Asked of the engine's own record of what the layer moved, not of the build's
            // slider positions — so the mark describes THESE totals and cannot outlive or
            // precede them.
            simulated: totals.what_if.touches(self.ledger_keys),
        }
    }
}

/// The buff percentage a travel speed was projected from, for that row's tooltip. The face shows
/// mph (feet for jump height) because that is what the game shows and what a build is planned
/// against; the percentage is the input, and losing sight of it would hide what the build's
/// powers and set bonuses are actually contributing.
fn travel_note(buff_percent: f64) -> String {
    let sign = if buff_percent >= 0.0 { "+" } else { "" };
    format!("{sign}{buff_percent:.2}% travel buff.")
}

/// A stat resolved against a build: the text to draw and the two non-colour status cues.
#[derive(Clone, PartialEq)]
pub struct ResolvedStat {
    pub text: String,
    /// What the row's tooltip says about the number itself (empty when there is nothing to add) —
    /// distinct from the two cue descriptions, which are about its status.
    pub note: String,
    /// At or above the stat's ceiling — drawn as a dotted underline.
    pub at_cap: bool,
    /// A set bonus feeding this stat was rejected by the Rule of 5, so the total is smaller
    /// than the build's bonuses nominally grant — drawn as a warning ring.
    pub rule_of_five_capped: bool,
    /// The what-if team-buff layer moved one of the accumulator keys this number is built from,
    /// so it is a SIMULATED total rather than the build's own. Persistent by design: without a
    /// per-row mark, a screenshot of a buffed dashboard is indistinguishable from an unbuffed
    /// one, and the layer's whole risk is a simulated number read as real.
    pub simulated: bool,
}

/// Look a stat up by its persisted id. `None` for an id this build no longer has — the same
/// retirement case [`crate::layout_store`]'s `Retirable` handles for panels: a stat that was
/// removed from the vocabulary should cost the user that one toggle, not their whole config.
pub fn by_id(id: &str) -> Option<&'static StatDef> {
    ALL.iter().find(|stat| stat.id == id)
}

/// The dashboard label for an accumulator key, for a surface naming what it will move before it
/// moves it — the buff-pet opt-in promising "+Melee Def" is promising THAT row.
///
/// Read off [`StatDef::ledger_keys`] rather than a table of its own: the row a key feeds is the
/// row that must light up, so a key with no row here has nothing to promise and names nothing
/// (`None`) rather than inventing a label the dashboard would not answer to.
pub fn label_for_ledger_key(key: &str) -> Option<&'static str> {
    stat_for_ledger_key(key).map(|stat| stat.label)
}

/// The stat an accumulator key feeds — what [`label_for_ledger_key`] names, for a caller that
/// also needs the stat's section or family. A surface grouping engine keys by where they land on
/// the dashboard reads the grouping from here rather than keeping its own map.
pub fn stat_for_ledger_key(key: &str) -> Option<&'static StatDef> {
    ALL.iter().find(|stat| stat.ledger_keys.contains(&key))
}

/// The dashboard rows a set-bonus stat lands on — the inverse of [`StatDef::breakdown_keys`].
///
/// This is what lets a surface that describes set bonuses group them without a second table of
/// its own: the row a bonus feeds IS the name a reader knows it by, so "+3% Smashing/Lethal
/// Defense" groups under the panel row that will move when they slot it. The beta's set-bonus
/// finder kept a hand-written `NORMALIZED_EFFECT_MAP` for this and silently dropped any stat it
/// forgot; here the mapping is TOTAL by gate — `every_routed_set_bonus_stat_reaches_some_dashboard_row`
/// proves every [`set_bonuses::SetBonusStat`] reaches at least one row, so an empty result means
/// the registry changed, not that the bonus does nothing.
///
/// Usually one row. Several when the bonus itself names several types — the game's "+3%
/// Smashing/Lethal Defense" is two stats and lands on both rows — and two when the engine itself
/// fans the stat out — a `+Res(Recharge Debuff)` bonus grants Slow resistance as well, so it is
/// findable under both.
pub fn rows_for_bonus_stat(stat: set_bonuses::SetBonusStat) -> Vec<&'static StatDef> {
    let keys = stat.breakdown_keys();
    ALL.iter()
        .filter(|row| row.breakdown_keys.iter().any(|key| keys.contains(key)))
        .collect()
}

/// Every stat in one section, in dashboard order.
pub fn in_section(section: StatSection) -> impl Iterator<Item = &'static StatDef> {
    ALL.iter().filter(move |stat| stat.section == section)
}

/// The stats a fresh install shows: the original Sidekick's default set (2026-10-04,
/// user-directed), in section order so `Dashboards::defaults` can split it one panel per
/// section.
///
/// The original showed Smashing resistance alone. The six paired types are all here instead
/// (2026-10-06, user-requested): they are the ones a set's two halves can disagree on, and a
/// panel showing one half invites reading it as the pair.
///
/// The seven-row set this replaced (2026-07-27) left out to-hit, regeneration and the endurance
/// rows, which a new user had to find in the organizer before the dashboard said much.
pub const DEFAULT_VISIBLE: [&str; 18] = [
    // Offense
    "damage",
    "accuracy",
    "tohit",
    "recharge",
    "level_shift",
    // Defense
    "defense_melee",
    "defense_ranged",
    // Resistance
    "res_smashing",
    "res_lethal",
    "res_fire",
    "res_cold",
    "res_energy",
    "res_negative",
    // Survival
    "health",
    "regeneration",
    "recovery",
    "endcost",
    "netend",
];

/// The stat ids an earlier build stored that this one does not have, each with the rows that
/// replaced it — read by the roster's load path so a saved dashboard keeps what it showed.
///
/// These six are the S/L, F/C and E/N pairs, which showed `max` of their two halves. A pair
/// whose halves differ (Fiery Aura's Fire and Cold resistance) read as both being the larger
/// one, so the pair rows went and every type has its own (2026-10-06, user-requested). They are
/// not kept as an option: every surface that lists a section — the exports, the Totals sheet,
/// the set-bonus finder — would then list each type twice.
pub const RETIRED: [(&str, [&str; 2]); 6] = [
    ("defense_sl", ["def_smashing", "def_lethal"]),
    ("defense_fc", ["def_fire", "def_cold"]),
    ("defense_en", ["def_energy", "def_negative"]),
    ("res_sl", ["res_smashing", "res_lethal"]),
    ("res_fc", ["res_fire", "res_cold"]),
    ("res_en", ["res_energy", "res_negative"]),
];

/// The rows that replaced a retired stat id, or `None` for an id that was never retired.
pub fn successors_of(id: &str) -> Option<[&'static str; 2]> {
    RETIRED
        .iter()
        .find(|(retired, _)| *retired == id)
        .map(|(_, successors)| *successors)
}

/// The whole vocabulary, in dashboard order within each section.
///
/// Defense and Resistance carry the engine's per-type PROJECTION — each type floored, ceilinged
/// and (for resistance) clamped to the archetype cap in `coh_math::finalize`. That projection is
/// the single capped truth (D4); the raw per-type accumulator fields stay off the dashboard,
/// since an uncapped number beside capped ones would have nothing marking which is which.
pub static ALL: &[StatDef] = &[
    // ---- Offense ----
    StatDef {
        id: "damage",
        label: "Damage",
        family: StatFamily::Damage,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["damage"],
        ledger_keys: &["damage"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.damage,
        annotate: None,
    },
    StatDef {
        id: "accuracy",
        label: "Accuracy",
        family: StatFamily::ToHit,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["accuracy"],
        ledger_keys: &["accuracy"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.accuracy,
        annotate: None,
    },
    StatDef {
        id: "tohit",
        label: "To Hit",
        family: StatFamily::ToHit,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["toHit"],
        ledger_keys: &["toHit"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.to_hit,
        annotate: None,
    },
    StatDef {
        id: "recharge",
        label: "Recharge",
        family: StatFamily::Neutral,
        section: StatSection::Offense,
        format: StatFormat::HastePercent,
        cap: StatCap::None,
        breakdown_keys: &["recharge"],
        ledger_keys: &["recharge"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.recharge,
        annotate: None,
    },
    StatDef {
        id: "range_bonus",
        label: "Range",
        family: StatFamily::ToHit,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["range"],
        ledger_keys: &["range"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.range,
        annotate: None,
    },
    StatDef {
        id: "level_shift",
        label: "Level Shift",
        family: StatFamily::Special,
        section: StatSection::Offense,
        format: StatFormat::LevelShift,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["levelShift"],
        ledger_format: StatFormat::LevelShift,
        read: |t| t.bonuses.level_shift,
        annotate: None,
    },
    StatDef {
        id: "stealth_pve",
        label: "Stealth (PvE)",
        family: StatFamily::Defense,
        section: StatSection::Offense,
        format: StatFormat::Feet,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["stealthRadiusPvE"],
        ledger_format: StatFormat::Feet,
        read: |t| t.bonuses.stealth_radius_pve,
        annotate: None,
    },
    StatDef {
        id: "stealth_pvp",
        label: "Stealth (PvP)",
        family: StatFamily::Defense,
        section: StatSection::Offense,
        format: StatFormat::Feet,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["stealthRadiusPvP"],
        ledger_format: StatFormat::Feet,
        read: |t| t.bonuses.stealth_radius_pvp,
        annotate: None,
    },
    StatDef {
        id: "perception_bonus",
        label: "Perception",
        family: StatFamily::ToHit,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["perceptionRadius"],
        ledger_keys: &["perceptionRadius"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.perception_radius,
        annotate: None,
    },
    // The six OFFENSIVE control durations — how much longer the mez this build APPLIES lasts.
    // They live in Offense, not beside the `prot_*`/`mezres_*` rows those names echo: those two
    // sections are about mez landing ON the character, and these are something the build does to
    // a target, like Damage and Accuracy. Each label says "Duration" for that reason — a bare
    // "Hold" would be the third row of that name across the dashboard and the only one pointing
    // outward.
    //
    // Not display-only: `coh_math::granted`'s `GLOBAL_BONUS_ASPECTS` reads each of these to scale
    // the matching mez row of every power's projection, so the Info panel already spends them.
    // What was missing until now is the build-wide readout, and with it the Rule-of-5 cue.
    StatDef {
        id: "control_hold",
        label: "Hold Duration",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["holdDuration"],
        ledger_keys: &["holdDuration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.hold_duration,
        annotate: None,
    },
    StatDef {
        id: "control_stun",
        label: "Stun Duration",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["stunDuration"],
        ledger_keys: &["stunDuration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.stun_duration,
        annotate: None,
    },
    StatDef {
        id: "control_immob",
        label: "Immobilize Duration",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["immobilizeDuration"],
        ledger_keys: &["immobilizeDuration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.immobilize_duration,
        annotate: None,
    },
    StatDef {
        id: "control_sleep",
        label: "Sleep Duration",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["sleepDuration"],
        ledger_keys: &["sleepDuration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.sleep_duration,
        annotate: None,
    },
    StatDef {
        id: "control_confuse",
        label: "Confuse Duration",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["confuseDuration"],
        ledger_keys: &["confuseDuration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.confuse_duration,
        annotate: None,
    },
    StatDef {
        // `terrorDuration` is the accumulator's spelling; every other surface in this family
        // calls the mez Fear (`prot_fear`, `mezres_fear`), and so does the effect registry.
        id: "control_fear",
        label: "Fear Duration",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["terrorDuration"],
        ledger_keys: &["terrorDuration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.terror_duration,
        annotate: None,
    },
    // Offensive knockback magnitude, and the seventh member of the block above in every way that
    // matters: outward-facing, and read by `GLOBAL_BONUS_ASPECTS` to scale the knockback DISTANCE
    // on every power's projection. "Strength" rather than "Duration" because a knockback has no
    // duration to lengthen — the quantity it scales is how far the foe goes.
    StatDef {
        id: "control_knockback",
        label: "Knockback Strength",
        family: StatFamily::Mez,
        section: StatSection::Offense,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["knockbackStrength"],
        ledger_keys: &["knockbackStrength"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.knockback_strength,
        annotate: None,
    },
    // ---- Defense ----
    StatDef {
        id: "defense_melee",
        label: "Melee",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defMelee"],
        ledger_keys: &["defMelee"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_melee,
        annotate: None,
    },
    StatDef {
        id: "defense_ranged",
        label: "Ranged",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defRanged"],
        ledger_keys: &["defRanged"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_ranged,
        annotate: None,
    },
    StatDef {
        id: "defense_aoe",
        label: "AoE",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defAoE"],
        ledger_keys: &["defAoE"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_aoe,
        annotate: None,
    },
    StatDef {
        id: "def_smashing",
        label: "Smashing",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defSmashing"],
        ledger_keys: &["defSmashing"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_smashing,
        annotate: None,
    },
    StatDef {
        id: "def_lethal",
        label: "Lethal",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defLethal"],
        ledger_keys: &["defLethal"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_lethal,
        annotate: None,
    },
    StatDef {
        id: "def_fire",
        label: "Fire",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defFire"],
        ledger_keys: &["defFire"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_fire,
        annotate: None,
    },
    StatDef {
        id: "def_cold",
        label: "Cold",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defCold"],
        ledger_keys: &["defCold"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_cold,
        annotate: None,
    },
    StatDef {
        id: "def_energy",
        label: "Energy",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defEnergy"],
        ledger_keys: &["defEnergy"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_energy,
        annotate: None,
    },
    StatDef {
        id: "def_negative",
        label: "Negative",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defNegative"],
        ledger_keys: &["defNegative"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_negative,
        annotate: None,
    },
    StatDef {
        id: "defense_psionic",
        label: "Psionic",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defPsionic"],
        ledger_keys: &["defPsionic"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_psionic,
        annotate: None,
    },
    StatDef {
        id: "defense_toxic",
        label: "Toxic",
        family: StatFamily::Defense,
        section: StatSection::Defense,
        format: StatFormat::Percent,
        cap: StatCap::DefenseSoftcap,
        breakdown_keys: &["defToxic"],
        ledger_keys: &["defToxic"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.def_toxic,
        annotate: None,
    },
    // ---- Resistance ----
    StatDef {
        id: "res_smashing",
        label: "Smashing",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resSmashing"],
        ledger_keys: &["resSmashing"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_smashing,
        annotate: None,
    },
    StatDef {
        id: "res_lethal",
        label: "Lethal",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resLethal"],
        ledger_keys: &["resLethal"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_lethal,
        annotate: None,
    },
    StatDef {
        id: "res_fire",
        label: "Fire",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resFire"],
        ledger_keys: &["resFire"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_fire,
        annotate: None,
    },
    StatDef {
        id: "res_cold",
        label: "Cold",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resCold"],
        ledger_keys: &["resCold"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_cold,
        annotate: None,
    },
    StatDef {
        id: "res_energy",
        label: "Energy",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resEnergy"],
        ledger_keys: &["resEnergy"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_energy,
        annotate: None,
    },
    StatDef {
        id: "res_negative",
        label: "Negative",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resNegative"],
        ledger_keys: &["resNegative"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_negative,
        annotate: None,
    },
    StatDef {
        id: "res_psionic",
        label: "Psionic",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resPsionic"],
        ledger_keys: &["resPsionic"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_psionic,
        annotate: None,
    },
    StatDef {
        id: "res_toxic",
        label: "Toxic",
        family: StatFamily::Resistance,
        section: StatSection::Resistance,
        format: StatFormat::Percent,
        cap: StatCap::ResistanceCap,
        breakdown_keys: &["resToxic"],
        ledger_keys: &["resToxic"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.res_toxic,
        annotate: None,
    },
    // ---- Survival ----
    StatDef {
        id: "health",
        label: "Max HP",
        family: StatFamily::Health,
        section: StatSection::Survival,
        format: StatFormat::HitPoints,
        cap: StatCap::MaxHpCap,
        breakdown_keys: &["maxHP"],
        ledger_keys: &["maxHP"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.max_hp_absolute,
        annotate: None,
    },
    StatDef {
        id: "absorb",
        label: "Absorb",
        family: StatFamily::Health,
        section: StatSection::Survival,
        format: StatFormat::HitPoints,
        cap: StatCap::AbsorbCap,
        breakdown_keys: &[],
        ledger_keys: &["absorb"],
        ledger_format: StatFormat::HitPoints,
        read: |t| t.stats.absorb,
        annotate: None,
    },
    StatDef {
        id: "regeneration",
        label: "Regeneration",
        family: StatFamily::Health,
        section: StatSection::Survival,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["regeneration"],
        ledger_keys: &["regeneration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.regeneration,
        annotate: None,
    },
    StatDef {
        id: "heal_other",
        label: "Heal Bonus",
        family: StatFamily::Health,
        section: StatSection::Survival,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["healOther"],
        ledger_keys: &["healOther"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.heal_other,
        annotate: None,
    },
    StatDef {
        id: "heal_received",
        label: "Heal Received",
        family: StatFamily::Health,
        section: StatSection::Survival,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["healReceived"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.heal_received,
        annotate: None,
    },
    StatDef {
        id: "maxend",
        label: "Max Endurance",
        family: StatFamily::Endurance,
        section: StatSection::Survival,
        format: StatFormat::Points,
        cap: StatCap::None,
        breakdown_keys: &["maxEndurance"],
        ledger_keys: &["maxEndurance"],
        ledger_format: StatFormat::Points,
        read: |t| t.stats.max_end,
        annotate: None,
    },
    StatDef {
        id: "recovery",
        label: "Recovery",
        family: StatFamily::Endurance,
        section: StatSection::Survival,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &["recovery"],
        ledger_keys: &["recovery"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.recovery,
        annotate: None,
    },
    StatDef {
        id: "endreduction",
        label: "End Discount",
        family: StatFamily::Endurance,
        section: StatSection::Survival,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &["endurance"],
        ledger_keys: &["endurance"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.endurance_reduction,
        annotate: None,
    },
    // The Step 9.7 pair. Both are ABSOLUTE end/sec, not percentages, and both read
    // `GlobalBonuses` rather than `stats` — the projection fills them after the stats are
    // built, so they have no `CharacterStats` twin. Neither carries a breakdown key: no
    // `SetBonusStat` routes to either, so naming one would be a Rule-of-5 cue that can never
    // fire (the `breakdown_keys_are_ones_the_engine_emits` lesson).
    StatDef {
        id: "endcost",
        label: "End Cost",
        family: StatFamily::Endurance,
        section: StatSection::Survival,
        format: StatFormat::EndurancePerSecond,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["toggleEndCost"],
        ledger_format: StatFormat::EndurancePerSecond,
        read: |t| t.bonuses.toggle_end_cost,
        annotate: None,
    },
    StatDef {
        id: "netend",
        label: "Net End",
        family: StatFamily::Endurance,
        section: StatSection::Survival,
        format: StatFormat::SignedEndurancePerSecond,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &[],
        ledger_format: StatFormat::SignedEndurancePerSecond,
        read: |t| t.bonuses.net_end_per_sec,
        annotate: None,
    },
    // ---- Movement ----
    StatDef {
        id: "runspeed",
        label: "Run Speed",
        family: StatFamily::Movement,
        section: StatSection::Movement,
        format: StatFormat::MilesPerHour,
        cap: StatCap::TravelCap(MovementStat::RunSpeed),
        breakdown_keys: &["runSpeed"],
        ledger_keys: &["runSpeed"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.run_speed.value,
        annotate: Some(|t| travel_note(t.bonuses.run_speed)),
    },
    StatDef {
        id: "flyspeed",
        label: "Fly Speed",
        family: StatFamily::Movement,
        section: StatSection::Movement,
        format: StatFormat::MilesPerHour,
        cap: StatCap::TravelCap(MovementStat::FlySpeed),
        breakdown_keys: &["flySpeed"],
        ledger_keys: &["flySpeed"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.fly_speed.value,
        annotate: Some(|t| travel_note(t.bonuses.fly_speed)),
    },
    StatDef {
        id: "jumpspeed",
        label: "Jump Speed",
        family: StatFamily::Movement,
        section: StatSection::Movement,
        format: StatFormat::MilesPerHour,
        cap: StatCap::TravelCap(MovementStat::JumpSpeed),
        breakdown_keys: &["jumpSpeed"],
        ledger_keys: &["jumpSpeed"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.jump_speed.value,
        annotate: Some(|t| travel_note(t.bonuses.jump_speed)),
    },
    StatDef {
        id: "jumpheight",
        label: "Jump Height",
        family: StatFamily::Movement,
        section: StatSection::Movement,
        format: StatFormat::FeetPrecise,
        cap: StatCap::TravelCap(MovementStat::JumpHeight),
        breakdown_keys: &["jumpHeight"],
        ledger_keys: &["jumpHeight"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.jump_height.value,
        annotate: Some(|t| travel_note(t.bonuses.jump_height)),
    },
    // The two travel PENALTIES. They read `bonuses` rather than `stats` because nothing
    // projects them onto a speed: unlike the four axes above, these have no base to raise —
    // Super Speed's −10% is the whole quantity. Signed, because zero is the baseline and every
    // value the corpus can produce is a cost.
    StatDef {
        id: "movecontrol",
        label: "Air Control",
        family: StatFamily::Movement,
        section: StatSection::Movement,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["movementControl"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.movement_control,
        annotate: None,
    },
    StatDef {
        id: "movefriction",
        label: "Friction",
        family: StatFamily::Movement,
        section: StatSection::Movement,
        format: StatFormat::SignedPercent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["movementFriction"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.movement_friction,
        annotate: None,
    },
    // ---- Status Protection ----
    StatDef {
        id: "prot_hold",
        label: "Hold",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["protHold"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_hold,
        annotate: None,
    },
    StatDef {
        id: "prot_stun",
        label: "Stun",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["protStun"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_stun,
        annotate: None,
    },
    StatDef {
        id: "prot_immob",
        label: "Immobilize",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["protImmobilize"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_immobilize,
        annotate: None,
    },
    StatDef {
        id: "prot_sleep",
        label: "Sleep",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["protSleep"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_sleep,
        annotate: None,
    },
    StatDef {
        id: "prot_confuse",
        label: "Confuse",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["protConfuse"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_confuse,
        annotate: None,
    },
    StatDef {
        id: "prot_fear",
        label: "Fear",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["protFear"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_fear,
        annotate: None,
    },
    StatDef {
        id: "prot_kb",
        label: "Knockback",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &["protKnockback"],
        ledger_keys: &["protKnockback"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_knockback,
        annotate: None,
    },
    // Repel protection is its own row for the same reason the Repel row exists under Status
    // Resistance: it protects against the continuous push, not knockback, and a power can grant
    // the two at different magnitudes (Granite Armor 10 / 10, Power Surge 100 / 10). Folding them
    // into one row would show a Brute a knockback number the game does not give.
    StatDef {
        id: "prot_repel",
        label: "Repel",
        family: StatFamily::Mez,
        section: StatSection::StatusProtection,
        format: StatFormat::Magnitude,
        cap: StatCap::None,
        breakdown_keys: &["protRepel"],
        ledger_keys: &["protRepel"],
        ledger_format: StatFormat::Magnitude,
        read: |t| t.bonuses.protection_repel,
        annotate: None,
    },
    // ---- Status Resistance ----
    // The six status mezzes show residual DURATION; knockback resistance reduces distance
    // rather than duration, so it keeps the raw percentage (the beta draws the same line).
    StatDef {
        id: "mezres_hold",
        label: "Hold",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &["mezResistHold"],
        ledger_keys: &["mezResistHold"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_hold,
        annotate: None,
    },
    StatDef {
        id: "mezres_stun",
        label: "Stun",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &["mezResistStun"],
        ledger_keys: &["mezResistStun"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_stun,
        annotate: None,
    },
    StatDef {
        id: "mezres_immob",
        label: "Immobilize",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &["mezResistImmobilize"],
        ledger_keys: &["mezResistImmobilize"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_immobilize,
        annotate: None,
    },
    StatDef {
        id: "mezres_sleep",
        label: "Sleep",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &["mezResistSleep"],
        ledger_keys: &["mezResistSleep"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_sleep,
        annotate: None,
    },
    StatDef {
        id: "mezres_confuse",
        label: "Confuse",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &["mezResistConfuse"],
        ledger_keys: &["mezResistConfuse"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_confuse,
        annotate: None,
    },
    StatDef {
        id: "mezres_fear",
        label: "Fear",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &["mezResistFear"],
        ledger_keys: &["mezResistFear"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_fear,
        annotate: None,
    },
    StatDef {
        id: "mezres_kb",
        label: "Knockback",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &["mezResistKnockback"],
        ledger_keys: &["mezResistKnockback"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_knockback,
        annotate: None,
    },
    // Repel joins Knockback on the raw percentage for the same reason: it resists a force, and
    // the residual-duration convention describes a mez that has a duration to shorten.
    StatDef {
        id: "mezres_repel",
        label: "Repel",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &["mezResistRepel"],
        ledger_keys: &["mezResistRepel"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_repel,
        annotate: None,
    },
    // Teleport joins them for the third time: it resists being moved, not being held, so there is
    // no duration to shorten. Only the caster-facing carriers reach this total — the reader
    // withholds the immunity a teleported entity gets (MEZRES-3).
    StatDef {
        id: "mezres_teleport",
        label: "Teleport",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["mezResistTeleport"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_teleport,
        annotate: None,
    },
    StatDef {
        id: "mezres_taunt",
        label: "Taunt",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["mezResistTaunt"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_taunt,
        annotate: None,
    },
    StatDef {
        id: "mezres_placate",
        label: "Placate",
        family: StatFamily::Mez,
        section: StatSection::StatusResistance,
        format: StatFormat::MezDuration,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["mezResistPlacate"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.mez_resist_placate,
        annotate: None,
    },
    // ---- Debuff Resistance ----
    StatDef {
        id: "debuff_defense",
        label: "Defense",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistDefense"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_defense,
        annotate: None,
    },
    StatDef {
        id: "debuff_tohit",
        label: "To Hit",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistToHit"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_to_hit,
        annotate: None,
    },
    StatDef {
        id: "debuff_recharge",
        label: "Recharge",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &["debuffResistRecharge"],
        ledger_keys: &["debuffResistRecharge"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_recharge,
        annotate: None,
    },
    StatDef {
        id: "debuff_endurance",
        label: "Endurance",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        // Thunderspy's `kb` 4pc grants this as `endurance_drain_resistance`, which is the same
        // `kEndurance`/`aspect=Res` attrib every power in this row's ledger authors — so the row
        // now has a Rule-of-5 cue that can actually fire (SETSTAT-1).
        breakdown_keys: &["debuffResistEndurance"],
        ledger_keys: &["debuffResistEndurance"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_endurance,
        annotate: None,
    },
    StatDef {
        id: "debuff_recovery",
        label: "Recovery",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistRecovery"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_recovery,
        annotate: None,
    },
    StatDef {
        id: "debuff_regen",
        label: "Regeneration",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistRegeneration"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_regeneration,
        annotate: None,
    },
    StatDef {
        id: "debuff_slow",
        label: "Slow",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &["debuffResistSlow"],
        ledger_keys: &["debuffResistSlow"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_slow,
        annotate: None,
    },
    StatDef {
        id: "debuff_perception",
        label: "Perception",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistPerception"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.stats.debuff_resist_perception,
        annotate: None,
    },
    // DEBUFFRES-1. These two read the accumulator, not `stats`: the beta `CharacterStats` has no
    // field for either, so `finalize`'s projection — which mirrors that struct — has nowhere to
    // put them. Same shape as the `mezResistTaunt`/`Placate` rows above.
    StatDef {
        id: "debuff_accuracy",
        label: "Accuracy",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistAccuracy"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.debuff_resist_accuracy,
        annotate: None,
    },
    StatDef {
        id: "debuff_range",
        label: "Range",
        family: StatFamily::DebuffResist,
        section: StatSection::DebuffResistance,
        format: StatFormat::Percent,
        cap: StatCap::None,
        breakdown_keys: &[],
        ledger_keys: &["debuffResistRange"],
        ledger_format: StatFormat::SignedPercent,
        read: |t| t.bonuses.debuff_resist_range,
        annotate: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The gate [`rows_for_bonus_stat`]'s own doc has cited since it was written, and which did
    /// not exist until 2026-09-27.
    ///
    /// The claim it makes is that the mapping is TOTAL: every set-bonus stat the engine can grant
    /// reaches at least one dashboard row, so an empty result from that fn means the registry
    /// changed rather than that the bonus does nothing. The beta kept a hand-written map here and
    /// silently dropped any stat it forgot — a dropped stat is a bonus the finder cannot find,
    /// which reads to a user as a bonus that is not there.
    #[test]
    fn every_routed_set_bonus_stat_reaches_some_dashboard_row() {
        let orphans: Vec<_> = set_bonuses::SetBonusStat::ALL
            .iter()
            .copied()
            .filter(|stat| rows_for_bonus_stat(*stat).is_empty())
            .map(|stat| format!("{stat:?} (breakdown keys {:?})", stat.breakdown_keys()))
            .collect();
        assert!(
            orphans.is_empty(),
            "{} of {} set-bonus stats reach no dashboard row:\n  {}",
            orphans.len(),
            set_bonuses::SetBonusStat::ALL.len(),
            orphans.join("\n  ")
        );
    }

    /// A retired id must be gone from the vocabulary (or the load path would split a row that
    /// still exists), and its successors must be live rows in the same section that between them
    /// read every ledger key the pair did — otherwise the migration loses a number.
    #[test]
    fn retired_ids_are_gone_and_their_successors_cover_them() {
        let pair_keys = |retired: &str| -> [&str; 2] {
            match retired {
                "defense_sl" => ["defSmashing", "defLethal"],
                "defense_fc" => ["defFire", "defCold"],
                "defense_en" => ["defEnergy", "defNegative"],
                "res_sl" => ["resSmashing", "resLethal"],
                "res_fc" => ["resFire", "resCold"],
                "res_en" => ["resEnergy", "resNegative"],
                other => panic!("{other} has no recorded pair keys"),
            }
        };
        for (retired, successors) in RETIRED {
            assert!(
                by_id(retired).is_none(),
                "{retired} is retired but still a row"
            );
            let rows: Vec<_> = successors
                .iter()
                .map(|id| by_id(id).unwrap_or_else(|| panic!("{id} is not a row")))
                .collect();
            assert_eq!(rows[0].section, rows[1].section);
            let covered: Vec<&str> = rows
                .iter()
                .flat_map(|row| row.ledger_keys.iter().copied())
                .collect();
            for key in pair_keys(retired) {
                assert!(
                    covered.contains(&key),
                    "{retired}'s {key} reaches no successor"
                );
            }
        }
    }
}
