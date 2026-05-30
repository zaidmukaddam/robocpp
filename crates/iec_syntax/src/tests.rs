use iec_profile::ImplementationParameters;

use super::*;

#[test]
fn parses_simple_program() {
    let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 1;
                B : BOOL;
            END_VAR
            IF A < 5 THEN
                A := A + 1;
            ELSE
                B := TRUE;
            END_IF;
            END_PROGRAM
        "#;

    let output = parse_project("test.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.project.pous().count(), 1);
    let pou = output.project.first_program().unwrap();
    assert_eq!(pou.name.canonical, "DEMO");
    assert_eq!(pou.variable_declarations().count(), 2);
}

#[test]
fn enforces_comment_and_pragma_implementation_limits() {
    let source = r#"
            { vendor_hint }
            PROGRAM Demo
            (* longer than the configured limit *)
            VAR A : INT; END_VAR
            END_PROGRAM
        "#;
    let output = parse_project_with_options(
        "limits.st",
        source,
        &ParseOptions {
            implementation: ImplementationParameters {
                max_comment_length: 8,
                pragmas_enabled: false,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("pragmas are disabled by implementation parameters")));
    assert!(output
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("comment length")));

    let output = parse_project_with_options(
        "limits_ok.st",
        "{ vendor_hint } PROGRAM Demo VAR A : INT; END_VAR END_PROGRAM",
        &ParseOptions {
            implementation: ImplementationParameters {
                pragmas_enabled: true,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn enforces_array_literal_repetition_limit_during_parse() {
    let source = r#"
            PROGRAM Demo
            VAR
                Values : ARRAY [1..10] OF INT := [5(1)];
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project_with_options(
        "array_repeat_limit.st",
        source,
        &ParseOptions {
            implementation: ImplementationParameters {
                max_array_elements: 4,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array literal repetition count 5 exceeds maximum 4")));
}

#[test]
fn rejects_huge_malformed_array_repetition_without_expansion() {
    let source = r#"
            PROGRAM Demo
            VAR
                Motor : BOOL := [[00000000000000001629084365346150(096[[[[[[[[LSE;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("array_repeat_fuzz_regression.st", source);
    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array literal repetition count 1629084365346150 exceeds maximum 1000000")));
}

#[test]
fn parses_duration_literal() {
    assert_eq!(parse_hash_literal("T#1s"), Literal::DurationMs(1000));
    assert_eq!(
        parse_hash_literal("TIME#2m_500ms"),
        Literal::DurationMs(120500)
    );
}

#[test]
fn lexes_typed_enum_case_label_before_colon() {
    let source = r#"
            TYPE Mode : (Idle, Run); END_TYPE
            PROGRAM Demo
            VAR State : Mode := Idle; END_VAR
            CASE State OF
                Mode#Run: State := Idle;
            END_CASE;
            END_PROGRAM
        "#;
    let output = parse_project("typed_enum_case_label.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    let Statement::Case { cases, .. } = &pou.body.statements[0] else {
        panic!("expected CASE");
    };
    let CaseLabel::Single(Expr::Literal(Literal::Typed { type_name, value })) = &cases[0].0[0]
    else {
        panic!("expected typed enum literal label");
    };
    assert_eq!(type_name.original, "Mode");
    assert_eq!(value, "Run");
}

#[test]
fn diagnoses_invalid_literal_forms() {
    let source = r#"
            PROGRAM BadLiterals
            VAR
                A : INT := 0;
                B : TIME := T#0ms;
                C : DATE := D#1970-01-01;
                D : TIME_OF_DAY := TOD#00:00:00;
                E : BOOL := FALSE;
                F : TIME := T#0ms;
            END_VAR
            A := 2#102;
            B := T#1h_75m;
            C := D#2023-02-29;
            D := TOD#24:00:00;
            E := BOOL#YES;
            F := T#1m_1h;
            F := T#1.5h_1m;
            F := T#1s_1000ms;
            END_PROGRAM
        "#;
    let output = parse_project("bad_literals.st", source);
    let messages = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    assert!(messages
        .iter()
        .any(|message| message.contains("invalid base-2 digit sequence")));
    assert!(messages
        .iter()
        .any(|message| message.contains("duration m component 75 exceeds 59")));
    assert!(
        messages
            .iter()
            .any(|message| message
                .contains("duration components must be ordered largest to smallest"))
    );
    assert!(messages
        .iter()
        .any(|message| message.contains("fractional duration component '1.5h' must be last")));
    assert!(messages
        .iter()
        .any(|message| message.contains("duration milliseconds component 1000 exceeds 999")));
    assert!(messages
        .iter()
        .any(|message| message.contains("invalid DATE literal")));
    assert!(messages
        .iter()
        .any(|message| message.contains("invalid TIME_OF_DAY literal")));
    assert!(messages
        .iter()
        .any(|message| message.contains("invalid BOOL literal value")));
}

#[test]
fn parses_wstring_literals_distinct_from_string_literals() {
    let source = r#"
            PROGRAM WideText
            VAR
                Narrow : STRING[8] := 'robot';
                Wide : WSTRING[8] := "robot";
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("wstring.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    let vars = pou.variable_declarations().collect::<Vec<_>>();
    assert!(matches!(
        vars[0].initial_value,
        Some(Expr::Literal(Literal::String(_)))
    ));
    assert!(matches!(
        vars[1].initial_value,
        Some(Expr::Literal(Literal::WString(_)))
    ));
}

#[test]
fn parses_iec_character_string_escapes() {
    let source = r#"
            PROGRAM Escapes
            VAR
                Narrow : STRING[8] := 'A$0A$27$$';
                Wide : WSTRING[8] := "$0041$000A$0022$$";
                TypedNarrow : STRING[8] := STRING#'OK$21';
                TypedWide : WSTRING[8] := WSTRING#'A$000A';
                NarrowQuoted : STRING[8] := 'A$"B$'';
                WideQuoted : WSTRING[8] := "A$'B$"";
                TypedNarrowQuoted : STRING[8] := STRING#'A$"B$'';
                TypedWideQuoted : WSTRING[8] := WSTRING#"A$'B$"";
                NamedEscapes : STRING[8] := '$L$N$P$R$T$$$'$"';
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("string_escapes.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    let vars = pou.variable_declarations().collect::<Vec<_>>();
    assert_eq!(
        vars[0].initial_value,
        Some(Expr::Literal(Literal::String("A\n'$".to_string())))
    );
    assert_eq!(
        vars[1].initial_value,
        Some(Expr::Literal(Literal::WString("A\n\"$".to_string())))
    );
    assert_eq!(
        vars[2].initial_value,
        Some(Expr::Literal(Literal::String("OK!".to_string())))
    );
    assert_eq!(
        vars[3].initial_value,
        Some(Expr::Literal(Literal::WString("A\n".to_string())))
    );
    assert_eq!(
        vars[4].initial_value,
        Some(Expr::Literal(Literal::String("A\"B'".to_string())))
    );
    assert_eq!(
        vars[5].initial_value,
        Some(Expr::Literal(Literal::WString("A'B\"".to_string())))
    );
    assert_eq!(
        vars[6].initial_value,
        Some(Expr::Literal(Literal::String("A\"B'".to_string())))
    );
    assert_eq!(
        vars[7].initial_value,
        Some(Expr::Literal(Literal::WString("A'B\"".to_string())))
    );
    assert_eq!(
        vars[8].initial_value,
        Some(Expr::Literal(Literal::String(
            "\n\n\u{000C}\r\t$'\"".to_string()
        )))
    );
}

#[test]
fn diagnoses_invalid_character_string_escapes() {
    let source = r#"
            PROGRAM BadEscapes
            VAR
                BadCommon : STRING[8] := 'bad$Q';
                BadSingleHex : STRING[8] := 'bad$0G';
                BadWideHex : WSTRING[8] := "$00Q1";
                BadLine : STRING[16] := 'bad
line';
                BadTypedLine : STRING[16] := STRING#'bad
line';
                BadSingleByte : STRING[8] := 'badλ';
                BadTypedSingleByte : STRING[8] := STRING#'badλ';
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("bad_string_escapes.st", source);
    let messages = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert!(messages
        .iter()
        .any(|message| message.contains("invalid character string escape '$Q'")));
    assert!(messages.iter().any(|message| message
        .contains("invalid character string hex escape '$0': expected 2 hexadecimal digit(s)")));
    assert!(messages.iter().any(|message| message
        .contains("invalid character string hex escape '$00': expected 4 hexadecimal digit(s)")));
    assert_eq!(
        messages
            .iter()
            .filter(|message| message
                .contains("unescaped control character U+000A in character string literal"))
            .count(),
        2
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.contains("character U+03BB exceeds single-byte STRING range"))
            .count(),
        2
    );
}

#[test]
fn parses_derived_types_and_control_flow() {
    let source = r#"
            TYPE
                Speed : INT := 0;
                Mode : (Idle, Run, Fault);
                Window : STRUCT
                    Low : INT := 1;
                    High : INT := 10;
                END_STRUCT;
                Buffer : ARRAY [1..4] OF INT;
            END_TYPE

            PROGRAM Demo
            VAR
                I : INT := 0;
                Total : INT := 0;
                Done : BOOL := FALSE;
                Values : ARRAY [1..3] OF INT := [1, 2, 3];
                Limits : Window := (Low := 2, High := 8);
            END_VAR

            FOR I := 1 TO 3 DO
                Total := Total + I;
            END_FOR;

            WHILE Total < 10 DO
                Total := Total + 1;
            END_WHILE;

            REPEAT
                Total := Total - 1;
            UNTIL Total = 9
            END_REPEAT;

            CASE Total OF
                0..8: Done := FALSE;
                9: Done := TRUE;
                ELSE Done := FALSE;
            END_CASE;
            END_PROGRAM
        "#;

    let output = parse_project("control.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.project.data_types().count(), 4);
    let pou = output.project.first_program().unwrap();
    assert!(pou
        .variable_declarations()
        .any(|var| matches!(var.initial_value, Some(Expr::ArrayLiteral(_)))));
    assert!(pou
        .variable_declarations()
        .any(|var| matches!(var.initial_value, Some(Expr::StructLiteral(_)))));
    assert_eq!(pou.body.statements.len(), 4);
}

#[test]
fn parses_repeated_array_initializers() {
    let source = r#"
            PROGRAM Demo
            VAR
                Values : ARRAY [1..5] OF INT := [2(1), 3(5)];
            END_VAR
            END_PROGRAM
        "#;

    let output = parse_project("array_repeat.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    let values = pou
        .variable_declarations()
        .find(|var| var.name.canonical == "VALUES")
        .and_then(|var| var.initial_value.as_ref())
        .expect("Values initializer should parse");
    let Expr::ArrayLiteral(elements) = values else {
        panic!("expected array literal");
    };
    assert_eq!(elements.len(), 5);
}

#[test]
fn parses_basic_instruction_list_statements() {
    let source = r#"
            PROGRAM IlDemo
            VAR
                A : INT := 1;
                B : INT := 2;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            LD A;
            ADD B;
            Done:
            ST C;
            LD TRUE;
            AND (C > 0);
            ST Flag;
            END_PROGRAM
        "#;

    let output = parse_project("il.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    assert!(matches!(pou.body.statements[0], Statement::Il { .. }));
    assert!(matches!(pou.body.statements[2], Statement::IlLabel(_)));
    assert_eq!(pou.body.statements.len(), 7);
}

#[test]
fn parses_line_oriented_instruction_list_without_semicolons() {
    let source = r#"
            PROGRAM LineIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            LD A
            ADD B
            Done:
            ST C
            LD TRUE
            AND (C > 0)
            ST Flag
            END_PROGRAM
        "#;

    let output = parse_project("line_il.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let statements = &output.project.first_program().unwrap().body.statements;
    assert_eq!(statements.len(), 7);
    assert!(matches!(statements[0], Statement::Il { op: IlOp::Ld, .. }));
    assert!(matches!(statements[2], Statement::IlLabel(_)));
    assert!(matches!(statements[6], Statement::Il { op: IlOp::St, .. }));
}

#[test]
fn parses_typed_instruction_list_mnemonics() {
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

    let output = parse_project("typed_il.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let statements = &output.project.first_program().unwrap().body.statements;
    assert!(matches!(statements[0], Statement::Il { op: IlOp::Ld, .. }));
    assert!(matches!(statements[1], Statement::Il { op: IlOp::Add, .. }));
    assert!(matches!(statements[5], Statement::Il { op: IlOp::St, .. }));
}

#[test]
fn parses_instruction_list_parenthesized_expression_lists() {
    let source = r#"
            PROGRAM NestedIlDemo
            VAR
                A : BOOL := TRUE;
                B : BOOL := FALSE;
                C : BOOL := TRUE;
                Out : BOOL := FALSE;
            END_VAR

            LD TRUE
            AND (
                LD A
                OR (
                    LD B
                    ANDN C
                )
            )
            ST Out
            END_PROGRAM
        "#;

    let output = parse_project("nested_il.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let statements = &output.project.first_program().unwrap().body.statements;
    let Statement::Il {
        op: IlOp::And,
        operand: Some(Expr::Binary { op, .. }),
    } = &statements[1]
    else {
        panic!("expected nested IL expression to lower into a binary operand");
    };
    assert_eq!(*op, BinaryOp::Or);
}

#[test]
fn parses_configuration_resources_tasks_and_program_instances() {
    let source = r#"
            PROGRAM Demo
            VAR A : INT := 0; END_VAR
            A := A + 1;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Shared : INT := 1;
            END_VAR
            VAR_ACCESS
                SharedAccess : Shared : INT READ_WRITE;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_CONFIG
                    Tunable : INT := 2;
                END_VAR
                VAR_ACCESS
                    InputBit : %IX1.1 : BOOL READ_ONLY;
                    MainA : Main.A : INT;
                END_VAR
                TASK Fast(SINGLE := Shared > 0, INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Demo(A := 5);
            END_RESOURCE
            END_CONFIGURATION
        "#;

    let output = parse_project("config.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let configuration = output
        .project
        .library_elements
        .iter()
        .find_map(|element| {
            if let LibraryElement::Configuration(configuration) = element {
                Some(configuration)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(configuration.var_blocks.len(), 2);
    assert_eq!(configuration.resources.len(), 1);
    assert_eq!(configuration.resources[0].var_blocks.len(), 2);
    assert_eq!(configuration.resources[0].tasks.len(), 1);
    assert!(matches!(
        configuration.resources[0].tasks[0].single,
        Some(Expr::Binary {
            op: BinaryOp::Greater,
            ..
        })
    ));
    assert_eq!(configuration.resources[0].program_instances.len(), 1);
    assert_eq!(
        configuration.resources[0].program_instances[0].args.len(),
        1
    );
    let access = &configuration.var_blocks[1].vars[0].access;
    assert!(matches!(
        access.as_ref().map(|access| access.direction),
        Some(AccessDirection::ReadWrite)
    ));
    assert_eq!(
        configuration.resources[0].var_blocks[1].vars[0]
            .access
            .as_ref()
            .map(|access| access.path.as_str()),
        Some("%IX1.1")
    );
}

#[test]
fn parses_textual_sfc_body() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                MarkDone(P);
            END_STEP;
            Running: STEP
                MarkDone(L, T#5ms);
            END_STEP;
            STEP DoneStep;
            Go: TRANSITION FROM Start TO (Running, DoneStep) := Ready;
            END_TRANSITION;
            MarkDone: ACTION (L, T#5ms)
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;

    let output = parse_project("sfc.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    assert_eq!(
        pou.body.language,
        ImplementationLanguage::SequentialFunctionChart
    );
    let sfc = pou.body.sfc.as_ref().unwrap();
    assert_eq!(sfc.steps.len(), 3);
    assert!(sfc.steps[0].initial);
    assert_eq!(sfc.steps[0].actions.len(), 1);
    assert_eq!(sfc.steps[0].actions[0].name.canonical, "MARKDONE");
    assert_eq!(
        sfc.steps[0].actions[0].qualifier,
        Some(SfcActionQualifier::Pulse)
    );
    assert_eq!(sfc.steps[1].name.canonical, "RUNNING");
    assert_eq!(sfc.steps[1].actions.len(), 1);
    assert_eq!(
        sfc.steps[1].actions[0].qualifier,
        Some(SfcActionQualifier::TimeLimited)
    );
    assert_eq!(
        sfc.steps[1].actions[0].duration,
        Some(Literal::DurationMs(5))
    );
    assert_eq!(sfc.transitions.len(), 1);
    assert_eq!(
        sfc.transitions[0]
            .name
            .as_ref()
            .map(|name| name.canonical.as_str()),
        Some("GO")
    );
    assert_eq!(sfc.transitions[0].from[0].canonical, "START");
    assert_eq!(sfc.transitions[0].to.len(), 2);
    assert_eq!(sfc.transitions[0].to[1].canonical, "DONESTEP");
    assert_eq!(sfc.actions.len(), 1);
    assert_eq!(sfc.actions[0].qualifier, SfcActionQualifier::TimeLimited);
    assert_eq!(sfc.actions[0].duration, Some(Literal::DurationMs(5)));
    assert_eq!(sfc.actions[0].body.len(), 1);
}

#[test]
fn parses_textual_sfc_il_transition_bodies() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 2;
            END_VAR
            INITIAL_STEP Start;
            STEP Done;
            TRANSITION FROM Start TO Done:
                LD Count
                GE 2
            END_TRANSITION;
            END_PROGRAM
        "#;

    let output = parse_project("sfc_il_transition_body.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    let transition = &pou.body.sfc.as_ref().unwrap().transitions[0];
    assert!(matches!(
        transition.condition,
        Some(Expr::Binary {
            op: BinaryOp::GreaterEqual,
            ..
        })
    ));
}

#[test]
fn parses_native_textual_ladder_and_fbd_bodies() {
    let ladder = r#"
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
    let output = parse_project("native_ladder.ld", ladder);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    assert_eq!(pou.body.language, ImplementationLanguage::LadderDiagram);
    assert_eq!(pou.body.networks.len(), 2);
    assert_eq!(pou.body.statements.len(), 2);
    assert!(matches!(
        pou.body.statements.first(),
        Some(Statement::Assignment { target, .. }) if target.to_string() == "Motor"
    ));

    let fbd = r#"
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
    let output = parse_project("native_fbd.fbd", fbd);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    assert_eq!(
        pou.body.language,
        ImplementationLanguage::FunctionBlockDiagram
    );
    assert_eq!(pou.body.networks.len(), 1);
    assert_eq!(pou.body.statements.len(), 2);
}

#[test]
fn parses_native_ld_and_fbd_sfc_transition_bodies() {
    let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 2;
            END_VAR
            INITIAL_STEP Start;
            STEP Middle;
            STEP Done;
            TRANSITION FROM Start TO Middle:
                LADDER
                RUNG Ready:
                    CONTACT Count >= 2;
                END_RUNG;
                END_LADDER
            END_TRANSITION;
            TRANSITION FROM Middle TO Done:
                FBD
                NETWORK Ready:
                    OUT := Count >= 2;
                END_NETWORK;
                END_FBD
            END_TRANSITION;
            END_PROGRAM
        "#;

    let output = parse_project("sfc_native_ld_fbd_transition.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();
    let transitions = &pou.body.sfc.as_ref().unwrap().transitions;
    assert_eq!(transitions.len(), 2);
    assert!(matches!(
        transitions[0].condition,
        Some(Expr::Binary {
            op: BinaryOp::And,
            ..
        })
    ));
    assert!(matches!(
        transitions[1].condition,
        Some(Expr::Binary {
            op: BinaryOp::GreaterEqual,
            ..
        })
    ));
}

#[test]
fn diagnoses_unsupported_statements_during_parsing() {
    let source = r#"
            PROGRAM BadStatement
            VAR
                A : INT;
            END_VAR
            GOTO Somewhere;
            END_PROGRAM
        "#;

    let output = parse_project("unsupported_statement.st", source);
    assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unsupported or invalid statement")));
    let pou = output.project.first_program().unwrap();
    assert!(matches!(
        pou.body.statements.first(),
        Some(Statement::Unsupported(_))
    ));
}

#[test]
fn parses_function_block_input_edge_qualifiers() {
    let source = r#"
            FUNCTION_BLOCK EdgeInputs
            VAR_INPUT
                Start : BOOL R_EDGE;
                Stop : BOOL F_EDGE;
            END_VAR
            END_FUNCTION_BLOCK
        "#;

    let output = parse_project("edge_qualifiers.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.find_pou("EdgeInputs").unwrap();
    let vars = pou.variable_declarations().collect::<Vec<_>>();
    assert_eq!(vars[0].edge, Some(EdgeQualifier::Rising));
    assert_eq!(vars[1].edge, Some(EdgeQualifier::Falling));
}

#[test]
fn parser_golden_corpus_covers_language_families() {
    let cases = [
        (
            "structured-text",
            r#"
                PROGRAM Demo
                VAR A : INT := 0; END_VAR
                IF A < 5 THEN A := A + 1; END_IF;
                END_PROGRAM
                "#,
            ImplementationLanguage::StructuredText,
        ),
        (
            "data-types",
            r#"
                TYPE
                    Percent : INT(0..100);
                    Mode : (Idle, Armed, Fault);
                    Pair : STRUCT
                        Low : Percent := 1;
                        High : Percent := 99;
                    END_STRUCT;
                    Samples : ARRAY [1..2, 0..1] OF Percent;
                END_TYPE
                PROGRAM Demo
                VAR
                    Window : Pair := (Low := 10, High := 20);
                    State : Mode := Armed;
                    Grid : Samples := [1, 2, 3, 4];
                END_VAR
                Window.Low := Grid[1, 0];
                END_PROGRAM
                "#,
            ImplementationLanguage::StructuredText,
        ),
        (
            "functions",
            r#"
                FUNCTION Scale : INT
                VAR_INPUT
                    Input : INT;
                    Factor : INT;
                END_VAR
                VAR_TEMP
                    Temp : INT;
                END_VAR
                Temp := Input * Factor;
                Scale := Temp;
                RETURN;
                END_FUNCTION

                PROGRAM Demo
                VAR
                    Out : INT := 0;
                END_VAR
                Out := Scale(Input := 2, Factor := 3);
                END_PROGRAM
                "#,
            ImplementationLanguage::StructuredText,
        ),
        (
            "function-blocks",
            r#"
                FUNCTION_BLOCK Accumulator
                VAR_INPUT
                    Enable : BOOL;
                END_VAR
                VAR_IN_OUT
                    Total : INT;
                END_VAR
                VAR_OUTPUT
                    Done : BOOL;
                END_VAR
                IF Enable THEN
                    Total := Total + 1;
                END_IF;
                Done := Total >= 2;
                END_FUNCTION_BLOCK

                PROGRAM Demo
                VAR
                    Fb : Accumulator;
                    Count : INT := 0;
                    Done : BOOL := FALSE;
                END_VAR
                Fb(Enable := TRUE, Total := Count, Done => Done);
                END_PROGRAM
                "#,
            ImplementationLanguage::StructuredText,
        ),
        (
            "instruction-list",
            r#"
                PROGRAM Demo
                VAR A : INT := 0; END_VAR
                LD 1;
                ST A;
                END_PROGRAM
                "#,
            ImplementationLanguage::StructuredText,
        ),
        (
            "textual-sfc",
            r#"
                PROGRAM Demo
                VAR Ready : BOOL := TRUE; END_VAR
                INITIAL_STEP Start;
                STEP Run;
                TRANSITION T1 := Ready;
                ACTION Run(P):
                    Ready := FALSE;
                END_ACTION;
                END_PROGRAM
                "#,
            ImplementationLanguage::SequentialFunctionChart,
        ),
        (
            "configuration",
            r#"
                PROGRAM Demo END_PROGRAM
                CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Demo;
                END_RESOURCE
                END_CONFIGURATION
                "#,
            ImplementationLanguage::StructuredText,
        ),
    ];

    for (name, source, expected_language) in cases {
        let output = parse_project(format!("golden_{name}.st"), source);
        assert!(
            output.diagnostics.is_empty(),
            "{name}: {:?}",
            output.diagnostics
        );
        let pou = output
            .project
            .first_program()
            .expect("program should parse");
        assert_eq!(pou.body.language, expected_language, "{name}");
    }
}

#[test]
fn pseudo_fuzz_corpus_covers_literals_comments_identifiers_and_precedence() {
    let mut seed = 0x0006_1131_2003_u64;

    for index in 0..96_u64 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let a = (seed & 0x3f) as i64;
        let b = ((seed >> 8) & 0x1f) as i64 + 1;
        let c = ((seed >> 16) & 0x0f) as i64;
        let comment = if index % 3 == 0 {
            "(* generated literal/comment case *)"
        } else if index % 3 == 1 {
            "(* alternate generated comment *)"
        } else {
            ""
        };
        let source = format!(
            r#"
                {comment}
                PROGRAM Fuzz{index}
                VAR
                    Value_{index} : DINT := {a};
                    Flag_{index} : BOOL := FALSE;
                    Text_{index} : STRING[32] := 'seed$20{index}';
                END_VAR
                Value_{index} := (({a} + {b}) * ({c} + 1)) - ({b} ** 2);
                Flag_{index} := (Value_{index} >= 0) AND NOT FALSE;
                Text_{index} := CONCAT(LEFT(Text_{index}, 4), RIGHT('robot', 2));
                END_PROGRAM
                "#
        );
        let output = parse_project(format!("pseudo_fuzz_{index}.st"), &source);
        assert!(
            output.diagnostics.is_empty(),
            "case {index}: {:?}",
            output.diagnostics
        );
        let pou = output.project.first_program().unwrap();
        assert_eq!(pou.name.canonical, format!("FUZZ{index}"));
        assert_eq!(pou.body.statements.len(), 3);
    }
}

#[test]
fn literal_property_corpus_parses_generated_values() {
    for index in 0..64_i64 {
        let signed = index * 257 - 4096;
        let duration = index * 17 + 1;
        let source = format!(
            r#"
                PROGRAM Literals
                VAR
                    A : LINT := 0;
                    B : REAL := 0.0;
                    C : TIME := T#0ms;
                    D : STRING[32] := '';
                END_VAR
                A := {signed};
                B := {index}.25;
                C := T#{duration}ms;
                D := 'case_{index}';
                END_PROGRAM
                "#
        );
        let output = parse_project(format!("literal_property_{index}.st"), &source);
        assert!(
            output.diagnostics.is_empty(),
            "case {index}: {:?}",
            output.diagnostics
        );
        let pou = output.project.first_program().unwrap();
        assert_eq!(pou.body.statements.len(), 4);
    }
}

#[test]
fn comment_and_identifier_property_corpus_parses_generated_programs() {
    let identifier_cases = [
        ("CamelCase", "CAMELCASE"),
        ("snake_case", "SNAKE_CASE"),
        ("MIXED_123_Name", "MIXED_123_NAME"),
        ("CaseFold", "CASEFOLD"),
    ];
    let comment_cases = [
        ("(* leading block comment *)", ""),
        ("", "(* trailing statement comment *)"),
        ("(* implementation note *)", ""),
        ("", "(* after assignment *)"),
    ];

    for (index, (name, canonical)) in identifier_cases.into_iter().enumerate() {
        let (prefix_comment, suffix_comment) = comment_cases[index];
        let source = format!(
            r#"
                {prefix_comment}
                PROGRAM Commented
                VAR
                    {name} : INT := 0;
                END_VAR
                {name} := {name} + 1; {suffix_comment}
                END_PROGRAM
                "#
        );
        let output = parse_project(format!("comment_identifier_{index}.st"), &source);
        assert!(
            output.diagnostics.is_empty(),
            "case {index}: {:?}",
            output.diagnostics
        );
        let pou = output.project.first_program().unwrap();
        let var = pou.variable_declarations().next().unwrap();
        assert_eq!(var.name.original, name);
        assert_eq!(var.name.canonical, canonical);
        assert_eq!(pou.body.statements.len(), 1);
    }
}

#[test]
fn operator_precedence_property_corpus_builds_expected_ast_shapes() {
    let source = r#"
            PROGRAM Precedence
            VAR
                A : INT := 0;
                B : BOOL := FALSE;
                C : INT := 0;
            END_VAR
            A := 1 + 2 * 3 ** 2;
            B := TRUE OR FALSE XOR TRUE AND NOT FALSE;
            C := +1 + +(+2);
            END_PROGRAM
        "#;
    let output = parse_project("precedence_property.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let pou = output.project.first_program().unwrap();

    let Statement::Assignment { value: numeric, .. } = &pou.body.statements[0] else {
        panic!("first statement should be assignment");
    };
    assert_binary_shape(numeric, BinaryOp::Add);
    let Expr::Binary { right, .. } = numeric else {
        unreachable!();
    };
    assert_binary_shape(right, BinaryOp::Mul);
    let Expr::Binary { right, .. } = right.as_ref() else {
        unreachable!();
    };
    assert_binary_shape(right, BinaryOp::Power);

    let Statement::Assignment { value: boolean, .. } = &pou.body.statements[1] else {
        panic!("second statement should be assignment");
    };
    assert_binary_shape(boolean, BinaryOp::Or);
    let Expr::Binary { right, .. } = boolean else {
        unreachable!();
    };
    assert_binary_shape(right, BinaryOp::Xor);
    let Expr::Binary { right, .. } = right.as_ref() else {
        unreachable!();
    };
    assert_binary_shape(right, BinaryOp::And);
    let Expr::Binary { right, .. } = right.as_ref() else {
        unreachable!();
    };
    assert!(matches!(
        right.as_ref(),
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));

    let Statement::Assignment { value: plus, .. } = &pou.body.statements[2] else {
        panic!("third statement should be assignment");
    };
    assert_binary_shape(plus, BinaryOp::Add);
}

fn assert_binary_shape(expr: &Expr, expected: BinaryOp) {
    assert!(
        matches!(expr, Expr::Binary { op, .. } if *op == expected),
        "expected {expected:?}, got {expr:?}"
    );
}
