(* validation: feature=diagnostics.calls clause=2.5 *)
PROGRAM BadCall
VAR
    Out : INT := 0;
END_VAR

Out := ADD(IN1 := 1, Missing := 2);
END_PROGRAM
