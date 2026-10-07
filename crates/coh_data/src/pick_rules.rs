//! Which powers a build may pick — the export's own `requires` gate, evaluated.
//!
//! Every power carries a `requires` string: the server's reverse-polish prerequisite
//! expression, the same one `power_system.c` evaluates when the game offers you a pick. It
//! states the archetype gate (`$archetype @Class_Brute == $archetype @Class_Tanker == ||`),
//! the intra-set prior-pick gate (`Epic ownPowerNum? 1 >`, `A B + C + 1 >`), and the
//! mutual-exclusion locks (`Pool.Fighting.Strike !`) — all of it, per power, per dataset.
//!
//! That is why nothing here is a table. The beta reimplemented these gates as hand-written
//! constants (`EPIC_TIER_REQUIREMENTS` plus a `datasetId !== 'thunderspy'` carve-out for the
//! fork whose epic pools are flat, and a `POOL_EXCLUSION_GROUPS` list of pool names) — and
//! every one of those is a restatement of something the export already says. Thunderspy's
//! epic powers simply carry no prior-pick clause and all unlock at the same `available`, so
//! reading the data reproduces the fork's flat behaviour with no fork branch at all.
//!
//! `POOL_EXCLUSION_GROUPS` is the pool-mutual-exclusion half of that, and this file was
//! wrong about it until SETGATE-1: the rule lives on the POWERSET record, not on any power,
//! as `SetBuyRequires`, and the parser was reading that field into nothing. So the sentence
//! above described a replacement that had not happened — the export did say it, and nothing
//! here was listening. [`set_gate`] is the reader; a set-level gate is answered by the same
//! evaluator over the same language, and only `powerset?` is unique to it.
//!
//! Pairs with [`crate::LevelingSchedule`], which owns the other half of pick eligibility:
//! the schedule says WHETHER a pick is left and at what level, this says whether the game
//! would let this power take it.
//!
//! Pure over `(power, build, set names)` — no I/O, no database handle. The third input is
//! [`SetPaths`], the dataset's own spelling of each set it ships: a gate names a set the way
//! the binary does and the build stores it the way the display does, and nothing in a
//! `CharacterState` can bridge those two.

use crate::character::{CharacterState, SelectedPower};

/// What an evaluation is missing when it cannot answer. Returned rather than defaulted, so
/// an expression this evaluator does not understand shows as a visibly ungated row instead
/// of silently allowing or silently forbidding a pick (Rule 1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequiresError {
    #[error("unknown token {token:?} in requires expression {expression:?}")]
    UnknownToken { token: String, expression: String },
    #[error("operator {operator:?} underflowed the stack in requires expression {expression:?}")]
    StackUnderflow {
        operator: String,
        expression: String,
    },
    #[error("requires expression {expression:?} left {depth} values on the stack, expected 1")]
    UnbalancedStack { expression: String, depth: usize },
    #[error("operator {operator:?} needs a number but got the name {name:?} in {expression:?}")]
    NameWhereNumberExpected {
        operator: String,
        name: String,
        expression: String,
    },
}

/// A stack value. The language mixes two kinds: numbers (booleans are 0 / non-zero, exactly
/// as the server treats them) and bare NAMES, which are not values in themselves — they are
/// operands waiting for the reader that consumes them (`Owned?`, `ownPowerNum?`, `char>`) or
/// for the `==` identity comparison. A name that reaches an arithmetic operator is a token
/// this evaluator failed to recognize, and says so rather than coercing to zero.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Number(f64),
    Name(String),
}

impl Value {
    fn number(&self, operator: &str, expression: &str) -> Result<f64, RequiresError> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Name(name) => Err(RequiresError::NameWhereNumberExpected {
                operator: operator.to_string(),
                name: name.clone(),
                expression: expression.to_string(),
            }),
        }
    }
}

/// The access level `char> accesslevel` reads for a player character. Every account that can
/// hold a build is access level 0; the gates that read it (`accesslevel char> 0 >=`) exist to
/// exclude NPC and GM-only records, so a planner's answer is always "yes, and only just".
const PLAYER_ACCESS_LEVEL: f64 = 0.0;

