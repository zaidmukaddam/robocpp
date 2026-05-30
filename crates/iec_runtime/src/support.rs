// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{
    eval_standard_function, is_communication_function_block, is_standard_function,
    is_standard_void_function, standard_function_input_index,
};

use crate::configuration::*;
use crate::runtime::*;
use crate::state::*;
use crate::*;

pub(crate) enum Control {
    Continue,
    Exit,
    Return,
    Jump(String),
}

pub(crate) fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        text.chars().take(limit).collect()
    }
}

pub(crate) fn literal_to_value(project: &Project, literal: &Literal) -> Value {
    match literal {
        Literal::Int(value) => Value::Int(*value),
        Literal::Real(value) => Value::Real(*value),
        Literal::Bool(value) => Value::Bool(*value),
        Literal::String(value) => Value::String(value.clone()),
        Literal::WString(value) => Value::WString(value.clone()),
        Literal::DurationMs(value) => Value::TimeMs(*value),
        Literal::Date(value) => Value::TimeMs(parse_date_days(value).unwrap_or(0) as i128),
        Literal::TimeOfDay(value) => Value::TimeMs(parse_time_of_day_ms(value).unwrap_or(0)),
        Literal::DateAndTime(value) => Value::TimeMs(parse_date_time_ms(value).unwrap_or(0)),
        Literal::Typed { type_name, value } => typed_literal_value(project, type_name, value)
            .unwrap_or_else(|| Value::String(value.clone())),
    }
}

pub(crate) fn typed_literal_value(
    project: &Project,
    type_name: &Identifier,
    value: &str,
) -> Option<Value> {
    if let Some(elementary) = ElementaryType::parse(&type_name.original) {
        return typed_literal_elementary_value(elementary, value);
    }
    let spec = project
        .data_types()
        .find(|data_type| data_type.name.canonical == type_name.canonical)
        .map(|data_type| data_type.spec.clone())?;
    typed_literal_spec_value(project, &spec, value, &mut BTreeSet::new())
}

pub(crate) fn typed_literal_spec_value(
    project: &Project,
    spec: &DataTypeSpec,
    value: &str,
    seen: &mut BTreeSet<String>,
) -> Option<Value> {
    match resolve_project_spec(project, spec) {
        DataTypeSpec::Elementary(elementary) => typed_literal_elementary_value(elementary, value),
        DataTypeSpec::Subrange { .. } => typed_literal_i128(value)
            .and_then(|value| i64::try_from(value).ok())
            .map(Value::Int),
        DataTypeSpec::Enum { values } => {
            let value = canonical_identifier(value);
            values
                .iter()
                .position(|candidate| candidate.canonical == value)
                .map(|index| Value::Int(index as i64))
        }
        DataTypeSpec::String { wide, .. } => {
            if wide {
                Some(Value::WString(value.to_string()))
            } else {
                Some(Value::String(value.to_string()))
            }
        }
        DataTypeSpec::Named(name) => {
            if !seen.insert(name.canonical.clone()) {
                return None;
            }
            let data_type = project
                .data_types()
                .find(|data_type| data_type.name.canonical == name.canonical)?;
            typed_literal_spec_value(project, &data_type.spec, value, seen)
        }
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => None,
    }
}

pub(crate) fn typed_literal_elementary_value(
    elementary: ElementaryType,
    value: &str,
) -> Option<Value> {
    match elementary {
        ElementaryType::Bool => parse_typed_bool(value).map(Value::Bool),
        ElementaryType::Sint
        | ElementaryType::Int
        | ElementaryType::Dint
        | ElementaryType::Lint
        | ElementaryType::Usint
        | ElementaryType::Uint
        | ElementaryType::Udint
        | ElementaryType::Ulint
        | ElementaryType::Byte
        | ElementaryType::Word
        | ElementaryType::Dword
        | ElementaryType::Lword => typed_literal_i128(value)
            .and_then(|value| i64::try_from(value).ok())
            .map(Value::Int),
        ElementaryType::Real | ElementaryType::Lreal => parse_typed_real(value).map(Value::Real),
        ElementaryType::Time => typed_literal_i128(value)
            .or_else(|| parse_duration_ms_checked(value))
            .map(Value::TimeMs),
        ElementaryType::Date => parse_date_days(value).map(|value| Value::TimeMs(value as i128)),
        ElementaryType::TimeOfDay => parse_time_of_day_ms(value).map(Value::TimeMs),
        ElementaryType::DateAndTime => parse_date_time_ms(value).map(Value::TimeMs),
        ElementaryType::String => Some(Value::String(value.to_string())),
        ElementaryType::WString => Some(Value::WString(value.to_string())),
    }
}

