//! The CoH expression evaluator, a pure stack machine for the game's RPN attribute language.
//! This is the "master key": every `requires_expression`, `magnitude_expression`, and
//! `duration_expression` in the corpus is a whitespace-delimited reverse-Polish program over the
//! same tiny operator set and a set of context *readers*. Reading them is derivation;
//! transcribing their results into hand-built tables is invention. [[derive-dont-invent]]
//!
//! The grammar is read from the data, not invented. The operator set (`+ - * / minmax dup pow
//! negate`, comparisons `> < >= <= ==`, symbol-equality `eq`, logical `&& || !`) and the reader
//! vocabulary (47 `>`/`?` readers, 13 `@`-constants, a handful of bareword runtime scalars) are
//! exactly the tokens appearing across all `*_expression` fields of the
//! Homecoming/Rebirth/Thunderspy exports. A token that isn't a number, an operator, or a known
//! reader is a symbol: an attribute name, enum value, tag, power path, or mode that a
//! reader consumes (`arch target>` reads the target's `arch`) or that `eq` compares
//! (`… critter eq`). Every real gate string runs through this VM and none of them is
//! [`EvalError::Malformed`]. A structural failure on real data would mean the
//! grammar is wrong.
//!
//! Two engine conventions, both observed in the data, matter:
//! * *The result is the top of the stack.* Leftovers below it are ignored. Vigilance's gate
//!   `0.0 source.TeamSize> 1 >` leaves `[0.0, (team>1)]`; the engine takes the top, so the
//!   leading `0.0` is inert. (This reproduces the beta's Vigilance table exactly: with
//!   `source.TeamSize>` = total team size, `team > 1/2/3` gives the −0.1 steps for the 1st/2nd/3rd
//!   teammate.)
//! * *Unknown ≠ false.* A reader this context can't resolve (a target entity, a die roll, an
//!   unmodeled combat meter) yields [`EvalError::Indeterminate`], which propagates. It's NOT an
//!   error and NOT `false`: the caller decides what an unknown gate means. This is what keeps a
//!   totals consumer honest: the gate corpus is overwhelmingly *target-side* (`enttype target>
//!   critter eq`, `arch target> Class_Minion_Grunt eq`), and a character-totals calculator has no
//!   specific target, so most gates are legitimately Indeterminate rather than silently dropped or
//!   kept.
//!
//!   Unknown propagates through the LOGIC as Kleene truth, though (EXPR-1, 2026-08-19): inside
//!   the machine an unresolvable term is [`Value::Unknown`] on the stack, not an abort, and
//!   `&&`/`||` absorb it exactly where two-valued logic already has the answer — `false && x`
//!   is false and `true || x` is true for EVERY x, so folding them decodes nothing. This is
//!   still the no-approximation rule, not a soft default: an unknown that reaches the RESULT is
//!   the same [`EvalError::Indeterminate`] it always was ([`eval`] converts at the boundary, so
//!   the variant never escapes the module). What it fixes is a definite verdict being discarded
//!   because an already-dead sibling clause couldn't be valued — Stun's redirect table is the
//!   proof: its `kBoostRange Source.Mode? distance 7 > &&` branch is definitively false the
//!   moment the mode is off, but valuing `distance` is impossible for a planner, and the old
//!   abort-on-first-unknown walk never reached the live `kBoostPower` branch below it, so the
//!   projection showed the base form while Power Boost was on.
//!
//! This module is pure (data in, value out) with no IO and no dependency on the dataset types, so
//! the calculation stays trivially testable and cacheable. Consumers that feed
//! it real build state (deriving Vigilance/Fury, gating conditional effects) land on top of it in
//! later passes; see DATA-GAP INHERENT-2.

use std::collections::{HashMap, HashSet};

/// A value on the expression stack: a number, an identifier symbol, or a value known only
/// in distribution or in bounds.
///
/// Symbols are first-class because the language pushes bare identifiers as operands: an
/// attribute name for a reader (`kMeter source>`), an enum value or tag for `eq`
/// (`… Class_Defender eq`), a power path for `source.ownPower?`. Keeping them distinct from
/// numbers lets `eq` compare identity while the arithmetic operators reject a symbol instead of
/// coercing it to a bogus number.
///
/// The last four variants are how a program stays HONEST about a register no single number
/// answers. A context may resolve a die register (`rand`, `@ToHitRoll`) to [`Value::Die`] and a
/// bounded circumstance (`distance`) to [`Value::Range`], and the operators carry them through
/// exactly: a determinate chance compared against the die folds to [`Value::Bernoulli`], since
/// the game's own roll is `fRand < fChance` over a uniform [0,1) (`eval.c` `Random`,
/// `rule30Float`); monotone arithmetic moves a range's endpoints without widening them. Every
/// combination whose result would NOT be an exact probability or exact bounds is
/// [`EvalError::Indeterminate`]: the algebra never approximates.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Symbol(Box<str>),
    /// A uniform-[0,1) die the engine rolls fresh per evaluation. Carries the register's own
    /// name (`rand`, `@ToHitRoll`) so downstream reporting can say which roll.
    Die(Box<str>),
    /// A boolean that's true with probability `p`, a determinate number compared against
    /// [`Value::Die`].
    Bernoulli(f64),
    /// `value` with probability `p` and zero otherwise, [`Value::Bernoulli`] scaled by a
    /// determinate number.
    Chance {
        p: f64,
        value: f64,
    },
    /// A number known only to lie in `[lo, hi]`, a bounded register carried through monotone
    /// arithmetic. The endpoints are EXACT images of the register's own domain endpoints, never
    /// an interval over-approximation; `over` names the register the value ranges over.
    Range {
        lo: f64,
        hi: f64,
        over: Box<str>,
    },
    /// A term this context cannot value at all, carried on the STACK instead of aborting the
    /// program (EXPR-1). Carrying it is what lets `&&`/`||` apply Kleene truth — `false && x`
    /// is false whatever x is — while everything else propagates it unchanged. It never
    /// escapes the module: [`eval`] converts a result-position `Unknown` back into
    /// [`EvalError::Indeterminate`] with the same reason, so every external contract is
    /// exactly what it was.
    Unknown(Box<str>),
}

