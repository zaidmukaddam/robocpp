(* validation: feature=robustness.nesting clause=limits *)
PROGRAM DeepExpression
VAR
    A : INT := 1;
    B : INT := 0;
END_VAR

B := (((((((((((((((((((A + 1)))))))))))))))))));
END_PROGRAM
