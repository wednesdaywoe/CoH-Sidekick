//! Pass 3 — the additive archetype-inherent damage (Defender **Vigilance**, Brute **Fury**).
//!
//! These enter as a flat `globalBonuses.damage +=`, porting `character-totals.ts` Step 9.1
//! (the flat add lands last, over the accumulated base damage).
//!
//! **Vigilance is now DERIVED from the game data** (DATA-GAP INHERENT-2, Vigilance half
//! resolved). The Vigilance power is extracted into the contract (`scripts/convert-inherents.cjs`
//! → the `Inherent` powerset) as an `Auto` power carrying a `scale 0.3` base damage-strength
//! effect on the `*Uniqueness` per-level table plus three `scale -0.1` effects gated
//! `requires_expression: "0.0 source.TeamSize> N >"` (N = 1,2,3). [`vigilance_damage_derived`]
//! resolves it: it sums the base step and each team-size step whose gate is TRUE for the build's
//! team size (evaluated through the CoH stack-machine evaluator, [`crate::expr`], source-side
//! [`SourceContext`]), each scaled through its AT table ([`resolve_scaled_effect`]). That equals
//! the beta's hardcoded `(0.3 − 0.1 × teammates) × Uniqueness[level]` table exactly — but read
//! from the data, not transcribed. [[derive-dont-invent]]
//!
//! **Provenance.** Each inherent's write is bracketed by an accumulator snapshot and filed into the
//! same per-power ledger the apply walk keeps ([`crate::apply::PowerBreakdownSource`], under
//! [`PowerSourceKind::Inherent`]) — Vigilance and Rage_Buff are extracted defs in the `Inherent`
//! set, so they are addressed as the powers they are; only the pass that applies them differs.
//!
//! **This is NOT wired into the general apply loop.** Gate evaluation stays here, in a targeted
//! Pass 3: the beta's `calculateCharacterTotals` never evaluates `requiresExpression`, and most
//! gates are target-side (Indeterminate for a self-totals calc), so honoring gates in the general
//! loop would diverge from the oracle. Gather also SKIPS archetype inherents entirely
//! ([`crate::gather`]) so their atoms cannot double-count through Pass 2 (which drops gated atoms
//! and can't see team size). Fitness inherents remain ordinary powers — no parallel pass.
//!
//! **Brute Fury is now DERIVED too** (DATA-GAP INHERENT-2, Fury half). `convert-inherents.cjs`
//! extracts `Rage_Buff` — the `Auto`, Brute-gated power whose damage-Strength atom(s) carry
//! `magnitude_expression: "kRage source> .02 *"` (2% damage-Strength per Rage point; eight per-type
//! atoms on HC/Rebirth, one generic-front atom on Thunderspy) — as the `Inherent` set's "Fury"
//! power. [`fury_damage_derived`] evaluates that
//! expression through [`crate::expr`] with `kRage` bound to the build's Fury meter, reading the
//! 2%/point coefficient FROM THE DATA rather than a hardcoded constant. The Rage meter itself is
//! genuine runtime combat state (grown per attack by the Rage proc), so it cannot be derived at
//! build time and stays a build INPUT (`combat.fury_level`), exactly as the beta models it — an
//! unknowable runtime input the user supplies, not an invention. The old `2%/point` stopgap
//! constant is DELETED. [[derive-dont-invent]]
//!
//! Fury now derives on ALL THREE datasets: Homecoming carried the expression from the start;
//! Rebirth's Parse6 export recovered it once the parser read the discarded `MagnitudeExpr`; and
//! Thunderspy's scan-based parser recovered it the same way (its Rage_Buff carries the single
//! `Ones`-front damage-Strength atom with `kRage source> .02 *`, effectType left `Unmapped`, which
//! is why [`fury_damage_derived`] keys on aspect, not effectType). DATA-GAP INHERENT-2 (Fury half)
//! is closed across the board.
//!
//! **Where Fury cannot be derived, it still FAILS LOUD**. Should a future export
//! drop `Rage_Buff`'s `magnitude_expression`, [`fury_damage_derived`] returns `Err` — recorded in
//! the [`GlobalBonuses::errors`] channel, never substituted with the old constant or a silent zero.
//! The rest of the build still computes; the gap is surfaced for the UI to mark.
//!
//! **What is NOT here — fitness.** Health / Stamina are ordinary `Auto` powers, fully atomized;
//! their regen/recovery — and Health's caster `Res(Sleep)` — flow through the apply loop like any
//! power. INHERENT-1 is resolved: the beta atomizes fitness the same way (no `applyFitnessPowerBonuses`
//! silo), so the "Fitness inherent probe" fixture grades both engines' Health/Stamina totals in
//! parity. Defiance — the Blaster per-attack damage buff — is still open: the RB5-c chain landed
//! without modeling it (each cast's damage is the projection's, with no per-attack stack ramp).