/// Evaluate a power's `requires` against a build.
///
/// An empty expression is satisfied — "no prerequisite" is what the game means by it. A
/// non-empty expression resolves to the stack's single remaining value, true when non-zero.
pub fn requires_met<S: AsRef<str>>(
    tokens: &[S],
    state: &CharacterState,
    archetype_id: Option<&str>,
    sets: &SetPaths,
) -> Result<bool, RequiresError> {
    if tokens.is_empty() {
        return Ok(true);
    }
    // Joined only for the error messages below, which quote the whole program.
    let expression = &tokens
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(" ");

    let mut stack: Vec<Value> = Vec::new();
    for token in tokens.iter().map(AsRef::as_ref) {
        apply_token(token, expression, state, archetype_id, sets, &mut stack)?;
    }

    match stack.len() {
        // The result is the stack top, as in the server's evaluator.
        1 => Ok(stack[0].number("<result>", expression)? != 0.0),
        depth => Err(RequiresError::UnbalancedStack {
            expression: expression.to_string(),
            depth,
        }),
    }
}

fn apply_token(
    token: &str,
    expression: &str,
    state: &CharacterState,
    archetype_id: Option<&str>,
    sets: &SetPaths,
    stack: &mut Vec<Value>,
) -> Result<(), RequiresError> {
    let mut pop = |operator: &str| -> Result<Value, RequiresError> {
        stack.pop().ok_or_else(|| RequiresError::StackUnderflow {
            operator: operator.to_string(),
            expression: expression.to_string(),
        })
    };

    match token {
        // ---- binary operators -------------------------------------------------------
        "&&" | "||" | "+" | ">" | ">=" | "<" | "<=" => {
            let right = pop(token)?.number(token, expression)?;
            let left = pop(token)?.number(token, expression)?;
            let value = match token {
                "&&" => boolean(left != 0.0 && right != 0.0),
                "||" => boolean(left != 0.0 || right != 0.0),
                "+" => left + right,
                ">" => boolean(left > right),
                ">=" => boolean(left >= right),
                "<" => boolean(left < right),
                _ => boolean(left <= right),
            };
            stack.push(Value::Number(value));
        }
        // Identity comparison — the only operator that takes NAMES, and the only way an
        // archetype gate is ever written (`$archetype @Class_Brute ==`).
        "==" => {
            let right = pop(token)?;
            let left = pop(token)?;
            let equal = match (&left, &right) {
                (Value::Name(a), Value::Name(b)) => a == b,
                _ => left.number(token, expression)? == right.number(token, expression)?,
            };
            stack.push(Value::Number(boolean(equal)));
        }
        "!" => {
            let value = pop(token)?.number(token, expression)?;
            stack.push(Value::Number(boolean(value == 0.0)));
        }
        // ---- readers: postfix unary, each consuming the NAME beneath it --------------
        // Ownership of a non-power token — the patron-unlock and beta-grant tokens that
        // gate the patron masteries. These are ACCOUNT state, not build state: a planner
        // is designing the build that unlock leads to, so it assumes the unlock. The rule
        // is uniform over every such token; nothing here knows which ones exist.
        "Owned?" => {
            let name = pop(token)?;
            let owned = match &name {
                Value::Name(path) if path.contains('.') => owns_power_path(state, sets, path),
                Value::Name(_) => true,
                Value::Number(n) => *n != 0.0,
            };
            stack.push(Value::Number(boolean(owned)));
        }
        // How many powers the build owns under a path prefix — a category (`Epic`), a set,
        // or an exact power.
        "ownPowerNum?" => {
            let name = pop(token)?;
            let count = match &name {
                Value::Name(path) => owned_power_count(state, sets, path),
                Value::Number(n) => *n,
            };
            stack.push(Value::Number(count));
        }
        // Does the build hold this POWERSET? The set-level gates' own reader
        // (`SpecializeRequires`), asking about a set rather than a power — which is what
        // the VEAT branch choices need: holding Bane Spider Soldier excludes Crab, and
        // neither branch's powers are what the gate is about. Rebirth spells the token
        // `Powerset?` in two of its expressions, so the match is case-folded.
        //
        // Its operand always arrives ALREADY ANSWERED, and that is not a coincidence to
        // paper over: every `powerset?` operand in the corpus is a two-segment
        // `Category.Set` path, the operand rule below evaluates a dotted token eagerly,
        // and `owns_power_path` reads a two-segment path as set ownership — the same
        // question this reader asks. So the arm exists to CONSUME the token (an
        // unconsumed one would survive to the result as a bare name and fail the whole
        // expression), and the value it passes through is the operand rule's answer.
        //
        // Consequence, recorded rather than hidden: stubbing the `Name` branch below
        // changes no gate in any fork, because nothing routes a bare name here. It is
        // reached only if a fork someday writes a `powerset?` operand the operand rule
        // does not recognise, which is exactly when a wrong answer would matter.
        _ if token.eq_ignore_ascii_case("powerset?") => {
            let name = pop(token)?;
            let held = match &name {
                Value::Number(answered) => *answered != 0.0,
                Value::Name(path) => holds_set(state, sets, path),
            };
            stack.push(Value::Number(boolean(held)));
        }
        // A character variable.
        "char>" => {
            let variable = match pop(token)? {
                Value::Name(name) => name,
                Value::Number(number) => number.to_string(),
            };
            match variable.as_str() {
                "accesslevel" => stack.push(Value::Number(PLAYER_ACCESS_LEVEL)),
                // The build's own level, 1-based: the evaluator pushes `iLevel + 1`
                // (`Common/entity/character_eval.c:183`), so this is the level a player would
                // say out loud and needs no conversion. Do not borrow that reading for
                // `SpecializeAt`, which sits on the raw 0-based scale — see [`set_gate`].
                // `Pool.Fitness` gates its specialization on this (`… level char> 2 > &&`),
                // the only set-level gate that reads build state rather than what the build
                // holds.
                "level" => stack.push(Value::Number(f64::from(state.level))),
                _ => {
                    return Err(RequiresError::UnknownToken {
                        token: variable,
                        expression: expression.to_string(),
                    })
                }
            }
        }
        // ---- operands ---------------------------------------------------------------
        // `$archtype` is the export's own misspelling, present beside `$archetype` in the
        // same dataset; both name the same variable.
        "$archetype" | "$archtype" => {
            stack.push(Value::Name(match archetype_id {
                Some(id) => normalize(id),
                // No archetype chosen: a name that equals no class token, so every
                // archetype gate reads false and nothing archetype-gated is offered.
                None => String::new(),
            }));
        }
        _ => {
            if let Ok(number) = token.parse::<f64>() {
                stack.push(Value::Number(number));
            } else if let Some(class) = token.strip_prefix("@Class_") {
                stack.push(Value::Name(normalize(class)));
            } else if token.contains('.') {
                // A dotted power path is an operand in its own right: the count of that
                // power the build owns, which the boolean operators read as owned-ness and
                // `+` sums into the intra-set prior-pick counts.
                stack.push(Value::Number(boolean(owns_power_path(state, sets, token))));
            } else {
                // A bare name — legal only as the operand of a reader above, which will
                // consume it. If one survives to an operator or to the result, the error
                // paths report it.
                stack.push(Value::Name(token.to_string()));
            }
        }
    }
    Ok(())
}