impl Value {
    /// CoH truthiness: a nonzero number is true, zero is false. A symbol isn't a boolean, and
    /// using one where a condition is required is a malformed program, surfaced rather than
    /// silently coerced. A die-derived value has no single truth either, but that's an
    /// UNKNOWN rather than a grammar fault, so it's Indeterminate.
    pub fn truthy(&self) -> Result<bool, EvalError> {
        match self {
            Value::Number(n) => Ok(*n != 0.0),
            Value::Symbol(s) => Err(EvalError::Malformed(
                format!("symbol {s:?} used as a boolean").into(),
            )),
            // An Unknown keeps its ORIGINAL reason — it already says what couldn't be valued,
            // and wrapping it in "used as a boolean" would bury the reader's name the
            // unresolved-gate reporting shows.
            Value::Unknown(reason) => Err(EvalError::Indeterminate(reason.clone())),
            stochastic => Err(EvalError::Indeterminate(
                format!("{} used as a boolean", describe(stochastic)).into(),
            )),
        }
    }

    /// The numeric value. A symbol here is [`EvalError::Malformed`], because arithmetic and
    /// numeric comparison never coerce an identifier. A die-derived value is
    /// [`EvalError::Indeterminate`] instead: the program is well-formed, the value just isn't
    /// one number.
    fn number(&self) -> Result<f64, EvalError> {
        match self {
            Value::Number(n) => Ok(*n),
            Value::Symbol(s) => Err(EvalError::Malformed(
                format!("symbol {s:?} used as a number").into(),
            )),
            Value::Unknown(reason) => Err(EvalError::Indeterminate(reason.clone())),
            stochastic => Err(EvalError::Indeterminate(
                format!("{} where one number is required", describe(stochastic)).into(),
            )),
        }
    }
}

/// A value as an Indeterminate reason names it, slotted into the callers' "depends on {…}"
/// phrasing, so each reads as a noun.
fn describe(value: &Value) -> String {
    match value {
        Value::Number(_) => "a number".to_string(),
        Value::Symbol(s) => format!("symbol {s:?}"),
        Value::Die(name) => format!("the {name} roll"),
        Value::Bernoulli(_) => "a pass/fail roll".to_string(),
        Value::Chance { .. } => "a rolled value".to_string(),
        Value::Range { over, .. } => format!("a value ranged over {over}"),
        Value::Unknown(reason) => reason.to_string(),
    }
}

/// Why an expression didn't yield a value.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// The expression depends on runtime state this context doesn't model: a target entity, a
    /// die roll, an unimplemented reader. NOT an error and NOT `false`: the caller decides what an
    /// unknown gate means. Carries the token (or reason) that couldn't be resolved.
    Indeterminate(Box<str>),
    /// The expression is structurally broken *for this grammar*: a stack underflow, a symbol
    /// where a number/boolean was required, an unknown operator, or an empty program. This marks a
    /// grammar gap or a genuinely malformed source string; a real corpus expression must never be
    /// Malformed, so this is loud, never swallowed.
    Malformed(Box<str>),
}

/// Supplies the runtime values the grammar's readers name. An expression is a pure function of
/// its context: same context, same result. Readers a context can't know (a specific target, a
/// die roll) must return [`EvalError::Indeterminate`], never a fabricated value, which would be
/// the invention this whole subsystem exists to avoid.
pub trait EvalContext {
    /// Resolve a reader or `@`-constant to a value, given its already-popped `operands` in push
    /// order (empty for a nullary reader; `operands[0]` is the single operand of a unary reader
    /// such as the attribute name in `kMeter source>`).
    fn resolve(&self, reader: &str, operands: &[Value]) -> Result<Value, EvalError>;
}

/// The language's operators: the fixed opcodes, as opposed to the open-ended reader and symbol
/// vocabulary the context owns.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    NumberEqual,
    Equal,
    And,
    Or,
    Not,
    MinMax,
    Duplicate,
    Power,
    Negate,
}

/// One token's role in the grammar.
enum Term {
    Number(f64),
    Operator(Op),
    /// A reader or `@`-constant that consumes `arity` stack operands and resolves via the context.
    Reader {
        arity: usize,
    },
    /// A bare identifier, pushed as [`Value::Symbol`] and consumed by a reader or `eq`.
    Symbol,
}

/// Classify a token structurally. The order matters: a numeric literal wins over everything
/// (including a bare `-` sign, since `"-0.1"` parses but `"-"` doesn't), then the fixed
/// operators, then readers, and finally the symbol fallback for the open identifier space.
fn classify(token: &str) -> Term {
    if let Ok(n) = token.parse::<f64>() {
        return Term::Number(n);
    }
    if let Some(op) = operator(token) {
        return Term::Operator(op);
    }
    // `@`-constants (`@StdResult`, `@ToHit`, …) are nullary reads of a combat register.
    if token.starts_with('@') {
        return Term::Reader { arity: 0 };
    }
    if let Some(arity) = reader_arity(token) {
        return Term::Reader { arity };
    }
    Term::Symbol
}

fn operator(token: &str) -> Option<Op> {
    Some(match token {
        "+" => Op::Add,
        "-" => Op::Sub,
        "*" => Op::Mul,
        "/" => Op::Div,
        ">" => Op::Greater,
        "<" => Op::Less,
        ">=" => Op::GreaterEqual,
        "<=" => Op::LessEqual,
        "==" => Op::NumberEqual,
        "eq" => Op::Equal,
        "&&" => Op::And,
        "||" => Op::Or,
        "!" => Op::Not,
        "minmax" => Op::MinMax,
        "dup" => Op::Duplicate,
        "pow" => Op::Power,
        "negate" => Op::Negate,
        _ => return None,
    })
}

