//! Power-effect display/resolution registry, read from `hand-data/effect-registry.json`.
//!
//! An exported power's `effects` bag is a map of effect key → authored value
//! (`{ scale, table }`, a by-type object, a mez `{ mag, scale, table }`, or a bare
//! number). Turning one of those into a displayable quantity needs per-key rules:
//! which unit it lands in, which enhancement aspect scales it, whether it expands
//! into per-type rows. Those rules are app glue over the export — not binary-derived
//! — so they are hand-authored in the contract and read here.
//!
//! Same single-source pattern as `set-bonus-stat-vocab.json` (PROD6A): the beta
//! reads a build-copied generated copy of this same file, so the resolution rules
//! cannot drift between engine and beta.

use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

const EFFECT_REGISTRY_JSON: &str = include_str!("../../../hand-data/effect-registry.json");

/// Display grouping an effect belongs to. Exhaustive: a new category in the contract
/// must break the build rather than fall into a default bucket (Rule 1).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectCategory {
    Execution,
    Damage,
    Control,
    Buff,
    Debuff,
    Protection,
    Movement,
    Special,
}

/// Unit a resolved effect value lands in.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectFormat {
    Percent,
    Value,
    Mag,
    Duration,
    Scale,
    Damage,
    Degrees,
    /// A length in feet — `range`, `radius`. Its own unit rather than a `value` the view
    /// recognises by label: the view used to append feet on `label == "Range" || "Radius"`,
    /// and no key has ever been labelled `Range` — `range` is spelled `Pwr Range` — so half
    /// that rule was dead from the day it was written and a power's range read as a bare
    /// number. A unit belongs to the registry, which is the thing that knows what an effect
    /// key MEANS; a label is vocabulary and gets respelled (PR8).
    Distance,
    Custom,
}

impl EffectFormat {
    /// Max decimals a value in this unit displays with, absent a per-effect override — the
    /// beta `DEFAULT_EFFECT_PRECISION`. Power-effect values are the planner's 2-decimal
    /// "stat-like" tier; the 3-decimal tier belongs to set bonuses, not here.
    ///
    /// Precision lives beside the format rather than in a rendering component because the
    /// registry is what declares a value's unit, and unit and precision are one decision.
    pub fn default_precision(self) -> u8 {
        match self {
            EffectFormat::Percent
            | EffectFormat::Value
            | EffectFormat::Duration
            | EffectFormat::Scale
            | EffectFormat::Custom => 2,
            EffectFormat::Damage | EffectFormat::Mag => 1,
            // Whole units. The game authors both in whole feet, and a radius of `20.00ft`
            // spends two decimals on a precision the source never had.
            EffectFormat::Degrees | EffectFormat::Distance => 0,
        }
    }
}

/// Which base rate the table-less fallback path uses for this effect.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuffDebuffFace {
    Buff,
    Debuff,
}

impl BuffDebuffFace {
    /// Base value per scale point at modifier 1.0 — the canonical CoH "10% per scale
    /// (buff) / 5% per scale (debuff)" rule. Only reached when the effect carries no
    /// resolvable AT table.
    pub fn base_rate(self) -> f64 {
        match self {
            BuffDebuffFace::Buff => 0.10,
            BuffDebuffFace::Debuff => 0.05,
        }
    }
}

/// How one effect key is resolved and displayed. Mirrors the beta's
/// `EffectDisplayConfig` minus its presentation-only fields (`colorClass`, `renderAs`).
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectDisplayConfig {
    pub label: String,
    pub category: EffectCategory,
    pub format: EffectFormat,
    /// Per-power enhancement bonus that scales this effect. Absent ⇒ not enhanceable,
    /// so all three tiers are the base value.
    #[serde(default)]
    pub enhancement_aspect: Option<String>,
    /// Build-wide global / +Strength key that scales this effect, when it differs from
    /// [`Self::enhancement_aspect`]. The server reads Strength at the attrib mod's OWN
    /// offset (`attribmod.c` `mod_Fill`), while one boost template may enhance several
    /// attribs at once — a Healing IO lists `Heal_Dmg` AND `Absorb`, so absorb IS heal-
    /// enhanceable, but +Heal Strength and the +Heal set bonus target `Heal_Dmg` alone and
    /// never reach it. Absent ⇒ same key as the enhancement aspect.
    #[serde(default)]
    pub strength_aspect: Option<String>,
    #[serde(default)]
    pub calculation: Option<BuffDebuffFace>,
    /// Display order within a category. Fractional in the contract (1.5, 6.5).
    #[serde(default)]
    pub priority: Option<f64>,
    /// Words that name this effect in a power's authored one-line summary — the export's
    /// `shortHelp`, which is the game designers' own answer to "what is this power for"
    /// (`Toggle: Ranged (Targeted AoE), Foe -DEF, -To Hit`). Unsigned: the clause's `+`/`-`
    /// picks between the buff and debuff key through [`Self::category`], so `def` sits on
    /// `defense` and `defenseDebuff` alike. Empty for the `execution` rows, which are never
    /// gated — see [`summary_gate`].
    #[serde(default)]
    pub summary_tokens: Vec<String>,
    #[serde(default)]
    pub can_be_by_type: bool,
    #[serde(default)]
    pub expand_by_type: bool,
    #[serde(default)]
    pub precision: Option<u8>,
    /// Percent multiplier for the `percent` format. Absent ⇒ 100.
    #[serde(default)]
    pub base_multiplier: Option<f64>,
    /// Fixed percent per scale point, ignoring any AT-table reference the data carries.
    /// The game stores a heal-table ref on some effects for engine bookkeeping but
    /// applies a flat multiplier; without this the display would multiply
    /// scale × heal-table × 100 and produce absurd percentages.
    #[serde(default)]
    pub flat_percent_per_scale: Option<f64>,
    /// A NON-by-type value resolves through the table-base resistance-percent path
    /// rather than the generic percent path.
    #[serde(default)]
    pub scalar_from_table_percent: bool,
    /// A `value`-format effect whose scale resolves to an AMOUNT through its AT table at
    /// the build level (heal / absorb HP) rather than displaying the bare scale.
    #[serde(default)]
    pub value_from_table: bool,
    /// An authored `maxHPFraction`, or a scale on a `*_Ones` table, means "this fraction of
    /// Max HP" and displays as a percent instead of an amount.
    #[serde(default)]
    pub max_hp_fraction_percent_form: bool,
}

