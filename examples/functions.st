FUNCTION Scale : INT
VAR_INPUT
    Input : INT;
    Factor : INT;
END_VAR

Scale := Input * Factor;
END_FUNCTION

PROGRAM FunctionDemo
VAR
    A : INT := 4;
    B : INT := 0;
    C : INT := 0;
END_VAR

B := Scale(A, 3);
C := Scale(Input := B, Factor := 2);
END_PROGRAM
