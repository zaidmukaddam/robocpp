PROGRAM Demo
VAR
    Mask : INT := 0;
    Inverted : INT := 0;
    Flag : BOOL := FALSE;
END_VAR

Mask := (15 AND 51) OR (1 XOR 3);
Inverted := NOT 15;
Flag := (TRUE AND FALSE) OR TRUE;
END_PROGRAM
