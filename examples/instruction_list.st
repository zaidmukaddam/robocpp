PROGRAM InstructionListDemo
VAR
    A : INT := 3;
    B : INT := 4;
    C : INT := 0;
    Bigger : BOOL := FALSE;
    Complex : BOOL := FALSE;
END_VAR

LD A;
ADD B;
ST C;
GT 5;
ST Bigger;
LD TRUE;
AND (Bigger OR FALSE);
ST Complex;
END_PROGRAM
