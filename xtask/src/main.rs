// SPDX-License-Identifier: MIT OR Apache-2.0

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use iec_c::{generate_c, generate_c_with_options, COptions};
use iec_diagnostics::{Diagnostic, Severity};
use iec_ir::{LibraryElement, Project, Value};
use iec_plcopen::{
    export_plcopen_xml, import_plcopen_xml, import_plcopen_xml_with_options, PlcOpenImportOptions,
};
use iec_profile::ImplementationParameters;
use iec_runtime::{run_program, RuntimeOptions};
use iec_semantics::{check_project, CheckOptions};
use iec_syntax::{parse_project, parse_project_with_options, ParseOptions};

const HARDENING_MODULE_LINE_BUDGET: usize = 3_000;
const HARDENING_PRODUCTION_UNWRAP_EXPECT_BUDGET: usize = 29;
const HARDENING_IEC_C_UNWRAP_EXPECT_BUDGET: usize = 13;
const HARDENING_PLCOPEN_STRING_HELPER_BUDGET: usize = 0;
const HARDENING_GENERATED_C_TOTAL_BYTES_BUDGET: usize = 2_000_000;
const HARDENING_GENERATED_C_MAX_CASE_BYTES_BUDGET: usize = 250_000;
const HARDENING_GENERATED_C_ELAPSED_MS_BUDGET: u128 = 15_000;
const GRANDFATHERED_MONOLITHS: [&str; 0] = [];

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("validate-corpus") => validate_corpus(),
        Some("validate-differential") => validate_differential(),
        Some("validate-robustness") => validate_robustness(),
        Some("validate-sanitizers") => validate_sanitizers(),
        Some("fuzz-smoke") => fuzz_smoke(),
        Some("hardening-check") => hardening_check(),
        Some("release-report") => release_report(&args[1..]),
        Some("-h" | "--help" | "help") | None => {
            print_usage();
            Ok(())
        }
        Some(other) => Err(format!("unknown xtask command '{other}'")),
    }
}

fn print_usage() {
    eprintln!(
        "usage:\n  cargo run -p xtask -- validate-corpus\n  cargo run -p xtask -- validate-differential\n  cargo run -p xtask -- validate-robustness\n  cargo run -p xtask -- validate-sanitizers\n  cargo run -p xtask -- fuzz-smoke\n  cargo run -p xtask -- hardening-check\n  cargo run -p xtask -- release-report [--output PATH]"
    );
}

fn validate_corpus() -> Result<(), String> {
    let root = workspace_root();
    let validation = root.join("validation");
    let mut summary = ValidationSummary::default();

    validate_tree(
        &validation.join("corpus/accepted"),
        &mut summary,
        validate_accepted_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/rejected"),
        &mut summary,
        validate_rejected_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/runtime"),
        &mut summary,
        validate_runtime_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/c-parity"),
        &mut summary,
        validate_c_parity_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/plcopen/roundtrip"),
        &mut summary,
        validate_plcopen_roundtrip_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/plcopen/vendor"),
        &mut summary,
        validate_plcopen_vendor_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/plcopen/hostile"),
        &mut summary,
        validate_rejected_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/stress"),
        &mut summary,
        validate_accepted_fixture,
    )?;
    validate_tree(
        &validation.join("corpus/regressions"),
        &mut summary,
        validate_rejected_fixture,
    )?;

    if summary.fixtures == 0 {
        return Err("validation corpus did not contain any source fixtures".to_string());
    }

    println!(
        "validated {} corpus fixture(s) with {} expectation file(s)",
        summary.fixtures, summary.expectations
    );
    Ok(())
}

fn validate_tree(
    dir: &Path,
    summary: &mut ValidationSummary,
    validate: fn(&Path, &FixtureMetadata) -> Result<usize, String>,
) -> Result<(), String> {
    if !dir.exists() {
        return Err(format!("missing validation directory {}", dir.display()));
    }

    for path in source_files(dir)? {
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let metadata = parse_metadata(&path, &source)?;
        let _evidence_key = (&metadata.feature, &metadata.clause);
        let expectations = validate(&path, &metadata)?;
        summary.fixtures += 1;
        summary.expectations += expectations;
    }
    Ok(())
}

fn validate_accepted_fixture(path: &Path, _metadata: &FixtureMetadata) -> Result<usize, String> {
    let LoadedProject {
        project: _,
        diagnostics,
    } = load_and_check(path)?;
    expect_no_errors(path, &diagnostics)?;
    Ok(0)
}

fn validate_plcopen_vendor_fixture(
    path: &Path,
    _metadata: &FixtureMetadata,
) -> Result<usize, String> {
    let LoadedProject {
        project: _,
        diagnostics,
    } = load_and_check(path)?;
    if !sidecar_path(path, "diag").exists() {
        expect_no_errors(path, &diagnostics)?;
        return Ok(0);
    }

    let expectations = read_expectations(path, "diag")?;
    let rendered = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in &expectations {
        if !rendered.contains(expected) {
            return Err(format!(
                "{} missing expected vendor diagnostic substring {:?}\nactual diagnostics:\n{}",
                path.display(),
                expected,
                rendered
            ));
        }
    }
    Ok(expectations.len())
}

fn validate_rejected_fixture(path: &Path, _metadata: &FixtureMetadata) -> Result<usize, String> {
    let LoadedProject {
        project,
        mut diagnostics,
    } = load_project(path)?;
    if !has_error(&diagnostics) {
        diagnostics.extend(check_project(&project, &CheckOptions::default()));
    }
    if !has_error(&diagnostics) {
        return Err(format!(
            "{} is in a rejected corpus directory but produced no errors",
            path.display()
        ));
    }

    let expectations = read_expectations(path, "diag")?;
    let rendered = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in &expectations {
        if !rendered.contains(expected) {
            return Err(format!(
                "{} missing expected diagnostic substring {:?}\nactual diagnostics:\n{}",
                path.display(),
                expected,
                rendered
            ));
        }
    }
    Ok(expectations.len())
}

fn validate_runtime_fixture(path: &Path, metadata: &FixtureMetadata) -> Result<usize, String> {
    let LoadedProject {
        project,
        diagnostics,
    } = load_and_check(path)?;
    expect_no_errors(path, &diagnostics)?;

    let cycles = metadata.cycles.unwrap_or(1);
    let trace = run_program(
        &project,
        metadata.program.as_deref(),
        cycles,
        &RuntimeOptions::default(),
    )
    .map_err(|diagnostics| {
        format!(
            "runtime failed for {}:\n{}",
            path.display(),
            render_messages(&diagnostics)
        )
    })?;
    let actual = render_runtime_trace(&trace);
    let expected_path = sidecar_path(path, "trace");
    let expected = fs::read_to_string(&expected_path)
        .map_err(|err| format!("failed to read {}: {err}", expected_path.display()))?;
    if normalize_text(&actual) != normalize_text(&expected) {
        return Err(format!(
            "{} runtime trace mismatch\nexpected:\n{}\nactual:\n{}",
            path.display(),
            expected,
            actual
        ));
    }
    Ok(1)
}

