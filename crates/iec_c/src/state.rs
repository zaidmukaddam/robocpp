// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::fmt::{self, Write};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::ImplementationParameters;
use iec_stdlib::{is_standard_function, standard_function_input_index};

use crate::addressing::*;
use crate::expressions::*;
use crate::fb::*;
use crate::functions::*;
use crate::*;

pub(crate) fn program_vars_with_retain<'a>(
    program: &'a Pou,
    project: &'a Project,
) -> Vec<(&'a VarDecl, Option<RetainKind>)> {
    let mut vars = project_global_vars_with_retain(project, &program.name.canonical);
    for block in program
        .var_blocks
        .iter()
        .filter(|block| !matches!(block.kind, VarBlockKind::Access | VarBlockKind::External))
    {
        vars.extend(block.vars.iter().map(|var| (var, block.retain)));
    }
    vars
}

pub(crate) fn project_global_vars_with_retain<'a>(
    project: &'a Project,
    current_pou: &str,
) -> Vec<(&'a VarDecl, Option<RetainKind>)> {
    let mut vars = Vec::new();
    for pou in project.pous() {
        if pou.name.canonical == current_pou {
            continue;
        }
        for block in &pou.var_blocks {
            if block.kind == VarBlockKind::Global {
                vars.extend(block.vars.iter().map(|var| (var, block.retain)));
            }
        }
    }
    for configuration in project.library_elements.iter().filter_map(|element| {
        if let LibraryElement::Configuration(configuration) = element {
            Some(configuration)
        } else {
            None
        }
    }) {
        for block in configuration.var_blocks.iter().chain(
            configuration
                .resources
                .iter()
                .flat_map(|resource| resource.var_blocks.iter()),
        ) {
            if block.kind == VarBlockKind::Global {
                vars.extend(block.vars.iter().map(|var| (var, block.retain)));
            }
        }
    }
    vars
}

pub(crate) fn emit_temp_initialization(out: &mut CEmitter<'_>, program: &Pou, project: &Project) {
    for block in program
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Temp)
    {
        for var in &block.vars {
            emit_state_initialization(out, var, project);
        }
    }
}

pub(crate) fn user_function_block<'a>(
    project: &'a Project,
    spec: &DataTypeSpec,
) -> Option<&'a Pou> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    project
        .find_pou(&type_name.original)
        .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
}

pub(crate) fn emit_function_block_state_declaration(
    out: &mut CEmitter<'_>,
    instance: &str,
    spec: &DataTypeSpec,
    project: &Project,
) -> bool {
    if let Some(fields) = standard_fb_fields(spec) {
        for field in fields {
            c_writeln!(
                out,
                "    {} {};",
                field.c_type,
                fb_field_ident(instance, field.name)
            );
        }
        return true;
    }
    let Some(function_block) = user_function_block(project, spec) else {
        return false;
    };
    for field in function_block.variable_declarations() {
        let nested_instance = field_key_for_c(instance, &field.name.original);
        if emit_function_block_state_declaration(out, &nested_instance, &field.type_spec, project) {
            continue;
        }
        emit_c_declaration(
            out,
            "    ",
            &fb_field_ident(instance, &field.name.original),
            &field.type_spec,
            project,
        );
        if field.edge.is_some() {
            c_writeln!(
                out,
                "    bool {};",
                fb_field_ident(instance, &edge_state_field_name(&field.name.canonical))
            );
        }
    }
    true
}

pub(crate) fn emit_communication_abi(out: &mut CEmitter<'_>) {
    c_writeln!(out, "typedef struct {{");
    c_writeln!(out, "    const char *block;");
    c_writeln!(out, "    const char *instance;");
    c_writeln!(out, "    bool req;");
    c_writeln!(out, "    bool en_r;");
    c_writeln!(out, "    int64_t id;");
    c_writeln!(out, "    int64_t length;");
    c_writeln!(out, "}} rbcpp_comm_request;");
    c_writeln!(out, "typedef struct {{");
    c_writeln!(out, "    bool done;");
    c_writeln!(out, "    bool ndr;");
    c_writeln!(out, "    bool error;");
    c_writeln!(out, "    int64_t status;");
    c_writeln!(out, "}} rbcpp_comm_response;");
    c_writeln!(
        out,
        "typedef bool (*rbcpp_comm_hook)(void *ctx, const rbcpp_comm_request *request, rbcpp_comm_response *response);"
    );
    c_writeln!(out);
}

pub(crate) fn emit_target_abi(out: &mut CEmitter<'_>) {
    c_writeln!(out, "typedef enum {{ RBCPP_IO_INPUT, RBCPP_IO_OUTPUT, RBCPP_IO_MEMORY, RBCPP_IO_UNKNOWN }} rbcpp_io_direction;");
    c_writeln!(out, "typedef struct {{ const char *name; const char *location; rbcpp_io_direction direction; const char *type; const char *c_type; size_t size; }} rbcpp_io_symbol;");
    c_writeln!(out, "typedef struct {{ uint64_t cycle; int64_t cycle_ms; int64_t monotonic_ms; }} rbcpp_scan_context;");
    c_writeln!(out, "typedef bool (*rbcpp_io_read_hook)(void *ctx, const rbcpp_io_symbol *symbol, void *value, size_t size);");
    c_writeln!(out, "typedef bool (*rbcpp_io_write_hook)(void *ctx, const rbcpp_io_symbol *symbol, const void *value, size_t size);");
    c_writeln!(out, "typedef bool (*rbcpp_retain_load_hook)(void *ctx, const char *name, void *value, size_t size);");
    c_writeln!(out, "typedef bool (*rbcpp_retain_save_hook)(void *ctx, const char *name, const void *value, size_t size);");
    c_writeln!(out, "typedef int64_t (*rbcpp_time_ms_hook)(void *ctx);");
    c_writeln!(
        out,
        "typedef void (*rbcpp_scan_hook)(void *ctx, const rbcpp_scan_context *scan);"
    );
    c_writeln!(out, "typedef struct {{ rbcpp_io_read_hook io_read; rbcpp_io_write_hook io_write; rbcpp_retain_load_hook retain_load; rbcpp_retain_save_hook retain_save; rbcpp_time_ms_hook time_ms; rbcpp_scan_hook begin_scan; rbcpp_scan_hook end_scan; rbcpp_scan_hook watchdog_pet; }} rbcpp_target_hooks;");
    c_writeln!(out);
}

