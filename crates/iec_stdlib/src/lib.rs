// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_ir::{canonical_identifier, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StandardSymbolKind {
    Function,
    FunctionBlock,
}

#[derive(Debug, Clone, Copy)]
pub struct StandardSymbol {
    pub name: &'static str,
    pub kind: StandardSymbolKind,
    pub clause: &'static str,
}

pub fn standard_symbols() -> &'static [StandardSymbol] {
    &[
        StandardSymbol {
            name: "ABS",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "SQRT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "LN",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "LOG",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "EXP",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "SIN",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "COS",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "TAN",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "ADD",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "SUB",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "MUL",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "DIV",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "MOD",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "EXPT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.2",
        },
        StandardSymbol {
            name: "MOVE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.4",
        },
        StandardSymbol {
            name: "MIN",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.4",
        },
        StandardSymbol {
            name: "MAX",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.4",
        },
        StandardSymbol {
            name: "LIMIT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.4",
        },
        StandardSymbol {
            name: "SEL",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.4",
        },
        StandardSymbol {
            name: "MUX",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.4",
        },
        StandardSymbol {
            name: "GT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.5",
        },
        StandardSymbol {
            name: "GE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.5",
        },
        StandardSymbol {
            name: "EQ",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.5",
        },
        StandardSymbol {
            name: "NE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.5",
        },
        StandardSymbol {
            name: "LE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.5",
        },
        StandardSymbol {
            name: "LT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.5",
        },
        StandardSymbol {
            name: "SHL",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "SHR",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "ROL",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "ROR",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "AND",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "OR",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "XOR",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "NOT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.6",
        },
        StandardSymbol {
            name: "LEN",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "LEFT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "RIGHT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "MID",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "CONCAT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "INSERT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "DELETE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "REPLACE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "FIND",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.7",
        },
        StandardSymbol {
            name: "ADD_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "ADD_TOD_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "ADD_DT_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "CONCAT_DATE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "CONCAT_TOD",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "CONCAT_DT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "CONCAT_DATE_TOD",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "DAY_OF_WEEK",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SPLIT_DATE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SPLIT_TOD",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SPLIT_DT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SUB_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SUB_DATE_DATE",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SUB_TOD_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SUB_TOD_TOD",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SUB_DT_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "SUB_DT_DT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "MUL_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "MULTIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "DIV_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "DIVTIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "BOOL_TO_INT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.1",
        },
        StandardSymbol {
            name: "TRUNC",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.1",
        },
        StandardSymbol {
            name: "INT_TO_BOOL",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.1",
        },
        StandardSymbol {
            name: "INT_TO_REAL",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.1",
        },
        StandardSymbol {
            name: "REAL_TO_INT",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.1",
        },
        StandardSymbol {
            name: "SR",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.1",
        },
        StandardSymbol {
            name: "RS",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.1",
        },
        StandardSymbol {
            name: "R_TRIG",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.2",
        },
        StandardSymbol {
            name: "F_TRIG",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.2",
        },
        StandardSymbol {
            name: "CTU",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.3",
        },
        StandardSymbol {
            name: "CTD",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.3",
        },
        StandardSymbol {
            name: "CTUD",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.3",
        },
        StandardSymbol {
            name: "TP",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.4",
        },
        StandardSymbol {
            name: "TON",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.4",
        },
        StandardSymbol {
            name: "TOF",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.4",
        },
        StandardSymbol {
            name: "USEND",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.5",
        },
        StandardSymbol {
            name: "URCV",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.5",
        },
        StandardSymbol {
            name: "BSEND",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.5",
        },
        StandardSymbol {
            name: "BRCV",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.5",
        },
        StandardSymbol {
            name: "SEND",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.5",
        },
        StandardSymbol {
            name: "RCV",
            kind: StandardSymbolKind::FunctionBlock,
            clause: "2.5.2.3.5",
        },
    ]
}

pub fn is_standard_function(name: &str) -> bool {
    let name = canonical_identifier(name);
    if is_standard_void_function(&name) {
        return false;
    }
    if is_bcd_conversion_function_name(&name) {
        return true;
    }
    if is_conversion_function_name(&name) {
        return true;
    }
    standard_symbols()
        .iter()
        .any(|symbol| symbol.kind == StandardSymbolKind::Function && symbol.name == name)
}

pub fn is_standard_void_function(name: &str) -> bool {
    matches!(
        canonical_identifier(name).as_str(),
        "SPLIT_DATE" | "SPLIT_TOD" | "SPLIT_DT"
    )
}

pub fn is_standard_function_block(name: &str) -> bool {
    let name = canonical_identifier(name);
    standard_symbols()
        .iter()
        .any(|symbol| symbol.kind == StandardSymbolKind::FunctionBlock && symbol.name == name)
}

