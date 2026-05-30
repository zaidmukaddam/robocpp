import type { WorkspaceFile } from "@/types";

export const workspaceFiles: WorkspaceFile[] = [
  {
    name: "counter.st",
    languageId: "st",
    text: `PROGRAM Counter
VAR
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR

IF Count < 10 THEN
    Count := Count + 1;
ELSE
    Done := TRUE;
END_IF;
END_PROGRAM
`
  },
  {
    name: "native_ladder.ld",
    languageId: "ld",
    text: `PROGRAM NativeLd
VAR
    Start : BOOL;
    Motor : BOOL;
END_VAR
LADDER
RUNG
    CONTACT Start;
    COIL Motor;
END_RUNG
END_LADDER
END_PROGRAM
`
  },
  {
    name: "sequence.sfc",
    languageId: "sfc",
    text: `PROGRAM Sequence
VAR
    Ready : BOOL := TRUE;
    Done : BOOL := FALSE;
END_VAR

INITIAL_STEP Start;
STEP Run;
TRANSITION Go FROM Start TO Run := Ready;
END_TRANSITION;
ACTION Run:
    Done := TRUE;
END_ACTION;
END_PROGRAM
`
  },
  {
    name: "native_fbd.fbd",
    languageId: "fbd",
    text: `PROGRAM NativeFbd
VAR
    Enable : BOOL := TRUE;
    Interlock : BOOL := TRUE;
    MotorCmd : BOOL;
END_VAR
FBD
NETWORK
    OUT MotorCmd := AND(Enable, Interlock);
END_NETWORK
END_FBD
END_PROGRAM
`
  },
  {
    name: "plcopen_fbd.xml",
    languageId: "xml",
    text: `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types>
    <pous>
      <pou name="Blocks" pouType="program">
        <interface>
          <localVars>
            <variable name="A"><type><INT /></type></variable>
            <variable name="B"><type><INT /></type></variable>
            <variable name="C"><type><INT /></type></variable>
          </localVars>
        </interface>
        <body>
          <FBD>
            <inVariable localId="1"><expression>A</expression></inVariable>
            <inVariable localId="2"><expression>B</expression></inVariable>
            <block localId="3" typeName="ADD">
              <inputVariables>
                <variable formalParameter="IN1">
                  <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                </variable>
                <variable formalParameter="IN2">
                  <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                </variable>
              </inputVariables>
            </block>
            <outVariable localId="4">
              <expression>C</expression>
              <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
            </outVariable>
          </FBD>
        </body>
      </pou>
    </pous>
  </types>
</project>
`
  }
];