pub(crate) fn parse_typed_bool(value: &str) -> Option<bool> {
    match canonical_identifier(value).as_str() {
        "TRUE" | "1" => Some(true),
        "FALSE" | "0" => Some(false),
        _ => None,
    }
}

pub(crate) fn parse_typed_real(value: &str) -> Option<f64> {
    let value = value.trim().replace('_', "");
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

pub(crate) fn typed_literal_i128(value: &str) -> Option<i128> {
    if let Some((base, digits)) = value.split_once('#') {
        let base = match canonical_identifier(base).as_str() {
            "2" => 2,
            "8" => 8,
            "16" => 16,
            _ => return None,
        };
        if !valid_integer_underscore_placement(digits) {
            return None;
        }
        return i128::from_str_radix(&digits.replace('_', ""), base).ok();
    }
    let value = value.trim();
    if !valid_integer_underscore_placement(value) {
        return None;
    }
    value.replace('_', "").parse::<i128>().ok()
}

pub(crate) fn valid_integer_underscore_placement(raw: &str) -> bool {
    let raw = raw
        .strip_prefix('-')
        .or_else(|| raw.strip_prefix('+'))
        .unwrap_or(raw);
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    !bytes.windows(2).any(|pair| pair == b"__")
}

pub(crate) fn parse_duration_ms_checked(raw: &str) -> Option<i128> {
    let mut chars = raw.replace('_', "").to_ascii_lowercase();
    let sign = if chars.starts_with('-') {
        chars.remove(0);
        -1_i128
    } else {
        1_i128
    };
    let mut rest = chars.as_str();
    if rest.is_empty() {
        return None;
    }

    let mut total = 0.0_f64;
    let mut previous_rank = 6_u8;
    let mut saw_component = false;
    while !rest.is_empty() {
        let number_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
            .map(char::len_utf8)
            .sum::<usize>();
        if number_len == 0 {
            return None;
        }
        let number_text = &rest[..number_len];
        if !valid_decimal_component(number_text) {
            return None;
        }
        let number = number_text.parse::<f64>().ok()?;
        rest = &rest[number_len..];
        let (factor, consumed, rank, unit) = if rest.starts_with("ms") {
            (1.0, 2, 1, "ms")
        } else if rest.starts_with('d') {
            (86_400_000.0, 1, 5, "d")
        } else if rest.starts_with('h') {
            (3_600_000.0, 1, 4, "h")
        } else if rest.starts_with('m') {
            (60_000.0, 1, 3, "m")
        } else if rest.starts_with('s') {
            (1_000.0, 1, 2, "s")
        } else {
            return None;
        };
        if rank >= previous_rank {
            return None;
        }
        let has_more = rest.get(consumed..).is_some_and(|tail| !tail.is_empty());
        if has_more && number_text.contains('.') {
            return None;
        }
        if has_more || previous_rank != 6 {
            match unit {
                "h" if number >= 24.0 => return None,
                "m" | "s" if number >= 60.0 => return None,
                "ms" if number >= 1000.0 => return None,
                _ => {}
            }
        }
        saw_component = true;
        previous_rank = rank;
        total += number * factor;
        rest = &rest[consumed..];
    }
    saw_component.then_some(sign * total.round() as i128)
}

pub(crate) fn valid_decimal_component(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    if bytes.windows(2).any(|pair| pair == b"__") {
        return false;
    }
    let mut dot_count = 0_u8;
    let mut digit_count = 0_usize;
    for ch in raw.chars() {
        if ch == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else if ch.is_ascii_digit() || ch == '_' {
            if ch.is_ascii_digit() {
                digit_count += 1;
            }
        } else {
            return false;
        }
    }
    digit_count > 0
}

pub(crate) fn parse_date_time_ms(input: &str) -> Option<i128> {
    if input.len() < 11 {
        return None;
    }
    let date = parse_date_days(input.get(..10)?)? as i128;
    let separator = input.as_bytes().get(10).copied()?;
    if separator != b'-' && separator != b'T' && separator != b't' {
        return None;
    }
    Some(date * 86_400_000 + parse_time_of_day_ms(input.get(11..)?)?)
}

pub(crate) fn parse_date_days(input: &str) -> Option<i64> {
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

pub(crate) fn parse_time_of_day_ms(input: &str) -> Option<i128> {
    let mut parts = input.split(':');
    let hour = parts.next()?.parse::<i128>().ok()?;
    let minute = parts.next()?.parse::<i128>().ok()?;
    let second_part = parts.next()?;
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    let (second, millis) = if let Some((seconds, fraction)) = second_part.split_once('.') {
        if fraction.is_empty() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        let mut millis_text = fraction.chars().take(3).collect::<String>();
        while millis_text.len() < 3 {
            millis_text.push('0');
        }
        (
            seconds.parse::<i128>().ok()?,
            millis_text.parse::<i128>().ok()?,
        )
    } else {
        (second_part.parse::<i128>().ok()?, 0)
    };
    if !(0..=59).contains(&second) {
        return None;
    }
    Some(((hour * 60 + minute) * 60 + second) * 1000 + millis)
}

pub(crate) fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub(crate) fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn array_element_count(ranges: &[Subrange]) -> usize {
    ranges.iter().fold(1_usize, |total, range| {
        total.saturating_mul((range.high - range.low + 1).max(0) as usize)
    })
}

pub(crate) fn runtime_spec_label(spec: &DataTypeSpec) -> &'static str {
    match spec {
        DataTypeSpec::Elementary(elementary) => match elementary {
            ElementaryType::Bool => "BOOL",
            ElementaryType::Sint
            | ElementaryType::Int
            | ElementaryType::Dint
            | ElementaryType::Lint
            | ElementaryType::Usint
            | ElementaryType::Uint
            | ElementaryType::Udint
            | ElementaryType::Ulint => "integer",
            ElementaryType::Real | ElementaryType::Lreal => "REAL",
            ElementaryType::Byte
            | ElementaryType::Word
            | ElementaryType::Dword
            | ElementaryType::Lword => "bit-string",
            ElementaryType::String => "STRING",
            ElementaryType::WString => "WSTRING",
            ElementaryType::Time => "TIME",
            ElementaryType::Date => "DATE",
            ElementaryType::TimeOfDay => "TIME_OF_DAY",
            ElementaryType::DateAndTime => "DATE_AND_TIME",
        },
        DataTypeSpec::String { wide, .. } => {
            if *wide {
                "WSTRING"
            } else {
                "STRING"
            }
        }
        DataTypeSpec::Subrange { .. } => "subrange",
        DataTypeSpec::Enum { .. } => "enumerated",
        DataTypeSpec::Array { .. } => "array",
        DataTypeSpec::Struct { .. } => "structure",
        DataTypeSpec::Named(_) => "value",
    }
}

pub(crate) fn runtime_value_label(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "BOOL",
        Value::Int(_) => "integer",
        Value::Real(_) => "REAL",
        Value::String(_) => "STRING",
        Value::WString(_) => "WSTRING",
        Value::TimeMs(_) => "TIME",
        Value::Array(_) => "array",
        Value::Struct(_) => "structure",
        Value::Unit => "unit",
    }
}