pub fn standard_function_input_index(function_name: &str, argument_name: &str) -> Option<usize> {
    let function_name = canonical_identifier(function_name);
    let argument_name = canonical_identifier(argument_name);

    match function_name.as_str() {
        "SEL" => return named_index(&argument_name, &[("G", 0), ("IN0", 1), ("IN1", 2)]),
        "MUX" => {
            return if argument_name == "K" {
                Some(0)
            } else {
                input_n_index(&argument_name).map(|index| index + 1)
            };
        }
        _ => {}
    }

    if let Some(index) = extensible_input_index(&argument_name) {
        if let Some(mapped) = match function_name.as_str() {
            "ADD" | "MUL" | "MIN" | "MAX" | "GT" | "GE" | "EQ" | "LE" | "LT" | "AND" | "OR"
            | "XOR" | "CONCAT" => Some(index),
            "SUB" | "DIV" | "MOD" | "EXPT" | "NE" | "ADD_TIME" | "SUB_TIME" | "ADD_TOD_TIME"
            | "SUB_TOD_TIME" | "ADD_DT_TIME" | "SUB_DT_TIME" | "SUB_DATE_DATE" | "SUB_TOD_TOD"
            | "SUB_DT_DT" | "MUL_TIME" | "MULTIME" | "DIV_TIME" | "DIVTIME" | "FIND"
            | "CONCAT_DATE_TOD"
                if index < 2 =>
            {
                Some(index)
            }
            _ => None,
        } {
            return Some(mapped);
        }
    }

    match function_name.as_str() {
        "ABS" | "SQRT" | "LN" | "LOG" | "EXP" | "SIN" | "COS" | "TAN" | "TRUNC" | "MOVE"
        | "NOT" | "LEN" | "DAY_OF_WEEK" => named_index(&argument_name, &[("IN", 0)]),
        "SHL" | "SHR" | "ROL" | "ROR" => named_index(&argument_name, &[("IN", 0), ("N", 1)]),
        "LIMIT" => named_index(&argument_name, &[("MN", 0), ("IN", 1), ("MX", 2)]),
        "LEFT" | "RIGHT" => named_index(&argument_name, &[("IN", 0), ("L", 1)]),
        "MID" => named_index(&argument_name, &[("IN", 0), ("L", 1), ("P", 2)]),
        "INSERT" => named_index(&argument_name, &[("IN1", 0), ("IN2", 1), ("P", 2)]),
        "DELETE" => named_index(&argument_name, &[("IN", 0), ("L", 1), ("P", 2)]),
        "REPLACE" => named_index(
            &argument_name,
            &[("IN1", 0), ("IN2", 1), ("L", 2), ("P", 3)],
        ),
        "CONCAT_DATE" => named_index(
            &argument_name,
            &[("YEAR", 0), ("MONTH", 1), ("DAY", 2), ("DATE", 2)],
        ),
        "CONCAT_TOD" => named_index(
            &argument_name,
            &[
                ("HOUR", 0),
                ("MINUTE", 1),
                ("SECOND", 2),
                ("MILLISECOND", 3),
            ],
        ),
        "CONCAT_DT" => named_index(
            &argument_name,
            &[
                ("YEAR", 0),
                ("MONTH", 1),
                ("DAY", 2),
                ("DATE", 2),
                ("HOUR", 3),
                ("MINUTE", 4),
                ("SECOND", 5),
                ("MILLISECOND", 6),
            ],
        ),
        "CONCAT_DATE_TOD" => named_index(
            &argument_name,
            &[("DATE", 0), ("TOD", 1), ("TIME_OF_DAY", 1)],
        ),
        name if is_conversion_function_name(name) || is_bcd_conversion_function_name(name) => {
            named_index(&argument_name, &[("IN", 0)])
        }
        _ => None,
    }
}

fn named_index(argument_name: &str, names: &[(&str, usize)]) -> Option<usize> {
    names
        .iter()
        .find_map(|(name, index)| (*name == argument_name).then_some(*index))
}

fn extensible_input_index(argument_name: &str) -> Option<usize> {
    let suffix = argument_name.strip_prefix("IN")?;
    if suffix.is_empty() {
        return Some(0);
    }
    let value = suffix.parse::<usize>().ok()?;
    value.checked_sub(1)
}

fn input_n_index(argument_name: &str) -> Option<usize> {
    let suffix = argument_name.strip_prefix("IN")?;
    suffix.parse::<usize>().ok()
}

pub fn is_communication_function_block(name: &str) -> bool {
    matches!(
        canonical_identifier(name).as_str(),
        "USEND" | "URCV" | "BSEND" | "BRCV" | "SEND" | "RCV"
    )
}

pub fn eval_standard_function(name: &str, args: &[Value]) -> Option<Value> {
    let name = canonical_identifier(name);
    if let Some(value) = eval_bcd_conversion_function(&name, args) {
        return Some(value);
    }
    if let Some(value) = eval_conversion_function(&name, args) {
        return Some(value);
    }
    match name.as_str() {
        "ABS" => unary_numeric(args, |v| v.abs(), |v| v.abs()),
        "SQRT" => args.first()?.as_f64().map(|v| Value::Real(v.sqrt())),
        "LN" => args.first()?.as_f64().map(|v| Value::Real(v.ln())),
        "LOG" => args.first()?.as_f64().map(|v| Value::Real(v.log10())),
        "EXP" => args.first()?.as_f64().map(|v| Value::Real(v.exp())),
        "SIN" => args.first()?.as_f64().map(|v| Value::Real(v.sin())),
        "COS" => args.first()?.as_f64().map(|v| Value::Real(v.cos())),
        "TAN" => args.first()?.as_f64().map(|v| Value::Real(v.tan())),
        "ADD" => numeric_fold(args, |a, b| a + b, |a, b| a + b),
        "SUB" => numeric_pair(args, |a, b| a - b, |a, b| a - b),
        "MUL" => numeric_fold(args, |a, b| a * b, |a, b| a * b),
        "DIV" => {
            if args.iter().any(|value| matches!(value, Value::Real(_))) {
                let right = args.get(1)?.as_f64()?;
                if right == 0.0 {
                    None
                } else {
                    Some(Value::Real(args.first()?.as_f64()? / right))
                }
            } else {
                let right = args.get(1)?.as_i64()?;
                if right == 0 {
                    None
                } else {
                    Some(Value::Int(args.first()?.as_i64()? / right))
                }
            }
        }
        "MOD" => {
            let left = args.first()?.as_i64()?;
            let right = args.get(1)?.as_i64()?;
            if right == 0 {
                None
            } else {
                Some(Value::Int(left % right))
            }
        }
        "EXPT" => {
            let left = args.first()?.as_f64()?;
            let right = args.get(1)?.as_f64()?;
            Some(Value::Real(left.powf(right)))
        }
        "TRUNC" => args.first()?.as_f64().map(|value| Value::Int(value as i64)),
        "MOVE" => args.first().cloned(),
        "MIN" => min_max(args, false),
        "MAX" => min_max(args, true),
        "LIMIT" => limit(args),
        "SEL" => {
            let g = args.first()?.as_bool()?;
            if g {
                args.get(2).cloned()
            } else {
                args.get(1).cloned()
            }
        }
        "MUX" => {
            let index = args.first()?.as_i64()? as usize;
            args.get(index + 1).cloned()
        }
        "GT" => compare_chain(args, |ordering| ordering > 0),
        "GE" => compare_chain(args, |ordering| ordering >= 0),
        "EQ" => compare_chain(args, |ordering| ordering == 0),
        "NE" => compare_chain(args, |ordering| ordering != 0),
        "LE" => compare_chain(args, |ordering| ordering <= 0),
        "LT" => compare_chain(args, |ordering| ordering < 0),
        "SHL" => bit_shift(args, false),
        "SHR" => bit_shift(args, true),
        "ROL" => bit_rotate(args, false),
        "ROR" => bit_rotate(args, true),
        "AND" => bit_bool_fold(args, |a, b| a & b, |a, b| a && b),
        "OR" => bit_bool_fold(args, |a, b| a | b, |a, b| a || b),
        "XOR" => bit_bool_fold(args, |a, b| a ^ b, |a, b| a ^ b),
        "NOT" => bit_bool_not(args),
        "LEN" => args
            .first()
            .and_then(value_string)
            .map(|value| Value::Int(value.chars().count() as i64)),
        "LEFT" => string_left(args),
        "RIGHT" => string_right(args),
        "MID" => string_mid(args),
        "CONCAT" => string_concat(args),
        "INSERT" => string_insert(args),
        "DELETE" => string_delete(args),
        "REPLACE" => string_replace(args),
        "FIND" => string_find(args),
        "ADD_TIME" | "ADD_TOD_TIME" | "ADD_DT_TIME" => time_pair(args, |a, b| a + b),
        "CONCAT_DATE" => concat_date(args),
        "CONCAT_TOD" => concat_tod(args),
        "CONCAT_DT" => concat_dt(args),
        "CONCAT_DATE_TOD" => concat_date_tod(args),
        "DAY_OF_WEEK" => day_of_week(args),
        "SUB_TIME" | "SUB_TOD_TIME" | "SUB_DT_TIME" => time_pair(args, |a, b| a - b),
        "SUB_DATE_DATE" => date_pair(args, |a, b| (a - b) * 86_400_000),
        "SUB_TOD_TOD" | "SUB_DT_DT" => time_pair(args, |a, b| a - b),
        "MUL_TIME" | "MULTIME" => time_scale(args, |time, factor| time * factor),
        "DIV_TIME" | "DIVTIME" => {
            let divisor = args.get(1)?.as_i64()?;
            if divisor == 0 {
                None
            } else {
                time_scale(args, |time, factor| time / factor)
            }
        }
        _ => None,
    }
}