/// How many stack operands a reader consumes. This table IS the reader grammar, transcribed
/// from the corpus: the entity readers (`source>`, `target>`) take the attribute name as an
/// operand, the dotted membership/event readers take the name they test, and the readers whose
/// attribute is baked into the token (`source.TeamSize>`, `power.base>`, `mapname>`) take none.
/// A token ending in `>`/`?` that's absent here falls through to a symbol and surfaces as
/// [`EvalError::Malformed`] the moment it reaches an operator, so a newly-seen reader fails
/// loudly rather than being silently mistaken for an operand.
fn reader_arity(token: &str) -> Option<usize> {
    Some(match token {
        // Entity-value readers: pop an attribute symbol, return that entity's value for it.
        "source>" | "Source>" | "target>" | "maintarget>" | "source.owner>" | "target.owner>" => 1,

        // The caster-ownership readers, matched case-folded: the export spells the same reader
        // `source.ownPower?`, `source.OwnPower?` and an all-lower `source.ownpower?`, and an
        // exact-string arm left the third a bare symbol. Latent rather than live, since the only
        // carrier of the lower spelling is `Assassins_Mark_Proc`, which no bundle currently
        // ships. The failure mode is why it's closed anyway: a lone unrecognized reader never
        // reaches an operator, so it yields a Symbol result instead of the Malformed this
        // table's fall-through promises, and nothing would have said so.
        _ if is_caster_ownership_reader(token) => 1,

        // Membership / mode / tag / event readers: pop the name they test.
        "target.ownPower?"
        | "target.Owned?"
        | "target.ownPowerNum?"
        | "source.Mode?"
        | "Source.Mode?"
        | "source.mode?"
        | "target.mode?"
        | "target.Mode?"
        | "target.HasTag?"
        | "target.hasTag?"
        | "source.TokenOwned?"
        | "target.TokenOwned?"
        | "source.EventTimeSince>"
        | "target.EventTimeSince>"
        | "source.EventCount>"
        | "target.EventCount>"
        | "source.TokenTime>"
        | "target.TokenTime>"
        | "source.inVolume>"
        | "target.inVolume>"
        | "arcvar>"
        | "ZoneEvent>"
        | "ScriptMessage>"
        | "auth>"
        | "source.ProductOwned?" => 1,

        // Nullary readers: the attribute is baked into the token, so there's no operand.
        // (`target.TickDamage` is the odd one out, a dotted reader with no `>`/`?` sigil, read
        // standalone as in `target.TickDamage 0 >`.)
        "source.TeamSize>"
        | "isPVPMap?"
        | "power.base>"
        | "power.boosted>"
        | "target.isFriend?"
        | "Challenge?"
        | "target.Challenge?"
        | "target.VillainName>"
        | "source.VillainName>"
        | "source.MapTeamArea>"
        | "mapname>"
        | "source.isAccountServerAvailable?"
        | "target.TickDamage" => 0,

        // Bareword runtime scalars. They stand where a number would, so they must read (and, in
        // a build-only context, be Indeterminate) rather than parse as a symbol and jam an operator.
        "now" | "distance" | "prevdistance" | "combatlevel" | "activatetime" | "rechargetime"
        | "activateperiod" | "areafactor" | "rand" => 0,

        _ => return None,
    })
}

/// Evaluate an RPN expression against `ctx`, returning the value left on top of the stack.
///
/// Errors are total: a reader the context can't resolve yields [`EvalError::Indeterminate`]
/// (propagated), and any structural fault yields [`EvalError::Malformed`]. Operands left beneath
/// the result are ignored, per the engine convention.
pub fn eval(expression: &[Box<str>], ctx: &dyn EvalContext) -> Result<Value, EvalError> {
    let mut stack: Vec<Value> = Vec::new();
    for token in expression.iter().map(|t| &**t) {
        match classify(token) {
            Term::Number(n) => stack.push(Value::Number(n)),
            Term::Symbol => stack.push(Value::Symbol(token.into())),
            Term::Operator(op) => apply_operator(op, &mut stack)?,
            Term::Reader { arity } => {
                let operands = pop_n(&mut stack, arity, || token.to_string())?;
                // An unknown reaching a reader's operand slot, or a reader the context can't
                // resolve, stays ON the stack as [`Value::Unknown`] (EXPR-1) so a downstream
                // `&&`/`||` can still absorb it. Malformed still aborts: a grammar fault is a
                // fault whatever the operands were.
                if let Some(reason) = unknown_among(&operands) {
                    stack.push(Value::Unknown(reason));
                    continue;
                }
                match ctx.resolve(token, &operands) {
                    Err(EvalError::Indeterminate(reason)) => stack.push(Value::Unknown(reason)),
                    other => stack.push(other?),
                }
            }
        }
    }
    match stack.pop() {
        // The boundary conversion (EXPR-1): an unknown in RESULT position is the same
        // Indeterminate it was before unknowns rode the stack, so no consumer sees the variant.
        Some(Value::Unknown(reason)) => Err(EvalError::Indeterminate(reason)),
        Some(value) => Ok(value),
        None => Err(EvalError::Malformed("empty expression".into())),
    }
}

/// The reason of the first [`Value::Unknown`] among `values`, if any.
fn unknown_among(values: &[Value]) -> Option<Box<str>> {
    values.iter().find_map(|v| match v {
        Value::Unknown(reason) => Some(reason.clone()),
        _ => None,
    })
}

/// Tokenize an expression WRITTEN as text — a test literal, a hand-authored oracle, a `.powers`
/// def quoted in a comment.
///
/// Never for data off the wire: an expression that came from the export already knows its own
/// token boundaries, and splitting it back apart is what COND-8 was. Naming this separately is
/// what keeps the two apart at a glance.
pub fn tokens_from_text(text: &str) -> Vec<Box<str>> {
    text.split_whitespace().map(Box::from).collect()
}

