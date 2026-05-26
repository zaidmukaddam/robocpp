PROGRAM Qualifiers
VAR
    PulseCount : INT := 0;
    DelayCount : INT := 0;
    LimitCount : INT := 0;
END_VAR

INITIAL_STEP Pulse;
STEP Delay;
STEP Limit;
TRANSITION ToDelay := PulseCount >= 1;
TRANSITION ToLimit := DelayCount >= 1;
ACTION Pulse(P):
    PulseCount := PulseCount + 1;
END_ACTION;
ACTION Delay(D, T#2ms):
    DelayCount := DelayCount + 1;
END_ACTION;
ACTION Limit(L, T#2ms):
    LimitCount := LimitCount + 1;
END_ACTION;
END_PROGRAM
