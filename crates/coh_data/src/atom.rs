//! `AtomicEffect` — the faithful Rust mirror of `scripts/_atomic-effect.ts`.
//!
//! Every field is `Option`: the wire trims trailing nulls and the TS decoder restores
//! absence as `undefined` with NO defaults, so a defaulted field here would fabricate a
//! discriminator the source never carried (the Thunderspy phantom-movement bug class).
//! Consumers mirror the TS truthiness semantics per use site (`gated` absent ⇒ base,
//! `resistible` absent ⇒ never treated as stated).

use crate::power::Power;
use std::str::FromStr;

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($wire => Ok(Self::$variant),)+
                    other => Err(format!(concat!(stringify!($name), ": unknown wire value {:?}"), other)),
                }
            }
        }

        impl $name {
            pub fn as_wire(&self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }
    };
}

wire_enum!(
    /// Which game system the effect touches. Exhaustive `match` over this enum is the
    /// rebuild's core invariant: a new effect type fails to compile at every consumer.
    EffectType {
        Absorb => "Absorb",
        Accuracy => "Accuracy",
        Damage => "Damage",
        DamageBuff => "DamageBuff",
        Defense => "Defense",
        Elusivity => "Elusivity",
        Endurance => "Endurance",
        EnduranceDiscount => "EnduranceDiscount",
        Enhancement => "Enhancement",
        EntCreate => "EntCreate",
        ExecutePower => "ExecutePower",
        GlobalChanceMod => "GlobalChanceMod",
        GrantPower => "GrantPower",
        Heal => "Heal",
        HealResistance => "HealResistance",
        MaxEndurance => "MaxEndurance",
        MaxHp => "MaxHP",
        Meta => "Meta",
        Mez => "Mez",
        MezResist => "MezResist",
        Movement => "Movement",
        Perception => "Perception",
        Range => "Range",
        RechargePower => "RechargePower",
        RechargeTime => "RechargeTime",
        Recovery => "Recovery",
        Regeneration => "Regeneration",
        Resistance => "Resistance",
        Stealth => "Stealth",
        ThreatLevel => "ThreatLevel",
        ToHit => "ToHit",
        Unmapped => "Unmapped",
    }
);

wire_enum!(
    /// The variant within a type: damage type, mez kind, defense position, movement axis.
    /// `FlyMode` (the kFly grant) is DISTINCT from `Fly` (FlyingSpeed) — collapsing them
    /// double-counts Fly by +200% (COH-DATA-MODEL §3).
    ///
    /// `RadiusPvE`/`RadiusPvP`/`Translucency` are the Stealth-family axes (BRIDGE-2): the
    /// bin export carries `StealthRadius_PvE`, `StealthRadius_PvP`, and `Translucency` as
    /// three DISTINCT attribs, and the bridge must keep them separable — without a subType
    /// the two radii collapse into one indistinguishable `Stealth` atom (they differ only in
    /// raw scale), which is why the applier could not read them PvE-vs-PvP. Named `Radius*`
    /// (not bare `PvE`/`PvP`) to avoid colliding with the `PvMode` enum, which is a distinct
    /// field.
    SubType {
        Afraid => "Afraid",
        All => "All",
        AoE => "AoE",
        Cold => "Cold",
        CombatPhase => "CombatPhase",
        Confused => "Confused",
        Control => "Control",
        Electrical => "Electrical",
        Energy => "Energy",
        Evade => "Evade",
        Fire => "Fire",
        Fly => "Fly",
        FlyMode => "FlyMode",
        Friction => "Friction",
        Held => "Held",
        Immobilized => "Immobilized",
        Intangible => "Intangible",
        Jump => "Jump",
        JumpHeight => "JumpHeight",
        Knockback => "Knockback",
        Knockup => "Knockup",
        Lethal => "Lethal",
        Melee => "Melee",
        Negative => "Negative",
        OnlyAffectsSelf => "OnlyAffectsSelf",
        Placate => "Placate",
        Psionic => "Psionic",
        Quantum => "Quantum",
        Radiation => "Radiation",
        RadiusPvE => "RadiusPvE",
        RadiusPvP => "RadiusPvP",
        Ranged => "Ranged",
        Repel => "Repel",
        Run => "Run",
        Sleep => "Sleep",
        Smashing => "Smashing",
        Sonic => "Sonic",
        Special => "Special",
        Stunned => "Stunned",
        Taunt => "Taunt",
        Teleport => "Teleport",
        Terrorized => "Terrorized",
        Toxic => "Toxic",
        Translucency => "Translucency",
        Untouchable => "Untouchable",
    }
);

