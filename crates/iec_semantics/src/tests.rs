use iec_diagnostics::diagnostics_to_json;
use iec_profile::ImplementationParameters;
use iec_syntax::parse_project;

use super::*;

#[test]
fn flags_unknown_variable() {
    let source = r#"
            PROGRAM Demo
            VAR A : INT; END_VAR
            B := A + 1;
            END_PROGRAM
        "#;
    let output = parse_project("test.st", source);
    assert!(output.diagnostics.is_empty());
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown variable 'B'")));
}

#[test]
fn accepts_derived_type_reference_and_standard_function() {
    let source = r#"
            TYPE
                MyInt : INT;
            END_TYPE

            PROGRAM Demo
            VAR
                A : MyInt := 1;
                B : INT := 0;
            END_VAR
            B := ABS(A);
            END_PROGRAM
        "#;
    let output = parse_project("test.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
}

#[test]
fn flags_duplicate_variable() {
    let source = r#"
            PROGRAM Demo
            VAR
                A : INT;
                A : BOOL;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("test.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate variable 'A'")));
}

#[test]
fn checks_user_function_call_parameters() {
    let source = r#"
            FUNCTION Add : INT
            VAR_INPUT
                A : INT;
                B : INT;
            END_VAR
            Add := A + B;
            END_FUNCTION

            PROGRAM Demo
            VAR X : INT; END_VAR
            X := Add(A := 1, C := 2);
            END_PROGRAM
        "#;
    let output = parse_project("functions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("no input parameter 'C'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("missing input parameter 'B'")));
}

#[test]
fn rejects_value_returning_function_calls_as_statements() {
    let source = r#"
            FUNCTION Sum2 : INT
            VAR_INPUT
                A : INT;
                B : INT;
            END_VAR
            Sum2 := A + B;
            END_FUNCTION

            PROGRAM Demo
            Sum2(A := 1, B := 2);
            ADD(1, 2);
            ABS(1);
            END_PROGRAM
        "#;
    let output = parse_project("function_statement_calls.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'Sum2' returns a value and cannot be used as a statement")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'ABS' returns a value and cannot be used as a statement")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'ADD' returns a value and cannot be used as a statement")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown function block instance 'ABS'")));
}

#[test]
fn checks_user_call_parameter_range_and_length_constraints() {
    let source = r#"
            TYPE
                Small : INT(0..10);
            END_TYPE

            FUNCTION UseSmall : INT
            VAR_INPUT
                X : Small;
                Label : STRING[3];
            END_VAR
            UseSmall := X;
            END_FUNCTION

            FUNCTION_BLOCK Capture
            VAR_INPUT
                X : Small;
                Label : STRING[3];
            END_VAR
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Out : INT := 0;
                Fb : Capture;
            END_VAR

            Out := UseSmall(X := 11, Label := CONCAT('abc', 'd'));
            Out := UseSmall(12, 'abcde');
            Fb(X := 11, Label := CONCAT('abc', 'd'));
            Fb(12, 'abcde');
            END_PROGRAM
        "#;
    let output = parse_project("call_parameter_constraints.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'UseSmall' parameter 'X' value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'UseSmall' parameter 'X' value 12 is outside subrange 0..10")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "function 'UseSmall' parameter 'Label' exceeds string length 3 with 4 character(s)"
        )));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "function 'UseSmall' parameter 'Label' exceeds string length 3 with 5 character(s)"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function block 'Capture' parameter 'X' value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function block 'Capture' parameter 'X' value 12 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "function block 'Capture' parameter 'Label' exceeds string length 3 with 4 character(s)"
        )
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "function block 'Capture' parameter 'Label' exceeds string length 3 with 5 character(s)"
        )
    }));
}

#[test]
fn checks_function_en_eno_and_return_paths() {
    let source = r#"
            FUNCTION Maybe : INT
            VAR_INPUT
                A : INT;
            END_VAR
            IF A > 0 THEN
                Maybe := A;
            END_IF;
            END_FUNCTION

            PROGRAM Demo
            VAR
                X : INT := 0;
                Ok : BOOL := FALSE;
                BadEno : INT := 0;
            END_VAR
            X := Maybe(EN := TRUE, A := 1, ENO => Ok);
            X := Maybe(EN := 1, A := 1);
            X := Maybe(A := 1, ENO := TRUE);
            X := Maybe(A := 1, ENO => BadEno);
            END_PROGRAM
        "#;
    let output = parse_project("function_controls.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'Maybe' does not assign to its return variable on all paths")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function EN input expects BOOL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'Maybe' ENO must use output binding")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'Maybe' ENO expects BOOL output")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("has no input parameter 'EN'")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("has no input parameter 'ENO'")));
}

#[test]
fn rejects_recursive_function_cycles() {
    let source = r#"
            FUNCTION A : INT
            VAR_INPUT
                X : INT;
            END_VAR
            A := B(X := X);
            END_FUNCTION

            FUNCTION B : INT
            VAR_INPUT
                X : INT;
            END_VAR
            B := A(X := X);
            END_FUNCTION

            PROGRAM Demo
            VAR
                Out : INT := 0;
            END_VAR
            Out := A(X := 1);
            END_PROGRAM
        "#;
    let output = parse_project("recursive_functions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("recursive function call cycle involving 'A' is not supported")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("recursive function call cycle involving 'B' is not supported")));
}

#[test]
fn recognizes_communication_function_blocks_with_diagnostics() {
    let source = r#"
            PROGRAM Demo
            VAR
                Sender : USEND;
                Done : BOOL;
                Status : INT;
            END_VAR
            Sender(REQ := TRUE);
            Done := Sender.DONE;
            Status := Sender.STATUS;
            END_PROGRAM
        "#;
    let output = parse_project("communication_fb.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown type 'USEND'")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown function block instance")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown field")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("communication function block 'USEND' requires a target runtime hook")));
}

#[test]
fn flags_elementary_type_mismatches() {
    let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
                Enabled : BOOL;
            END_VAR
            Scale := Input;
            END_FUNCTION

            PROGRAM Demo
            VAR
                X : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            Flag := 1;
            IF X THEN
                X := 1;
            END_IF;
            X := Scale(Input := TRUE, Enabled := 1);
            END_PROGRAM
        "#;
    let output = parse_project("types.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'Flag' expects BOOL, got integer")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("IF condition expects BOOL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'Scale' parameter 'Input' expects integer, got BOOL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function 'Scale' parameter 'Enabled' expects BOOL, got integer")));
}

