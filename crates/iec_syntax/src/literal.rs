// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_ir::{canonical_identifier, ElementaryType, Expr, Identifier, Literal};

pub(crate) fn parse_number_literal_checked(raw: &str) -> (Expr, Vec<String>) {
    let mut diagnostics = Vec::new();
    if !valid_underscore_placement(raw) {
        diagnostics.push(format!(
            "invalid underscore placement in numeric literal '{raw}'"
        ));
    }
    let normalized = raw.replace('_', "");
    if normalized.contains('.') || normalized.contains('e') || normalized.contains('E') {
        match normalized.parse::<f64>() {
            Ok(value) if value.is_finite() => (Expr::Literal(Literal::Real(value)), diagnostics),
            _ => {
                diagnostics.push(format!("invalid real literal '{raw}'"));
                (Expr::Literal(Literal::Real(0.0)), diagnostics)
            }
        }
    } else {
        match normalized.parse::<i64>() {
            Ok(value) => (Expr::Literal(Literal::Int(value)), diagnostics),
            Err(_) => {
                diagnostics.push(format!("invalid integer literal '{raw}'"));
                (Expr::Literal(Literal::Int(0)), diagnostics)
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn parse_hash_literal(raw: &str) -> Literal {
    parse_hash_literal_checked(raw).0
}

pub(crate) fn parse_hash_literal_checked(raw: &str) -> (Literal, Vec<String>) {
    let mut diagnostics = Vec::new();
    let Some((prefix, value)) = raw.split_once('#') else {
        diagnostics.push(format!("invalid typed literal '{raw}'"));
        return (
            Literal::Typed {
                type_name: Identifier::new("<literal>"),
                value: raw.to_string(),
            },
            diagnostics,
        );
    };
    let prefix_upper = canonical_identifier(prefix);

    let literal = match prefix_upper.as_str() {
        "TRUE" => Literal::Bool(true),
        "FALSE" => Literal::Bool(false),
        "BOOL" => match parse_bool_literal_value(value) {
            Some(value) => Literal::Bool(value),
            None => {
                diagnostics.push(format!("invalid BOOL literal value '{value}'"));
                Literal::Bool(false)
            }
        },
        "T" | "TIME" => match parse_duration_ms_checked(value) {
            Ok(value) => Literal::DurationMs(value),
            Err(message) => {
                diagnostics.push(message);
                Literal::DurationMs(0)
            }
        },
        "D" | "DATE" => {
            if parse_date_days(value).is_none() {
                diagnostics.push(format!("invalid DATE literal '{raw}'"));
            }
            Literal::Date(value.to_string())
        }
        "TOD" | "TIME_OF_DAY" => {
            if parse_time_of_day_ms(value).is_none() {
                diagnostics.push(format!("invalid TIME_OF_DAY literal '{raw}'"));
            }
            Literal::TimeOfDay(value.to_string())
        }
        "DT" | "DATE_AND_TIME" => {
            if parse_date_time_ms(value).is_none() {
                diagnostics.push(format!("invalid DATE_AND_TIME literal '{raw}'"));
            }
            Literal::DateAndTime(value.to_string())
        }
        "STRING" => Literal::String(decode_typed_character_string(
            raw,
            value,
            false,
            &mut diagnostics,
        )),
        "WSTRING" => Literal::WString(decode_typed_character_string(
            raw,
            value,
            true,
            &mut diagnostics,
        )),
        "2" => parse_based_int_literal(value, 2, raw, &mut diagnostics),
        "8" => parse_based_int_literal(value, 8, raw, &mut diagnostics),
        "16" => parse_based_int_literal(value, 16, raw, &mut diagnostics),
        _ => Literal::Typed {
            type_name: Identifier::new(prefix),
            value: normalize_typed_literal_value(prefix, value, &mut diagnostics)
                .unwrap_or_else(|| value.to_string()),
        },
    };
    (literal, diagnostics)
}

fn decode_typed_character_string(
    full_raw: &str,
    raw: &str,
    wide: bool,
    diagnostics: &mut Vec<String>,
) -> String {
    let Some(quote @ ('\'' | '"')) = raw.chars().next() else {
        diagnostics.push(format!("invalid typed string literal '{full_raw}'"));
        return raw.to_string();
    };
    if !raw.ends_with(quote) || raw.len() == quote.len_utf8() {
        diagnostics.push(format!("unterminated typed string literal '{full_raw}'"));
        return raw.trim_matches(quote).to_string();
    }

    let body = &raw[quote.len_utf8()..raw.len() - quote.len_utf8()];
    decode_character_string_body(full_raw, body, quote, wide, diagnostics)
}

fn decode_character_string_body(
    full_raw: &str,
    body: &str,
    _quote: char,
    wide: bool,
    diagnostics: &mut Vec<String>,
) -> String {
    let mut value = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            if ch.is_control() {
                diagnostics.push(format!(
                    "unescaped control character {} in character string literal '{full_raw}'",
                    control_char_label(ch)
                ));
            }
            if !wide && (ch as u32) > 0xFF {
                diagnostics.push(format!(
                    "character {} exceeds single-byte STRING range in literal '{full_raw}'",
                    control_char_label(ch)
                ));
            }
            value.push(ch);
            continue;
        }

        let Some(escaped) = chars.peek().copied() else {
            diagnostics.push(format!(
                "unterminated character string escape in literal '{full_raw}'"
            ));
            break;
        };
        let decoded = match escaped {
            '$' => Some('$'),
            '\'' => Some('\''),
            '"' => Some('"'),
            'L' | 'l' | 'N' | 'n' => Some('\n'),
            'P' | 'p' => Some('\u{000C}'),
            'R' | 'r' => Some('\r'),
            'T' | 't' => Some('\t'),
            _ => None,
        };
        if let Some(decoded) = decoded {
            value.push(decoded);
            chars.next();
            continue;
        }

        if escaped.is_ascii_hexdigit() {
            let required_digits = if wide { 4 } else { 2 };
            let mut digits = String::new();
            while digits.len() < required_digits {
                let Some(hex) = chars.peek().copied() else {
                    break;
                };
                if !hex.is_ascii_hexdigit() {
                    break;
                }
                digits.push(hex);
                chars.next();
            }
            if digits.len() != required_digits {
                diagnostics.push(format!(
                    "invalid character string hex escape '${digits}' in literal '{full_raw}': expected {required_digits} hexadecimal digit(s)"
                ));
                continue;
            }
            let code = u32::from_str_radix(&digits, 16).unwrap_or(0);
            if let Some(decoded) = char::from_u32(code) {
                value.push(decoded);
            } else {
                diagnostics.push(format!(
                    "invalid character code '${digits}' in literal '{full_raw}'"
                ));
            }
            continue;
        }

        diagnostics.push(format!(
            "invalid character string escape '${escaped}' in literal '{full_raw}'"
        ));
        chars.next();
    }
    value
}

fn parse_duration_ms_checked(raw: &str) -> Result<i128, String> {
    let mut chars = raw.replace('_', "").to_ascii_lowercase();
    let sign = if chars.starts_with('-') {
        chars.remove(0);
        -1_i128
    } else {
        1_i128
    };

    let mut rest = chars.as_str();
    if rest.is_empty() {
        return Err(format!("invalid duration literal 'T#{raw}'"));
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
            return Err(format!("invalid duration literal 'T#{raw}'"));
        }
        let number_text = &rest[..number_len];
        if !valid_decimal_component(number_text) {
            return Err(format!("invalid duration component '{number_text}'"));
        }
        let number = number_text
            .parse::<f64>()
            .map_err(|_| format!("invalid duration component '{number_text}'"))?;
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
            return Err(format!("invalid duration unit in 'T#{raw}'"));
        };
        if rank >= previous_rank {
            return Err(format!(
                "duration components must be ordered largest to smallest in 'T#{raw}'"
            ));
        }
        let has_more = rest.get(consumed..).is_some_and(|tail| !tail.is_empty());
        if has_more && number_text.contains('.') {
            return Err(format!(
                "fractional duration component '{number_text}{unit}' must be last"
            ));
        }
        if has_more || previous_rank != 6 {
            match unit {
                "h" if number >= 24.0 => {
                    return Err(format!("duration hours component {number_text} exceeds 23"));
                }
                "m" | "s" if number >= 60.0 => {
                    return Err(format!(
                        "duration {unit} component {number_text} exceeds 59"
                    ));
                }
                "ms" if number >= 1000.0 => {
                    return Err(format!(
                        "duration milliseconds component {number_text} exceeds 999"
                    ));
                }
                _ => {}
            }
        }
        saw_component = true;
        previous_rank = rank;
        total += number * factor;
        rest = &rest[consumed..];
    }

    if !saw_component {
        return Err(format!("invalid duration literal 'T#{raw}'"));
    }
    Ok(sign * total.round() as i128)
}