pub(crate) fn elementary_integer_range(
    elementary: &ElementaryType,
) -> Option<(&'static str, i128, i128)> {
    match elementary {
        ElementaryType::Sint => Some(("SINT", -128, 127)),
        ElementaryType::Usint | ElementaryType::Byte => Some((
            if matches!(elementary, ElementaryType::Byte) {
                "BYTE"
            } else {
                "USINT"
            },
            0,
            255,
        )),
        ElementaryType::Int => Some(("INT", -32_768, 32_767)),
        ElementaryType::Uint | ElementaryType::Word => Some((
            if matches!(elementary, ElementaryType::Word) {
                "WORD"
            } else {
                "UINT"
            },
            0,
            65_535,
        )),
        ElementaryType::Dint => Some(("DINT", -2_147_483_648, 2_147_483_647)),
        ElementaryType::Udint | ElementaryType::Dword => Some((
            if matches!(elementary, ElementaryType::Dword) {
                "DWORD"
            } else {
                "UDINT"
            },
            0,
            4_294_967_295,
        )),
        ElementaryType::Lint => Some(("LINT", i64::MIN as i128, i64::MAX as i128)),
        ElementaryType::Ulint | ElementaryType::Lword => Some((
            if matches!(elementary, ElementaryType::Lword) {
                "LWORD"
            } else {
                "ULINT"
            },
            0,
            i64::MAX as i128,
        )),
        _ => None,
    }
}

