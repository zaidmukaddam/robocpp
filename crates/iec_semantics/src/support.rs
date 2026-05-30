// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_ir::*;
use iec_stdlib::{
    is_communication_function_block, standard_function_input_index, standard_symbols,
    StandardSymbolKind,
};

use crate::Checker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimpleType {
    Bool,
    Integer,
    Real,
    BitString,
    String,
    WString,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
    Enum,
    Aggregate,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenericFamily {
    Any,
    #[allow(dead_code)]
    AnyDerived,
    AnyElementary,
    AnyMagnitude,
    AnyNum,
    AnyReal,
    AnyInt,
    AnyBit,
    AnyString,
    #[allow(dead_code)]
    AnyDate,
    BitString,
    Bool,
    String,
    WString,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
}

impl GenericFamily {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Any => "ANY",
            Self::AnyDerived => "ANY_DERIVED",
            Self::AnyElementary => "ANY_ELEMENTARY",
            Self::AnyMagnitude => "ANY_MAGNITUDE",
            Self::AnyNum => "ANY_NUM",
            Self::AnyReal => "ANY_REAL",
            Self::AnyInt => "ANY_INT",
            Self::AnyBit => "ANY_BIT",
            Self::AnyString => "ANY_STRING",
            Self::AnyDate => "ANY_DATE",
            Self::BitString => "bit-string",
            Self::Bool => "BOOL",
            Self::String => "STRING",
            Self::WString => "WSTRING",
            Self::Time => "TIME",
            Self::Date => "DATE",
            Self::TimeOfDay => "TIME_OF_DAY",
            Self::DateAndTime => "DATE_AND_TIME",
        }
    }

    pub(crate) fn contains(self, actual: SimpleType) -> bool {
        if actual == SimpleType::Unknown || self == Self::Any {
            return true;
        }
        match self {
            Self::Any => true,
            Self::AnyDerived => matches!(actual, SimpleType::Enum | SimpleType::Aggregate),
            Self::AnyElementary => !matches!(actual, SimpleType::Aggregate),
            Self::AnyMagnitude => matches!(
                actual,
                SimpleType::Integer | SimpleType::Real | SimpleType::Time
            ),
            Self::AnyNum => matches!(actual, SimpleType::Integer | SimpleType::Real),
            Self::AnyReal => actual == SimpleType::Real,
            Self::AnyInt => actual == SimpleType::Integer,
            Self::AnyBit => matches!(
                actual,
                SimpleType::Bool | SimpleType::Integer | SimpleType::BitString
            ),
            Self::AnyString => matches!(actual, SimpleType::String | SimpleType::WString),
            Self::AnyDate => matches!(
                actual,
                SimpleType::Date | SimpleType::TimeOfDay | SimpleType::DateAndTime
            ),
            Self::BitString => actual == SimpleType::BitString,
            Self::Bool => actual == SimpleType::Bool,
            Self::String => actual == SimpleType::String,
            Self::WString => actual == SimpleType::WString,
            Self::Time => actual == SimpleType::Time,
            Self::Date => actual == SimpleType::Date,
            Self::TimeOfDay => actual == SimpleType::TimeOfDay,
            Self::DateAndTime => actual == SimpleType::DateAndTime,
        }
    }
}

impl SimpleType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "BOOL",
            Self::Integer => "integer",
            Self::Real => "REAL",
            Self::BitString => "bit-string",
            Self::String => "STRING",
            Self::WString => "WSTRING",
            Self::Time => "TIME",
            Self::Date => "DATE",
            Self::TimeOfDay => "TIME_OF_DAY",
            Self::DateAndTime => "DATE_AND_TIME",
            Self::Enum => "enumerated",
            Self::Aggregate => "aggregate",
            Self::Unknown => "unknown",
        }
    }

    pub(crate) fn numeric_or_unknown(self) -> Self {
        match self {
            Self::Integer | Self::Real | Self::Unknown => self,
            _ => Self::Unknown,
        }
    }
}

pub(crate) fn types_are_assignable(expected: SimpleType, actual: SimpleType) -> bool {
    match (expected, actual) {
        (SimpleType::Unknown, _) | (_, SimpleType::Unknown) => true,
        (left, right) if left == right => true,
        (SimpleType::Real, SimpleType::Integer) => true,
        (SimpleType::BitString, SimpleType::Integer) => true,
        (SimpleType::Integer, SimpleType::BitString) => true,
        _ => false,
    }
}

pub(crate) fn types_have_common_value_type(left: SimpleType, right: SimpleType) -> bool {
    types_are_assignable(left, right) || types_are_assignable(right, left)
}

pub(crate) fn literal_type(literal: &Literal, project: &Project) -> SimpleType {
    match literal {
        Literal::Int(_) => SimpleType::Integer,
        Literal::Real(_) => SimpleType::Real,
        Literal::Bool(_) => SimpleType::Bool,
        Literal::String(_) => SimpleType::String,
        Literal::WString(_) => SimpleType::WString,
        Literal::DurationMs(_) => SimpleType::Time,
        Literal::Date(_) => SimpleType::Date,
        Literal::TimeOfDay(_) => SimpleType::TimeOfDay,
        Literal::DateAndTime(_) => SimpleType::DateAndTime,
        Literal::Typed { type_name, .. } => ElementaryType::parse(&type_name.original)
            .map(|elementary| elementary_type(&elementary))
            .or_else(|| typed_literal_named_type(project, type_name))
            .unwrap_or(SimpleType::Unknown),
    }
}

pub(crate) fn typed_literal_named_type(
    project: &Project,
    type_name: &Identifier,
) -> Option<SimpleType> {
    typed_literal_named_type_inner(project, type_name, &mut BTreeSet::new())
}

pub(crate) fn typed_literal_named_type_inner(
    project: &Project,
    type_name: &Identifier,
    seen: &mut BTreeSet<String>,
) -> Option<SimpleType> {
    if !seen.insert(type_name.canonical.clone()) {
        return Some(SimpleType::Unknown);
    }
    let data_type = project
        .data_types()
        .find(|data_type| data_type.name.canonical == type_name.canonical)?;
    match &data_type.spec {
        DataTypeSpec::Elementary(elementary) => Some(elementary_type(elementary)),
        DataTypeSpec::Subrange { base, .. } => Some(elementary_type(base)),
        DataTypeSpec::Enum { .. } => Some(SimpleType::Enum),
        DataTypeSpec::String { wide, .. } => Some(if *wide {
            SimpleType::WString
        } else {
            SimpleType::String
        }),
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => Some(SimpleType::Aggregate),
        DataTypeSpec::Named(next) => typed_literal_named_type_inner(project, next, seen),
    }
}

