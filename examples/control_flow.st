PROGRAM ControlFlowDemo
VAR
    I : INT := 0;
    Total : INT := 0;
    Selected : INT := 0;
    Done : BOOL := FALSE;
END_VAR

FOR I := 1 TO 3 DO
    Total := Total + I;
END_FOR;

WHILE Total < 8 DO
    Total := Total + 1;
END_WHILE;

REPEAT
    Total := Total - 1;
UNTIL Total = 7
END_REPEAT;

Selected := MAX(Total, 3);

CASE Selected OF
    7: Done := TRUE;
    ELSE Done := FALSE;
END_CASE;
END_PROGRAM