fn validate_c_parity_fixture(path: &Path, metadata: &FixtureMetadata) -> Result<usize, String> {
    let LoadedProject {
        project,
        diagnostics,
    } = load_and_check(path)?;
    expect_no_errors(path, &diagnostics)?;
    let output = generate_c(&project, metadata.program.as_deref()).map_err(|diagnostics| {
        format!(
            "C generation failed for {}:\n{}",
            path.display(),
            render_messages(&diagnostics)
        )
    })?;
    compile_generated_c(path, &output.source)?;
    Ok(1)
}

fn validate_plcopen_roundtrip_fixture(
    path: &Path,
    _metadata: &FixtureMetadata,
) -> Result<usize, String> {
    if !has_extension(path, "xml") {
        return Err(format!(
            "{} is in plcopen/roundtrip but is not an XML fixture",
            path.display()
        ));
    }

    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let imported = import_plcopen_xml(&path.to_string_lossy(), &source);
    expect_no_errors(path, &imported.diagnostics)?;
    let exported = export_plcopen_xml(&imported.project);
    let reimported = import_plcopen_xml("roundtrip.xml", &exported);
    expect_no_errors(path, &reimported.diagnostics)?;

    let before = project_shape(&imported.project);
    let after = project_shape(&reimported.project);
    if before != after {
        return Err(format!(
            "{} PLCopen round-trip changed normalized project shape\nbefore:\n{}\nafter:\n{}",
            path.display(),
            before,
            after
        ));
    }
    Ok(1)
}

fn validate_differential() -> Result<(), String> {
    let mut checked = 0;
    for case in generated_differential_cases() {
        validate_differential_case(&case)?;
        checked += 1;
    }
    validate_metamorphic_cases()?;
    println!(
        "validated {checked} generated differential case(s) plus metamorphic equivalence cases"
    );
    Ok(())
}

fn validate_differential_case(case: &DifferentialCase) -> Result<(), String> {
    let parsed = parse_project(format!("{}.st", case.name), case.source);
    expect_no_errors(Path::new(case.name), &parsed.diagnostics)?;
    let diagnostics = check_project(&parsed.project, &CheckOptions::default());
    expect_no_errors(Path::new(case.name), &diagnostics)?;

    let trace = run_program(
        &parsed.project,
        Some(case.program),
        case.cycles,
        &RuntimeOptions::default(),
    )
    .map_err(|diagnostics| {
        format!(
            "{} interpreter failed:\n{}",
            case.name,
            render_messages(&diagnostics)
        )
    })?;
    let expected = render_probe_trace_from_runtime(&trace, &case.probes)?;

    let output = generate_c(&parsed.project, Some(case.program)).map_err(|diagnostics| {
        format!(
            "{} C generation failed:\n{}",
            case.name,
            render_messages(&diagnostics)
        )
    })?;
    let actual = compile_and_run_c(
        &format!("diff_{}", case.name),
        &differential_c_source(&output.source, case),
        Duration::from_secs(10),
    )?;

    if normalize_text(&expected) != normalize_text(&actual) {
        return Err(format!(
            "{} interpreter/C trace mismatch\nexpected:\n{}\nactual:\n{}",
            case.name, expected, actual
        ));
    }
    Ok(())
}

fn validate_metamorphic_cases() -> Result<(), String> {
    let pairs = [
        (
            "parentheses",
            r#"
PROGRAM Meta
VAR
    A : INT := 1;
    B : INT := 2;
    Out : INT := 0;
END_VAR
Out := A + B * 3;
END_PROGRAM
"#,
            r#"
PROGRAM Meta
VAR
    B : INT := 2;
    A : INT := 1;
    Out : INT := 0;
END_VAR
Out := (A + (B * 3));
END_PROGRAM
"#,
        ),
        (
            "formatting",
            "PROGRAM Meta\nVAR\nA:INT:=1;\nOut:INT:=0;\nEND_VAR\nOut:=A+1;\nEND_PROGRAM\n",
            "PROGRAM Meta\nVAR\n    A : INT := 1;\n    Out : INT := 0;\nEND_VAR\n\nOut := A + 1;\nEND_PROGRAM\n",
        ),
    ];

    let probes = [Probe {
        name: "OUT",
        c_expr: "s.out",
        kind: ProbeKind::Int,
    }];
    for (name, left, right) in pairs {
        let left_trace = interpreter_probe_trace(name, left, "Meta", 2, &probes)?;
        let right_trace = interpreter_probe_trace(name, right, "Meta", 2, &probes)?;
        if left_trace != right_trace {
            return Err(format!(
                "metamorphic case {name} changed behavior\nleft:\n{left_trace}\nright:\n{right_trace}"
            ));
        }
    }
    Ok(())
}