pub(crate) fn il_label_operand(expr: &Expr) -> Option<&Identifier> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some() || variable.path.len() != 1 {
        return None;
    }
    variable.root_name()
}

pub(crate) fn field_key(instance: &str, field: &str) -> String {
    format!(
        "{}.{}",
        canonical_identifier(instance),
        canonical_identifier(field)
    )
}

#[derive(Debug, Clone)]
pub(crate) struct FunctionBlockRuntimeField {
    pub(crate) name: String,
    pub(crate) spec: DataTypeSpec,
}

pub(crate) fn function_block_field_specs(
    project: &Project,
    spec: &DataTypeSpec,
) -> Option<Vec<FunctionBlockRuntimeField>> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    let fields = match type_name.canonical.as_str() {
        "SR" | "RS" => vec![("Q1", DataTypeSpec::Elementary(ElementaryType::Bool))],
        "R_TRIG" | "F_TRIG" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("M", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "CTU" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("CV", DataTypeSpec::Elementary(ElementaryType::Int)),
            ("_CU", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "CTD" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("CV", DataTypeSpec::Elementary(ElementaryType::Int)),
            ("_CD", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "CTUD" => vec![
            ("QU", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("QD", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("CV", DataTypeSpec::Elementary(ElementaryType::Int)),
            ("_CU", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("_CD", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "TON" | "TOF" | "TP" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("ET", DataTypeSpec::Elementary(ElementaryType::Time)),
            ("_IN", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("_RUN", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        name if is_communication_function_block(name) => vec![
            ("DONE", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("NDR", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("ERROR", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("STATUS", DataTypeSpec::Elementary(ElementaryType::Int)),
        ],
        _ => {
            let function_block = project
                .find_pou(&type_name.original)
                .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))?;
            return Some(
                function_block
                    .variable_declarations()
                    .flat_map(|field| {
                        let mut fields = vec![FunctionBlockRuntimeField {
                            name: field.name.canonical.clone(),
                            spec: field.type_spec.clone(),
                        }];
                        if field.edge.is_some() {
                            fields.push(FunctionBlockRuntimeField {
                                name: edge_state_field_name(&field.name.canonical),
                                spec: DataTypeSpec::Elementary(ElementaryType::Bool),
                            });
                        }
                        fields
                    })
                    .collect(),
            );
        }
    };
    Some(
        fields
            .into_iter()
            .map(|(name, spec)| FunctionBlockRuntimeField {
                name: name.to_string(),
                spec,
            })
            .collect(),
    )
}

pub(crate) fn flattened_field_key(variable: &VariableRef) -> Option<String> {
    if variable.direct.is_some()
        || variable.path.len() < 2
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    Some(
        variable
            .path
            .iter()
            .map(|part| part.canonical.as_str())
            .collect::<Vec<_>>()
            .join("."),
    )
}

pub(crate) fn sfc_step_key(step: &Identifier) -> String {
    format!("$SFC_STEP_{}", step.canonical)
}

pub(crate) fn sfc_transition_steps<'a>(
    sfc: &'a Sfc,
    transition: &'a SfcTransition,
    index: usize,
) -> Option<(Vec<&'a Identifier>, Vec<&'a Identifier>)> {
    if !transition.from.is_empty() || !transition.to.is_empty() {
        if transition.from.is_empty() || transition.to.is_empty() {
            return None;
        }
        return Some((
            transition.from.iter().collect(),
            transition.to.iter().collect(),
        ));
    }

    let from = &sfc.steps.get(index)?.name;
    let to = &sfc.steps.get(index + 1)?.name;
    Some((vec![from], vec![to]))
}

pub(crate) struct SfcActionInput<'a> {
    pub(crate) qualifier: SfcActionQualifier,
    pub(crate) duration: Option<&'a Literal>,
    pub(crate) active: bool,
}

pub(crate) fn sfc_action_inputs<'a>(
    sfc: &'a Sfc,
    action: &'a SfcAction,
    active_steps: &[String],
) -> Vec<SfcActionInput<'a>> {
    let mut inputs = Vec::new();
    for step in &sfc.steps {
        let active = active_steps.contains(&step.name.canonical);
        for association in &step.actions {
            if association.name.canonical != action.name.canonical {
                continue;
            }
            inputs.push(SfcActionInput {
                qualifier: association.qualifier.unwrap_or(action.qualifier),
                duration: association.duration.as_ref().or(action.duration.as_ref()),
                active,
            });
        }
    }

    if inputs.is_empty() {
        let active = active_steps.contains(&action.name.canonical);
        inputs.push(SfcActionInput {
            qualifier: action.qualifier,
            duration: action.duration.as_ref(),
            active,
        });
    }

    inputs
}

pub(crate) fn sfc_action_control_key(action: &Identifier) -> String {
    action.canonical.clone()
}

pub(crate) fn sfc_action_control_key_stored(key: &str) -> String {
    format!("$SFC_ACTION_{key}")
}

pub(crate) fn sfc_action_control_key_previous(key: &str) -> String {
    format!("$SFC_ACTION_PREVIOUS_{key}")
}

pub(crate) fn sfc_action_control_key_elapsed(key: &str) -> String {
    format!("$SFC_ACTION_ELAPSED_{key}")
}

pub(crate) fn sfc_action_duration_ms(duration: Option<&Literal>) -> i128 {
    match duration {
        Some(Literal::DurationMs(value)) => (*value).max(0),
        Some(Literal::Int(value)) => (*value as i128).max(0),
        _ => 0,
    }
}

pub(crate) fn is_implicit_en(name: &Identifier) -> bool {
    name.canonical == "EN"
}

pub(crate) fn is_implicit_eno(name: &Identifier) -> bool {
    name.canonical == "ENO"
}

pub(crate) fn split_input_expr(args: &[ParamAssignment]) -> Option<&Expr> {
    args.iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(|name| name.canonical == "IN"))
        .and_then(|arg| arg.expr.as_ref())
        .or_else(|| split_positional_args(args).first().copied())
}

pub(crate) fn split_positional_args(args: &[ParamAssignment]) -> Vec<&Expr> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter(|arg| arg.name.is_none())
        .filter_map(|arg| arg.expr.as_ref())
        .collect()
}

pub(crate) fn split_formal_output<'a>(
    args: &'a [ParamAssignment],
    output: &str,
) -> Option<&'a VariableRef> {
    args.iter()
        .find(|arg| {
            arg.output
                && arg
                    .name
                    .as_ref()
                    .is_some_and(|name| name.canonical == output)
        })
        .and_then(|arg| arg.variable.as_ref())
}

pub(crate) fn split_output_variable<'a>(
    args: &'a [ParamAssignment],
    output: &str,
    positional_index: usize,
) -> Option<&'a VariableRef> {
    split_formal_output(args, output).or_else(|| {
        let positional = split_positional_args(args);
        let Expr::Variable(variable) = positional.get(positional_index + 1)? else {
            return None;
        };
        Some(variable)
    })
}