impl SubType {
    /// Every sub type, so a lookup over the vocabulary cannot drift from the enum: a variant
    /// added to `wire_enum!` above breaks the declared array length until it is admitted here,
    /// and an admitted variant missing from the array breaks it in the other direction.
    const VARIANTS: [Self; 45] = [
        Self::Afraid,
        Self::All,
        Self::AoE,
        Self::Cold,
        Self::CombatPhase,
        Self::Confused,
        Self::Control,
        Self::Electrical,
        Self::Energy,
        Self::Evade,
        Self::Fire,
        Self::Fly,
        Self::FlyMode,
        Self::Friction,
        Self::Held,
        Self::Immobilized,
        Self::Intangible,
        Self::Jump,
        Self::JumpHeight,
        Self::Knockback,
        Self::Knockup,
        Self::Lethal,
        Self::Melee,
        Self::Negative,
        Self::OnlyAffectsSelf,
        Self::Placate,
        Self::Psionic,
        Self::Quantum,
        Self::Radiation,
        Self::RadiusPvE,
        Self::RadiusPvP,
        Self::Ranged,
        Self::Repel,
        Self::Run,
        Self::Sleep,
        Self::Smashing,
        Self::Sonic,
        Self::Special,
        Self::Stunned,
        Self::Taunt,
        Self::Teleport,
        Self::Terrorized,
        Self::Toxic,
        Self::Translucency,
        Self::Untouchable,
    ];

    /// The sub type behind a lowercase bag key, or `None` for a key no sub type names.
    ///
    /// The bag keys are lowercase wire names (`"ranged"`, `"fire"`, …) by construction here
    /// — `defense_buff_value`, `resistance_buff_value` and the flatten pass emit
    /// `as_wire().to_lowercase()` — but a consumer that reaches a key from another source
    /// should not need to have already lowercased it, so the match is ASCII-case-folded. First
    ///-char capitalization can't be the rule: `RadiusPvE` would read `Pve` and `AoE` would
    /// read `Aoe`. The find over `VARIANTS` keeps the vocabulary a compile-time tripwire
    /// instead of a second hand-typed list.
    pub fn from_wire_lower(key: &str) -> Option<Self> {
        Self::VARIANTS
            .into_iter()
            .find(|v| v.as_wire().eq_ignore_ascii_case(key))
    }
}

wire_enum!(
    /// Which FACE of the attribute: the live value, the cap, resistance-to-it,
    /// enhance-ability, or absolute. `Unspecified` is a member, never a default.
    Aspect {
        Abs => "Abs",
        Cur => "Cur",
        Max => "Max",
        Res => "Res",
        Str => "Str",
        Unspecified => "Unspecified",
    }
);

wire_enum!(
    AttribType {
        Magnitude => "Magnitude",
        Duration => "Duration",
        Constant => "Constant",
        Expression => "Expression",
    }
);

wire_enum!(
    /// Who the effect lands on — one member per `ModTarget`
    /// (`Common/entity/attribmod.h:69`), because the game's seven values are not four.
    ///
    /// `All` used to stand where `SelfAndPets` and `TargetAndPets` are now, and that fold
    /// was the defect TARGETS-2 measured: the first anchors on the CASTER, the second on
    /// whoever the power hit, and one member could not tell a self-buff from a foe-facing
    /// one. Half the appliers read `All` as "lands on the caster" and half did not.
    ///
    /// Names follow the export's spellings except `Target`, which the export writes
    /// `AnyAffected`; the member predates the split by 25k atoms.
    ///
    /// The `…AndPets` members are an ANCHOR plus a pet copy, not a recipient: the engine
    /// resolves the anchor to its top-level owner, attaches there, then recurses over that
    /// owner's pet list (`character_combat.c:749`). Ask
    /// [`lands_on_caster`](Self::lands_on_caster) rather than matching members yourself.
    ToWho {
        Unspecified => "Unspecified",
        Target => "Target",
        TargetOnly => "TargetOnly",
        TargetOnlyAndPets => "TargetOnlyAndPets",
        TargetAndPets => "TargetAndPets",
        Self_ => "Self",
        SelfAndPets => "SelfAndPets",
        Marker => "Marker",
    }
);

impl ToWho {
    /// Does an atom addressed this way land on the CASTER — the character whose totals we
    /// are computing?
    ///
    /// The one place that question is answered, because it used to be answered fourteen
    /// times: some appliers matched `Self_`, others `Self_ | All`, and the two readings
    /// disagreed about every `SelfAndPets` atom in the corpus. Thunderspy states its whole
    /// flight kit that way, so its Fly kept the `+` half of a normalising pair and dropped
    /// the `-` half — the movement reader asked one way, the row it needed answered the
    /// other (TARGETS-2).
    ///
    /// - `Self_` and `SelfAndPets` anchor on the caster by construction: the engine starts
    ///   the walk at `pSrc` and never consults the power's targets, so neither needs any
    ///   context to answer.
    /// - `TargetAndPets` anchors on whoever the power hit, walks UP to that entity's
    ///   top-level owner, and fans out over the owner's pets. The caster is in that set
    ///   whenever caster and target share an owner — every player power in the corpus that
    ///   carries it: Serum and Smoke Flash hit `MyPet`, Force Shield hits `MyOwner`, the
    ///   Incarnate sockets hit `Self`. `scripts/planb-shadow-towho.cjs` pinned that premise
    ///   and was deleted on 2026-09-25 with the `src/` tree it read, so nothing measures it
    ///   now: a foe-facing one arriving in a future export would pass quietly, and the
    ///   premise is an assumption on this comment's word rather than a checked claim.
    /// - `Target` and `TargetOnly` are the open half. They reach the caster exactly when
    ///   the power's own [`targets_affected`](crate::Power::targets_affected) names `Self`
    ///   — Maneuvers buffs you, Wormhole's teleport resistance is the victim's — and no
    ///   atom carries that, because it is a power-level field. The bag route already reads
    ///   it as `selfIsCountedTarget`; the atom route does not, which is why every family
    ///   whose atoms are `Target` still answers from the bag. Deliberately unchanged here:
    ///   2,082 / 1,948 / 2,208 base atoms would move at once. See DATA-GAP-REGISTER
    ///   TARGETS-3.
    #[must_use]
    pub fn lands_on_caster(self) -> bool {
        match self {
            Self::Self_ | Self::SelfAndPets | Self::TargetAndPets => true,
            Self::Target
            | Self::TargetOnly
            | Self::TargetOnlyAndPets
            | Self::Marker
            | Self::Unspecified => false,
        }
    }
}

