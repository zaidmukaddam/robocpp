(* validation: feature=language.st clause=3.3 *)
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
