(* validation: feature=diagnostics.configuration clause=2.7 *)
PROGRAM Worker
END_PROGRAM

CONFIGURATION BadPlant
RESOURCE Cpu ON PLC
    PROGRAM Main WITH MissingTask : Worker;
END_RESOURCE
END_CONFIGURATION