/// [`ToWho::lands_on_caster`] over an atom, with an absent recipient reading as "no".
///
/// An unstated recipient is unstated — the same rule [`Aspect::Unspecified`] carries — and
/// crediting the caster on a guess would fabricate the discriminator this asks about.
///
/// This answers the ANCHOR question only, so it says no to every `Target` atom whatever
/// the power does. A reader that means "does this reach the character whose totals I am
/// computing" wants [`reaches_caster`] instead.
#[must_use]
pub fn lands_on_caster(a: &AtomicEffect) -> bool {
    a.to_who.is_some_and(ToWho::lands_on_caster)
}

/// The RPN clause `target ≠ source`, the game's way of saying "everyone the power reaches
/// except the one who cast it". Both operand orders and both equality tokens appear in the
/// export.
///
/// The `.owner` variants (`entref target.owner> entref source> eq !`) are deliberately not
/// matched: those compare the target's OWNER, a question about pets, and no oracle here
/// settles what the caster should get from one.
fn requires_excludes_self(req: &[Box<str>]) -> bool {
    // Joined only to ask a question of it — a token boundary can neither create nor destroy
    // this clause. Never split the result back apart (COND-8).
    let squashed = req.join(" ");
    [
        "entref target> entref source> eq !",
        "entref source> entref target> eq !",
        "entref target> entref source> == !",
        "entref source> entref target> == !",
    ]
    .iter()
    .any(|clause| squashed.contains(clause))
}

/// Does this atom reach everyone the power hits EXCEPT the caster?
///
/// Two fields have to agree, and reading either alone gets it wrong. Shield Defense's Grant
/// Cover and Shield Defense's Phalanx Fighting both carry the `target ≠ source` clause, but
/// Grant Cover's defense rows are aimed at `Target` (the ally standing in the sphere) while
/// Phalanx's are aimed at `Self` — Phalanx counts nearby allies to size a buff it then hands
/// to the caster. So the clause alone would delete Phalanx, and the recipient alone would
/// keep Grant Cover.
///
/// `Unspecified` is not treated as `Target`: an unstated recipient is unstated, and guessing
/// one here would fabricate the very discriminator this function reads.
#[must_use]
pub fn excludes_caster(a: &AtomicEffect) -> bool {
    !lands_on_caster(a)
        && a.to_who != Some(ToWho::Unspecified)
        && a.requires_expression
            .as_deref()
            .is_some_and(requires_excludes_self)
}

/// Does this atom land on the CASTER once the power resolves the pronoun in it?
///
/// [`lands_on_caster`] can only answer for the recipients that name somebody. `AnyAffected`
/// (the atom's `Target`) names nobody at all: it means "whoever this power affects", so the
/// identical spelling is the caster on Maneuvers and the yanked foe on Wormhole. What settles
/// it is the power's own [`targets_affected`](Power::targets_affected), which no atom carries
/// because it is a POWER-level field (TARGETS-3).
///
/// Three terms, and the corpus needs all three:
///
/// * The anchored recipients answer without the power at all, so they pass straight through.
/// * `Target` / `TargetOnly` inherit [`Power::affects_caster`], read off
///   [`owner_targets`](AtomicEffect::owner_targets) when the atom carries one, because then
///   `power` is the shell a collector attached it to and not the power it lives on.
/// * The atom's own gate then takes the caster back out again, when the gate says who the
///   target IS ([`gate_excludes_caster`]). That term is not optional trim: Rebirth's Force
///   Bubble, Frigid Protection and Thunderspy's Velocity Siphon are `["Foe", "Self"]` powers
///   whose `Self` is one caster-facing row beside a foe aura, and without it the join hands
///   their caster the aura's `-Speed` as a movement strength buff.
///
/// The gate never overrides an anchored recipient, [`excludes_caster`]'s rule: for a `Self`
/// atom the gate says when the mod fires, not who it lands on.
///
/// Not every site wants this question. A reader rebuilding what a power GRANTS (the defense
/// a team buff hands its targets, shown on the power card) is not asking about the caster,
/// and the ally-buff powers are exactly where the two questions come apart.
///
/// A helper's row ([`AtomicEffect::pet_class`]) turns the recipients around: its `Self` is the
/// helper, and the player is on its side, so its `Friend` reaches him.
#[must_use]
pub fn reaches_caster(a: &AtomicEffect, power: &Power) -> bool {
    match a.to_who {
        Some(ToWho::Target | ToWho::TargetOnly) => {
            affects_caster(a, power) && !gate_excludes_caster(a)
        }
        _ if a.pet_class.is_some() => false,
        other => other.is_some_and(ToWho::lands_on_caster),
    }
}

