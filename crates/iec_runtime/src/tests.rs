use iec_semantics::{check_project, CheckOptions};
use iec_syntax::parse_project;

use super::*;

#[test]
fn executes_counter_program() {
    let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; Done : BOOL := FALSE; END_VAR
            Count := Count + 1;
            IF Count >= 2 THEN Done := TRUE; END_IF;
            END_PROGRAM
        "#;
    let output = parse_project("test.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_var_external_against_project_global_state() {
    let source = r#"
            PROGRAM Globals
            VAR_GLOBAL
                Shared : INT := 5;
            END_VAR
            END_PROGRAM

            PROGRAM Demo
            VAR_EXTERNAL
                Shared : INT;
            END_VAR
            VAR
                Local : INT := 0;
            END_VAR
            Shared := Shared + 2;
            Local := Shared;
            END_PROGRAM
        "#;
    let output = parse_project("var_external_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SHARED" && *value == Value::Int(7)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "LOCAL" && *value == Value::Int(7)));
}

#[test]
fn traces_program_access_paths() {
    let source = r#"
            TYPE
                Pair : STRUCT
                    Flag : BOOL;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Count : INT := 0;
                Data : Pair;
                Edge : R_TRIG;
            END_VAR
            VAR_ACCESS
                PublicCount : Count : INT READ_WRITE;
                PublicFlag : Data.Flag : BOOL READ_ONLY;
                PublicEdge : Edge.Q : BOOL READ_ONLY;
            END_VAR
            Count := Count + 1;
            Data.Flag := Count >= 1;
            Edge(CLK := TRUE);
            END_PROGRAM
        "#;
    let output = parse_project("access_trace.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let access_paths = &trace.cycles[0].access_paths;
    assert!(access_paths.iter().any(|access| {
        access.name == "PublicCount"
            && access.target == "Count"
            && access.direction == AccessDirection::ReadWrite
            && access.value == Some(Value::Int(1))
    }));
    assert!(access_paths.iter().any(|access| {
        access.name == "PublicFlag"
            && access.target == "Data.Flag"
            && access.direction == AccessDirection::ReadOnly
            && access.value == Some(Value::Bool(true))
    }));
    assert!(access_paths.iter().any(|access| {
        access.name == "PublicEdge"
            && access.target == "Edge.Q"
            && access.value == Some(Value::Bool(true))
    }));
}

#[test]
fn applies_program_access_path_writes_before_scan() {
    let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            VAR_ACCESS
                PublicCount : Count : INT READ_WRITE;
                PublicFlag : Flag : BOOL READ_ONLY;
            END_VAR
            Count := Count + 1;
            END_PROGRAM
        "#;
    let output = parse_project("access_writes.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_program_with_access_writes(
        &output.project,
        Some("Demo"),
        1,
        &RuntimeOptions::default(),
        &[AccessPathWrite {
            cycle: 0,
            name: "PublicCount".to_string(),
            value: Value::Int(41),
        }],
    )
    .expect("program should run");
    let variables = &trace.cycles[0].variables;
    assert!(variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(42)));
    assert!(trace.cycles[0]
        .access_paths
        .iter()
        .any(|access| { access.name == "PublicCount" && access.value == Some(Value::Int(42)) }));

    let error = run_program_with_access_writes(
        &output.project,
        Some("Demo"),
        1,
        &RuntimeOptions::default(),
        &[AccessPathWrite {
            cycle: 0,
            name: "PublicFlag".to_string(),
            value: Value::Bool(true),
        }],
    )
    .expect_err("READ_ONLY access write should fail");
    assert!(error
        .iter()
        .any(|diagnostic| diagnostic.message.contains("PublicFlag' is READ_ONLY")));

    let error = run_program_with_access_writes(
        &output.project,
        Some("Demo"),
        1,
        &RuntimeOptions::default(),
        &[AccessPathWrite {
            cycle: 0,
            name: "PublicCount".to_string(),
            value: Value::String("bad".to_string()),
        }],
    )
    .expect_err("wrong access write type should fail");
    assert!(error.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_ACCESS path 'PublicCount' expects integer, got STRING")));
}

#[test]
fn traces_configuration_access_paths_to_program_instances() {
    let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Demo;
                    VAR_ACCESS
                        ResourceCount : Main.Count : INT READ_ONLY;
                    END_VAR
                END_RESOURCE
                VAR_ACCESS
                    ConfigCount : Cpu.Main.Count : INT READ_ONLY;
                END_VAR
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_access_trace.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_configuration(
        &output.project,
        Some("Plant"),
        2,
        &RuntimeOptions::default(),
    )
    .expect("configuration should run");
    let cycle = &trace.cycles[1];
    assert!(cycle.access_paths.iter().any(|access| {
        access.name == "ConfigCount"
            && access.target == "Cpu.Main.Count"
            && access.value == Some(Value::Int(2))
    }));
    assert!(cycle.access_paths.iter().any(|access| {
        access.name == "Cpu.ResourceCount"
            && access.target == "Main.Count"
            && access.value == Some(Value::Int(2))
    }));
}

#[test]
fn applies_configuration_program_instance_output_bindings() {
    let source = r#"
            PROGRAM Producer
            VAR_OUTPUT
                Count : INT := 0;
            END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                VAR_GLOBAL
                    PlantObserved : INT := 0;
                END_VAR
                VAR_ACCESS
                    ConfigObserved : PlantObserved : INT READ_ONLY;
                END_VAR
                RESOURCE Cpu ON PLC
                    VAR_GLOBAL
                        ResourceObserved : INT := 0;
                    END_VAR
                    VAR_ACCESS
                        LocalObserved : ResourceObserved : INT READ_ONLY;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Producer(Count => ResourceObserved);
                    PROGRAM ConfigMain WITH Fast : Producer(Count => PlantObserved);
                END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_program_output_bindings.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_configuration(
        &output.project,
        Some("Plant"),
        2,
        &RuntimeOptions::default(),
    )
    .expect("configuration should run");

    let cycle1 = &trace.cycles[1];
    assert!(cycle1
        .access_paths
        .iter()
        .any(|access| { access.name == "ConfigObserved" && access.value == Some(Value::Int(2)) }));
    assert!(cycle1.access_paths.iter().any(|access| {
        access.name == "Cpu.LocalObserved" && access.value == Some(Value::Int(2))
    }));
}

#[test]
fn applies_program_instance_output_bindings_to_indexed_globals() {
    let source = r#"
            TYPE
                Slots : ARRAY [1..2] OF INT;
            END_TYPE

            PROGRAM Producer
            VAR_OUTPUT
                Count : INT := 0;
            END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                VAR_GLOBAL
                    Values : Slots;
                END_VAR
                VAR_ACCESS
                    PublicValues : Values : Slots READ_ONLY;
                END_VAR
                RESOURCE Cpu ON PLC
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Producer(Count => Values[2]);
                END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_program_indexed_output_bindings.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_configuration(
        &output.project,
        Some("Plant"),
        2,
        &RuntimeOptions::default(),
    )
    .expect("configuration should run");

    assert!(trace.cycles[1].access_paths.iter().any(|access| {
        access.name == "PublicValues"
            && access.value == Some(Value::Array(vec![Value::Int(0), Value::Int(2)]))
    }));
}

#[test]
fn applies_configuration_access_path_writes_to_globals_resources_and_programs() {
    let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                VAR_GLOBAL
                    Shared : INT := ADD(2, 3);
                END_VAR
                VAR_ACCESS
                    ConfigShared : Shared : INT READ_WRITE;
                    CpuProgramCount : Cpu.Main.Count : INT READ_WRITE;
                END_VAR
                RESOURCE Cpu ON PLC
                    VAR_GLOBAL
                        DeviceReady : BOOL := FALSE;
                    END_VAR
                    VAR_ACCESS
                        ResourceReady : DeviceReady : BOOL READ_WRITE;
                        ReadOnlyCount : Main.Count : INT READ_ONLY;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Demo;
                END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_access_writes.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_configuration_with_access_writes(
        &output.project,
        Some("Plant"),
        2,
        &RuntimeOptions::default(),
        &[
            AccessPathWrite {
                cycle: 1,
                name: "ConfigShared".to_string(),
                value: Value::Int(7),
            },
            AccessPathWrite {
                cycle: 0,
                name: "Cpu.ResourceReady".to_string(),
                value: Value::Bool(true),
            },
            AccessPathWrite {
                cycle: 0,
                name: "CpuProgramCount".to_string(),
                value: Value::Int(41),
            },
        ],
    )
    .expect("configuration should run");

    let cycle0 = &trace.cycles[0];
    assert!(cycle0
        .access_paths
        .iter()
        .any(|access| { access.name == "ConfigShared" && access.value == Some(Value::Int(5)) }));
    assert!(cycle0.access_paths.iter().any(|access| {
        access.name == "Cpu.ResourceReady" && access.value == Some(Value::Bool(true))
    }));
    assert!(cycle0.access_paths.iter().any(|access| {
        access.name == "CpuProgramCount" && access.value == Some(Value::Int(42))
    }));

    let cycle1 = &trace.cycles[1];
    assert!(cycle1
        .access_paths
        .iter()
        .any(|access| { access.name == "ConfigShared" && access.value == Some(Value::Int(7)) }));
    assert!(cycle1.access_paths.iter().any(|access| {
        access.name == "Cpu.ResourceReady" && access.value == Some(Value::Bool(true))
    }));
    assert!(cycle1.access_paths.iter().any(|access| {
        access.name == "CpuProgramCount" && access.value == Some(Value::Int(43))
    }));

    let error = run_configuration_with_access_writes(
        &output.project,
        Some("Plant"),
        1,
        &RuntimeOptions::default(),
        &[AccessPathWrite {
            cycle: 0,
            name: "Cpu.ReadOnlyCount".to_string(),
            value: Value::Int(10),
        }],
    )
    .expect_err("READ_ONLY configuration access write should fail");
    assert!(error.iter().any(|diagnostic| diagnostic
        .message
        .contains("Cpu.ReadOnlyCount' is READ_ONLY")));

    let error = run_configuration_with_access_writes(
        &output.project,
        Some("Plant"),
        1,
        &RuntimeOptions::default(),
        &[AccessPathWrite {
            cycle: 0,
            name: "ConfigShared".to_string(),
            value: Value::String("bad".to_string()),
        }],
    )
    .expect_err("wrong configuration access write type should fail");
    assert!(error.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_ACCESS path 'ConfigShared' expects integer, got STRING")));
}

#[test]
fn routes_configuration_direct_access_and_outputs_through_shared_state() {
    let output_source = r#"
            PROGRAM Producer
            VAR_OUTPUT
                Out : BOOL := FALSE;
            END_VAR
            Out := TRUE;
            END_PROGRAM

            PROGRAM Consumer
            VAR
                Seen : BOOL := FALSE;
            END_VAR
            Seen := %QX0.0;
            END_PROGRAM

            CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    VAR_ACCESS
                        DirectOut : %QX0.0 : BOOL READ_WRITE;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM AProducer WITH Fast : Producer(Out => %QX0.0);
                    PROGRAM ZConsumer WITH Fast : Consumer;
                END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_direct_output.st", output_source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_configuration(
        &output.project,
        Some("Plant"),
        1,
        &RuntimeOptions::default(),
    )
    .expect("configuration should run");
    let cycle0 = &trace.cycles[0];
    assert!(cycle0.programs.iter().any(|program| {
        program.instance == "ZConsumer"
            && program
                .variables
                .iter()
                .any(|(name, value)| name == "SEEN" && *value == Value::Bool(true))
    }));
    assert!(cycle0.access_paths.iter().any(|access| {
        access.name == "Cpu.DirectOut" && access.value == Some(Value::Bool(true))
    }));

    let access_source = r#"
            PROGRAM Consumer
            VAR
                Seen : BOOL := FALSE;
            END_VAR
            Seen := %QX0.1;
            END_PROGRAM

            CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    VAR_ACCESS
                        DirectOut : %QX0.1 : BOOL READ_WRITE;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Consumer;
                END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_direct_access_write.st", access_source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_configuration_with_access_writes(
        &output.project,
        Some("Plant"),
        1,
        &RuntimeOptions::default(),
        &[AccessPathWrite {
            cycle: 0,
            name: "Cpu.DirectOut".to_string(),
            value: Value::Bool(true),
        }],
    )
    .expect("configuration should run");
    let cycle0 = &trace.cycles[0];
    assert!(cycle0.programs.iter().any(|program| {
        program.instance == "Main"
            && program
                .variables
                .iter()
                .any(|(name, value)| name == "SEEN" && *value == Value::Bool(true))
    }));
    assert!(cycle0.access_paths.iter().any(|access| {
        access.name == "Cpu.DirectOut" && access.value == Some(Value::Bool(true))
    }));
}