#[test]
fn flags_invalid_st_operator_operands() {
    let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 0;
                R : REAL := 0.0;
                Flag : BOOL := FALSE;
                Text : STRING[8] := 'x';
            END_VAR
            A := TRUE + 1;
            Flag := Text AND TRUE;
            A := Text MOD 2;
            R := 2 ** TRUE;
            Flag := A = Text;
            END_PROGRAM
        "#;
    let output = parse_project("operator_types.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("operator + cannot be applied to BOOL and integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("operator AND cannot be applied to STRING and BOOL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("operator MOD cannot be applied to STRING and integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("operator ** cannot be applied to integer and BOOL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("operator = cannot be applied to integer and STRING")));
}

#[test]
fn rejects_exit_outside_iteration() {
    let source = r#"
            PROGRAM Demo
            VAR
                I : INT := 0;
                Done : BOOL := FALSE;
            END_VAR
            IF Done THEN
                EXIT;
            END_IF;
            WHILE I < 2 DO
                I := I + 1;
                IF I = 1 THEN
                    EXIT;
                END_IF;
            END_WHILE;
            FOR I := 0 TO 2 BY 0 DO
                Done := TRUE;
            END_FOR;
            END_PROGRAM
        "#;
    let output = parse_project("exit_context.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("EXIT used outside of an iteration"))
            .count(),
        1
    );
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("FOR BY value cannot be zero")));
}

#[test]
fn checks_case_selector_and_constant_labels() {
    let source = r#"
            TYPE
                Mode : (Idle, Run, Fault);
                OtherMode : (Cold, Hot);
            END_TYPE

            PROGRAM Demo
            VAR
                I : INT := 0;
                Text : STRING[8] := 'x';
                State : Mode := Idle;
            END_VAR
            CASE Text OF
                'x': I := 1;
            END_CASE;
            CASE I OF
                1, 1: I := 2;
                2..4: I := 3;
                3: I := 4;
                7..5: I := 5;
            ELSE
                I := 6;
            END_CASE;
            CASE State OF
                Idle, Mode#Run, Idle: I := 7;
                Fault..Run: I := 8;
                OtherMode#Cold: I := 9;
                1: I := 10;
            END_CASE;
            END_PROGRAM
        "#;
    let output = parse_project("case_semantics.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("CASE selector expects integer or enumerated, got STRING")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("CASE label range 1 overlaps previous range 1")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("CASE label range 3 overlaps previous range 2..4")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("CASE range lower bound 7 exceeds upper bound 5")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("CASE label range 0 overlaps previous range 0")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("CASE enumerated selector does not support range labels")));
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("CASE label expects value of enum type 'Mode'"))
            .count(),
        2
    );
}

#[test]
fn checks_standard_function_generic_families() {
    let source = r#"
            TYPE
                Mode : (Idle, Run);
            END_TYPE

            PROGRAM Demo
            VAR
                A : INT := 0;
                Shifted : INT := 0;
                Text : STRING[8] := 'x';
                Delay : TIME := T#0ms;
                R : REAL := 0.0;
                Today : DATE := D#1970-01-01;
                Clock : TIME_OF_DAY := TOD#00:00:00;
                Stamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
                State : Mode := Idle;
                Other : Mode := Run;
                Selected : Mode := Idle;
                Same : BOOL := FALSE;
                Flag : BOOL := FALSE;
            END_VAR
            A := ADD(Text, 1);
            A := LEN(1);
            A := SEL(TRUE, 1, Text);
            Shifted := SHL(Text, 1);
            Text := CONCAT('a', 1);
            Delay := ADD_TIME(T#1s, 1);
            Delay := MIN(T#2s, T#1s);
            Delay := MIN(T#1s, 2);
            Delay := MIN(Today, Today);
            R := SQRT(4);
            R := EXPT(2, 3);
            A := TRUNC(1);
            A := LIMIT(0.0, 1, 2.0);
            Clock := ADD_TOD_TIME(TOD#00:00:01, T#2s);
            Stamp := ADD_DT_TIME(DT#1970-01-01-00:00:01, T#2s);
            Delay := SUB_DATE_DATE(Today, Today);
            Today := CONCAT_DATE(1970, 1, 1);
            Clock := CONCAT_TOD(0, 0, 1, 0);
            Stamp := CONCAT_DT(1970, 1, 1, 0, 0, 1, 0);
            Stamp := CONCAT_DATE_TOD(Today, Clock);
            A := DAY_OF_WEEK(Today);
            Delay := SUB_TOD_TOD(Clock, T#1s);
            Stamp := ADD_DT_TIME(Stamp, 1);
            A := DAY_OF_WEEK(1);
            Selected := SEL(TRUE, State, Other);
            Same := EQ(State, Other);
            A := ADD(State, 1);
            Shifted := SHL(State, 1);
            Same := EQ(State, Text);
            Shifted := SHL(1, -1);
            SPLIT_DATE(IN := 1, YEAR => A);
            SPLIT_DT(IN := Stamp, YEAR => Flag);
            A := ABS(1, 2);
            A := SUB(1, 2, 3);
            A := MUX(3, 10, 20);
            Same := NE(1, 2, 3);
            Text := REPLACE('abc', 'x', 1);
            END_PROGRAM
        "#;
    let output = parse_project("standard_generic_types.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'ADD' argument 1 expects ANY_NUM, got STRING")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'LEN' argument 1 expects ANY_STRING, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "standard function 'SEL' data arguments must have compatible types, got integer and STRING"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SHL' argument 1 expects ANY_BIT, got STRING")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'CONCAT' argument 2 expects ANY_STRING, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'ADD_TIME' argument 2 expects TIME, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "standard function 'MIN' data arguments must have compatible types, got TIME and integer"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'MIN' argument 1 expects ANY_MAGNITUDE, got DATE")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SQRT' argument 1 expects ANY_REAL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'EXPT' argument 1 expects ANY_REAL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'TRUNC' argument 1 expects ANY_REAL, got integer")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'A' expects integer, got REAL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SUB_TOD_TOD' argument 2 expects TIME_OF_DAY, got TIME")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'ADD_DT_TIME' argument 2 expects TIME, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'DAY_OF_WEEK' argument 1 expects DATE, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SPLIT_DATE' argument 1 expects DATE, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "standard function 'SPLIT_DT' output 'YEAR' expects INT-compatible variable, got BOOL",
        )
    }));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'ADD' argument 1 expects ANY_NUM, got enumerated")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SHL' argument 1 expects ANY_BIT, got enumerated")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "standard function 'EQ' data arguments must have compatible types, got enumerated and STRING"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SHL' argument 2 must be non-negative, got -1")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'ABS' expects exactly 1 input argument(s), got 2")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SUB' expects exactly 2 input argument(s), got 3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'MUX' selector must be in range 0..1, got 3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'NE' expects exactly 2 input argument(s), got 3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'REPLACE' expects exactly 4 input argument(s), got 3")));
}

#[test]
fn checks_standard_function_formal_input_names_and_duplicates() {
    let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 0;
                B : BOOL := FALSE;
            END_VAR
            A := LIMIT(IN := 5, MN := 0, MX := 10);
            A := SEL(IN1 := 2, G := FALSE, IN0 := 1);
            A := MUX(IN1 := 20, K := 1, IN0 := 10);
            A := SHL(N := 2, IN := 1);
            A := LIMIT(IN := 5, BAD := 0, MX := 10);
            A := LIMIT(0, 5, IN := 6);
            A := ADD(1, 2, OUT => A);
            B := SEL(IN1 := TRUE, G := FALSE, IN0 := FALSE);
            END_PROGRAM
        "#;
    let output = parse_project("standard_formals.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'LIMIT' has no input parameter 'BAD'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "standard function 'LIMIT' input parameter 'IN' duplicates 'positional argument 2'"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'ADD' has no output parameter 'OUT'")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SHL' argument 1 expects ANY_BIT")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("standard function 'SEL' data")));
}

#[test]
fn orders_standard_formals_before_inferring_return_type() {
    let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 5;
                B : BOOL := FALSE;
            END_VAR
            A := LIMIT(MX := 10.0, MN := 0.0, IN := A);
            B := EQ(IN2 := A, IN1 := LIMIT(MX := 10.0, MN := 0.0, IN := A));
            END_PROGRAM
        "#;
    let output = parse_project("standard_return_formals.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(
        diagnostics.is_empty(),
        "formal input ordering should drive return type inference: {diagnostics:?}"
    );
}

