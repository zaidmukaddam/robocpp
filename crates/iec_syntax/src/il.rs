// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_ir::IlOp;

pub(crate) fn il_op_from_upper(op: &str) -> Option<IlOp> {
    let base_op = typed_il_base_op(op);
    match base_op {
        "LD" => Some(IlOp::Ld),
        "LDN" => Some(IlOp::Ldn),
        "ST" => Some(IlOp::St),
        "STN" => Some(IlOp::Stn),
        "S" => Some(IlOp::S),
        "R" => Some(IlOp::R),
        "AND" => Some(IlOp::And),
        "ANDN" => Some(IlOp::Andn),
        "OR" => Some(IlOp::Or),
        "ORN" => Some(IlOp::Orn),
        "XOR" => Some(IlOp::Xor),
        "XORN" => Some(IlOp::Xorn),
        "NOT" => Some(IlOp::Not),
        "ADD" => Some(IlOp::Add),
        "SUB" => Some(IlOp::Sub),
        "MUL" => Some(IlOp::Mul),
        "DIV" => Some(IlOp::Div),
        "MOD" => Some(IlOp::Mod),
        "GT" => Some(IlOp::Gt),
        "GE" => Some(IlOp::Ge),
        "EQ" => Some(IlOp::Eq),
        "NE" => Some(IlOp::Ne),
        "LE" => Some(IlOp::Le),
        "LT" => Some(IlOp::Lt),
        "JMP" => Some(IlOp::Jmp),
        "JMPC" => Some(IlOp::Jmpc),
        "JMPCN" => Some(IlOp::Jmpcn),
        "CAL" => Some(IlOp::Cal),
        "CALC" => Some(IlOp::Calc),
        "CALCN" => Some(IlOp::Calcn),
        "RET" => Some(IlOp::Ret),
        "RETC" => Some(IlOp::Retc),
        "RETCN" => Some(IlOp::Retcn),
        _ => None,
    }
}

pub(crate) fn il_op_needs_operand(op: IlOp) -> bool {
    !matches!(op, IlOp::Not | IlOp::Ret | IlOp::Retc | IlOp::Retcn)
}

pub(crate) fn il_op_name(op: IlOp) -> &'static str {
    match op {
        IlOp::Ld => "LD",
        IlOp::Ldn => "LDN",
        IlOp::St => "ST",
        IlOp::Stn => "STN",
        IlOp::S => "S",
        IlOp::R => "R",
        IlOp::And => "AND",
        IlOp::Andn => "ANDN",
        IlOp::Or => "OR",
        IlOp::Orn => "ORN",
        IlOp::Xor => "XOR",
        IlOp::Xorn => "XORN",
        IlOp::Not => "NOT",
        IlOp::Add => "ADD",
        IlOp::Sub => "SUB",
        IlOp::Mul => "MUL",
        IlOp::Div => "DIV",
        IlOp::Mod => "MOD",
        IlOp::Gt => "GT",
        IlOp::Ge => "GE",
        IlOp::Eq => "EQ",
        IlOp::Ne => "NE",
        IlOp::Le => "LE",
        IlOp::Lt => "LT",
        IlOp::Jmp => "JMP",
        IlOp::Jmpc => "JMPC",
        IlOp::Jmpcn => "JMPCN",
        IlOp::Cal => "CAL",
        IlOp::Calc => "CALC",
        IlOp::Calcn => "CALCN",
        IlOp::Ret => "RET",
        IlOp::Retc => "RETC",
        IlOp::Retcn => "RETCN",
    }
}

fn typed_il_base_op(op: &str) -> &str {
    let Some((base, suffix)) = op.split_once('_') else {
        return op;
    };
    if suffix.is_empty() || !is_il_type_suffix(suffix) {
        return op;
    }
    base
}

fn is_il_type_suffix(suffix: &str) -> bool {
    matches!(
        suffix,
        "BOOL"
            | "SINT"
            | "INT"
            | "DINT"
            | "LINT"
            | "USINT"
            | "UINT"
            | "UDINT"
            | "ULINT"
            | "REAL"
            | "LREAL"
            | "BYTE"
            | "WORD"
            | "DWORD"
            | "LWORD"
            | "STRING"
            | "WSTRING"
            | "TIME"
            | "DATE"
            | "TOD"
            | "TIME_OF_DAY"
            | "DT"
            | "DATE_AND_TIME"
    )
}