/// Is the caster among the targets of the power this atom lives on? `Self` on the player's own
/// power; on a helper's, any token that includes the summoner — the game's comments on
/// `TargetType` (`Common/entity/powers.h:287`): `Friend` is everyone on the caster's side but the
/// caster, `Teammate` likewise, `MyOwner` the summoner exactly, `Any` everybody.
fn affects_caster(a: &AtomicEffect, power: &Power) -> bool {
    let caster_token = |t: &str| match a.pet_class {
        Some(_) => matches!(t, "Friend" | "Teammate" | "MyOwner" | "Any"),
        None => t == "Self",
    };
    match a.owner_targets.as_deref() {
        Some(targets) => targets.iter().any(|t| caster_token(t)),
        None => a.pet_class.is_none() && power.affects_caster(),
    }
}

/// Does this atom's gate say the target is somebody the caster is not?
///
/// The power's `EntsAffected` is a union over the whole power, so it says the caster is
/// somewhere in the target list, never that THIS mod lands on him. What narrows it is the
/// mod's own `Requires`, and three clause families in the corpus do that. Measured over every
/// gate carried by a `Target` atom of a power whose targets name `Self` (54 distinct gates
/// across the three forks), these three are the whole vocabulary of clauses that speak about
/// who the target IS:
///
/// * `target ≠ source` — the caster by name, [`requires_excludes_self`].
/// * `enttype target> critter eq` — the target is a critter, and a player caster is not. This
///   is how Homecoming writes Force of Thunder's knockdown and Reaction Time's `-1` run cap,
///   both on `["Foe", "Self"]` powers.
/// * `target.isFriend? !` — the target is not an ally, and you are your own ally. Thunderspy's
///   Anguishing Cry debuffs eight resistances this way on an `["Any", "Self"]` power, so
///   without this term the caster reads `-3` resistance to everything.
///
/// Every other gate in that corpus is about WHEN the mod fires or about the caster's own
/// state (`isPVPMap?`, `arch source>`, the mode and token gates, the event timers), which is
/// a question no recipient test should be answering. A corpus sweep fails on a fourth
/// spelling rather than letting it read as "no gate".
///
/// A substring test cannot see RPN structure, so a clause nested under an `||` would read as
/// unconditional. None is: measured, every occurrence sits in a top-level conjunction.
fn gate_excludes_caster(a: &AtomicEffect) -> bool {
    a.requires_expression.as_deref().is_some_and(|req| {
        let squashed = req.join(" ");
        requires_excludes_self(req)
            || squashed.contains("enttype target> critter eq")
            || squashed.contains("target.isFriend? !")
    })
}

wire_enum!(
    PvMode {
        Any => "Any",
        PvE => "PvE",
        PvP => "PvP",
    }
);

wire_enum!(
    /// How re-application combines — the game's `StackTypeEnum`
    /// (`Common/entity/attribmod.h`) plus Mids' `Yes`/`No`. `Yes` is in the TS union
    /// but unseen in the current corpus; kept so a future regen carrying it still
    /// decodes. `StackThenIgnore` and `Continuous` were absent here for as long as
    /// the converter folded both to `No`, so this fail-loud enum never saw the 744
    /// templates that carry one (STACK-3).
    Stacking {
        No => "No",
        Yes => "Yes",
        Stack => "Stack",
        Replace => "Replace",
        Extend => "Extend",
        Refresh => "Refresh",
        RefreshToCount => "RefreshToCount",
        Overlap => "Overlap",
        Maximize => "Maximize",
        Ignore => "Ignore",
        Suppress => "Suppress",
        StackThenIgnore => "StackThenIgnore",
        Continuous => "Continuous",
    }
);

wire_enum!(
    /// When the game applies the row — the template's `application_type`. Exhaustive
    /// over the three exports (192,251 templates, zero absent), and fail-loud so a
    /// seventh value stops the load rather than being read as standing.
    ///
    /// `OnTick` is the standing case and encodes as ABSENT on the wire, so `None`
    /// here means `OnTick` and not "unstated" — the one field on the atom where that
    /// is true. The converter throws on a template that states nothing, which is what
    /// keeps absence single-caused; see `ATOM_TUPLE_FIELDS` in
    /// `scripts/_atomic-effect.ts`.
    ApplicationType {
        OnTick => "OnTick",
        OnActivate => "OnActivate",
        OnEnable => "OnEnable",
        OnDisable => "OnDisable",
        OnDeactivate => "OnDeactivate",
        OnExpire => "OnExpire",
    }
);

