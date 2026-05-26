use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use iec_c::generate_c;
use iec_diagnostics::{diagnostics_to_json, json_escape, render_diagnostics, Diagnostic};
use iec_ir::{LibraryElement, Project, Value};
use iec_plcopen::{export_plcopen_xml, import_plcopen_xml};
use iec_profile::{ComplianceMatrix, EditionProfile};
use iec_runtime::{run_configuration, run_program, RuntimeOptions};
use iec_semantics::{check_project, CheckOptions};
use iec_syntax::parse_project;

fn main() -> ExitCode {
    match run_cli(env::args().collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_cli(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.get(1).map(String::as_str) else {
        print_usage();
        return Ok(());
    };

    match command {
        "check" => command_check(&args[2..]),
        "run" => command_run(&args[2..]),
        "build-c" => command_build_c(&args[2..]),
        "import-plcopen" => command_import_plcopen(&args[2..]),
        "export-plcopen" => command_export_plcopen(&args[2..]),
        "compliance" => command_compliance(&args[2..]),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        other => Err(format!("unknown command '{other}'")),
    }
}

fn command_check(args: &[String]) -> Result<(), String> {
    let options = CliOptions::parse(args)?;
    let input = options.input_path()?;
    let LoadedProject {
        project,
        mut diagnostics,
    } = load_project(&input, options.profile)?;

    if diagnostics.is_empty() {
        diagnostics.extend(check_project(
            &project,
            &CheckOptions {
                profile: options.profile,
                ..CheckOptions::default()
            },
        ));
    }

    print_diagnostics(&diagnostics, options.json);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == iec_diagnostics::Severity::Error)
    {
        Err("check failed".to_string())
    } else {
        Ok(())
    }
}

fn command_run(args: &[String]) -> Result<(), String> {
    let options = CliOptions::parse(args)?;
    let input = options.input_path()?;
    let LoadedProject {
        project,
        mut diagnostics,
    } = load_project(&input, options.profile)?;
    diagnostics.extend(check_project(
        &project,
        &CheckOptions {
            profile: options.profile,
            ..CheckOptions::default()
        },
    ));
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == iec_diagnostics::Severity::Error)
    {
        print_diagnostics(&diagnostics, options.json);
        return Err("cannot run program with errors".to_string());
    }

    if options.configuration.is_some()
        || (options.program.is_none() && project_has_configuration(&project))
    {
        return match run_configuration(
            &project,
            options.configuration.as_deref(),
            options.cycles.unwrap_or(1),
            &RuntimeOptions::default(),
        ) {
            Ok(trace) => {
                if options.json {
                    println!("{}", configuration_trace_to_json(&trace));
                } else {
                    println!("configuration: {}", trace.configuration);
                    for cycle in trace.cycles {
                        println!("cycle {}", cycle.cycle);
                        for program in cycle.programs {
                            println!(
                                "  {}.{} : {}",
                                program.resource, program.instance, program.program
                            );
                            for (name, value) in program.variables {
                                println!("    {name} = {value}");
                            }
                        }
                    }
                }
                Ok(())
            }
            Err(runtime_diagnostics) => {
                print_diagnostics(&runtime_diagnostics, options.json);
                Err("runtime failed".to_string())
            }
        };
    }

    match run_program(
        &project,
        options.program.as_deref(),
        options.cycles.unwrap_or(1),
        &RuntimeOptions::default(),
    ) {
        Ok(trace) => {
            if options.json {
                println!("{}", trace_to_json(&trace));
            } else {
                println!("program: {}", trace.program);
                for cycle in trace.cycles {
                    println!("cycle {}", cycle.cycle);
                    for (name, value) in cycle.variables {
                        println!("  {name} = {value}");
                    }
                }
            }
            Ok(())
        }
        Err(runtime_diagnostics) => {
            print_diagnostics(&runtime_diagnostics, options.json);
            Err("runtime failed".to_string())
        }
    }
}