/// The token list a JSON gate field holds.
///
/// Converter-written gates (`formVariants[].condition`, `modeVariants`, …) are token arrays for
/// the same reason the export's own are: a joined gate cannot be re-split once an operand
/// contains a space (DATA-GAP-REGISTER COND-8). `None` when the field is absent or is not an
/// array of strings — a malformed gate is not silently an empty one.
pub fn json_tokens(value: Option<&serde_json::Value>) -> Option<Vec<Box<str>>> {
    coh_data::expression_tokens(value)
}

/// Evaluate `expression` as a boolean gate, the truthiness of its result.
pub fn eval_bool(expression: &[Box<str>], ctx: &dyn EvalContext) -> Result<bool, EvalError> {
    eval(expression, ctx)?.truthy()
}

/// Pop the top `n` values, returned in push order (`out[0]` was pushed first). Underflow is a
/// malformed program. `token` is lazy so the happy path allocates nothing.
fn pop_n(
    stack: &mut Vec<Value>,
    n: usize,
    token: impl FnOnce() -> String,
) -> Result<Vec<Value>, EvalError> {
    if stack.len() < n {
        return Err(EvalError::Malformed(
            format!(
                "`{}` needs {n} operand(s) but the stack held {}",
                token(),
                stack.len()
            )
            .into(),
        ));
    }
    Ok(stack.split_off(stack.len() - n))
}

fn apply_operator(op: Op, stack: &mut Vec<Value>) -> Result<(), EvalError> {
    match op {
        // Binary arithmetic: exact on two numbers, exact endpoint motion on a range and a
        // number, a probability fold on a Bernoulli and a number, Indeterminate anywhere
        // exactness would be lost.
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Power => {
            let [a, b] = pop2(stack, op)?;
            let result = arithmetic(op, a, b)?;
            stack.push(result);
        }

        // Numeric comparisons: push 1.0 / 0.0, or the exact probability when one side is the die.
        Op::Greater | Op::Less | Op::GreaterEqual | Op::LessEqual | Op::NumberEqual => {
            let [a, b] = pop2(stack, op)?;
            // `==` between two names is the game's string equality: `eval.c` NumericEqual hands
            // the pair to StringEqual ("The user probably meant to use string equality"), and
            // one in seven `arch source>` checks is spelled `Class_Stalker ==`. The parser's
            // `_gate_default.py` reads it the same way.
            if let (Op::NumberEqual, Value::Symbol(x), Value::Symbol(y)) = (op, &a, &b) {
                stack.push(boolean(x.eq_ignore_ascii_case(y)));
                return Ok(());
            }
            let result = comparison(op, a, b)?;
            stack.push(result);
        }

        // Symbol-or-number equality. Identifiers compare case-insensitively because the
        // exports carry casing variants of the same identity (`Class_Scrapper` / `class_scrapper`,
        // `source.Mode?` / `Source.Mode?`); numbers compare exactly; a symbol never equals a
        // number. A die-derived operand has no identity to compare, and folding it to `false`
        // would decode an unknown into an answer.
        Op::Equal => {
            let [a, b] = pop2(stack, op)?;
            // An unknown operand has no identity to compare — unknown, carried on (EXPR-1).
            if let Value::Unknown(reason) = &a {
                stack.push(Value::Unknown(reason.clone()));
                return Ok(());
            }
            if let Value::Unknown(reason) = &b {
                stack.push(Value::Unknown(reason.clone()));
                return Ok(());
            }
            let result = match (&a, &b) {
                (Value::Number(x), Value::Number(y)) => x == y,
                (Value::Symbol(x), Value::Symbol(y)) => x.eq_ignore_ascii_case(y),
                (Value::Number(_), Value::Symbol(_)) | (Value::Symbol(_), Value::Number(_)) => {
                    false
                }
                // `Unknown` is in the lists only for exhaustiveness — the early returns above
                // have already carried it past this match.
                (
                    stochastic @ (Value::Die(_)
                    | Value::Bernoulli(_)
                    | Value::Chance { .. }
                    | Value::Range { .. }
                    | Value::Unknown(_)),
                    _,
                )
                | (
                    _,
                    stochastic @ (Value::Die(_)
                    | Value::Bernoulli(_)
                    | Value::Chance { .. }
                    | Value::Range { .. }
                    | Value::Unknown(_)),
                ) => {
                    return Err(EvalError::Indeterminate(
                        format!("an eq against {}", describe(stochastic)).into(),
                    ))
                }
            };
            stack.push(boolean(result));
        }

        // Binary logical: Kleene truth (EXPR-1). A definite absorbing operand answers whatever
        // the other one is — `false && x` is false, `true || x` is true, exactly — and only a
        // question two-valued logic genuinely can't answer stays Unknown. A symbol is still a
        // grammar fault, checked first so Kleene can't paper over a malformed program.
        Op::And | Op::Or => {
            let [a, b] = pop2(stack, op)?;
            let absorbing = op != Op::And;
            let result = if let Some(reason) = unknown_among(&[a.clone(), b.clone()]) {
                // An unknown operand: the definite sibling can still absorb it (`false && x`,
                // `true || x`), else the result is that unknown. The symbol fault is NOT
                // raised on this path — the pre-EXPR-1 machine aborted at the unresolved
                // reader before ever judging the sibling, and that precedence is kept.
                if [&a, &b]
                    .into_iter()
                    .any(|v| definite_truth(v) == Some(absorbing))
                {
                    boolean(absorbing)
                } else {
                    Value::Unknown(reason)
                }
            } else {
                let (ta, tb) = (kleene_truth(&a)?, kleene_truth(&b)?);
                match (ta, tb) {
                    (x, y) if x == Some(absorbing) || y == Some(absorbing) => boolean(absorbing),
                    (Some(x), Some(y)) => boolean(if op == Op::And { x && y } else { x || y }),
                    _ => unknown_operand(&a, &b),
                }
            };
            stack.push(result);
        }

        // Unary logical. `!unknown` is unknown — negation has no absorbing operand.
        Op::Not => {
            let a = pop1(stack, op)?;
            let result = match kleene_truth(&a)? {
                Some(truth) => boolean(!truth),
                None => Value::Unknown(unknown_reason(&a)),
            };
            stack.push(result);
        }

        // Unary arithmetic negation. On a range, the endpoints swap exactly.
        Op::Negate => {
            let a = match die_as_range(pop1(stack, op)?) {
                Value::Range { lo, hi, over } => range(-hi, -lo, over),
                Value::Unknown(reason) => Value::Unknown(reason),
                other => Value::Number(-other.number()?),
            };
            stack.push(a);
        }

        // Clamp: `value low high minmax` → value bounded to [low, high]. Clamping is monotone,
        // so a ranged value's endpoints clamp independently and stay exact. That's how the
        // unbounded `distance` register becomes finite in the programs that saturate it.
        Op::MinMax => {
            let [value, low, high] = pop3(stack, op)?;
            if let Some(reason) = unknown_among(&[value.clone(), low.clone(), high.clone()]) {
                stack.push(Value::Unknown(reason));
                return Ok(());
            }
            let (low, high) = (low.number()?, high.number()?);
            let clamped = match die_as_range(value) {
                Value::Range { lo, hi, over } => {
                    range(lo.max(low).min(high), hi.max(low).min(high), over)
                }
                other => Value::Number(other.number()?.max(low).min(high)),
            };
            stack.push(clamped);
        }

        // Duplicate the top of stack (`… dup *` squares it). Duplicating a die or range is
        // fine in itself; an operator that would COMBINE the two copies refuses there, which
        // keeps every surviving range exact.
        Op::Duplicate => {
            let a = pop1(stack, op)?;
            stack.push(a.clone());
            stack.push(a);
        }
    }
    Ok(())
}