pub(crate) fn is_numeric_simple(simple: SimpleType) -> bool {
    matches!(simple, SimpleType::Integer | SimpleType::Real)
}

pub(crate) fn is_bitwise_simple(simple: SimpleType) -> bool {
    matches!(simple, SimpleType::Integer | SimpleType::BitString)
}

pub(crate) fn unary_op_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "NOT",
    }
}

pub(crate) fn binary_op_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "OR",
        BinaryOp::Xor => "XOR",
        BinaryOp::And => "AND",
        BinaryOp::Equal => "=",
        BinaryOp::NotEqual => "<>",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "MOD",
        BinaryOp::Power => "**",
    }
}

pub(crate) fn il_op_name(op: IlOp) -> &'static str {
    match op {
        IlOp::Ld => "LD",
        IlOp::Ldn => "LDN",
        IlOp::St => "ST",
        IlOp::Stn => "STN",
        IlOp::S => "S",
        IlOp::R => "R",
        IlOp::And => "AND",
        IlOp::Andn => "ANDN",
        IlOp::Or => "OR",
        IlOp::Orn => "ORN",
        IlOp::Xor => "XOR",
        IlOp::Xorn => "XORN",
        IlOp::Not => "NOT",
        IlOp::Add => "ADD",
        IlOp::Sub => "SUB",
        IlOp::Mul => "MUL",
        IlOp::Div => "DIV",
        IlOp::Mod => "MOD",
        IlOp::Gt => "GT",
        IlOp::Ge => "GE",
        IlOp::Eq => "EQ",
        IlOp::Ne => "NE",
        IlOp::Le => "LE",
        IlOp::Lt => "LT",
        IlOp::Jmp => "JMP",
        IlOp::Jmpc => "JMPC",
        IlOp::Jmpcn => "JMPCN",
        IlOp::Cal => "CAL",
        IlOp::Calc => "CALC",
        IlOp::Calcn => "CALCN",
        IlOp::Ret => "RET",
        IlOp::Retc => "RETC",
        IlOp::Retcn => "RETCN",
    }
}

pub(crate) fn case_range_label(low: i128, high: i128) -> String {
    if low == high {
        low.to_string()
    } else {
        format!("{low}..{high}")
    }
}

pub(crate) fn elementary_type(elementary: &ElementaryType) -> SimpleType {
    match elementary {
        ElementaryType::Bool => SimpleType::Bool,
        ElementaryType::Sint
        | ElementaryType::Int
        | ElementaryType::Dint
        | ElementaryType::Lint
        | ElementaryType::Usint
        | ElementaryType::Uint
        | ElementaryType::Udint
        | ElementaryType::Ulint => SimpleType::Integer,
        ElementaryType::Real | ElementaryType::Lreal => SimpleType::Real,
        ElementaryType::Byte
        | ElementaryType::Word
        | ElementaryType::Dword
        | ElementaryType::Lword => SimpleType::BitString,
        ElementaryType::String => SimpleType::String,
        ElementaryType::WString => SimpleType::WString,
        ElementaryType::Time => SimpleType::Time,
        ElementaryType::Date => SimpleType::Date,
        ElementaryType::TimeOfDay => SimpleType::TimeOfDay,
        ElementaryType::DateAndTime => SimpleType::DateAndTime,
    }
}

pub(crate) fn standard_function_return_type(
    name: &Identifier,
    args: &[ParamAssignment],
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> SimpleType {
    let arg_types = ordered_standard_function_input_exprs(name, args)
        .into_iter()
        .map(|expr| checker.type_of_expr(expr, variables, project))
        .collect::<Vec<_>>();

    match name.canonical.as_str() {
        name if bcd_conversion_return_type(name).is_some() => {
            bcd_conversion_return_type(name).unwrap()
        }
        name if conversion_return_type(name).is_some() => conversion_return_type(name).unwrap(),
        "MOVE" => arg_types.first().copied().unwrap_or(SimpleType::Unknown),
        "ABS" => arg_types
            .first()
            .copied()
            .map(SimpleType::numeric_or_unknown)
            .unwrap_or(SimpleType::Unknown),
        "TRUNC" => SimpleType::Integer,
        "SQRT" | "LN" | "LOG" | "EXP" | "SIN" | "COS" | "TAN" => SimpleType::Real,
        "EXPT" => SimpleType::Real,
        "ADD" | "SUB" | "MUL" | "DIV" | "MOD" => {
            if arg_types.contains(&SimpleType::Real) {
                SimpleType::Real
            } else if arg_types.contains(&SimpleType::Unknown) {
                SimpleType::Unknown
            } else {
                SimpleType::Integer
            }
        }
        "MIN" | "MAX" => {
            if arg_types.contains(&SimpleType::Real) {
                SimpleType::Real
            } else if arg_types.contains(&SimpleType::Unknown) {
                SimpleType::Unknown
            } else {
                arg_types.first().copied().unwrap_or(SimpleType::Unknown)
            }
        }
        "LIMIT" => arg_types.get(1).copied().unwrap_or(SimpleType::Unknown),
        "SEL" => arg_types.get(2).copied().unwrap_or(SimpleType::Unknown),
        "MUX" => arg_types.get(1).copied().unwrap_or(SimpleType::Unknown),
        "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" => SimpleType::Bool,
        "SHL" | "SHR" | "ROL" | "ROR" => SimpleType::Integer,
        "AND" | "OR" | "XOR" => {
            if arg_types
                .iter()
                .all(|arg_type| *arg_type == SimpleType::Bool)
            {
                SimpleType::Bool
            } else {
                SimpleType::BitString
            }
        }
        "NOT" => arg_types.first().copied().unwrap_or(SimpleType::Unknown),
        "LEN" | "FIND" => SimpleType::Integer,
        "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => {
            if arg_types.contains(&SimpleType::WString) {
                SimpleType::WString
            } else {
                SimpleType::String
            }
        }
        "ADD_TIME" | "SUB_TIME" | "MUL_TIME" | "DIV_TIME" | "MULTIME" | "DIVTIME"
        | "SUB_DATE_DATE" | "SUB_TOD_TOD" | "SUB_DT_DT" => SimpleType::Time,
        "ADD_TOD_TIME" | "SUB_TOD_TIME" => SimpleType::TimeOfDay,
        "ADD_DT_TIME" | "SUB_DT_TIME" => SimpleType::DateAndTime,
        "CONCAT_DATE" => SimpleType::Date,
        "CONCAT_TOD" => SimpleType::TimeOfDay,
        "CONCAT_DT" | "CONCAT_DATE_TOD" => SimpleType::DateAndTime,
        "DAY_OF_WEEK" => SimpleType::Integer,
        "BOOL_TO_INT" | "REAL_TO_INT" => SimpleType::Integer,
        "INT_TO_BOOL" => SimpleType::Bool,
        "INT_TO_REAL" => SimpleType::Real,
        _ => SimpleType::Unknown,
    }
}