fn boolean(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

/// Fold an identifier to the one spelling the whole codebase compares on: lower-case, with
/// the id separator normalized. The export writes the same identity three ways —
/// `@Class_Arachnos_Soldier` in an expression, `arachnos-soldier` as an archetype id,
/// `arachnos_soldier` as a section key — and they are one archetype.
pub(crate) fn normalize(value: &str) -> String {
    value.to_ascii_lowercase().replace('-', "_")
}

/// The build's four power buckets, as `(the set the bucket names, its picks)`.
///
/// The two archetype selections, the pools, the epic pool. The flat `inherents` list owns no
/// set, which is why the `Inherent`/`Prestige` categories are answered by their own rule below
/// rather than by a set match.
fn buckets(state: &CharacterState) -> impl Iterator<Item = (&str, &[SelectedPower])> {
    [&state.primary, &state.secondary]
        .into_iter()
        .filter_map(|selection| Some((selection.id.as_deref()?, selection.powers.as_slice())))
        .chain(
            state
                .pools
                .iter()
                .chain(state.epic_pool.iter())
                .map(|pool| (pool.id.as_str(), pool.powers.as_slice())),
        )
}

/// Every pick the build holds, paired with the set it belongs to — its OWN
/// [`SelectedPower::powerset`], not the bucket it sits in.
///
/// The two are the same everywhere except one case, and that case is why this reads the pick:
/// a VEAT's branch sets are offered beside the base pair and are chosen by nothing, so their
/// picks live in the base role's list carrying their own set id. Buying into the set is the
/// only thing that can say the build took that branch — which is also the game's own test
/// (`character_OwnsPowerSet` walks the sets the character has powers in).
///
/// A pick with no set of its own falls back to its bucket's, so a hand-built state that leaves
/// the tag empty answers as it always did rather than dropping out of every gate.
fn held_picks(state: &CharacterState) -> impl Iterator<Item = (&str, &SelectedPower)> {
    buckets(state).flat_map(|(bucket_id, powers)| {
        powers.iter().map(move |power| {
            let owner = if power.powerset.is_empty() {
                bucket_id
            } else {
                power.powerset.as_str()
            };
            (owner, power)
        })
    })
}

/// Every set the build holds: the four buckets' own sets, plus any set a pick names for
/// itself. A bucket is held from the moment it is chosen, with nothing picked from it yet;
/// a branch set is held only once something is picked from it.
fn held_set_ids(state: &CharacterState) -> impl Iterator<Item = &str> {
    buckets(state)
        .map(|(id, _)| id)
        .chain(held_picks(state).map(|(id, _)| id))
}

/// Does the build hold the set this `Category.Set` path names? Matched on the set's BINARY
/// name, which is the only spelling a gate uses.
///
/// The build stores sets by their contract id, which slugs the DISPLAY name, and the two
/// diverge on six sets across the three forks (AUTOISSUE-1): Bio Armor is
/// `Bio_Organic_Armor`, Presence `Manipulation`, Spines `Quills`, Ninja Blade `Ninja_Sword`,
/// Sonic Resonance `Sonic_Debuff`, Nature Armor `Sacred_Armor`. Comparing the id's trailing
/// component instead read every gate naming one of those as "no build holds this set" — which
/// silently opened the Shield/weapon exclusions and silently closed the Presence tier gates.
fn holds_set(state: &CharacterState, sets: &SetPaths, path: &str) -> bool {
    let want = normalize(path);
    held_set_ids(state).any(|id| sets.of(id).is_some_and(|named| named == want))
}

/// The build's picks that belong to the set this `Category.Set` path names, matched the same
/// way as [`holds_set`].
fn picks_in_set<'a>(
    state: &'a CharacterState,
    sets: &'a SetPaths,
    path: &str,
) -> impl Iterator<Item = &'a SelectedPower> + 'a {
    let want = normalize(path);
    held_picks(state).filter_map(move |(id, power)| (sets.of(id)? == want).then_some(power))
}

