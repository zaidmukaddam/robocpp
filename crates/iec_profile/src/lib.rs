// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditionProfile {
    #[default]
    Iec61131_3_2003Strict,
    Iec61131_3_2003PlusExtensions,
    Iec61131_3_2013Placeholder,
    Iec61131_3_2025Placeholder,
}

impl EditionProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            EditionProfile::Iec61131_3_2003Strict => "2003-strict",
            EditionProfile::Iec61131_3_2003PlusExtensions => "2003-plus-extensions",
            EditionProfile::Iec61131_3_2013Placeholder => "2013-placeholder",
            EditionProfile::Iec61131_3_2025Placeholder => "2025-placeholder",
        }
    }

    pub fn is_claimable(self) -> bool {
        matches!(
            self,
            EditionProfile::Iec61131_3_2003Strict | EditionProfile::Iec61131_3_2003PlusExtensions
        )
    }
}

impl fmt::Display for EditionProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EditionProfile {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.trim().to_ascii_lowercase().as_str() {
            "2003" | "2003-strict" | "iec61131-3:2003" => Ok(Self::Iec61131_3_2003Strict),
            "2003-plus" | "2003-plus-extensions" => Ok(Self::Iec61131_3_2003PlusExtensions),
            "2013" | "2013-placeholder" => Ok(Self::Iec61131_3_2013Placeholder),
            "2025" | "2025-placeholder" => Ok(Self::Iec61131_3_2025Placeholder),
            other => Err(format!("unknown IEC 61131-3 profile '{other}'")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImplementationParameters {
    pub max_source_bytes: usize,
    pub max_plcopen_xml_bytes: usize,
    pub max_plcopen_xml_nodes: usize,
    pub max_plcopen_xml_depth: usize,
    pub max_plcopen_xml_text_bytes: usize,
    pub max_plcopen_xml_attribute_bytes: usize,
    pub max_identifier_length: usize,
    pub max_comment_length: usize,
    pub max_expression_depth: usize,
    pub max_statement_depth: usize,
    pub max_array_elements: usize,
    pub max_structure_elements: usize,
    pub max_string_length: usize,
    pub max_pous: usize,
    pub max_variables: usize,
    pub max_symbols: usize,
    pub max_generated_c_bytes: usize,
    pub max_scan_cycles: usize,
    pub pragmas_enabled: bool,
}

impl Default for ImplementationParameters {
    fn default() -> Self {
        Self {
            max_source_bytes: 1_048_576,
            max_plcopen_xml_bytes: 1_048_576,
            max_plcopen_xml_nodes: 150_000,
            max_plcopen_xml_depth: 256,
            max_plcopen_xml_text_bytes: 65_535,
            max_plcopen_xml_attribute_bytes: 65_535,
            max_identifier_length: 128,
            max_comment_length: 1_000_000,
            max_expression_depth: 256,
            max_statement_depth: 256,
            max_array_elements: 1_000_000,
            max_structure_elements: 4096,
            max_string_length: 65_535,
            max_pous: 10_000,
            max_variables: 100_000,
            max_symbols: 150_000,
            max_generated_c_bytes: 1_048_576,
            max_scan_cycles: 10_000,
            pragmas_enabled: false,
        }
    }
}

impl ImplementationParameters {
    pub fn annex_d_report(&self) -> Vec<ImplementationParameter> {
        vec![
            parameter(
                "max_source_bytes",
                "implementation / Annex D",
                "Maximum textual source size",
                self.max_source_bytes.to_string(),
                "bytes",
                "Applied before lexing textual IEC source.",
            ),
            parameter(
                "max_plcopen_xml_bytes",
                "PLCopen / Annex D",
                "Maximum PLCopen XML source size",
                self.max_plcopen_xml_bytes.to_string(),
                "bytes",
                "Applied before importing PLCopen XML.",
            ),
            parameter(
                "max_plcopen_xml_nodes",
                "PLCopen / Annex D",
                "Maximum PLCopen XML node count",
                self.max_plcopen_xml_nodes.to_string(),
                "nodes",
                "Applied by the PLCopen XML parser before project lowering.",
            ),
            parameter(
                "max_plcopen_xml_depth",
                "PLCopen / Annex D",
                "Maximum PLCopen XML nesting depth",
                self.max_plcopen_xml_depth.to_string(),
                "levels",
                "Applied by the PLCopen XML parser before project lowering.",
            ),
            parameter(
                "max_plcopen_xml_text_bytes",
                "PLCopen / Annex D",
                "Maximum PLCopen XML text node size",
                self.max_plcopen_xml_text_bytes.to_string(),
                "bytes",
                "Applied by the PLCopen XML parser before project lowering.",
            ),
            parameter(
                "max_plcopen_xml_attribute_bytes",
                "PLCopen / Annex D",
                "Maximum PLCopen XML attribute value size",
                self.max_plcopen_xml_attribute_bytes.to_string(),
                "bytes",
                "Applied by the PLCopen XML parser before project lowering.",
            ),
            parameter(
                "max_identifier_length",
                "2.1.2 / Annex D",
                "Maximum identifier length",
                self.max_identifier_length.to_string(),
                "characters",
                "Applied by the lexer/parser profile checks.",
            ),
            parameter(
                "max_comment_length",
                "2.1.5 / Annex D",
                "Maximum comment length",
                self.max_comment_length.to_string(),
                "bytes",
                "Applied while lexing framed comments.",
            ),
            parameter(
                "pragmas_enabled",
                "2.1.6 / Annex D",
                "Pragma support",
                self.pragmas_enabled.to_string(),
                "boolean",
                "Disabled in the 2003 strict default profile.",
            ),
            parameter(
                "max_expression_depth",
                "3.3.1 / Annex D",
                "Maximum expression nesting depth",
                self.max_expression_depth.to_string(),
                "levels",
                "Applied by semantic analysis to ST expressions and IL operands.",
            ),
            parameter(
                "max_statement_depth",
                "3.3.2 / Annex D",
                "Maximum statement nesting depth",
                self.max_statement_depth.to_string(),
                "levels",
                "Applied by semantic analysis to nested statement lists.",
            ),
            parameter(
                "max_array_elements",
                "2.3.3.1 / Annex D",
                "Maximum array elements",
                self.max_array_elements.to_string(),
                "elements",
                "Applied to total flattened array element counts.",
            ),
            parameter(
                "max_structure_elements",
                "2.3.3.1 / Annex D",
                "Maximum structure elements",
                self.max_structure_elements.to_string(),
                "fields",
                "Applied to each STRUCT declaration.",
            ),
            parameter(
                "max_string_length",
                "2.2.2 / 2.3.3.1 / Annex D",
                "Maximum string length",
                self.max_string_length.to_string(),
                "characters",
                "Applied to STRING and WSTRING length declarations.",
            ),
            parameter(
                "max_pous",
                "2.5 / Annex D",
                "Maximum POU count",
                self.max_pous.to_string(),
                "POUs",
                "Applied by semantic analysis to project library elements.",
            ),
            parameter(
                "max_variables",
                "2.4 / Annex D",
                "Maximum variable declaration count",
                self.max_variables.to_string(),
                "variables",
                "Applied by semantic analysis across POUs and configurations.",
            ),
            parameter(
                "max_symbols",
                "implementation / Annex D",
                "Maximum named symbol count",
                self.max_symbols.to_string(),
                "symbols",
                "Applied by semantic analysis across library elements and variables.",
            ),
            parameter(
                "max_generated_c_bytes",
                "backend / Annex D",
                "Maximum generated C output size",
                self.max_generated_c_bytes.to_string(),
                "bytes",
                "Applied by the generated C backend.",
            ),
            parameter(
                "max_scan_cycles",
                "runtime / Annex D",
                "Maximum simulator scan cycles",
                self.max_scan_cycles.to_string(),
                "cycles",
                "Applied by the deterministic interpreter runtime.",
            ),
        ]
    }

    pub fn annex_d_markdown(&self) -> String {
        let mut out = "# RoboC++ Implementation-Dependent Parameters\n\n| ID | Clause | Parameter | Value | Unit | Notes |\n| --- | --- | --- | --- | --- | --- |\n".to_string();
        for parameter in self.annex_d_report() {
            out.push_str(&format!(
                "| `{}` | {} | {} | `{}` | {} | {} |\n",
                parameter.id,
                parameter.clause,
                parameter.title,
                parameter.value,
                parameter.unit,
                parameter.notes
            ));
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationParameter {
    pub id: &'static str,
    pub clause: &'static str,
    pub title: &'static str,
    pub value: String,
    pub unit: &'static str,
    pub notes: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureStatus {
    Implemented,
    Partial,
    Planned,
    Unsupported,
}

impl FeatureStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            FeatureStatus::Implemented => "implemented",
            FeatureStatus::Partial => "partial",
            FeatureStatus::Planned => "planned",
            FeatureStatus::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComplianceFeature {
    pub id: &'static str,
    pub clause: &'static str,
    pub title: &'static str,
    pub status: FeatureStatus,
    pub notes: &'static str,
    pub test_expectation: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SfcComplianceItem {
    pub id: &'static str,
    pub clause: &'static str,
    pub representation: &'static str,
    pub requirement_set: &'static str,
    pub status: FeatureStatus,
    pub evidence: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConformanceFixture {
    pub table: &'static str,
    pub fixture: &'static str,
    pub evidence: &'static str,
}

pub fn conformance_fixtures() -> &'static [ConformanceFixture] {
    CONFORMANCE_FIXTURES
}

const CONFORMANCE_FIXTURES: &[ConformanceFixture] = &[
    fixture(
        "Table 1",
        "iec_syntax::tests::parses_simple_program",
        "printed character subset and case-insensitive source parsing",
    ),
    fixture(
        "Table 2",
        "iec_syntax::tests::comment_and_identifier_property_corpus_parses_generated_programs",
        "identifier spelling, canonicalization, and strict-profile checks",
    ),
    fixture(
        "Table 3",
        "iec_syntax::tests::enforces_comment_and_pragma_implementation_limits",
        "framed comments and pragma handling",
    ),
    fixture(
        "Table 3a",
        "iec_syntax::tests::enforces_comment_and_pragma_implementation_limits",
        "implementation-defined pragma gating",
    ),
    fixture(
        "Table 4",
        "iec_syntax::tests::literal_property_corpus_parses_generated_values",
        "numeric literal syntax and range diagnostics",
    ),
    fixture(
        "Table 5",
        "iec_syntax::tests::parses_iec_character_string_escapes",
        "STRING/WSTRING literal forms and escapes",
    ),
    fixture(
        "Table 7",
        "iec_syntax::tests::parses_duration_literal",
        "duration literal aliases, ordering, and bounds",
    ),
    fixture(
        "Tables 8-9",
        "iec_runtime::tests::executes_date_and_time_of_day_literals",
        "DATE, TIME_OF_DAY, and DATE_AND_TIME runtime/C encodings",
    ),
    fixture(
        "Table 10",
        "iec_semantics::tests::flags_elementary_type_mismatches",
        "elementary type declarations and compatibility diagnostics",
    ),
    fixture(
        "Table 11",
        "iec_semantics::tests::checks_standard_function_generic_families",
        "Table 11 generic hierarchy, standard overload checks, and formal-order return inference",
    ),
    fixture(
        "Table 12",
        "iec_runtime::tests::executes_arrays_structs_enums_and_subrange_checks",
        "derived aliases, enums, arrays, structures, subranges, and initialization",
    ),
    fixture(
        "Tables 12, 14",
        "iec_c::tests::generated_c_matches_interpreter_for_nested_aggregate_access",
        "derived aggregate nested array access and C parity",
    ),
    fixture(
        "Tables 13, 16",
        "iec_semantics::tests::validates_derived_type_initializers",
        "initial values in variable declarations",
    ),
    fixture(
        "Tables 16-17",
        "iec_syntax::tests::parses_configuration_resources_tasks_and_program_instances",
        "variable declaration blocks, access paths, and configuration variables",
    ),
    fixture(
        "Table 25",
        "iec_runtime::tests::executes_string_bit_and_time_standard_functions",
        "bit shift and rotate functions",
    ),
    fixture(
        "Table 26",
        "iec_runtime::tests::executes_bool_and_bit_string_st_operators",
        "bitwise Boolean functions",
    ),
    fixture(
        "Tables 27-28",
        "iec_c::tests::generated_c_matches_interpreter_for_standard_function_formal_inputs",
        "selection/comparison formal input ordering and parity",
    ),
    fixture(
        "Table 29",
        "iec_stdlib::tests::enforces_string_function_positions_and_lengths",
        "character-string functions and bounds diagnostics",
    ),
    fixture(
        "Table 30",
        "iec_stdlib::tests::evaluates_date_and_time_table_functions",
        "date and time data type functions",
    ),
    fixture(
        "Table 31",
        "iec_runtime::tests::executes_arrays_structs_enums_and_subrange_checks",
        "enumerated data functions and ordinal parity",
    ),
    fixture(
        "Table 33",
        "iec_runtime::tests::executes_user_function_block_input_edge_qualifiers",
        "user-defined function block declarations, retained state, and R_EDGE/F_EDGE input qualifiers",
    ),
    fixture(
        "Table 34",
        "iec_runtime::tests::executes_bistable_and_edge_function_blocks",
        "SR and RS bistable function blocks",
    ),
    fixture(
        "Table 35",
        "iec_runtime::tests::executes_bistable_and_edge_function_blocks",
        "R_TRIG and F_TRIG edge detection",
    ),
    fixture(
        "Table 36",
        "iec_runtime::tests::executes_standard_counter_function_block",
        "CTU, CTD, and CTUD counters",
    ),
    fixture(
        "Tables 37-38",
        "iec_runtime::tests::executes_timer_function_blocks_with_cycle_time",
        "TON, TOF, and TP timers",
    ),
    fixture(
        "Table 39",
        "iec_runtime::tests::executes_counter_program",
        "PROGRAM declarations and scan-cycle execution",
    ),
    fixture(
        "Table 40",
        "iec_runtime::tests::validates_textual_sfc_elements",
        "SFC steps and initial steps",
    ),
    fixture(
        "Table 41",
        "iec_c::tests::generated_c_matches_interpreter_for_plcopen_sfc_topology",
        "SFC transition parsing, PLCopen step-transition-step topology, and evolution",
    ),
    fixture(
        "Table 42",
        "iec_runtime::tests::executes_sfc_action_qualifiers_and_timers",
        "SFC action declarations",
    ),
    fixture(
        "Table 43",
        "iec_runtime::tests::executes_sfc_action_qualifiers_and_timers",
        "SFC step/action association",
    ),
    fixture(
        "Tables 44-45a",
        "iec_c::tests::generated_c_matches_interpreter_for_sfc_qualifiers",
        "SFC action qualifiers and C parity",
    ),
    fixture(
        "Table 46",
        "iec_c::tests::generated_c_matches_interpreter_for_explicit_sfc_divergence_convergence",
        "SFC simultaneous transition update and divergence/convergence parity",
    ),
    fixture(
        "Tables 47-48",
        "iec_profile::tests::sfc_compliance_report_separates_sets",
        "SFC compatible/minimal compliance reporting",
    ),
    fixture(
        "Table 49",
        "iec_runtime::tests::runs_configuration_tasks_by_interval_and_priority",
        "configuration and resource declarations",
    ),
    fixture(
        "Table 50",
        "iec_runtime::tests::runs_configuration_tasks_by_interval_and_priority",
        "task interval and priority scheduling",
    ),
    fixture(
        "Table 51a",
        "iec_runtime::tests::executes_basic_instruction_list",
        "IL instruction and operand execution",
    ),
    fixture(
        "Table 51b",
        "iec_c::tests::generated_c_matches_interpreter_for_il_parenthesized_expression_lists",
        "IL parenthesized expression-list lowering and parity",
    ),
    fixture(
        "Table 52",
        "iec_runtime::tests::executes_instruction_list_jumps",
        "IL operators, jumps, and modifiers",
    ),
    fixture(
        "Tables 53-54",
        "iec_runtime::tests::executes_instruction_list_calls_and_conditional_returns",
        "IL function and function-block invocation forms",
    ),
    fixture(
        "Table 55",
        "iec_syntax::tests::operator_precedence_property_corpus_builds_expected_ast_shapes",
        "ST expression precedence and operators",
    ),
    fixture(
        "Table 56",
        "iec_runtime::tests::executes_loops_case_and_standard_functions",
        "ST assignments, calls, selection, iteration, RETURN, and EXIT",
    ),
    fixture(
        "Tables 57-62",
        "iec_plcopen::tests::lowers_ld_power_flow_connections",
        "LD import/export and power-flow lowering",
    ),
    fixture(
        "Tables 59-62",
        "iec_plcopen::tests::lowers_ld_parallel_branches_and_stored_coils",
        "LD contacts, coils, branches, and coil ordering",
    ),
    fixture(
        "Table E.1",
        "iec_semantics::tests::annex_e_style_negative_cases_emit_stable_diagnostics",
        "Annex E-style diagnostic fixtures",
    ),
    fixture(
        "Table D.1",
        "iec_profile::tests::annex_d_report_exposes_implementation_limits",
        "implementation-dependent parameter reporting",
    ),
];

pub fn sfc_compliance_report() -> &'static [SfcComplianceItem] {
    &[
        SfcComplianceItem {
            id: "sfc.compatible.textual",
            clause: "2.6.6 / Table 47",
            representation: "textual",
            requirement_set: "compatible SFC elements",
            status: FeatureStatus::Implemented,
            evidence:
                "Textual labeled steps/transitions/actions, explicit action associations, qualifiers, action-control set/reset aggregation, timed-association contention diagnostics, transition priorities, and divergence/convergence evolution are covered by interpreter/C parity tests.",
        },
        SfcComplianceItem {
            id: "sfc.compatible.graphical",
            clause: "2.6.6 / Table 47",
            representation: "graphical",
            requirement_set: "compatible SFC elements",
            status: FeatureStatus::Implemented,
            evidence:
                "PLCopen graphical SFC steps, macro steps, transitions, action blocks, direct links, selection/simultaneous connectors, jump targets, priorities, and action bodies import/export and execute with interpreter/generated-C parity.",
        },
        SfcComplianceItem {
            id: "sfc.minimal.textual",
            clause: "2.6.7 / Table 48",
            representation: "textual",
            requirement_set: "minimal SFC compliance",
            status: FeatureStatus::Implemented,
            evidence:
                "RoboC++ supports textual FROM/TO transitions, labeled SFC elements, explicit step/action associations, all Table 45 qualifiers, and action-control conflict diagnostics for the 2003 strict textual profile.",
        },
        SfcComplianceItem {
            id: "sfc.minimal.graphical",
            clause: "2.6.7 / Table 48",
            representation: "graphical",
            requirement_set: "minimal SFC compliance",
            status: FeatureStatus::Implemented,
            evidence:
                "The implemented PLCopen graphical SFC subset includes the minimal graphical compliance elements and is covered by topology, action, and generated-C parity fixtures.",
        },
    ]
}

const fn fixture(
    table: &'static str,
    fixture: &'static str,
    evidence: &'static str,
) -> ConformanceFixture {
    ConformanceFixture {
        table,
        fixture,
        evidence,
    }
}

#[derive(Debug, Clone)]
pub struct ComplianceMatrix {
    pub profile: EditionProfile,
    pub features: Vec<ComplianceFeature>,
}

impl ComplianceMatrix {
    pub fn for_profile(profile: EditionProfile) -> Self {
        let mut matrix = Self {
            profile,
            features: baseline_2003_features(),
        };

        if matches!(
            profile,
            EditionProfile::Iec61131_3_2013Placeholder | EditionProfile::Iec61131_3_2025Placeholder
        ) {
            matrix.features.push(ComplianceFeature {
                id: "edition.placeholder",
                clause: "profile",
                title: "Later edition placeholder",
                status: FeatureStatus::Unsupported,
                notes: "No compliance claim without licensed edition text and audit.",
                test_expectation:
                    "Add licensed-edition clause mapping and conformance fixtures before claimability.",
            });
        }

        matrix
    }

    pub fn counts(&self) -> ComplianceCounts {
        let mut counts = ComplianceCounts::default();
        for feature in &self.features {
            match feature.status {
                FeatureStatus::Implemented => counts.implemented += 1,
                FeatureStatus::Partial => counts.partial += 1,
                FeatureStatus::Planned => counts.planned += 1,
                FeatureStatus::Unsupported => counts.unsupported += 1,
            }
        }
        counts
    }

    pub fn to_markdown(&self) -> String {
        let counts = self.counts();
        let mut out = format!(
            "# IEC 61131-3 Compliance Matrix\n\nProfile: `{}`\n\nImplemented: {} | Partial: {} | Planned: {} | Unsupported: {}\n\n| ID | Clause | Feature | Status | Notes | Test expectation |\n| --- | --- | --- | --- | --- | --- |\n",
            self.profile,
            counts.implemented,
            counts.partial,
            counts.planned,
            counts.unsupported
        );

        for feature in &self.features {
            out.push_str(&format!(
                "| `{}` | {} | {} | `{}` | {} | {} |\n",
                feature.id,
                feature.clause,
                feature.title,
                feature.status.as_str(),
                feature.notes,
                feature.test_expectation
            ));
        }
        out
    }

    pub fn open_features(&self) -> impl Iterator<Item = &ComplianceFeature> {
        self.features
            .iter()
            .filter(|feature| feature.status != FeatureStatus::Implemented)
    }

    pub fn to_todo_markdown(&self) -> String {
        let counts = self.counts();
        let remaining = counts.partial + counts.planned + counts.unsupported;
        let mut out = format!(
            "# RoboC++ IEC 61131-3 TODOs\n\nProfile: `{}`\n\nRemaining: {} | Partial: {} | Planned: {} | Unsupported: {}\n\n",
            self.profile, remaining, counts.partial, counts.planned, counts.unsupported
        );
        out.push_str(&format!(
            "Scoped profile remaining: {remaining}\n\nFull compiler completion remaining: {remaining}\n\n"
        ));

        for feature in self.open_features() {
            out.push_str(&format!(
                "- [ ] `{}` ({}, `{}`): {} - {} Test expectation: {}\n",
                feature.id,
                feature.clause,
                feature.status.as_str(),
                feature.title,
                feature.notes,
                feature.test_expectation
            ));
        }
        out
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ComplianceCounts {
    pub implemented: usize,
    pub partial: usize,
    pub planned: usize,
    pub unsupported: usize,
}

pub fn baseline_2003_features() -> Vec<ComplianceFeature> {
    use FeatureStatus::Implemented;

    vec![
        feature("common.characters.ascii", "2.1.1 / Table 1", "Required printed character subset", Implemented, "The lexer accepts the ASCII character subset used by textual IEC sources."),
        feature("common.characters.case", "2.1.1 / Table 1", "Case-insensitive language elements", Implemented, "Keywords and identifiers are normalized for matching while original spelling is retained."),
        feature("common.identifiers.basic", "2.1.2 / Table 2", "Identifier syntax", Implemented, "Identifiers are parsed with implementation limits and strict-profile underscore diagnostics."),
        feature("common.keywords", "2.1.3 / Annex C", "Reserved keyword recognition", Implemented, "The parser recognizes IEC keywords case-insensitively in supported grammar positions."),
        feature("common.whitespace", "2.1.4", "Whitespace handling", Implemented, "Whitespace is skipped outside strings, comments, pragmas, and token-sensitive contexts."),
        feature("common.comments", "2.1.5 / Table 3", "Framed comments", Implemented, "Framed comments are skipped, unterminated comments are diagnosed, and nested comments are rejected for the 2003 strict profile."),
        feature("common.pragmas", "2.1.6 / Table 3a", "Implementation-defined pragmas", Implemented, "Pragmas are lexed and gated by implementation parameters with stable diagnostics when disabled."),
        feature("literals.numeric.syntax", "2.2.1 / Table 4", "Numeric literal syntax", Implemented, "Integer, real, based, typed, boolean, and bit-string literal forms parse with separator and invalid-digit diagnostics."),
        feature("literals.numeric.ranges", "2.2.1 / Table 4", "Numeric literal range validation", Implemented, "Constant integer, typed based literals, nested alias typed literals, unknown typed-literal type names, i128 typed-literal expressions, finite REAL/LREAL checks, REAL range checks, conversion ranges, and user function/function-block input actual ranges are checked in parser/semantic/runtime/backend paths covered by the regression suite."),
        feature("literals.strings.single_byte", "2.2.2 / Table 5", "Single-byte string literals", Implemented, "Quoted strings, IEC named `$` escapes, escaped single/double quote characters, IEC `$xx` hexadecimal character escapes, non-8-bit character diagnostics, raw control-character diagnostics, and Table 5 typed string literals parse, diagnose invalid escapes, and run through interpreter/C paths."),
        feature("literals.strings.wstring", "2.2.2 / Table 5", "Double-byte WSTRING literals", Implemented, "WSTRING has distinct double-quoted literals, escape handling, type checking, runtime values, PLCopen export, and generated C wide-codepoint storage with UTF-8 conversion helpers."),
        feature("literals.time.duration", "2.2.3.1 / Table 7", "Duration literals", Implemented, "Duration literals parse to millisecond values with sign handling, component ordering, fractional-placement checks, sub-component bound diagnostics, and alias typed-literal validation."),
        feature("literals.time.date_tod_dt", "2.2.3.2 / Tables 8-9", "DATE, TIME_OF_DAY, and DATE_AND_TIME literals", Implemented, "Date, time-of-day, and date-and-time literals parse, validate calendar/time fields, validate alias typed literals, and encode consistently for interpreter and C output."),
        feature("types.elementary.names", "2.3.1 / Table 10", "Elementary type names", Implemented, "BOOL, integer, real, bit-string, string, and date/time names are modeled in the IR."),
        feature("types.elementary.compatibility", "2.3.1 / Table 10", "Elementary type compatibility", Implemented, "Assignment, initialization, condition, arithmetic, equality/inequality, conversion, generic-family, and parameter compatibility checks are implemented for the supported IEC elementary scalar, string, bit-string, and date/time families."),
        feature("types.generic.hierarchy", "2.3.2 / Table 11", "Generic ANY_* hierarchy", Implemented, "Semantic checks encode the Table 11 ANY, ANY_DERIVED, ANY_ELEMENTARY, ANY_MAGNITUDE, ANY_NUM, ANY_REAL, ANY_INT, ANY_BIT, ANY_STRING, ANY_DATE, and concrete date/time/string/bit families used by standard functions. Overload checks cover formal-input ordering, EXPT real-base rules, enum separation from numeric/bit-string families, and MIN/MAX/LIMIT return-family preservation."),
        feature("types.derived.alias", "2.3.3 / Table 12", "Alias/simple derived types", Implemented, "Alias declarations parse, resolve through nested alias chains, initialize, interpret, and emit through supported scalar and aggregate paths, including typed scalar literals for BOOL, integer/bit-string, REAL, TIME, DATE/TOD/DT, enum, and subrange aliases."),
        feature("types.derived.enums", "2.3.3 / Tables 12, 14", "Enumerated types", Implemented, "Enum declarations, values, typed enum literals, initializer type ownership through aliases, ordinals, runtime values, C storage, duplicate-value diagnostics, cross-type ambiguity diagnostics, and enum-aware standard-function generic checks are modeled."),
        feature("types.derived.subranges", "2.3.3 / Tables 12, 14", "Subrange types", Implemented, "Subrange declarations, integer-base diagnostics, base-bound diagnostics, nested-alias initializer checks including constant standard-function formal-call folding, non-zero default initialization for globals/program vars/user-FB fields/disabled user-function returns, and runtime/backend range checks are covered."),
        feature("types.derived.arrays", "2.3.3 / Tables 12, 14", "Array types", Implemented, "Array types, initialization including repeated array initializer syntax, compatible whole-array assignment including local user-function copies, named-ARRAY input-to-return copies, local named-ARRAY function-return assignments, and user-function-block input/output/body copies, multidimensional and nested named-array/alias index arity/range diagnostics, interpreter values, generated-C state/copy output, named-ARRAY user-function inputs/returns, and declared lower-bound indexing plus local/input-variable subscript expressions are covered."),
        feature("types.derived.structures", "2.3.3 / Tables 12, 14", "Structured types", Implemented, "STRUCT declarations, nested alias-backed array field access, initialization, compatible whole-structure assignment including local user-function copies and user-function-block input/output/body copies, interpreter values, and C state/copy output are covered."),
        feature("types.derived.strings", "2.3.3 / Tables 12, 14", "String type declarations", Implemented, "STRING and WSTRING declarations support bounded storage, zero-length and implementation-maximum diagnostics, nested-alias literal and formal-call constant-expression length checks, runtime values, generated-C UTF-8 codepoint indexing/counting, C fixed-capacity storage, interpreter/generated-C bounded assignment truncation, and generated-C bounded STRING propagation through user-defined function-block inputs/outputs/body assignments."),
        feature("variables.symbolic", "2.4.1", "Symbolic variables", Implemented, "Simple symbolic references, array subscripts, and structure field selectors are represented in the IR."),
        feature("variables.direct", "2.4.1.1", "Direct variables and locations", Implemented, "AT locations and direct variable address syntax are parsed and validated for supported prefixes and sizes."),
        feature("variables.declarations", "2.4.2-2.4.3 / Tables 16-17", "Variable declaration blocks", Implemented, "VAR, VAR_INPUT, VAR_OUTPUT, VAR_IN_OUT, VAR_GLOBAL, VAR_EXTERNAL, VAR_TEMP, VAR_CONFIG, and VAR_ACCESS forms are parsed and checked in their supported contexts; VAR_EXTERNAL declarations validate matching VAR_GLOBAL declarations, enforce CONSTANT consistency, and bind to shared project-global interpreter/generated-C state; incomplete located declarations are validated; program-level VAR_TEMP storage resets on each interpreter/generated-C scan; program/configuration/resource VAR_ACCESS READ_WRITE simulator injection includes type/readonly diagnostics and persistent global/resource state."),
        feature("variables.initialization", "2.4.2 / Tables 13, 16", "Variable initialization", Implemented, "Scalar, typed-literal, enum, subrange, string, and aggregate initializers are checked and executed for supported types, including configuration/resource VAR_GLOBAL and VAR_CONFIG initializer diagnostics and constant standard-call folding for simulator global state, repeated array initializer expansion, formal-input ordering while folding constant standard calls for range/length checks, and subrange lower-bound defaults when zero is outside the declared range."),
        feature("variables.retain", "2.4.3", "RETAIN and NON_RETAIN", Implemented, "Retain qualifiers are validated and warm-restart behavior preserves retained state in the interpreter."),
        feature("variables.constants", "2.4.3", "CONSTANT variables", Implemented, "Writes to constant variables are diagnosed."),
        feature("variables.incomplete_locations", "2.4.3", "Incomplete located variables", Implemented, "Wildcard direct locations such as `%IX*` and `%QW*` are accepted in declarations for target resolution and rejected as executable direct references."),
        feature("pou.functions.declaration", "2.5.1.3", "Function declarations", Implemented, "Function POUs with supported ST bodies parse, check, reject recursive call cycles, interpret, and emit to C."),
        feature("pou.functions.en_eno", "2.5.1.2", "Function EN/ENO behavior", Implemented, "Implicit EN/ENO bindings, scalar standard-function output-formal diagnostics, standard formal-input ordering, type-aware disabled standard-call defaults for BOOL, REAL, TIME, STRING, and WSTRING interpreter/C paths, standard split-function statement EN/ENO in generated C inside user-function and user-FB bodies, named string/structure disabled user-function defaults in the interpreter, generated-C string/WSTRING, named-STRUCT, and named-ARRAY user-function returns including local array assignments with EN/ENO defaults, named-ARRAY user-function inputs, declaration-ordered named input actuals, and type-aware generated-C user-function expression lowering are modeled."),
        feature("pou.functions.overloads", "2.5.1.4", "Overloaded and extensible functions", Implemented, "Common standard overloads resolve through runtime/backend support with generic-family argument diagnostics, exact argument-count checks for non-extensible functions, non-ENO output rejection on scalar standard functions, EXPT real-base checks, constant MUX selector range checks, out-of-order formal input binding, user-function input range/length constraints, and formal-order return type inference for standard functions."),
        feature("stdlib.conversions", "2.5.1.5.1", "Type conversion functions", Implemented, "Scalar typed conversions, TRUNC, BCD helpers, DATE/TIME_OF_DAY/DATE_AND_TIME string conversions, and multi-component/fractional-last-component STRING_TO_TIME duration strings execute and emit to C for supported types, with source type-family, argument-count, target-range, BCD, invalid constant string/date/time conversion diagnostics, and standard-library matrix coverage for advertised source/target names."),
        feature("stdlib.numeric", "2.5.1.5.2", "Numeric functions", Implemented, "Arithmetic, min/max/limit/selection, power, and common numeric helpers execute and emit for supported scalar values."),
        feature("stdlib.bit_shift", "2.5.1.5.3 / Table 25", "Bit shift and rotate functions", Implemented, "SHL, SHR, ROL, and ROR families execute and emit for supported bit-string/integer storage; negative counts are diagnosed or rejected and generated C uses defined helper functions."),
        feature("stdlib.bit_boolean", "2.5.1.5.4 / Table 26", "Bitwise Boolean functions", Implemented, "AND, OR, XOR, and NOT are supported for BOOL and bit-string-like scalar values, with generated C using logical NOT for BOOL and bitwise complement for bit-string/integer values."),
        feature("stdlib.selection_comparison", "2.5.1.5.5 / Tables 27-28", "Selection and comparison functions", Implemented, "Selection and comparison functions are implemented for supported scalar values, including formal input ordering for `SEL`, `MUX`, and extensible comparison calls in interpreter and generated C parity tests."),
        feature("stdlib.strings", "2.5.1.5.6 / Table 29", "Character string functions", Implemented, "`LEN`, `LEFT`, `RIGHT`, `MID`, `CONCAT`, `INSERT`, `DELETE`, `REPLACE`, and `FIND` execute and emit to C for STRING/WSTRING values with ANY_STRING/ANY_INT semantic checks, negative-count diagnostics, constant position/length diagnostics, UTF-8 codepoint-aware indexing/counting, and generated-C parity coverage."),
        feature("stdlib.time", "2.5.1.5.7 / Table 30", "Time data type functions", Implemented, "TIME, DATE, TIME_OF_DAY, and DATE_AND_TIME arithmetic, construction, day-of-week, split/extraction, and string conversion paths including multi-component and fractional STRING_TO_TIME durations are checked, interpreted, and emitted to C for the 2003 strict profile."),
        feature("stdlib.enums", "2.5.1.5.8 / Table 31", "Enumerated data type functions", Implemented, "SEL, MUX, EQ, and NE accept enumerated data values with enum-aware semantic checks plus interpreter and C ordinal parity for the 2003 strict profile."),
        feature("pou.function_blocks.user", "2.5.2.2 / Table 33", "User-defined function blocks", Implemented, "Flat and nested user FBs parse, check, interpret, and emit to C for supported ST/IL bodies; R_EDGE/F_EDGE BOOL input qualifiers execute with retained previous-input state in interpreter and generated C; positional input binding, `VAR_IN_OUT` alias copy-back including nested positional calls, persistent nested FB state, nested user-FB calls, duplicate positional/named input binding diagnostics, input range/length constraints, bounded STRING and aggregate input/output/body propagation, type-aware generated-C expressions including array lower bounds, BOOL/integer NOT, nested function-call formal ordering and EN/ENO defaults, nested standard FB instances, standard split-function statement calls, IL LD/ST/JMP/RET/CAL bodies with unique inlined labels, CASE/FOR/WHILE/REPEAT/EXIT/RETURN control flow in generated C, and direct nested field access are covered."),
        feature("pou.function_blocks.en_eno", "2.5.2.1a", "Function block EN/ENO behavior", Implemented, "ST function-block calls support implicit EN gating, positional standard FB inputs, standard FB output binding diagnostics, negated BOOL output copy-back, disabled-call output preservation, and ENO output diagnostics/execution in interpreter and C; line-oriented IL CAL covers positional standard FB inputs; nested user-FB generated C honors EN/ENO gates."),
        feature("stdlib.fb.bistable", "2.5.2.3.1 / Table 34", "SR and RS bistables", Implemented, "SR and RS execute in the interpreter and C backend."),
        feature("stdlib.fb.edge", "2.5.2.3.2 / Table 35", "R_TRIG and F_TRIG edge detection", Implemented, "Rising and falling edge function blocks execute with retained prior-input state."),
        feature("stdlib.fb.counters", "2.5.2.3.3 / Table 36", "CTU, CTD, and CTUD counters", Implemented, "Counter FBs execute and emit to C with current supported state fields."),
        feature("stdlib.fb.timers", "2.5.2.3.4 / Tables 37-38", "TON, TOF, and TP timers", Implemented, "Timer FBs execute against deterministic cycle time and emit to C."),
        feature("stdlib.fb.communication", "2.5.2.3.5", "Communication function blocks", Implemented, "Communication FB names expose typed status fields, semantic hook diagnostics, Rust runtime hooks, and generated C hook ABI; protocol-specific behavior is target-supplied through those hooks."),
        feature("pou.programs.declaration", "2.5.3 / Table 39", "Program declarations", Implemented, "Program POUs parse, check, execute, and emit to C for supported bodies."),
        feature("sfc.steps", "2.6.2 / Table 40", "SFC steps and initial steps", Implemented, "Textual SFC steps and initial-step flags parse in keyword and labeled forms and validate duplicate and initial-step errors."),
        feature("sfc.transitions", "2.6.3 / Table 41", "SFC transitions", Implemented, "Textual ST-expression transitions, textual IL accumulator transition bodies, native textual LADDER transition bodies, and native textual FBD transition outputs parse and run, including `TRANSITION ... FROM ... TO ... END_TRANSITION`, labeled `Name: TRANSITION ...`, and multi-predecessor/multi-successor step lists. Direct PLCopen graphical step-transition-step links, selection/simultaneous branch connector nodes, `jumpStep`/`jump` targets, and `macroStep` nodes import/export into executable `from`/`to` edges with interpreter/generated-C parity."),
        feature("sfc.actions.declaration", "2.6.4.1 / Table 42", "SFC action declarations", Implemented, "Named ST action bodies parse in `ACTION Name:` and labeled `Name: ACTION` forms, PLCopen graphical action declarations import/export, and action bodies attach to step execution."),
        feature("sfc.actions.association", "2.6.4.2 / Table 43", "Step/action association", Implemented, "Step-named actions and explicit STEP/INITIAL_STEP action association blocks parse, validate referenced actions, execute in the interpreter/generated C, and round-trip through PLCopen actionBlock."),
        feature("sfc.actions.qualifiers", "2.6.4.3-2.6.4.5 / Tables 44-45a", "Action qualifiers and action control", Implemented, "N, S, R, P/P1, P0, L, D, SD, DS, and SL qualifier paths execute in interpreter/C for action declarations and explicit step associations. Action control is aggregated per action, S/R associations share stored state, active timed-association contention is diagnosed, and generated C metadata reports effective association qualifiers."),
        feature("sfc.sequence_evolution", "2.6.5 / Table 46", "SFC sequence evolution", Implemented, "Linear transition firing is deterministic, generated C computes transition fire flags before step updates to match interpreter scan-cycle semantics, explicit textual divergence/convergence transitions plus direct, branch-connector, jump-target, and macro-step PLCopen graphical topology have interpreter/C parity, and transition priorities resolve same-predecessor conflicts while independent transitions still fire in the same scan."),
        feature("sfc.compliance_sets", "2.6 / Tables 47-48", "SFC compliance sets", Implemented, "`rbcpp sfc-compliance` reports textual/graphical compatible and minimal SFC support states separately, including non-claimable gaps."),
        feature("configuration.declaration", "2.7.1 / Table 49", "Configuration and resource declarations", Implemented, "CONFIGURATION, RESOURCE, VAR_GLOBAL, VAR_CONFIG, PROGRAM instances, task references, and access-path declarations parse and check. PROGRAM instance initialization actuals parse, validate against target PROGRAM variables, and apply constant initial values in the configuration runtime; PROGRAM instance VAR_OUTPUT bindings validate against configuration/resource scalar and indexed aggregate targets and copy values after scheduled scans."),
        feature("configuration.tasks", "2.7.2 / Table 50", "Tasks", Implemented, "`TASK` declarations preserve and validate `SINGLE`, `INTERVAL`, and `PRIORITY` parameters; the deterministic configuration runner schedules interval tasks by priority and `SINGLE` event tasks on rising BOOL edges."),
        feature("configuration.access_paths", "2.7.1", "Access paths", Implemented, "`VAR_ACCESS` declarations parse with `READ_ONLY`/`READ_WRITE`, direct/simple symbolic targets plus dotted POU/configuration/resource/program-instance targets are checked, access paths stay out of executable state, simulator traces/C metadata expose resolved runtime values where storage exists, generated C exposes program-level read/write access services with READ_ONLY enforcement, configuration/resource READ_WRITE simulator injection persists globals across scan cycles, and rbcpp_target binds access paths to retained state or external HAL transports including ROS 2 parameters through a target supervisor."),
        feature("language.il.instructions", "3.2.1 / Table 51a", "IL instructions and operands", Implemented, "Semicolon-delimited and line-oriented IL instructions parse, check, interpret, and emit to C, including LD/ST/JMP/RET/CAL coverage inside user-defined function-block bodies with unique labels."),
        feature("language.il.parenthesized", "3.2.1 / Table 51b", "IL parenthesized expressions", Implemented, "ST-style parenthesized operands and IEC-style nested IL expression lists lower into expression IR and are covered by parser, interpreter, and generated C parity tests."),
        feature("language.il.operators", "3.2.2 / Table 52", "IL operators", Implemented, "Accumulator operators, typed IL mnemonic suffixes, N modifiers, store/latch operand diagnostics, jumps, conditional returns, and calls parse and execute through interpreter/generated-C parity tests."),
        feature("language.il.invocation", "3.2.3 / Tables 53-54", "IL function and FB invocation", Implemented, "CAL/CALC/CALCN formal call operands, simple instance operands, positional standard FB CAL inputs, and positional user-FB CAL inputs with VAR_IN_OUT copy-back execute through interpreter/generated-C parity tests, including nested standard FB calls inside generated-C user function-block bodies."),
        feature("language.st.expressions", "3.3.1 / Table 55", "ST expressions and operators", Implemented, "ST expression support covers arithmetic, comparisons, unary plus/minus, BOOL/bit operations, calls, aggregates, indexing, fields, operator precedence, enum-aware CASE labels, and type-aware interpreter/generated-C lowering."),
        feature("language.st.assignments", "3.3.2.1 / Table 56", "ST assignment statements", Implemented, "Assignments to supported symbolic, array, and structure targets parse, check, interpret, and emit to C."),
        feature("language.st.subprogram_control", "3.3.2.2 / Table 56", "ST calls, RETURN, and EXIT", Implemented, "Calls, RETURN, and EXIT are supported in semantic/runtime/backend paths, including generated-C RETURN parity for user function-block bodies, EXIT context diagnostics, attached ST calls that overlap IL mnemonics, and diagnostics for value-returning user/standard function calls used as stand-alone statements."),
        feature("language.st.selection", "3.3.2.3 / Table 56", "ST IF and CASE statements", Implemented, "IF and CASE parse, check, execute, and emit to C for supported expressions; CASE selector type, reversed constant ranges, overlapping constant integer labels, typed enum labels adjacent to colons, enum label ownership, enum duplicate labels, and enum CASE runtime/C paths are covered."),
        feature("language.st.iteration", "3.3.2.4 / Table 56", "ST FOR, WHILE, and REPEAT statements", Implemented, "FOR, WHILE, REPEAT, and EXIT execute in the interpreter and supported C paths; constant zero FOR BY steps are diagnosed before runtime."),
        feature("language.ld.import_preserve", "4.2 / Tables 57-62", "LD PLCopen preservation", Implemented, "PLCopen LD nodes are imported, represented, and exported without custom editor support."),
        feature("language.ld.native_textual", "4.2", "Native textual LD source entry", Implemented, "The textual frontend accepts LADDER/RUNG bodies with CONTACT, CONTACT_NOT, COIL, SET, and RESET elements. Native textual LD lowers into normalized statement/expression IR and is covered by parser, interpreter, generated-C parity, CLI, and shipped-example tests."),
        feature("language.ld.simple_lowering", "4.2 / Tables 59-62", "Simple LD network lowering", Implemented, "Native textual LD and PLCopen XML contact-to-coil networks lower into normalized assignment IR and are covered by interpreter/generated-C parity tests."),
        feature("language.ld.power_flow", "4.2.1-4.2.6", "LD power rails, contacts, coils, and evaluation", Implemented, "Native textual LD handles series contacts, negated contacts, coils, set/reset coils, and deterministic rung ordering; PLCopen XML LD lowering handles left rails, series contacts, parallel branches, multiple coils, negated contacts/coils, rising/falling edge contacts via hidden R_TRIG/F_TRIG helper instances, set/reset coils, connector/continuation forwarding, deterministic coil ordering, and interpreter/generated-C parity for imported power-flow networks."),
        feature("language.fbd.import_preserve", "4.3", "FBD PLCopen preservation", Implemented, "PLCopen FBD nodes are imported, represented, and exported without custom editor support."),
        feature("language.fbd.native_textual", "4.3", "Native textual FBD source entry", Implemented, "The textual frontend accepts FBD/NETWORK bodies with OUT assignments whose right-hand sides are function-block/data-flow expressions. Native textual FBD lowers into normalized statement/expression IR and is covered by parser, interpreter, generated-C parity, CLI, and shipped-example tests."),
        feature("language.fbd.simple_lowering", "4.3.2", "Simple FBD network lowering", Implemented, "Native textual FBD and PLCopen XML FBD data-flow networks lower into calls/assignments and are covered by interpreter/generated-C parity tests."),
        feature("language.fbd.data_flow", "4.3.1-4.3.3", "FBD data-flow, feedback, and evaluation", Implemented, "Native textual FBD handles ordered OUT data-flow assignments using nested call/expression graphs; PLCopen XML FBD lowering handles multi-output acyclic data-flow graphs, nested block calls, formal input wiring, connector/continuation forwarding, deterministic output ordering, feedback diagnostics for cycles that cannot be lowered safely, and interpreter/generated-C parity for imported data-flow networks."),
        feature("plcopen.project", "PLCopen XML 2.01", "PLCopen project import/export", Implemented, "Projects, POUs, interfaces including function `returnType`, schema-style elementary type tags, simple/array/structure variable `initialValue` round-trips, `accessVars/accessVariable` aliases and directions, ST/IL text, SFC including actionBlock associations plus direct, branch-connector, jumpStep/jump, and macroStep topology, LD/FBD nodes with geometry metadata, configurations including interval/event task attributes, schema-style `configVariable` declarations, schema-style task/resource `pouInstance` program instances plus RoboC++ vendor addData for PROGRAM input/output actuals, data types including schema-style array `baseType` and signed/unsigned subranges, and a composite full-project graphical/configuration fixture round-trip are covered."),
        feature("plcopen.vendor_metadata", "PLCopen XML 2.01", "PLCopen vendor metadata and extensions", Implemented, "Project-level PLCopen fileHeader, contentHeader, addData vendor metadata, and nested addData payloads are preserved through import/export fixtures, including nested addData extension bodies that contain vendor namespaces."),
        feature("backend.interpreter.scan", "implementation", "Deterministic scan-cycle interpreter", Implemented, "Programs execute with deterministic cycle traces and bounded scan-cycle options."),
        feature("backend.interpreter.configuration", "implementation", "Configuration task scheduler", Implemented, "The deterministic configuration scheduler supports interval tasks, priority ordering, `SINGLE` rising-edge event tasks, PROGRAM instance initializers, constant standard-call initialization for configuration/resource globals, configuration/resource access-path writes, and persistent simulator global/resource state."),
        feature("backend.interpreter.parity", "implementation", "Interpreter language parity", Implemented, "The interpreter covers ST, IL, native textual LD, native textual FBD, SFC including explicit step/action associations and direct/branch/jump/macro-step PLCopen SFC topology, nested user-defined function blocks including positional input binding, IL CAL positional inputs, nested VAR_IN_OUT copy-back, and RETURN control flow, standard FB output bindings with enabled/disabled EN paths, positional standard FB inputs in ST and IL CAL calls, date/time literals/conversions, typed enum and scalar alias literals, bounded string assignment truncation, program-level VAR_TEMP scan reset, configuration PROGRAM output copy-back, and the standard-library corpus."),
        feature("backend.c.program_scan", "implementation", "Portable C program scan ABI", Implemented, "Generated C exposes init, warm-restart, state, scan, metadata, and helper routines for supported programs."),
        feature("backend.c.language_parity", "implementation", "C backend language parity", Implemented, "C generation covers ST, IL, native textual LD, native textual FBD, SFC including explicit step/action associations and direct/branch/jump/macro-step PLCopen SFC topology, standard FBs with output bindings, disabled EN preservation, and positional inputs in ST and IL CAL calls, nested user-defined FBs for supported ST/IL bodies including positional input binding, IL CAL positional inputs, nested VAR_IN_OUT copy-back, type-aware expressions, nested standard FB instances, standard split-function statement calls with EN/ENO inside user functions and user FBs, IL LD/ST/JMP/RET/CAL bodies with unique labels, bounded STRING/aggregate propagation, CASE/FOR/WHILE/REPEAT/EXIT/RETURN control flow, functions including bounded STRING/WSTRING, named-STRUCT, named-ARRAY user-function inputs/returns with disabled-call defaults, declaration-ordered named user-function inputs, type-aware local expressions, whole-array and whole-structure copies, repeated array initializer output, UTF-8 codepoint-aware string helpers, DATE/TOD/DT conversions, typed scalar alias literals, non-zero subrange defaults, BOOL-vs-bit-string NOT lowering, program-level VAR_TEMP scan reset, aggregate state, all shipped examples, and a standard-library parity corpus against interpreter traces."),
        feature("backend.c.target_abi", "implementation", "Target HAL ABI", Implemented, "Generated C exposes target hooks for located %I/%Q/%M I/O, direct-variable storage, retained load/save, scan lifecycle callbacks, watchdog petting, and monotonic cycle-time context; the rbcpp_target crate provides Linux-style file-backed I/O mapping, Modbus coil/register image mapping, EtherCAT PDO image mapping, ROS 2 topic/parameter bridge mapping, transport-backed Modbus/EtherCAT/ROS 2 HAL adapter traits, target-side VAR_ACCESS bindings, retained-state files, mapping-file loading, watchdog helpers, target supervisor cycle reports, and non-certified safety gating. RoboC++ deliberately does not claim safety-certified controller status without external certification evidence."),
        feature("diagnostics.human_json", "1.5 / Annex E", "Human and JSON diagnostics", Implemented, "Diagnostics render as human text and JSON with stable category codes."),
        feature("diagnostics.annex_e", "Annex E / Table E.1", "Annex E-style error coverage", Implemented, "Broad negative fixtures cover duplicate/unknown names, unknown typed-literal types, type mismatches, direct variables, strict identifiers, configurations, SFC action-control contention, constant writes, VAR_EXTERNAL CONSTANT mismatches, EXIT misuse, CASE overlaps, subrange bounds, IL labels, missing function returns, enum duplicates, array bounds, access paths, standard function arity, conversion ranges and invalid constant conversions, RETAIN qualifiers, recursive functions, and VAR_IN_OUT actuals with stable human and JSON diagnostics."),
        feature("diagnostics.compliance", "1.5", "Compliance reporting", Implemented, "The CLI reports profile-gated compliance status and feature notes, the generated TODO report includes both scoped-profile and full-completion remaining counts, and regression tests keep the human TODO, README scope statement, conformance notes, and compliance matrix synchronized."),
        feature("diagnostics.unsupported_ir_boundary", "Annex E", "Unsupported IR boundary", Implemented, "Unsupported statement nodes are parser recovery sentinels only. Claimed language constructs parse into concrete IR, invalid constructs produce syntax or semantic diagnostics, and later stages keep defensive handling so recovered invalid IR cannot crash the interpreter or generated-C pipeline."),
        feature("parameters.annex_d", "Annex D / Table D.1", "Implementation-dependent parameter reporting", Implemented, "Core implementation-dependent limits are reported through the profile API and `rbcpp parameters`."),
    ]
}

fn parameter(
    id: &'static str,
    clause: &'static str,
    title: &'static str,
    value: String,
    unit: &'static str,
    notes: &'static str,
) -> ImplementationParameter {
    ImplementationParameter {
        id,
        clause,
        title,
        value,
        unit,
        notes,
    }
}

fn feature(
    id: &'static str,
    clause: &'static str,
    title: &'static str,
    status: FeatureStatus,
    notes: &'static str,
) -> ComplianceFeature {
    ComplianceFeature {
        id,
        clause,
        title,
        status,
        notes,
        test_expectation: test_expectation_for(id, status),
    }
}

fn test_expectation_for(id: &str, status: FeatureStatus) -> &'static str {
    if status == FeatureStatus::Implemented {
        return "Covered by the implemented feature regression suite.";
    }

    if id.starts_with("literals.") {
        "Add positive and negative parser/semantic fixtures for every literal form plus runtime/C parity for encodable values."
    } else if id.starts_with("types.") {
        "Add declaration, initialization, assignment, conversion, runtime, and C parity fixtures for every affected type form."
    } else if id.starts_with("variables.") {
        "Add declaration-context diagnostics plus interpreter/C state and binding fixtures for the full variable form."
    } else if id.starts_with("pou.") {
        "Add semantic, interpreter, and C fixtures covering every supported invocation and state/return edge case."
    } else if id.starts_with("stdlib.") {
        "Add standard-library truth-table fixtures and interpreter/C parity traces for all supported overloads and edge cases."
    } else if id.starts_with("sfc.") {
        "Add sequence-evolution fixtures that prove active-step, transition, action, qualifier, and C parity behavior."
    } else if id.starts_with("configuration.") {
        "Add configuration/resource/task/access-path fixtures with semantic diagnostics and scheduler/runtime traces."
    } else if id.starts_with("language.il.") {
        "Add parser, semantic, interpreter, and C parity fixtures for each IL grammar/operator/invocation form."
    } else if id.starts_with("language.st.") {
        "Add parser, semantic, interpreter, and C parity fixtures for each ST expression/statement edge case."
    } else if id.starts_with("language.ld.") {
        "Add PLCopen import/export and lowered-IR parity fixtures for ladder power-flow networks."
    } else if id.starts_with("language.fbd.") {
        "Add PLCopen import/export and lowered-IR parity fixtures for FBD data-flow, feedback, and scheduling."
    } else if id.starts_with("plcopen.") {
        "Add PLCopen XML round-trip fixtures that preserve schema metadata and generated normalized IR."
    } else if id.starts_with("backend.") {
        "Add interpreter-versus-C or target ABI parity fixtures with observable scan traces and generated metadata checks."
    } else if id.starts_with("diagnostics.") {
        "Add stable human and JSON diagnostic fixtures for every listed error condition."
    } else {
        "Add conformance fixtures that prove the feature against the cited clause and compliance matrix ID."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_matrix_is_leaf_level() {
        let matrix = ComplianceMatrix::for_profile(EditionProfile::Iec61131_3_2003Strict);
        let counts = matrix.counts();

        assert!(matrix.features.len() > 70);
        assert_eq!(counts.partial, 0);
        assert_eq!(counts.planned, 0);
        assert_eq!(counts.unsupported, 0);
        assert!(matrix.features.iter().any(|feature| {
            feature.id == "language.st.assignments" && feature.status == FeatureStatus::Implemented
        }));
        assert!(matrix.features.iter().any(|feature| {
            feature.id == "language.ld.power_flow" && feature.status == FeatureStatus::Implemented
        }));
        assert!(matrix.features.iter().any(|feature| {
            feature.id == "language.ld.native_textual"
                && feature.status == FeatureStatus::Implemented
        }));
        assert!(matrix.features.iter().any(|feature| {
            feature.id == "language.fbd.native_textual"
                && feature.status == FeatureStatus::Implemented
        }));
        assert!(matrix.features.iter().any(|feature| {
            feature.id == "sfc.transitions" && feature.status == FeatureStatus::Implemented
        }));
    }

    #[test]
    fn todo_markdown_excludes_implemented_features() {
        let matrix = ComplianceMatrix::for_profile(EditionProfile::Iec61131_3_2003Strict);
        let todos = matrix.to_todo_markdown();

        assert!(todos.contains("Remaining: 0"));
        assert!(todos.contains("Scoped profile remaining: 0"));
        assert!(todos.contains("Full compiler completion remaining: 0"));
        assert!(!todos.contains("Test expectation:"));
        assert!(!todos.contains("language.st.assignments"));
        assert!(!todos.contains("sfc.transitions"));
    }

    #[test]
    fn open_features_have_test_expectations() {
        let matrix = ComplianceMatrix::for_profile(EditionProfile::Iec61131_3_2003Strict);
        for feature in matrix.open_features() {
            assert!(!feature.id.is_empty());
            assert!(!feature.clause.is_empty());
            assert!(
                !feature.test_expectation.trim().is_empty(),
                "{} is missing a test expectation",
                feature.id
            );
        }
        assert!(matrix.to_todo_markdown().contains("Remaining: 0"));
    }

    #[test]
    fn human_docs_track_open_compliance_features() {
        let matrix = ComplianceMatrix::for_profile(EditionProfile::Iec61131_3_2003Strict);
        let readme = include_str!("../../../README.md");
        let conformance = include_str!("../../../CONFORMANCE.md");
        let checklist = include_str!("../../../docs/iec61131-2003-checklist.md");

        for feature in matrix.open_features() {
            assert!(
                checklist.contains(feature.id),
                "conformance checklist is missing open feature {}",
                feature.id
            );
        }
        assert!(readme.contains("complete for the repository's current `2003-strict`"));
        assert!(conformance.contains("complete for the repository's current `2003-strict`"));
        assert_profile_qualified_completeness_claims(&[
            ("README.md", readme),
            ("CONFORMANCE.md", conformance),
            (
                "PRODUCTION_READINESS.md",
                include_str!("../../../PRODUCTION_READINESS.md"),
            ),
            (
                "validation/releases/current.md",
                include_str!("../../../validation/releases/current.md"),
            ),
            (
                "validation/releases/RELEASE_NOTES_CURRENT.md",
                include_str!("../../../validation/releases/RELEASE_NOTES_CURRENT.md"),
            ),
        ]);
    }

    fn assert_profile_qualified_completeness_claims(docs: &[(&str, &str)]) {
        for (path, text) in docs {
            for sentence in profile_claim_sentences(text) {
                let normalized = sentence.to_ascii_lowercase();
                if normalized.contains("complete iec 61131")
                    || normalized.contains("complete compiler")
                    || normalized.contains("complete language compiler")
                {
                    assert!(
                        normalized.contains("current profile")
                            || normalized.contains("2003-strict")
                            || normalized.contains("iec61131-3:2003-strict"),
                        "{path} has an unqualified compiler-completeness claim: {sentence}"
                    );
                }
            }
        }
    }

    fn profile_claim_sentences(text: &str) -> Vec<String> {
        text.split_terminator(['.', '!', '?'])
            .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|part| !part.is_empty())
            .collect()
    }

    #[test]
    fn every_matrix_table_has_conformance_fixture_evidence() {
        let matrix = ComplianceMatrix::for_profile(EditionProfile::Iec61131_3_2003Strict);
        let fixtures = conformance_fixtures();

        for feature in &matrix.features {
            if !feature.clause.contains("Table") {
                continue;
            }
            assert!(
                fixtures
                    .iter()
                    .any(|fixture| feature.clause.contains(fixture.table)),
                "{} references '{}' without a conformance fixture",
                feature.id,
                feature.clause
            );
        }

        for fixture in fixtures {
            assert!(!fixture.fixture.trim().is_empty());
            assert!(!fixture.evidence.trim().is_empty());
            assert!(
                matrix
                    .features
                    .iter()
                    .any(|feature| feature.clause.contains(fixture.table)),
                "fixture {} references unused {}",
                fixture.fixture,
                fixture.table
            );
        }
    }

    #[test]
    fn annex_d_report_exposes_implementation_limits() {
        let parameters = ImplementationParameters::default();
        let report = parameters.annex_d_report();

        assert!(report
            .iter()
            .any(|parameter| parameter.id == "max_identifier_length" && parameter.value == "128"));
        assert!(report.iter().any(|parameter| {
            parameter.id == "max_plcopen_xml_depth" && parameter.value == "256"
        }));
        assert!(report
            .iter()
            .any(|parameter| parameter.id == "pragmas_enabled" && parameter.value == "false"));
        assert!(parameters
            .annex_d_markdown()
            .contains("Maximum string length"));
    }

    #[test]
    fn sfc_compliance_report_separates_sets() {
        let report = sfc_compliance_report();

        assert!(report
            .iter()
            .any(|item| item.id == "sfc.compatible.textual"
                && item.status == FeatureStatus::Implemented));
        assert!(report
            .iter()
            .any(|item| item.id == "sfc.minimal.graphical"
                && item.status == FeatureStatus::Implemented));
    }
}