#[test]
fn generic_family_models_table_11_hierarchy() {
    assert!(GenericFamily::Any.contains(SimpleType::Aggregate));
    assert!(GenericFamily::AnyDerived.contains(SimpleType::Enum));
    assert!(GenericFamily::AnyDerived.contains(SimpleType::Aggregate));
    assert!(!GenericFamily::AnyDerived.contains(SimpleType::Integer));
    assert!(GenericFamily::AnyElementary.contains(SimpleType::Integer));
    assert!(GenericFamily::AnyElementary.contains(SimpleType::DateAndTime));
    assert!(!GenericFamily::AnyElementary.contains(SimpleType::Aggregate));
    assert!(GenericFamily::AnyMagnitude.contains(SimpleType::Integer));
    assert!(GenericFamily::AnyMagnitude.contains(SimpleType::Real));
    assert!(GenericFamily::AnyMagnitude.contains(SimpleType::Time));
    assert!(!GenericFamily::AnyMagnitude.contains(SimpleType::Date));
    assert!(GenericFamily::AnyDate.contains(SimpleType::Date));
    assert!(GenericFamily::AnyDate.contains(SimpleType::TimeOfDay));
    assert!(GenericFamily::AnyDate.contains(SimpleType::DateAndTime));
    assert!(!GenericFamily::AnyDate.contains(SimpleType::Time));
    assert!(!GenericFamily::AnyDate.contains(SimpleType::Integer));
    assert_eq!(GenericFamily::AnyDerived.as_str(), "ANY_DERIVED");
    assert_eq!(GenericFamily::AnyDate.as_str(), "ANY_DATE");
}

#[test]
fn checks_string_function_bounds() {
    let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[8] := '';
            END_VAR
            Text := LEFT('ABC', 4);
            Text := RIGHT('ABC', -1);
            Text := MID('ABC', 2, 3);
            Text := DELETE('ABC', 1, 0);
            Text := INSERT('ABC', 'X', 4);
            Text := REPLACE('ABC', 'X', 2, 3);
            END_PROGRAM
        "#;
    let output = parse_project("string_bounds.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'LEFT' length 4 exceeds string length 3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'RIGHT' argument 2 must be non-negative, got -1")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'MID' length 2 from position 3 exceeds string length 3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'DELETE' position 0 is outside string positions 1..3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'INSERT' insert position 4 is outside range 0..3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'REPLACE' length 2 from position 3 exceeds string length 3")));
}

#[test]
fn checks_bounded_string_constant_expression_lengths() {
    let source = r#"
            PROGRAM Demo
            VAR
                Short : STRING[3] := '';
                Wide : WSTRING[3] := "";
            END_VAR
            Short := CONCAT('ab', 'cd');
            Short := LEFT('abcd', 3);
            Wide := CONCAT("ab", "cd");
            END_PROGRAM
        "#;
    let output = parse_project("string_constant_expression_bounds.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'Short' exceeds string length 3 with 4 character(s)")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'Wide' exceeds string length 3 with 4 character(s)")));
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("LEFT")),
        "{diagnostics:?}"
    );
}

#[test]
fn validates_derived_type_initializers() {
    let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                ShortText : STRING[3];
            END_TYPE

            PROGRAM Demo
            VAR
                GoodSmall : Small := 5;
                BadSmall : Small := 11;
                GoodMode : Mode := Run;
                BadMode : Mode := 1;
                GoodText : ShortText := 'abc';
                BadText : ShortText := 'abcd';
                BadSmallFromFormalMux : Small := MUX(IN1 := 11, K := 1, IN0 := 0);
                BadTextFromFormalLeft : ShortText := LEFT(L := 4, IN := 'abcd');
                GoodTextFromFormalLeft : ShortText := LEFT(L := 3, IN := 'abcd');
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("derived_init.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadSmall' value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadMode' expects one of: Idle, Run, Fault")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadText' exceeds string length 3")));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "initial value for variable 'BadSmallFromFormalMux' value 11 is outside subrange 0..10",
        )
    }));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadTextFromFormalLeft' exceeds string length 3")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown variable 'Run'")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodTextFromFormalLeft")));
}

#[test]
fn validates_nested_derived_alias_initializers_and_access() {
    let source = r#"
            TYPE
                Small : INT(0..10);
                SmallAlias : Small;
                SmallAlias2 : SmallAlias;
                Text3 : STRING[3];
                TextAlias : Text3;
                Row : ARRAY [1..2] OF SmallAlias2;
                RowAlias : Row;
                Holder : STRUCT
                    Values : RowAlias;
                    Label : TextAlias;
                END_STRUCT;
                HolderAlias : Holder;
                Mode : (Idle, Run);
                ModeAlias : Mode;
                ModeAlias2 : ModeAlias;
            END_TYPE

            PROGRAM Demo
            VAR
                GoodHolder : HolderAlias := (Values := [1, 2], Label := 'abc');
                BadSubrange : SmallAlias2 := 11;
                BadText : TextAlias := 'abcd';
                GoodMode : ModeAlias2 := Mode#Run;
                BadMode : ModeAlias2 := 1;
                BadFromFormalMux : SmallAlias2 := MUX(K := 1, IN0 := 0, IN1 := 11);
            END_VAR
            GoodHolder.Values[1] := GoodHolder.Values[2];
            END_PROGRAM
        "#;
    let output = parse_project("nested_derived_aliases.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadSubrange' value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadText' exceeds string length 3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadMode' expects one of: Idle, Run")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadFromFormalMux' value 11 is outside subrange 0..10"
        )));
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodHolder.Values")),
        "{diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodMode")),
        "{diagnostics:?}"
    );
}

#[test]
fn validates_subrange_base_type_and_bounds() {
    let source = r#"
            TYPE
                GoodSint : SINT(-128..127);
                GoodUint : UINT(0..65535);
                BadOrder : INT(10..0);
                BadSint : SINT(-129..127);
                BadUsint : USINT(-1..256);
                BadReal : REAL(0..10);
                BadByte : BYTE(0..10);
            END_TYPE

            PROGRAM Demo
            END_PROGRAM
        "#;
    let output = parse_project("subrange_bounds.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("invalid subrange 10..0")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("subrange -129..127 is outside SINT range -128..127")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("subrange -1..256 is outside USINT range 0..255")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("subrange base type 'REAL' must be an integer type")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("subrange base type 'BYTE' must be an integer type")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodSint")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodUint")));
}

#[test]
fn diagnoses_enum_duplicate_values_and_cross_type_ambiguity() {
    let source = r#"
            TYPE
                ModeA : (Idle, Run);
                ModeB : (Run, Fault);
                BadEnum : (Repeat, Repeat);
            END_TYPE

            PROGRAM Demo
            VAR
                StateA : ModeA := ModeA#Idle;
                StateB : ModeB := ModeB#Fault;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("enum_ambiguity.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("duplicate enumerated value 'Repeat' in enum type 'BadEnum'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("enumerated value 'RUN' is declared by multiple enum types: ModeA, ModeB")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("StateA")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("StateB")));
}

#[test]
fn rejects_typed_enum_literals_from_incompatible_enum_types() {
    let source = r#"
            TYPE
                ModeA : (Idle, Run);
                AliasA : ModeA;
                ModeB : (Run, Fault);
            END_TYPE

            PROGRAM Demo
            VAR
                GoodA : ModeA := ModeA#Run;
                GoodAliasA : AliasA := AliasA#Idle;
                GoodAliasBase : AliasA := ModeA#Idle;
                BadA : ModeA := ModeB#Run;
                BadAliasA : AliasA := ModeB#Run;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("enum_typed_type_mismatch.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadA' expects enum type 'ModeA', got typed enum literal 'ModeB#Run'"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadAliasA' expects enum type 'ModeA', got typed enum literal 'ModeB#Run'"
        )));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodA")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodAliasA")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodAliasBase")));
}

