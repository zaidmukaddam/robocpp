PROGRAM TimerDemo
VAR
    Delay : TON;
    Pulse : TP;
    Done : BOOL := FALSE;
    PulseActive : BOOL := FALSE;
    Elapsed : TIME := T#0ms;
END_VAR

Delay(IN := TRUE, PT := T#2ms);
Pulse(IN := TRUE, PT := T#2ms);

Done := Delay.Q;
PulseActive := Pulse.Q;
Elapsed := Delay.ET;
END_PROGRAM
