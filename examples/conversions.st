PROGRAM Demo
VAR
    Text : STRING[32] := '';
    Parsed : INT := 0;
    Truncated : INT := 0;
    Bcd : WORD := 0;
    FromBcd : INT := 0;
    RealValue : REAL := 0.0;
    Flag : BOOL := FALSE;
    Delay : TIME := T#0ms;
END_VAR

Parsed := STRING_TO_INT('42');
Truncated := TRUNC(-1.6);
Bcd := INT_TO_BCD(369);
FromBcd := BCD_TO_INT(Bcd) + WORD_BCD_TO_UINT(UINT_TO_BCD_WORD(25));
RealValue := STRING_TO_REAL('2.5');
Flag := STRING_TO_BOOL('TRUE');
Delay := STRING_TO_TIME('T#250ms') + INT_TO_TIME(50);
Text := CONCAT(BOOL_TO_STRING(Flag), INT_TO_STRING(Parsed));
END_PROGRAM