/// Does the build own the power at this dotted path?
///
/// A path is `Category.Set.Power` or, naming the set itself, `Category.Set`. The last segment
/// of a full path is the power's internal name; everything before it is the set, matched
/// through [`holds_set`] / [`picks_in_set`]. No fork's gate corpus carries a two-segment
/// `Category.Power` token (measured 2026-08-06, AUTOISSUE-1), so the arity is unambiguous.
fn owns_power_path(state: &CharacterState, sets: &SetPaths, path: &str) -> bool {
    let segments: Vec<&str> = path.split('.').collect();
    // A two-segment path names the SET — the Shield/weapon mutual-exclusion gates, the VEAT
    // branch exclusions and Thunderspy's mechanism-parent grants (Swap Ammo, Staff Mastery,
    // Evolution) are all written this way.
    if let [_, _] = segments[..] {
        return holds_set(state, sets, path);
    }
    let Some((&internal_name, leading)) = segments.split_last() else {
        return false;
    };
    // The build flattens every granted inherent into one list (they have no owning powerset),
    // so an inherent path's set segment has nothing to match against.
    if matches!(
        leading.first().map(|c| normalize(c)).as_deref(),
        Some("inherent") | Some("prestige")
    ) {
        return has_power(&state.inherents, internal_name);
    }
    picks_in_set(state, sets, &leading.join(".")).any(|power| power.internal_name == internal_name)
}