pub(crate) fn ordered_standard_function_input_exprs<'a>(
    name: &Identifier,
    args: &'a [ParamAssignment],
) -> Vec<&'a Expr> {
    let mut ordered = Vec::new();
    let mut positional_index = 0;
    let mut unknown_index = usize::MAX.saturating_sub(args.len());

    for arg in args {
        if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
            continue;
        }
        let Some(expr) = arg.expr.as_ref() else {
            continue;
        };
        let index = if let Some(arg_name) = &arg.name {
            standard_function_input_index(&name.original, &arg_name.original).unwrap_or_else(|| {
                let index = unknown_index;
                unknown_index = unknown_index.saturating_add(1);
                index
            })
        } else {
            let index = positional_index;
            positional_index += 1;
            index
        };
        ordered.push((index, expr));
    }

    ordered.sort_by_key(|(index, _)| *index);
    ordered.into_iter().map(|(_, expr)| expr).collect()
}

pub(crate) fn conversion_return_type(name: &str) -> Option<SimpleType> {
    let (_, target) = name.split_once("_TO_")?;
    match target {
        "BOOL" => Some(SimpleType::Bool),
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT" => {
            Some(SimpleType::Integer)
        }
        "BYTE" | "WORD" | "DWORD" | "LWORD" => Some(SimpleType::BitString),
        "REAL" | "LREAL" => Some(SimpleType::Real),
        "STRING" => Some(SimpleType::String),
        "WSTRING" => Some(SimpleType::WString),
        "TIME" => Some(SimpleType::Time),
        "DATE" => Some(SimpleType::Date),
        "TOD" | "TIME_OF_DAY" => Some(SimpleType::TimeOfDay),
        "DT" | "DATE_AND_TIME" => Some(SimpleType::DateAndTime),
        _ => None,
    }
}

pub(crate) fn conversion_source_family(name: &str) -> Option<GenericFamily> {
    let (source, _) = name.split_once("_TO_")?;
    conversion_type_family(source)
}

pub(crate) fn conversion_type_family(name: &str) -> Option<GenericFamily> {
    match name {
        "BOOL" => Some(GenericFamily::Bool),
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT" => {
            Some(GenericFamily::AnyInt)
        }
        "BYTE" | "WORD" | "DWORD" | "LWORD" => Some(GenericFamily::BitString),
        "REAL" | "LREAL" => Some(GenericFamily::AnyReal),
        "STRING" => Some(GenericFamily::String),
        "WSTRING" => Some(GenericFamily::WString),
        "TIME" => Some(GenericFamily::Time),
        "DATE" => Some(GenericFamily::Date),
        "TOD" | "TIME_OF_DAY" => Some(GenericFamily::TimeOfDay),
        "DT" | "DATE_AND_TIME" => Some(GenericFamily::DateAndTime),
        _ => None,
    }
}

pub(crate) fn bcd_conversion_return_type(name: &str) -> Option<SimpleType> {
    if name == "BCD_TO_INT" {
        return Some(SimpleType::Integer);
    }
    if name == "INT_TO_BCD" {
        return Some(SimpleType::BitString);
    }
    if name.split_once("_BCD_TO_").is_some() {
        return Some(SimpleType::Integer);
    }
    if name.split_once("_TO_BCD_").is_some() {
        return Some(SimpleType::BitString);
    }
    None
}

