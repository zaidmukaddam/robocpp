PROGRAM ConfiguredProgram
VAR
    Count : INT := 0;
END_VAR

Count := Count + 1;
END_PROGRAM

CONFIGURATION Plant
VAR_GLOBAL
    PlantOffset : INT := 1;
END_VAR
RESOURCE Cpu ON PLC
    VAR_CONFIG
        Tunable AT %MW10 : INT := 5;
    END_VAR
    VAR_ACCESS
        StartAccess AT %MX0.0 : BOOL;
    END_VAR
    TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM Main WITH Fast : ConfiguredProgram;
END_RESOURCE
END_CONFIGURATION
