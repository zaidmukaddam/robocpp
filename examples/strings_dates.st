PROGRAM Demo
VAR
    Text : STRING[16] := 'robot';
    Out : STRING[16] := '';
    Found : INT := 0;
    Today : DATE := D#1970-01-02;
    Noon : TIME_OF_DAY := TOD#12:00:00.250;
    Stamp : DATE_AND_TIME := DT#1970-01-02-00:00:01;
END_VAR

Out := CONCAT(LEFT(Text, 2), RIGHT('code', 2));
Found := FIND(INSERT('ro', 'bo', 2), 'bo');
END_PROGRAM
