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
            name: "SUB_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "MUL_TIME",
            kind: StandardSymbolKind::Function,
            clause: "2.5.1.5.3",
        },
        StandardSymbol {
            name: "DIV_TIME",
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

pub fn is_standard_function_block(name: &str) -> bool {
    let name = canonical_identifier(name);
    standard_symbols()
        .iter()
        .any(|symbol| symbol.kind == StandardSymbolKind::FunctionBlock && symbol.name == name)
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
        "LIMIT" => {
            let mn = args.get(0)?.as_f64()?;
            let input = args.get(1)?.as_f64()?;
            let mx = args.get(2)?.as_f64()?;
            Some(Value::Real(input.max(mn).min(mx)))
        }
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
        "ADD_TIME" => time_pair(args, |a, b| a + b),
        "SUB_TIME" => time_pair(args, |a, b| a - b),
        "MUL_TIME" => time_scale(args, |time, factor| time * factor),
        "DIV_TIME" => {
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
    let (_, target) = name.split_once("_TO_")?;
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
        "STRING" | "WSTRING" => Some(Value::String(value_to_string(value))),
        "TIME" => value_to_time_ms(value).map(Value::TimeMs),
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
    if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
        let left = left.to_string();
        let right = right.to_string();
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
    let value = args.first()?.as_i64()?;
    let shift = args.get(1)?.as_i64()?.clamp(0, 63) as u32;
    if right {
        Some(Value::Int(value >> shift))
    } else {
        Some(Value::Int(value << shift))
    }
}

fn bit_rotate(args: &[Value], right: bool) -> Option<Value> {
    let value = args.first()?.as_i64()? as u64;
    let shift = args.get(1)?.as_i64()?.rem_euclid(64) as u32;
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
    let len = args.get(1)?.as_i64()?.max(0) as usize;
    Some(Value::String(input.chars().take(len).collect()))
}

fn string_right(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = args.get(1)?.as_i64()?.max(0) as usize;
    let chars = input.chars().collect::<Vec<_>>();
    let start = chars.len().saturating_sub(len);
    Some(Value::String(chars[start..].iter().collect()))
}

fn string_mid(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = args.get(1)?.as_i64()?.max(0) as usize;
    let pos = args.get(2)?.as_i64()?.max(1) as usize - 1;
    Some(Value::String(input.chars().skip(pos).take(len).collect()))
}

fn string_concat(args: &[Value]) -> Option<Value> {
    let mut out = String::new();
    for value in args {
        out.push_str(&value_string(value)?);
    }
    Some(Value::String(out))
}

fn string_insert(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let insert = value_string(args.get(1)?)?;
    let pos = args.get(2)?.as_i64()?.max(1) as usize - 1;
    let mut chars = input.chars().collect::<Vec<_>>();
    let pos = pos.min(chars.len());
    for (offset, ch) in insert.chars().enumerate() {
        chars.insert(pos + offset, ch);
    }
    Some(Value::String(chars.into_iter().collect()))
}

fn string_delete(args: &[Value]) -> Option<Value> {
    let input = value_string(args.first()?)?;
    let len = args.get(1)?.as_i64()?.max(0) as usize;
    let pos = args.get(2)?.as_i64()?.max(1) as usize - 1;
    Some(Value::String(
        input
            .chars()
            .enumerate()
            .filter_map(|(index, ch)| {
                if index >= pos && index < pos.saturating_add(len) {
                    None
                } else {
                    Some(ch)
                }
            })
            .collect(),
    ))
}

fn string_replace(args: &[Value]) -> Option<Value> {
    let deleted = string_delete(&[
        args.first()?.clone(),
        args.get(2)?.clone(),
        args.get(3)?.clone(),
    ])?;
    let Value::String(deleted) = deleted else {
        return None;
    };
    string_insert(&[
        Value::String(deleted),
        args.get(1)?.clone(),
        args.get(3)?.clone(),
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

fn time_pair(args: &[Value], op: fn(i128, i128) -> i128) -> Option<Value> {
    Some(Value::TimeMs(op(
        value_time_ms(args.first()?)?,
        value_time_ms(args.get(1)?)?,
    )))
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
        _ => None,
    }
}

fn value_time_ms(value: &Value) -> Option<i128> {
    match value {
        Value::TimeMs(value) => Some(*value),
        value => value.as_i64().map(|value| value as i128),
    }
}

fn value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::String(value) => match canonical_identifier(value).as_str() {
            "TRUE" | "1" => Some(true),
            "FALSE" | "0" | "" => Some(false),
            _ => None,
        },
        Value::TimeMs(value) => Some(*value != 0),
        value => value.as_bool(),
    }
}

fn value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::String(value) => value.trim().parse::<i64>().ok(),
        Value::TimeMs(value) => i64::try_from(*value).ok(),
        value => value.as_i64(),
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::String(value) => value.trim().parse::<f64>().ok(),
        Value::TimeMs(value) => Some(*value as f64),
        value => value.as_f64(),
    }
}

fn value_to_time_ms(value: &Value) -> Option<i128> {
    match value {
        Value::TimeMs(value) => Some(*value),
        Value::String(value) => parse_time_string_ms(value),
        value => value.as_i64().map(i128::from),
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
        Value::String(value) => value.clone(),
        Value::TimeMs(value) => format!("T#{value}ms"),
        Value::Array(_) | Value::Struct(_) | Value::Unit => value.to_string(),
    }
}

fn parse_time_string_ms(input: &str) -> Option<i128> {
    let text = input.trim();
    let text = text
        .strip_prefix("T#")
        .or_else(|| text.strip_prefix("TIME#"))
        .unwrap_or(text);
    if let Some(ms) = text.strip_suffix("ms") {
        return ms.trim().parse::<i128>().ok();
    }
    if let Some(seconds) = text.strip_suffix('s') {
        return seconds
            .trim()
            .parse::<i128>()
            .ok()
            .map(|value| value * 1000);
    }
    if let Some(minutes) = text.strip_suffix('m') {
        return minutes
            .trim()
            .parse::<i128>()
            .ok()
            .map(|value| value * 60_000);
    }
    text.parse::<i128>().ok()
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