pub(crate) fn input_time_value(value: &Value) -> i128 {
    match value {
        Value::TimeMs(value) => *value,
        value => value.as_i64().unwrap_or(0) as i128,
    }
}

pub(crate) fn civil_from_days(days: i128) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_prime = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * month_prime + 2).div_euclid(5) + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i64, month as i64, day as i64)
}

pub(crate) fn tod_parts(ms: i128) -> (i64, i64, i64, i64) {
    let ms = ms.rem_euclid(86_400_000);
    let hour = ms / 3_600_000;
    let minute = (ms % 3_600_000) / 60_000;
    let second = (ms % 60_000) / 1_000;
    let millisecond = ms % 1_000;
    (
        hour as i64,
        minute as i64,
        second as i64,
        millisecond as i64,
    )
}

pub(crate) fn is_standard_function_block_type(name: &str) -> bool {
    matches!(
        canonical_identifier(name).as_str(),
        "SR" | "RS" | "R_TRIG" | "F_TRIG" | "CTU" | "CTD" | "CTUD" | "TON" | "TOF" | "TP"
    ) || is_communication_function_block(name)
}

pub(crate) fn standard_function_block_output_names(name: &str) -> &'static [&'static str] {
    match canonical_identifier(name).as_str() {
        "SR" | "RS" => &["Q1"],
        "R_TRIG" | "F_TRIG" => &["Q"],
        "CTU" | "CTD" => &["Q", "CV"],
        "CTUD" => &["QU", "QD", "CV"],
        "TON" | "TOF" | "TP" => &["Q", "ET"],
        name if is_communication_function_block(name) => &["DONE", "NDR", "ERROR", "STATUS"],
        _ => &[],
    }
}