pub(crate) fn bcd_conversion_source_family(name: &str) -> Option<GenericFamily> {
    if name == "BCD_TO_INT" {
        return Some(GenericFamily::BitString);
    }
    if name == "INT_TO_BCD" {
        return Some(GenericFamily::AnyInt);
    }
    if name.split_once("_BCD_TO_").is_some() {
        return Some(GenericFamily::BitString);
    }
    if name.split_once("_TO_BCD_").is_some() {
        return Some(GenericFamily::AnyInt);
    }
    None
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BcdConversionKind {
    BcdToInt { digits: Option<u32> },
    IntToBcd { digits: Option<u32> },
}

pub(crate) fn bcd_conversion_kind(name: &str) -> Option<BcdConversionKind> {
    if name == "BCD_TO_INT" {
        return Some(BcdConversionKind::BcdToInt { digits: None });
    }
    if name == "INT_TO_BCD" {
        return Some(BcdConversionKind::IntToBcd { digits: None });
    }
    if let Some((source, _target)) = name.split_once("_BCD_TO_") {
        return Some(BcdConversionKind::BcdToInt {
            digits: bcd_digit_capacity(source),
        });
    }
    if let Some((_source, target)) = name.split_once("_TO_BCD_") {
        return Some(BcdConversionKind::IntToBcd {
            digits: bcd_digit_capacity(target),
        });
    }
    None
}

pub(crate) fn bcd_digit_capacity(name: &str) -> Option<u32> {
    match name {
        "BYTE" => Some(2),
        "WORD" => Some(4),
        "DWORD" => Some(8),
        "LWORD" => Some(16),
        _ => None,
    }
}

pub(crate) fn bcd_decode_i128(value: i128, digits: Option<u32>) -> Option<i128> {
    if value < 0 {
        return None;
    }
    let mut raw = value as u128;
    if let Some(digits) = digits {
        let bits = digits.saturating_mul(4);
        let mask = if bits >= 128 {
            u128::MAX
        } else {
            (1_u128 << bits) - 1
        };
        if raw & !mask != 0 {
            return None;
        }
    }
    let mut result = 0_i128;
    let mut place = 1_i128;
    while raw != 0 {
        let digit = (raw & 0x0f) as i128;
        if digit > 9 {
            return None;
        }
        result = result.checked_add(digit.checked_mul(place)?)?;
        place = place.checked_mul(10)?;
        raw >>= 4;
    }
    Some(result)
}

pub(crate) fn bcd_encode_i128(value: i128, digits: Option<u32>) -> Option<i128> {
    if value < 0 {
        return None;
    }
    let max_digits = digits.unwrap_or(16);
    let mut decimal = value;
    let mut raw = 0_i128;
    let mut used_digits = 0_u32;
    if decimal == 0 {
        return Some(0);
    }
    while decimal != 0 {
        if used_digits >= max_digits {
            return None;
        }
        let digit = decimal % 10;
        raw |= digit << (used_digits * 4);
        decimal /= 10;
        used_digits += 1;
    }
    Some(raw)
}

pub(crate) fn conversion_target_integer_range(name: &str) -> Option<(&'static str, i128, i128)> {
    let (_, target) = name.split_once("_TO_")?;
    match target {
        "SINT" => Some(("SINT", -128, 127)),
        "USINT" | "BYTE" => Some((if target == "BYTE" { "BYTE" } else { "USINT" }, 0, 255)),
        "INT" => Some(("INT", -32_768, 32_767)),
        "UINT" | "WORD" => Some((if target == "WORD" { "WORD" } else { "UINT" }, 0, 65_535)),
        "DINT" => Some(("DINT", -2_147_483_648, 2_147_483_647)),
        "UDINT" | "DWORD" => Some((
            if target == "DWORD" { "DWORD" } else { "UDINT" },
            0,
            4_294_967_295,
        )),
        "LINT" => Some(("LINT", i64::MIN as i128, i64::MAX as i128)),
        "ULINT" | "LWORD" => Some((
            if target == "LWORD" { "LWORD" } else { "ULINT" },
            0,
            i64::MAX as i128,
        )),
        _ => None,
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

pub(crate) fn enum_expr_name(expr: &Expr) -> Option<String> {
    if let Expr::Literal(Literal::Typed { value, .. }) = expr {
        return Some(canonical_identifier(value));
    }

    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some()
        || variable.path.len() != 1
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    variable.root_name().map(|name| name.canonical.clone())
}

pub(crate) fn enum_type_root(project: &Project, type_name: &Identifier) -> Option<String> {
    let mut current = type_name.canonical.clone();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        let data_type = project
            .data_types()
            .find(|data_type| data_type.name.canonical == current)?;
        match &data_type.spec {
            DataTypeSpec::Enum { .. } => return Some(data_type.name.canonical.clone()),
            DataTypeSpec::Named(next) => current = next.canonical.clone(),
            _ => return None,
        }
    }
}

pub(crate) fn enum_type_name_for_spec(
    project: &Project,
    spec: &DataTypeSpec,
) -> Option<Identifier> {
    match spec {
        DataTypeSpec::Named(name) => {
            let data_type = project
                .data_types()
                .find(|data_type| data_type.name.canonical == name.canonical)?;
            match &data_type.spec {
                DataTypeSpec::Enum { .. } => Some(data_type.name.clone()),
                nested => enum_type_name_for_spec(project, nested),
            }
        }
        _ => None,
    }
}

pub(crate) fn enum_case_label_ordinal(
    project: &Project,
    expected_type: &Identifier,
    expr: &Expr,
) -> Option<i128> {
    let expected_root = enum_type_root(project, expected_type)?;
    match expr {
        Expr::Literal(Literal::Typed { type_name, value }) => (enum_type_root(project, type_name)?
            == expected_root)
            .then(|| enum_ordinal_in_root(project, &expected_root, &canonical_identifier(value)))
            .flatten(),
        Expr::Variable(variable)
            if variable.direct.is_none()
                && variable.path.len() == 1
                && variable.indices.iter().all(Vec::is_empty) =>
        {
            enum_ordinal_in_root(project, &expected_root, &variable.root_name()?.canonical)
        }
        _ => None,
    }
}

pub(crate) fn enum_ordinal_in_root(
    project: &Project,
    root_type: &str,
    value_name: &str,
) -> Option<i128> {
    project.data_types().find_map(|data_type| {
        if data_type.name.canonical != root_type {
            return None;
        }
        let DataTypeSpec::Enum { values } = &data_type.spec else {
            return None;
        };
        values
            .iter()
            .position(|value| value.canonical == value_name)
            .map(|index| index as i128)
    })
}

pub(crate) fn enum_selection_data_args<'a>(
    name: &Identifier,
    args: &'a [ParamAssignment],
) -> Option<Vec<&'a Expr>> {
    match name.canonical.as_str() {
        "SEL" => {
            let formal = ["IN0", "IN1"]
                .into_iter()
                .filter_map(|param| {
                    args.iter()
                        .find(|arg| {
                            !arg.output
                                && arg
                                    .name
                                    .as_ref()
                                    .is_some_and(|name| name.canonical == param)
                        })
                        .and_then(|arg| arg.expr.as_ref())
                })
                .collect::<Vec<_>>();
            if !formal.is_empty() {
                return Some(formal);
            }
            Some(positional_input_exprs(args).into_iter().skip(1).collect())
        }
        "MUX" => {
            let mut formal = args
                .iter()
                .filter_map(|arg| {
                    let name = arg.name.as_ref()?;
                    let suffix = name.canonical.strip_prefix("IN")?;
                    let index = suffix.parse::<usize>().ok()?;
                    (!arg.output).then_some((index, arg.expr.as_ref()?))
                })
                .collect::<Vec<_>>();
            if !formal.is_empty() {
                formal.sort_by_key(|(index, _)| *index);
                return Some(formal.into_iter().map(|(_, expr)| expr).collect());
            }
            Some(positional_input_exprs(args).into_iter().skip(1).collect())
        }
        _ => None,
    }
}

pub(crate) fn positional_input_exprs(args: &[ParamAssignment]) -> Vec<&Expr> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter(|arg| arg.name.is_none())
        .filter_map(|arg| arg.expr.as_ref())
        .collect()
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
    valid_underscore_placement(raw)
}

pub(crate) fn valid_underscore_placement(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    !bytes.windows(2).any(|pair| pair == b"__")
}

pub(crate) fn real_literal_f64(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    let unsigned = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('+'))
        .unwrap_or(trimmed);
    if !valid_underscore_placement(unsigned) {
        return None;
    }
    let value = trimmed.replace('_', "").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
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
    if !valid_underscore_placement(raw) {
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

pub(crate) fn function_block_pou<'a>(project: &'a Project, spec: &DataTypeSpec) -> Option<&'a Pou> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    project
        .find_pou(&type_name.original)
        .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
}