#[test]
fn rejects_zero_length_string_types() {
    let source = r#"
            PROGRAM Demo
            VAR
                EmptyText : STRING[0];
                EmptyWide : WSTRING[0];
                TooLong : STRING[5];
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("zero_strings.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(
        &output.project,
        &CheckOptions {
            implementation: ImplementationParameters {
                max_string_length: 4,
                ..ImplementationParameters::default()
            },
            ..CheckOptions::default()
        },
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic
                .message
                .contains("string length must be at least 1"))
            .count(),
        2
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("string length exceeds maximum 4")));
}

#[test]
fn validates_array_and_structure_initializers() {
    let source = r#"
            TYPE
                Small : INT(0..10);
                Pair : STRUCT
                    Low : Small;
                    Flag : BOOL;
                END_STRUCT;
                OtherPair : STRUCT
                    Low : Small;
                    Flag : INT;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                GoodArray : ARRAY [1..3] OF Small := [1, 2, 3];
                GoodArrayCopy : ARRAY [1..3] OF Small := [0, 0, 0];
                GoodRepeat : ARRAY [1..5] OF Small := [2(1), 3(2)];
                BadArrayLength : ARRAY [1..3] OF Small := [1, 2];
                BadArrayElement : ARRAY [1..2] OF Small := [1, 11];
                BadRepeatedElement : ARRAY [1..3] OF Small := [2(11), 0];
                BadArrayCopy : ARRAY [1..2] OF Small := [0, 0];
                GoodPair : Pair := (Low := 5, Flag := TRUE);
                GoodPairCopy : Pair := (Low := 0, Flag := FALSE);
                BadPairCopy : OtherPair := (Low := 0, Flag := 0);
                UnknownField : Pair := (Low := 5, Missing := TRUE);
                DuplicateField : Pair := (Low := 5, Low := 6, Flag := TRUE);
                BadFieldType : Pair := (Low := TRUE, Flag := 1);
            END_VAR
            GoodArrayCopy := GoodArray;
            BadArrayCopy := GoodArray;
            GoodPairCopy := GoodPair;
            BadPairCopy := GoodPair;
            END_PROGRAM
        "#;
    let output = parse_project("aggregates.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadArrayLength' expects 3 array element(s), got 2"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadArrayElement' element 2 value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadRepeatedElement' element 1 value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'BadArrayCopy' expects a compatible array value")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("assignment to 'BadPairCopy' expects a compatible structure value")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'UnknownField' has unknown structure field 'Missing'"
        )));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'DuplicateField' initializes field 'Low' more than once"
        )));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadFieldType' field 'Low' expects integer, got BOOL"
        )));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadFieldType' field 'Flag' expects BOOL, got integer"
        )));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodArrayCopy")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodRepeat")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodPairCopy")));
}

#[test]
fn checks_array_index_arity_and_constant_bounds() {
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
                Values : ARRAY [1..3, 0..1] OF INT;
                Pair : STRUCT
                    Nested : ARRAY [2..4] OF INT;
                END_STRUCT;
                Box : Holder;
                RowCopy : Row;
                Ok : INT := 0;
                BadLow : INT := 0;
                BadHigh : INT := 0;
                BadArity : INT := 0;
                BadNested : INT := 0;
                GoodNested : INT := 0;
                BadNestedOuter : INT := 0;
                BadNestedInner : INT := 0;
            END_VAR

            Ok := Values[1, 0];
            BadLow := Values[0, 0];
            BadHigh := Values[1, 2];
            BadArity := Values[1];
            BadNested := Pair.Nested[5];
            GoodNested := Box.Rows[1][2];
            RowCopy := Box.Rows[2];
            BadNestedOuter := Box.Rows[0][2];
            BadNestedInner := Box.Rows[1][5];
            END_PROGRAM
        "#;
    let output = parse_project("array_indices.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array index 0 in 'Values[0, 0]' is outside range 1..3")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array index 2 in 'Values[1, 2]' is outside range 0..1")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array access 'Values[1]' expects 2 index(es), got 1")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array index 5 in 'Pair.Nested[5]' is outside range 2..4")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array index 0 in 'Box.Rows[0, 2]' is outside range 1..2")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("array index 5 in 'Box.Rows[1, 5]' is outside range 2..4")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Ok")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodNested")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("RowCopy")));
}

#[test]
fn rejects_writes_to_constant_variables() {
    let source = r#"
            PROGRAM Demo
            VAR CONSTANT
                Limit : INT := 5;
            END_VAR
            VAR
                Count : INT := 0;
            END_VAR

            Count := Limit;
            Limit := 6;
            END_PROGRAM
        "#;
    let output = parse_project("constant.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("cannot assign to CONSTANT variable 'Limit'")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("assignment to 'Count'")));
}

#[test]
fn validates_retain_qualifiers() {
    let source = r#"
            FUNCTION BadFunction : INT
            VAR RETAIN
                Saved : INT := 0;
            END_VAR
            BadFunction := Saved;
            END_FUNCTION

            PROGRAM Demo
            VAR RETAIN
                Kept : INT := 1;
            END_VAR
            VAR NON_RETAIN
                Reset : INT := 1;
            END_VAR
            VAR_TEMP RETAIN
                TempSaved : INT;
            END_VAR
            VAR CONSTANT RETAIN
                BadConstant : INT := 1;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("retain.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("FUNCTION 'BadFunction' cannot declare RETAIN variables")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_TEMP cannot be declared RETAIN")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR CONSTANT cannot also be declared RETAIN")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Kept")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Reset")));
}

#[test]
fn enforces_expression_and_statement_depth_limits() {
    let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 0;
            END_VAR

            A := 1 + (2 * (3 + 4));
            IF TRUE THEN
                IF TRUE THEN
                    A := 1;
                END_IF;
            END_IF;
            END_PROGRAM
        "#;
    let output = parse_project("depth_limits.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(
        &output.project,
        &CheckOptions {
            implementation: ImplementationParameters {
                max_expression_depth: 2,
                max_statement_depth: 1,
                ..ImplementationParameters::default()
            },
            ..CheckOptions::default()
        },
    );
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("assignment expression depth")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("statement nesting depth 2")));
}

