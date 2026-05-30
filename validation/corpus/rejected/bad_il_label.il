(* validation: feature=diagnostics.il clause=3.2 *)
PROGRAM BadIlLabel
VAR
    Out : INT := 0;
END_VAR

JMP Missing
END_PROGRAM
