PROGRAM Demo
VAR
    Counter : CTU;
    Done : BOOL := FALSE;
    Cv : INT := 0;
    Skipped : INT := 0;
END_VAR

LD TRUE;
CALC Counter(CU := TRUE, R := FALSE, PV := 1);
LD Counter.Q;
ST Done;
LD Counter.CV;
ST Cv;
LD TRUE;
RETC;
Skipped := 1;
END_PROGRAM