#[test]
fn validates_direct_variable_locations() {
    let good = r#"
            PROGRAM GoodIo
            VAR
                Sensor AT %IX0.0 : BOOL;
                OutputWord AT %QW2 : INT;
                MemoryDint AT %MD10 : DINT;
                IncompleteInput AT %IX* : BOOL;
                IncompleteOutput AT %QW* : INT;
            END_VAR
            Sensor := %IX0.1;
            END_PROGRAM
        "#;
    let output = parse_project("good_io.st", good);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let bad = r#"
            PROGRAM BadIo
            VAR
                BadArea AT %ZX0.0 : BOOL;
                MissingAddress AT %IX : BOOL;
                BadAddress AT %QW1-A : INT;
                BadWildcard AT %MX1.* : BOOL;
                NotDirect AT Symbolic : INT;
            END_VAR
            %Q.1 := TRUE;
            %IX* := TRUE;
            END_PROGRAM
        "#;
    let output = parse_project("bad_io.st", bad);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("invalid area 'Z'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("'%IX' is missing an address")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("invalid address '1-A'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("invalid address '1.*'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("must start with '%'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("malformed address '.1'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("incomplete direct variable location '%IX*' is only valid in a declaration")));
}

#[test]
fn annex_e_style_negative_cases_emit_stable_diagnostics() {
    let cases = [
        (
            "duplicate-variable",
            r#"
                PROGRAM BadDuplicate
                VAR
                    A : INT;
                    A : BOOL;
                END_VAR
                END_PROGRAM
                "#,
            "duplicate variable 'A'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "unknown-variable",
            r#"
                PROGRAM BadUnknown
                VAR A : INT; END_VAR
                B := A + 1;
                END_PROGRAM
                "#,
            "unknown variable 'B'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "type-mismatch",
            r#"
                PROGRAM BadTypes
                VAR Flag : BOOL; END_VAR
                Flag := 1;
                END_PROGRAM
                "#,
            "assignment to 'Flag' expects BOOL, got integer",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "bad-direct-variable",
            r#"
                PROGRAM BadDirectVariable
                VAR Broken AT %ZX0.0 : BOOL; END_VAR
                END_PROGRAM
                "#,
            "invalid area 'Z'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "strict-identifier",
            r#"
                PROGRAM BadIdentifier
                VAR Bad__Name : INT; END_VAR
                END_PROGRAM
                "#,
            "violates 2003-strict identifier underscore rules",
            "\"stableCode\":\"RBCPP-COMPLIANCE\"",
        ),
        (
            "bad-configuration-reference",
            r#"
                CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    PROGRAM Main WITH MissingTask : MissingProgram;
                END_RESOURCE
                END_CONFIGURATION
                "#,
            "unknown PROGRAM type 'MissingProgram'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "bad-sfc-transition",
            r#"
                PROGRAM BadSequence
                VAR Ready : INT := 1; END_VAR
                INITIAL_STEP Start;
                STEP Start;
                TRANSITION Go := Ready;
                END_PROGRAM
                "#,
            "SFC transition condition expects BOOL, got integer",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "write-to-constant",
            r#"
                PROGRAM BadConstantWrite
                VAR CONSTANT
                    Limit : INT := 5;
                END_VAR
                Limit := 6;
                END_PROGRAM
                "#,
            "cannot assign to CONSTANT variable 'Limit'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "exit-outside-iteration",
            r#"
                PROGRAM BadExit
                VAR Done : BOOL := FALSE; END_VAR
                IF Done THEN
                    EXIT;
                END_IF;
                END_PROGRAM
                "#,
            "EXIT used outside of an iteration",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "case-label-overlap",
            r#"
                PROGRAM BadCase
                VAR Selected : INT := 0; END_VAR
                CASE Selected OF
                    1, 1: Selected := 2;
                ELSE
                    Selected := 3;
                END_CASE;
                END_PROGRAM
                "#,
            "CASE label range 1 overlaps previous range 1",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "subrange-out-of-base-range",
            r#"
                TYPE
                    BadSmall : SINT(-129..127);
                END_TYPE
                PROGRAM BadSubrange
                VAR Value : BadSmall := 0; END_VAR
                END_PROGRAM
                "#,
            "subrange -129..127 is outside SINT range -128..127",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "unknown-il-label",
            r#"
                PROGRAM BadIl
                VAR A : INT := 0; END_VAR
                JMP Missing;
                END_PROGRAM
                "#,
            "unknown IL label 'Missing'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "missing-function-return",
            r#"
                FUNCTION Maybe : INT
                VAR_INPUT
                    Flag : BOOL;
                END_VAR
                IF Flag THEN
                    Maybe := 1;
                END_IF;
                END_FUNCTION
                "#,
            "function 'Maybe' does not assign to its return variable on all paths",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "duplicate-enumerated-value",
            r#"
                TYPE
                    BadEnum : (Repeat, Repeat);
                END_TYPE
                PROGRAM BadEnumProgram
                VAR State : BadEnum := Repeat; END_VAR
                END_PROGRAM
                "#,
            "duplicate enumerated value 'Repeat' in enum type 'BadEnum'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "array-index-bounds",
            r#"
                PROGRAM BadArray
                VAR
                    Values : ARRAY [1..3, 0..1] OF INT;
                    Out : INT := 0;
                END_VAR
                Out := Values[0, 0];
                END_PROGRAM
                "#,
            "array index 0 in 'Values[0, 0]' is outside range 1..3",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "bad-access-path-target",
            r#"
                PROGRAM BadAccess
                VAR
                    Local : INT := 1;
                END_VAR
                VAR_TEMP
                    Scratch : INT;
                END_VAR
                VAR_ACCESS
                    BadType : Local : BOOL READ_ONLY;
                    BadTemp : Scratch : INT READ_ONLY;
                END_VAR
                END_PROGRAM
                "#,
            "access path 'BadType' type does not match target 'Local'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "standard-function-arity",
            r#"
                PROGRAM BadArity
                VAR A : INT := 0; END_VAR
                A := ABS(1, 2);
                END_PROGRAM
                "#,
            "standard function 'ABS' expects exactly 1 input argument(s), got 2",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "conversion-range",
            r#"
                PROGRAM BadConversion
                VAR A : USINT := 0; END_VAR
                A := INT_TO_USINT(300);
                END_PROGRAM
                "#,
            "conversion 'INT_TO_USINT' value 300 is outside target range 0..255",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "bad-retain-qualifier",
            r#"
                FUNCTION BadFunction : INT
                VAR RETAIN
                    Saved : INT := 0;
                END_VAR
                BadFunction := Saved;
                END_FUNCTION
                "#,
            "FUNCTION 'BadFunction' cannot declare RETAIN variables",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "duplicate-il-label",
            r#"
                PROGRAM BadIlLabel
                VAR A : INT := 0; END_VAR
                Start:
                Start:
                LD A;
                END_PROGRAM
                "#,
            "duplicate IL label 'Start'",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "recursive-function-cycle",
            r#"
                FUNCTION A : INT
                A := B();
                END_FUNCTION
                FUNCTION B : INT
                B := A();
                END_FUNCTION
                "#,
            "recursive function call cycle involving 'A' is not supported",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
        (
            "non-variable-var-in-out-actual",
            r#"
                FUNCTION_BLOCK Mutate
                VAR_IN_OUT
                    X : INT;
                END_VAR
                X := X + 1;
                END_FUNCTION_BLOCK

                PROGRAM BadInOut
                VAR Fb : Mutate; END_VAR
                Fb(X := 1);
                END_PROGRAM
                "#,
            "function block 'Mutate' VAR_IN_OUT parameter 'X' requires a variable actual",
            "\"stableCode\":\"RBCPP-SEMANTIC\"",
        ),
    ];

    for (name, source, expected_message, expected_stable_code) in cases {
        let output = parse_project(format!("annex_e_{name}.st"), source);
        assert!(
            output.diagnostics.is_empty(),
            "{name}: parse diagnostics: {:?}",
            output.diagnostics
        );
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected_message)),
            "{name}: expected '{expected_message}', got {diagnostics:?}"
        );
        let json = diagnostics_to_json(&diagnostics);
        assert!(
            json.contains("\"stableCode\""),
            "{name}: expected stableCode in {json}"
        );
        assert!(
            json.contains(expected_stable_code),
            "{name}: expected {expected_stable_code} in {json}"
        );
    }
}

#[test]
fn enforces_strict_profile_identifier_underscore_rules() {
    let source = r#"
            PROGRAM Demo
            VAR
                Bad__Name : INT := 0;
                BadTrailing_ : INT := 0;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("profile_identifiers.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let strict = check_project(&output.project, &CheckOptions::default());
    assert!(strict.iter().any(|diagnostic| diagnostic
        .message
        .contains("Bad__Name' violates 2003-strict identifier underscore rules")));
    assert!(strict.iter().any(|diagnostic| diagnostic
        .message
        .contains("BadTrailing_' violates 2003-strict identifier underscore rules")));

    let plus = check_project(
        &output.project,
        &CheckOptions {
            profile: EditionProfile::Iec61131_3_2003PlusExtensions,
            ..CheckOptions::default()
        },
    );
    assert!(!plus
        .iter()
        .any(|diagnostic| diagnostic.message.contains("underscore rules")));
}

#[test]
fn later_edition_profiles_are_non_claimable() {
    let source = "PROGRAM Demo END_PROGRAM";
    let output = parse_project("placeholder_profile.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(
        &output.project,
        &CheckOptions {
            profile: EditionProfile::Iec61131_3_2025Placeholder,
            ..CheckOptions::default()
        },
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("profile '2025-placeholder' is a placeholder")));
}

#[test]
fn handles_overflowing_constant_expressions_without_panic() {
    let source = r#"
            TYPE Small : INT(0..10); END_TYPE

            PROGRAM Demo
            VAR
                Value : Small := 9223372036854775807 + 1;
                Huge : LINT := LINT#9223372036854775807 + DINT#1;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("constant_overflow.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'Value' value 9223372036854775808 is outside subrange 0..10")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'Huge' value 9223372036854775808 is outside LINT range"
        )));
}