fn has_power(powers: &[crate::SelectedPower], internal_name: &str) -> bool {
    powers
        .iter()
        .any(|power| power.internal_name == internal_name)
}

/// How many powers the build owns beneath a path prefix. A one-segment prefix is a category
/// (`Epic` — every epic pick), a two-segment one a set, a full path one power.
///
/// Public because the EFFECT gates ask the same question the pick gates do, through their own
/// grammar (`<path> source.ownPower?` / `source.ownPowerNum?` in `coh_math::expr`). Answering
/// them from a second resolver would be two implementations of one build fact, free to disagree;
/// this one already folds case and reads the build's buckets structurally.
pub fn owned_power_count(state: &CharacterState, sets: &SetPaths, path: &str) -> f64 {
    let segments: Vec<&str> = path.split('.').collect();
    // A full `Category.Set.Power` path is just that power's ownership.
    if segments.len() >= 3 {
        return boolean(owns_power_path(state, sets, path));
    }
    let category = normalize(segments[0]);
    // The inherents are flat and answer by category at either arity — a set match cannot
    // reach them, since no bucket of the build holds `Inherent.Inherent`.
    if matches!(category.as_str(), "inherent" | "prestige") {
        return state.inherents.len() as f64;
    }
    // A two-segment prefix names one set; a one-segment prefix is the whole category, which
    // the build's bucket structure answers directly.
    let count: usize = if segments.len() == 2 {
        picks_in_set(state, sets, path).count()
    } else {
        match category.as_str() {
            "pool" => state.pools.iter().map(|pool| pool.powers.len()).sum(),
            "epic" => state.epic_pool.iter().map(|pool| pool.powers.len()).sum(),
            _ => [&state.primary, &state.secondary]
                .iter()
                .map(|selection| selection.powers.len())
                .sum(),
        }
    };
    count as f64
}

/// Why a set may not be taken, when it may not be.
///
/// The game answers this with one bool from
/// `character_IsAllowedToHavePowerSetHypotheticallyAtSpecializedLevel`
/// (`Common/entity/character_base.c:1499`), handing the message back through an out-pointer.
/// Its own two surfaces then split that bool three ways, so the split below is the game's
/// and not a convenience invented here:
///
/// * `uiPowers.c:395` — a refused set is HIDDEN when it is a specialization set (or an
///   epic), and otherwise drawn disabled carrying `SetBuyRequiresFailedText`.
/// * `uiLevelPower.c:243` — an unowned specialization set IS listed while
///   `character_WillBeAllowedToSpecializeInPowerSet` holds, which evaluates
///   `SpecializeRequires` and ignores the level entirely: "could be available in future (ie.
///   only barred by level, not by requirements)", in its own comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetGate {
    /// No set-level gate refuses this set.
    Open,
    /// `SetBuyRequires` refuses it. `reason` is the export's own
    /// `SetBuyRequiresFailedText` — "You can only have one Specialized power pool in your
    /// build." — rather than anything written here.
    Closed { reason: String },
    /// A specialization set whose `SpecializeRequires` this build fails: it went down
    /// another branch, or holds the thing the branch forbids. No level reaches it, which is
    /// why the game lists such a set on no surface.
    BranchClosed,
    /// A specialization set this build still qualifies for and has not yet reached the level
    /// of. `at` is the CHARACTER level, not the wire's `SpecializeAt` — see below.
    NotYet { at: u8 },
}