fn is_conversion_function_name(name: &str) -> bool {
    let Some((source, target)) = name.split_once("_TO_") else {
        return false;
    };
    is_conversion_type(source) && is_conversion_type(target)
}

fn is_bcd_conversion_function_name(name: &str) -> bool {
    if matches!(name, "BCD_TO_INT" | "INT_TO_BCD") {
        return true;
    }
    if let Some((source, target)) = name.split_once("_BCD_TO_") {
        return is_bcd_bit_type(source) && is_bcd_integer_type(target);
    }
    if let Some((source, target)) = name.split_once("_TO_BCD_") {
        return is_bcd_integer_type(source) && is_bcd_bit_type(target);
    }
    false
}

fn is_conversion_type(name: &str) -> bool {
    matches!(
        name,
        "BOOL"
            | "SINT"
            | "INT"
            | "DINT"
            | "LINT"
            | "USINT"
            | "UINT"
            | "UDINT"
            | "ULINT"
            | "BYTE"
            | "WORD"
            | "DWORD"
            | "LWORD"
            | "REAL"
            | "LREAL"
            | "STRING"
            | "WSTRING"
            | "TIME"
            | "DATE"
            | "TOD"
            | "TIME_OF_DAY"
            | "DT"
            | "DATE_AND_TIME"
    )
}

fn is_bcd_integer_type(name: &str) -> bool {
    matches!(
        name,
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT"
    )
}

fn is_bcd_bit_type(name: &str) -> bool {
    matches!(name, "BYTE" | "WORD" | "DWORD" | "LWORD")
}

fn bcd_digit_capacity(name: &str) -> Option<u32> {
    match name {
        "BYTE" => Some(2),
        "WORD" => Some(4),
        "DWORD" => Some(8),
        "LWORD" => Some(16),
        _ => None,
    }
}

fn eval_bcd_conversion_function(name: &str, args: &[Value]) -> Option<Value> {
    let value = args.first()?.as_i64()?;
    if name == "BCD_TO_INT" {
        return bcd_to_int(value, None).map(Value::Int);
    }
    if name == "INT_TO_BCD" {
        return int_to_bcd(value, None).map(Value::Int);
    }
    if let Some((source, _target)) = name.split_once("_BCD_TO_") {
        return bcd_to_int(value, bcd_digit_capacity(source)).map(Value::Int);
    }
    if let Some((_source, target)) = name.split_once("_TO_BCD_") {
        return int_to_bcd(value, bcd_digit_capacity(target)).map(Value::Int);
    }
    None
}

fn bcd_to_int(value: i64, digits: Option<u32>) -> Option<i64> {
    if value < 0 {
        return None;
    }
    let mut raw = value as u64;
    if let Some(digits) = digits {
        let bits = digits.saturating_mul(4);
        let mask = if bits >= 64 {
            u64::MAX
        } else {
            (1_u64 << bits) - 1
        };
        if raw & !mask != 0 {
            return None;
        }
    }
    let mut result = 0_i64;
    let mut place = 1_i64;
    while raw != 0 {
        let digit = (raw & 0x0f) as i64;
        if digit > 9 {
            return None;
        }
        result = result.checked_add(digit.checked_mul(place)?)?;
        place = place.checked_mul(10)?;
        raw >>= 4;
    }
    Some(result)
}

fn int_to_bcd(value: i64, digits: Option<u32>) -> Option<i64> {
    if value < 0 {
        return None;
    }
    let max_digits = digits.unwrap_or(16);
    let mut decimal = value;
    let mut raw = 0_u64;
    let mut used_digits = 0_u32;
    if decimal == 0 {
        return Some(0);
    }
    while decimal != 0 {
        if used_digits >= max_digits {
            return None;
        }
        let digit = (decimal % 10) as u64;
        raw |= digit << (used_digits * 4);
        decimal /= 10;
        used_digits += 1;
    }
    Some(raw as i64)
}

fn eval_conversion_function(name: &str, args: &[Value]) -> Option<Value> {
    let (source, target) = name.split_once("_TO_")?;
    let value = args.first()?;
    match target {
        "BOOL" => value_to_bool(value).map(Value::Bool),
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT" | "BYTE"
        | "WORD" | "DWORD" | "LWORD" => {
            let converted = value_to_i64(value)?;
            let (_, low, high) = conversion_target_integer_range(target)?;
            (i128::from(converted) >= low && i128::from(converted) <= high)
                .then_some(Value::Int(converted))
        }
        "REAL" | "LREAL" => value_to_f64(value).map(Value::Real),
        "STRING" => Some(Value::String(value_to_string_for_source(source, value)?)),
        "WSTRING" => Some(Value::WString(value_to_string_for_source(source, value)?)),
        "TIME" => value_to_time_ms(value).map(Value::TimeMs),
        "DATE" => value_to_date_days(value).map(Value::TimeMs),
        "TOD" | "TIME_OF_DAY" => value_to_tod_ms(value).map(Value::TimeMs),
        "DT" | "DATE_AND_TIME" => value_to_dt_ms(value).map(Value::TimeMs),
        _ => None,
    }
}