#[test]
fn enforces_max_scan_cycles() {
    let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM
        "#;
    let output = parse_project("scan_limit.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let result = run_program(
        &output.project,
        Some("Demo"),
        3,
        &RuntimeOptions {
            max_scan_cycles: 2,
            ..RuntimeOptions::default()
        },
    );
    let diagnostics = result.expect_err("scan limit should reject the run");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("scan cycle count 3")));
}

#[test]
fn runs_configuration_tasks_by_interval_and_priority() {
    let source = r#"
            PROGRAM FastProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            PROGRAM SlowProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                TASK Slow(INTERVAL := T#2ms, PRIORITY := 2);
                PROGRAM FastInstance WITH Fast : FastProgram(Count := 10);
                PROGRAM SlowInstance WITH Slow : SlowProgram(Count := ADD(20, 1));
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_configuration(
        &output.project,
        Some("Plant"),
        3,
        &RuntimeOptions::default(),
    )
    .expect("configuration should run");
    assert_eq!(trace.cycles.len(), 3);
    assert_eq!(trace.cycles[0].programs[0].instance, "FastInstance");
    assert_eq!(trace.cycles[0].programs[1].instance, "SlowInstance");
    assert_eq!(trace.cycles[1].programs.len(), 1);
    let fast_last = trace
        .cycles
        .last()
        .unwrap()
        .programs
        .iter()
        .find(|program| program.instance == "FastInstance")
        .unwrap();
    assert!(fast_last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(13)));
    let slow_last = trace.cycles[2]
        .programs
        .iter()
        .find(|program| program.instance == "SlowInstance")
        .unwrap();
    assert!(slow_last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(23)));
}