#[test]
fn flags_constant_conversion_target_range_errors() {
    let source = r#"
            PROGRAM Demo
            VAR
                Bad : INT := 0;
                BadByte : BYTE := 300;
                BadSint : SINT := -129;
                BadReal : REAL := REAL#1e39;
                BadLreal : LREAL := LREAL#1e5000;
            END_VAR
            Bad := INT_TO_USINT(300);
            Bad := WORD_BCD_TO_UINT(WORD#16#1A);
            Bad := INT_TO_BCD_BYTE(123);
            Bad := REAL_TO_INT(1);
            Bad := BOOL_TO_INT(1);
            Bad := STRING_TO_INT("wide");
            Bad := WSTRING_TO_INT('narrow');
            Bad := WORD_TO_UINT(1);
            END_PROGRAM
        "#;
    let output = parse_project("conversion_range.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("conversion 'INT_TO_USINT' value 300 is outside target range 0..255")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("conversion 'WORD_BCD_TO_UINT' value 26 is not valid BCD")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("conversion 'INT_TO_BCD_BYTE' value 123 cannot be represented as BCD")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'REAL_TO_INT' argument 1 expects ANY_REAL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'BOOL_TO_INT' argument 1 expects BOOL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'STRING_TO_INT' argument 1 expects STRING, got WSTRING")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'WSTRING_TO_INT' argument 1 expects WSTRING, got STRING")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'WORD_TO_UINT' argument 1 expects bit-string, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadByte' value 300 is outside BYTE range 0..255")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadSint' value -129 is outside SINT range -128..127"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal value 1e39 is outside REAL range")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal 'LREAL#1e5000' is not a valid LREAL value")));
}

#[test]
fn flags_invalid_constant_conversion_inputs() {
    let source = r#"
            PROGRAM Demo
            VAR
                BadInt : INT := 0;
                BadBool : BOOL := FALSE;
                BadReal : REAL := 0.0;
                BadTime : TIME := T#0s;
                BadDate : DATE := D#1970-01-01;
                BadTod : TIME_OF_DAY := TOD#00:00:00;
                BadDt : DATE_AND_TIME := DT#1970-01-01-00:00:00;
            END_VAR

            BadInt := STRING_TO_INT('not-an-int');
            BadBool := STRING_TO_BOOL('maybe');
            BadReal := STRING_TO_REAL('NaN');
            BadTime := STRING_TO_TIME('no-time');
            BadDate := STRING_TO_DATE('2024-02-30');
            BadTod := STRING_TO_TOD('25:00:00');
            BadDt := STRING_TO_DATE_AND_TIME('2024-02-30-00:00:00');
            END_PROGRAM
        "#;
    let output = parse_project("invalid_constant_conversions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    for expected in [
        "conversion 'STRING_TO_INT' cannot convert constant input to INT",
        "conversion 'STRING_TO_BOOL' cannot convert constant input to BOOL",
        "conversion 'STRING_TO_REAL' produced non-finite REAL from constant input",
        "conversion 'STRING_TO_TIME' cannot convert constant input to TIME",
        "conversion 'STRING_TO_DATE' cannot convert constant input to DATE",
        "conversion 'STRING_TO_TOD' cannot convert constant input to TOD",
        "conversion 'STRING_TO_DATE_AND_TIME' cannot convert constant input to DATE_AND_TIME",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing {expected}; diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn checks_date_time_conversion_function_families() {
    let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[40] := '';
                Wide : WSTRING[40] := "";
                Today : DATE := D#1970-01-01;
                Clock : TIME_OF_DAY := TOD#00:00:00;
                Stamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
            END_VAR

            Today := STRING_TO_DATE('D#1970-01-02');
            Clock := STRING_TO_TOD('TOD#01:02:03.004');
            Stamp := STRING_TO_DT('DT#1970-01-02-01:02:03.004');
            Text := DATE_TO_STRING(Today);
            Wide := TOD_TO_WSTRING(Clock);
            Text := DATE_AND_TIME_TO_STRING(Stamp);
            Text := DATE_TO_STRING(Clock);
            Today := STRING_TO_DATE(T#1s);
            END_PROGRAM
        "#;
    let output = parse_project("date_time_conversions.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'DATE_TO_STRING' argument 1 expects DATE, got TIME_OF_DAY")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'STRING_TO_DATE' argument 1 expects STRING, got TIME")));
}

#[test]
fn checks_typed_alias_literal_families_in_standard_calls() {
    let source = r#"
            TYPE
                MyInt : INT;
                MyInt2 : MyInt;
                MyReal : REAL;
                MyReal2 : MyReal;
                MyTod : TIME_OF_DAY;
                MyTod2 : MyTod;
            END_TYPE

            PROGRAM Demo
            VAR
                RealOut : REAL := 0.0;
                Text : STRING[32] := '';
            END_VAR

            RealOut := SIN(MyReal2#1.5);
            RealOut := SIN(MyInt2#1);
            Text := DATE_TO_STRING(MyTod2#01:02:03.004);
            END_PROGRAM
        "#;
    let output = parse_project("typed_alias_literal_families.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'SIN' argument 1 expects ANY_REAL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function 'DATE_TO_STRING' argument 1 expects DATE, got TIME_OF_DAY")));
}

#[test]
fn validates_typed_literal_ranges_and_enum_values() {
    let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                AliasInt : INT;
                AliasWord : WORD;
                AliasTime : TIME;
                AliasDate : DATE;
                AliasTod : TIME_OF_DAY;
                AliasDt : DATE_AND_TIME;
                AliasBool : BOOL;
                AliasAliasDate : AliasDate;
            END_TYPE

            PROGRAM Demo
            VAR
                BadSmall : Small := Small#11;
                BadMode : Mode := Mode#Missing;
                GoodMode : Mode := Mode#Run;
                BadByte : BYTE := BYTE#16#100;
                BadAliasInt : AliasInt := AliasInt#40000;
                BadAliasWord : AliasWord := AliasWord#16#1_0000;
                BadAliasTime : AliasTime := AliasTime#1m_75s;
                BadAliasDate : AliasDate := AliasDate#2023-02-29;
                BadAliasTod : AliasTod := AliasTod#24:00:00;
                BadAliasDt : AliasDt := AliasDt#2024-02-29-25:00:00;
                BadAliasBool : AliasBool := AliasBool#maybe;
                BadNestedAliasDate : AliasAliasDate := AliasAliasDate#2023-02-29;
                BadUnknownType : INT := MissingType#1;
                GoodAliasTime : AliasTime := AliasTime#1m_30s;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("typed_literals.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed enum literal 'Mode#Missing' is not a value of 'Mode'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value 256 is outside BYTE range 0..255")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value 40000 is outside INT range -32768..32767")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("value 65536 is outside WORD range 0..65535")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal 'AliasTime#1m_75s' is not a valid TIME value")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal 'AliasDate#2023-02-29' is not a valid DATE value")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal 'AliasTod#24:00:00' is not a valid TIME_OF_DAY value")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "typed literal 'AliasDt#2024-02-29-25:00:00' is not a valid DATE_AND_TIME value"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal 'AliasBool#maybe' is not a valid BOOL value")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("typed literal 'AliasAliasDate#2023-02-29' is not a valid DATE value")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown typed literal type 'MissingType'")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodAliasTime")));
}

#[test]
fn distinguishes_string_and_wstring_assignments() {
    let source = r#"
            PROGRAM TextTypes
            VAR
                Narrow : STRING[8] := "wide";
                Wide : WSTRING[8] := 'narrow';
                GoodWide : WSTRING[8] := "ok";
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("text_types.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'Narrow' expects STRING, got WSTRING")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'Wide' expects WSTRING, got STRING")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'GoodWide'")));
}

#[test]
fn validates_textual_sfc_elements() {
    let valid = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR
            INITIAL_STEP Start:
                MarkDone(N);
            END_STEP;
            STEP Run;
            Go: TRANSITION FROM Start TO Run := Ready;
            END_TRANSITION;
            MarkDone: ACTION
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("valid_sfc.st", valid);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);

    let invalid = r#"
            PROGRAM BadSequence
            VAR
                Ready : INT := 1;
                Done : BOOL := FALSE;
            END_VAR
            INITIAL_STEP Start;
            STEP Start;
            STEP Other:
                Unknown(D);
                Delay(L, T#1ms);
                Delay(D, T#2ms);
            END_STEP;
            TRANSITION Go FROM Missing TO Done := Ready;
            END_TRANSITION;
            ACTION MarkDone:
                Done := TRUE;
            END_ACTION;
            ACTION MarkDone:
                Done := FALSE;
            END_ACTION;
            ACTION Delay(D):
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
    let output = parse_project("invalid_sfc.st", invalid);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate SFC step 'Start'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("SFC transition condition expects BOOL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("SFC transition references unknown FROM step 'Missing'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("SFC transition references unknown TO step 'Done'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("duplicate SFC action 'MarkDone'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("SFC action 'Delay' qualifier D requires a duration")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("SFC step 'Other' references unknown action 'Unknown'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "SFC step 'Other' action association 'Unknown' qualifier D requires a duration"
        )));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "SFC step 'Other' has more than one time-related association for action 'Delay'"
        )));
}