/// One record per (template × affected attribute). Mirrors the TS `AtomicEffect`;
/// field order here matches `ATOM_TUPLE_FIELDS` for readability.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AtomicEffect {
    pub effect_type: Option<EffectType>,
    pub sub_type: Option<SubType>,
    pub scale: Option<f64>,
    pub magnitude: Option<f64>,
    pub duration: Option<f64>,
    pub modifier_table: Option<Box<str>>,
    pub aspect: Option<Aspect>,
    pub attrib_type: Option<AttribType>,
    pub to_who: Option<ToWho>,
    pub pv_mode: Option<PvMode>,
    pub resistible: Option<bool>,
    pub stacking: Option<Stacking>,
    pub stack_cap: Option<f64>,
    pub ticks: Option<f64>,
    pub application_period: Option<f64>,
    pub base_probability: Option<f64>,
    pub procs_per_minute: Option<f64>,
    pub ignore_strength: Option<bool>,
    pub buffable: Option<bool>,
    pub ignore_ed: Option<bool>,
    pub ignore_scaling: Option<bool>,
    pub special_case: Option<Box<str>>,
    pub requires_expression: Option<Box<[Box<str>]>>,
    pub gated: Option<bool>,
    pub per_target: Option<f64>,
    pub suppressible: Option<bool>,
    pub not_on_caster: Option<bool>,
    pub stack_key: Option<Box<str>>,
    /// Raw `magnitude_expression` — the CoH stack-machine program that VALUES this atom's
    /// magnitude (evaluated by [`crate`]'s expr VM in `coh_math`), where the value is a
    /// formula rather than a fixed `scale`. Homecoming's `Rage_Buff` carries `kRage source>
    /// .02 *` here (Brute Fury's 2%/point), so the calc derives Fury from the data. Absent
    /// for ordinary numeric atoms — and absent on Rebirth/Thunderspy even for `Rage_Buff`,
    /// whose parsers drop the expression (DATA-GAP INHERENT-2). Distinct from
    /// `requires_expression`, which gates rather than values.
    pub magnitude_expression: Option<Box<[Box<str>]>>,
    /// Raw `RequiredEvents` gate from the AttribMod tail, comma-joined in authored order
    /// (`"Held,Sleep"`): the mod applies only while one of these events is live on the
    /// target/caster (mez-state bonus damage, repel-while-Immobilized, per-mez debuffs).
    /// Every carrier is also `gated` — the converter's base collectors treat the gate as
    /// conditional. The per-event post-event window stays in the export, not on the wire.
    pub required_events: Option<Box<str>>,
    /// Raw template-level `tick_chance` — the roll THIS template makes, distinct from
    /// `base_probability`, which is the enclosing effect group's. Absent means 1: every
    /// application lands.
    ///
    /// On a periodic template (`application_period` > 0) it is the per-tick apply chance,
    /// and `cancel_on_miss` says how the misses compound. On an instant template its meaning
    /// is unsettled and the forks disagree in shape: every Rebirth carrier duplicates its own
    /// group's `chance` exactly, every Homecoming carrier differs from it, and Thunderspy
    /// ships none. It carries here as data; consumers decide, and `coh_math::damage` reports
    /// the instant case rather than folding it in.
    pub tick_chance: Option<f64>,
    /// The template's `CancelOnMiss` flag — a periodic effect whose chain stops at the first
    /// missed tick. Only ever present beside a `tick_chance`, which is the only thing that
    /// makes it readable: with every tick certain there is no miss to cancel on.
    pub cancel_on_miss: Option<bool>,
    /// The enclosing effect group's authored `Tag` list, comma-joined in source order
    /// (`"CritLarge,ScrapperCrit_ST"`). This is where the game NAMES a mechanic: the
    /// archetype hit-time forks tag themselves (`ScrapperCrit_ST`/`ScrapperCrit_AoE`,
    /// `SentCrit`, `StealthCrit`, `ASTeamCrit`, `Containment`, `PvPCrit`), and
    /// `FieryEmbrace` names the buff that wakes a component sitting at probability 0.
    ///
    /// Carried by all three forks. Parse6 (Rebirth, Thunderspy) has no EffectGroup wrapper,
    /// but the vocabulary is there under other names — Rebirth's per-AttribMod leading string
    /// and Thunderspy's element-header string array are both `AttribModTemplate.pchName`, the
    /// field `ChanceModAccumulate` matches `Global_Chance_Mod` filters against, which is the
    /// job HC's Parse7 moved onto the group's `Tag`. Both readers surfaced it as `tags`
    /// (COND-11, 2026-08-18) after shipping zero for as long as they existed — reading this
    /// field as fork-absent is the exact misread that census recorded.
    pub tags: Option<Box<str>>,
    /// The archetypes this atom is base FOR, comma-joined in the export's own `Class_*`
    /// spelling (`"Class_Peacebringer,Class_Warshade"`). Absent means every archetype.
    ///
    /// An effect group gated `arch source> Class_Scrapper eq` belongs to a Scrapper's
    /// base and to nobody else's, which one base/conditional boolean cannot say.
    /// Rebirth spells the fork as a pair of arms — Tough carries a Kheldian arm and a
    /// thirteen-archetype arm — and with both in base every Rebirth build read 3.0 S/L
    /// resistance where Homecoming and Thunderspy read 1.5 (DATA-GAP-REGISTER AT-FORK-1).
    ///
    /// Resolved by the parser (once per archetype the dataset defines) and stamped by
    /// the converter, because deciding it needs the archetype roster and a three-valued
    /// walk that holds a definite `false` under an indeterminate sibling — neither of
    /// which [`crate::AtomicEffect::requires_expression`] plus the engine's eager
    /// evaluator can reproduce. [`coh_math::gather`] drops the atom when the build's
    /// class is absent from the list; it stays BASE so it keeps its slots.
    pub caster_archetypes: Option<Box<str>>,
    /// The export attrib this atom came from, lowercased — carried ONLY on
    /// [`EffectType::Meta`], the one type whose name does not determine it.
    ///
    /// Every other effect type names its own attrib: a `Defense`/`Ranged` atom IS the
    /// `Ranged` attrib. `Meta` is the bucket — ~44 non-stat engine markers (`meter`,
    /// `rage`, `set_mode`, `set_token`, `designer_status`, the travel stances) collapse
    /// onto it with no [`SubType`], because none of them is a numeric stat any applier
    /// wants a subtype for. Harmless until a GATE names one, and then the collapse bites.
    ///
    /// CHAIN-1 is that case. `kMeter` is ONE character attribute that ten mechanics
    /// publish — Hide, Placate, Domination, Defiance, Opportunity, Fury/Rage, Primal
    /// Energy, Battle Euphoria, Pack Mentality — because a character has exactly one
    /// meter mechanic, its archetype's. Asking "is this build's meter the HIDE meter"
    /// is therefore asking which marker its own powers publish, and the closest
    /// shape-only proxy (Self-targeted + [`suppressible`](Self::suppressible) `Meta`)
    /// matches 8 / 3 / 4 powers against this field's 3 / 3 / 3 — sweeping in the
    /// Teleport family through `designer_status` and, on Thunderspy,
    /// `Primalists_Cloak` through `set_mode`, which is the archetype whose scalar
    /// `cur.kMeter` programs the scoping exists to protect. Measured and held by
    /// the hide-meter gate; see `coh_math::gather::publishes_hide_meter`.
    ///
    /// [`EffectType::Unmapped`] is the other many-to-one collapse and deliberately does
    /// not carry this: it is a tracked coverage gap (ATOMIC-STATE-AUDIT), its members
    /// feed no gate, and it is a much larger population.
    pub meta_attrib: Option<Box<str>>,

    /// The `EntsAffected` of the power this atom LIVES on, when that is not the power
    /// carrying it. Absent means the carrier is also the owner, the ordinary case.
    ///
    /// A [`ToWho::Target`] atom names no recipient. `AnyAffected` means "whoever this power
    /// affects", so only [`Power::targets_affected`] says whether the caster is one of them,
    /// and a collector that follows a redirect or an `Execute_Power` attaches the child's
    /// AttribMods to the SHELL, whose list answers about the shell. The pool's Spring Attack
    /// is `["Self"]` because the parent teleports you, while the foe knockback it pulls in
    /// belongs to a `["Foe"]` power; Trick Arrow's EMP Arrow is a `["Self"]` shell over a
    /// `["Friend"]` field, so the field's buffs are the team's and not the caster's. Reading
    /// the shell's list credits the caster with both (TARGETS-3).
    ///
    /// STAMPED BY THE CONVERTER, like [`gated`](Self::gated), [`per_target`](Self::per_target),
    /// [`suppressible`](Self::suppressible) and [`not_on_caster`](Self::not_on_caster): the
    /// redirect chain is only walkable at convert time and nothing else on the wire records
    /// that a walk happened. [`reaches_caster`] reads it in place of the power's own list.
    pub owner_targets: Option<Box<[Box<str>]>>,
    /// Seconds after the cast this mod BEGINS: the AttribMod's own `Delay` plus every
    /// enclosing effect group's, composed by the converter's collectors the way a nested
    /// gate composes. Absent means it starts with the cast.
    ///
    /// It is what separates a power's own effect from its CRASH, and nothing else on the
    /// atom does. Rage states `+8 × Melee_Buff_Dmg` and `+2 × Melee_Buff_ToHit` for 120
    /// seconds at delay 0, and `−0.2 Base_Defense` for 10 seconds at `Delay 120`; all three
    /// are `to_who: Self`, `aspect: Cur` rows of one power, and the crash reads as a
    /// sustained −20% defense to any consumer that cannot see this field. The whole
    /// caster-reaching −ToHit population of all three forks is likewise a rez after-effect
    /// at delay 60–90 (DEFDEBUFF-1).
    ///
    /// It is NOT a "transient" flag. Half the delayed population is sub-second (7.5k of the
    /// export's 14.7k delayed templates) and that half is animation timing — a heal landing
    /// 0.25s into its own cast — so what distinguishes a later phase is the SIZE of the delay
    /// against the power's own window, not its presence. `appliers::defense::defense_self_debuff_value` states the reading it uses.
    pub delay: Option<f64>,
    /// Event names that suppress this effect while they're recent, verbatim from the
    /// template's `suppress_events` tail, with [`suppress_seconds`](Self::suppress_seconds)
    /// carrying the window. [`suppressible`](Self::suppressible) folds this same tail into
    /// one combat-suppression verdict; these fields are the tail itself, for the consumer
    /// that needs the clock rather than the verdict. RB5-d's per-cast walk is that
    /// consumer: Hide's meter suppresses on Attacked/Damaged at 8.0s, so an attack drops
    /// the meter for 8 seconds and a gapped rotation re-hides.
    ///
    /// Emitted ONLY on [`EffectType::Meta`] atoms, like [`meta_attrib`](Self::meta_attrib)
    /// and on the same economics (~256 carrier templates across the three forks against
    /// 6,485 suppress tails corpus-wide). Absence on a non-Meta atom therefore states
    /// nothing; ask [`suppressible`](Self::suppressible) there.
    pub suppress_events: Option<Box<[Box<str>]>>,
    /// The suppress window in seconds, one value per template. The export states it
    /// per event; every emitted template's events agree and the converter refuses a
    /// disagreement rather than collapsing it. Present exactly when
    /// [`suppress_events`](Self::suppress_events) is.
    pub suppress_seconds: Option<f64>,
    /// The suppress tail's `always` flag, uniform per emitted template and guarded like
    /// [`suppress_seconds`](Self::suppress_seconds). False on exactly one carrier today,
    /// inherent Engagement's `Set_Mode`. Present exactly when
    /// [`suppress_events`](Self::suppress_events) is.
    pub suppress_always: Option<bool>,
    /// Event names that cancel this effect outright, verbatim from the template's
    /// `cancel_events` tail. Meta-scoped like [`suppress_events`](Self::suppress_events).
    /// Placate's meter cancels on Attacked/Damaged/MissionObjectClick: the 10s re-hide
    /// dies the moment you act, which is the from-Hide position rule RB5-d schedules by.
    pub cancel_events: Option<Box<[Box<str>]>>,
    /// When the game applies this row. `None` means `OnTick` — the standing case,
    /// omitted by the encoder — so read it through [`applied_on`](Self::applied_on)
    /// rather than testing the field.
    pub application_type: Option<ApplicationType>,
    /// The caster-side window this row opens, in seconds, when the row IS the power's summon:
    /// the summoned pet's lifespan.
    ///
    /// STAMPED BY THE CONVERTER, like [`gated`](Self::gated) and its siblings, and for a
    /// sharper reason than theirs — the fact is not merely hard to re-derive at runtime, it is
    /// unstateable. [`duration`](Self::duration) says how long THIS row's entity lives, which
    /// is a different question from which row constitutes the power's window, and three
    /// measured shapes separate only on the gate:
    ///
    /// - Homecoming Soul Extraction — three `EntCreate` rows gated on which henchman was
    ///   sacrificed, exactly one ever applies. An ungated-only read says the power summons
    ///   nothing.
    /// - Victory Rush (Rebirth, Thunderspy) — six rows gated on the defeated foe's rank, each
    ///   spawning a 2-second `PL_StaticObject` that carries a buff and is not a kept pet. A
    ///   gated-inclusive read invents a 2-second window.
    /// - Rebirth Soul Extraction — the template states no duration; the pet lives 300.
    ///
    /// Telling them apart at runtime means testing `Class_*_Henchman` against `rank target>`,
    /// i.e. a game class name in an `if` (Rule 0). The converter's `extractSummon` and
    /// `rebuildTierConditionalSummon` already know which template they read, so they say so
    /// here instead (DATA-GAP-REGISTER ENT-14). Absence on an `EntCreate` row is therefore a
    /// statement: the power creates that entity but does not keep it.
    pub summon_window: Option<f64>,
    /// The `conditionalEffects` entry this atom belongs to, by that entry's `id`.
    ///
    /// The join between a per-power adjuster and the atoms it turns on. Absent means the
    /// atom belongs to no surviving entry — the ordinary case for a base atom, and also for
    /// a [`gated`](Self::gated) one whose gate the conditional extractor never surfaces (a
    /// PvP `enttype` pair, a chance-0 proc, an out-of-combat gate). So absence is not the
    /// complement of `gated`, and a consumer must not read it as one.
    ///
    /// STAMPED BY THE CONVERTER, like [`gated`](Self::gated) and its siblings, for a reason
    /// of the same kind: the id is not a property of the atom's own gate.
    /// `_classifyGateExpression` folds the POWERSET key and the gate's referenced power name
    /// to mint it, and `extractConditionalEffects` then discards every group that projects to
    /// no payload — so which ids exist at all is a whole-power verdict. Recomputing an atom's
    /// membership from [`requires_expression`](Self::requires_expression) would mean a second
    /// implementation of that classifier and its survivability filter, which is the drift the
    /// chain-window migration measured at a third of the corpus. Here it would also fail
    /// SILENTLY: an entry that joins no atoms reports an empty key set, not an error (Rule 1).
    pub conditional_id: Option<Box<str>>,
    /// The template's `StackByAttribAndKey` flag: the game keys this buff by
    /// (attrib, [`stack_key`](Self::stack_key)) rather than by the casting power, so a
    /// re-application REFRESHES the existing mod instead of adding a second one. That is
    /// what lets Icy Bastion's toggle re-execute sixty times without stacking to +24,000%
    /// regen.
    ///
    /// A parser field like [`stack_key`](Self::stack_key), not a converter verdict.
    ///
    /// The flag ALONE says refresh; the flag beside `stacking: Stack`/`Continuous` says
    /// something else — a per-target increment the converter's `computeAoePerTargetPatches`
    /// folds separately, so a router that also routes it double-counts. The display mirror
    /// used to ask a proxy ("Stack/Continuous AND a non-empty `stack_key`") because the flag
    /// was not on the wire, and Reactive Regeneration's five flagged, KEYLESS `Stack`
    /// templates falsified it (DATA-GAP-REGISTER STACK-5). Ask this field, never the proxy.
    pub stack_by_attrib_and_key: Option<bool>,
    /// This atom's share of a redirect chain's BASE contribution to its slot — the other arm of
    /// the branch [`per_target`](Self::per_target) covers (PERFOE-2).
    ///
    /// `detectStackingEffects` walks an `Execute_Power` chain into another power's file and adds
    /// what it finds to the patch: as `perTarget` when the outer row targets `AnyAffected` or the
    /// redirect declares `number_allowed > 1`, and as `scale` otherwise. Fulcrum Shift and
    /// Kinetic Transfer take their base 4 from `Redirects.Kinetics.KineticTransferBuffSelf` that
    /// way, Siphon Power its 2 from `Redirects.Kinetics.SiphonPower`.
    ///
    /// STAMPED BY THE CONVERTER for [`per_target`](Self::per_target)'s reason and by the same
    /// `(|scale|, table)` replay: the chain's own templates are parsed from another file and
    /// never become this power's atoms, and the redirect's `number_allowed` is not on the wire,
    /// so nothing here can re-derive it. The two arms never claim the same template — a per-foe
    /// increment and a base one-shot differ in scale.
    ///
    /// A consumer rebuilding the patched slot sums the DISTINCT stamps, the way it sums
    /// `per_target`.
    pub redirect_base: Option<f64>,
    /// The character class of the spawned entity whose power this atom is, when the row is a
    /// helper's rather than the player's — `minion_pets` for Rebirth's Fulcrum Shift, whose
    /// whole +damage comes from two `Create_Entity` spawns. `None` on every atom the player
    /// applies.
    ///
    /// Two readings change with it. The [`modifier_table`](Self::modifier_table) is read
    /// through this class ([`crate::TableScope::Pet`]), not the build's archetype:
    /// `Melee_Buff_Dmg` is 0.1 on `minion_pets` and 0.085 on a Corruptor at 50, and only the
    /// pet reading reproduces Homecoming's per-archetype Fulcrum Shift. And the recipients in
    /// [`owner_targets`](Self::owner_targets) are the helper's, whose `Friend` is the summoner
    /// (see [`reaches_caster`]).
    ///
    /// STAMPED BY THE CONVERTER for [`redirect_base`](Self::redirect_base)'s reason: the class
    /// lives on the entity def the spawning row names, two files away from the row.
    pub pet_class: Option<Box<str>>,
}

