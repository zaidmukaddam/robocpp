(* validation: feature=diagnostics.syntax clause=3.3 *)
PROGRAM MissingEnd
VAR
    Count : INT := 0;
END_VAR

Count := Count + 1;