fn conversion_target_integer_range(target: &str) -> Option<(&'static str, i128, i128)> {
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

fn unary_numeric(args: &[Value], int_op: fn(i64) -> i64, real_op: fn(f64) -> f64) -> Option<Value> {
    match args.first()? {
        Value::Real(value) => Some(Value::Real(real_op(*value))),
        value => value.as_i64().map(|v| Value::Int(int_op(v))),
    }
}

fn numeric_fold(
    args: &[Value],
    int_op: fn(i64, i64) -> i64,
    real_op: fn(f64, f64) -> f64,
) -> Option<Value> {
    if args.is_empty() {
        return None;
    }
    if args.iter().any(|value| matches!(value, Value::Real(_))) {
        let mut current = args.first()?.as_f64()?;
        for value in &args[1..] {
            current = real_op(current, value.as_f64()?);
        }
        Some(Value::Real(current))
    } else {
        let mut current = args.first()?.as_i64()?;
        for value in &args[1..] {
            current = int_op(current, value.as_i64()?);
        }
        Some(Value::Int(current))
    }
}

fn numeric_pair(
    args: &[Value],
    int_op: fn(i64, i64) -> i64,
    real_op: fn(f64, f64) -> f64,
) -> Option<Value> {
    if args.iter().any(|value| matches!(value, Value::Real(_))) {
        Some(Value::Real(real_op(
            args.first()?.as_f64()?,
            args.get(1)?.as_f64()?,
        )))
    } else {
        Some(Value::Int(int_op(
            args.first()?.as_i64()?,
            args.get(1)?.as_i64()?,
        )))
    }
}

fn compare_chain(args: &[Value], predicate: fn(i8) -> bool) -> Option<Value> {
    if args.len() < 2 {
        return None;
    }
    for pair in args.windows(2) {
        if !predicate(compare_values(&pair[0], &pair[1])?) {
            return Some(Value::Bool(false));
        }
    }
    Some(Value::Bool(true))
}

fn compare_values(left: &Value, right: &Value) -> Option<i8> {
    if value_string(left).is_some() || value_string(right).is_some() {
        let left = value_to_string(left);
        let right = value_to_string(right);
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

fn bit_shift(args: &[Value], right: bool) -> Option<Value> {
    let value = args.first()?.as_i64()? as u64;
    let shift = args.get(1)?.as_i64()?;
    if shift < 0 {
        return None;
    }
    let shift = shift as u32;
    if shift >= 64 {
        return Some(Value::Int(0));
    }
    let shifted = if right {
        value >> shift
    } else {
        value << shift
    };
    Some(Value::Int(shifted as i64))
}

fn bit_rotate(args: &[Value], right: bool) -> Option<Value> {
    let value = args.first()?.as_i64()? as u64;
    let shift = args.get(1)?.as_i64()?;
    if shift < 0 {
        return None;
    }
    let shift = (shift % 64) as u32;
    let rotated = if right {
        value.rotate_right(shift)
    } else {
        value.rotate_left(shift)
    };
    Some(Value::Int(rotated as i64))
}

fn bit_bool_fold(
    args: &[Value],
    int_op: fn(i64, i64) -> i64,
    bool_op: fn(bool, bool) -> bool,
) -> Option<Value> {
    if args.is_empty() {
        return None;
    }
    if args.iter().all(|value| matches!(value, Value::Bool(_))) {
        let mut current = args.first()?.as_bool()?;
        for value in &args[1..] {
            current = bool_op(current, value.as_bool()?);
        }
        Some(Value::Bool(current))
    } else {
        let mut current = args.first()?.as_i64()?;
        for value in &args[1..] {
            current = int_op(current, value.as_i64()?);
        }
        Some(Value::Int(current))
    }
}

fn bit_bool_not(args: &[Value]) -> Option<Value> {
    match args.first()? {
        Value::Bool(value) => Some(Value::Bool(!value)),
        value => value.as_i64().map(|value| Value::Int(!value)),
    }
}

fn string_left(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = string_count_arg(args.get(1)?)?;
    if len > input.chars().count() {
        return None;
    }
    Some(string_like_result(
        args.first()?,
        input.chars().take(len).collect(),
    ))
}

fn string_right(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = string_count_arg(args.get(1)?)?;
    let chars = input.chars().collect::<Vec<_>>();
    if len > chars.len() {
        return None;
    }
    let start = chars.len().saturating_sub(len);
    Some(string_like_result(
        args.first()?,
        chars[start..].iter().collect(),
    ))
}

fn string_mid(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = string_count_arg(args.get(1)?)?;
    let pos = string_position_arg(args.get(2)?)?;
    let chars = input.chars().collect::<Vec<_>>();
    if pos == 0 || pos > chars.len() || pos - 1 + len > chars.len() {
        return None;
    }
    Some(string_like_result(
        args.first()?,
        chars.iter().skip(pos - 1).take(len).collect(),
    ))
}

fn string_concat(args: &[Value]) -> Option<Value> {
    let mut out = String::new();
    let wide = args.iter().any(|value| matches!(value, Value::WString(_)));
    for value in args {
        out.push_str(&value_string(value)?);
    }
    if wide {
        Some(Value::WString(out))
    } else {
        Some(Value::String(out))
    }
}

fn string_insert(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let insert = value_string(args.get(1)?)?;
    let pos = string_position_arg(args.get(2)?)?;
    let mut chars = input.chars().collect::<Vec<_>>();
    if pos > chars.len() {
        return None;
    }
    for (offset, ch) in insert.chars().enumerate() {
        chars.insert(pos + offset, ch);
    }
    Some(string_like_result(
        args.first()?,
        chars.into_iter().collect(),
    ))
}

fn string_delete(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = string_count_arg(args.get(1)?)?;
    let pos = string_position_arg(args.get(2)?)?;
    let chars = input.chars().collect::<Vec<_>>();
    if pos == 0 || pos > chars.len() || pos - 1 + len > chars.len() {
        return None;
    }
    Some(string_like_result(
        args.first()?,
        chars
            .into_iter()
            .enumerate()
            .filter_map(|(index, ch)| {
                if index >= pos - 1 && index < (pos - 1).saturating_add(len) {
                    None
                } else {
                    Some(ch)
                }
            })
            .collect(),
    ))
}

fn string_replace(args: &[Value]) -> Option<Value> {
    let pos = args.get(3)?.as_i64()?;
    if pos <= 0 {
        return None;
    }
    let deleted = string_delete(&[
        args.first()?.clone(),
        args.get(2)?.clone(),
        args.get(3)?.clone(),
    ])?;
    let deleted = value_string(&deleted)?;
    string_insert(&[
        string_like_result(args.first()?, deleted),
        args.get(1)?.clone(),
        Value::Int(pos - 1),
    ])
}

fn string_find(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let needle = value_string(args.get(1)?)?;
    let position = input
        .find(&needle)
        .map(|index| input[..index].chars().count() as i64 + 1)
        .unwrap_or(0);
    Some(Value::Int(position))
}

fn string_count_arg(value: &Value) -> Option<usize> {
    let count = value.as_i64()?;
    if count < 0 {
        return None;
    }
    usize::try_from(count).ok()
}

fn string_position_arg(value: &Value) -> Option<usize> {
    let position = value.as_i64()?;
    if position < 0 {
        return None;
    }
    usize::try_from(position).ok()
}

fn time_pair(args: &[Value], op: fn(i128, i128) -> i128) -> Option<Value> {
    Some(Value::TimeMs(op(
        value_time_ms(args.first()?)?,
        value_time_ms(args.get(1)?)?,
    )))
}

fn date_pair(args: &[Value], op: fn(i128, i128) -> i128) -> Option<Value> {
    let left = value_time_ms(args.first()?)?;
    let right = value_time_ms(args.get(1)?)?;
    Some(Value::TimeMs(op(left, right)))
}

fn concat_date(args: &[Value]) -> Option<Value> {
    let year = args.first()?.as_i64()?;
    let month = args.get(1)?.as_i64()?;
    let day = args.get(2)?.as_i64()?;
    valid_date(year, month, day).then_some(Value::TimeMs(days_from_civil(year, month, day) as i128))
}

fn concat_tod(args: &[Value]) -> Option<Value> {
    let hour = args.first()?.as_i64()?;
    let minute = args.get(1)?.as_i64()?;
    let second = args.get(2)?.as_i64()?;
    let millisecond = args.get(3)?.as_i64()?;
    if !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
        || !(0..=999).contains(&millisecond)
    {
        return None;
    }
    Some(Value::TimeMs(
        (((hour as i128 * 60) + minute as i128) * 60 + second as i128) * 1000 + millisecond as i128,
    ))
}

fn concat_dt(args: &[Value]) -> Option<Value> {
    let date = concat_date(args)?;
    let tod = concat_tod(args.get(3..7)?)?;
    concat_date_tod(&[date, tod])
}

fn concat_date_tod(args: &[Value]) -> Option<Value> {
    let date_days = value_time_ms(args.first()?)?;
    let tod_ms = value_time_ms(args.get(1)?)?;
    Some(Value::TimeMs(date_days * 86_400_000 + tod_ms))
}

fn day_of_week(args: &[Value]) -> Option<Value> {
    let date_days = value_time_ms(args.first()?)?;
    Some(Value::Int((((date_days + 3).rem_euclid(7)) + 1) as i64))
}

fn time_scale(args: &[Value], op: fn(i128, i128) -> i128) -> Option<Value> {
    Some(Value::TimeMs(op(
        value_time_ms(args.first()?)?,
        args.get(1)?.as_i64()? as i128,
    )))
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::WString(value) => Some(value.clone()),
        _ => None,
    }
}

fn string_like_result(template: &Value, value: String) -> Value {
    if matches!(template, Value::WString(_)) {
        Value::WString(value)
    } else {
        Value::String(value)
    }
}

fn value_time_ms(value: &Value) -> Option<i128> {
    match value {
        Value::TimeMs(value) => Some(*value),
        value => value.as_i64().map(|value| value as i128),
    }
}

fn valid_date(year: i64, month: i64, day: i64) -> bool {
    (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
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

fn value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::String(value) | Value::WString(value) => {
            match canonical_identifier(value).as_str() {
                "TRUE" | "1" => Some(true),
                "FALSE" | "0" | "" => Some(false),
                _ => None,
            }
        }
        Value::TimeMs(value) => Some(*value != 0),
        value => value.as_bool(),
    }
}

fn value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::String(value) | Value::WString(value) => value.trim().parse::<i64>().ok(),
        Value::TimeMs(value) => i64::try_from(*value).ok(),
        value => value.as_i64(),
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::String(value) | Value::WString(value) => value.trim().parse::<f64>().ok(),
        Value::TimeMs(value) => Some(*value as f64),
        value => value.as_f64(),
    }
}

