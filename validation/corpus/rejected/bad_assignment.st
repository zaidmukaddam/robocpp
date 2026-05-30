(* validation: feature=diagnostics.semantic clause=3.3 *)
PROGRAM BadAssignment
VAR
    Count : INT := 0;
END_VAR

Count := TRUE;
END_PROGRAM
