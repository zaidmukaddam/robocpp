(* validation: feature=language.il clause=3.2 *)
PROGRAM InstructionListFile
VAR
    A : INT := 3;
    B : INT := 4;
    Sum : INT := 0;
    Bigger : BOOL := FALSE;
END_VAR

LD A
ADD B
ST Sum
LD Sum
GT 5
ST Bigger
END_PROGRAM