#[test]
fn runs_configuration_single_tasks_on_rising_edges() {
    let source = r#"
            PROGRAM EventProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Trigger : BOOL := FALSE;
            END_VAR
            VAR_ACCESS
                PublicTrigger : Trigger : BOOL READ_WRITE;
            END_VAR
            RESOURCE Cpu ON PLC
                TASK OnTrigger(SINGLE := Trigger, PRIORITY := 1);
                PROGRAM EventInstance WITH OnTrigger : EventProgram;
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_single_task_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let writes = [
        AccessPathWrite {
            cycle: 0,
            name: "PublicTrigger".to_string(),
            value: Value::Bool(true),
        },
        AccessPathWrite {
            cycle: 2,
            name: "PublicTrigger".to_string(),
            value: Value::Bool(false),
        },
        AccessPathWrite {
            cycle: 3,
            name: "PublicTrigger".to_string(),
            value: Value::Bool(true),
        },
    ];
    let trace = run_configuration_with_access_writes(
        &output.project,
        Some("Plant"),
        5,
        &RuntimeOptions::default(),
        &writes,
    )
    .expect("configuration should run");
    assert_eq!(trace.cycles.len(), 5);
    assert_eq!(trace.cycles[0].programs.len(), 1);
    assert!(trace.cycles[1].programs.is_empty());
    assert!(trace.cycles[2].programs.is_empty());
    assert_eq!(trace.cycles[3].programs.len(), 1);
    assert!(trace.cycles[4].programs.is_empty());
    let second_fire = &trace.cycles[3].programs[0];
    assert!(second_fire
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
}

#[test]
fn stress_schedules_interval_single_direct_globals_and_access_writes() {
    let source = r#"
            PROGRAM Producer
            VAR_OUTPUT
                Count : INT := 0;
                Pulse : BOOL := FALSE;
            END_VAR
            Count := Count + 1;
            Pulse := Count >= 2;
            END_PROGRAM

            PROGRAM Reader
            VAR_OUTPUT
                DirectSeen : BOOL := FALSE;
            END_VAR
            DirectSeen := %QX0.2;
            END_PROGRAM

            PROGRAM EventProgram
            VAR_OUTPUT
                EventTotal : INT := 0;
            END_VAR
            EventTotal := EventTotal + 10;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Trigger : BOOL := FALSE;
                Shared : INT := 0;
                DirectSeenGlobal : BOOL := FALSE;
                EventTotalGlobal : INT := 0;
            END_VAR
            VAR_ACCESS
                PublicTrigger : Trigger : BOOL READ_WRITE;
                PublicShared : Shared : INT READ_ONLY;
                PublicDirectSeen : DirectSeenGlobal : BOOL READ_ONLY;
                PublicEventTotal : EventTotalGlobal : INT READ_ONLY;
                PublicDirectOut : %QX0.2 : BOOL READ_ONLY;
            END_VAR
            RESOURCE Cpu ON PLC
                TASK OnTrigger(SINGLE := Trigger, PRIORITY := 0);
                TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                TASK Slow(INTERVAL := T#2ms, PRIORITY := 2);
                PROGRAM FastProducer WITH Fast : Producer(Count => Shared, Pulse => %QX0.2);
                PROGRAM SlowReader WITH Slow : Reader(DirectSeen => DirectSeenGlobal);
                PROGRAM EventInstance WITH OnTrigger : EventProgram(EventTotal => EventTotalGlobal);
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("configuration_scheduling_stress.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_configuration_with_access_writes(
        &output.project,
        Some("Plant"),
        5,
        &RuntimeOptions::default(),
        &[
            AccessPathWrite {
                cycle: 1,
                name: "PublicTrigger".to_string(),
                value: Value::Bool(true),
            },
            AccessPathWrite {
                cycle: 2,
                name: "PublicTrigger".to_string(),
                value: Value::Bool(false),
            },
            AccessPathWrite {
                cycle: 3,
                name: "PublicTrigger".to_string(),
                value: Value::Bool(true),
            },
        ],
    )
    .expect("configuration should run");

    assert_eq!(trace.cycles.len(), 5);
    assert_eq!(trace.cycles[0].programs.len(), 2);
    assert_eq!(trace.cycles[1].programs.len(), 2);
    assert_eq!(trace.cycles[2].programs.len(), 2);
    assert_eq!(trace.cycles[3].programs.len(), 2);
    assert_eq!(trace.cycles[4].programs.len(), 2);

    let cycle4 = &trace.cycles[4];
    assert!(cycle4
        .access_paths
        .iter()
        .any(|access| { access.name == "PublicShared" && access.value == Some(Value::Int(5)) }));
    assert!(cycle4.access_paths.iter().any(|access| {
        access.name == "PublicDirectOut" && access.value == Some(Value::Bool(true))
    }));
    assert!(cycle4.access_paths.iter().any(|access| {
        access.name == "PublicDirectSeen" && access.value == Some(Value::Bool(true))
    }));
    assert!(cycle4.access_paths.iter().any(|access| {
        access.name == "PublicEventTotal" && access.value == Some(Value::Int(20))
    }));
}

#[test]
fn executes_loops_case_and_standard_functions() {
    let source = r#"
            TYPE
                Mode : (Idle, Run, Fault);
            END_TYPE

            PROGRAM Demo
            VAR
                I : INT := 0;
                Total : INT := 0;
                Selected : INT := 0;
                Done : BOOL := FALSE;
                State : Mode := Run;
                EnumDone : BOOL := FALSE;
            END_VAR

            FOR I := 1 TO 3 DO
                Total := Total + I;
            END_FOR;

            WHILE Total < 8 DO
                Total := Total + 1;
            END_WHILE;

            REPEAT
                Total := Total - 1;
            UNTIL Total = 7
            END_REPEAT;

            Selected := MAX(Total, 3);

            CASE Selected OF
                7: Done := TRUE;
                ELSE Done := FALSE;
            END_CASE;
            CASE State OF
                Idle: EnumDone := FALSE;
                Run, Fault: EnumDone := TRUE;
            END_CASE;
            END_PROGRAM
        "#;
    let output = parse_project("test.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TOTAL" && *value == Value::Int(7)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SELECTED" && *value == Value::Int(7)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ENUMDONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_standard_power_precedence_and_associativity() {
    let source = r#"
            PROGRAM Demo
            VAR
                RightAssoc : REAL := 0.0;
                NegatedPower : REAL := 0.0;
                Positive : INT := 0;
            END_VAR
            RightAssoc := 2 ** 3 ** 2;
            NegatedPower := -2 ** 2;
            Positive := +2 + +(+3);
            END_PROGRAM
        "#;
    let output = parse_project("power_precedence.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "RIGHTASSOC" && *value == Value::Real(512.0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "NEGATEDPOWER" && *value == Value::Real(-4.0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "POSITIVE" && *value == Value::Int(5)));
}

#[test]
fn executes_user_defined_functions() {
    let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
                Factor : INT;
            END_VAR
            Scale := Input * Factor;
            END_FUNCTION

            PROGRAM Demo
            VAR
                A : INT := 4;
                B : INT := 0;
                C : INT := 0;
            END_VAR
            B := Scale(A, 3);
            C := Scale(Input := B, Factor := 2);
            END_PROGRAM
        "#;
    let output = parse_project("functions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "B" && *value == Value::Int(12)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "C" && *value == Value::Int(24)));
}

#[test]
fn executes_function_en_eno_controls() {
    let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
            END_VAR
            Scale := Input * 2;
            END_FUNCTION

            PROGRAM Demo
            VAR
                EnabledResult : INT := 0;
                DisabledResult : INT := 5;
                EnabledOk : BOOL := FALSE;
                DisabledOk : BOOL := TRUE;
            END_VAR

            EnabledResult := Scale(EN := TRUE, Input := 3, ENO => EnabledOk);
            DisabledResult := Scale(EN := FALSE, Input := 10, ENO => DisabledOk);
            END_PROGRAM
        "#;
    let output = parse_project("function_controls.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ENABLEDRESULT" && *value == Value::Int(6)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DISABLEDRESULT" && *value == Value::Int(0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ENABLEDOK" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DISABLEDOK" && *value == Value::Bool(false)));
}

#[test]
fn executes_disabled_standard_function_defaults_by_return_family() {
    let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[8] := 'keep';
                Wide : WSTRING[8] := "keep";
                Cmp : BOOL := TRUE;
                Root : REAL := 5.5;
                Delay : TIME := T#1s;
                TextOk : BOOL := TRUE;
                WideOk : BOOL := TRUE;
                CmpOk : BOOL := TRUE;
                RealOk : BOOL := TRUE;
                TimeOk : BOOL := TRUE;
            END_VAR

            Text := LEFT(EN := FALSE, IN := 'robot', L := 2, ENO => TextOk);
            Wide := LEFT(EN := FALSE, IN := "robot", L := 2, ENO => WideOk);
            Cmp := EQ(EN := FALSE, IN1 := 1, IN2 := 1, ENO => CmpOk);
            Root := SQRT(EN := FALSE, IN := 4.0, ENO => RealOk);
            Delay := ADD_TIME(EN := FALSE, IN1 := T#1s, IN2 := T#2s, ENO => TimeOk);
            END_PROGRAM
        "#;
    let output = parse_project("disabled_standard_defaults.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXT" && *value == Value::String(String::new())));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "WIDE" && *value == Value::WString(String::new())));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "CMP" && *value == Value::Bool(false)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ROOT" && *value == Value::Real(0.0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(0)));
    for flag in ["TEXTOK", "WIDEOK", "CMPOK", "REALOK", "TIMEOK"] {
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == flag && *value == Value::Bool(false)));
    }
}

#[test]
fn executes_disabled_user_function_defaults_for_named_returns() {
    let source = r#"
            TYPE
                ShortText : STRING[8];
                Pair : STRUCT
                    A : INT := 1;
                    B : BOOL := TRUE;
                END_STRUCT;
            END_TYPE

            FUNCTION Label : ShortText
            Label := 'live';
            END_FUNCTION

            FUNCTION MakePair : Pair
            MakePair := (A := 7, B := FALSE);
            END_FUNCTION

            PROGRAM Demo
            VAR
                Text : ShortText := 'keep';
                Item : Pair := (A := 9, B := TRUE);
                TextOk : BOOL := TRUE;
                PairOk : BOOL := TRUE;
            END_VAR

            Text := Label(EN := FALSE, ENO => TextOk);
            Item := MakePair(EN := FALSE, ENO => PairOk);
            END_PROGRAM
        "#;
    let output = parse_project("disabled_user_function_named_returns.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXT" && *value == Value::String(String::new())));
    let mut expected = BTreeMap::new();
    expected.insert("A".to_string(), Value::Int(1));
    expected.insert("B".to_string(), Value::Bool(true));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ITEM" && *value == Value::Struct(expected.clone())));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXTOK" && *value == Value::Bool(false)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "PAIROK" && *value == Value::Bool(false)));
}

#[test]
fn executes_named_subrange_defaults_for_state_and_disabled_returns() {
    let source = r#"
            TYPE
                Positive : INT(5..10);
                Zeroable : INT(-1..3);
            END_TYPE

            FUNCTION Pick : Positive
            Pick := 9;
            END_FUNCTION

            FUNCTION_BLOCK Holder
            VAR_OUTPUT
                Out : Positive;
            END_VAR
            END_FUNCTION_BLOCK

            PROGRAM Globals
            VAR_GLOBAL
                Shared : Positive;
            END_VAR
            END_PROGRAM

            PROGRAM Demo
            VAR_EXTERNAL
                Shared : Positive;
            END_VAR
            VAR
                Direct : Positive;
                IncludesZero : Zeroable;
                Fb : Holder;
                Disabled : Positive := 6;
                Ok : BOOL := TRUE;
            END_VAR

            Disabled := Pick(EN := FALSE, ENO => Ok);
            END_PROGRAM
        "#;
    let output = parse_project("subrange_defaults_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SHARED" && *value == Value::Int(5)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DIRECT" && *value == Value::Int(5)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "INCLUDESZERO" && *value == Value::Int(0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FB.OUT" && *value == Value::Int(5)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DISABLED" && *value == Value::Int(5)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OK" && *value == Value::Bool(false)));
}

#[test]
fn executes_expanded_standard_functions() {
    let source = r#"
            PROGRAM Demo
            VAR
                Sum : INT := 0;
                Product : INT := 0;
                Choice : INT := 0;
                Shifted : INT := 0;
                Rotated : INT := 0;
                Ok : BOOL := FALSE;
            END_VAR

            Sum := ADD(1, 2, 3);
            Product := MUL(Sum, 2);
            Choice := MUX(2, 10, 20, 30);
            Shifted := SHL(1, 3);
            Rotated := ROL(1, 1);
            Ok := GT(Product, Sum) AND EQ(MOVE(Choice), 30) AND NE(Shifted, Rotated);
            END_PROGRAM
        "#;
    let output = parse_project("standard_functions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SUM" && *value == Value::Int(6)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "PRODUCT" && *value == Value::Int(12)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "CHOICE" && *value == Value::Int(30)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SHIFTED" && *value == Value::Int(8)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ROTATED" && *value == Value::Int(2)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
}

#[test]
fn executes_out_of_order_standard_function_formal_inputs() {
    let source = r#"
            PROGRAM Demo
            VAR
                Limited : INT := 0;
                Selected : INT := 0;
                Muxed : INT := 0;
                Shifted : INT := 0;
                Text : STRING[8] := '';
                Ok : BOOL := FALSE;
            END_VAR

            Limited := LIMIT(IN := 12, MN := 0, MX := 10);
            Selected := SEL(IN1 := 20, G := FALSE, IN0 := 10);
            Muxed := MUX(IN1 := 200, K := 1, IN0 := 100);
            Shifted := SHL(N := 2, IN := 1);
            Text := LEFT(L := 3, IN := 'robot');
            Ok := EQ(IN2 := 10, IN1 := Limited);
            END_PROGRAM
        "#;
    let output = parse_project("standard_formal_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "LIMITED" && *value == Value::Int(10)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SELECTED" && *value == Value::Int(10)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "MUXED" && *value == Value::Int(200)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SHIFTED" && *value == Value::Int(4)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXT" && *value == Value::String("rob".to_string())));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
}