fn command_build_c(args: &[String]) -> Result<(), String> {
    let options = CliOptions::parse(args)?;
    let input = options.input_path()?;
    let LoadedProject {
        project,
        mut diagnostics,
    } = load_project(&input, options.profile)?;
    diagnostics.extend(check_project(
        &project,
        &CheckOptions {
            profile: options.profile,
            ..CheckOptions::default()
        },
    ));
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == iec_diagnostics::Severity::Error)
    {
        print_diagnostics(&diagnostics, options.json);
        return Err("cannot generate C with errors".to_string());
    }

    match generate_c(&project, options.program.as_deref()) {
        Ok(output) => {
            if let Some(path) = options.output {
                if let Some(parent) = path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                }
                fs::write(&path, output.source).map_err(|err| err.to_string())?;
                println!("{}", path.display());
            } else {
                print!("{}", output.source);
            }
            Ok(())
        }
        Err(codegen_diagnostics) => {
            print_diagnostics(&codegen_diagnostics, options.json);
            Err("C generation failed".to_string())
        }
    }
}

fn command_import_plcopen(args: &[String]) -> Result<(), String> {
    let options = CliOptions::parse(args)?;
    let input = options.input_path()?;
    let xml = fs::read_to_string(&input).map_err(|err| err.to_string())?;
    let imported = import_plcopen_xml(&input.to_string_lossy(), &xml);
    if !imported.diagnostics.is_empty() {
        print_diagnostics(&imported.diagnostics, options.json);
    }

    if options.json {
        println!(
            "{{\"pous\":{},\"diagnostics\":{}}}",
            imported.project.pous().count(),
            diagnostics_to_json(&imported.diagnostics)
        );
    } else {
        println!("imported {} POU(s)", imported.project.pous().count());
        for pou in imported.project.pous() {
            println!("  {}", pou.name.original);
        }
    }
    Ok(())
}

fn command_export_plcopen(args: &[String]) -> Result<(), String> {
    let options = CliOptions::parse(args)?;
    let input = options.input_path()?;
    let LoadedProject {
        project,
        diagnostics,
    } = load_project(&input, options.profile)?;
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == iec_diagnostics::Severity::Error)
    {
        print_diagnostics(&diagnostics, options.json);
        return Err("cannot export project with parse errors".to_string());
    }

    let xml = export_plcopen_xml(&project);
    if let Some(path) = options.output {
        fs::write(&path, xml).map_err(|err| err.to_string())?;
        println!("{}", path.display());
    } else {
        print!("{xml}");
    }
    Ok(())
}

fn command_compliance(args: &[String]) -> Result<(), String> {
    let options = CliOptions::parse(args)?;
    let matrix = ComplianceMatrix::for_profile(options.profile);
    if options.json {
        let features = matrix
            .features
            .iter()
            .map(|feature| {
                format!(
                    "{{\"id\":\"{}\",\"clause\":\"{}\",\"title\":\"{}\",\"status\":\"{}\",\"notes\":\"{}\"}}",
                    json_escape(feature.id),
                    json_escape(feature.clause),
                    json_escape(feature.title),
                    feature.status.as_str(),
                    json_escape(feature.notes)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"profile\":\"{}\",\"features\":[{}]}}",
            matrix.profile, features
        );
    } else {
        print!("{}", matrix.to_markdown());
    }
    Ok(())
}

struct LoadedProject {
    project: Project,
    diagnostics: Vec<Diagnostic>,
}

fn load_project(path: &Path, profile: EditionProfile) -> Result<LoadedProject, String> {
    let source = fs::read_to_string(path).map_err(|err| err.to_string())?;
    let source_name = path.to_string_lossy();
    let mut loaded = if path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("xml"))
    {
        let imported = import_plcopen_xml(&source_name, &source);
        LoadedProject {
            project: imported.project,
            diagnostics: imported.diagnostics,
        }
    } else {
        let parsed = parse_project(source_name.as_ref(), &source);
        LoadedProject {
            project: parsed.project,
            diagnostics: parsed.diagnostics,
        }
    };
    loaded.project.profile = profile;
    Ok(loaded)
}

#[derive(Debug, Clone)]
struct CliOptions {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    json: bool,
    profile: EditionProfile,
    program: Option<String>,
    configuration: Option<String>,
    cycles: Option<usize>,
}

impl CliOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            input: None,
            output: None,
            json: false,
            profile: EditionProfile::Iec61131_3_2003Strict,
            program: None,
            configuration: None,
            cycles: None,
        };

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--json" => options.json = true,
                "--profile" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "--profile requires a value".to_string())?;
                    options.profile = value.parse::<EditionProfile>()?;
                }
                "--program" => {
                    index += 1;
                    options.program = Some(
                        args.get(index)
                            .ok_or_else(|| "--program requires a value".to_string())?
                            .clone(),
                    );
                }
                "--configuration" => {
                    index += 1;
                    options.configuration = Some(
                        args.get(index)
                            .ok_or_else(|| "--configuration requires a value".to_string())?
                            .clone(),
                    );
                }
                "--cycles" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "--cycles requires a value".to_string())?;
                    options.cycles = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| "--cycles must be a positive integer".to_string())?,
                    );
                }
                "-o" | "--output" => {
                    index += 1;
                    options.output = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--output requires a path".to_string())?,
                    ));
                }
                "-h" | "--help" => {
                    print_usage();
                }
                value if value.starts_with('-') => {
                    return Err(format!("unknown option '{value}'"));
                }
                value => {
                    if options.input.is_some() {
                        return Err(format!("unexpected extra argument '{value}'"));
                    }
                    options.input = Some(PathBuf::from(value));
                }
            }
            index += 1;
        }
        Ok(options)
    }

    fn input_path(&self) -> Result<PathBuf, String> {
        self.input
            .clone()
            .ok_or_else(|| "missing input path".to_string())
    }
}

