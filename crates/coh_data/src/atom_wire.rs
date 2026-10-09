//! Positional-tuple wire decode, mirrored 1:1 from `ATOM_TUPLE_FIELDS` in
//! `scripts/_atomic-effect.ts`. The field-name array also ships in
//! `contract/schema-version.json`; [`assert_schema`] compares it against this decoder's
//! order so a field reorder upstream is a loud load error instead of silent garbage.

use crate::atom::*;
use serde_json::Value;

/// Wire order authority. MUST match the TS `ATOM_TUPLE_FIELDS` exactly.
pub const ATOM_TUPLE_FIELDS: [&str; 47] = [
    "effectType",
    "subType",
    "scale",
    "magnitude",
    "duration",
    "modifierTable",
    "aspect",
    "attribType",
    "toWho",
    "pvMode",
    "resistible",
    "stacking",
    "stackCap",
    "ticks",
    "applicationPeriod",
    "baseProbability",
    "procsPerMinute",
    "ignoreStrength",
    "buffable",
    "ignoreED",
    "ignoreScaling",
    "specialCase",
    "requiresExpression",
    "gated",
    "perTarget",
    "suppressible",
    "notOnCaster",
    "stackKey",
    "magnitudeExpression",
    "requiredEvents",
    "tickChance",
    "cancelOnMiss",
    "tags",
    "casterArchetypes",
    "metaAttrib",
    "ownerTargets",
    "delay",
    "suppressEvents",
    "suppressSeconds",
    "suppressAlways",
    "cancelEvents",
    "applicationType",
    "summonWindow",
    "conditionalId",
    "stackByAttribAndKey",
    "redirectBase",
    "petClass",
];

#[derive(Debug, thiserror::Error)]
#[error("atom field {field:?} (index {index}): {problem}")]
pub struct AtomDecodeError {
    pub field: &'static str,
    pub index: usize,
    pub problem: String,
}

fn err(index: usize, problem: String) -> AtomDecodeError {
    AtomDecodeError {
        field: ATOM_TUPLE_FIELDS[index],
        index,
        problem,
    }
}

/// A `null` at position `i` means the field is absent; a short tuple leaves every field
/// past its end absent. No defaults are applied — absence decodes as `None`.
pub fn decode_atom(tuple: &[Value]) -> Result<AtomicEffect, AtomDecodeError> {
    if tuple.len() > ATOM_TUPLE_FIELDS.len() {
        return Err(err(
            ATOM_TUPLE_FIELDS.len() - 1,
            format!(
                "tuple has {} fields, wire schema has {}",
                tuple.len(),
                ATOM_TUPLE_FIELDS.len()
            ),
        ));
    }
    let get = |i: usize| tuple.get(i).filter(|v| !v.is_null());
    let s = |i: usize| -> Result<Option<&str>, AtomDecodeError> {
        get(i)
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| err(i, format!("expected string, got {v}")))
            })
            .transpose()
    };
    let f = |i: usize| -> Result<Option<f64>, AtomDecodeError> {
        get(i)
            .map(|v| {
                v.as_f64()
                    .ok_or_else(|| err(i, format!("expected number, got {v}")))
            })
            .transpose()
    };
    // A gate expression is a TOKEN ARRAY on the wire, not a joined string: the
    // game's `Requires` is one string-table offset per token, and Homecoming's
    // costume-FX values contain spaces, so a joined form cannot be re-split
    // (DATA-GAP-REGISTER COND-8).
    let toks = |i: usize| -> Result<Option<Box<[Box<str>]>>, AtomDecodeError> {
        get(i)
            .map(|v| {
                let arr = v
                    .as_array()
                    .ok_or_else(|| err(i, format!("expected token array, got {v}")))?;
                arr.iter()
                    .map(|t| {
                        t.as_str()
                            .map(Box::from)
                            .ok_or_else(|| err(i, format!("expected string token, got {t}")))
                    })
                    .collect::<Result<Vec<Box<str>>, _>>()
                    .map(Vec::into_boxed_slice)
            })
            .transpose()
    };
    let b = |i: usize| -> Result<Option<bool>, AtomDecodeError> {
        get(i)
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| err(i, format!("expected bool, got {v}")))
            })
            .transpose()
    };
    fn parse<T: std::str::FromStr<Err = String>>(
        i: usize,
        v: Option<&str>,
    ) -> Result<Option<T>, AtomDecodeError> {
        v.map(|s| s.parse::<T>().map_err(|e| err(i, e))).transpose()
    }

    Ok(AtomicEffect {
        effect_type: parse::<EffectType>(0, s(0)?)?,
        sub_type: parse::<SubType>(1, s(1)?)?,
        scale: f(2)?,
        magnitude: f(3)?,
        duration: f(4)?,
        modifier_table: s(5)?.map(Box::from),
        aspect: parse::<Aspect>(6, s(6)?)?,
        attrib_type: parse::<AttribType>(7, s(7)?)?,
        to_who: parse::<ToWho>(8, s(8)?)?,
        pv_mode: parse::<PvMode>(9, s(9)?)?,
        resistible: b(10)?,
        stacking: parse::<Stacking>(11, s(11)?)?,
        stack_cap: f(12)?,
        ticks: f(13)?,
        application_period: f(14)?,
        base_probability: f(15)?,
        procs_per_minute: f(16)?,
        ignore_strength: b(17)?,
        buffable: b(18)?,
        ignore_ed: b(19)?,
        ignore_scaling: b(20)?,
        special_case: s(21)?.map(Box::from),
        requires_expression: toks(22)?,
        gated: b(23)?,
        per_target: f(24)?,
        suppressible: b(25)?,
        not_on_caster: b(26)?,
        stack_key: s(27)?.map(Box::from),
        magnitude_expression: toks(28)?,
        required_events: s(29)?.map(Box::from),
        tick_chance: f(30)?,
        cancel_on_miss: b(31)?,
        tags: s(32)?.map(Box::from),
        caster_archetypes: s(33)?.map(Box::from),
        meta_attrib: s(34)?.map(Box::from),
        owner_targets: toks(35)?,
        delay: f(36)?,
        suppress_events: toks(37)?,
        suppress_seconds: f(38)?,
        suppress_always: b(39)?,
        cancel_events: toks(40)?,
        application_type: parse::<ApplicationType>(41, s(41)?)?,
        summon_window: f(42)?,
        conditional_id: s(43)?.map(Box::from),
        stack_by_attrib_and_key: b(44)?,
        redirect_base: f(45)?,
        pet_class: s(46)?.map(Box::from),
    })
}

/// Assert the contract's `schema-version.json` field order matches this decoder.
pub fn assert_schema(wire_fields: &[String]) -> Result<(), String> {
    if wire_fields.len() != ATOM_TUPLE_FIELDS.len()
        || wire_fields
            .iter()
            .zip(ATOM_TUPLE_FIELDS)
            .any(|(a, b)| a != b)
    {
        return Err(format!(
            "atom tuple schema drift: contract has {wire_fields:?}, decoder has {ATOM_TUPLE_FIELDS:?}"
        ));
    }
    Ok(())
}