#[test]
fn rejects_negative_shift_counts_at_runtime() {
    let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := -1;
                Shifted : INT := 0;
            END_VAR
            Shifted := SHL(1, Count);
            END_PROGRAM
        "#;
    let output = parse_project("negative_shift_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
    assert!(result
        .expect_err("negative shift should fail")
        .iter()
        .any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SHL' failed for supplied arguments")));
}

#[test]
fn executes_string_bit_and_time_standard_functions() {
    let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING := '';
                QuotedText : STRING[16] := 'A$"B$'';
                DateText : STRING[32] := '';
                TodText : WSTRING[32] := "";
                QuotedWide : WSTRING[16] := "A$'B$"";
                DtText : STRING[40] := '';
                Found : INT := 0;
                Mask : INT := 0;
                Flag : BOOL := FALSE;
                Delay : TIME := T#0ms;
                MinDelay : TIME := T#0ms;
                Span : TIME := T#0ms;
                Scale : TIME := T#0ms;
                TimeOfDay : TIME_OF_DAY := TOD#00:00:00;
                BuiltDate : DATE := D#1970-01-01;
                BuiltTod : TIME_OF_DAY := TOD#00:00:00;
                Stamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
                BuiltStamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
                Weekday : INT := 0;
                EscapedLen : INT := 0;
                Year : INT := 0;
                Month : INT := 0;
                DatePart : INT := 0;
                Hour : INT := 0;
                Minute : INT := 0;
                Second : INT := 0;
                Millisecond : INT := 0;
            END_VAR

            Text := CONCAT(LEFT('robot', 2), RIGHT('code', 2));
            DateText := DATE_TO_STRING(STRING_TO_DATE('D#1970-01-02'));
            TodText := TOD_TO_WSTRING(STRING_TO_TOD('TOD#01:02:03.004'));
            DtText := DATE_AND_TIME_TO_STRING(STRING_TO_DATE_AND_TIME('DT#1970-01-02-01:02:03.004'));
            Found := FIND(Text, 'de');
            Mask := OR(AND(15, 51), XOR(1, 3));
            Flag := XOR(TRUE, FALSE);
            Delay := ADD_TIME(T#1s, MUL_TIME(T#100ms, 2));
            MinDelay := MIN(T#2s, T#750ms);
            Span := SUB_DATE_DATE(D#1970-01-03, D#1970-01-01);
            Scale := DIVTIME(MULTIME(T#750ms, 4), 2);
            TimeOfDay := ADD_TOD_TIME(TOD#00:00:01, T#2s);
            BuiltDate := CONCAT_DATE(1970, 1, 3);
            BuiltTod := CONCAT_TOD(0, 0, 3, 250);
            Stamp := ADD_DT_TIME(DT#1970-01-01-00:00:01, T#2s);
            BuiltStamp := CONCAT_DATE_TOD(BuiltDate, BuiltTod);
            Weekday := DAY_OF_WEEK(D#1970-01-01);
            EscapedLen := LEN('A$0A$27$$');
            SPLIT_DT(
                IN := DT#1970-01-03-01:02:03.004,
                YEAR => Year,
                MONTH => Month,
                DATE => DatePart,
                HOUR => Hour,
                MINUTE => Minute,
                SECOND => Second,
                MILLISECOND => Millisecond);
            END_PROGRAM
        "#;
    let output = parse_project("standard_catalog.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXT" && *value == Value::String("rode".to_string())));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "QUOTEDTEXT" && *value == Value::String("A\"B'".to_string())
    }));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "DATETEXT" && *value == Value::String("D#1970-01-02".to_string())
    }));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "TODTEXT" && *value == Value::WString("TOD#01:02:03.004".to_string())
    }));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "QUOTEDWIDE" && *value == Value::WString("A'B\"".to_string())
    }));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "DTTEXT" && *value == Value::String("DT#1970-01-02-01:02:03.004".to_string())
    }));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FOUND" && *value == Value::Int(3)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "MASK" && *value == Value::Int(3)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(1200)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "MINDELAY" && *value == Value::TimeMs(750)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SPAN" && *value == Value::TimeMs(172_800_000)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SCALE" && *value == Value::TimeMs(1_500)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TIMEOFDAY" && *value == Value::TimeMs(3_000)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "BUILTDATE" && *value == Value::TimeMs(2)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "BUILTTOD" && *value == Value::TimeMs(3_250)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "STAMP" && *value == Value::TimeMs(3_000)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| { name == "BUILTSTAMP" && *value == Value::TimeMs(172_803_250) }));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "WEEKDAY" && *value == Value::Int(4)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ESCAPEDLEN" && *value == Value::Int(4)));
    for (expected_name, expected_value) in [
        ("YEAR", 1970),
        ("MONTH", 1),
        ("DATEPART", 3),
        ("HOUR", 1),
        ("MINUTE", 2),
        ("SECOND", 3),
        ("MILLISECOND", 4),
    ] {
        assert!(last.variables.iter().any(|(name, value)| {
            name == expected_name && *value == Value::Int(expected_value)
        }));
    }
}

#[test]
fn truncates_bounded_string_assignments_at_runtime() {
    let source = r#"
            PROGRAM Demo
            VAR
                Source : STRING[8] := 'abcdef';
                WideSource : WSTRING[8] := "abcdef";
                Text : STRING[3] := '';
                Wide : WSTRING[3] := "";
            END_VAR

            Text := CONCAT(LEFT(Source, 4), RIGHT(Source, 2));
            Wide := CONCAT(LEFT(WideSource, 4), RIGHT(WideSource, 2));
            END_PROGRAM
        "#;
    let output = parse_project("bounded_string_truncation_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXT" && *value == Value::String("abc".to_string())));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "WIDE" && *value == Value::WString("abc".to_string())));
}

#[test]
fn executes_wstring_literals_and_string_functions() {
    let source = r#"
            PROGRAM Demo
            VAR
                Text : WSTRING[16] := "ro";
                Out : WSTRING[16] := "";
            END_VAR

            Out := CONCAT(Text, "bot");
            Text := LEFT(Out, 4);
            END_PROGRAM
        "#;
    let output = parse_project("wstring_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| { name == "OUT" && *value == Value::WString("robot".to_string()) }));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| { name == "TEXT" && *value == Value::WString("robo".to_string()) }));
}

#[test]
fn executes_date_and_time_of_day_literals() {
    let source = r#"
            PROGRAM Demo
            VAR
                Today : DATE := D#1970-01-02;
                Leap : DATE := D#2024-02-29;
                Noon : TIME_OF_DAY := TOD#12:00:00.250;
                Stamp : DATE_AND_TIME := DT#1970-01-02-00:00:01;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("date_literals.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TODAY" && *value == Value::TimeMs(1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| { name == "LEAP" && *value == Value::TimeMs(19_782) }));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "NOON" && *value == Value::TimeMs(43_200_250)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "STAMP" && *value == Value::TimeMs(86_401_000)));
}

#[test]
fn executes_typed_alias_literals_for_all_scalar_families() {
    let source = r#"
            TYPE
                MyBool : BOOL;
                MyBool2 : MyBool;
                MyReal : REAL;
                MyReal2 : MyReal;
                MyTime : TIME;
                MyTime2 : MyTime;
                MyDate : DATE;
                MyDate2 : MyDate;
                MyTod : TIME_OF_DAY;
                MyTod2 : MyTod;
                MyDt : DATE_AND_TIME;
                MyDt2 : MyDt;
                Small : INT(0..10);
                Small2 : Small;
                Mode : (Idle, Run, Fault);
                ModeAlias : Mode;
            END_TYPE

            PROGRAM Demo
            VAR
                Flag : MyBool2 := MyBool2#FALSE;
                RealValue : MyReal2 := MyReal2#0.0;
                Delay : MyTime2 := MyTime2#0ms;
                Today : MyDate2 := MyDate2#1970-01-01;
                Clock : MyTod2 := MyTod2#00:00:00;
                Stamp : MyDt2 := MyDt2#1970-01-01-00:00:00;
                SmallValue : Small2 := Small2#0;
                State : ModeAlias := ModeAlias#Idle;
                Text : STRING[64] := '';
            END_VAR

            Flag := MyBool2#TRUE;
            RealValue := MyReal2#1.5 + 0.5;
            Delay := MyTime2#1.5s;
            Today := MyDate2#1970-01-02;
            Clock := MyTod2#01:02:03.004;
            Stamp := MyDt2#1970-01-02-01:02:03.004;
            SmallValue := Small2#7;
            State := ModeAlias#Fault;
            Text := DATE_TO_STRING(Today);
            END_PROGRAM
        "#;
    let output = parse_project("typed_alias_literal_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "REALVALUE" && *value == Value::Real(2.0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(1500)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TODAY" && *value == Value::TimeMs(1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "CLOCK" && *value == Value::TimeMs(3_723_004)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "STAMP" && *value == Value::TimeMs(90_123_004)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SMALLVALUE" && *value == Value::Int(7)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "STATE" && *value == Value::Int(2)));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "TEXT" && *value == Value::String("D#1970-01-02".to_string())
    }));
}

