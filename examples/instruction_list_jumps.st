PROGRAM IlJumpDemo
VAR
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR

LD Count;
GE 3;
JMPC DoneLabel;
LD Count;
ADD 1;
ST Count;
JMP EndLabel;

DoneLabel:
LD TRUE;
ST Done;

EndLabel:
END_PROGRAM