fn parse_bool_literal_value(raw: &str) -> Option<bool> {
    match canonical_identifier(raw).as_str() {
        "1" | "TRUE" => Some(true),
        "0" | "FALSE" => Some(false),
        _ => None,
    }
}

pub(crate) fn control_char_label(ch: char) -> String {
    format!("U+{:04X}", ch as u32)
}

fn parse_based_int_literal(
    raw: &str,
    base: u32,
    full_raw: &str,
    diagnostics: &mut Vec<String>,
) -> Literal {
    match parse_based_i64(raw, base) {
        Ok(value) => Literal::Int(value),
        Err(message) => {
            diagnostics.push(format!("{message} in literal '{full_raw}'"));
            Literal::Int(0)
        }
    }
}

fn normalize_typed_literal_value(
    type_name: &str,
    raw: &str,
    diagnostics: &mut Vec<String>,
) -> Option<String> {
    let elementary = ElementaryType::parse(type_name)?;
    match elementary {
        ElementaryType::Bool => parse_bool_literal_value(raw).map(|value| {
            if value {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }),
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
        | ElementaryType::Lword => match parse_integer_text_i128(raw) {
            Ok(value) => Some(value.to_string()),
            Err(message) => {
                diagnostics.push(format!("{message} in typed literal '{type_name}#{raw}'"));
                Some("0".to_string())
            }
        },
        ElementaryType::Real | ElementaryType::Lreal => {
            let normalized = raw.replace('_', "");
            if !valid_underscore_placement(raw) || normalized.parse::<f64>().is_err() {
                diagnostics.push(format!("invalid real typed literal '{type_name}#{raw}'"));
                Some("0.0".to_string())
            } else {
                Some(normalized)
            }
        }
        ElementaryType::Time => parse_duration_ms_checked(raw)
            .map(|value| value.to_string())
            .map_err(|message| diagnostics.push(message))
            .ok(),
        ElementaryType::Date | ElementaryType::TimeOfDay | ElementaryType::DateAndTime => None,
        ElementaryType::String | ElementaryType::WString => None,
    }
}

fn parse_integer_text_i128(raw: &str) -> Result<i128, String> {
    if let Some((base, digits)) = raw.split_once('#') {
        let base = match canonical_identifier(base).as_str() {
            "2" => 2,
            "8" => 8,
            "16" => 16,
            other => return Err(format!("unsupported integer base '{other}'")),
        };
        return parse_based_i128(digits, base);
    }
    if !valid_underscore_placement(raw) {
        return Err(format!(
            "invalid underscore placement in integer literal '{raw}'"
        ));
    }
    raw.replace('_', "")
        .parse::<i128>()
        .map_err(|_| format!("invalid integer literal '{raw}'"))
}

fn parse_based_i64(raw: &str, base: u32) -> Result<i64, String> {
    parse_based_i128(raw, base).and_then(|value| {
        i64::try_from(value).map_err(|_| format!("based literal '{raw}' is outside LINT range"))
    })
}

fn parse_based_i128(raw: &str, base: u32) -> Result<i128, String> {
    if !valid_underscore_placement(raw) {
        return Err(format!(
            "invalid underscore placement in based literal '{raw}'"
        ));
    }
    let digits = raw.replace('_', "");
    if digits.is_empty() {
        return Err("empty based literal".to_string());
    }
    if !digits.chars().all(|ch| ch.is_digit(base)) {
        return Err(format!("invalid base-{base} digit sequence '{raw}'"));
    }
    i128::from_str_radix(&digits, base).map_err(|_| format!("based literal '{raw}' is too large"))
}

fn valid_underscore_placement(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    !bytes.windows(2).any(|pair| pair == b"__")
}

fn valid_decimal_component(raw: &str) -> bool {
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
    if parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
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
