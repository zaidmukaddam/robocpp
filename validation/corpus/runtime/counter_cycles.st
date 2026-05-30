(* validation: feature=runtime.scan-cycle clause=2.7 cycles=4 program=CounterDemo *)
PROGRAM CounterDemo
VAR
    Count : INT := 0;
    Limit : INT := 3;
    Done : BOOL := FALSE;
END_VAR

IF Count < Limit THEN
    Count := Count + 1;
ELSE
    Done := TRUE;
END_IF;
END_PROGRAM
