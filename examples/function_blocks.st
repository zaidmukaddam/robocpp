PROGRAM FunctionBlockDemo
VAR
    Counter : CTU;
    Latch : SR;
    Edge : R_TRIG;
    Pulse : BOOL := FALSE;
    Count : INT := 0;
    CountDone : BOOL := FALSE;
    Latched : BOOL := FALSE;
    Rising : BOOL := FALSE;
END_VAR

Pulse := NOT Pulse;
Counter(CU := Pulse, R := FALSE, PV := 2);
Latch(S1 := Counter.Q, R := FALSE);
Edge(CLK := Pulse);

Count := Counter.CV;
CountDone := Counter.Q;
Latched := Latch.Q1;
Rising := Edge.Q;
END_PROGRAM
