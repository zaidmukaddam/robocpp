(* validation: feature=diagnostics.access-paths clause=2.4 *)
PROGRAM BadAccess
VAR_ACCESS
    PublicValue AT Missing : INT;
END_VAR
END_PROGRAM