pub(crate) fn il_label_operand(expr: &Expr) -> Option<&Identifier> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some()
        || variable.path.len() != 1
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    variable.root_name()
}

pub(crate) fn enum_value_exists(project: &Project, canonical_name: &str) -> bool {
    project.data_types().any(|data_type| {
        if let DataTypeSpec::Enum { values } = &data_type.spec {
            values.iter().any(|value| value.canonical == canonical_name)
        } else {
            false
        }
    })
}

pub(crate) fn standard_fb_field_type(spec: &DataTypeSpec, field: &str) -> Option<DataTypeSpec> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    let spec = match type_name.canonical.as_str() {
        "SR" | "RS" if field == "Q1" => DataTypeSpec::Elementary(ElementaryType::Bool),
        "R_TRIG" | "F_TRIG" if matches!(field, "Q" | "M") => {
            DataTypeSpec::Elementary(ElementaryType::Bool)
        }
        "CTU" | "CTD" => match field {
            "Q" | "_CU" | "_CD" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "CV" => DataTypeSpec::Elementary(ElementaryType::Int),
            _ => return None,
        },
        "CTUD" => match field {
            "QU" | "QD" | "_CU" | "_CD" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "CV" => DataTypeSpec::Elementary(ElementaryType::Int),
            _ => return None,
        },
        "TON" | "TOF" | "TP" => match field {
            "Q" | "_IN" | "_RUN" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "ET" => DataTypeSpec::Elementary(ElementaryType::Time),
            _ => return None,
        },
        name if is_communication_function_block(name) => match field {
            "DONE" | "NDR" | "ERROR" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "STATUS" => DataTypeSpec::Elementary(ElementaryType::Int),
            _ => return None,
        },
        _ => return None,
    };
    Some(spec)
}

pub(crate) fn standard_function_block_inputs(name: &str) -> Vec<(&'static str, DataTypeSpec)> {
    let bool_spec = || DataTypeSpec::Elementary(ElementaryType::Bool);
    let int_spec = || DataTypeSpec::Elementary(ElementaryType::Int);
    let time_spec = || DataTypeSpec::Elementary(ElementaryType::Time);
    match canonical_identifier(name).as_str() {
        "SR" => vec![("S1", bool_spec()), ("R", bool_spec())],
        "RS" => vec![("S", bool_spec()), ("R1", bool_spec())],
        "R_TRIG" | "F_TRIG" => vec![("CLK", bool_spec())],
        "CTU" => vec![("CU", bool_spec()), ("R", bool_spec()), ("PV", int_spec())],
        "CTD" => vec![("CD", bool_spec()), ("LD", bool_spec()), ("PV", int_spec())],
        "CTUD" => vec![
            ("CU", bool_spec()),
            ("CD", bool_spec()),
            ("R", bool_spec()),
            ("LD", bool_spec()),
            ("PV", int_spec()),
        ],
        "TON" | "TOF" | "TP" => vec![("IN", bool_spec()), ("PT", time_spec())],
        name if is_communication_function_block(name) => vec![
            ("REQ", bool_spec()),
            ("EN_R", bool_spec()),
            ("ID", int_spec()),
            ("LEN", int_spec()),
        ],
        _ => Vec::new(),
    }
}

pub(crate) fn standard_function_block_outputs(name: &str) -> Vec<(&'static str, DataTypeSpec)> {
    let bool_spec = || DataTypeSpec::Elementary(ElementaryType::Bool);
    let int_spec = || DataTypeSpec::Elementary(ElementaryType::Int);
    let time_spec = || DataTypeSpec::Elementary(ElementaryType::Time);
    match canonical_identifier(name).as_str() {
        "SR" | "RS" => vec![("Q1", bool_spec())],
        "R_TRIG" | "F_TRIG" => vec![("Q", bool_spec())],
        "CTU" | "CTD" => vec![("Q", bool_spec()), ("CV", int_spec())],
        "CTUD" => vec![("QU", bool_spec()), ("QD", bool_spec()), ("CV", int_spec())],
        "TON" | "TOF" | "TP" => vec![("Q", bool_spec()), ("ET", time_spec())],
        name if is_communication_function_block(name) => vec![
            ("DONE", bool_spec()),
            ("NDR", bool_spec()),
            ("ERROR", bool_spec()),
            ("STATUS", int_spec()),
        ],
        _ => Vec::new(),
    }
}

pub(crate) fn expr_depth(expr: &Expr) -> usize {
    match expr {
        Expr::Literal(_) | Expr::Variable(_) => 1,
        Expr::Unary { expr, .. } => 1 + expr_depth(expr),
        Expr::Binary { left, right, .. } => 1 + expr_depth(left).max(expr_depth(right)),
        Expr::Call { args, .. } => {
            1 + args
                .iter()
                .filter_map(|arg| arg.expr.as_ref())
                .map(expr_depth)
                .max()
                .unwrap_or(0)
        }
        Expr::ArrayLiteral(elements) => 1 + elements.iter().map(expr_depth).max().unwrap_or(0),
        Expr::StructLiteral(fields) => {
            1 + fields
                .iter()
                .filter_map(|field| field.expr.as_ref())
                .map(expr_depth)
                .max()
                .unwrap_or(0)
        }
    }
}

pub(crate) fn const_i64(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<i64> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(*value),
        Expr::Literal(Literal::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Expr::Literal(Literal::Typed { value, .. }) => {
            typed_literal_i128(value).and_then(|value| i64::try_from(value).ok())
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => const_i64(expr, variables, project, checker).and_then(i64::checked_neg),
        Expr::Binary { op, left, right } => {
            let left = const_i64(left, variables, project, checker)?;
            let right = const_i64(right, variables, project, checker)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right != 0 => left.checked_div(right),
                BinaryOp::Mod if right != 0 => left.checked_rem(right),
                _ => None,
            }
        }
        Expr::Call { .. } => {
            const_standard_value(expr, variables, project, checker).and_then(|value| value.as_i64())
        }
        _ => {
            let _ = checker.type_of_expr(expr, variables, project);
            None
        }
    }
}