use crate::apply::PowerBreakdownSource;
use crate::expr::{eval, eval_bool, SourceContext, Value};
use crate::gather::PowerSourceKind;
use crate::scaled::resolve_scaled_effect;
use crate::totals::{CalcError, GlobalBonuses};
use coh_data::{Aspect, CombatContext, PowerDatabase, INHERENT_SET};
use std::collections::HashMap;

/// The extracted Defender inherent's identity — the def [`vigilance_damage_derived`] reads and
/// the name its provenance row carries. One constant so the lookup and the ledger cannot come to
/// name different powers.
const VIGILANCE_IDENT: &str = "Vigilance";

/// The extracted Brute inherent's identity. `Rage_Buff` is the def; "Fury" is the display name
/// the converter gives it, which is what the breakdown resolves for the player.
const RAGE_BUFF_IDENT: &str = "Rage_Buff";

/// The Rage meter's player-facing range — the Fury bar runs 0..100 (the beta `getFuryInfo` min/max
/// and the UI slider domain). The meter is genuine runtime combat state, so it is a build INPUT
/// (`combat.fury_level`), clamped to this range before it feeds the derivation — matching the beta's
/// `calculateFuryDamageBonus` clamp. This is the input's DOMAIN, not an invented calc coefficient:
/// the 2%/point itself is read from the data (see [`fury_damage_derived`]).
const FURY_METER_MIN: f64 = 0.0;
const FURY_METER_MAX: f64 = 100.0;

/// The Defender Vigilance damage bonus as a FRACTION, derived from the extracted Vigilance power
/// def and the build's combat context. Mirrors the beta `calculateVigilanceDamageBonus`, but reads
/// the mechanic from the data instead of a hardcoded table.
///
/// The base step and each team-size step contribute `scale × Uniqueness[level]`; a team-size step
/// counts only when its `source.TeamSize>` gate is TRUE for `combat.vigilance_team_size` (the
/// 1-based total team size the gate reads directly — solo = 1). A large enough team legitimately
/// floors the bonus to 0. Returns `Err` (mirroring [`fury_damage_derived`]) when
/// the value cannot be derived: no Vigilance power in the dataset, no `*Uniqueness`-table damage
/// steps on it, or a step whose `scale` the export dropped — never a silent zero standing in for a
/// missing def.
pub fn vigilance_damage_derived(
    db: &PowerDatabase,
    level: i32,
    combat: &CombatContext,
) -> Result<f64, CalcError> {
    let Some(power) = db.all_powers().find(|p| p.ident() == VIGILANCE_IDENT) else {
        return Err(CalcError::new(
            "Vigilance",
            "no Vigilance power in this dataset — cannot derive Defender Vigilance damage",
        ));
    };
    let ctx = SourceContext {
        team_size: Some(f64::from(combat.vigilance_team_size)),
        ..Default::default()
    };

    // One representative per distinct gate: Vigilance explodes each effect group into one atom per
    // damage type (8 on HC/Rebirth), all sharing a gate + scale. The base group has no gate.
    let mut seen_gates: Vec<Option<&[Box<str>]>> = Vec::new();
    let mut sum = 0.0;
    for atom in &power.atoms {
        // The damage-strength steps are Vigilance's `*Uniqueness`-table atoms. Filter on the table,
        // NOT effectType/aspect: the Thunderspy parser leaves those `Unmapped`/`Unspecified`, and
        // this also drops the Rebirth `Melee_Ones` endurance-discount rider (which shares a
        // team-size gate and would otherwise shadow a −0.1 step in the dedup).
        let Some(table) = atom.modifier_table.as_deref() else {
            continue;
        };
        if !table.to_ascii_lowercase().contains("uniqueness") {
            continue;
        }
        let gate = atom
            .requires_expression
            .as_deref()
            .filter(|expr| !expr.is_empty());
        if seen_gates.contains(&gate) {
            continue;
        }
        seen_gates.push(gate);

        // The base group (no gate) always applies; a team-size step applies iff its gate evaluates
        // TRUE. Indeterminate/false → excluded, giving the beta's `(0.3 − 0.1 × teammates)` floor.
        let applies = match gate {
            None => true,
            Some(expr) => matches!(eval_bool(expr, &ctx), Ok(true)),
        };
        if applies {
            let Some(scale) = atom.scale else {
                return Err(CalcError::new(
                    "Vigilance",
                    format!("Vigilance step on table {table:?} carries no scale — cannot derive"),
                ));
            };
            let mut table_errors = Vec::new();
            sum +=
                resolve_scaled_effect(scale, Some(table), "defender", level, db, &mut table_errors);
            if let Some(e) = table_errors.pop() {
                return Err(e);
            }
        }
    }
    if seen_gates.is_empty() {
        return Err(CalcError::new(
            "Vigilance",
            "Vigilance has no *Uniqueness-table damage steps — cannot derive Defender Vigilance damage",
        ));
    }
    Ok(sum)
}