/// A value's definite truth, `None` when no single boolean states it (a die-derived value, a
/// straddling range, an [`Value::Unknown`]) — the three-valued read `&&`/`||`/`!` fold over.
/// A symbol is still [`EvalError::Malformed`]: Kleene absorbs unknowns, never grammar faults.
/// A range whose whole interval lies on one side of zero IS its truth — both endpoints agree,
/// so no exactness is lost (`0 < lo` or `hi < 0` … but `lo <= 0 <= hi` spans both answers).
fn kleene_truth(value: &Value) -> Result<Option<bool>, EvalError> {
    match value {
        Value::Symbol(s) => Err(EvalError::Malformed(
            format!("symbol {s:?} used as a boolean").into(),
        )),
        other => Ok(definite_truth(other)),
    }
}

/// [`kleene_truth`] without the grammar judgment: `None` for a symbol too. Used where an
/// unknown sibling has already suppressed the fault (see the `Op::And | Op::Or` arm).
fn definite_truth(value: &Value) -> Option<bool> {
    match value {
        Value::Number(n) => Some(*n != 0.0),
        Value::Range { lo, hi, .. } if *lo > 0.0 || *hi < 0.0 => Some(true),
        Value::Range { lo, hi, .. } if *lo == 0.0 && *hi == 0.0 => Some(false),
        _ => None,
    }
}

/// The `Unknown` a logical operator emits when neither operand decides it: the first operand
/// with no definite truth names the reason.
fn unknown_operand(a: &Value, b: &Value) -> Value {
    let culprit = if matches!(kleene_truth(a), Ok(Some(_))) {
        b
    } else {
        a
    };
    Value::Unknown(unknown_reason(culprit))
}

/// The reason string an indefinite value carries into an `Unknown` — the original reason for
/// one that already is `Unknown`, the value's own description otherwise.
fn unknown_reason(value: &Value) -> Box<str> {
    match value {
        Value::Unknown(reason) => reason.clone(),
        other => format!("{} used as a boolean", describe(other)).into(),
    }
}

/// A die entering ARITHMETIC is spent down to its bounds: only a comparison reads its uniform
/// measure, so past this point what flows on is the exact interval [0, 1).
fn die_as_range(value: Value) -> Value {
    match value {
        Value::Die(name) => Value::Range {
            lo: 0.0,
            hi: 1.0,
            over: name,
        },
        other => other,
    }
}

/// A range that collapses back to the number it is, because exactness means a pinched interval
/// IS its point.
fn range(lo: f64, hi: f64, over: Box<str>) -> Value {
    if lo == hi {
        Value::Number(lo)
    } else {
        Value::Range { lo, hi, over }
    }
}

/// The die's uniform-[0,1) measure of a probability operand: the game rolls `fRand < fChance`
/// with `fRand` from `rule30Float()`, so a chance at or below 0 never passes and one at or
/// above 1 always does.
fn clamp01(p: f64) -> f64 {
    p.clamp(0.0, 1.0)
}