pub(crate) fn const_integer_i128(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(i128::from(*value)),
        Expr::Literal(Literal::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Expr::Literal(Literal::Typed { value, .. }) => typed_literal_i128(value),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => const_integer_i128(expr, variables, project, checker).and_then(i128::checked_neg),
        Expr::Binary { op, left, right } => {
            let left = const_integer_i128(left, variables, project, checker)?;
            let right = const_integer_i128(right, variables, project, checker)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right != 0 => left.checked_div(right),
                BinaryOp::Mod if right != 0 => left.checked_rem(right),
                _ => None,
            }
        }
        _ => const_i64(expr, variables, project, checker).map(i128::from),
    }
}

pub(crate) fn const_string_expr(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<String> {
    match const_standard_value(expr, variables, project, checker)? {
        Value::String(value) | Value::WString(value) => Some(value),
        _ => None,
    }
}

pub(crate) fn const_standard_value(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<Value> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(Value::Int(*value)),
        Expr::Literal(Literal::Real(value)) if value.is_finite() => Some(Value::Real(*value)),
        Expr::Literal(Literal::Bool(value)) => Some(Value::Bool(*value)),
        Expr::Literal(Literal::String(value)) => Some(Value::String(value.clone())),
        Expr::Literal(Literal::WString(value)) => Some(Value::WString(value.clone())),
        Expr::Literal(Literal::DurationMs(value)) => Some(Value::TimeMs(*value)),
        Expr::Literal(Literal::Typed { type_name, value }) => {
            typed_literal_const_value(project, type_name, value)
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => match const_standard_value(expr, variables, project, checker)? {
            Value::Int(value) => value.checked_neg().map(Value::Int),
            Value::Real(value) => Some(Value::Real(-value)),
            Value::TimeMs(value) => value.checked_neg().map(Value::TimeMs),
            _ => None,
        },
        Expr::Binary { op, left, right } => {
            let left = const_i64(left, variables, project, checker)?;
            let right = const_i64(right, variables, project, checker)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right != 0 => left.checked_div(right),
                BinaryOp::Mod if right != 0 => left.checked_rem(right),
                _ => None,
            }
            .map(Value::Int)
        }
        Expr::Call { name, args } => {
            let values = ordered_standard_function_input_exprs(name, args)
                .into_iter()
                .map(|expr| const_standard_value(expr, variables, project, checker))
                .collect::<Option<Vec<_>>>()?;
            iec_stdlib::eval_standard_function(&name.original, &values)
        }
        _ => {
            let _ = checker.type_of_expr(expr, variables, project);
            None
        }
    }
}

pub(crate) fn typed_literal_const_value(
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
    match spec {
        DataTypeSpec::Elementary(elementary) => {
            typed_literal_elementary_value(elementary.clone(), value)
        }
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
            if *wide {
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
        ElementaryType::Real | ElementaryType::Lreal => real_literal_f64(value).map(Value::Real),
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

pub(crate) fn const_conversion_i128(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(*value as i128),
        Expr::Literal(Literal::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Expr::Literal(Literal::Real(value)) if value.is_finite() => Some(*value as i128),
        Expr::Literal(Literal::String(value) | Literal::WString(value)) => {
            value.trim().parse::<i128>().ok()
        }
        Expr::Literal(Literal::DurationMs(value)) => Some(*value),
        Expr::Literal(Literal::Typed { type_name, value }) => {
            typed_literal_const_value(project, type_name, value).and_then(|value| match value {
                Value::Bool(value) => Some(if value { 1 } else { 0 }),
                Value::Int(value) => Some(i128::from(value)),
                Value::Real(value) if value.is_finite() => Some(value as i128),
                Value::Real(_) => None,
                Value::String(value) | Value::WString(value) => value.trim().parse::<i128>().ok(),
                Value::TimeMs(value) => Some(value),
                Value::Array(_) | Value::Struct(_) | Value::Unit => None,
            })
        }
        _ => const_i64(expr, variables, project, checker).map(i128::from),
    }
}

pub(crate) fn retain_kind_label(kind: RetainKind) -> &'static str {
    match kind {
        RetainKind::Retain => "RETAIN",
        RetainKind::NonRetain => "NON_RETAIN",
    }
}

pub(crate) fn edge_qualifier_label(kind: EdgeQualifier) -> &'static str {
    match kind {
        EdgeQualifier::Rising => "R_EDGE",
        EdgeQualifier::Falling => "F_EDGE",
    }
}

pub(crate) fn var_block_kind_label(kind: VarBlockKind) -> &'static str {
    match kind {
        VarBlockKind::Local => "VAR",
        VarBlockKind::Input => "VAR_INPUT",
        VarBlockKind::Output => "VAR_OUTPUT",
        VarBlockKind::InOut => "VAR_IN_OUT",
        VarBlockKind::External => "VAR_EXTERNAL",
        VarBlockKind::Global => "VAR_GLOBAL",
        VarBlockKind::Temp => "VAR_TEMP",
        VarBlockKind::Access => "VAR_ACCESS",
        VarBlockKind::Config => "VAR_CONFIG",
    }
}

pub(crate) fn collect_function_calls_in_statements(
    statements: &[Statement],
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    for statement in statements {
        collect_function_calls_in_statement(statement, project, calls);
    }
}

pub(crate) fn collect_function_calls_in_statement(
    statement: &Statement,
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    match statement {
        Statement::Assignment { value, .. } => {
            collect_function_calls_in_expr(value, project, calls)
        }
        Statement::FbCall { args, .. } => collect_function_calls_in_args(args, project, calls),
        Statement::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                collect_function_calls_in_expr(condition, project, calls);
                collect_function_calls_in_statements(body, project, calls);
            }
            collect_function_calls_in_statements(else_branch, project, calls);
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => {
            collect_function_calls_in_expr(selector, project, calls);
            for (labels, body) in cases {
                for label in labels {
                    match label {
                        CaseLabel::Single(expr) => {
                            collect_function_calls_in_expr(expr, project, calls)
                        }
                        CaseLabel::Range(low, high) => {
                            collect_function_calls_in_expr(low, project, calls);
                            collect_function_calls_in_expr(high, project, calls);
                        }
                    }
                }
                collect_function_calls_in_statements(body, project, calls);
            }
            collect_function_calls_in_statements(else_branch, project, calls);
        }
        Statement::For {
            from, to, by, body, ..
        } => {
            collect_function_calls_in_expr(from, project, calls);
            collect_function_calls_in_expr(to, project, calls);
            if let Some(by) = by {
                collect_function_calls_in_expr(by, project, calls);
            }
            collect_function_calls_in_statements(body, project, calls);
        }
        Statement::While { condition, body } => {
            collect_function_calls_in_expr(condition, project, calls);
            collect_function_calls_in_statements(body, project, calls);
        }
        Statement::Repeat { body, until } => {
            collect_function_calls_in_statements(body, project, calls);
            collect_function_calls_in_expr(until, project, calls);
        }
        Statement::Il { operand, .. } => {
            if let Some(operand) = operand {
                collect_function_calls_in_expr(operand, project, calls);
            }
        }
        Statement::Empty
        | Statement::IlLabel(_)
        | Statement::Exit
        | Statement::Return
        | Statement::Unsupported(_) => {}
    }
}