fn validate_robustness() -> Result<(), String> {
    let mut implementation = ImplementationParameters {
        max_source_bytes: 16,
        max_plcopen_xml_bytes: 32,
        max_pous: 1,
        max_variables: 1,
        max_symbols: 2,
        max_generated_c_bytes: 128,
        ..ImplementationParameters::default()
    };

    let source = "PROGRAM TooLarge\nEND_PROGRAM\n";
    let parsed = parse_project_with_options(
        "too_large.st",
        source,
        &ParseOptions {
            implementation: implementation.clone(),
        },
    );
    expect_diagnostic_contains("source size limit", &parsed.diagnostics, "source size")?;

    let xml = "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types /></project>";
    let imported = import_plcopen_xml_with_options(
        "too_large.xml",
        xml,
        &PlcOpenImportOptions {
            implementation: implementation.clone(),
        },
    );
    expect_diagnostic_contains("PLCopen XML size limit", &imported.diagnostics, "XML size")?;

    implementation.max_plcopen_xml_bytes = 1_048_576;
    implementation.max_plcopen_xml_nodes = 2;
    let nested_xml =
        "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types><pous /></types></project>";
    let imported = import_plcopen_xml_with_options(
        "too_many_nodes.xml",
        nested_xml,
        &PlcOpenImportOptions {
            implementation: implementation.clone(),
        },
    );
    expect_diagnostic_contains(
        "PLCopen XML node limit",
        &imported.diagnostics,
        "nodes limit",
    )?;

    implementation.max_plcopen_xml_nodes =
        ImplementationParameters::default().max_plcopen_xml_nodes;
    implementation.max_plcopen_xml_depth = 2;
    let imported = import_plcopen_xml_with_options(
        "too_deep.xml",
        nested_xml,
        &PlcOpenImportOptions {
            implementation: implementation.clone(),
        },
    );
    expect_diagnostic_contains(
        "PLCopen XML depth limit",
        &imported.diagnostics,
        "nesting depth",
    )?;

    implementation.max_plcopen_xml_depth =
        ImplementationParameters::default().max_plcopen_xml_depth;
    implementation.max_plcopen_xml_text_bytes = 4;
    let text_xml = "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\">abcdef</project>";
    let imported = import_plcopen_xml_with_options(
        "too_much_text.xml",
        text_xml,
        &PlcOpenImportOptions {
            implementation: implementation.clone(),
        },
    );
    expect_diagnostic_contains("PLCopen XML text limit", &imported.diagnostics, "text node")?;

    implementation.max_plcopen_xml_text_bytes =
        ImplementationParameters::default().max_plcopen_xml_text_bytes;
    implementation.max_plcopen_xml_attribute_bytes = 4;
    let attr_xml =
        "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types name=\"abcdef\" /></project>";
    let imported = import_plcopen_xml_with_options(
        "too_large_attr.xml",
        attr_xml,
        &PlcOpenImportOptions {
            implementation: implementation.clone(),
        },
    );
    expect_diagnostic_contains(
        "PLCopen XML attribute limit",
        &imported.diagnostics,
        "attribute 'name'",
    )?;

    implementation.max_plcopen_xml_attribute_bytes =
        ImplementationParameters::default().max_plcopen_xml_attribute_bytes;
    implementation.max_source_bytes = 1_048_576;
    let project_source = r#"
PROGRAM A
VAR
    One : INT;
    Two : INT;
END_VAR
END_PROGRAM

PROGRAM B
END_PROGRAM
"#;
    let parsed = parse_project("project_limits.st", project_source);
    expect_no_errors(Path::new("project_limits.st"), &parsed.diagnostics)?;
    let diagnostics = check_project(
        &parsed.project,
        &CheckOptions {
            implementation: implementation.clone(),
            ..CheckOptions::default()
        },
    );
    expect_diagnostic_contains("POU count limit", &diagnostics, "POU count")?;
    expect_diagnostic_contains(
        "variable count limit",
        &diagnostics,
        "variable declaration count",
    )?;
    expect_diagnostic_contains("symbol count limit", &diagnostics, "named symbol count")?;

    let parsed = parse_project(
        "generated_c_limit.st",
        "PROGRAM CLimit\nVAR\nOut : INT := 0;\nEND_VAR\nOut := Out + 1;\nEND_PROGRAM",
    );
    expect_no_errors(Path::new("generated_c_limit.st"), &parsed.diagnostics)?;
    let diagnostics = generate_c_with_options(
        &parsed.project,
        Some("CLimit"),
        &COptions {
            implementation: implementation.clone(),
        },
    )
    .expect_err("small generated-C limit should fail");
    expect_diagnostic_contains("generated C size limit", &diagnostics, "generated C size")?;

    run_timed("textual parse", Duration::from_secs(2), || {
        let parsed = parse_project("timed.st", generated_large_text_source(200).as_str());
        if has_error(&parsed.diagnostics) {
            Err(render_messages(&parsed.diagnostics))
        } else {
            Ok(())
        }
    })?;
    run_timed("PLCopen import", Duration::from_secs(2), || {
        let imported = import_plcopen_xml("timed.xml", &generated_large_plcopen_xml(100));
        if has_error(&imported.diagnostics) {
            Err(render_messages(&imported.diagnostics))
        } else {
            Ok(())
        }
    })?;
    run_timed("semantic analysis", Duration::from_secs(2), || {
        let parsed = parse_project(
            "timed_semantics.st",
            generated_large_text_source(200).as_str(),
        );
        let diagnostics = check_project(&parsed.project, &CheckOptions::default());
        if has_error(&diagnostics) {
            Err(render_messages(&diagnostics))
        } else {
            Ok(())
        }
    })?;
    run_timed("generated C", Duration::from_secs(2), || {
        let parsed = parse_project("timed_c.st", generated_large_text_source(200).as_str());
        generate_c(&parsed.project, Some("GeneratedLarge"))
            .map(|_| ())
            .map_err(|diagnostics| render_messages(&diagnostics))
    })?;

    let large_xml = generated_large_plcopen_xml(1000);
    if large_xml.len() < 100_000 {
        return Err("large PLCopen memory-growth fixture is unexpectedly small".to_string());
    }
    let imported = import_plcopen_xml("large.xml", &large_xml);
    expect_no_errors(Path::new("large.xml"), &imported.diagnostics)?;

    let large_source = generated_large_text_source(300);
    let parsed = parse_project("large_c.st", &large_source);
    expect_no_errors(Path::new("large_c.st"), &parsed.diagnostics)?;
    let output = generate_c(&parsed.project, Some("GeneratedLarge")).map_err(|diagnostics| {
        format!(
            "large generated-C memory-growth fixture failed:\n{}",
            render_messages(&diagnostics)
        )
    })?;
    if output.source.len() > ImplementationParameters::default().max_generated_c_bytes {
        return Err(format!(
            "large generated-C output {} bytes exceeds default maximum {}",
            output.source.len(),
            ImplementationParameters::default().max_generated_c_bytes
        ));
    }

    println!("validated explicit limits, timeout budgets, and large-input growth checks");
    Ok(())
}

fn validate_sanitizers() -> Result<(), String> {
    let fixtures = source_files(&workspace_root().join("validation/corpus/c-parity"))?;
    if fixtures.is_empty() {
        return Err("no C-parity fixtures available for sanitizer validation".to_string());
    }
    let mut checked = 0;
    for path in fixtures {
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let metadata = parse_metadata(&path, &source)?;
        let LoadedProject {
            project,
            diagnostics,
        } = load_and_check(&path)?;
        expect_no_errors(&path, &diagnostics)?;
        let program = metadata
            .program
            .as_deref()
            .or_else(|| {
                project
                    .first_program()
                    .map(|program| program.name.original.as_str())
            })
            .ok_or_else(|| format!("{} has no PROGRAM for sanitizer validation", path.display()))?;
        let output = generate_c(&project, Some(program)).map_err(|diagnostics| {
            format!(
                "{} C generation failed for sanitizer validation:\n{}",
                path.display(),
                render_messages(&diagnostics)
            )
        })?;
        compile_and_run_c_with_flags(
            &format!(
                "sanitize_{}",
                path.file_stem()
                    .and_then(OsStr::to_str)
                    .unwrap_or("fixture")
            ),
            &scan_only_c_source(&output.source, program, 3),
            &["-fsanitize=address,undefined", "-fno-omit-frame-pointer"],
            Duration::from_secs(10),
        )?;
        checked += 1;
    }
    println!("validated {checked} generated C fixture(s) with ASan/UBSan");
    Ok(())
}

fn fuzz_smoke() -> Result<(), String> {
    let textual_inputs = [
        "",
        "PROGRAM X\nVAR\nA : INT := 1;\nEND_VAR\nA := A + 1;\nEND_PROGRAM",
        "PROGRAM MissingEnd\nVAR\nA : INT;\nEND_VAR\nA := TRUE;",
        "PROGRAM Deep\nVAR\nA : INT;\nEND_VAR\nA := (((((((((((1))))))))))); END_PROGRAM",
        "LADDER\nRUNG A:\nCONTACT Start;\nCOIL Motor;\nEND_RUNG;\nEND_LADDER",
        "PROGRAM Bad\nVAR\nA : INT;\nEND_VAR\nIF THEN THEN A := 1; END_IF;\nEND_PROGRAM",
    ];
    for (index, source) in textual_inputs.iter().enumerate() {
        catch_task(format!("textual fuzz smoke input {index}"), || {
            let parsed = parse_project(format!("fuzz{index}.st"), source);
            if !has_error(&parsed.diagnostics) {
                let diagnostics = check_project(&parsed.project, &CheckOptions::default());
                if !has_error(&diagnostics) {
                    let _ = generate_c(&parsed.project, None);
                }
            }
        })?;
    }

    let xml_inputs = [
        "",
        "<project>",
        "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types /></project>",
        "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types><pous><pou name=\"P\" pouType=\"program\"><body><FBD /></body></pou></pous></types></project>",
        "<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types><pous><pou name=\"P\" pouType=\"program\"><body><LD><leftPowerRail localId=\"1\" /></LD></body></pou></pous></types></project>",
    ];
    for (index, source) in xml_inputs.iter().enumerate() {
        catch_task(format!("PLCopen fuzz smoke input {index}"), || {
            let imported = import_plcopen_xml(&format!("fuzz{index}.xml"), source);
            if !has_error(&imported.diagnostics) {
                let exported = export_plcopen_xml(&imported.project);
                let _ = import_plcopen_xml("roundtrip.xml", &exported);
            }
        })?;
    }

    println!(
        "fuzz smoke passed for {} textual input(s) and {} PLCopen XML input(s)",
        textual_inputs.len(),
        xml_inputs.len()
    );
    Ok(())
}