fn arithmetic(op: Op, a: Value, b: Value) -> Result<Value, EvalError> {
    let unsupported = |a: &Value, b: &Value| {
        let noun = match op {
            Op::Add => "sum",
            Op::Sub => "difference",
            Op::Mul => "product",
            Op::Div => "quotient",
            Op::Power => "power",
            _ => unreachable!(),
        };
        EvalError::Indeterminate(
            format!("the {noun} of {} and {}", describe(a), describe(b)).into(),
        )
    };
    match (die_as_range(a), die_as_range(b)) {
        (Value::Number(a), Value::Number(b)) => {
            let result = match op {
                Op::Add => a + b,
                Op::Sub => a - b,
                Op::Mul => a * b,
                Op::Div => {
                    if b == 0.0 {
                        return Err(EvalError::Malformed("division by zero".into()));
                    }
                    a / b
                }
                Op::Power => a.powf(b),
                _ => unreachable!(),
            };
            Ok(Value::Number(result))
        }

        // An unknown operand makes an unknown result, carried on (EXPR-1) — checked BEFORE the
        // symbol fault below, because the pre-EXPR-1 machine aborted at the unresolved reader
        // and never reached this judgment: keeping that precedence is what keeps every corpus
        // program's verdict Indeterminate-or-better, never newly Malformed.
        (Value::Unknown(reason), _) | (_, Value::Unknown(reason)) => Ok(Value::Unknown(reason)),

        // A symbol in an arithmetic slot is a grammar fault regardless of the other operand.
        (a @ Value::Symbol(_), _) | (_, a @ Value::Symbol(_)) => Err(EvalError::Malformed(
            format!("{} used as a number", describe(&a)).into(),
        )),

        // A pass/fail roll scales into "value with probability"; any other arithmetic on it
        // has no single-value or single-probability answer.
        (Value::Bernoulli(p), Value::Number(value))
        | (Value::Number(value), Value::Bernoulli(p))
            if op == Op::Mul =>
        {
            Ok(Value::Chance { p, value })
        }
        (Value::Chance { p, value }, Value::Number(k))
        | (Value::Number(k), Value::Chance { p, value })
            if op == Op::Mul =>
        {
            Ok(Value::Chance {
                p,
                value: value * k,
            })
        }

        // A range against a number: every op here is monotone in the ranged operand, so the
        // result's endpoints are the images of the operand's: exact, not interval arithmetic.
        (Value::Range { lo, hi, over }, Value::Number(k)) => match op {
            Op::Add => Ok(range(lo + k, hi + k, over)),
            Op::Sub => Ok(range(lo - k, hi - k, over)),
            Op::Mul if k > 0.0 => Ok(range(lo * k, hi * k, over)),
            Op::Mul if k < 0.0 => Ok(range(hi * k, lo * k, over)),
            Op::Mul => Ok(Value::Number(0.0)),
            Op::Div => {
                if k == 0.0 {
                    return Err(EvalError::Malformed("division by zero".into()));
                }
                if k > 0.0 {
                    Ok(range(lo / k, hi / k, over))
                } else {
                    Ok(range(hi / k, lo / k, over))
                }
            }
            _ => Err(unsupported(
                &Value::Range { lo, hi, over },
                &Value::Number(k),
            )),
        },
        (Value::Number(k), Value::Range { lo, hi, over }) => match op {
            Op::Add => Ok(range(k + lo, k + hi, over)),
            Op::Sub => Ok(range(k - hi, k - lo, over)),
            Op::Mul if k > 0.0 => Ok(range(k * lo, k * hi, over)),
            Op::Mul if k < 0.0 => Ok(range(k * hi, k * lo, over)),
            Op::Mul => Ok(Value::Number(0.0)),
            // k ÷ range crosses a pole wherever the range spans zero, and no shipped program
            // divides BY a ranged value, so it's refused rather than approximated.
            _ => Err(unsupported(
                &Value::Number(k),
                &Value::Range { lo, hi, over },
            )),
        },

        // Two die-derived operands (a dup'd roll, two registers): their combination depends on
        // the joint distribution, which one interval can't state exactly.
        (a, b) => Err(unsupported(&a, &b)),
    }
}

fn comparison(op: Op, a: Value, b: Value) -> Result<Value, EvalError> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let result = match op {
                Op::Greater => a > b,
                Op::Less => a < b,
                Op::GreaterEqual => a >= b,
                Op::LessEqual => a <= b,
                Op::NumberEqual => a == b,
                _ => unreachable!(),
            };
            Ok(boolean(result))
        }

        // An unknown operand makes an unknown comparison, carried on (EXPR-1) — before the
        // symbol fault, for the same precedence reason as in `arithmetic`.
        (Value::Unknown(reason), _) | (_, Value::Unknown(reason)) => Ok(Value::Unknown(reason)),

        (a @ Value::Symbol(_), _) | (_, a @ Value::Symbol(_)) => Err(EvalError::Malformed(
            format!("{} used as a number", describe(&a)).into(),
        )),

        // A determinate chance against the die is the game's own roll, and the uniform measure
        // prices it exactly. The die is continuous, so strict and non-strict compare alike, and
        // `==` against it is measure-zero. An authored one would be a defect to surface, not a
        // 0 to fold.
        (Value::Number(chance), Value::Die(_)) => match op {
            Op::Greater | Op::GreaterEqual => Ok(Value::Bernoulli(clamp01(chance))),
            Op::Less | Op::LessEqual => Ok(Value::Bernoulli(1.0 - clamp01(chance))),
            _ => Err(EvalError::Indeterminate(
                "an equality against a die roll".into(),
            )),
        },
        (Value::Die(_), Value::Number(chance)) => match op {
            Op::Less | Op::LessEqual => Ok(Value::Bernoulli(clamp01(chance))),
            Op::Greater | Op::GreaterEqual => Ok(Value::Bernoulli(1.0 - clamp01(chance))),
            _ => Err(EvalError::Indeterminate(
                "an equality against a die roll".into(),
            )),
        },

        (a, b) => Err(EvalError::Indeterminate(
            format!("a comparison of {} and {}", describe(&a), describe(&b)).into(),
        )),
    }
}

fn boolean(b: bool) -> Value {
    Value::Number(if b { 1.0 } else { 0.0 })
}

fn pop1(stack: &mut Vec<Value>, op: Op) -> Result<Value, EvalError> {
    stack
        .pop()
        .ok_or_else(|| EvalError::Malformed(format!("{op:?} underflow: needs 1 operand").into()))
}

fn pop2(stack: &mut Vec<Value>, op: Op) -> Result<[Value; 2], EvalError> {
    let v = pop_n(stack, 2, || format!("{op:?}"))?;
    let mut it = v.into_iter();
    Ok([it.next().unwrap(), it.next().unwrap()])
}

fn pop3(stack: &mut Vec<Value>, op: Op) -> Result<[Value; 3], EvalError> {
    let v = pop_n(stack, 3, || format!("{op:?}"))?;
    let mut it = v.into_iter();
    Ok([it.next().unwrap(), it.next().unwrap(), it.next().unwrap()])
}