impl AtomicEffect {
    /// When the game applies this row, with the encoder's omission undone: an atom
    /// carrying no `applicationType` is `OnTick`.
    ///
    /// Every consumer should ask through here. Testing the field directly reads the
    /// 94% standing majority as unstated, which is the reading MOVEMAP-6 was.
    pub fn applied_on(&self) -> ApplicationType {
        self.application_type.unwrap_or(ApplicationType::OnTick)
    }

    /// Does this row fire only when the power turns OFF?
    ///
    /// A toggle's shutdown burst is not part of what the power does while running, and
    /// no standing total may spend it. The bag has skipped these at its routing pass
    /// since it was written (`convert-powerset.cjs:6387`); this is the atom stream's
    /// half of the same rule.
    pub fn is_deactivation_burst(&self) -> bool {
        self.applied_on() == ApplicationType::OnDeactivate
    }
    /// TS-truthiness helper: part of the power's unconditional base?
    /// (`baseAtoms` drops `gated` atoms; absent ⇒ base.)
    pub fn is_gated(&self) -> bool {
        self.gated == Some(true)
    }

    /// Whether this atom applies to a caster of class `class_name` (the export's
    /// `Class_*` spelling). True for every atom that carries no archetype fork.
    ///
    /// Case-folded because the gate operator that produced the list is the game's
    /// `eq` — string equality that folds case — and the forks disagree on the casing
    /// of the same class token.
    pub fn applies_to_class(&self, class_name: &str) -> bool {
        let Some(list) = self.caster_archetypes.as_deref() else {
            return true;
        };
        list.split(',')
            .any(|named| named.eq_ignore_ascii_case(class_name))
    }
}