impl EffectDisplayConfig {
    /// The percent multiplier to apply on the generic percent path.
    pub fn percent_multiplier(&self) -> f64 {
        self.base_multiplier.unwrap_or(100.0)
    }

    /// Max decimals this effect's value displays with — the beta `effectValuePrecision`: the
    /// declared override, else the format's default.
    pub fn value_precision(&self) -> u8 {
        self.precision.unwrap_or(self.format.default_precision())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    effects: HashMap<String, EffectDisplayConfig>,
    type_labels: HashMap<String, String>,
    mez_labels: HashMap<String, String>,
    by_type_keys: Vec<String>,
}

static REGISTRY: LazyLock<RegistryFile> = LazyLock::new(|| {
    serde_json::from_str(EFFECT_REGISTRY_JSON)
        .expect("contract/effect-registry.json is valid and matches EffectDisplayConfig")
});

/// The resolution config for an effect key, or `None` when the key is not a
/// registered effect. An unregistered key is not an error — the `effects` bag also
/// carries non-effect bookkeeping (`durations`, `maxStacks`, `onlyAffectsSelf`, …)
/// that has no display row.
pub fn lookup(effect_key: &str) -> Option<&'static EffectDisplayConfig> {
    REGISTRY.effects.get(effect_key)
}

/// Every registered effect key. Used by the drift gate.
pub fn registered_keys() -> impl Iterator<Item = &'static String> {
    REGISTRY.effects.keys()
}

/// Row label for one type of an expanded by-type row (`fire` → `Fire`), falling back to
/// the raw key when the vocabulary has no entry (the beta's `|| typeKey`).
pub fn type_label(type_key: &str) -> &str {
    REGISTRY
        .type_labels
        .get(type_key)
        .map(String::as_str)
        .unwrap_or(type_key)
}

/// Row label for one mez type of an expanded protection row (`immobilize` → `Immob`).
pub fn mez_label(mez_key: &str) -> &str {
    REGISTRY
        .mez_labels
        .get(mez_key)
        .map(String::as_str)
        .unwrap_or(mez_key)
}

/// Whether an authored effect value is a by-type object (`{ fire: {...}, cold: {...} }`)
/// rather than a scaled effect or bare number — true when ANY key is in the by-type
/// vocabulary (the beta `isByTypeObject`, which lowercases each key before matching).
pub fn is_by_type_key(key: &str) -> bool {
    let lowered = key.to_lowercase();
    REGISTRY.by_type_keys.contains(&lowered)
}

// ---------------------------------------------------------------------------
// the authored summary gate
// ---------------------------------------------------------------------------

/// What a power's authored one-line summary says the power is *for*.
///
/// `shortHelp` is the designers' own answer, and it already draws the distinction a display
/// needs: Smite is `Melee, High DMG(Smash/Negative), Foe -To Hit` and Radiation Infection is
/// `Toggle: Ranged (Targeted AoE), Foe -DEF, -To Hit`. Both carry a to-hit debuff; only one is
/// about it, and nobody has to guess which. It is on 99.9% of powers in all four forks, and
/// Mids renders the same line as the headline of its own first tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SummaryGate {
    /// The summary named these effect keys. A surface with limited room leads with their rows.
    Named(BTreeSet<&'static str>),
    /// The summary named nothing this registry recognises, or said "Special" — the designers'
    /// own "there is more here and we are not itemising it". Nothing is gated and the caller
    /// shows everything: not knowing what matters is never a licence to hide a row (Rule 1).
    Open,
}