fn hardening_check() -> Result<(), String> {
    let root = workspace_root();
    let metrics = collect_hardening_metrics(&root)?;
    let mut failures = Vec::new();

    if let Some(module) = &metrics.largest_non_grandfathered_module {
        if module.lines > HARDENING_MODULE_LINE_BUDGET {
            failures.push(format!(
                "{} has {} line(s), above the hardening module budget of {}",
                module.path, module.lines, HARDENING_MODULE_LINE_BUDGET
            ));
        }
    }

    if metrics.production_unwrap_expect_count > HARDENING_PRODUCTION_UNWRAP_EXPECT_BUDGET {
        failures.push(format!(
            "production unwrap/expect count is {}, above the baseline budget of {}",
            metrics.production_unwrap_expect_count, HARDENING_PRODUCTION_UNWRAP_EXPECT_BUDGET
        ));
    }

    if metrics.iec_c_production_unwrap_expect_count > HARDENING_IEC_C_UNWRAP_EXPECT_BUDGET {
        failures.push(format!(
            "iec_c production unwrap/expect count is {}, above the baseline budget of {}",
            metrics.iec_c_production_unwrap_expect_count, HARDENING_IEC_C_UNWRAP_EXPECT_BUDGET
        ));
    }

    if metrics.plcopen_string_helper_count > HARDENING_PLCOPEN_STRING_HELPER_BUDGET {
        failures.push(format!(
            "PLCopen string-helper reference count is {}, above the baseline budget of {}",
            metrics.plcopen_string_helper_count, HARDENING_PLCOPEN_STRING_HELPER_BUDGET
        ));
    }

    if metrics.generated_c_total_bytes > HARDENING_GENERATED_C_TOTAL_BYTES_BUDGET {
        failures.push(format!(
            "generated-C benchmark output is {} byte(s), above the total budget of {}",
            metrics.generated_c_total_bytes, HARDENING_GENERATED_C_TOTAL_BYTES_BUDGET
        ));
    }

    if metrics.generated_c_max_case_bytes > HARDENING_GENERATED_C_MAX_CASE_BYTES_BUDGET {
        failures.push(format!(
            "largest generated-C benchmark case is {} byte(s), above the per-case budget of {}",
            metrics.generated_c_max_case_bytes, HARDENING_GENERATED_C_MAX_CASE_BYTES_BUDGET
        ));
    }

    if metrics.generated_c_elapsed_ms > HARDENING_GENERATED_C_ELAPSED_MS_BUDGET {
        failures.push(format!(
            "generated-C benchmark took {} ms, above the elapsed-time budget of {} ms",
            metrics.generated_c_elapsed_ms, HARDENING_GENERATED_C_ELAPSED_MS_BUDGET
        ));
    }

    if failures.is_empty() {
        print!("{}", render_hardening_metrics(&metrics));
        Ok(())
    } else {
        Err(format!(
            "{}\n\n{}",
            failures
                .iter()
                .map(|failure| format!("- {failure}"))
                .collect::<Vec<_>>()
                .join("\n"),
            render_hardening_metrics(&metrics)
        ))
    }
}

fn release_report(args: &[String]) -> Result<(), String> {
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--output" => {
                index += 1;
                output = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            other => return Err(format!("unknown release-report option '{other}'")),
        }
        index += 1;
    }

    let root = workspace_root();
    let validation = root.join("validation");
    let accepted_count = source_files(&validation.join("corpus/accepted"))?.len();
    let rejected_count = source_files(&validation.join("corpus/rejected"))?.len();
    let runtime_count = source_files(&validation.join("corpus/runtime"))?.len();
    let c_parity_count = source_files(&validation.join("corpus/c-parity"))?.len();
    let plcopen_count = source_files(&validation.join("corpus/plcopen"))?.len();
    let stress_count = source_files(&validation.join("corpus/stress"))?.len();
    let regression_dir = validation.join("corpus/regressions");
    let regression_count = source_files(&regression_dir)?.len();
    let (fuzz_regression_count, non_fuzz_regression_count) =
        regression_origin_counts(&regression_dir)?;
    let commercial_external_count =
        commercial_external_run_count(&validation.join("differential/external-runs.md"))?;
    let differential_count = generated_differential_cases().len();
    let source_count = accepted_count
        + rejected_count
        + runtime_count
        + c_parity_count
        + plcopen_count
        + stress_count
        + regression_count;
    let fuzz_target_count = count_files_with_extension(&root.join("fuzz/fuzz_targets"), "rs")?;
    let hardening_metrics = collect_hardening_metrics(&root)?;
    let hardening_report = render_hardening_metrics(&hardening_metrics);
    let status = fs::read_to_string(validation.join("STATUS.toml"))
        .unwrap_or_else(|_| "status file unavailable".to_string());
    let report = format!(
        "# RoboC++ Release Validation Report\n\n- Git commit: `{}`\n- Rust toolchain: `{}`\n- Host: `{}/{}`\n- CI workflow: `.github/workflows/rust.yml`\n- Scheduled workflow: `.github/workflows/scheduled-validation.yml`\n- Corpus source fixtures: `{}`\n- Accepted fixtures: `{}`\n- Rejected fixtures: `{}`\n- Runtime trace fixtures: `{}`\n- Generated-C fixtures: `{}`\n- PLCopen fixtures: `{}`\n- Stress fixtures: `{}`\n- Regression fixtures: `{}`\n- Fuzz-discovered regression fixtures: `{}`\n- Non-fuzz regression fixtures: `{}`\n- Generated differential cases: `{}`\n- Commercial external differential runs recorded: `{}`\n- Fuzz targets: `{}`\n- Required commands:\n  - `cargo fmt --check`\n  - `cargo clippy --workspace --all-targets -- -D warnings`\n  - `cargo check --workspace`\n  - `cargo test --workspace`\n  - `cargo run -p xtask -- hardening-check`\n  - `cargo run -p xtask -- validate-corpus`\n  - `cargo run -p xtask -- validate-differential`\n  - `cargo run -p xtask -- validate-robustness`\n  - `cargo run -p xtask -- validate-sanitizers`\n  - `cargo run -p xtask -- fuzz-smoke`\n  - scheduled `cargo +nightly fuzz run <target> -- -max_total_time=1800`\n\n{}\n\n## Versioned Readiness Status\n\n```toml\n{}```\n\n## Known Production-Readiness Scope\n\nRoboC++ is not safety-certified. Target deployment validation, tool qualification, hazard analysis, and certification evidence remain the responsibility of the deploying organization. Release notes must link to this report before publishing compiler-readiness claims.\n",
        command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string()),
        command_output("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string()),
        std::env::consts::OS,
        std::env::consts::ARCH,
        source_count,
        accepted_count,
        rejected_count,
        runtime_count,
        c_parity_count,
        plcopen_count,
        stress_count,
        regression_count,
        fuzz_regression_count,
        non_fuzz_regression_count,
        differential_count,
        commercial_external_count,
        fuzz_target_count,
        hardening_report.trim_end(),
        status
    );

    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        }
        fs::write(&path, report)
            .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
        println!("{}", path.display());
    } else {
        print!("{report}");
    }
    Ok(())
}