/// A gate or magnitude expression rendered as text, for display or a substring probe.
///
/// An expression is a TOKEN LIST on the wire, because that is what the game holds: one
/// string-table offset per token. Joining is fine for asking a question of one — a token
/// boundary can neither create nor destroy a substring. Splitting the result back apart is
/// the defect: Homecoming's costume-FX operands contain spaces, so a re-split invents
/// boundaries the data never had (DATA-GAP-REGISTER COND-8). Use the slice itself to walk
/// tokens; use this only to read or to match.
pub fn expression_text(tokens: Option<&[Box<str>]>) -> String {
    tokens.map(|t| t.join(" ")).unwrap_or_default()
}

/// The token list a JSON gate field holds, or `None` when the field is absent or is not an
/// array of strings.
///
/// Converter-written gates (`formVariants[].condition`, `modeVariants`, `requires`, …) are token
/// arrays for the same reason the export's own are: a joined gate cannot be re-split once an
/// operand contains a space (DATA-GAP-REGISTER COND-8). A malformed gate reads as `None` rather
/// than as an empty one, so a shape fault surfaces instead of becoming "no gate".
pub fn expression_tokens(value: Option<&serde_json::Value>) -> Option<Vec<Box<str>>> {
    value?
        .as_array()?
        .iter()
        .map(|t| t.as_str().map(Box::from))
        .collect()
}
