PROGRAM NativeLadderDemo
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