#[test]
fn executes_expanded_conversion_functions() {
    let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[32] := '';
                Parsed : INT := 0;
                Truncated : INT := 0;
                Bcd : WORD := 0;
                FromBcd : INT := 0;
                RealValue : REAL := 0.0;
                Flag : BOOL := FALSE;
                Delay : TIME := T#0ms;
                LongDelay : TIME := T#0ms;
                FractionalDelay : TIME := T#0ms;
            END_VAR

            Parsed := STRING_TO_INT('42');
            Truncated := TRUNC(-1.6);
            Bcd := INT_TO_BCD(369);
            FromBcd := BCD_TO_INT(Bcd) + WORD_BCD_TO_UINT(UINT_TO_BCD_WORD(25));
            RealValue := STRING_TO_REAL('2.5');
            Flag := STRING_TO_BOOL('TRUE');
            Delay := STRING_TO_TIME('T#250ms') + INT_TO_TIME(50);
            LongDelay := STRING_TO_TIME('T#1h2m3s4ms');
            FractionalDelay := STRING_TO_TIME('T#1.5s');
            Text := CONCAT(BOOL_TO_STRING(Flag), INT_TO_STRING(Parsed));
            END_PROGRAM
        "#;
    let output = parse_project("conversions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "PARSED" && *value == Value::Int(42)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TRUNCATED" && *value == Value::Int(-1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "BCD" && *value == Value::Int(0x369)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FROMBCD" && *value == Value::Int(394)));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "REALVALUE"
            && matches!(value, Value::Real(value) if (*value - 2.5).abs() < f64::EPSILON)
    }));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(300)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "LONGDELAY" && *value == Value::TimeMs(3_723_004)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| { name == "FRACTIONALDELAY" && *value == Value::TimeMs(1_500) }));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TEXT" && *value == Value::String("TRUE42".to_string())));
}

#[test]
fn executes_bool_and_bit_string_st_operators() {
    let source = r#"
            PROGRAM Demo
            VAR
                Mask : INT := 0;
                Inverted : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            Mask := (15 AND 51) OR (1 XOR 3);
            Inverted := NOT 15;
            Flag := (TRUE AND FALSE) OR TRUE;
            END_PROGRAM
        "#;
    let output = parse_project("st_bit_ops.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "MASK" && *value == Value::Int(3)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "INVERTED" && *value == Value::Int(!15)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
}

#[test]
fn short_circuits_bool_and_or_operands() {
    let source = r#"
            PROGRAM Demo
            VAR
                Ok1 : BOOL := TRUE;
                Ok2 : BOOL := FALSE;
            END_VAR

            Ok1 := FALSE AND (1 / 0 = 0);
            Ok2 := TRUE OR (1 / 0 = 0);
            END_PROGRAM
        "#;
    let output = parse_project("short_circuit.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OK1" && *value == Value::Bool(false)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OK2" && *value == Value::Bool(true)));
}

#[test]
fn reports_integer_overflow() {
    let source = r#"
            PROGRAM Demo
            VAR A : LINT := 9223372036854775807; END_VAR
            A := A + 1;
            END_PROGRAM
        "#;
    let output = parse_project("overflow.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
    let diagnostics = result.expect_err("overflow should reject the run");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("integer overflow during addition")));
}

#[test]
fn executes_arrays_structs_enums_and_subrange_checks() {
    let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                Pair : STRUCT
                    Low : Small := 1;
                    High : Small := 2;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Values : ARRAY [1..3] OF Small := [1, 2, 3];
                Copy : ARRAY [1..3] OF Small := [0, 0, 0];
                Repeated : ARRAY [1..5] OF Small := [2(1), 3(2)];
                Window : Pair := (Low := 4, High := 6);
                Backup : Pair := (Low := 0, High := 0);
                State : Mode := Idle;
                Selected : Mode := Idle;
                Total : INT := 0;
                IsRun : BOOL := FALSE;
                IsNotIdle : BOOL := FALSE;
            END_VAR

            Values[2] := Values[1] + Window.High;
            Copy := Values;
            Window.Low := Values[2];
            Backup := Window;
            State := Run;
            Selected := MUX(1, Idle, Fault);
            IsRun := State = Run;
            IsNotIdle := NE(Selected, Idle);
            Total := Values[2] + Window.Low;
            END_PROGRAM
        "#;
    let output = parse_project("aggregates_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TOTAL" && *value == Value::Int(14)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ISRUN" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SELECTED" && *value == Value::Int(2)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ISNOTIDLE" && *value == Value::Bool(true)));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "COPY" && *value == Value::Array(vec![Value::Int(1), Value::Int(7), Value::Int(3)])
    }));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "REPEATED"
            && *value
                == Value::Array(vec![
                    Value::Int(1),
                    Value::Int(1),
                    Value::Int(2),
                    Value::Int(2),
                    Value::Int(2),
                ])
    }));
    let mut backup = BTreeMap::new();
    backup.insert("LOW".to_string(), Value::Int(7));
    backup.insert("HIGH".to_string(), Value::Int(6));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "BACKUP" && *value == Value::Struct(backup.clone())));
}

#[test]
fn executes_nested_array_access_inside_structures() {
    let source = r#"
            TYPE
                Row : ARRAY [2..4] OF INT;
                Matrix : ARRAY [1..2] OF Row;
                Holder : STRUCT
                    Rows : Matrix;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Box : Holder := (Rows := [[1, 2, 3], [4, 5, 6]]);
                RowCopy : Row := [0, 0, 0];
                Total : INT := 0;
            END_VAR

            Box.Rows[1][3] := 20;
            RowCopy := Box.Rows[1];
            Total := Box.Rows[1][2] + RowCopy[3] + Box.Rows[2][4];
            END_PROGRAM
        "#;
    let output = parse_project("nested_aggregates_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TOTAL" && *value == Value::Int(27)));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "ROWCOPY"
            && *value == Value::Array(vec![Value::Int(1), Value::Int(20), Value::Int(3)])
    }));
}

#[test]
fn executes_nested_derived_aliases_for_aggregates_and_enums() {
    let source = r#"
            TYPE
                Small : INT(0..10);
                SmallAlias : Small;
                SmallAlias2 : SmallAlias;
                Row : ARRAY [1..2] OF SmallAlias2;
                RowAlias : Row;
                Holder : STRUCT
                    Values : RowAlias;
                END_STRUCT;
                HolderAlias : Holder;
                Mode : (Idle, Run);
                ModeAlias : Mode;
                ModeAlias2 : ModeAlias;
            END_TYPE

            PROGRAM Demo
            VAR
                Box : HolderAlias := (Values := [2, 3]);
                Copy : RowAlias := [0, 0];
                State : ModeAlias2 := Idle;
                Total : INT := 0;
                IsRun : BOOL := FALSE;
            END_VAR

            Box.Values[1] := 7;
            Copy := Box.Values;
            State := Run;
            IsRun := State = Run;
            Total := Copy[1] + Copy[2];
            END_PROGRAM
        "#;
    let output = parse_project("nested_alias_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TOTAL" && *value == Value::Int(10)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ISRUN" && *value == Value::Bool(true)));
    assert!(last.variables.iter().any(|(name, value)| {
        name == "COPY" && *value == Value::Array(vec![Value::Int(7), Value::Int(3)])
    }));
}

#[test]
fn rejects_runtime_subrange_violations() {
    let source = r#"
            TYPE Small : INT(0..10); END_TYPE
            PROGRAM Demo
            VAR Value : Small := 1; END_VAR
            Value := 11;
            END_PROGRAM
        "#;
    let output = parse_project("subrange_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'Value' value 11 is outside subrange 0..10")));
    let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
    let diagnostics = result.expect_err("runtime should reject subrange violation");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value 11 is outside subrange 0..10")));
}