#[test]
fn resolves_global_variables_across_pous() {
    let source = r#"
            PROGRAM Globals
            VAR_GLOBAL
                Shared : INT := 1;
            END_VAR
            END_PROGRAM

            PROGRAM Main
            VAR
                Local : INT := 0;
            END_VAR
            Local := Shared + 1;
            Local := ConfigShared + ResourceShared;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                ConfigShared : INT := 2;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_GLOBAL
                    ResourceShared : INT := 3;
                END_VAR
                VAR_CONFIG
                    Tunable AT %MW10 : INT := 4;
                END_VAR
                VAR_ACCESS
                    AccessPoint AT %MX0.0 : BOOL;
                END_VAR
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("globals.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown variable 'Shared'")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown variable 'ConfigShared'")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown variable 'ResourceShared'")));
}

#[test]
fn validates_var_external_against_global_declarations() {
    let source = r#"
            PROGRAM Globals
            VAR_GLOBAL
                Shared : INT := 1;
                Flag : BOOL := TRUE;
            END_VAR
            VAR_GLOBAL CONSTANT
                ConstShared : INT := 2;
            END_VAR
            END_PROGRAM

            PROGRAM Main
            VAR_EXTERNAL
                Shared : INT;
                Flag : INT;
                Missing : INT;
                ConstShared : INT;
            END_VAR
            VAR_EXTERNAL CONSTANT
                Shared : INT;
            END_VAR
            VAR
                Local : INT := 0;
            END_VAR
            Local := Shared + Missing;
            END_PROGRAM
        "#;
    let output = parse_project("var_external_semantics.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_EXTERNAL variable 'Flag' type does not match")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_EXTERNAL variable 'Missing' has no matching VAR_GLOBAL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_EXTERNAL variable 'ConstShared' must be declared CONSTANT")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("VAR_EXTERNAL variable 'Shared' cannot be declared CONSTANT")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate variable 'Shared'")));
}

#[test]
fn validates_function_block_input_edge_qualifiers() {
    let source = r#"
            FUNCTION_BLOCK EdgeOk
            VAR_INPUT
                Start : BOOL R_EDGE;
                Stop : BOOL F_EDGE;
            END_VAR
            END_FUNCTION_BLOCK

            FUNCTION BadFunction : BOOL
            VAR_INPUT
                Start : BOOL R_EDGE;
            END_VAR
            BadFunction := Start;
            END_FUNCTION

            FUNCTION_BLOCK BadType
            VAR_INPUT
                Count : INT R_EDGE;
            END_VAR
            END_FUNCTION_BLOCK
        "#;
    let output = parse_project("edge_qualifiers_semantics.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains(
            "R_EDGE edge qualifier on variable 'Start' is only valid on FUNCTION_BLOCK VAR_INPUT",
        )
    }));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("R_EDGE edge qualifier on variable 'Count' requires BOOL")));
    assert!(!diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("edge qualifier on variable 'Stop'")));
}

#[test]
fn checks_program_access_paths() {
    let source = r#"
            TYPE
                Pair : STRUCT
                    Low : INT;
                    Flag : BOOL;
                END_STRUCT;
            END_TYPE

            PROGRAM AccessDemo
            VAR
                Local : INT := 1;
                Counter : CTU;
                PairValue : Pair;
            END_VAR
            VAR_TEMP
                Scratch : INT;
            END_VAR
            VAR_ACCESS
                GoodLocal : Local : INT READ_WRITE;
                GoodFbField : Counter.CV : INT READ_ONLY;
                GoodStructField : PairValue.Flag : BOOL READ_ONLY;
                GoodDirect : %IX1.1 : BOOL READ_ONLY;
                BadType : Local : BOOL READ_ONLY;
                BadNested : PairValue.Missing : INT READ_ONLY;
                BadTemp : Scratch : INT READ_ONLY;
            END_VAR
            END_PROGRAM
        "#;
    let output = parse_project("access_paths.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("access path 'BadType' type does not match target 'Local'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("access path 'BadNested' references unknown target 'PairValue.Missing'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("access path 'BadTemp' cannot target VAR_TEMP")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodLocal")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodDirect")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodFbField")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodStructField")));
}

#[test]
fn checks_function_block_positional_and_named_duplicate_inputs() {
    let source = r#"
            FUNCTION_BLOCK Capture
            VAR_INPUT
                X : INT;
                Y : INT;
            END_VAR
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Capture;
            END_VAR

            Fb(1, X := 2);
            Fb(1, 2, Y := 3);
            END_PROGRAM
        "#;
    let output = parse_project("fb_duplicate_inputs.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function block 'Capture' input parameter 'X' is bound more than once")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function block 'Capture' input parameter 'Y' is bound more than once")));
}