#[derive(Clone)]
struct ModuleLineCount {
    path: String,
    lines: usize,
}

struct HardeningMetrics {
    monolith_line_counts: Vec<ModuleLineCount>,
    largest_non_grandfathered_module: Option<ModuleLineCount>,
    production_unwrap_expect_count: usize,
    iec_c_production_unwrap_expect_count: usize,
    plcopen_string_helper_count: usize,
    generated_c_case_count: usize,
    generated_c_total_bytes: usize,
    generated_c_max_case_bytes: usize,
    generated_c_elapsed_ms: u128,
}

fn collect_hardening_metrics(root: &Path) -> Result<HardeningMetrics, String> {
    let monolith_line_counts = GRANDFATHERED_MONOLITHS
        .iter()
        .map(|relative| {
            let path = root.join(relative);
            let lines = count_lines(&path)?;
            Ok(ModuleLineCount {
                path: (*relative).to_string(),
                lines,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let rust_files = rust_source_files(&root.join("crates"))?;
    let largest_non_grandfathered_module = rust_files
        .iter()
        .filter(|path| !is_test_helper_file(path))
        .filter_map(|path| {
            let relative = relative_path_string(root, path);
            if GRANDFATHERED_MONOLITHS.contains(&relative.as_str()) {
                return None;
            }
            count_lines(path).ok().map(|lines| ModuleLineCount {
                path: relative,
                lines,
            })
        })
        .max_by_key(|module| module.lines);

    let mut production_unwrap_expect_count = 0;
    for path in &rust_files {
        if is_test_helper_file(path) {
            continue;
        }
        let source = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let production = production_rust_section(&source);
        production_unwrap_expect_count += count_occurrences(production, ".unwrap(");
        production_unwrap_expect_count += count_occurrences(production, ".expect(");
    }

    let mut iec_c_production_unwrap_expect_count = 0;
    for path in rust_source_files(&root.join("crates/iec_c/src"))? {
        if is_test_helper_file(&path) {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let production = production_rust_section(&source);
        iec_c_production_unwrap_expect_count += count_occurrences(production, ".unwrap(");
        iec_c_production_unwrap_expect_count += count_occurrences(production, ".expect(");
    }

    let mut plcopen_string_helper_count = 0;
    for path in rust_source_files(&root.join("crates/iec_plcopen/src"))? {
        if is_test_helper_file(&path) {
            continue;
        }
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let production = production_rust_section(&source);
        plcopen_string_helper_count += ["extract_tag", "xml_elements", "strip_xml_tags", "attr"]
            .iter()
            .map(|name| count_identifier_calls(production, name))
            .sum::<usize>();
    }

    let generated_c = measure_generated_c_budget()?;

    Ok(HardeningMetrics {
        monolith_line_counts,
        largest_non_grandfathered_module,
        production_unwrap_expect_count,
        iec_c_production_unwrap_expect_count,
        plcopen_string_helper_count,
        generated_c_case_count: generated_c.case_count,
        generated_c_total_bytes: generated_c.total_bytes,
        generated_c_max_case_bytes: generated_c.max_case_bytes,
        generated_c_elapsed_ms: generated_c.elapsed_ms,
    })
}

fn render_hardening_metrics(metrics: &HardeningMetrics) -> String {
    let mut report = String::new();
    report.push_str("## Compiler Hardening Metrics\n\n");
    report.push_str("### Monolith Line Counts\n\n");
    if metrics.monolith_line_counts.is_empty() {
        report
            .push_str("- none; all previously grandfathered compiler monoliths have been split\n");
    } else {
        for module in &metrics.monolith_line_counts {
            report.push_str(&format!(
                "- `{}`: `{}` line(s)\n",
                module.path, module.lines
            ));
        }
    }

    report.push_str("\n### Guarded Budgets\n\n");
    if let Some(module) = &metrics.largest_non_grandfathered_module {
        report.push_str(&format!(
            "- Largest non-grandfathered Rust source module: `{}` with `{}` line(s) (budget `{}`)\n",
            module.path, module.lines, HARDENING_MODULE_LINE_BUDGET
        ));
    }
    report.push_str(&format!(
        "- Production `.unwrap()`/`.expect()` sites outside test helpers: `{}` (baseline budget `{}`)\n",
        metrics.production_unwrap_expect_count, HARDENING_PRODUCTION_UNWRAP_EXPECT_BUDGET
    ));
    report.push_str(&format!(
        "- `iec_c` production `.unwrap()`/`.expect()` sites: `{}` (baseline budget `{}`)\n",
        metrics.iec_c_production_unwrap_expect_count, HARDENING_IEC_C_UNWRAP_EXPECT_BUDGET
    ));
    report.push_str(&format!(
        "- PLCopen string-helper references in production lowering: `{}` (baseline budget `{}`)\n",
        metrics.plcopen_string_helper_count, HARDENING_PLCOPEN_STRING_HELPER_BUDGET
    ));
    report.push_str(&format!(
        "- Generated-C benchmark cases: `{}`; total output `{}` byte(s); largest case `{}` byte(s); elapsed `{}` ms\n",
        metrics.generated_c_case_count,
        metrics.generated_c_total_bytes,
        metrics.generated_c_max_case_bytes,
        metrics.generated_c_elapsed_ms
    ));
    report.push_str(&format!(
        "- Generated-C budgets: total `{}` byte(s), per-case `{}` byte(s), elapsed `{}` ms\n",
        HARDENING_GENERATED_C_TOTAL_BYTES_BUDGET,
        HARDENING_GENERATED_C_MAX_CASE_BYTES_BUDGET,
        HARDENING_GENERATED_C_ELAPSED_MS_BUDGET
    ));
    report
}

struct GeneratedCBudgetMetrics {
    case_count: usize,
    total_bytes: usize,
    max_case_bytes: usize,
    elapsed_ms: u128,
}

fn measure_generated_c_budget() -> Result<GeneratedCBudgetMetrics, String> {
    let started = Instant::now();
    let mut case_count = 0;
    let mut total_bytes = 0;
    let mut max_case_bytes = 0;

    for case in generated_differential_cases() {
        let parsed = parse_project(format!("hardening-{}.st", case.name), case.source);
        expect_no_errors(Path::new(case.name), &parsed.diagnostics)?;
        let diagnostics = check_project(&parsed.project, &CheckOptions::default());
        expect_no_errors(Path::new(case.name), &diagnostics)?;
        let output = generate_c(&parsed.project, Some(case.program)).map_err(|diagnostics| {
            format!(
                "generated-C budget case '{}' failed:\n{}",
                case.name,
                render_messages(&diagnostics)
            )
        })?;
        case_count += 1;
        total_bytes += output.source.len();
        max_case_bytes = max_case_bytes.max(output.source.len());
    }

    Ok(GeneratedCBudgetMetrics {
        case_count,
        total_bytes,
        max_case_bytes,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn count_lines(path: &Path) -> Result<usize, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(source.lines().count())
}

fn rust_source_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_with_extension(dir, "rs", &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_with_extension(
    dir: &Path,
    extension: &str,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_extension(&path, extension, files)?;
        } else if has_extension(&path, extension) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_test_helper_file(path: &Path) -> bool {
    if path.file_name() == Some(OsStr::new("tests.rs")) {
        return true;
    }
    path.components()
        .any(|component| component.as_os_str() == OsStr::new("tests"))
}

fn production_rust_section(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn count_identifier_calls(source: &str, name: &str) -> usize {
    source
        .match_indices(name)
        .filter(|(index, _)| {
            let before = source[..*index].chars().next_back();
            let after = source[*index + name.len()..].chars().next();
            !before.is_some_and(is_rust_identifier_char) && after == Some('(')
        })
        .count()
}

fn is_rust_identifier_char(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

fn relative_path_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[derive(Clone)]
struct DifferentialCase {
    name: &'static str,
    source: &'static str,
    program: &'static str,
    cycles: usize,
    probes: Vec<Probe>,
}

#[derive(Clone)]
struct Probe {
    name: &'static str,
    c_expr: &'static str,
    kind: ProbeKind,
}

#[derive(Clone, Copy)]
enum ProbeKind {
    Bool,
    Int,
}

fn generated_differential_cases() -> Vec<DifferentialCase> {
    vec![
        diff_case(
            "arithmetic",
            r#"
PROGRAM GeneratedArithmetic
VAR
    A : INT := 2;
    B : INT := 3;
    Out : INT := 0;
END_VAR
Out := (A + B) * 4 - 1;
END_PROGRAM
"#,
            "GeneratedArithmetic",
            2,
            vec![int_probe("OUT", "s.out")],
        ),
        diff_case(
            "conversions",
            r#"
PROGRAM GeneratedConversions
VAR
    Out : INT := 0;
END_VAR
Out := STRING_TO_INT('42') + TRUNC(-1.6);
END_PROGRAM
"#,
            "GeneratedConversions",
            1,
            vec![int_probe("OUT", "s.out")],
        ),
        diff_case(
            "arrays",
            r#"
PROGRAM GeneratedArrays
VAR
    Values : ARRAY [1..3] OF INT := [1, 2, 3];
    Out : INT := 0;
END_VAR
Values[2] := Values[1] + Values[3];
Out := Values[2];
END_PROGRAM
"#,
            "GeneratedArrays",
            2,
            vec![int_probe("OUT", "s.out")],
        ),
        diff_case(
            "structs",
            r#"
TYPE
    Pair : STRUCT
        Low : INT := 1;
        High : INT := 2;
    END_STRUCT;
END_TYPE

PROGRAM GeneratedStructs
VAR
    Window : Pair := (Low := 4, High := 6);
    Out : INT := 0;
END_VAR
Window.Low := Window.High + 1;
Out := Window.Low + Window.High;
END_PROGRAM
"#,
            "GeneratedStructs",
            1,
            vec![int_probe("OUT", "s.out")],
        ),
        diff_case(
            "strings",
            r#"
PROGRAM GeneratedStrings
VAR
    Out : INT := 0;
END_VAR
Out := LEN(CONCAT('A', 'BC'));
END_PROGRAM
"#,
            "GeneratedStrings",
            1,
            vec![int_probe("OUT", "s.out")],
        ),
        diff_case(
            "timers",
            r#"
PROGRAM GeneratedTimers
VAR
    Delay : TON;
    Done : BOOL := FALSE;
END_VAR
Delay(IN := TRUE, PT := T#2ms);
Done := Delay.Q;
END_PROGRAM
"#,
            "GeneratedTimers",
            4,
            vec![bool_probe("DONE", "s.done")],
        ),
        diff_case(
            "counters",
            r#"
PROGRAM GeneratedCounters
VAR
    Counter : CTU;
    Pulse : BOOL := TRUE;
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR
Counter(CU := Pulse, R := FALSE, PV := 2);
Count := Counter.CV;
Done := Counter.Q;
Pulse := NOT Pulse;
END_PROGRAM
"#,
            "GeneratedCounters",
            4,
            vec![int_probe("COUNT", "s.count"), bool_probe("DONE", "s.done")],
        ),
        diff_case(
            "sfc",
            r#"
PROGRAM GeneratedSfc
VAR
    Ready : BOOL := TRUE;
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR

INITIAL_STEP Start:
    CountAction(N);
END_STEP;
Running: STEP
    DoneAction(N);
END_STEP;
ToRun: TRANSITION FROM Start TO Running := Ready;
END_TRANSITION;
CountAction: ACTION
    Count := Count + 1;
END_ACTION;
DoneAction: ACTION
    Done := TRUE;
END_ACTION;
END_PROGRAM
"#,
            "GeneratedSfc",
            3,
            vec![int_probe("COUNT", "s.count"), bool_probe("DONE", "s.done")],
        ),
        diff_case(
            "instruction_list",
            r#"
PROGRAM GeneratedIl
VAR
    A : INT := 3;
    B : INT := 4;
    Out : INT := 0;
    Bigger : BOOL := FALSE;
END_VAR
LD A
ADD B
ST Out
LD Out
GT 5
ST Bigger
END_PROGRAM
"#,
            "GeneratedIl",
            1,
            vec![int_probe("OUT", "s.out"), bool_probe("BIGGER", "s.bigger")],
        ),
        diff_case(
            "ladder",
            r#"
PROGRAM GeneratedLadder
VAR
    Start : BOOL := TRUE;
    Stop : BOOL := FALSE;
    Out : BOOL := FALSE;
END_VAR
LADDER
RUNG Motor:
    CONTACT Start;
    CONTACT_NOT Stop;
    COIL Out;
END_RUNG;
END_LADDER
END_PROGRAM
"#,
            "GeneratedLadder",
            1,
            vec![bool_probe("OUT", "s.out")],
        ),
        diff_case(
            "fbd",
            r#"
PROGRAM GeneratedFbd
VAR
    A : INT := 2;
    B : INT := 3;
    Out : INT := 0;
END_VAR
FBD
NETWORK Arithmetic:
    OUT Out := A + B;
END_NETWORK;
END_FBD
END_PROGRAM
"#,
            "GeneratedFbd",
            1,
            vec![int_probe("OUT", "s.out")],
        ),
        diff_case(
            "tasks_resources",
            r#"
PROGRAM GeneratedTasks
VAR
    Count : INT := 0;
END_VAR
Count := Count + 1;
END_PROGRAM

CONFIGURATION Plant
RESOURCE Cpu ON PLC
    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
    PROGRAM Main WITH Fast : GeneratedTasks;
END_RESOURCE
END_CONFIGURATION
"#,
            "GeneratedTasks",
            2,
            vec![int_probe("COUNT", "s.count")],
        ),
    ]
}

fn diff_case(
    name: &'static str,
    source: &'static str,
    program: &'static str,
    cycles: usize,
    probes: Vec<Probe>,
) -> DifferentialCase {
    DifferentialCase {
        name,
        source,
        program,
        cycles,
        probes,
    }
}

fn int_probe(name: &'static str, c_expr: &'static str) -> Probe {
    Probe {
        name,
        c_expr,
        kind: ProbeKind::Int,
    }
}

fn bool_probe(name: &'static str, c_expr: &'static str) -> Probe {
    Probe {
        name,
        c_expr,
        kind: ProbeKind::Bool,
    }
}

fn interpreter_probe_trace(
    name: &str,
    source: &str,
    program: &str,
    cycles: usize,
    probes: &[Probe],
) -> Result<String, String> {
    let parsed = parse_project(format!("{name}.st"), source);
    expect_no_errors(Path::new(name), &parsed.diagnostics)?;
    let diagnostics = check_project(&parsed.project, &CheckOptions::default());
    expect_no_errors(Path::new(name), &diagnostics)?;
    let trace = run_program(
        &parsed.project,
        Some(program),
        cycles,
        &RuntimeOptions::default(),
    )
    .map_err(|diagnostics| format!("{name} failed:\n{}", render_messages(&diagnostics)))?;
    render_probe_trace_from_runtime(&trace, probes)
}

fn render_probe_trace_from_runtime(
    trace: &iec_runtime::RuntimeTrace,
    probes: &[Probe],
) -> Result<String, String> {
    let mut lines = Vec::new();
    for cycle in &trace.cycles {
        lines.push(format!("cycle {}", cycle.cycle));
        for probe in probes {
            let value = cycle
                .variables
                .iter()
                .find(|(name, _)| name == probe.name)
                .map(|(_, value)| value)
                .ok_or_else(|| format!("runtime trace missing variable {}", probe.name))?;
            lines.push(format!(
                "{} = {}",
                probe.name,
                render_probe_value(value, probe.kind)
            ));
        }
    }
    Ok(lines.join("\n") + "\n")
}

fn render_probe_value(value: &Value, kind: ProbeKind) -> String {
    match (value, kind) {
        (Value::Bool(value), ProbeKind::Bool) => {
            if *value {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        (Value::Int(value), ProbeKind::Int) => value.to_string(),
        (other, _) => other.to_string(),
    }
}

fn differential_c_source(generated: &str, case: &DifferentialCase) -> String {
    let ident = c_ident(case.program);
    let mut main =
        format!("{generated}\nint main(void) {{\n    {ident}_state s;\n    {ident}_init(&s);\n");
    for cycle in 0..case.cycles {
        main.push_str(&format!(
            "    {ident}_scan(&s);\n    printf(\"cycle {cycle}\\n\");\n"
        ));
        for probe in &case.probes {
            match probe.kind {
                ProbeKind::Bool => main.push_str(&format!(
                    "    printf(\"{} = %s\\n\", ({}) ? \"TRUE\" : \"FALSE\");\n",
                    probe.name, probe.c_expr
                )),
                ProbeKind::Int => main.push_str(&format!(
                    "    printf(\"{} = %lld\\n\", (long long)({}));\n",
                    probe.name, probe.c_expr
                )),
            }
        }
    }
    main.push_str("    return 0;\n}\n");
    main
}

fn scan_only_c_source(generated: &str, program: &str, cycles: usize) -> String {
    let ident = c_ident(program);
    let mut main =
        format!("{generated}\nint main(void) {{\n    {ident}_state s;\n    {ident}_init(&s);\n");
    for _ in 0..cycles {
        main.push_str(&format!("    {ident}_scan(&s);\n"));
    }
    main.push_str("    return 0;\n}\n");
    main
}

fn c_ident(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

fn compile_and_run_c(test_name: &str, source: &str, timeout: Duration) -> Result<String, String> {
    compile_and_run_c_with_flags(test_name, source, &[], timeout)
}

fn compile_and_run_c_with_flags(
    test_name: &str,
    source: &str,
    flags: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let dir = env::temp_dir().join(format!("rbcpp_xtask_{test_name}_{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;
    let source_path = dir.join(format!("{test_name}.c"));
    let binary_path = dir.join(test_name);
    fs::write(&source_path, source)
        .map_err(|err| format!("failed to write {}: {err}", source_path.display()))?;

    let mut compile = Command::new("cc");
    compile
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg(&source_path)
        .arg("-lm")
        .arg("-o")
        .arg(&binary_path);
    for flag in flags {
        compile.arg(flag);
    }
    let output = run_command_with_timeout(compile, timeout)?;
    if !output.status.success() {
        return Err(format!(
            "C compile failed for {test_name}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let output = run_command_with_timeout(Command::new(&binary_path), timeout)?;
    if !output.status.success() {
        return Err(format!(
            "C run failed for {test_name}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("C output was not UTF-8: {err}"))
}

fn run_command_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to spawn command: {err}"))?;
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|err| format!("failed to wait for command: {err}"))?
            .is_some()
        {
            return child
                .wait_with_output()
                .map_err(|err| format!("failed to collect command output: {err}"));
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(format!(
                "command exceeded timeout of {}s",
                timeout.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn expect_diagnostic_contains(
    context: &str,
    diagnostics: &[Diagnostic],
    expected: &str,
) -> Result<(), String> {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(expected))
    {
        Ok(())
    } else {
        Err(format!(
            "{context} missing diagnostic substring {expected:?}\nactual diagnostics:\n{}",
            render_messages(diagnostics)
        ))
    }
}

fn run_timed(
    name: &str,
    max_duration: Duration,
    task: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let start = Instant::now();
    let result = task();
    let elapsed = start.elapsed();
    if elapsed > max_duration {
        return Err(format!(
            "{name} took {:.3}s, exceeding {:.3}s",
            elapsed.as_secs_f64(),
            max_duration.as_secs_f64()
        ));
    }
    result
}

fn generated_large_text_source(variable_count: usize) -> String {
    let mut source = String::from("PROGRAM GeneratedLarge\nVAR\n");
    for index in 0..variable_count {
        source.push_str(&format!("    V{index} : INT := {index};\n"));
    }
    source.push_str("    Out : INT := 0;\nEND_VAR\nOut := 0");
    for index in 0..variable_count {
        source.push_str(&format!(" + V{index}"));
    }
    source.push_str(";\nEND_PROGRAM\n");
    source
}

fn generated_large_plcopen_xml(pou_count: usize) -> String {
    let mut xml =
        String::from("<project xmlns=\"http://www.plcopen.org/xml/tc6_0201\"><types><pous>");
    for index in 0..pou_count {
        xml.push_str(&format!(
            "<pou name=\"P{index}\" pouType=\"program\"><interface><localVars><variable name=\"A\"><type><INT /></type></variable></localVars></interface><body><ST><xhtml:p xmlns:xhtml=\"http://www.w3.org/1999/xhtml\"><![CDATA[A := A + {index};]]></xhtml:p></ST></body></pou>"
        ));
    }
    xml.push_str("</pous></types></project>");
    xml
}

#[derive(Default)]
struct ValidationSummary {
    fixtures: usize,
    expectations: usize,
}

#[derive(Debug)]
struct FixtureMetadata {
    feature: String,
    clause: String,
    origin: Option<String>,
    cycles: Option<usize>,
    program: Option<String>,
}

struct LoadedProject {
    project: Project,
    diagnostics: Vec<Diagnostic>,
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask should live under the workspace root")
        .to_path_buf()
}

fn source_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_source_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_source_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, files)?;
        } else if is_source_fixture(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_source_fixture(path: &Path) -> bool {
    ["st", "il", "sfc", "ld", "fbd", "xml"]
        .iter()
        .any(|extension| has_extension(path, extension))
}

fn count_files_with_extension(dir: &Path, extension: &str) -> Result<usize, String> {
    if !dir.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            count += count_files_with_extension(&path, extension)?;
        } else if has_extension(&path, extension) {
            count += 1;
        }
    }
    Ok(count)
}

fn regression_origin_counts(dir: &Path) -> Result<(usize, usize), String> {
    let mut fuzz = 0;
    let mut non_fuzz = 0;
    for path in source_files(dir)? {
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let metadata = parse_metadata(&path, &source)?;
        if metadata.origin.as_deref() == Some("fuzz") {
            fuzz += 1;
        } else {
            non_fuzz += 1;
        }
    }
    Ok((fuzz, non_fuzz))
}

fn commercial_external_run_count(path: &Path) -> Result<usize, String> {
    if !path.exists() {
        return Ok(0);
    }
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(source
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter(|line| !line.contains("---"))
        .filter(|line| !line.contains("Date | Tool"))
        .filter(|line| !line.contains("RoboC++ internal generated suite"))
        .count())
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(extension))
}

fn parse_metadata(path: &Path, source: &str) -> Result<FixtureMetadata, String> {
    let line = source
        .lines()
        .take(20)
        .find(|line| line.contains("validation:"))
        .ok_or_else(|| {
            format!(
                "{} is missing a validation metadata comment with feature=... and clause=...",
                path.display()
            )
        })?;
    let payload = line
        .split_once("validation:")
        .map(|(_, right)| right)
        .unwrap_or_default();
    let mut feature = None;
    let mut clause = None;
    let mut origin = None;
    let mut cycles = None;
    let mut program = None;

    for raw in payload.split_whitespace() {
        let token = raw.trim_matches(|ch: char| matches!(ch, '*' | ')' | '-' | '>' | ';'));
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        match key {
            "feature" => feature = Some(value.to_string()),
            "clause" => clause = Some(value.to_string()),
            "origin" => origin = Some(value.to_string()),
            "cycles" => {
                cycles = Some(value.parse::<usize>().map_err(|_| {
                    format!("{} has invalid cycles value {value:?}", path.display())
                })?)
            }
            "program" => program = Some(value.to_string()),
            _ => {}
        }
    }

    Ok(FixtureMetadata {
        feature: feature
            .ok_or_else(|| format!("{} metadata missing feature=...", path.display()))?,
        clause: clause.ok_or_else(|| format!("{} metadata missing clause=...", path.display()))?,
        origin,
        cycles,
        program,
    })
}

fn load_project(path: &Path) -> Result<LoadedProject, String> {
    let source = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    if has_extension(path, "xml") {
        let imported = import_plcopen_xml(&path.to_string_lossy(), &source);
        Ok(LoadedProject {
            project: imported.project,
            diagnostics: imported.diagnostics,
        })
    } else {
        let parsed = parse_project(path.to_string_lossy(), &source);
        Ok(LoadedProject {
            project: parsed.project,
            diagnostics: parsed.diagnostics,
        })
    }
}

fn load_and_check(path: &Path) -> Result<LoadedProject, String> {
    let mut loaded = load_project(path)?;
    if !has_error(&loaded.diagnostics) {
        loaded
            .diagnostics
            .extend(check_project(&loaded.project, &CheckOptions::default()));
    }
    Ok(loaded)
}

fn has_error(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}

fn expect_no_errors(path: &Path, diagnostics: &[Diagnostic]) -> Result<(), String> {
    if has_error(diagnostics) {
        return Err(format!(
            "{} produced unexpected diagnostics:\n{}",
            path.display(),
            render_messages(diagnostics)
        ));
    }
    Ok(())
}

fn render_messages(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_expectations(path: &Path, extension: &str) -> Result<Vec<String>, String> {
    let sidecar = sidecar_path(path, extension);
    let source = fs::read_to_string(&sidecar)
        .map_err(|err| format!("failed to read {}: {err}", sidecar.display()))?;
    let expectations = source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if expectations.is_empty() {
        return Err(format!(
            "{} does not contain expectations",
            sidecar.display()
        ));
    }
    Ok(expectations)
}

fn sidecar_path(path: &Path, extension: &str) -> PathBuf {
    let mut sidecar = path.to_path_buf();
    sidecar.set_extension(extension);
    sidecar
}

fn normalize_text(input: &str) -> String {
    input
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn render_runtime_trace(trace: &iec_runtime::RuntimeTrace) -> String {
    let mut lines = Vec::new();
    for cycle in &trace.cycles {
        lines.push(format!("cycle {}", cycle.cycle));
        for (name, value) in &cycle.variables {
            lines.push(format!("{name} = {value}"));
        }
    }
    lines.join("\n") + "\n"
}

fn compile_generated_c(path: &Path, source: &str) -> Result<(), String> {
    let temp_dir = env::temp_dir().join(format!("rbcpp_xtask_{}", std::process::id()));
    fs::create_dir_all(&temp_dir)
        .map_err(|err| format!("failed to create {}: {err}", temp_dir.display()))?;
    let source_path = temp_dir.join("generated.c");
    let object_path = temp_dir.join("generated.o");
    fs::write(&source_path, source)
        .map_err(|err| format!("failed to write {}: {err}", source_path.display()))?;
    let output = Command::new("cc")
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-c")
        .arg(&source_path)
        .arg("-o")
        .arg(&object_path)
        .output()
        .map_err(|err| format!("failed to invoke cc for {}: {err}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "generated C for {} failed to compile:\n{}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn project_shape(project: &Project) -> String {
    project
        .library_elements
        .iter()
        .map(|element| match element {
            LibraryElement::DataType(data_type) => {
                format!("TYPE {}", data_type.name.original)
            }
            LibraryElement::Pou(pou) => format!("POU {:?} {}", pou.kind, pou.name.original),
            LibraryElement::Configuration(configuration) => {
                format!("CONFIGURATION {}", configuration.name.original)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn catch_task(name: String, task: impl FnOnce()) -> Result<(), String> {
    catch_unwind(AssertUnwindSafe(task)).map_err(|_| format!("{name} panicked"))
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