/// The Brute Fury damage bonus as a FRACTION, DERIVED from the extracted `Rage_Buff` power and the
/// build's Fury meter. Mirrors the beta `calculateFuryDamageBonus`, but reads the per-point
/// coefficient from `Rage_Buff`'s damage-Strength `magnitude_expression` (`kRage source> .02 *`)
/// and evaluates it through the CoH stack machine ([`crate::expr`]) with `kRage` bound to the meter
/// — instead of a hardcoded `0.02`. [[derive-dont-invent]]
///
/// The meter (`kRage`) is genuine runtime combat state — grown per attack by the Rage proc — so it
/// cannot be derived at build time and stays a build input (the Fury slider), exactly as the beta
/// models it. That is an unknowable runtime input the user supplies, not an invention.
///
/// Returns `Err` (fail loud) when the value cannot be derived: no `Rage_Buff`
/// power, no damage-Strength atom, or — the Rebirth/Thunderspy case — a damage-Strength atom whose
/// `magnitude_expression` the export dropped. It must NEVER substitute the old `0.02` constant or a
/// silent zero; the gap is surfaced so the UI marks Fury while the rest of the build computes.
pub fn fury_damage_derived(db: &PowerDatabase, combat: &CombatContext) -> Result<f64, CalcError> {
    let Some(power) = db.all_powers().find(|p| p.ident() == RAGE_BUFF_IDENT) else {
        return Err(CalcError::new(
            "Fury",
            "no Rage_Buff power in this dataset — cannot derive Brute Fury damage",
        ));
    };
    // The Rage meter (kRage) drives Fury; bind the build's Fury bar, clamped to the meter range.
    let meter = combat.fury_level.clamp(FURY_METER_MIN, FURY_METER_MAX);
    let ctx = SourceContext {
        source_attributes: HashMap::from([("kRage".to_string(), meter)]),
        ..Default::default()
    };

    // Fury is ONE global +damage% (the beta adds a single number), though the data explodes it into
    // eight per-type damage-Strength atoms that share one expression. Read the first such atom's
    // expression; its evaluated value IS the whole bonus.
    for atom in &power.atoms {
        // The damage-Strength atom is Fury's carrier. Filter on aspect, NOT effectType: the
        // Thunderspy parser leaves effectType `Unmapped` (its front is the generic `Ones` token, not
        // a per-damage-type attrib), exactly as Vigilance filters on its table for the same reason.
        // `aspect == Str` still excludes Rage_Buff's `Cur` `.01` rider, so it selects only the
        // damage-Strength atom on all three forks (8 on HC/Rebirth, 1 on Thunderspy).
        if atom.aspect != Some(Aspect::Str) {
            continue;
        }
        let Some(expression) = atom
            .magnitude_expression
            .as_deref()
            .filter(|e| !e.is_empty())
        else {
            // The Rebirth/Thunderspy shape: an Expression-typed damage atom whose expression the
            // parser dropped. Exactly the missing-field case Rule 1 targets — fail loud.
            return Err(CalcError::new(
                "Fury",
                "Rage_Buff's damage atom carries no magnitude_expression — this dataset's export \
                 drops it (DATA-GAP INHERENT-2), so Fury cannot be derived here",
            ));
        };
        return match eval(expression, &ctx) {
            Ok(Value::Number(n)) => Ok(n),
            // Fury's context answers every register its expression reads with a number, so a
            // symbol, die, or ranged result alike means the expression is not the one this
            // derivation understands.
            Ok(other) => Err(CalcError::new(
                "Fury",
                format!(
                    "Rage_Buff damage expression {expression:?} yielded {other:?}, not a number"
                ),
            )),
            Err(e) => Err(CalcError::new(
                "Fury",
                format!("Rage_Buff damage expression {expression:?} did not evaluate: {e:?}"),
            )),
        };
    }
    Err(CalcError::new(
        "Fury",
        "Rage_Buff has no damage-Strength atom — cannot derive Brute Fury damage",
    ))
}