/// The attribute symbol under which the build's own to-hit chance is supplied to
/// [`SourceContext::source_attributes`]: what the fork snipes' fast-form gate reads
/// (`cur.kToHit source> .97 >=`) and what the `@ToHit` register resolves from. Schema
/// vocabulary (an export attribute name), not a game proper noun. No THRESHOLD is here,
/// because each threshold belongs to its fork's own condition string.
pub const CURRENT_TO_HIT: &str = "cur.kToHit";

/// The attribute symbol under which the build's HIDE state is supplied to
/// [`SourceContext::source_attributes`]: what the from-Hide openers' redirect reads
/// (`kMeter source> .9 <` selects the Quick form, `Always` the Stealth one) and what the
/// Stalker/Widow hide-bonus damage atoms gate on (`kMeter source> 0 >`).
///
/// Schema vocabulary, and deliberately the BARE spelling. The corpus reads this attribute under
/// two distinct keys that mean different things: `kMeter source>` (1,455 occurrences) is the
/// boolean hide family, while `cur.kMeter source>` (50, Beast Mastery's Fortify Pack and
/// Thunderspy's Primalist) reads the meter as a scaling QUANTITY. They're separate map keys, so
/// binding this one can't reach those programs. That's a second guard beside
/// [`crate::gather::publishes_hide_meter`], which is the one that carries the argument.
/// `kMeter target>` (150) is target-side live state and stays Indeterminate by construction.
///
/// No THRESHOLD is here, for the same reason [`CURRENT_TO_HIT`] carries none: each threshold
/// belongs to its own condition string, and the forks spell theirs differently.
pub const HIDE_METER: &str = "kMeter";

/// The readers that ask whether the CASTER holds a power, lower-cased for comparison.
///
/// Their target-side twins (`target.ownPower?`, `target.ownPowerNum?`) are deliberately absent:
/// who is being hit is a per-cast fact, not a build one, and only the source side can be answered
/// from a build. The bare `ownPowerNum?` the corpus also carries is absent for a different
/// reason: it appears only in a power's `requires` (the PICK rule), never in an effect gate, and
/// `coh_data::pick_rules` owns that evaluator.
pub const CASTER_OWNERSHIP_READERS: [&str; 3] =
    ["source.ownpower?", "source.owned?", "source.ownpowernum?"];

/// Is `token` one of [`CASTER_OWNERSHIP_READERS`], whatever case the fork spelled it in?
///
/// Case-folded in place rather than by lower-casing the token: this runs over every token of every
/// gate in the dataset when the ownership corpus is collected, and an allocation per token means
/// hundreds of thousands of them per build recalculation.
pub fn is_caster_ownership_reader(token: &str) -> bool {
    CASTER_OWNERSHIP_READERS
        .iter()
        .any(|reader| token.eq_ignore_ascii_case(reader))
}

/// The dotted power paths `expression` reads through a caster-ownership reader.
///
/// The grammar is postfix, so the path is the token immediately BEFORE its
/// reader. The pairing is what makes the read structural. A bare scan for dotted tokens would
/// also collect the operands of `target.ownPower?`, which name a power the TARGET might hold and
/// no build-side fact can answer.
pub fn caster_owned_paths(expression: &[Box<str>]) -> impl Iterator<Item = &str> {
    let mut previous: Option<&str> = None;
    expression.iter().map(|t| &**t).filter_map(move |token| {
        let path = previous.filter(|_| is_caster_ownership_reader(token));
        previous = Some(token);
        path
    })
}

/// Fold a power path to the one spelling [`SourceContext::owned_powers`] is keyed by.
///
/// The corpus spells one power two ways: `Temporary_Powers.Temporary_Powers.Tidal_Power` and
/// `temporary_powers.temporary_powers.tidal_power` both appear, on Homecoming and Thunderspy
/// respectively. An exact-string map would therefore answer the same question differently
/// depending on which fork authored the gate. `coh_data`'s own path resolver folds case for the
/// same reason.
pub fn normalize_power_path(path: &str) -> String {
    path.to_ascii_lowercase()
}

/// A context that resolves *source-side*, statically-known-build facts and returns
/// [`EvalError::Indeterminate`] for everything a character-totals calculation can't know: a
/// specific target, a die roll, or an unsupplied runtime combat meter.
///
/// The distinction is principled. A build always knows its own selected powers and chosen modes,
/// so `source.ownPower?` and `source.Mode?` resolve to a definite yes/no. A combat *meter*
/// (`kMeter source>` = Fury/Rage) or the team size is dynamic; it resolves only when the combat
/// context supplies it, and is Indeterminate otherwise. Anything target-relative is Indeterminate
/// by construction, because there's no target in a self-totals calculation.
#[derive(Debug, Default, Clone)]
pub struct SourceContext {
    /// Total team size including the caster (solo = 1). Feeds `source.TeamSize>`. `None` when the
    /// build didn't specify a team size (Indeterminate).
    pub team_size: Option<f64>,
    /// Live source attribute/meter values readable via `<attr> source>` (e.g. `kMeter`, `kRage`,
    /// `cur.kHeld`), keyed by the attribute symbol. An absent key is a runtime unknown →
    /// Indeterminate, not zero.
    pub source_attributes: HashMap<String, f64>,
    /// Powers the caster has active, by path, for `<path> source.ownPower?` (owned = count > 0)
    /// and `source.ownPowerNum?` (the count). A build knows its powers, so an absent path is a
    /// definite "not owned", not Indeterminate.
    pub owned_powers: HashMap<String, f64>,
    /// Active source modes / stances, for `<mode> source.Mode?`. Known from the build; absent =
    /// not active.
    pub source_modes: HashSet<String>,
    /// Whether the current map is a PvP map, for `isPVPMap?`. `None` → Indeterminate.
    pub is_pvp_map: Option<bool>,
    /// Who the caster is hitting, for the target-side readers (`arch target>`, `enttype target>`).
    /// `None` (the default, and what the totals loop passes) leaves every target reader
    /// Indeterminate, which is what a self-totals calculation means: there's no one target.
    /// A per-power damage projection is the caller that does have one.
    pub target: Option<TargetIdentity>,
    /// `arch source>`: the caster's own class token in the export's spelling (`Class_Stalker`).
    /// `None` leaves the reader Indeterminate. The damage fork gates conjoin this with live
    /// target state (`kHeld target> 0 > … arch source> Class_Stalker eq &&`), so the converter's
    /// `caster_archetypes` stamp can't carry them; answering it here lets `&&` absorb the other
    /// archetypes' arms instead of reporting them unresolved.
    pub caster_class: Option<String>,
}