#[test]
fn checks_function_block_en_eno_controls() {
    let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                BadEno : INT := 0;
                GoodEno : BOOL := FALSE;
            END_VAR

            Counter(EN := 1, CU := TRUE, R := FALSE, PV := 1);
            Counter(EN := TRUE, CU := TRUE, R := FALSE, PV := 1, ENO => BadEno);
            Counter(EN := TRUE, CU := TRUE, R := FALSE, PV := 1, ENO => GoodEno);
            END_PROGRAM
        "#;
    let output = parse_project("fb_controls_semantics.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function block EN input expects BOOL")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("function block 'Counter' ENO expects BOOL output")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodEno")));
}

#[test]
fn checks_standard_function_block_parameter_bindings() {
    let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Flag : BOOL := FALSE;
                Count : INT := 0;
                BadCount : BOOL := FALSE;
            END_VAR

            Counter(CU := TRUE, R := FALSE, PV := 1, Q => Flag, CV => Count);
            Counter(CU := TRUE, R := FALSE, PV := 1, Missing => Flag);
            Counter(CU := TRUE, R := FALSE, PV := 1, CV => BadCount);
            Counter(CU := TRUE, R := FALSE, PV := 1, NOT CV => Count);
            Counter(CU := TRUE, R := FALSE, PV := 1, Q => Flag, Q => Flag);
            Counter(CU := TRUE, BadInput := FALSE, PV := 1);
            END_PROGRAM
        "#;
    let output = parse_project("standard_fb_bindings_semantics.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function block 'CTU' has no output parameter 'Missing'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "standard function block 'CTU' output parameter 'CV' expects BOOL, got integer"
        )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function block 'CTU' output parameter 'CV' cannot be negated")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function block 'CTU' output parameter 'Q' is bound more than once")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("standard function block 'CTU' has no input parameter 'BadInput'")));
}

#[test]
fn checks_instruction_list_labels() {
    let source = r#"
            PROGRAM BadIl
            VAR
                A : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            JMP Missing;
            Start:
            Start:
            LD A;
            ST 1;
            STN A;
            S A;
            R Flag;
            END_PROGRAM
        "#;
    let output = parse_project("bad_il.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown IL label 'Missing'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("duplicate IL label 'Start'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("IL ST instruction requires a variable operand")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("IL STN target expects BOOL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("IL S target expects BOOL, got integer")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("IL R target expects BOOL")));
}

#[test]
fn checks_configuration_program_and_task_references() {
    let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Demo(Count := 5);
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("config.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
}

#[test]
fn checks_configuration_variable_initializers() {
    let source = r#"
            TYPE
                Small : INT(0..10);
            END_TYPE

            CONFIGURATION Plant
            VAR_GLOBAL
                BadGlobal : Small := 11;
                BadBool : BOOL := 1;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_CONFIG
                    BadResource : Small := 12;
                END_VAR
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("config_initializers.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadGlobal' value 11 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadResource' value 12 is outside subrange 0..10")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("initial value for variable 'BadBool' expects BOOL, got integer")));
}

#[test]
fn checks_configuration_program_instance_initializers() {
    let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            VAR_OUTPUT
                OutCount : INT := 0;
            END_VAR
            VAR_TEMP
                Scratch : INT := 0;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Observed : INT := 0;
                WrongFlag : BOOL := FALSE;
            END_VAR
            RESOURCE Cpu ON PLC
                PROGRAM Good : Demo(Count := ADD(2, 3), Flag := TRUE);
                PROGRAM GoodOutput : Demo(OutCount => Observed);
                PROGRAM BadUnknown : Demo(Missing := 1);
                PROGRAM BadType : Demo(Flag := 1);
                PROGRAM BadTemp : Demo(Scratch := 1);
                PROGRAM BadDuplicate : Demo(Count := 1, Count := 2);
                PROGRAM BadDynamic : Demo(Count := MissingConfig);
                PROGRAM BadOutputKind : Demo(Count => Observed);
                PROGRAM BadOutputType : Demo(OutCount => WrongFlag);
                PROGRAM BadOutputUnknown : Demo(OutCount => MissingTarget);
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("config_program_instance_initializers.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("program instance 'BadUnknown' references unknown PROGRAM variable 'Missing'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("program instance 'BadType' parameter 'Flag' expects BOOL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("program instance 'BadTemp' cannot initialize VAR_TEMP variable 'Scratch'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("program instance 'BadDuplicate' initializes parameter 'Count' more than once")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "program instance 'BadOutputKind' output binding 'Count' must reference a VAR_OUTPUT variable"
            )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "program instance 'BadOutputType' output binding 'OutCount' expects integer target, got BOOL"
            )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "program instance 'BadOutputUnknown' output binding 'OutCount' references unknown target 'MissingTarget'"
            )));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown variable 'MissingConfig'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains(
            "program instance 'BadDynamic' parameter 'Count' must be a constant expression"
        )));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("Good")));
    assert!(!diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("GoodOutput")));
}

#[test]
fn checks_configuration_single_task_expressions() {
    let source = r#"
            PROGRAM Demo
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Trigger : BOOL;
                Count : INT;
            END_VAR
            RESOURCE Cpu ON PLC
                TASK Good(SINGLE := Trigger, PRIORITY := 1);
                TASK BadType(SINGLE := Count, PRIORITY := 2);
                TASK BadUnknown(SINGLE := MissingTrigger, PRIORITY := 3);
                TASK BadInterval(INTERVAL := Trigger, PRIORITY := 4);
                TASK BadPriority(PRIORITY := Trigger);
                TASK BadPriorityNegative(PRIORITY := -1);
                PROGRAM Main WITH Good : Demo;
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("config_single_task.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("task 'BadType' SINGLE expects BOOL, got integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown variable 'MissingTrigger'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("task 'BadInterval' INTERVAL expects TIME duration or integer milliseconds")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("task 'BadPriority' PRIORITY expects integer")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("task 'BadPriorityNegative' PRIORITY must be non-negative")));
}

#[test]
fn checks_nested_user_function_block_fields() {
    let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            Total := Total + In;
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
    let output = parse_project("nested_user_fb_semantics.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.is_empty(), "{:?}", diagnostics);
}

#[test]
fn checks_configuration_access_paths() {
    let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
                Counter : CTU;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                ConfigValue : INT;
            END_VAR
            VAR_ACCESS
                ConfigAccess : ConfigValue : INT READ_ONLY;
                ProgramAccess : Cpu.Main.Count : INT READ_ONLY;
                ProgramFbAccess : Cpu.Main.Counter.CV : INT READ_ONLY;
                BadProgramAccess : Cpu.Main.Missing : INT READ_ONLY;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_GLOBAL
                    ResourceFlag : BOOL;
                END_VAR
                VAR_ACCESS
                    ResourceAccess : ResourceFlag : BOOL READ_ONLY;
                    LocalProgramAccess : Main.Count : INT READ_ONLY;
                    BadResourceAccess : Main.Count : BOOL READ_ONLY;
                END_VAR
                PROGRAM Main : Demo;
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("config_access_paths.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("access path 'BadProgramAccess' references unknown target 'Cpu.Main.Missing'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("access path 'BadResourceAccess' type does not match target 'Main.Count'")));
    for good in [
        "ConfigAccess",
        "ProgramAccess",
        "ProgramFbAccess",
        "ResourceAccess",
        "LocalProgramAccess",
    ] {
        assert!(
            !diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains(&format!("access path '{good}'"))),
            "{good} should not produce diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn flags_bad_configuration_references() {
    let source = r#"
            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                PROGRAM Main WITH MissingTask : MissingProgram;
            END_RESOURCE
            END_CONFIGURATION
        "#;
    let output = parse_project("config.st", source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let diagnostics = check_project(&output.project, &CheckOptions::default());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown PROGRAM type 'MissingProgram'")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("unknown task 'MissingTask'")));
}