#[test]
fn rejects_runtime_elementary_range_and_conversion_violations() {
    let elementary_source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 300;
                ByteValue : BYTE := 0;
            END_VAR
            ByteValue := Count;
            END_PROGRAM
        "#;
    let output = parse_project("elementary_range_runtime.st", elementary_source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
    let diagnostics = result.expect_err("runtime should reject BYTE range violation");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value 300 is outside BYTE range 0..255")));

    let conversion_source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 300;
                Converted : USINT := 0;
            END_VAR
            Converted := INT_TO_USINT(Count);
            END_PROGRAM
        "#;
    let output = parse_project("conversion_range_runtime.st", conversion_source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
    let diagnostics = result.expect_err("runtime should reject conversion range violation");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'INT_TO_USINT' failed for supplied arguments")));
}

#[test]
fn preserves_retain_variables_across_warm_restart() {
    let source = r#"
            PROGRAM Demo
            VAR RETAIN
                Kept : INT := 10;
            END_VAR
            VAR NON_RETAIN
                Reset : INT := 10;
            END_VAR
            VAR
                Plain : INT := 10;
            END_VAR

            Kept := Kept + 1;
            Reset := Reset + 1;
            Plain := Plain + 1;
            END_PROGRAM
        "#;
    let output = parse_project("retain_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let options = RuntimeOptions {
        warm_restart_before_cycles: vec![2],
        ..RuntimeOptions::default()
    };
    let trace =
        run_program(&output.project, Some("Demo"), 4, &options).expect("program should run");
    let before_restart = &trace.cycles[1].variables;
    let after_restart = &trace.cycles[2].variables;
    let last = &trace.cycles[3].variables;

    assert!(before_restart
        .iter()
        .any(|(name, value)| name == "KEPT" && *value == Value::Int(12)));
    assert!(after_restart
        .iter()
        .any(|(name, value)| name == "KEPT" && *value == Value::Int(13)));
    assert!(after_restart
        .iter()
        .any(|(name, value)| name == "RESET" && *value == Value::Int(11)));
    assert!(after_restart
        .iter()
        .any(|(name, value)| name == "PLAIN" && *value == Value::Int(11)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "KEPT" && *value == Value::Int(14)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "RESET" && *value == Value::Int(12)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "PLAIN" && *value == Value::Int(12)));
}

#[test]
fn resets_program_var_temp_each_scan() {
    let source = r#"
            PROGRAM Demo
            VAR
                Total : INT := 0;
            END_VAR
            VAR_TEMP
                Scratch : INT := 5;
            END_VAR

            Scratch := Scratch + 1;
            Total := Total + Scratch;
            END_PROGRAM
        "#;
    let output = parse_project("var_temp_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TOTAL" && *value == Value::Int(18)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SCRATCH" && *value == Value::Int(6)));
}

#[test]
fn executes_standard_counter_function_block() {
    let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Pulse : BOOL := FALSE;
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            Pulse := NOT Pulse;
            Counter(CU := Pulse, R := FALSE, PV := 2);
            Count := Counter.CV;
            Done := Counter.Q;
            END_PROGRAM
        "#;
    let output = parse_project("fb_counter.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNTER.CV" && *value == Value::Int(2)));
}

#[test]
fn executes_standard_function_block_positional_inputs() {
    let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                IlCounter : CTU;
                Count : INT := 0;
                IlCount : INT := 0;
                Done : BOOL := FALSE;
                IlDone : BOOL := FALSE;
            END_VAR

            Counter(TRUE, FALSE, 1);
            LD TRUE
            CAL IlCounter(TRUE, FALSE, 1)
            LD Counter.CV
            ST Count
            LD Counter.Q
            ST Done
            LD IlCounter.CV
            ST IlCount
            LD IlCounter.Q
            ST IlDone
            END_PROGRAM
        "#;
    let output = parse_project("fb_positional_inputs.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ILCOUNT" && *value == Value::Int(1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ILDONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_function_block_en_eno_controls() {
    let source = r#"
            PROGRAM Demo
            VAR
                Disabled : CTU;
                Enabled : CTU;
                DisabledOk : BOOL := TRUE;
                EnabledOk : BOOL := FALSE;
            END_VAR

            Disabled(EN := FALSE, CU := TRUE, R := FALSE, PV := 1, ENO => DisabledOk);
            Enabled(EN := TRUE, CU := TRUE, R := FALSE, PV := 1, ENO => EnabledOk);
            END_PROGRAM
        "#;
    let output = parse_project("fb_controls.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DISABLED.CV" && *value == Value::Int(0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ENABLED.CV" && *value == Value::Int(1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DISABLEDOK" && *value == Value::Bool(false)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ENABLEDOK" && *value == Value::Bool(true)));
}

#[test]
fn executes_communication_function_block_through_runtime_hook() {
    struct Hook;

    impl CommunicationHooks for Hook {
        fn execute(&self, invocation: &CommunicationInvocation) -> Option<CommunicationOutcome> {
            assert_eq!(invocation.block, "USEND");
            assert_eq!(invocation.instance, "SENDER");
            assert_eq!(invocation.inputs.get("REQ"), Some(&Value::Bool(true)));
            assert_eq!(invocation.inputs.get("ID"), Some(&Value::Int(7)));
            assert_eq!(invocation.inputs.get("LEN"), Some(&Value::Int(3)));
            Some(CommunicationOutcome {
                outputs: BTreeMap::from([
                    ("done".to_string(), Value::Bool(true)),
                    ("error".to_string(), Value::Bool(false)),
                    ("status".to_string(), Value::Int(42)),
                ]),
            })
        }
    }

    let source = r#"
            PROGRAM Demo
            VAR
                Sender : USEND;
                Done : BOOL := FALSE;
                Error : BOOL := TRUE;
                Status : INT := 0;
                Ok : BOOL := FALSE;
            END_VAR

            Sender(REQ := TRUE, ID := 7, LEN := 3, ENO => Ok);
            Done := Sender.DONE;
            Error := Sender.ERROR;
            Status := Sender.STATUS;
            END_PROGRAM
        "#;
    let output = parse_project("communication_hook.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let trace = run_program_with_communication_hooks(
        &output.project,
        Some("Demo"),
        1,
        &RuntimeOptions::default(),
        &Hook,
    )
    .expect("program should run with communication hook");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ERROR" && *value == Value::Bool(false)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "STATUS" && *value == Value::Int(42)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
}

#[test]
fn executes_user_defined_function_block_state() {
    let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Step : INT := 1;
            END_VAR

            IF Reset THEN
                Total := 0;
            ELSE
                Total := Total + In + Step;
            END_IF;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Acc : Accumulator;
                Out : INT := 0;
            END_VAR

            Acc(In := 2, Reset := FALSE, Total => Out);
            END_PROGRAM
        "#;
    let output = parse_project("user_fb.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "ACC.TOTAL" && *value == Value::Int(6)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OUT" && *value == Value::Int(6)));
}

#[test]
fn executes_user_function_block_positional_inputs_and_inouts() {
    let source = r#"
            FUNCTION_BLOCK Accumulate
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_IN_OUT
                Carry : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR

            IF Reset THEN
                Carry := 0;
            END_IF;
            Carry := Carry + In;
            Total := Carry;
            END_FUNCTION_BLOCK

            FUNCTION_BLOCK Wrapper
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_IN_OUT
                Carry : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Inner : Accumulate;
            END_VAR

            Inner(In, Reset, Carry, Total => Total);
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Block : Wrapper;
                Value : INT := 10;
                Out : INT := 0;
            END_VAR

            Block(2, FALSE, Value, Total => Out);
            END_PROGRAM
        "#;
    let output = parse_project("user_fb_positional.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "VALUE" && *value == Value::Int(12)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OUT" && *value == Value::Int(12)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "BLOCK.INNER.TOTAL" && *value == Value::Int(12)));
}

#[test]
fn executes_user_function_block_input_edge_qualifiers() {
    let source = r#"
            FUNCTION_BLOCK EdgeCounter
            VAR_INPUT
                Start : BOOL R_EDGE;
                Stop : BOOL F_EDGE;
            END_VAR
            VAR_OUTPUT
                RiseCount : INT := 0;
                FallCount : INT := 0;
            END_VAR
            IF Start THEN
                RiseCount := RiseCount + 1;
            END_IF;
            IF Stop THEN
                FallCount := FallCount + 1;
            END_IF;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : EdgeCounter;
                Signal : BOOL := FALSE;
                Rises : INT := 0;
                Falls : INT := 0;
            END_VAR
            Fb(Start := Signal, Stop := Signal, RiseCount => Rises, FallCount => Falls);
            Signal := NOT Signal;
            END_PROGRAM
        "#;
    let output = parse_project("fb_edge_inputs_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 4, &RuntimeOptions::default())
        .expect("program should run");
    let last = &trace.cycles.last().unwrap().variables;
    assert!(last
        .iter()
        .any(|(name, value)| name == "RISES" && *value == Value::Int(2)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "FALLS" && *value == Value::Int(1)));
}

#[test]
fn executes_user_function_block_return_control() {
    let source = r#"
            FUNCTION_BLOCK Gate
            VAR_INPUT
                Stop : BOOL;
            END_VAR
            VAR_OUTPUT
                Count : INT;
                Done : BOOL;
            END_VAR

            IF Stop THEN
                Done := TRUE;
                RETURN;
            END_IF;
            Count := Count + 1;
            Done := FALSE;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Gate;
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            Fb(Stop := TRUE, Count => Count, Done => Done);
            END_PROGRAM
        "#;
    let output = parse_project("user_fb_return.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(0)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_nested_user_defined_function_block_state() {
    let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Step : INT := 1;
            END_VAR
            Total := Total + In + Step;
            END_FUNCTION_BLOCK

            FUNCTION_BLOCK Wrapper
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Inner : Accumulator;
            END_VAR
            Inner(In := In, Total => Total);
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Wrapper;
                Out : INT := 0;
                Mirror : INT := 0;
            END_VAR
            Fb(In := 2, Total => Out);
            Mirror := Fb.Inner.Total;
            END_PROGRAM
        "#;
    let output = parse_project("nested_user_fb_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FB.INNER.TOTAL" && *value == Value::Int(6)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FB.TOTAL" && *value == Value::Int(6)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OUT" && *value == Value::Int(6)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "MIRROR" && *value == Value::Int(6)));
}

#[test]
fn executes_var_in_out_function_block_aliases() {
    let source = r#"
            FUNCTION_BLOCK Bump
            VAR_IN_OUT
                Value : INT;
            END_VAR
            Value := Value + 1;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Bump;
                Count : INT := 1;
            END_VAR

            Fb(Value := Count);
            END_PROGRAM
        "#;
    let output = parse_project("fb_inout.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(3)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FB.VALUE" && *value == Value::Int(3)));
}

#[test]
fn rejects_non_variable_var_in_out_actuals() {
    let source = r#"
            FUNCTION_BLOCK Bump
            VAR_IN_OUT
                Value : INT;
            END_VAR
            Value := Value + 1;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Bump;
            END_VAR

            Fb(Value := 1);
            END_PROGRAM
        "#;
    let output = parse_project("bad_fb_inout.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_IN_OUT parameter 'Value' requires a variable actual")));
}

#[test]
fn executes_bistable_and_edge_function_blocks() {
    let source = r#"
            PROGRAM Demo
            VAR
                Latch : SR;
                Edge : R_TRIG;
                Input : BOOL := FALSE;
                Latched : BOOL := FALSE;
                Rising : BOOL := FALSE;
            END_VAR

            Input := NOT Input;
            Latch(S1 := Input, R := FALSE);
            Edge(CLK := Input);
            Latched := Latch.Q1;
            Rising := Edge.Q;
            END_PROGRAM
        "#;
    let output = parse_project("fb_bits.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
        .expect("program should run");
    let first = &trace.cycles[0];
    assert!(first
        .variables
        .iter()
        .any(|(name, value)| name == "LATCH.Q1" && *value == Value::Bool(true)));
    assert!(first
        .variables
        .iter()
        .any(|(name, value)| name == "RISING" && *value == Value::Bool(true)));

    let second = &trace.cycles[1];
    assert!(second
        .variables
        .iter()
        .any(|(name, value)| name == "LATCH.Q1" && *value == Value::Bool(true)));
    assert!(second
        .variables
        .iter()
        .any(|(name, value)| name == "RISING" && *value == Value::Bool(false)));
}

#[test]
fn executes_timer_function_blocks_with_cycle_time() {
    let source = r#"
            PROGRAM Demo
            VAR
                Delay : TON;
                Pulse : TP;
                Done : BOOL := FALSE;
                PulseDone : BOOL := FALSE;
                Elapsed : TIME := T#0ms;
            END_VAR

            Delay(IN := TRUE, PT := T#2ms);
            Pulse(IN := TRUE, PT := T#2ms);
            Done := Delay.Q;
            PulseDone := Pulse.Q;
            Elapsed := Delay.ET;
            END_PROGRAM
        "#;
    let output = parse_project("timers.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
        .expect("program should run");

    let first = &trace.cycles[0];
    assert!(first
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(false)));
    assert!(first
        .variables
        .iter()
        .any(|(name, value)| name == "PULSEDONE" && *value == Value::Bool(true)));

    let second = &trace.cycles[1];
    assert!(second
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(second
        .variables
        .iter()
        .any(|(name, value)| name == "ELAPSED" && *value == Value::TimeMs(2)));

    let third = &trace.cycles[2];
    assert!(third
        .variables
        .iter()
        .any(|(name, value)| name == "PULSEDONE" && *value == Value::Bool(false)));
}

#[test]
fn executes_textual_sfc_scan_evolution() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start;
            STEP Run;
            TRANSITION Go := Ready;
            ACTION Run:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Sequence"),
        2,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let first = &trace.cycles[0].variables;
    assert!(first
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(false)));
    assert!(first
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_RUN" && *value == Value::Bool(true)));
    let second = &trace.cycles[1].variables;
    assert!(second
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_textual_sfc_il_transition_body() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                Increment(N);
            END_STEP;
            STEP Run:
                Finish(N);
            END_STEP;
            TRANSITION FROM Start TO Run:
                LD Count
                GE 2
            END_TRANSITION;
            ACTION Increment:
                Count := Count + 1;
            END_ACTION;
            ACTION Finish:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_il_transition_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_program(
        &output.project,
        Some("Sequence"),
        4,
        &RuntimeOptions::default(),
    )
    .expect("program should run");

    assert!(trace.cycles[0]
        .variables
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_START" && *value == Value::Bool(true)));
    assert!(trace.cycles[1]
        .variables
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_RUN" && *value == Value::Bool(true)));
    assert!(trace.cycles[2]
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_native_textual_ladder_body() {
    let source = r#"
            PROGRAM NativeLd
            VAR
                Start : BOOL := TRUE;
                Stop : BOOL := FALSE;
                Motor : BOOL := FALSE;
                Latched : BOOL := FALSE;
            END_VAR
            LADDER
            RUNG MotorRun:
                CONTACT Start;
                CONTACT_NOT Stop;
                COIL Motor;
            END_RUNG;
            RUNG Latch:
                CONTACT Start;
                SET Latched;
            END_RUNG;
            END_LADDER
            END_PROGRAM
        "#;
    let output = parse_project("native_ladder_runtime.ld", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_program(
        &output.project,
        Some("NativeLd"),
        1,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let cycle0 = &trace.cycles[0];
    assert!(cycle0
        .variables
        .iter()
        .any(|(name, value)| name == "MOTOR" && *value == Value::Bool(true)));
    assert!(cycle0
        .variables
        .iter()
        .any(|(name, value)| name == "LATCHED" && *value == Value::Bool(true)));
}

#[test]
fn executes_native_textual_fbd_body() {
    let source = r#"
            PROGRAM NativeFbd
            VAR
                A : INT := 2;
                B : INT := 3;
                C : INT := 0;
                Ready : BOOL := FALSE;
            END_VAR
            FBD
            NETWORK Sum:
                OUT C := ADD(A, B);
                OUT Ready := C >= 5;
            END_NETWORK;
            END_FBD
            END_PROGRAM
        "#;
    let output = parse_project("native_fbd_runtime.fbd", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let trace = run_program(
        &output.project,
        Some("NativeFbd"),
        1,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let cycle0 = &trace.cycles[0];
    assert!(cycle0
        .variables
        .iter()
        .any(|(name, value)| name == "C" && *value == Value::Int(5)));
    assert!(cycle0
        .variables
        .iter()
        .any(|(name, value)| name == "READY" && *value == Value::Bool(true)));
}

#[test]
fn executes_native_ld_and_fbd_sfc_transition_bodies() {
    let ladder = r#"
            PROGRAM LdSequence
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                Increment(N);
            END_STEP;
            STEP Run:
                Finish(N);
            END_STEP;
            TRANSITION FROM Start TO Run:
                LADDER
                RUNG Ready:
                    CONTACT Count >= 2;
                END_RUNG;
                END_LADDER
            END_TRANSITION;
            ACTION Increment:
                Count := Count + 1;
            END_ACTION;
            ACTION Finish:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_ld_transition_runtime.st", ladder);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("LdSequence"),
        4,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    assert!(trace.cycles[2]
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));

    let fbd = r#"
            PROGRAM FbdSequence
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                Increment(N);
            END_STEP;
            STEP Run:
                Finish(N);
            END_STEP;
            TRANSITION FROM Start TO Run:
                FBD
                NETWORK Ready:
                    OUT := Count >= 2;
                END_NETWORK;
                END_FBD
            END_TRANSITION;
            ACTION Increment:
                Count := Count + 1;
            END_ACTION;
            ACTION Finish:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_fbd_transition_runtime.st", fbd);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("FbdSequence"),
        4,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    assert!(trace.cycles[2]
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_explicit_sfc_divergence_and_convergence() {
    let source = r#"
            PROGRAM Sequence
            VAR
                ACount : INT := 0;
                BCount : INT := 0;
                DoneCount : INT := 0;
            END_VAR

            INITIAL_STEP Start;
            STEP A;
            STEP B;
            STEP DoneStep;
            TRANSITION Split FROM Start TO (A, B) := TRUE;
            END_TRANSITION;
            TRANSITION Join FROM (A, B) TO DoneStep := TRUE;
            END_TRANSITION;
            ACTION A:
                ACount := ACount + 1;
            END_ACTION;
            ACTION B:
                BCount := BCount + 1;
            END_ACTION;
            ACTION DoneStep:
                DoneCount := DoneCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_explicit_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Sequence"),
        3,
        &RuntimeOptions::default(),
    )
    .expect("program should run");

    let first = &trace.cycles[0].variables;
    assert!(first
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_A" && *value == Value::Bool(true)));
    assert!(first
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_B" && *value == Value::Bool(true)));

    let second = &trace.cycles[1].variables;
    assert!(second
        .iter()
        .any(|(name, value)| name == "ACOUNT" && *value == Value::Int(1)));
    assert!(second
        .iter()
        .any(|(name, value)| name == "BCOUNT" && *value == Value::Int(1)));
    assert!(second
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_DONESTEP" && *value == Value::Bool(true)));

    let third = &trace.cycles[2].variables;
    assert!(third
        .iter()
        .any(|(name, value)| name == "DONECOUNT" && *value == Value::Int(1)));
}

#[test]
fn executes_sfc_transition_priority_conflicts() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Selected : INT := 0;
            END_VAR

            INITIAL_STEP Start;
            STEP Low;
            STEP High;
            TRANSITION LowPriority (PRIORITY := 2) FROM Start TO Low := TRUE;
            END_TRANSITION;
            TRANSITION HighPriority (PRIORITY := 1) FROM Start TO High := TRUE;
            END_TRANSITION;
            ACTION Low:
                Selected := 1;
            END_ACTION;
            ACTION High:
                Selected := 2;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_priority_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Sequence"),
        2,
        &RuntimeOptions::default(),
    )
    .expect("program should run");

    let first = &trace.cycles[0].variables;
    assert!(first
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_LOW" && *value == Value::Bool(false)));
    assert!(first
        .iter()
        .any(|(name, value)| name == "$SFC_STEP_HIGH" && *value == Value::Bool(true)));

    let second = &trace.cycles[1].variables;
    assert!(second
        .iter()
        .any(|(name, value)| name == "SELECTED" && *value == Value::Int(2)));
}

#[test]
fn executes_sfc_action_qualifiers_and_timers() {
    let source = r#"
            PROGRAM Qualifiers
            VAR
                PulseCount : INT := 0;
                DelayCount : INT := 0;
                LimitCount : INT := 0;
            END_VAR

            INITIAL_STEP Pulse;
            STEP Delay;
            STEP Limit;
            TRANSITION ToDelay := PulseCount >= 1;
            TRANSITION ToLimit := DelayCount >= 1;
            ACTION Pulse(P):
                PulseCount := PulseCount + 1;
            END_ACTION;
            ACTION Delay(D, T#2ms):
                DelayCount := DelayCount + 1;
            END_ACTION;
            ACTION Limit(L, T#2ms):
                LimitCount := LimitCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_qualifiers.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Qualifiers"),
        6,
        &RuntimeOptions {
            cycle_time_ms: 1,
            ..RuntimeOptions::default()
        },
    )
    .expect("program should run");
    let last = &trace.cycles.last().unwrap().variables;
    assert!(last
        .iter()
        .any(|(name, value)| name == "PULSECOUNT" && *value == Value::Int(1)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "DELAYCOUNT" && *value == Value::Int(1)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "LIMITCOUNT" && *value == Value::Int(2)));
}

#[test]
fn executes_sfc_step_action_associations() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 0;
                PulseCount : INT := 0;
                DelayCount : INT := 0;
            END_VAR

            INITIAL_STEP Start:
                CountAction(N);
                PulseAction(P);
            END_STEP;
            Running: STEP
                DelayAction(D, T#2ms);
            END_STEP;
            ToRun: TRANSITION FROM Start TO Running := Count >= 2;
            END_TRANSITION;
            CountAction: ACTION
                Count := Count + 1;
            END_ACTION;
            PulseAction: ACTION
                PulseCount := PulseCount + 1;
            END_ACTION;
            DelayAction: ACTION
                DelayCount := DelayCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_associations.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Sequence"),
        5,
        &RuntimeOptions {
            cycle_time_ms: 1,
            ..RuntimeOptions::default()
        },
    )
    .expect("program should run");
    let last = &trace.cycles.last().unwrap().variables;
    assert!(last
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "PULSECOUNT" && *value == Value::Int(1)));
    assert!(last
        .iter()
        .any(|(name, value)| name == "DELAYCOUNT" && *value == Value::Int(2)));
}

#[test]
fn executes_sfc_action_control_set_reset_across_steps() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 0;
            END_VAR

            INITIAL_STEP SetStep:
                Shared(S);
            END_STEP;
            STEP ResetStep:
                Shared(R);
            END_STEP;
            TRANSITION LeaveSet FROM SetStep TO ResetStep := Count >= 2;
            END_TRANSITION;
            Shared: ACTION
                Count := Count + 1;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_action_control_reset.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Sequence"),
        4,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let last = &trace.cycles.last().unwrap().variables;
    assert!(last
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
    assert!(last
        .iter()
        .any(|(name, value)| { name == "$SFC_ACTION_SHARED" && *value == Value::Bool(false) }));
}

#[test]
fn executes_sfc_falling_pulse_action_qualifier() {
    let source = r#"
            PROGRAM Sequence
            VAR
                ExitCount : INT := 0;
            END_VAR
            INITIAL_STEP RunExit;
            STEP Done;
            TRANSITION Leave := TRUE;
            ACTION RunExit(P0):
                ExitCount := ExitCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("sfc_p0.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("Sequence"),
        2,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let first = &trace.cycles[0].variables;
    let second = &trace.cycles[1].variables;
    assert!(first
        .iter()
        .any(|(name, value)| name == "EXITCOUNT" && *value == Value::Int(0)));
    assert!(second
        .iter()
        .any(|(name, value)| name == "EXITCOUNT" && *value == Value::Int(1)));
}

#[test]
fn executes_basic_instruction_list() {
    let source = r#"
            PROGRAM IlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Bigger : BOOL := FALSE;
                Complex : BOOL := FALSE;
            END_VAR

            LD A
            ADD B
            ST C
            GT 5
            ST Bigger
            LD TRUE
            AND (Bigger OR FALSE)
            ST Complex
            END_PROGRAM
        "#;
    let output = parse_project("il.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("IlDemo"),
        1,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "C" && *value == Value::Int(7)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "BIGGER" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COMPLEX" && *value == Value::Bool(true)));
}

#[test]
fn executes_typed_instruction_list_operators() {
    let source = r#"
            PROGRAM TypedIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            LD_INT A;
            ADD_INT B;
            ST_INT C;
            LD_BOOL TRUE;
            AND_BOOL (C = 7);
            ST_BOOL Flag;
            END_PROGRAM
        "#;
    let output = parse_project("typed_il_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("TypedIlDemo"),
        1,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "C" && *value == Value::Int(7)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
}

#[test]
fn executes_instruction_list_parenthesized_expression_lists() {
    let source = r#"
            PROGRAM NestedIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 2;
                Total : INT := 0;
                Good : BOOL := FALSE;
            END_VAR

            LD A;
            ADD (
                LD B;
                MUL (
                    LD C;
                    ADD 1;
                );
            );
            ST Total;
            LD TRUE;
            AND (
                LD Total;
                EQ 15;
            );
            ST Good;
            END_PROGRAM
        "#;
    let output = parse_project("nested_il_runtime.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("NestedIlDemo"),
        1,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "TOTAL" && *value == Value::Int(15)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "GOOD" && *value == Value::Bool(true)));
}

#[test]
fn executes_instruction_list_jumps() {
    let source = r#"
            PROGRAM IlJumpDemo
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            LD Count;
            GE 3;
            JMPC DoneLabel;
            LD Count;
            ADD 1;
            ST Count;
            JMP EndLabel;
            DoneLabel:
            LD TRUE;
            ST Done;
            EndLabel:
            END_PROGRAM
        "#;
    let output = parse_project("il_jump.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(
        &output.project,
        Some("IlJumpDemo"),
        4,
        &RuntimeOptions::default(),
    )
    .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "COUNT" && *value == Value::Int(3)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
}

#[test]
fn executes_instruction_list_calls_and_conditional_returns() {
    let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Done : BOOL := FALSE;
                Cv : INT := 0;
                Skipped : INT := 0;
            END_VAR

            LD TRUE;
            CALC Counter(CU := TRUE, R := FALSE, PV := 1);
            LD Counter.Q;
            ST Done;
            LD Counter.CV;
            ST Cv;
            LD TRUE;
            RETC;
            Skipped := 1;
            END_PROGRAM
        "#;
    let output = parse_project("il_calls.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "CV" && *value == Value::Int(1)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SKIPPED" && *value == Value::Int(0)));
}

#[test]
fn executes_instruction_list_simple_and_negated_call_forms() {
    let source = r#"
            PROGRAM Demo
            VAR
                CountUp : CTU;
                Skipped : CTU;
                Done : BOOL := FALSE;
                SkippedCv : INT := 0;
            END_VAR

            CountUp(CU := TRUE, R := FALSE, PV := 2);
            LD FALSE;
            CALCN CountUp;
            LD TRUE;
            CALCN Skipped(CU := TRUE, R := FALSE, PV := 1);
            LD CountUp.Q;
            ST Done;
            LD Skipped.CV;
            ST SkippedCv;
            END_PROGRAM
        "#;
    let output = parse_project("il_call_forms.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "SKIPPEDCV" && *value == Value::Int(0)));
}

#[test]
fn executes_instruction_list_user_fb_positional_call() {
    let source = r#"
            FUNCTION_BLOCK Accumulate
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_IN_OUT
                Carry : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR

            IF Reset THEN
                Carry := 0;
            END_IF;
            Carry := Carry + In;
            Total := Carry;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Accumulate;
                Value : INT := 10;
                Out : INT := 0;
            END_VAR

            LD TRUE
            CAL Fb(2, FALSE, Value, Total => Out)
            END_PROGRAM
        "#;
    let output = parse_project("il_user_fb_positional.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
        .expect("program should run");
    let last = trace.cycles.last().unwrap();
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "VALUE" && *value == Value::Int(12)));
    assert!(last
        .variables
        .iter()
        .any(|(name, value)| name == "OUT" && *value == Value::Int(12)));
}