/// Does a set-level gate refuse this set to this build?
///
/// Distinct from [`requires_met`], which answers the same question about one POWER. Both
/// gates are evaluated by the same evaluator over the same postfix language — the only
/// reader unique to the set-level ones is `powerset?`.
///
/// The game's own predicate has a third clause this does not: a set is also refused when
/// none of its powers is individually allowed. That clause is left to the caller because the
/// caller already computes it per power (a picker needs each power's verdict anyway), and
/// deriving it twice is how two answers drift apart.
pub fn set_gate(
    buy_requires: &[String],
    buy_requires_failed: &str,
    specialize_at: u8,
    specialize_requires: &[String],
    state: &CharacterState,
    archetype_id: Option<&str>,
    sets: &SetPaths,
) -> Result<SetGate, RequiresError> {
    // Specialization is checked before the buy gate (`character_base.c:1507`), and a set is
    // a specialization set exactly when `SpecializeAt` is non-zero (`baseset_IsSpecialization`,
    // `powers.h:1449`) — so 0 means "not one", never "specializes at the first level".
    if specialize_at != 0 {
        // Evaluated WITHOUT the level, matching `character_WillBeAllowedToSpecializeInPowerSet`:
        // failing this is permanent for the build as it stands, whereas being under the level
        // is only "not yet", and the game renders those two differently.
        if !specialize_requires.is_empty()
            && !requires_met(specialize_requires, state, archetype_id, sets)?
        {
            return Ok(SetGate::BranchClosed);
        }
        // `SpecializeAt` is compared against the server's raw 0-based `iLevel`
        // (`character_base.c:1564`), the same scale `available` uses — so the character level
        // is one higher, exactly as in [`crate::Power::unlock_level`]. It is NOT the scale the
        // expression language's `level` reads: that one pushes `iLevel + 1`
        // (`character_eval.c:183`). Two bases, adjacent fields of one record, and this gate
        // reads both.
        let at = specialize_at.saturating_add(1);
        if state.level < at {
            return Ok(SetGate::NotYet { at });
        }
    }
    if !buy_requires.is_empty() && !requires_met(buy_requires, state, archetype_id, sets)? {
        return Ok(SetGate::Closed {
            reason: buy_requires_failed.to_string(),
        });
    }
    Ok(SetGate::Open)
}

/// The binary set path for every powerset a build can hold, keyed by the id the build stores.
///
/// Built once per dataset at load ([`crate::PowerDatabase`]) rather than derived per gate: the
/// pick gates are evaluated per power per render, and this is a whole-dataset fact.
///
/// A build id absent from the index has no binary name, so no set path can name it. That is a
/// dataset the build does not belong to (a cross-dataset import), not a defect here — the
/// converter throws on a set with no `key`, and `every_powerset_ships_its_binary_set_path`
/// pins that every shipped record carries one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SetPaths(std::collections::HashMap<String, String>);

impl SetPaths {
    /// Index every set a dataset ships, from the records that carry the binary name.
    ///
    /// A record shipping none is left OUT rather than defaulted to its id: an id that happens
    /// to match the binary name is the coincidence this whole gap was made of, and defaulting
    /// would reinstate it for exactly the six sets where it does not hold.
    pub fn of_dataset(powersets: &[crate::Powerset], pools: &crate::PoolCatalog) -> Self {
        SetPaths::new(
            powersets
                .iter()
                .filter_map(|set| Some((set.id.clone(), set.set_path.clone()?)))
                .chain(
                    pools
                        .pools
                        .iter()
                        .chain(pools.epics.iter())
                        .filter_map(|pool| Some((pool.id.clone(), pool.set_path.clone()?))),
                ),
        )
    }

    /// Index `(build id, binary set path)` pairs, both folded to the comparison spelling.
    pub fn new(pairs: impl IntoIterator<Item = (String, String)>) -> Self {
        SetPaths(
            pairs
                .into_iter()
                .map(|(id, path)| (normalize(&id), normalize(&path)))
                .collect(),
        )
    }

    /// The gate spelling of the set this build id names.
    fn of(&self, id: &str) -> Option<&str> {
        self.0.get(&normalize(id)).map(String::as_str)
    }
}
