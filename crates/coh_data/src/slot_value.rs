//! The slot-value shapes the atom appliers produce. These mirrored the old `effects` bag
//! (`coh_data::bag`, retired 2026-08-28 as the ATOM1-13 migrations moved every routed family onto
//! the typed atom list), so they carry the bag's shape for the `atom ?? bag` seams that still fall
//! back to it per-family. The atom readers return these directly; the shapes are the common
//! vocabulary the two sides resolve to.

/// A scaled effect value: `{ scale, table, perTarget }` (the object form of the beta's
/// `ScalarOrScaled`). `table` is `None` for a bare-number slot; the caller resolves it exactly as
/// `resolveScaledEffect` does (`coh_math::scaled`).
///
/// `per_target` is the AoE per-target increment (`coh_math::stacking`): an effect carrying it
/// grows by `per_target × (targetsHit − 1)`. It is `None` for the (overwhelmingly common)
/// always-on buff.
///
/// KNOWN LOSSY FLATTENING (corpus-vacuous, see DATA-GAP STACK-1): a bare number becomes
/// `{scale, table:None, per_target:None}`, indistinguishable from a table-less object. The
/// beta's stack multiply guards on `typeof value !== 'object'`, so a bare-number value would
/// be exempt from `× stacks` there but not here. No corpus data reaches that branch —
/// `scripts/survey-stacking.ts` proved ZERO bare-number values for any `stacksLinear` effect
/// key (slot-level and inner-level, Homecoming + Rebirth). That probe walked `src/` and was
/// deleted with it on 2026-09-25; the measurement stands as recorded here, and re-running it
/// means recovering it from history (`git log --diff-filter=D -- scripts/survey-stacking.ts`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scaled {
    pub scale: f64,
    pub table: Option<String>,
    pub per_target: Option<f64>,
}

/// A mez / taunt / placate protection slot value — `{ scale, table }` plus whether the winning
/// atom is protection-spelled.
///
/// Protection-spelled is `scale < 0 || magnitude < 0 || Expression` rather than applied control.
/// The apply pass credits a slot when the table is `Res_Boolean` OR this flag is set on a
/// non-foe-affecting power. Set by the atom reader ([`coh_math::appliers::mez_protection`]); the
/// old bag reader could not know the spelling (the converter abs-s the scale) and left it `false`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScaledMez {
    pub scale: f64,
    pub table: String,
    pub is_protection: bool,
}

/// The `stealth` slot — a stealth-radius source, in FEET (each radius resolves with NO `× 100`;
/// it is a distance, not a percentage).
///
/// `stack_key` is the binary suppress group: sources sharing a non-`None` key do not stack —
/// only the largest radius in the group applies — while a `None` key stacks additively. The
/// grouping is resolved across ALL sources at once, so a reader can only report the key;
/// see `coh_math::stealth`.
///
/// Either radius may be absent (Super Speed is PvE-only); absent means "no component on that
/// axis", which the resolve treats as zero rather than as a missing power.
#[derive(Debug, Clone, PartialEq)]
pub struct Stealth {
    pub stack_key: Option<String>,
    pub pve: Option<Scaled>,
    pub pvp: Option<Scaled>,
}

/// The `absorb` slot: a Heal-tabled absorb shield with two magnitude forms. `max_hp_fraction` set
/// — OR a `_ones` `table` — is the MaxHP-FRACTION form (a fraction of the final build Max HP,
/// resolved in `coh_math` Step 9.2 — ATOM10); otherwise the FLAT-HP form resolves the Heal
/// `table` to absolute HP. `scale` defaults to 0 for the beta's optional `scale?` (a
/// fraction-only slot may omit it). `applies_strength` gates the FRACTION branch's
/// `+Strength(Absorb)` multiplier (`appliesStrength !== false`, so `None`/absent ⇒ applies —
/// only an explicit `false`, e.g. an ATO proc, opts out).
///
/// Per-foe growth reaches each form through its own increment (PROD6C-3j): `per_target` grows
/// the `scale`, while `max_hp_fraction_per_target` grows the FRACTION — whose magnitude is an
/// Expression, so it carries no scale an increment could ride.
#[derive(Debug, Clone, PartialEq)]
pub struct Absorb {
    pub scale: f64,
    pub table: Option<String>,
    pub max_hp_fraction: Option<f64>,
    pub max_hp_fraction_per_target: Option<f64>,
    pub per_target: Option<f64>,
    pub applies_strength: Option<bool>,
}

/// One axis entry of a movement buff — `{ scale, table?, stackKey?, suppressible? }` — the
/// atom-native `coh_math::appliers::movement` reader's shape. `stack_key` is the binary suppress
/// group (present only with `stacking: Suppress` on the atom side); `suppressible` marks a buff
/// the game shuts off in combat. Both resolve together across all sources in
/// `coh_math::movement`.
#[derive(Debug, Clone, PartialEq)]
pub struct MovementValue {
    pub scale: f64,
    pub table: Option<Box<str>>,
    pub stack_key: Option<Box<str>>,
    pub suppressible: bool,
    /// The caster's `Run/Fly/Jump` enhancements do NOT multiply this entry.
    ///
    /// A power can buff one axis twice — Sprint's `RunningSpeed 0.5 Melee_Ones`
    /// and its `IgnoreStrength` twin are +100% run of which only the first half
    /// enhances — and the two are the same effect on every axis but this one.
    /// Carried per entry so they can be told apart; see the key note on
    /// `coh_math::appliers::movement::movement_buff_value`.
    pub ignore_strength: bool,
    /// The AoE per-target increment (MOVEMAP-5). `scale` is the value at 1 target;
    /// the calc applies `scale + per_target × (targetsHit − 1)`. Absent for the
    /// overwhelming majority of movement entries — only AoE/cone powers with
    /// Stack/Continuous/RefreshToCount self-buffs carry one (Thunderspy's Dangerous
    /// Acceleration is the sole carrier, corpus-wide).
    pub per_target: Option<f64>,
}