/// Apply the additive archetype-inherent damage (Vigilance and Fury, both derived) into
/// `g.damage`, porting `character-totals.ts` Step 9.1. Combat-context driven; runs AFTER the apply
/// loop so the flat add lands on the accumulated base damage in the beta's order (both bonuses are
/// `× 100` to reach the percentage convention `damage` is stored in).
///
/// Each inherent's contribution is bracketed by an accumulator snapshot and filed into `breakdown`
/// as [`PowerSourceKind::Inherent`] rows — the same MEASURED attribution the apply walk uses, so
/// Pass 3's share of a stat is provenance the ledger carries rather than a total with no source
/// behind it. The extracted defs live in the `Inherent` set, which is the set the rows name.
pub fn apply_archetype_inherents(
    g: &mut GlobalBonuses,
    archetype: &str,
    level: i32,
    combat: &CombatContext,
    db: &PowerDatabase,
    breakdown: &mut Vec<PowerBreakdownSource>,
) {
    // Defender Vigilance — derived from the extracted power. The beta guards the write on
    // `vigBonus > 0` (a 3+ teammate team floors to no bonus). An underivable Vigilance (missing
    // def, missing steps) lands in the fail-loud channel like Fury's — never a silent zero.
    if archetype == "defender" {
        let before = g.clone();
        match vigilance_damage_derived(db, level, combat) {
            Ok(bonus) if bonus > 0.0 => g.damage += bonus * 100.0,
            Ok(_) => {}
            Err(e) => g.errors.push(e),
        }
        record_inherent(&before, g, VIGILANCE_IDENT, breakdown);
    }
    // Brute Fury — DERIVED from the extracted Rage_Buff power (INHERENT-2 Fury half resolved). The
    // beta gates on `furyLevel > 0` before computing (a 0 bar is the resting state, no bonus), then
    // guards `furyBonus > 0` before the write. Where the value cannot be derived (Rebirth/Thunderspy
    // drop the meter expression) the derivation returns Err: record it in the fail-loud channel —
    // never a silent zero or a stale constant — and leave `damage` untouched so
    // the rest of the build still computes.
    if archetype == "brute" && combat.fury_level > 0.0 {
        let before = g.clone();
        match fury_damage_derived(db, combat) {
            Ok(bonus) if bonus > 0.0 => g.damage += bonus * 100.0,
            Ok(_) => {}
            Err(e) => g.errors.push(e),
        }
        record_inherent(&before, g, RAGE_BUFF_IDENT, breakdown);
    }
}

/// File one inherent's measured deltas. A floored Vigilance and an underivable Fury both move
/// nothing, so neither produces a row — the ledger describes contributions, and the fail-loud
/// channel already carries the gap.
fn record_inherent(
    before: &GlobalBonuses,
    g: &GlobalBonuses,
    power_ident: &str,
    breakdown: &mut Vec<PowerBreakdownSource>,
) {
    PowerBreakdownSource::record_deltas(
        before,
        g,
        power_ident,
        INHERENT_SET,
        PowerSourceKind::Inherent,
        breakdown,
    );
}