/// Noise a clause can be wrapped in before it names a stat — who it lands on, how big it is,
/// and how it is delivered. Stripped from the front repeatedly, so `pbaoe moderate dmg` and
/// `foe very high dmg` both reduce to `dmg`.
const CLAUSE_NOISE: &[&str] = &[
    // recipient
    "self", "foe", "ally", "team", "teammate", "player", "pet", "target", "enemy", "all",
    // magnitude
    "light", "minor", "moderate", "high", "extreme", "superior", "heavy", "very",
    // delivery
    "ranged", "melee", "pbaoe", "aoe", "cone", "close", "sniper", "toggle", "auto", "click",
    "location", "targeted", "wide", "area", "point", "blank", "chance", "of", "for",
];

/// Summary token → the effect keys it can name, inverted from the registry's `summaryTokens`.
static SUMMARY_TOKENS: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
    let registry: &'static RegistryFile = &REGISTRY;
    let mut map: HashMap<&'static str, Vec<&'static str>> = HashMap::new();
    for (key, config) in registry.effects.iter() {
        for token in &config.summary_tokens {
            map.entry(token.as_str()).or_default().push(key.as_str());
        }
    }
    map
});

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sign {
    Plus,
    Minus,
    Unstated,
}

/// Which face of a stat a clause's sign selects. The registry's own `category` answers it, so
/// `-DEF` reaches `defenseDebuff` and `+DEF` reaches `defense` off one unsigned token.
fn sign_admits(sign: Sign, category: EffectCategory) -> bool {
    match sign {
        Sign::Unstated => true,
        Sign::Minus => category == EffectCategory::Debuff,
        Sign::Plus => matches!(
            category,
            EffectCategory::Buff | EffectCategory::Protection | EffectCategory::Movement
        ),
    }
}

/// A parenthetical is the type list of the clause it follows (`DMG(Smash/Negative)`), never a
/// stat of its own — and its commas would otherwise split into clauses that name nothing.
fn strip_parentheticals(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// One clause down to its sign and its bare stat word, or `None` when nothing is left.
fn reduce_clause(clause: &str) -> Option<(Sign, &str)> {
    let mut rest = clause;
    let mut sign = Sign::Unstated;
    loop {
        let trimmed = rest.trim();
        if let Some(tail) = trimmed.strip_prefix('+') {
            sign = Sign::Plus;
            rest = tail;
            continue;
        }
        if let Some(tail) = trimmed.strip_prefix('-') {
            sign = Sign::Minus;
            rest = tail;
            continue;
        }
        let word = trimmed.split_whitespace().next()?;
        if CLAUSE_NOISE.contains(&word) {
            rest = &trimmed[word.len()..];
            continue;
        }
        return Some((sign, trimmed));
    }
}

/// Read a power's authored `shortHelp` as a set of effect keys to lead with.
pub fn summary_gate(short_help: &str) -> SummaryGate {
    let registry: &'static RegistryFile = &REGISTRY;
    let lowered = short_help
        .to_lowercase()
        .replace(['\u{2013}', '\u{2014}'], "-")
        .replace(':', " ");
    // A sign after a space starts a new stat even with no comma before it: Accelerate
    // Metabolism ends `+DMG +Res(Effects)`, one comma-clause naming two of them, and without
    // this the whole clause reduces to the unmatchable string `dmg +res` and BOTH are lost.
    let clauses = strip_parentheticals(&lowered)
        .replace(" +", ", +")
        .replace(" -", ", -");
    let mut named = BTreeSet::new();
    for clause in clauses.split([',', ';']) {
        let Some((sign, token)) = reduce_clause(clause) else {
            continue;
        };
        // A BARE `Special` is the designers' disclaimer, and reading it as a whitelist would
        // hide the very rows it warns about. A SIGNED one is the stat of that name: Ice Arrow
        // ends `-SPD, -Recharge, -DMG, -Special`, and taking that for the disclaimer opened the
        // gate on a power whose summary names a Hold first — the one Mids puts on its own
        // first tab.
        if token == "special" && sign == Sign::Unstated {
            return SummaryGate::Open;
        }
        let Some(candidates) = SUMMARY_TOKENS.get(token) else {
            continue;
        };
        let signed: Vec<&'static str> = candidates
            .iter()
            .copied()
            .filter(|key| {
                registry
                    .effects
                    .get(*key)
                    .is_some_and(|config| sign_admits(sign, config.category))
            })
            .collect();
        // A sign that admits nothing is a face the registry has no key for, not a reason to
        // drop the clause: `-Fly` names flight, and there is no fly-debuff key to reach.
        if signed.is_empty() {
            named.extend(candidates.iter().copied());
        } else {
            named.extend(signed);
        }
    }
    if named.is_empty() {
        SummaryGate::Open
    } else {
        SummaryGate::Named(named)
    }
}