/// The reader a target-rank gate asks through. It is Indeterminate exactly when no rank is chosen,
/// and callers match on it to offer the rank control rather than print the gate.
pub const RANK_READER: &str = "arch target>";

/// The target a per-power projection resolves against. Both fields hold the export's own
/// spelling because the gates compare against it directly (`arch target> Class_Minion_Grunt eq`);
/// storing a bucket enum instead would mean authoring the buckets, and the damage gates already
/// name them (Rule 0). `eq` folds case, so casing variants across forks compare equal.
///
/// The two fields are answered SEPARATELY because the reader answers them separately. Which side
/// of the PvE/PvP fork you are on is a thing anyone planning a build knows without being asked
/// (and the app has always defaulted it); which RANK you are hitting is a real choice, and 137 of
/// Homecoming's 1120 powers have a number that depends on it. Requiring both at once meant
/// neither was answered until the rank was picked, which left 612 powers reporting their whole
/// damage as unresolved when only the rank was ever missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIdentity {
    /// `arch target>`: the target's class token, e.g. `Class_Minion_Grunt`. `None` is a target
    /// whose RANK nobody has stated — the gates that ask stay Indeterminate and say so, while
    /// every gate that only asks `enttype` is answered.
    pub archetype_class: Option<String>,
    /// `enttype target>`: `critter` or `player`, the PvE/PvP fork every fork's damage atoms carry.
    pub entity_type: String,
}

impl EvalContext for SourceContext {
    fn resolve(&self, reader: &str, operands: &[Value]) -> Result<Value, EvalError> {
        let name = |i: usize| -> Result<String, EvalError> {
            match operands.get(i) {
                Some(Value::Symbol(s)) => Ok(s.to_string()),
                Some(Value::Number(n)) => Ok(n.to_string()),
                // A die or ranged operand names nothing. No reader takes one, so reaching
                // here is a program using a roll where an identifier belongs.
                Some(stochastic) => Err(EvalError::Indeterminate(
                    format!("{} as the operand of `{reader}`", describe(stochastic)).into(),
                )),
                None => Err(EvalError::Malformed(
                    format!("`{reader}` missing operand {i}").into(),
                )),
            }
        };
        let indeterminate = || Err(EvalError::Indeterminate(reader.into()));

        match reader {
            "source.TeamSize>" => self
                .team_size
                .map(Value::Number)
                .ok_or_else(|| EvalError::Indeterminate(reader.into())),

            "source>" | "Source>" if name(0)?.eq_ignore_ascii_case("arch") => self
                .caster_class
                .as_deref()
                .map(|class| Value::Symbol(class.into()))
                .ok_or_else(|| EvalError::Indeterminate(reader.into())),
            "source>" | "Source>" => self
                .source_attributes
                .get(&name(0)?)
                .copied()
                .map(Value::Number)
                .ok_or_else(|| EvalError::Indeterminate(reader.into())),

            // Case-folded for the same reason [`reader_arity`] is: the export spells this reader
            // three ways and an exact-string arm answered the all-lower one Indeterminate, which
            // is worse than a wrong definite answer because Indeterminate PROPAGATES. One
            // unmatched spelling unresolves the whole expression around it.
            _ if is_caster_ownership_reader(reader) => {
                let count = self
                    .owned_powers
                    .get(&normalize_power_path(&name(0)?))
                    .copied()
                    .unwrap_or(0.0);
                if reader.eq_ignore_ascii_case("source.ownPowerNum?") {
                    Ok(Value::Number(count))
                } else {
                    Ok(boolean(count > 0.0))
                }
            }
            "source.Mode?" | "Source.Mode?" | "source.mode?" => {
                Ok(boolean(self.source_modes.contains(&name(0)?)))
            }
            "isPVPMap?" => self
                .is_pvp_map
                .map(boolean)
                .ok_or_else(|| EvalError::Indeterminate(reader.into())),

            // The target's own identity, when the caller has one, plus the one attribute every
            // landed hit settles (Untouchable). Every other target attribute (its current HP,
            // what it's holding) is live combat state no projection knows, so it stays
            // Indeterminate rather than defaulted into a wrong definite answer.
            "target>" | "Target>" => {
                let Some(target) = self.target.as_ref() else {
                    return indeterminate();
                };
                match name(0)?.to_ascii_lowercase().as_str() {
                    // A target with no stated rank answers this one UNKNOWN rather than
                    // picking a rank: the gates here fork the number (a crit against a minion
                    // and against a boss are different probabilities), so a guess would be a
                    // definite wrong answer where Indeterminate is a true one.
                    "arch" => match target.archetype_class.as_deref() {
                        Some(class) => Ok(Value::Symbol(class.into())),
                        None => Err(EvalError::Indeterminate(RANK_READER.into())),
                    },
                    "enttype" => Ok(Value::Symbol(target.entity_type.clone().into())),
                    // An Untouchable target can't be affected at all, so a hit this projection
                    // values is already one on a target that isn't. Rebirth guards Force Bolt's
                    // base damage with `cur.kUntouchable target> 0 <=`, and leaving it unknown
                    // filed the attack's only hit as situational.
                    "cur.kuntouchable" => Ok(Value::Number(0.0)),
                    _ => indeterminate(),
                }
            }

            // Owner-relative and runtime reads have no answer in a self-totals context; consume
            // any operand for stack correctness, then report unknown.
            _ => indeterminate(),
        }
    }
}