pub(crate) fn standard_function_block_input_names(name: &str) -> &'static [&'static str] {
    match canonical_identifier(name).as_str() {
        "SR" => &["S1", "R"],
        "RS" => &["S", "R1"],
        "R_TRIG" | "F_TRIG" => &["CLK"],
        "CTU" => &["CU", "R", "PV"],
        "CTD" => &["CD", "LD", "PV"],
        "CTUD" => &["CU", "CD", "R", "LD", "PV"],
        "TON" | "TOF" | "TP" => &["IN", "PT"],
        name if is_communication_function_block(name) => &["REQ", "EN_R", "ID", "LEN"],
        _ => &[],
    }
}

pub(crate) fn user_fb_input_target(
    input_fields: &[(VarBlockKind, Identifier)],
    arg: &ParamAssignment,
    positional_index: &mut usize,
) -> Option<(VarBlockKind, Identifier)> {
    if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
        return None;
    }
    if let Some(name) = &arg.name {
        input_fields
            .iter()
            .find(|(_, field)| field.canonical == name.canonical)
            .cloned()
    } else {
        let target = input_fields.get(*positional_index).cloned();
        *positional_index += 1;
        target
    }
}

pub(crate) fn user_fb_input_edge(function_block: &Pou, input_name: &str) -> Option<EdgeQualifier> {
    function_block
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Input)
        .flat_map(|block| block.vars.iter())
        .find(|var| var.name.canonical == input_name)
        .and_then(|var| var.edge)
}

pub(crate) fn edge_state_field_name(input_name: &str) -> String {
    format!("$EDGE_{}", canonical_identifier(input_name))
}

pub(crate) fn input_bool(inputs: &BTreeMap<String, Value>, name: &str) -> bool {
    inputs
        .get(&canonical_identifier(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn input_i64(inputs: &BTreeMap<String, Value>, name: &str) -> i64 {
    inputs
        .get(&canonical_identifier(name))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

pub(crate) fn input_time_ms(inputs: &BTreeMap<String, Value>, name: &str) -> i128 {
    match inputs.get(&canonical_identifier(name)) {
        Some(Value::TimeMs(value)) => *value,
        Some(value) => value.as_i64().unwrap_or(0) as i128,
        None => 0,
    }
}

pub(crate) fn bit_bool_binary(
    left: Value,
    right: Value,
    int_op: fn(i64, i64) -> i64,
    bool_op: fn(bool, bool) -> bool,
) -> Option<Value> {
    if matches!(left, Value::Bool(_)) && matches!(right, Value::Bool(_)) {
        Some(Value::Bool(bool_op(left.as_bool()?, right.as_bool()?)))
    } else {
        Some(Value::Int(int_op(left.as_i64()?, right.as_i64()?)))
    }
}

pub(crate) fn compare_values(left: &Value, right: &Value) -> Option<i8> {
    if value_text(left).is_some() || value_text(right).is_some() {
        let left = value_text(left).unwrap_or_else(|| left.to_string());
        let right = value_text(right).unwrap_or_else(|| right.to_string());
        return Some(match left.cmp(&right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
    }

    let left = left.as_f64()?;
    let right = right.as_f64()?;
    if (left - right).abs() < f64::EPSILON {
        Some(0)
    } else if left < right {
        Some(-1)
    } else {
        Some(1)
    }
}

pub(crate) fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) | Value::WString(value) => Some(value.clone()),
        _ => None,
    }
}
