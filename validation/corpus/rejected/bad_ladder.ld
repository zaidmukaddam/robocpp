(* validation: feature=diagnostics.ld clause=4.2 *)
PROGRAM BadLadder
VAR
    Start : BOOL := TRUE;
    Motor : BOOL := FALSE;
END_VAR

LADDER
RUNG Broken:
    CONTACT Start;
    COIL MissingMotor;
END_RUNG;
END_LADDER
END_PROGRAM