fn value_to_time_ms(value: &Value) -> Option<i128> {
    match value {
        Value::TimeMs(value) => Some(*value),
        Value::String(value) | Value::WString(value) => parse_time_string_ms(value),
        value => value.as_i64().map(i128::from),
    }
}

fn value_to_date_days(value: &Value) -> Option<i128> {
    match value {
        Value::String(value) | Value::WString(value) => parse_date_string_days(value),
        _ => value_time_ms(value),
    }
}

fn value_to_tod_ms(value: &Value) -> Option<i128> {
    match value {
        Value::String(value) | Value::WString(value) => parse_tod_string_ms(value),
        _ => value_time_ms(value),
    }
}

fn value_to_dt_ms(value: &Value) -> Option<i128> {
    match value {
        Value::String(value) | Value::WString(value) => parse_dt_string_ms(value),
        _ => value_time_ms(value),
    }
}

fn value_to_string_for_source(source: &str, value: &Value) -> Option<String> {
    match source {
        "DATE" => value_to_date_days(value).map(date_days_to_string),
        "TOD" | "TIME_OF_DAY" => value_to_tod_ms(value).map(tod_ms_to_string),
        "DT" | "DATE_AND_TIME" => value_to_dt_ms(value).map(dt_ms_to_string),
        _ => Some(value_to_string(value)),
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Bool(value) => {
            if *value {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Value::Int(value) => value.to_string(),
        Value::Real(value) => value.to_string(),
        Value::String(value) | Value::WString(value) => value.clone(),
        Value::TimeMs(value) => format!("T#{value}ms"),
        Value::Array(_) | Value::Struct(_) | Value::Unit => value.to_string(),
    }
}

fn parse_date_string_days(input: &str) -> Option<i128> {
    let text = input.trim();
    let text = text
        .strip_prefix("D#")
        .or_else(|| text.strip_prefix("DATE#"))
        .unwrap_or(text);
    parse_date_days(text).map(i128::from)
}

fn parse_tod_string_ms(input: &str) -> Option<i128> {
    let text = input.trim();
    let text = text
        .strip_prefix("TOD#")
        .or_else(|| text.strip_prefix("TIME_OF_DAY#"))
        .unwrap_or(text);
    parse_time_of_day_ms(text)
}

fn parse_dt_string_ms(input: &str) -> Option<i128> {
    let text = input.trim();
    let text = text
        .strip_prefix("DT#")
        .or_else(|| text.strip_prefix("DATE_AND_TIME#"))
        .unwrap_or(text);
    parse_date_time_ms(text)
}

fn parse_time_string_ms(input: &str) -> Option<i128> {
    let text = input.trim();
    let text = text
        .strip_prefix("T#")
        .or_else(|| text.strip_prefix("TIME#"))
        .unwrap_or(text);
    parse_duration_text_ms(text)
}

fn parse_duration_text_ms(input: &str) -> Option<i128> {
    let mut text = input.trim().replace('_', "").to_ascii_lowercase();
    let sign = if text.starts_with('-') {
        text.remove(0);
        -1_i128
    } else {
        1_i128
    };
    if text.is_empty() {
        return None;
    }

    let mut rest = text.as_str();
    let mut total = 0.0_f64;
    let mut saw_unit = false;
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
        if number_text == "." || number_text.chars().filter(|ch| *ch == '.').count() > 1 {
            return None;
        }
        let fractional = number_text.contains('.');
        let value = number_text.parse::<f64>().ok()?;
        rest = &rest[number_len..];
        if rest.is_empty() {
            return if saw_unit {
                None
            } else {
                Some((sign as f64 * value) as i128)
            };
        }

        let (factor, consumed) = if rest.starts_with("ms") {
            (1.0_f64, 2)
        } else if rest.starts_with('d') {
            (86_400_000.0_f64, 1)
        } else if rest.starts_with('h') {
            (3_600_000.0_f64, 1)
        } else if rest.starts_with('m') {
            (60_000.0_f64, 1)
        } else if rest.starts_with('s') {
            (1_000.0_f64, 1)
        } else {
            return None;
        };
        rest = &rest[consumed..];
        if fractional && !rest.is_empty() {
            return None;
        }
        total += value * factor;
        saw_unit = true;
    }
    saw_unit.then_some((sign as f64 * total) as i128)
}

fn parse_date_time_ms(input: &str) -> Option<i128> {
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

fn parse_date_days(input: &str) -> Option<i64> {
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || !valid_date(year, month, day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn parse_time_of_day_ms(input: &str) -> Option<i128> {
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

fn date_days_to_string(days: i128) -> String {
    let parts = civil_from_days(days as i64);
    format!("D#{:04}-{:02}-{:02}", parts.0, parts.1, parts.2)
}

fn tod_ms_to_string(ms: i128) -> String {
    let ms = ms.rem_euclid(86_400_000);
    let hour = ms / 3_600_000;
    let minute = (ms % 3_600_000) / 60_000;
    let second = (ms % 60_000) / 1000;
    let millisecond = ms % 1000;
    format!("TOD#{hour:02}:{minute:02}:{second:02}.{millisecond:03}")
}

fn dt_ms_to_string(ms: i128) -> String {
    let mut days = ms / 86_400_000;
    let mut tod = ms % 86_400_000;
    if tod < 0 {
        tod += 86_400_000;
        days -= 1;
    }
    let date = civil_from_days(days as i64);
    let tod_text = tod_ms_to_string(tod);
    format!(
        "DT#{:04}-{:02}-{:02}-{}",
        date.0,
        date.1,
        date.2,
        tod_text.trim_start_matches("TOD#")
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

fn min_max(args: &[Value], max: bool) -> Option<Value> {
    if args.is_empty() {
        return None;
    }

    if args.iter().any(|value| matches!(value, Value::Real(_))) {
        let mut current = args.first()?.as_f64()?;
        for value in &args[1..] {
            let next = value.as_f64()?;
            current = if max {
                current.max(next)
            } else {
                current.min(next)
            };
        }
        Some(Value::Real(current))
    } else if args.iter().any(|value| matches!(value, Value::TimeMs(_))) {
        let mut current = value_to_time_ms(args.first()?)?;
        for value in &args[1..] {
            let next = value_to_time_ms(value)?;
            current = if max {
                current.max(next)
            } else {
                current.min(next)
            };
        }
        Some(Value::TimeMs(current))
    } else {
        let mut current = args.first()?.as_i64()?;
        for value in &args[1..] {
            let next = value.as_i64()?;
            current = if max {
                current.max(next)
            } else {
                current.min(next)
            };
        }
        Some(Value::Int(current))
    }
}

fn limit(args: &[Value]) -> Option<Value> {
    if args.len() != 3 {
        return None;
    }

    if args.iter().any(|value| matches!(value, Value::Real(_))) {
        let mn = args.first()?.as_f64()?;
        let input = args.get(1)?.as_f64()?;
        let mx = args.get(2)?.as_f64()?;
        Some(Value::Real(input.max(mn).min(mx)))
    } else if args.iter().any(|value| matches!(value, Value::TimeMs(_))) {
        let mn = value_to_time_ms(args.first()?)?;
        let input = value_to_time_ms(args.get(1)?)?;
        let mx = value_to_time_ms(args.get(2)?)?;
        Some(Value::TimeMs(input.max(mn).min(mx)))
    } else {
        let mn = args.first()?.as_i64()?;
        let input = args.get(1)?.as_i64()?;
        let mx = args.get(2)?.as_i64()?;
        Some(Value::Int(input.max(mn).min(mx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_standard_function_formal_input_positions() {
        assert_eq!(standard_function_input_index("LIMIT", "MN"), Some(0));
        assert_eq!(standard_function_input_index("LIMIT", "IN"), Some(1));
        assert_eq!(standard_function_input_index("LIMIT", "MX"), Some(2));
        assert_eq!(standard_function_input_index("SEL", "G"), Some(0));
        assert_eq!(standard_function_input_index("SEL", "IN0"), Some(1));
        assert_eq!(standard_function_input_index("SEL", "IN1"), Some(2));
        assert_eq!(standard_function_input_index("MUX", "K"), Some(0));
        assert_eq!(standard_function_input_index("MUX", "IN0"), Some(1));
        assert_eq!(standard_function_input_index("MUX", "IN3"), Some(4));
        assert_eq!(standard_function_input_index("LEFT", "L"), Some(1));
        assert_eq!(standard_function_input_index("SHL", "N"), Some(1));
        assert_eq!(standard_function_input_index("REAL_TO_INT", "IN"), Some(0));
        assert_eq!(standard_function_input_index("LIMIT", "BAD"), None);
    }

    #[test]
    fn rejects_out_of_range_integer_conversions() {
        assert_eq!(
            eval_standard_function("INT_TO_USINT", &[Value::Int(255)]),
            Some(Value::Int(255))
        );
        assert_eq!(
            eval_standard_function("INT_TO_USINT", &[Value::Int(256)]),
            None
        );
        assert_eq!(
            eval_standard_function("INT_TO_SINT", &[Value::Int(-129)]),
            None
        );
        assert_eq!(
            eval_standard_function("INT_TO_WORD", &[Value::Int(65_535)]),
            Some(Value::Int(65_535))
        );
        assert_eq!(
            eval_standard_function("INT_TO_WORD", &[Value::Int(65_536)]),
            None
        );
    }

    #[test]
    fn evaluates_date_time_string_conversions() {
        assert_eq!(
            eval_standard_function(
                "STRING_TO_DATE",
                &[Value::String("D#1970-01-02".to_string())]
            ),
            Some(Value::TimeMs(1))
        );
        assert_eq!(
            eval_standard_function(
                "STRING_TO_TOD",
                &[Value::String("TOD#01:02:03.004".to_string())],
            ),
            Some(Value::TimeMs(3_723_004))
        );
        assert_eq!(
            eval_standard_function(
                "STRING_TO_TIME",
                &[Value::String("T#1h2m3s4ms".to_string())],
            ),
            Some(Value::TimeMs(3_723_004))
        );
        assert_eq!(
            eval_standard_function("STRING_TO_TIME", &[Value::String("T#1.5s".to_string())],),
            Some(Value::TimeMs(1_500))
        );
        assert_eq!(
            eval_standard_function(
                "STRING_TO_DT",
                &[Value::String("DT#1970-01-02-01:02:03.004".to_string())],
            ),
            Some(Value::TimeMs(90_123_004))
        );
        assert_eq!(
            eval_standard_function("DATE_TO_STRING", &[Value::TimeMs(1)]),
            Some(Value::String("D#1970-01-02".to_string()))
        );
        assert_eq!(
            eval_standard_function("TOD_TO_WSTRING", &[Value::TimeMs(3_723_004)]),
            Some(Value::WString("TOD#01:02:03.004".to_string()))
        );
        assert_eq!(
            eval_standard_function("DT_TO_STRING", &[Value::TimeMs(90_123_004)]),
            Some(Value::String("DT#1970-01-02-01:02:03.004".to_string()))
        );
        assert_eq!(
            eval_standard_function(
                "STRING_TO_DATE",
                &[Value::String("D#1970-02-31".to_string())]
            ),
            None
        );
    }

    #[test]
    fn evaluates_conversion_matrix_for_supported_elementary_types() {
        const TYPES: &[&str] = &[
            "BOOL",
            "SINT",
            "INT",
            "DINT",
            "LINT",
            "USINT",
            "UINT",
            "UDINT",
            "ULINT",
            "BYTE",
            "WORD",
            "DWORD",
            "LWORD",
            "REAL",
            "LREAL",
            "STRING",
            "WSTRING",
            "TIME",
            "DATE",
            "TOD",
            "TIME_OF_DAY",
            "DT",
            "DATE_AND_TIME",
        ];

        for source in TYPES {
            for target in TYPES {
                let name = format!("{source}_TO_{target}");
                let value = sample_conversion_value(source, target);
                assert!(
                    is_standard_function(&name),
                    "{name} should be recognized as a standard conversion"
                );
                assert!(
                    eval_standard_function(&name, &[value]).is_some(),
                    "{name} should evaluate for the sample input"
                );
            }
        }

        assert_eq!(
            eval_standard_function("BCD_TO_INT", &[Value::Int(0x42)]),
            Some(Value::Int(42))
        );
        assert_eq!(
            eval_standard_function("INT_TO_BCD", &[Value::Int(42)]),
            Some(Value::Int(0x42))
        );
        for bit_type in ["BYTE", "WORD", "DWORD", "LWORD"] {
            for int_type in [
                "SINT", "INT", "DINT", "LINT", "USINT", "UINT", "UDINT", "ULINT",
            ] {
                assert!(eval_standard_function(
                    &format!("{bit_type}_BCD_TO_{int_type}"),
                    &[Value::Int(0x12)],
                )
                .is_some());
                assert!(eval_standard_function(
                    &format!("{int_type}_TO_BCD_{bit_type}"),
                    &[Value::Int(12)],
                )
                .is_some());
            }
        }
    }

    fn sample_conversion_value(source: &str, target: &str) -> Value {
        match source {
            "BOOL" => Value::Bool(true),
            "REAL" | "LREAL" => Value::Real(1.25),
            "STRING" => Value::String(sample_conversion_text(target).to_string()),
            "WSTRING" => Value::WString(sample_conversion_text(target).to_string()),
            "TIME" | "DATE" | "TOD" | "TIME_OF_DAY" | "DT" | "DATE_AND_TIME" => Value::TimeMs(1),
            _ => Value::Int(1),
        }
    }

    fn sample_conversion_text(target: &str) -> &'static str {
        match target {
            "BOOL" => "TRUE",
            "REAL" | "LREAL" => "1.25",
            "TIME" => "T#1ms",
            "DATE" => "D#1970-01-02",
            "TOD" | "TIME_OF_DAY" => "TOD#00:00:00.001",
            "DT" | "DATE_AND_TIME" => "DT#1970-01-01-00:00:00.001",
            "STRING" | "WSTRING" => "text",
            _ => "1",
        }
    }

    #[test]
    fn rejects_negative_bit_shift_counts() {
        assert_eq!(
            eval_standard_function("SHL", &[Value::Int(1), Value::Int(-1)]),
            None
        );
        assert_eq!(
            eval_standard_function("ROR", &[Value::Int(1), Value::Int(-1)]),
            None
        );
        assert_eq!(
            eval_standard_function("SHL", &[Value::Int(1), Value::Int(64)]),
            Some(Value::Int(0))
        );
        assert_eq!(
            eval_standard_function("SHR", &[Value::Int(-1), Value::Int(63)]),
            Some(Value::Int(1))
        );
    }

    #[test]
    fn enforces_string_function_positions_and_lengths() {
        assert_eq!(
            eval_standard_function(
                "INSERT",
                &[
                    Value::String("ABC".to_string()),
                    Value::String("XY".to_string()),
                    Value::Int(2),
                ],
            ),
            Some(Value::String("ABXYC".to_string()))
        );
        assert_eq!(
            eval_standard_function(
                "REPLACE",
                &[
                    Value::String("ABCDE".to_string()),
                    Value::String("X".to_string()),
                    Value::Int(2),
                    Value::Int(3),
                ],
            ),
            Some(Value::String("ABXE".to_string()))
        );
        assert_eq!(
            eval_standard_function("LEFT", &[Value::String("ABC".to_string()), Value::Int(4)],),
            None
        );
        assert_eq!(
            eval_standard_function(
                "MID",
                &[
                    Value::String("ABC".to_string()),
                    Value::Int(2),
                    Value::Int(3),
                ],
            ),
            None
        );
        assert_eq!(
            eval_standard_function(
                "DELETE",
                &[
                    Value::String("ABC".to_string()),
                    Value::Int(1),
                    Value::Int(0),
                ],
            ),
            None
        );
        assert_eq!(
            eval_standard_function("RIGHT", &[Value::String("ABC".to_string()), Value::Int(-1)],),
            None
        );
        assert_eq!(
            eval_standard_function("LEN", &[Value::WString("éλx".to_string())]),
            Some(Value::Int(3))
        );
        assert_eq!(
            eval_standard_function("LEFT", &[Value::WString("éλx".to_string()), Value::Int(2)],),
            Some(Value::WString("éλ".to_string()))
        );
        assert_eq!(
            eval_standard_function("RIGHT", &[Value::WString("xéλ".to_string()), Value::Int(2)],),
            Some(Value::WString("éλ".to_string()))
        );
        assert_eq!(
            eval_standard_function(
                "MID",
                &[
                    Value::WString("xéλy".to_string()),
                    Value::Int(2),
                    Value::Int(2),
                ],
            ),
            Some(Value::WString("éλ".to_string()))
        );
        assert_eq!(
            eval_standard_function(
                "DELETE",
                &[
                    Value::WString("aéλb".to_string()),
                    Value::Int(2),
                    Value::Int(2),
                ],
            ),
            Some(Value::WString("ab".to_string()))
        );
        assert_eq!(
            eval_standard_function(
                "INSERT",
                &[
                    Value::WString("aλ".to_string()),
                    Value::WString("é".to_string()),
                    Value::Int(1),
                ],
            ),
            Some(Value::WString("aéλ".to_string()))
        );
        assert_eq!(
            eval_standard_function(
                "FIND",
                &[
                    Value::WString("aéλb".to_string()),
                    Value::WString("λ".to_string()),
                ],
            ),
            Some(Value::Int(3))
        );
    }

    #[test]
    fn limit_preserves_magnitude_value_family() {
        assert_eq!(
            eval_standard_function("LIMIT", &[Value::Int(0), Value::Int(5), Value::Int(3)],),
            Some(Value::Int(3))
        );
        assert_eq!(
            eval_standard_function(
                "LIMIT",
                &[Value::TimeMs(100), Value::TimeMs(50), Value::TimeMs(200)],
            ),
            Some(Value::TimeMs(100))
        );
        assert_eq!(
            eval_standard_function(
                "LIMIT",
                &[Value::Real(0.0), Value::Real(0.5), Value::Real(1.0)],
            ),
            Some(Value::Real(0.5))
        );
    }

    #[test]
    fn evaluates_date_and_time_table_functions() {
        assert_eq!(
            eval_standard_function(
                "ADD_TOD_TIME",
                &[Value::TimeMs(1_000), Value::TimeMs(2_000)]
            ),
            Some(Value::TimeMs(3_000))
        );
        assert_eq!(
            eval_standard_function(
                "ADD_DT_TIME",
                &[Value::TimeMs(86_401_000), Value::TimeMs(2_000)]
            ),
            Some(Value::TimeMs(86_403_000))
        );
        assert_eq!(
            eval_standard_function("SUB_DATE_DATE", &[Value::TimeMs(3), Value::TimeMs(1)]),
            Some(Value::TimeMs(172_800_000))
        );
        assert_eq!(
            eval_standard_function("SUB_TOD_TOD", &[Value::TimeMs(3_000), Value::TimeMs(1_000)]),
            Some(Value::TimeMs(2_000))
        );
        assert_eq!(
            eval_standard_function(
                "SUB_DT_DT",
                &[Value::TimeMs(86_403_000), Value::TimeMs(86_401_000)]
            ),
            Some(Value::TimeMs(2_000))
        );
        assert_eq!(
            eval_standard_function("MULTIME", &[Value::TimeMs(1_500), Value::Int(2)]),
            Some(Value::TimeMs(3_000))
        );
        assert_eq!(
            eval_standard_function("DIVTIME", &[Value::TimeMs(3_000), Value::Int(2)]),
            Some(Value::TimeMs(1_500))
        );
        assert_eq!(
            eval_standard_function(
                "CONCAT_DATE",
                &[Value::Int(1970), Value::Int(1), Value::Int(3)]
            ),
            Some(Value::TimeMs(2))
        );
        assert_eq!(
            eval_standard_function(
                "CONCAT_TOD",
                &[Value::Int(0), Value::Int(0), Value::Int(3), Value::Int(250)]
            ),
            Some(Value::TimeMs(3_250))
        );
        assert_eq!(
            eval_standard_function(
                "CONCAT_DT",
                &[
                    Value::Int(1970),
                    Value::Int(1),
                    Value::Int(3),
                    Value::Int(0),
                    Value::Int(0),
                    Value::Int(3),
                    Value::Int(250)
                ]
            ),
            Some(Value::TimeMs(172_803_250))
        );
        assert_eq!(
            eval_standard_function("DAY_OF_WEEK", &[Value::TimeMs(0)]),
            Some(Value::Int(4))
        );
    }
}
