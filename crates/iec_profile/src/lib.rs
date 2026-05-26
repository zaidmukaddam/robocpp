use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditionProfile {
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

impl Default for EditionProfile {
    fn default() -> Self {
        EditionProfile::Iec61131_3_2003Strict
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
    pub max_identifier_length: usize,
    pub max_comment_length: usize,
    pub max_expression_depth: usize,
    pub max_statement_depth: usize,
    pub max_array_elements: usize,
    pub max_structure_elements: usize,
    pub max_string_length: usize,
    pub max_scan_cycles: usize,
    pub pragmas_enabled: bool,
}

impl Default for ImplementationParameters {
    fn default() -> Self {
        Self {
            max_identifier_length: 128,
            max_comment_length: 1_000_000,
            max_expression_depth: 256,
            max_statement_depth: 256,
            max_array_elements: 1_000_000,
            max_structure_elements: 4096,
            max_string_length: 65_535,
            max_scan_cycles: 10_000,
            pragmas_enabled: false,
        }
    }
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
            "# IEC 61131-3 Compliance Matrix\n\nProfile: `{}`\n\nImplemented: {} | Partial: {} | Planned: {} | Unsupported: {}\n\n| ID | Clause | Feature | Status | Notes |\n| --- | --- | --- | --- | --- |\n",
            self.profile,
            counts.implemented,
            counts.partial,
            counts.planned,
            counts.unsupported
        );

        for feature in &self.features {
            out.push_str(&format!(
                "| `{}` | {} | {} | `{}` | {} |\n",
                feature.id,
                feature.clause,
                feature.title,
                feature.status.as_str(),
                feature.notes
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
    use FeatureStatus::{Implemented, Partial};

    vec![
        ComplianceFeature {
            id: "common.characters",
            clause: "2.1",
            title: "Character set, comments, identifiers, keywords",
            status: Partial,
            notes: "Lexer supports ASCII, comments, pragma gating, comment length checks, and case-insensitive identifiers.",
        },
        ComplianceFeature {
            id: "common.literals",
            clause: "2.2",
            title: "Numeric, string, boolean, date/time literals",
            status: Partial,
            notes: "Common literal forms parse; detailed range validation remains planned.",
        },
        ComplianceFeature {
            id: "types.elementary",
            clause: "2.3.1",
            title: "Elementary data types",
            status: Partial,
            notes: "Core elementary names are modeled; assignment, condition, FOR, user function parameter compatibility, expression depth limits, statement depth limits, and runtime scan-cycle limits are checked.",
        },
        ComplianceFeature {
            id: "types.derived",
            clause: "2.3.3",
            title: "Derived types: aliases, enums, subranges, arrays, structures, strings",
            status: Partial,
            notes: "IR, parser, semantic validation, interpreter storage, and C aggregate state cover aliases, enums, subranges, arrays, structures, and strings for the supported ST subset; full conversion/range audit remains planned.",
        },
        ComplianceFeature {
            id: "variables",
            clause: "2.4",
            title: "Variable representation, declaration, location, initialization",
            status: Partial,
            notes: "VAR blocks, AT locations, direct variable address validation, initial values, CONSTANT write diagnostics, RETAIN/NON_RETAIN validation, interpreter warm-restart behavior, C warm-restart entry points, and basic initializer checks are modeled.",
        },
        ComplianceFeature {
            id: "pou.functions",
            clause: "2.5.1",
            title: "Functions, EN/ENO model, overloaded functions",
            status: Partial,
            notes: "Function POUs parse, check, interpret, and emit to C for basic ST bodies; implicit EN/ENO bindings and all-path return diagnostics are modeled; expanded arithmetic, selection, comparison, shift/rotate, bit-string boolean, common string, scalar TIME, TRUNC, BCD, and scalar typed conversion standard functions are implemented where current runtime/backend types support them; constant integer/bit-string/BCD conversion range diagnostics are checked; full overload audit remains planned.",
        },
        ComplianceFeature {
            id: "pou.function_blocks",
            clause: "2.5.2",
            title: "Function blocks and standard function blocks",
            status: Partial,
            notes: "Stateful interpreter and C support exists for user-defined FB flat state plus SR, RS, R_TRIG, F_TRIG, CTU, CTD, CTUD, TON, TOF, and TP; communication FBs USEND, URCV, BSEND, BRCV, SEND, and RCV are recognized with unsupported-simulation diagnostics.",
        },
        ComplianceFeature {
            id: "pou.programs",
            clause: "2.5.3",
            title: "Programs",
            status: Implemented,
            notes: "Program POUs parse, check, and execute for the supported ST subset.",
        },
        ComplianceFeature {
            id: "sfc",
            clause: "2.6",
            title: "Sequential Function Chart elements",
            status: Partial,
            notes: "Textual SFC steps, initial-step flags, transitions, and ST action bodies parse into normalized IR with duplicate, initial-step, and transition condition validation; a deterministic linear SFC interpreter and C backend model initial steps, transition firing, active step state, and matching step-named actions; SFC structure is also preserved through PLCopen XML; full action qualifier/timer semantics remain planned.",
        },
        ComplianceFeature {
            id: "configuration",
            clause: "2.7",
            title: "Configurations, resources, access paths, tasks",
            status: Partial,
            notes: "CONFIGURATION, RESOURCE, TASK, PROGRAM instance, VAR_GLOBAL, VAR_CONFIG, and VAR_ACCESS declarations parse and check references/locations; cross-POU and configuration/resource VAR_GLOBAL visibility is modeled for semantic checks; a deterministic configuration runner schedules program instances by task interval and priority with multi-program traces.",
        },
        ComplianceFeature {
            id: "language.il",
            clause: "3.2",
            title: "Instruction List",
            status: Partial,
            notes: "Core accumulator operators, N-modifier forms, labels, JMP/JMPC/JMPCN, CAL/CALC/CALCN function block calls, RETC/RETCN conditional returns, and parenthesized IL operand expressions parse, check, execute, and emit to C when semicolon-delimited.",
        },
        ComplianceFeature {
            id: "language.st",
            clause: "3.3",
            title: "Structured Text expressions and statements",
            status: Partial,
            notes: "Assignments, calls, IF, CASE, FOR, WHILE, REPEAT, EXIT, RETURN, aggregate initializers, array indexing, structure field access, enum values, subrange checks, BOOL/bit-string AND/OR/XOR/NOT, BOOL AND/OR short-circuiting, checked integer arithmetic diagnostics, overflow-safe constant-expression checks, and TIME +/- TIME are supported for the current ST subset.",
        },
        ComplianceFeature {
            id: "language.ld",
            clause: "4.2",
            title: "Ladder Diagram",
            status: Partial,
            notes: "PLCopen XML LD nodes are preserved in normalized networks; simple contact-to-coil networks lower into normalized assignment IR; full power-flow semantics remain planned.",
        },
        ComplianceFeature {
            id: "language.fbd",
            clause: "4.3",
            title: "Function Block Diagram",
            status: Partial,
            notes: "PLCopen XML FBD nodes are preserved in normalized networks; simple function-block networks lower into normalized call/assignment IR; full data-flow connection semantics remain planned.",
        },
        ComplianceFeature {
            id: "plcopen.xml",
            clause: "IEC 61131-10 / PLCopen TC6",
            title: "PLCopen XML import/export",
            status: Partial,
            notes: "Project, POU, ST, IL, basic SFC structure, and LD/FBD node import/export are implemented without external XML dependencies.",
        },
        ComplianceFeature {
            id: "backend.interpreter",
            clause: "implementation",
            title: "Deterministic scan-cycle interpreter",
            status: Partial,
            notes: "Supported ST subset, aggregate values, user functions, user-defined FB flat state, IL accumulator operations, linear textual SFC, deterministic configuration task scheduling, and bistable, edge, counter, and timer FBs can execute with cycle traces.",
        },
        ComplianceFeature {
            id: "backend.c",
            clause: "implementation",
            title: "Portable C code generation",
            status: Partial,
            notes: "ST assignments/control flow, CASE, aggregate array/structure state, enum ordinals, subrange-backed scalar storage, fixed-capacity strings, common string helper calls, scalar typed conversion/TRUNC/BCD calls, date/time-of-day/date-time literal encodings, basic user functions, user-defined FB flat state, linear textual SFC state, IL accumulator operations, common standard FBs, and generated debug-symbol metadata emit to C; edge-case type polish remains planned.",
        },
        ComplianceFeature {
            id: "diagnostics",
            clause: "1.5 / Annex E",
            title: "Diagnostics and compliance reporting",
            status: Partial,
            notes: "Human and JSON diagnostics with stable RBCPP-* category codes, feature matrix, symbol errors, configuration errors, conversion range errors, and elementary type mismatch errors are implemented.",
        },
    ]
}
