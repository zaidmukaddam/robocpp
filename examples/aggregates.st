TYPE
    Small : INT(0..10);
    Mode : (Idle, Run, Fault);
    Pair : STRUCT
        Low : Small := 1;
        High : Small := 2;
    END_STRUCT;
END_TYPE

PROGRAM Demo
VAR
    Values : ARRAY [1..3] OF Small := [1, 2, 3];
    Window : Pair := (Low := 4, High := 6);
    State : Mode := Idle;
    Total : INT := 0;
    IsRun : BOOL := FALSE;
END_VAR

Values[2] := Values[1] + Window.High;
Window.Low := Values[2];
State := Run;
IsRun := State = Run;
Total := Values[2] + Window.Low;
END_PROGRAM