pub(crate) fn emit_bcd_helpers(out: &mut CEmitter<'_>) {
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_bcd_to_int(int64_t input) {{"
    );
    c_writeln!(out, "    if (input < 0) {{ return 0; }}");
    c_writeln!(out, "    uint64_t raw = (uint64_t)input;");
    c_writeln!(out, "    int64_t result = 0;");
    c_writeln!(out, "    int64_t place = 1;");
    c_writeln!(out, "    while (raw != 0) {{");
    c_writeln!(out, "        int64_t digit = (int64_t)(raw & 0x0fu);");
    c_writeln!(out, "        if (digit > 9) {{ return 0; }}");
    c_writeln!(out, "        result += digit * place;");
    c_writeln!(out, "        place *= 10;");
    c_writeln!(out, "        raw >>= 4;");
    c_writeln!(out, "    }}");
    c_writeln!(out, "    return result;");
    c_writeln!(out, "}}");
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_int_to_bcd(int64_t input) {{"
    );
    c_writeln!(out, "    if (input < 0) {{ return 0; }}");
    c_writeln!(out, "    uint64_t raw = 0;");
    c_writeln!(out, "    unsigned shift = 0;");
    c_writeln!(out, "    while (input != 0 && shift < 64) {{");
    c_writeln!(out, "        raw |= ((uint64_t)(input % 10)) << shift;");
    c_writeln!(out, "        input /= 10;");
    c_writeln!(out, "        shift += 4;");
    c_writeln!(out, "    }}");
    c_writeln!(out, "    return (int64_t)raw;");
    c_writeln!(out, "}}");
    c_writeln!(out);
}

pub(crate) fn emit_date_time_helpers(out: &mut CEmitter<'_>) {
    c_writeln!(out, "typedef struct {{ int64_t year; int64_t month; int64_t day; int64_t hour; int64_t minute; int64_t second; int64_t millisecond; }} rbcpp_datetime_parts;");
    c_writeln!(out, "static RBCPP_UNUSED bool rbcpp_is_leap_year(int64_t y) {{ return (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0); }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_days_in_month(int64_t y, int64_t m) {{ switch (m) {{ case 1: case 3: case 5: case 7: case 8: case 10: case 12: return 31; case 4: case 6: case 9: case 11: return 30; case 2: return rbcpp_is_leap_year(y) ? 29 : 28; default: return 0; }} }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_days_from_civil(int64_t y, int64_t m, int64_t d) {{ y -= m <= 2; int64_t era = (y >= 0 ? y : y - 399) / 400; int64_t yoe = y - era * 400; int64_t mp = m + (m > 2 ? -3 : 9); int64_t doy = (153 * mp + 2) / 5 + d - 1; int64_t doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; return era * 146097 + doe - 719468; }}");
    c_writeln!(out, "static RBCPP_UNUSED rbcpp_datetime_parts rbcpp_civil_from_days(int64_t z) {{ z += 719468; int64_t era = (z >= 0 ? z : z - 146096) / 146097; int64_t doe = z - era * 146097; int64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; int64_t y = yoe + era * 400; int64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100); int64_t mp = (5 * doy + 2) / 153; int64_t d = doy - (153 * mp + 2) / 5 + 1; int64_t m = mp + (mp < 10 ? 3 : -9); y += m <= 2; rbcpp_datetime_parts parts = {{ y, m, d, 0, 0, 0, 0 }}; return parts; }}");
    c_writeln!(out, "static RBCPP_UNUSED rbcpp_datetime_parts rbcpp_split_tod_parts(int64_t ms) {{ ms = ((ms % 86400000LL) + 86400000LL) % 86400000LL; rbcpp_datetime_parts parts = {{ 0, 0, 0, ms / 3600000LL, (ms % 3600000LL) / 60000LL, (ms % 60000LL) / 1000LL, ms % 1000LL }}; return parts; }}");
    c_writeln!(out, "static RBCPP_UNUSED rbcpp_datetime_parts rbcpp_split_dt_parts(int64_t dt) {{ int64_t days = dt / 86400000LL; int64_t tod = dt % 86400000LL; if (tod < 0) {{ tod += 86400000LL; days -= 1; }} rbcpp_datetime_parts date = rbcpp_civil_from_days(days); rbcpp_datetime_parts time = rbcpp_split_tod_parts(tod); date.hour = time.hour; date.minute = time.minute; date.second = time.second; date.millisecond = time.millisecond; return date; }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_concat_date(int64_t y, int64_t m, int64_t d) {{ if (m < 1 || m > 12 || d < 1 || d > rbcpp_days_in_month(y, m)) {{ return 0; }} return rbcpp_days_from_civil(y, m, d); }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_concat_tod(int64_t h, int64_t m, int64_t s, int64_t ms) {{ if (h < 0 || h > 23 || m < 0 || m > 59 || s < 0 || s > 59 || ms < 0 || ms > 999) {{ return 0; }} return (((h * 60) + m) * 60 + s) * 1000 + ms; }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_concat_dt(int64_t y, int64_t mo, int64_t d, int64_t h, int64_t mi, int64_t s, int64_t ms) {{ return rbcpp_concat_date(y, mo, d) * 86400000LL + rbcpp_concat_tod(h, mi, s, ms); }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_concat_date_tod(int64_t date_days, int64_t tod_ms) {{ return date_days * 86400000LL + tod_ms; }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_day_of_week(int64_t date_days) {{ return ((date_days + 3) % 7 + 7) % 7 + 1; }}");
    c_writeln!(out);
}

pub(crate) fn emit_string_helpers(out: &mut CEmitter<'_>) {
    c_writeln!(
        out,
        "static RBCPP_UNUSED void rbcpp_strassign(char *dest, size_t cap, const char *src) {{"
    );
    c_writeln!(out, "    if (cap == 0) {{ return; }}");
    c_writeln!(out, "    snprintf(dest, cap, \"%s\", src ? src : \"\");");
    c_writeln!(out, "}}");
    c_writeln!(
        out,
        "static RBCPP_UNUSED void rbcpp_wstrassign_utf8(uint32_t *dest, size_t cap, const char *src) {{"
    );
    c_writeln!(out, "    if (cap == 0) {{ return; }}");
    c_writeln!(out, "    size_t out = 0;");
    c_writeln!(
        out,
        "    const unsigned char *p = (const unsigned char *)(src ? src : \"\");"
    );
    c_writeln!(out, "    while (*p && out + 1 < cap) {{");
    c_writeln!(out, "        uint32_t cp = 0;");
    c_writeln!(out, "        if ((*p & 0x80u) == 0) {{ cp = *p++; }}");
    c_writeln!(out, "        else if ((*p & 0xE0u) == 0xC0u && (p[1] & 0xC0u) == 0x80u) {{ cp = ((uint32_t)(p[0] & 0x1Fu) << 6) | (uint32_t)(p[1] & 0x3Fu); p += 2; }}");
    c_writeln!(out, "        else if ((*p & 0xF0u) == 0xE0u && (p[1] & 0xC0u) == 0x80u && (p[2] & 0xC0u) == 0x80u) {{ cp = ((uint32_t)(p[0] & 0x0Fu) << 12) | ((uint32_t)(p[1] & 0x3Fu) << 6) | (uint32_t)(p[2] & 0x3Fu); p += 3; }}");
    c_writeln!(out, "        else if ((*p & 0xF8u) == 0xF0u && (p[1] & 0xC0u) == 0x80u && (p[2] & 0xC0u) == 0x80u && (p[3] & 0xC0u) == 0x80u) {{ cp = ((uint32_t)(p[0] & 0x07u) << 18) | ((uint32_t)(p[1] & 0x3Fu) << 12) | ((uint32_t)(p[2] & 0x3Fu) << 6) | (uint32_t)(p[3] & 0x3Fu); p += 4; }}");
    c_writeln!(out, "        else {{ cp = *p++; }}");
    c_writeln!(out, "        dest[out++] = cp;");
    c_writeln!(out, "    }}");
    c_writeln!(out, "    dest[out] = 0;");
    c_writeln!(out, "}}");
    c_writeln!(
        out,
        "static RBCPP_UNUSED char *rbcpp_strtmp(void) {{ static char buffers[8][RBCPP_STRING_CAP]; static unsigned index; index = (index + 1u) % 8u; buffers[index][0] = '\\0'; return buffers[index]; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_wstr_to_utf8(const uint32_t *s) {{"
    );
    c_writeln!(out, "    char *out = rbcpp_strtmp();");
    c_writeln!(out, "    size_t pos = 0;");
    c_writeln!(out, "    if (!s) {{ return out; }}");
    c_writeln!(
        out,
        "    for (size_t i = 0; s[i] && pos + 4 < RBCPP_STRING_CAP; ++i) {{"
    );
    c_writeln!(out, "        uint32_t cp = s[i];");
    c_writeln!(out, "        if (cp <= 0x7Fu) {{ out[pos++] = (char)cp; }}");
    c_writeln!(out, "        else if (cp <= 0x7FFu && pos + 2 < RBCPP_STRING_CAP) {{ out[pos++] = (char)(0xC0u | (cp >> 6)); out[pos++] = (char)(0x80u | (cp & 0x3Fu)); }}");
    c_writeln!(out, "        else if (cp <= 0xFFFFu && pos + 3 < RBCPP_STRING_CAP) {{ out[pos++] = (char)(0xE0u | (cp >> 12)); out[pos++] = (char)(0x80u | ((cp >> 6) & 0x3Fu)); out[pos++] = (char)(0x80u | (cp & 0x3Fu)); }}");
    c_writeln!(out, "        else if (pos + 4 < RBCPP_STRING_CAP) {{ out[pos++] = (char)(0xF0u | (cp >> 18)); out[pos++] = (char)(0x80u | ((cp >> 12) & 0x3Fu)); out[pos++] = (char)(0x80u | ((cp >> 6) & 0x3Fu)); out[pos++] = (char)(0x80u | (cp & 0x3Fu)); }}");
    c_writeln!(out, "    }}");
    c_writeln!(out, "    out[pos] = '\\0';");
    c_writeln!(out, "    return out;");
    c_writeln!(out, "}}");
    c_writeln!(out, "static RBCPP_UNUSED size_t rbcpp_utf8_next(const char *s, size_t i) {{ unsigned char c = (unsigned char)s[i]; if (c == 0) {{ return i; }} if ((c & 0x80u) == 0) {{ return i + 1; }} if ((c & 0xE0u) == 0xC0u && (s[i + 1] & 0xC0u) == 0x80u) {{ return i + 2; }} if ((c & 0xF0u) == 0xE0u && (s[i + 1] & 0xC0u) == 0x80u && (s[i + 2] & 0xC0u) == 0x80u) {{ return i + 3; }} if ((c & 0xF8u) == 0xF0u && (s[i + 1] & 0xC0u) == 0x80u && (s[i + 2] & 0xC0u) == 0x80u && (s[i + 3] & 0xC0u) == 0x80u) {{ return i + 4; }} return i + 1; }}");
    c_writeln!(out, "static RBCPP_UNUSED size_t rbcpp_utf8_offset(const char *s, int64_t chars) {{ if (!s || chars <= 0) {{ return 0; }} size_t i = 0; int64_t count = 0; while (s[i] && count < chars) {{ i = rbcpp_utf8_next(s, i); count++; }} return i; }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_utf8_len(const char *s) {{ if (!s) {{ return 0; }} size_t i = 0; int64_t count = 0; while (s[i]) {{ i = rbcpp_utf8_next(s, i); count++; }} return count; }}");
    c_writeln!(out, "static RBCPP_UNUSED int64_t rbcpp_utf8_len_bytes(const char *s, size_t bytes) {{ if (!s) {{ return 0; }} size_t i = 0; int64_t count = 0; while (s[i] && i < bytes) {{ size_t next = rbcpp_utf8_next(s, i); if (next > bytes) {{ break; }} i = next; count++; }} return count; }}");
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_left(const char *s, int64_t len) {{ char *out = rbcpp_strtmp(); if (!s || len <= 0) {{ return out; }} size_t end = rbcpp_utf8_offset(s, len); snprintf(out, RBCPP_STRING_CAP, \"%.*s\", (int)end, s); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_right(const char *s, int64_t len) {{ char *out = rbcpp_strtmp(); if (!s || len <= 0) {{ return out; }} int64_t total = rbcpp_utf8_len(s); int64_t skip = total > len ? total - len : 0; size_t start = rbcpp_utf8_offset(s, skip); snprintf(out, RBCPP_STRING_CAP, \"%s\", s + start); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_mid(const char *s, int64_t len, int64_t pos) {{ char *out = rbcpp_strtmp(); if (!s || len <= 0) {{ return out; }} size_t start = rbcpp_utf8_offset(s, pos <= 1 ? 0 : pos - 1); size_t end = rbcpp_utf8_offset(s, (pos <= 1 ? 0 : pos - 1) + len); snprintf(out, RBCPP_STRING_CAP, \"%.*s\", (int)(end - start), s + start); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_concat2(const char *a, const char *b) {{ char *out = rbcpp_strtmp(); snprintf(out, RBCPP_STRING_CAP, \"%s%s\", a ? a : \"\", b ? b : \"\"); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_delete(const char *s, int64_t len, int64_t pos) {{ char *out = rbcpp_strtmp(); if (!s) {{ return out; }} int64_t start_chars = pos <= 1 ? 0 : pos - 1; size_t start = rbcpp_utf8_offset(s, start_chars); size_t end = rbcpp_utf8_offset(s, start_chars + (len <= 0 ? 0 : len)); snprintf(out, RBCPP_STRING_CAP, \"%.*s%s\", (int)start, s, s + end); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_insert(const char *s, const char *ins, int64_t pos) {{ char *out = rbcpp_strtmp(); if (!s) {{ s = \"\"; }} if (!ins) {{ ins = \"\"; }} size_t start = rbcpp_utf8_offset(s, pos <= 0 ? 0 : pos); snprintf(out, RBCPP_STRING_CAP, \"%.*s%s%s\", (int)start, s, ins, s + start); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_replace(const char *s, const char *rep, int64_t len, int64_t pos) {{ return rbcpp_insert(rbcpp_delete(s, len, pos), rep, pos - 1); }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_find(const char *s, const char *needle) {{ if (!s || !needle) {{ return 0; }} const char *p = strstr(s, needle); return p ? rbcpp_utf8_len_bytes(s, (size_t)(p - s)) + 1 : 0; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED bool rbcpp_string_to_bool(const char *s) {{ if (!s) {{ return false; }} return strcmp(s, \"TRUE\") == 0 || strcmp(s, \"true\") == 0 || strcmp(s, \"1\") == 0; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_string_to_int(const char *s) {{ return s ? (int64_t)strtoll(s, NULL, 10) : 0; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED double rbcpp_string_to_real(const char *s) {{ return s ? strtod(s, NULL) : 0.0; }}"
    );
    out.push_str(
        r#"static RBCPP_UNUSED int64_t rbcpp_string_to_time(const char *s) {
    if (!s) { return 0; }
    const char *p = s;
    if (strncmp(p, "T#", 2) == 0) { p += 2; }
    else if (strncmp(p, "TIME#", 5) == 0) { p += 5; }
    int sign = 1;
    if (*p == '-') { sign = -1; p += 1; }
    double total = 0.0;
    bool saw_unit = false;
    while (*p) {
        char *end = NULL;
        const char *start = p;
        double value = strtod(p, &end);
        if (end == p) { return 0; }
        bool fractional = false;
        for (const char *q = start; q < end; ++q) { if (*q == '.') { fractional = true; break; } }
        p = end;
        if (*p == '\0') { return saw_unit ? 0 : (int64_t)(sign * value); }
        double factor = 0.0;
        if (p[0] == 'm' && p[1] == 's') { factor = 1.0; p += 2; }
        else if (*p == 'd') { factor = 86400000.0; p += 1; }
        else if (*p == 'h') { factor = 3600000.0; p += 1; }
        else if (*p == 'm') { factor = 60000.0; p += 1; }
        else if (*p == 's') { factor = 1000.0; p += 1; }
        else { return 0; }
        if (fractional && *p != '\0') { return 0; }
        total += value * factor;
        saw_unit = true;
    }
    return (int64_t)(sign * total);
}
"#,
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_string_to_date(const char *s) {{ if (!s) {{ return 0; }} const char *p = s; if (strncmp(p, \"D#\", 2) == 0) {{ p += 2; }} else if (strncmp(p, \"DATE#\", 5) == 0) {{ p += 5; }} long long y = 0, m = 0, d = 0; if (sscanf(p, \"%lld-%lld-%lld\", &y, &m, &d) != 3) {{ return 0; }} return rbcpp_concat_date(y, m, d); }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_string_to_tod(const char *s) {{ if (!s) {{ return 0; }} const char *p = s; if (strncmp(p, \"TOD#\", 4) == 0) {{ p += 4; }} else if (strncmp(p, \"TIME_OF_DAY#\", 12) == 0) {{ p += 12; }} long long h = 0, m = 0, sec = 0, ms = 0; int parsed = sscanf(p, \"%lld:%lld:%lld.%lld\", &h, &m, &sec, &ms); if (parsed < 3) {{ parsed = sscanf(p, \"%lld:%lld:%lld\", &h, &m, &sec); ms = 0; }} if (parsed < 3) {{ return 0; }} return rbcpp_concat_tod(h, m, sec, ms); }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED int64_t rbcpp_string_to_dt(const char *s) {{ if (!s) {{ return 0; }} const char *p = s; if (strncmp(p, \"DT#\", 3) == 0) {{ p += 3; }} else if (strncmp(p, \"DATE_AND_TIME#\", 14) == 0) {{ p += 14; }} long long y = 0, mo = 0, d = 0, h = 0, mi = 0, sec = 0, ms = 0; int parsed = sscanf(p, \"%lld-%lld-%lld-%lld:%lld:%lld.%lld\", &y, &mo, &d, &h, &mi, &sec, &ms); if (parsed < 6) {{ parsed = sscanf(p, \"%lld-%lld-%lldT%lld:%lld:%lld.%lld\", &y, &mo, &d, &h, &mi, &sec, &ms); }} if (parsed < 6) {{ parsed = sscanf(p, \"%lld-%lld-%lld-%lld:%lld:%lld\", &y, &mo, &d, &h, &mi, &sec); ms = 0; }} if (parsed < 6) {{ parsed = sscanf(p, \"%lld-%lld-%lldT%lld:%lld:%lld\", &y, &mo, &d, &h, &mi, &sec); ms = 0; }} if (parsed < 6) {{ return 0; }} return rbcpp_concat_dt(y, mo, d, h, mi, sec, ms); }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_bool_to_string(bool v) {{ return v ? \"TRUE\" : \"FALSE\"; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_int_to_string(int64_t v) {{ char *out = rbcpp_strtmp(); snprintf(out, RBCPP_STRING_CAP, \"%lld\", (long long)v); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_real_to_string(double v) {{ char *out = rbcpp_strtmp(); snprintf(out, RBCPP_STRING_CAP, \"%.17g\", v); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_time_to_string(int64_t v) {{ char *out = rbcpp_strtmp(); snprintf(out, RBCPP_STRING_CAP, \"T#%lldms\", (long long)v); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_date_to_string(int64_t v) {{ char *out = rbcpp_strtmp(); rbcpp_datetime_parts p = rbcpp_civil_from_days(v); snprintf(out, RBCPP_STRING_CAP, \"D#%04lld-%02lld-%02lld\", (long long)p.year, (long long)p.month, (long long)p.day); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_tod_to_string(int64_t v) {{ char *out = rbcpp_strtmp(); rbcpp_datetime_parts p = rbcpp_split_tod_parts(v); snprintf(out, RBCPP_STRING_CAP, \"TOD#%02lld:%02lld:%02lld.%03lld\", (long long)p.hour, (long long)p.minute, (long long)p.second, (long long)p.millisecond); return out; }}"
    );
    c_writeln!(
        out,
        "static RBCPP_UNUSED const char *rbcpp_dt_to_string(int64_t v) {{ char *out = rbcpp_strtmp(); rbcpp_datetime_parts p = rbcpp_split_dt_parts(v); snprintf(out, RBCPP_STRING_CAP, \"DT#%04lld-%02lld-%02lld-%02lld:%02lld:%02lld.%03lld\", (long long)p.year, (long long)p.month, (long long)p.day, (long long)p.hour, (long long)p.minute, (long long)p.second, (long long)p.millisecond); return out; }}"
    );
    c_writeln!(out);
}

pub(crate) fn emit_data_type_declarations(out: &mut CEmitter<'_>, project: &Project) {
    for data_type in project.data_types() {
        match &data_type.spec {
            DataTypeSpec::Struct { fields } => {
                c_writeln!(out, "typedef struct {{");
                for field in fields {
                    emit_c_declaration(
                        out,
                        "    ",
                        &sanitize_c_ident(&field.name.original),
                        &field.spec,
                        project,
                    );
                }
                c_writeln!(out, "}} {};", type_c_ident(&data_type.name));
                c_writeln!(out);
            }
            DataTypeSpec::Array { .. } => {
                c_writeln!(out, "typedef struct {{");
                emit_c_declaration(out, "    ", "value", &data_type.spec, project);
                c_writeln!(out, "}} {};", type_c_ident(&data_type.name));
                c_writeln!(out);
            }
            _ => {}
        }
    }
}

pub(crate) fn emit_aggregate_state_declaration(
    out: &mut CEmitter<'_>,
    var: &VarDecl,
    project: &Project,
) -> bool {
    if !is_aggregate_spec(&var.type_spec, project) && !is_string_spec(project, &var.type_spec) {
        return false;
    }
    emit_c_declaration(
        out,
        "    ",
        &sanitize_c_ident(&var.name.original),
        &var.type_spec,
        project,
    );
    true
}

pub(crate) fn emit_c_declaration(
    out: &mut CEmitter<'_>,
    indent: &str,
    name: &str,
    spec: &DataTypeSpec,
    project: &Project,
) {
    let (base, dimensions) = peel_array_dimensions(project, spec);
    let dimensions = dimensions_to_c(&dimensions);

    if let Some(info) = c_text_info(project, &base) {
        let element_type = if info.wide { "uint32_t" } else { "char" };
        c_writeln!(
            out,
            "{indent}{element_type} {name}{dimensions}[{}];",
            info.capacity
        );
        return;
    }

    if let Some(type_ident) = named_struct_type_ident(project, &base) {
        c_writeln!(out, "{indent}{type_ident} {name}{dimensions};");
        return;
    }

    match resolve_named_spec(project, &base) {
        DataTypeSpec::Struct { fields } => {
            c_writeln!(out, "{indent}struct {{");
            let nested_indent = format!("{indent}    ");
            for field in fields {
                emit_c_declaration(
                    out,
                    &nested_indent,
                    &sanitize_c_ident(&field.name.original),
                    &field.spec,
                    project,
                );
            }
            c_writeln!(out, "{indent}}} {name}{dimensions};");
        }
        resolved => {
            c_writeln!(
                out,
                "{indent}{} {name}{dimensions};",
                c_storage_type(project, &resolved)
            );
        }
    }
}

pub(crate) fn emit_debug_metadata(
    out: &mut CEmitter<'_>,
    program: &Pou,
    project: &Project,
    state_type: &str,
) {
    c_writeln!(out);
    c_writeln!(
        out,
        "typedef struct {{ const char *name; const char *type; const char *storage; const char *c_type; const char *location; size_t elements; }} rbcpp_debug_symbol;"
    );
    c_writeln!(
        out,
        "static const rbcpp_debug_symbol rbcpp_debug_symbols[] RBCPP_UNUSED = {{"
    );
    for (var, retain) in program_vars_with_retain(program, project) {
        c_writeln!(
            out,
            "    {{\"{}\", \"{}\", \"{}\", \"{}\", \"{}\", {}}},",
            c_string_escape(&var.name.original),
            c_string_escape(&debug_type_name(project, &var.type_spec)),
            match retain {
                Some(RetainKind::Retain) => "retain",
                Some(RetainKind::NonRetain) => "non_retain",
                None => "plain",
            },
            c_string_escape(&debug_c_type_name(project, &var.type_spec)),
            c_string_escape(var.location.as_deref().unwrap_or("")),
            debug_element_count(project, &var.type_spec)
        );
    }
    if let Some(sfc) = &program.body.sfc {
        for step in &sfc.steps {
            c_writeln!(
                out,
                "    {{\"{}\", \"SFC_STEP\", \"sfc\", \"bool\", \"\", 1}},",
                c_string_escape(&sfc_step_field(&step.name))
            );
        }
        for control in sfc_action_controls(sfc) {
            c_writeln!(
                out,
                "    {{\"{}\", \"SFC_ACTION_{}\", \"sfc\", \"bool\", \"\", 1}},",
                c_string_escape(&sfc_action_field_from_key(&control.key)),
                c_string_escape(&sfc_action_control_qualifier_label(&control))
            );
        }
    }
    c_writeln!(out, "}};");
    c_writeln!(
        out,
        "static const size_t rbcpp_debug_symbol_count RBCPP_UNUSED = sizeof(rbcpp_debug_symbols) / sizeof(rbcpp_debug_symbols[0]);"
    );
    emit_io_metadata(out, program, project, state_type);
    emit_access_path_metadata(out, program, project);
}

pub(crate) fn emit_io_metadata(
    out: &mut CEmitter<'_>,
    program: &Pou,
    project: &Project,
    state_type: &str,
) {
    c_writeln!(out);
    c_writeln!(
        out,
        "static const rbcpp_io_symbol rbcpp_io_symbols[] RBCPP_UNUSED = {{"
    );
    for entry in io_entries_for_program(program, project) {
        c_writeln!(
            out,
            "    {{\"{}\", \"{}\", {}, \"{}\", \"{}\", sizeof({})}},",
            c_string_escape(&entry.name),
            c_string_escape(&entry.location),
            io_direction_c(entry.direction),
            c_string_escape(&debug_type_name(project, &entry.type_spec)),
            c_string_escape(&debug_c_type_name(project, &entry.type_spec)),
            state_field_size_expr(state_type, &entry.state_field),
        );
    }
    c_writeln!(out, "}};");
    c_writeln!(
        out,
        "static const size_t rbcpp_io_symbol_count RBCPP_UNUSED = sizeof(rbcpp_io_symbols) / sizeof(rbcpp_io_symbols[0]);"
    );
}

pub(crate) fn emit_access_path_metadata(out: &mut CEmitter<'_>, program: &Pou, project: &Project) {
    c_writeln!(out);
    c_writeln!(
        out,
        "typedef struct {{ const char *name; const char *target; const char *direction; const char *type; }} rbcpp_access_path_symbol;"
    );
    c_writeln!(
        out,
        "static const rbcpp_access_path_symbol rbcpp_access_paths[] RBCPP_UNUSED = {{"
    );
    for block in program
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Access)
    {
        for var in &block.vars {
            let Some(access) = &var.access else {
                continue;
            };
            c_writeln!(
                out,
                "    {{\"{}\", \"{}\", \"{}\", \"{}\"}},",
                c_string_escape(&var.name.original),
                c_string_escape(access.path.trim()),
                access_direction_name(access.direction),
                c_string_escape(&debug_type_name(project, &var.type_spec))
            );
        }
    }
    c_writeln!(out, "}};");
    c_writeln!(
        out,
        "static const size_t rbcpp_access_path_count RBCPP_UNUSED = sizeof(rbcpp_access_paths) / sizeof(rbcpp_access_paths[0]);"
    );
}

pub(crate) fn access_direction_name(direction: AccessDirection) -> &'static str {
    match direction {
        AccessDirection::ReadOnly => "READ_ONLY",
        AccessDirection::ReadWrite => "READ_WRITE",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IoDirection {
    Input,
    Output,
    Memory,
    Unknown,
}

#[derive(Debug, Clone)]
pub(crate) struct IoEntry {
    pub(crate) name: String,
    pub(crate) location: String,
    pub(crate) direction: IoDirection,
    pub(crate) type_spec: DataTypeSpec,
    pub(crate) state_field: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DirectVariableInfo {
    pub(crate) location: String,
    pub(crate) type_spec: DataTypeSpec,
}

#[derive(Debug, Clone)]
pub(crate) struct StateStorageEntry {
    pub(crate) name: String,
    pub(crate) state_field: String,
    pub(crate) type_spec: DataTypeSpec,
}

pub(crate) fn io_entries_for_program(program: &Pou, project: &Project) -> Vec<IoEntry> {
    let mut entries = Vec::new();
    for (var, _) in program_vars_with_retain(program, project) {
        let Some(location) = var.location.as_ref() else {
            continue;
        };
        entries.push(IoEntry {
            name: var.name.original.clone(),
            location: location.clone(),
            direction: io_direction_for_location(location),
            type_spec: var.type_spec.clone(),
            state_field: var_storage_root(&var.name.original, &var.type_spec, project),
        });
    }
    for direct in direct_variable_refs_for_program(program) {
        entries.push(IoEntry {
            name: direct.location.clone(),
            location: direct.location.clone(),
            direction: io_direction_for_location(&direct.location),
            type_spec: direct.type_spec.clone(),
            state_field: sanitize_c_ident(&direct.location),
        });
    }
    entries
}

pub(crate) fn io_direction_for_location(location: &str) -> IoDirection {
    let location = location.trim_start_matches('%').to_ascii_uppercase();
    match location.chars().next() {
        Some('I') => IoDirection::Input,
        Some('Q') => IoDirection::Output,
        Some('M') => IoDirection::Memory,
        _ => IoDirection::Unknown,
    }
}

pub(crate) fn io_direction_c(direction: IoDirection) -> &'static str {
    match direction {
        IoDirection::Input => "RBCPP_IO_INPUT",
        IoDirection::Output => "RBCPP_IO_OUTPUT",
        IoDirection::Memory => "RBCPP_IO_MEMORY",
        IoDirection::Unknown => "RBCPP_IO_UNKNOWN",
    }
}

pub(crate) fn should_read_io(direction: IoDirection) -> bool {
    matches!(
        direction,
        IoDirection::Input | IoDirection::Memory | IoDirection::Unknown
    )
}

pub(crate) fn should_write_io(direction: IoDirection) -> bool {
    matches!(direction, IoDirection::Output | IoDirection::Memory)
}

pub(crate) fn var_storage_root(name: &str, _spec: &DataTypeSpec, _project: &Project) -> String {
    sanitize_c_ident(name)
}

pub(crate) fn emit_retain_functions(
    out: &mut CEmitter<'_>,
    program: &Pou,
    project: &Project,
    state_type: &str,
) {
    let program_ident = sanitize_c_ident(&program.name.original);
    let entries = retain_entries_for_program(program, project);

    c_writeln!(out);
    c_writeln!(
        out,
        "void {program_ident}_load_retained({state_type} *s) {{"
    );
    c_writeln!(out, "    if (!s->rbcpp_target.retain_load) {{ return; }}");
    for entry in &entries {
        c_writeln!(
            out,
            "    s->rbcpp_target.retain_load(s->rbcpp_target_ctx, \"{}\", {}, {});",
            c_string_escape(&entry.name),
            state_storage_pointer(entry, project),
            state_field_size_expr(state_type, &entry.state_field),
        );
    }
    c_writeln!(out, "}}");

    c_writeln!(out);
    c_writeln!(
        out,
        "void {program_ident}_save_retained({state_type} *s) {{"
    );
    c_writeln!(out, "    if (!s->rbcpp_target.retain_save) {{ return; }}");
    for entry in &entries {
        c_writeln!(
            out,
            "    s->rbcpp_target.retain_save(s->rbcpp_target_ctx, \"{}\", {}, {});",
            c_string_escape(&entry.name),
            state_storage_pointer(entry, project),
            state_field_size_expr(state_type, &entry.state_field),
        );
    }
    c_writeln!(out, "}}");
}

pub(crate) fn emit_target_sync_functions(
    out: &mut CEmitter<'_>,
    program: &Pou,
    project: &Project,
    state_type: &str,
) {
    let program_ident = sanitize_c_ident(&program.name.original);
    let entries = io_entries_for_program(program, project);

    c_writeln!(out);
    c_writeln!(
        out,
        "static void {program_ident}_sync_inputs({state_type} *s) {{"
    );
    c_writeln!(out, "    if (!s->rbcpp_target.io_read) {{ return; }}");
    for (index, entry) in entries.iter().enumerate() {
        if !should_read_io(entry.direction) {
            continue;
        }
        c_writeln!(
            out,
            "    s->rbcpp_target.io_read(s->rbcpp_target_ctx, &rbcpp_io_symbols[{index}], {}, {});",
            io_storage_pointer(entry, project),
            state_field_size_expr(state_type, &entry.state_field),
        );
    }
    c_writeln!(out, "}}");

    c_writeln!(out);
    c_writeln!(
        out,
        "static void {program_ident}_sync_outputs({state_type} *s) {{"
    );
    c_writeln!(out, "    if (!s->rbcpp_target.io_write) {{ return; }}");
    for (index, entry) in entries.iter().enumerate() {
        if !should_write_io(entry.direction) {
            continue;
        }
        c_writeln!(
            out,
            "    s->rbcpp_target.io_write(s->rbcpp_target_ctx, &rbcpp_io_symbols[{index}], {}, {});",
            io_storage_pointer(entry, project),
            state_field_size_expr(state_type, &entry.state_field),
        );
    }
    c_writeln!(out, "}}");
}

pub(crate) fn emit_access_path_services(
    out: &mut CEmitter<'_>,
    program: &Pou,
    project: &Project,
    state_type: &str,
) {
    let program_ident = sanitize_c_ident(&program.name.original);
    let entries = access_service_entries_for_program(program, project);

    c_writeln!(out);
    c_writeln!(
        out,
        "bool {program_ident}_read_access_path({state_type} *s, const char *name, void *value, size_t size) {{"
    );
    c_writeln!(out, "    if (!name || !value) {{ return false; }}");
    for entry in &entries {
        c_writeln!(
            out,
            "    if (strcmp(name, \"{}\") == 0) {{",
            c_string_escape(&entry.storage.name)
        );
        c_writeln!(
            out,
            "        if (size < {}) {{ return false; }}",
            state_field_size_expr(state_type, &entry.storage.state_field)
        );
        c_writeln!(
            out,
            "        memcpy(value, {}, {});",
            state_storage_pointer(&entry.storage, project),
            state_field_size_expr(state_type, &entry.storage.state_field)
        );
        c_writeln!(out, "        return true;");
        c_writeln!(out, "    }}");
    }
    c_writeln!(out, "    return false;");
    c_writeln!(out, "}}");

    c_writeln!(out);
    c_writeln!(
        out,
        "bool {program_ident}_write_access_path({state_type} *s, const char *name, const void *value, size_t size) {{"
    );
    c_writeln!(out, "    if (!name || !value) {{ return false; }}");
    for entry in &entries {
        c_writeln!(
            out,
            "    if (strcmp(name, \"{}\") == 0) {{",
            c_string_escape(&entry.storage.name)
        );
        if entry.direction != AccessDirection::ReadWrite {
            c_writeln!(out, "        return false;");
            c_writeln!(out, "    }}");
            continue;
        }
        c_writeln!(
            out,
            "        if (size < {}) {{ return false; }}",
            state_field_size_expr(state_type, &entry.storage.state_field)
        );
        c_writeln!(
            out,
            "        memcpy({}, value, {});",
            state_storage_pointer(&entry.storage, project),
            state_field_size_expr(state_type, &entry.storage.state_field)
        );
        c_writeln!(out, "        return true;");
        c_writeln!(out, "    }}");
    }
    c_writeln!(out, "    return false;");
    c_writeln!(out, "}}");
}

#[derive(Debug, Clone)]
pub(crate) struct AccessServiceEntry {
    pub(crate) direction: AccessDirection,
    pub(crate) storage: StateStorageEntry,
}

pub(crate) fn access_service_entries_for_program(
    program: &Pou,
    project: &Project,
) -> Vec<AccessServiceEntry> {
    program
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Access)
        .flat_map(|block| block.vars.iter())
        .filter_map(|var| {
            let access = var.access.as_ref()?;
            let storage = access_path_storage_entry(
                program,
                project,
                &var.name.original,
                &access.path,
                &var.type_spec,
            )?;
            Some(AccessServiceEntry {
                direction: access.direction,
                storage,
            })
        })
        .collect()
}

pub(crate) fn access_path_storage_entry(
    program: &Pou,
    project: &Project,
    access_name: &str,
    path: &str,
    access_type: &DataTypeSpec,
) -> Option<StateStorageEntry> {
    let trimmed_path = path.trim();
    if trimmed_path.starts_with('%') {
        return Some(StateStorageEntry {
            name: access_name.to_string(),
            state_field: sanitize_c_ident(trimmed_path),
            type_spec: access_type.clone(),
        });
    }

    let mut parts = path
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let root_name = parts.next()?;
    let root_ident = Identifier::new(root_name);
    let root = program_vars_with_retain(program, project)
        .into_iter()
        .map(|(var, _)| var)
        .find(|var| var.name.canonical == root_ident.canonical)?;
    let remaining = parts.collect::<Vec<_>>();
    if remaining.is_empty() {
        return Some(StateStorageEntry {
            name: access_name.to_string(),
            state_field: var_storage_root(&root.name.original, &root.type_spec, project),
            type_spec: root.type_spec.clone(),
        });
    }

    let field_name = remaining.join(".");
    if let Some(fields) = standard_fb_fields(&root.type_spec) {
        if remaining.len() != 1 {
            return None;
        }
        let field_ident = Identifier::new(remaining[0]);
        let field = fields
            .iter()
            .find(|field| Identifier::new(field.name).canonical == field_ident.canonical)?;
        return Some(StateStorageEntry {
            name: access_name.to_string(),
            state_field: fb_field_ident(&root.name.original, field.name),
            type_spec: fb_field_data_type(field),
        });
    }

    if let Some(function_block) = user_function_block(project, &root.type_spec) {
        if remaining.len() != 1 {
            return None;
        }
        let field_ident = Identifier::new(remaining[0]);
        let field = function_block
            .variable_declarations()
            .find(|field| field.name.canonical == field_ident.canonical)?;
        return Some(StateStorageEntry {
            name: access_name.to_string(),
            state_field: fb_field_ident(&root.name.original, &field.name.original),
            type_spec: field.type_spec.clone(),
        });
    }

    let mut state_field = sanitize_c_ident(&root.name.original);
    let mut type_spec = root.type_spec.clone();
    for field in remaining {
        let DataTypeSpec::Struct { fields } = resolve_named_spec(project, &type_spec) else {
            return None;
        };
        let field_ident = Identifier::new(field);
        let field = fields
            .iter()
            .find(|candidate| candidate.name.canonical == field_ident.canonical)?;
        state_field.push('.');
        state_field.push_str(&sanitize_c_ident(&field.name.original));
        type_spec = field.spec.clone();
    }

    Some(StateStorageEntry {
        name: access_name.to_string(),
        state_field: if field_name.is_empty() {
            sanitize_c_ident(&root.name.original)
        } else {
            state_field
        },
        type_spec,
    })
}

pub(crate) fn retain_entries_for_program(
    program: &Pou,
    project: &Project,
) -> Vec<StateStorageEntry> {
    program_vars_with_retain(program, project)
        .into_iter()
        .filter(|(_, retain)| *retain == Some(RetainKind::Retain))
        .flat_map(|(var, _)| state_storage_entries_for_var(var, project))
        .collect()
}

pub(crate) fn state_storage_entries_for_var(
    var: &VarDecl,
    project: &Project,
) -> Vec<StateStorageEntry> {
    if let Some(fields) = standard_fb_fields(&var.type_spec) {
        return fields
            .iter()
            .map(|field| StateStorageEntry {
                name: format!("{}.{}", var.name.original, field.name),
                state_field: fb_field_ident(&var.name.original, field.name),
                type_spec: fb_field_data_type(field),
            })
            .collect();
    }

    if let Some(function_block) = user_function_block(project, &var.type_spec) {
        return function_block
            .variable_declarations()
            .map(|field| StateStorageEntry {
                name: format!("{}.{}", var.name.original, field.name.original),
                state_field: fb_field_ident(&var.name.original, &field.name.original),
                type_spec: field.type_spec.clone(),
            })
            .collect();
    }

    vec![StateStorageEntry {
        name: var.name.original.clone(),
        state_field: var_storage_root(&var.name.original, &var.type_spec, project),
        type_spec: var.type_spec.clone(),
    }]
}

pub(crate) fn fb_field_data_type(field: &FbField) -> DataTypeSpec {
    match field.c_type {
        "bool" => DataTypeSpec::Elementary(ElementaryType::Bool),
        _ => DataTypeSpec::Elementary(ElementaryType::Int),
    }
}

pub(crate) fn io_storage_pointer(entry: &IoEntry, project: &Project) -> String {
    storage_pointer(&entry.state_field, &entry.type_spec, project)
}

pub(crate) fn state_storage_pointer(entry: &StateStorageEntry, project: &Project) -> String {
    storage_pointer(&entry.state_field, &entry.type_spec, project)
}

pub(crate) fn storage_pointer(state_field: &str, spec: &DataTypeSpec, project: &Project) -> String {
    let value = format!("s->{state_field}");
    if storage_decays_to_pointer(spec, project) {
        value
    } else {
        format!("&{value}")
    }
}

pub(crate) fn storage_decays_to_pointer(spec: &DataTypeSpec, project: &Project) -> bool {
    is_string_spec(project, spec)
        || matches!(
            resolve_named_spec(project, spec),
            DataTypeSpec::Array { .. }
        )
}

pub(crate) fn state_field_size_expr(state_type: &str, state_field: &str) -> String {
    format!("sizeof((({state_type} *)0)->{state_field})")
}

pub(crate) fn direct_variable_refs_for_program(program: &Pou) -> Vec<DirectVariableInfo> {
    let mut refs = std::collections::BTreeMap::<String, DirectVariableInfo>::new();
    collect_direct_variable_refs_in_access_paths(program, &mut refs);
    for statement in &program.body.statements {
        collect_direct_variable_refs_in_statement(statement, &mut refs);
    }
    if let Some(sfc) = &program.body.sfc {
        for transition in &sfc.transitions {
            if let Some(condition) = &transition.condition {
                collect_direct_variable_refs_in_expr(condition, &mut refs);
            }
        }
        for action in &sfc.actions {
            for statement in &action.body {
                collect_direct_variable_refs_in_statement(statement, &mut refs);
            }
        }
    }
    refs.into_values().collect()
}

pub(crate) fn collect_direct_variable_refs_in_access_paths(
    program: &Pou,
    refs: &mut std::collections::BTreeMap<String, DirectVariableInfo>,
) {
    for var in program
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Access)
        .flat_map(|block| block.vars.iter())
    {
        let Some(access) = &var.access else {
            continue;
        };
        let location = access.path.trim();
        if location.starts_with('%') {
            refs.entry(location.to_string())
                .or_insert_with(|| DirectVariableInfo {
                    location: location.to_string(),
                    type_spec: var.type_spec.clone(),
                });
        }
    }
}

pub(crate) fn collect_direct_variable_refs_in_statement(
    statement: &Statement,
    refs: &mut std::collections::BTreeMap<String, DirectVariableInfo>,
) {
    match statement {
        Statement::Assignment { target, value } => {
            collect_direct_variable_refs_in_variable(target, refs);
            collect_direct_variable_refs_in_expr(value, refs);
        }
        Statement::FbCall { name, args } => {
            collect_direct_variable_refs_in_variable(name, refs);
            collect_direct_variable_refs_in_args(args, refs);
        }
        Statement::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                collect_direct_variable_refs_in_expr(condition, refs);
                for statement in body {
                    collect_direct_variable_refs_in_statement(statement, refs);
                }
            }
            for statement in else_branch {
                collect_direct_variable_refs_in_statement(statement, refs);
            }
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => {
            collect_direct_variable_refs_in_expr(selector, refs);
            for (labels, body) in cases {
                for label in labels {
                    match label {
                        CaseLabel::Single(expr) => collect_direct_variable_refs_in_expr(expr, refs),
                        CaseLabel::Range(low, high) => {
                            collect_direct_variable_refs_in_expr(low, refs);
                            collect_direct_variable_refs_in_expr(high, refs);
                        }
                    }
                }
                for statement in body {
                    collect_direct_variable_refs_in_statement(statement, refs);
                }
            }
            for statement in else_branch {
                collect_direct_variable_refs_in_statement(statement, refs);
            }
        }
        Statement::For {
            from, to, by, body, ..
        } => {
            collect_direct_variable_refs_in_expr(from, refs);
            collect_direct_variable_refs_in_expr(to, refs);
            if let Some(by) = by {
                collect_direct_variable_refs_in_expr(by, refs);
            }
            for statement in body {
                collect_direct_variable_refs_in_statement(statement, refs);
            }
        }
        Statement::While { condition, body } => {
            collect_direct_variable_refs_in_expr(condition, refs);
            for statement in body {
                collect_direct_variable_refs_in_statement(statement, refs);
            }
        }
        Statement::Repeat { body, until } => {
            for statement in body {
                collect_direct_variable_refs_in_statement(statement, refs);
            }
            collect_direct_variable_refs_in_expr(until, refs);
        }
        Statement::Il { operand, .. } => {
            if let Some(operand) = operand {
                collect_direct_variable_refs_in_expr(operand, refs);
            }
        }
        Statement::Empty
        | Statement::IlLabel(_)
        | Statement::Exit
        | Statement::Return
        | Statement::Unsupported(_) => {}
    }
}

pub(crate) fn collect_direct_variable_refs_in_args(
    args: &[ParamAssignment],
    refs: &mut std::collections::BTreeMap<String, DirectVariableInfo>,
) {
    for arg in args {
        if let Some(expr) = &arg.expr {
            collect_direct_variable_refs_in_expr(expr, refs);
        }
        if let Some(variable) = &arg.variable {
            collect_direct_variable_refs_in_variable(variable, refs);
        }
    }
}

pub(crate) fn collect_direct_variable_refs_in_expr(
    expr: &Expr,
    refs: &mut std::collections::BTreeMap<String, DirectVariableInfo>,
) {
    match expr {
        Expr::Variable(variable) => collect_direct_variable_refs_in_variable(variable, refs),
        Expr::Unary { expr, .. } => collect_direct_variable_refs_in_expr(expr, refs),
        Expr::Binary { left, right, .. } => {
            collect_direct_variable_refs_in_expr(left, refs);
            collect_direct_variable_refs_in_expr(right, refs);
        }
        Expr::Call { args, .. } => collect_direct_variable_refs_in_args(args, refs),
        Expr::ArrayLiteral(elements) => {
            for element in elements {
                collect_direct_variable_refs_in_expr(element, refs);
            }
        }
        Expr::StructLiteral(args) => collect_direct_variable_refs_in_args(args, refs),
        Expr::Literal(_) => {}
    }
}

pub(crate) fn collect_direct_variable_refs_in_variable(
    variable: &VariableRef,
    refs: &mut std::collections::BTreeMap<String, DirectVariableInfo>,
) {
    if let Some(location) = &variable.direct {
        refs.entry(location.clone())
            .or_insert_with(|| DirectVariableInfo {
                location: location.clone(),
                type_spec: direct_variable_type_spec(location),
            });
    }
    for indices in &variable.indices {
        for index in indices {
            collect_direct_variable_refs_in_expr(index, refs);
        }
    }
}

pub(crate) fn direct_variable_type_spec(location: &str) -> DataTypeSpec {
    let upper = location.trim_start_matches('%').to_ascii_uppercase();
    let mut chars = upper.chars();
    let _area = chars.next();
    match chars.next() {
        Some('X') => DataTypeSpec::Elementary(ElementaryType::Bool),
        Some('B') => DataTypeSpec::Elementary(ElementaryType::Byte),
        Some('W') => DataTypeSpec::Elementary(ElementaryType::Word),
        Some('D') => DataTypeSpec::Elementary(ElementaryType::Dword),
        Some('L') => DataTypeSpec::Elementary(ElementaryType::Lword),
        _ => DataTypeSpec::Elementary(ElementaryType::Int),
    }
}

pub(crate) fn debug_type_name(project: &Project, spec: &DataTypeSpec) -> String {
    match resolve_named_spec(project, spec) {
        DataTypeSpec::Elementary(elementary) => elementary.as_iec().to_string(),
        DataTypeSpec::Named(name) => name.original,
        DataTypeSpec::Array {
            ranges,
            element_type,
        } => {
            let ranges = ranges
                .iter()
                .map(|range| format!("{}..{}", range.low, range.high))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "ARRAY[{ranges}] OF {}",
                debug_type_name(project, &element_type)
            )
        }
        DataTypeSpec::Struct { fields } => {
            let fields = fields
                .iter()
                .map(|field| {
                    format!(
                        "{}:{}",
                        field.name.original,
                        debug_type_name(project, &field.spec)
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            format!("STRUCT{{{fields}}}")
        }
        DataTypeSpec::Enum { .. } => "ENUM".to_string(),
        DataTypeSpec::Subrange { base, range } => {
            format!("{}({}..{})", base.as_iec(), range.low, range.high)
        }
        DataTypeSpec::String { wide, length } => {
            let prefix = if wide { "WSTRING" } else { "STRING" };
            length
                .map(|length| format!("{prefix}[{length}]"))
                .unwrap_or_else(|| prefix.to_string())
        }
    }
}

pub(crate) fn debug_c_type_name(project: &Project, spec: &DataTypeSpec) -> String {
    if let Some(type_ident) = named_struct_type_ident(project, spec) {
        return type_ident;
    }
    let resolved = resolve_named_spec(project, spec);
    if let Some(info) = c_text_info(project, &resolved) {
        let element_type = if info.wide { "uint32_t" } else { "char" };
        return format!("{element_type}[{}]", info.capacity);
    }
    if matches!(resolved, DataTypeSpec::Array { .. }) {
        let (element, dimensions) = peel_array_dimensions(project, &resolved);
        return format!(
            "{}{}",
            c_storage_type(project, &element),
            dimensions_to_c(&dimensions)
        );
    }
    c_storage_type(project, &resolved).to_string()
}

pub(crate) fn debug_element_count(project: &Project, spec: &DataTypeSpec) -> usize {
    match resolve_named_spec(project, spec) {
        DataTypeSpec::Array { ranges, .. } => array_element_count(&ranges),
        DataTypeSpec::String { length, .. } => length.unwrap_or(RBCPP_DEFAULT_STRING_CAP),
        DataTypeSpec::Elementary(ElementaryType::String | ElementaryType::WString) => {
            RBCPP_DEFAULT_STRING_CAP
        }
        _ => 1,
    }
}

pub(crate) fn c_string_escape(input: &str) -> String {
    let mut escaped = String::new();
    for ch in input.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{000C}' => escaped.push_str("\\f"),
            ch if ch == ' ' || ch.is_ascii_graphic() => escaped.push(ch),
            ch => {
                let mut buffer = [0_u8; 4];
                for byte in ch.encode_utf8(&mut buffer).as_bytes() {
                    c_write!(escaped, "\\{:03o}", byte);
                }
            }
        }
    }
    escaped
}

pub(crate) fn emit_aggregate_state_initialization(
    out: &mut CEmitter<'_>,
    var_name: &str,
    spec: &DataTypeSpec,
    initial: Option<&Expr>,
    project: &Project,
) -> bool {
    if !is_aggregate_spec(spec, project) && !is_string_spec(project, spec) {
        return false;
    }
    let target = format!("s->{}", sanitize_c_ident(var_name));
    emit_initializer(out, &target, spec, initial, project);
    true
}

pub(crate) fn emit_initializer(
    out: &mut CEmitter<'_>,
    target: &str,
    spec: &DataTypeSpec,
    initial: Option<&Expr>,
    project: &Project,
) {
    match resolve_named_spec(project, spec) {
        DataTypeSpec::Elementary(ElementaryType::String | ElementaryType::WString)
        | DataTypeSpec::String { .. } => {
            let value = initial
                .map(|expr| initializer_expr_to_c(expr, spec, project))
                .unwrap_or_else(|| "\"\"".to_string());
            let info = c_text_info(project, spec).unwrap_or(CTextInfo {
                wide: false,
                capacity: RBCPP_DEFAULT_STRING_CAP,
            });
            let assign = if info.wide {
                "rbcpp_wstrassign_utf8"
            } else {
                "rbcpp_strassign"
            };
            c_writeln!(out, "    {assign}({target}, {}, {value});", info.capacity);
        }
        DataTypeSpec::Array {
            ranges,
            element_type,
        } => {
            let elements = match initial {
                Some(Expr::ArrayLiteral(elements)) => elements.as_slice(),
                _ => &[],
            };
            let total = array_element_count(&ranges);
            for offset in 0..total {
                let indexed_target = format!("{target}{}", c_zero_based_indices(offset, &ranges));
                emit_initializer(
                    out,
                    &indexed_target,
                    &element_type,
                    elements.get(offset),
                    project,
                );
            }
        }
        DataTypeSpec::Struct { fields } => {
            let initializers = match initial {
                Some(Expr::StructLiteral(initializers)) => initializers.as_slice(),
                _ => &[],
            };
            for field in fields {
                let initializer = initializers
                    .iter()
                    .find(|initializer| {
                        initializer
                            .name
                            .as_ref()
                            .is_some_and(|name| name.canonical == field.name.canonical)
                    })
                    .and_then(|initializer| initializer.expr.as_ref())
                    .or(field.initial_value.as_ref());
                let field_target = format!("{target}.{}", sanitize_c_ident(&field.name.original));
                emit_initializer(out, &field_target, &field.spec, initializer, project);
            }
        }
        DataTypeSpec::Enum { .. } => {
            let value = initial
                .map(|expr| initializer_expr_to_c(expr, spec, project))
                .unwrap_or_else(|| "0".to_string());
            c_writeln!(out, "    {target} = {value};");
        }
        DataTypeSpec::Subrange { range, .. } => {
            let value = initial
                .map(|expr| initializer_expr_to_c(expr, spec, project))
                .unwrap_or_else(|| {
                    if range.low <= 0 && range.high >= 0 {
                        "0".to_string()
                    } else {
                        range.low.to_string()
                    }
                });
            c_writeln!(out, "    {target} = {value};");
        }
        resolved => {
            let value = initial
                .map(|expr| initializer_expr_to_c(expr, &resolved, project))
                .unwrap_or_else(|| c_default(&resolved).to_string());
            c_writeln!(out, "    {target} = {value};");
        }
    }
}

pub(crate) fn emit_local_initializer(
    out: &mut CEmitter<'_>,
    indent: usize,
    target: &str,
    spec: &DataTypeSpec,
    initial: Option<&Expr>,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> bool {
    let pad = "    ".repeat(indent);
    if let Some(info) = c_text_info(project, spec) {
        let value = initial
            .map(|expr| initializer_expr_to_c_local_typed(expr, spec, var_types, project))
            .unwrap_or_else(|| "\"\"".to_string());
        let assign = if info.wide {
            "rbcpp_wstrassign_utf8"
        } else {
            "rbcpp_strassign"
        };
        c_writeln!(out, "{pad}{assign}({target}, {}, {value});", info.capacity);
        return true;
    }

    if let Some(value) = struct_compound_literal_to_c_local(project, spec, initial, var_types) {
        c_writeln!(out, "{pad}{target} = {value};");
        return true;
    }

    if let DataTypeSpec::Array {
        ranges,
        element_type,
    } = resolve_named_spec(project, spec)
    {
        let elements = match initial {
            Some(Expr::ArrayLiteral(elements)) => elements.as_slice(),
            _ => &[],
        };
        let total = array_element_count(&ranges);
        for offset in 0..total {
            let indexed_target = format!("{target}{}", c_zero_based_indices(offset, &ranges));
            if !emit_local_initializer(
                out,
                indent,
                &indexed_target,
                &element_type,
                elements.get(offset),
                var_types,
                project,
            ) {
                let value = elements
                    .get(offset)
                    .map(|expr| {
                        initializer_expr_to_c_local_typed(expr, &element_type, var_types, project)
                    })
                    .unwrap_or_else(|| default_expr_to_c(project, &element_type));
                c_writeln!(out, "{pad}{indexed_target} = {value};");
            }
        }
        return true;
    }

    false
}

pub(crate) fn emit_state_initialization(out: &mut CEmitter<'_>, var: &VarDecl, project: &Project) {
    let initialized =
        emit_function_block_state_initialization(out, &var.name.original, &var.type_spec, project)
            || emit_aggregate_state_initialization(
                out,
                &var.name.original,
                &var.type_spec,
                var.initial_value.as_ref(),
                project,
            );
    if initialized {
        return;
    }
    if let Some(initial) = &var.initial_value {
        c_writeln!(
            out,
            "    s->{} = {};",
            sanitize_c_ident(&var.name.original),
            initializer_expr_to_c(initial, &var.type_spec, project)
        );
    } else {
        c_writeln!(
            out,
            "    s->{} = {};",
            sanitize_c_ident(&var.name.original),
            default_expr_to_c(project, &var.type_spec)
        );
    }
}

pub(crate) fn emit_function_block_state_initialization(
    out: &mut CEmitter<'_>,
    instance: &str,
    spec: &DataTypeSpec,
    project: &Project,
) -> bool {
    if let Some(fields) = standard_fb_fields(spec) {
        for field in fields {
            c_writeln!(
                out,
                "    s->{} = {};",
                fb_field_ident(instance, field.name),
                field.default
            );
        }
        return true;
    }
    let Some(function_block) = user_function_block(project, spec) else {
        return false;
    };
    for field in function_block.variable_declarations() {
        let nested_instance = field_key_for_c(instance, &field.name.original);
        if emit_function_block_state_initialization(
            out,
            &nested_instance,
            &field.type_spec,
            project,
        ) {
            continue;
        }
        if is_aggregate_spec(&field.type_spec, project) || is_string_spec(project, &field.type_spec)
        {
            let target = format!("s->{}", fb_field_ident(instance, &field.name.original));
            emit_initializer(
                out,
                &target,
                &field.type_spec,
                field.initial_value.as_ref(),
                project,
            );
            continue;
        }
        let initial = field
            .initial_value
            .as_ref()
            .map(|expr| initializer_expr_to_c(expr, &field.type_spec, project))
            .unwrap_or_else(|| default_expr_to_c(project, &field.type_spec));
        c_writeln!(
            out,
            "    s->{} = {};",
            fb_field_ident(instance, &field.name.original),
            initial
        );
        if field.edge.is_some() {
            c_writeln!(
                out,
                "    s->{} = false;",
                fb_field_ident(instance, &edge_state_field_name(&field.name.canonical))
            );
        }
    }
    true
}