fn project_has_configuration(project: &Project) -> bool {
    project
        .library_elements
        .iter()
        .any(|element| matches!(element, LibraryElement::Configuration(_)))
}

fn print_diagnostics(diagnostics: &[Diagnostic], json: bool) {
    if diagnostics.is_empty() {
        if json {
            println!("[]");
        }
        return;
    }

    if json {
        println!("{}", diagnostics_to_json(diagnostics));
    } else {
        eprintln!("{}", render_diagnostics(diagnostics));
    }
}

fn trace_to_json(trace: &iec_runtime::RuntimeTrace) -> String {
    let cycles = trace
        .cycles
        .iter()
        .map(|cycle| {
            let variables = cycle
                .variables
                .iter()
                .map(|(name, value)| {
                    format!(
                        "{{\"name\":\"{}\",\"value\":{}}}",
                        json_escape(name),
                        value_to_json(value)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"cycle\":{},\"variables\":[{}]}}",
                cycle.cycle, variables
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"program\":\"{}\",\"cycles\":[{}]}}",
        json_escape(&trace.program),
        cycles
    )
}

fn configuration_trace_to_json(trace: &iec_runtime::ConfigurationTrace) -> String {
    let cycles = trace
        .cycles
        .iter()
        .map(|cycle| {
            let programs = cycle
                .programs
                .iter()
                .map(|program| {
                    let variables = program
                        .variables
                        .iter()
                        .map(|(name, value)| {
                            format!(
                                "{{\"name\":\"{}\",\"value\":{}}}",
                                json_escape(name),
                                value_to_json(value)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!(
                        "{{\"resource\":\"{}\",\"instance\":\"{}\",\"program\":\"{}\",\"variables\":[{}]}}",
                        json_escape(&program.resource),
                        json_escape(&program.instance),
                        json_escape(&program.program),
                        variables
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{\"cycle\":{},\"programs\":[{}]}}", cycle.cycle, programs)
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"configuration\":\"{}\",\"cycles\":[{}]}}",
        json_escape(&trace.configuration),
        cycles
    )
}

fn value_to_json(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Real(value) => value.to_string(),
        Value::String(value) => format!("\"{}\"", json_escape(value)),
        Value::TimeMs(value) => value.to_string(),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(value_to_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Struct(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("\"{}\":{}", json_escape(name), value_to_json(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Unit => "null".to_string(),
    }
}

fn print_usage() {
    println!(
        "RoboC++ (rbcpp) - IEC 61131-3 toolchain\n\n\
         Commands:\n\
           rbcpp check <file> [--json] [--profile 2003-strict]\n\
           rbcpp run <file> [--program NAME|--configuration NAME] [--cycles N] [--json]\n\
           rbcpp build-c <file> [--program NAME] [-o path]\n\
           rbcpp import-plcopen <file.xml> [--json]\n\
           rbcpp export-plcopen <file.st|file.xml> [-o path]\n\
           rbcpp compliance [--json] [--profile 2003-strict]"
    );
}
