TYPE
    AxisMode : (Disabled, Homing, Enabled, Faulted);
    Percent : INT(0..100);
    Counts : ARRAY [0..3] OF DINT;
    AxisStatus : STRUCT
        Mode : AxisMode := Disabled;
        Load : Percent := 0;
        Name : STRING[12] := 'axis';
    END_STRUCT;
END_TYPE

PROGRAM AllDataTypes
VAR
    BoolValue : BOOL := BOOL#TRUE;
    SintValue : SINT := SINT#-8;
    IntValue : INT := INT#123;
    DintValue : DINT := DINT#1000;
    LintValue : LINT := LINT#100000;
    UsintValue : USINT := USINT#200;
    UintValue : UINT := UINT#1000;
    UdintValue : UDINT := UDINT#100000;
    UlintValue : ULINT := ULINT#1000000;
    ByteValue : BYTE := BYTE#16#2A;
    WordValue : WORD := WORD#16#00FF;
    DwordValue : DWORD := DWORD#16#0000FFFF;
    LwordValue : LWORD := LWORD#16#00000000000000FF;
    RealValue : REAL := REAL#3.5;
    LrealValue : LREAL := LREAL#2.25;
    Duration : TIME := T#1d2h3m4s5ms;
    Today : DATE := D#2024-02-29;
    Clock : TIME_OF_DAY := TOD#23:59:59.999;
    Stamp : DATE_AND_TIME := DT#2024-02-29-23:59:59.999;
    Text : STRING[32] := STRING#'robot';
    Wide : WSTRING[32] := WSTRING#"axis";
    Mode : AxisMode := AxisMode#Homing;
    Load : Percent := Percent#75;
    Samples : Counts := [4(0)];
    Snapshot : AxisStatus := (Mode := Enabled, Load := 42, Name := 'joint');
END_VAR

BoolValue := NOT BoolValue;
IntValue := SintValue + 10;
DintValue := DintValue + 1;
Samples[1] := DintValue;
Mode := Enabled;
Snapshot.Load := Load;
Text := CONCAT(Text, ' arm');
Wide := WSTRING#"wide";
END_PROGRAM