pub(crate) fn collect_function_calls_in_args(
    args: &[ParamAssignment],
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    for arg in args {
        if let Some(expr) = &arg.expr {
            collect_function_calls_in_expr(expr, project, calls);
        }
    }
}

pub(crate) fn collect_function_calls_in_expr(
    expr: &Expr,
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    match expr {
        Expr::Call { name, args } => {
            if project
                .find_pou(&name.original)
                .is_some_and(|pou| matches!(&pou.kind, PouKind::Function { .. }))
            {
                calls.insert(name.canonical.clone());
            }
            collect_function_calls_in_args(args, project, calls);
        }
        Expr::Unary { expr, .. } => collect_function_calls_in_expr(expr, project, calls),
        Expr::Binary { left, right, .. } => {
            collect_function_calls_in_expr(left, project, calls);
            collect_function_calls_in_expr(right, project, calls);
        }
        Expr::ArrayLiteral(elements) => {
            for element in elements {
                collect_function_calls_in_expr(element, project, calls);
            }
        }
        Expr::StructLiteral(fields) => {
            for field in fields {
                if let Some(expr) = &field.expr {
                    collect_function_calls_in_expr(expr, project, calls);
                }
            }
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

pub(crate) fn function_reaches_itself(
    start: &str,
    current: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    path: &mut Vec<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if !visited.insert(current.to_string()) {
        return false;
    }
    path.push(current.to_string());
    let result = graph.get(current).is_some_and(|calls| {
        calls
            .iter()
            .any(|next| next == start || function_reaches_itself(start, next, graph, path, visited))
    });
    path.pop();
    result
}

pub(crate) fn access_path_parts(path: &str) -> Option<Vec<String>> {
    let parts = path
        .split('.')
        .map(str::trim)
        .map(|part| {
            let mut chars = part.chars();
            let first = chars.next()?;
            if !(first.is_ascii_alphabetic() || first == '_') {
                return None;
            }
            chars
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                .then(|| canonical_identifier(part))
        })
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

pub(crate) fn output_binding_access_target(variable: &VariableRef) -> Option<String> {
    if variable.indices.iter().any(|indices| !indices.is_empty()) {
        return None;
    }
    variable
        .direct
        .clone()
        .or_else(|| (!variable.path.is_empty()).then(|| variable.to_string()))
}

pub(crate) fn resolve_access_path_from_spec(
    spec: &DataTypeSpec,
    parts: &[String],
    project: &Project,
) -> Option<DataTypeSpec> {
    resolve_access_path_from_spec_inner(spec, parts, project, 0)
}

pub(crate) fn resolve_access_path_from_spec_inner(
    spec: &DataTypeSpec,
    parts: &[String],
    project: &Project,
    depth: usize,
) -> Option<DataTypeSpec> {
    if depth > 32 {
        return None;
    }
    if parts.is_empty() {
        return Some(spec.clone());
    }

    if let Some(field_spec) = standard_fb_field_type(spec, &parts[0]) {
        return resolve_access_path_from_spec_inner(&field_spec, &parts[1..], project, depth + 1);
    }

    if let Some(function_block) = function_block_pou(project, spec) {
        if let Some(field) = function_block
            .variable_declarations()
            .find(|field| field.name.canonical == parts[0])
        {
            return resolve_access_path_from_spec_inner(
                &field.type_spec,
                &parts[1..],
                project,
                depth + 1,
            );
        }
    }

    let resolved = match spec {
        DataTypeSpec::Named(name) => project
            .data_types()
            .find(|data_type| data_type.name.canonical == name.canonical)
            .map(|data_type| data_type.spec.clone())?,
        other => other.clone(),
    };

    match resolved {
        DataTypeSpec::Struct { fields } => {
            let field = fields
                .iter()
                .find(|field| field.name.canonical == parts[0])?;
            resolve_access_path_from_spec_inner(&field.spec, &parts[1..], project, depth + 1)
        }
        DataTypeSpec::Named(_) if &resolved != spec => {
            resolve_access_path_from_spec_inner(&resolved, parts, project, depth + 1)
        }
        _ => None,
    }
}

pub(crate) fn resolve_configuration_access_target(
    configuration: &Configuration,
    resource: Option<&Resource>,
    path: &str,
    project: &Project,
) -> Option<DataTypeSpec> {
    let parts = access_path_parts(path)?;

    if let Some(resource) = resource {
        if parts.first() == Some(&resource.name.canonical) {
            return resolve_resource_access_target(resource, &parts[1..], project);
        }
        if let Some(spec) = resolve_resource_access_target(resource, &parts, project) {
            return Some(spec);
        }
    }

    if let Some(spec) = variable_spec_in_blocks(&configuration.var_blocks, &parts[0])
        .and_then(|spec| resolve_access_path_from_spec(&spec, &parts[1..], project))
    {
        return Some(spec);
    }

    let resource = configuration
        .resources
        .iter()
        .find(|resource| resource.name.canonical == parts[0])?;
    resolve_resource_access_target(resource, &parts[1..], project)
}

pub(crate) fn resolve_resource_access_target(
    resource: &Resource,
    parts: &[String],
    project: &Project,
) -> Option<DataTypeSpec> {
    let root = parts.first()?;

    if let Some(spec) = variable_spec_in_blocks(&resource.var_blocks, root)
        .and_then(|spec| resolve_access_path_from_spec(&spec, &parts[1..], project))
    {
        return Some(spec);
    }

    let instance = resource
        .program_instances
        .iter()
        .find(|instance| instance.name.canonical == *root)?;
    let field = parts.get(1)?;
    let program = project
        .find_pou(&instance.program_type.original)
        .filter(|pou| matches!(&pou.kind, PouKind::Program))?;
    let spec = program
        .variable_declarations()
        .find(|var| var.name.canonical == *field)
        .map(|var| var.type_spec.clone())?;
    resolve_access_path_from_spec(&spec, &parts[2..], project)
}

pub(crate) fn variable_spec_in_blocks(blocks: &[VarBlock], name: &str) -> Option<DataTypeSpec> {
    blocks
        .iter()
        .filter(|block| block.kind != VarBlockKind::Access)
        .flat_map(|block| block.vars.iter())
        .find(|var| var.name.canonical == name)
        .map(|var| var.type_spec.clone())
}

pub(crate) fn program_variable_with_kind<'a>(
    program: &'a Pou,
    name: &Identifier,
) -> Option<(&'a VarDecl, VarBlockKind)> {
    program.var_blocks.iter().find_map(|block| {
        block
            .vars
            .iter()
            .find(|var| var.name.canonical == name.canonical)
            .map(|var| (var, block.kind))
    })
}

pub(crate) fn validate_direct_variable_location(
    location: &str,
    allow_incomplete: bool,
) -> Option<String> {
    if !location.starts_with('%') {
        return Some(format!(
            "direct variable location '{location}' must start with '%'"
        ));
    }

    let mut chars = location[1..].chars().peekable();
    let Some(area) = chars.next() else {
        return Some("direct variable location '%' is missing an area".to_string());
    };
    if !matches!(area.to_ascii_uppercase(), 'I' | 'Q' | 'M') {
        return Some(format!(
            "direct variable location '{location}' has invalid area '{area}'"
        ));
    }

    if chars
        .peek()
        .is_some_and(|ch| matches!(ch.to_ascii_uppercase(), 'X' | 'B' | 'W' | 'D' | 'L'))
    {
        chars.next();
    }

    let address = chars.collect::<String>();
    if address == "*" {
        return if allow_incomplete {
            None
        } else {
            Some(format!(
                "incomplete direct variable location '{location}' is only valid in a declaration"
            ))
        };
    }

    if address.is_empty() {
        return Some(format!(
            "direct variable location '{location}' is missing an address"
        ));
    }

    if address.contains('*') {
        return Some(format!(
            "direct variable location '{location}' has invalid address '{address}'"
        ));
    }

    if address.starts_with('.') || address.ends_with('.') || address.contains("..") {
        return Some(format!(
            "direct variable location '{location}' has malformed address '{address}'"
        ));
    }

    if !address.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return Some(format!(
            "direct variable location '{location}' has invalid address '{address}'"
        ));
    }

    None
}

pub(crate) fn array_element_count(ranges: &[Subrange]) -> usize {
    ranges.iter().fold(1_usize, |total, range| {
        total.saturating_mul((range.high - range.low + 1).max(0) as usize)
    })
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

pub(crate) fn uses_formal_split_outputs(args: &[ParamAssignment]) -> bool {
    args.iter()
        .any(|arg| arg.output && !arg.name.as_ref().is_some_and(is_implicit_eno))
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

pub(crate) fn statements_definitely_assign(statements: &[Statement], canonical_name: &str) -> bool {
    for statement in statements {
        if statement_definitely_assigns(statement, canonical_name) {
            return true;
        }
        if matches!(statement, Statement::Return | Statement::Exit) {
            return false;
        }
    }
    false
}

pub(crate) fn statement_definitely_assigns(statement: &Statement, canonical_name: &str) -> bool {
    match statement {
        Statement::Assignment { target, .. } => target
            .root_name()
            .is_some_and(|name| name.canonical == canonical_name),
        Statement::If {
            branches,
            else_branch,
        } => {
            !else_branch.is_empty()
                && branches
                    .iter()
                    .all(|(_, body)| statements_definitely_assign(body, canonical_name))
                && statements_definitely_assign(else_branch, canonical_name)
        }
        Statement::Case {
            cases, else_branch, ..
        } => {
            !else_branch.is_empty()
                && !cases.is_empty()
                && cases
                    .iter()
                    .all(|(_, body)| statements_definitely_assign(body, canonical_name))
                && statements_definitely_assign(else_branch, canonical_name)
        }
        Statement::Repeat { body, .. } => statements_definitely_assign(body, canonical_name),
        _ => false,
    }
}

pub(crate) fn count_project_variables(project: &Project) -> usize {
    let pou_variables = project
        .pous()
        .map(|pou| {
            pou.var_blocks
                .iter()
                .map(|block| block.vars.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    let configuration_variables = project
        .library_elements
        .iter()
        .filter_map(|element| {
            if let LibraryElement::Configuration(configuration) = element {
                Some(configuration)
            } else {
                None
            }
        })
        .map(|configuration| {
            let configuration_vars = configuration
                .var_blocks
                .iter()
                .map(|block| block.vars.len())
                .sum::<usize>();
            let resource_vars = configuration
                .resources
                .iter()
                .flat_map(|resource| resource.var_blocks.iter())
                .map(|block| block.vars.len())
                .sum::<usize>();
            configuration_vars + resource_vars
        })
        .sum::<usize>();
    pou_variables + configuration_variables
}

pub(crate) fn count_project_symbols(project: &Project) -> usize {
    let library_symbols = project.library_elements.len();
    let type_field_symbols = project
        .data_types()
        .map(|data_type| count_type_symbols(&data_type.spec))
        .sum::<usize>();
    let configuration_symbols = project
        .library_elements
        .iter()
        .filter_map(|element| {
            if let LibraryElement::Configuration(configuration) = element {
                Some(configuration)
            } else {
                None
            }
        })
        .map(|configuration| {
            1 + configuration
                .resources
                .iter()
                .map(|resource| 1 + resource.tasks.len() + resource.program_instances.len())
                .sum::<usize>()
        })
        .sum::<usize>();
    library_symbols + type_field_symbols + configuration_symbols + count_project_variables(project)
}

pub(crate) fn count_type_symbols(spec: &DataTypeSpec) -> usize {
    match spec {
        DataTypeSpec::Array { element_type, .. } => count_type_symbols(element_type),
        DataTypeSpec::Struct { fields } => {
            fields.len()
                + fields
                    .iter()
                    .map(|field| count_type_symbols(&field.spec))
                    .sum::<usize>()
        }
        DataTypeSpec::Elementary(_)
        | DataTypeSpec::Named(_)
        | DataTypeSpec::Enum { .. }
        | DataTypeSpec::Subrange { .. }
        | DataTypeSpec::String { .. } => 0,
    }
}
