// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::fmt;

use iec_profile::EditionProfile;

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub profile: EditionProfile,
    pub library_elements: Vec<LibraryElement>,
    pub metadata: BTreeMap<String, String>,
}

impl Project {
    pub fn new(profile: EditionProfile) -> Self {
        Self {
            profile,
            library_elements: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn pous(&self) -> impl Iterator<Item = &Pou> {
        self.library_elements.iter().filter_map(|element| {
            if let LibraryElement::Pou(pou) = element {
                Some(pou)
            } else {
                None
            }
        })
    }

    pub fn data_types(&self) -> impl Iterator<Item = &DataTypeDeclaration> {
        self.library_elements.iter().filter_map(|element| {
            if let LibraryElement::DataType(data_type) = element {
                Some(data_type)
            } else {
                None
            }
        })
    }

    pub fn find_pou(&self, name: &str) -> Option<&Pou> {
        let canonical = canonical_identifier(name);
        self.pous().find(|pou| pou.name.canonical == canonical)
    }

    pub fn first_program(&self) -> Option<&Pou> {
        self.pous()
            .find(|pou| matches!(&pou.kind, PouKind::Program))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LibraryElement {
    DataType(DataTypeDeclaration),
    Pou(Pou),
    Configuration(Configuration),
}

impl LibraryElement {
    pub fn name(&self) -> &Identifier {
        match self {
            LibraryElement::DataType(data_type) => &data_type.name,
            LibraryElement::Pou(pou) => &pou.name,
            LibraryElement::Configuration(configuration) => &configuration.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier {
    pub original: String,
    pub canonical: String,
}

impl Identifier {
    pub fn new(input: impl Into<String>) -> Self {
        let original = input.into();
        let canonical = canonical_identifier(&original);
        Self {
            original,
            canonical,
        }
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.original)
    }
}

pub fn canonical_identifier(input: &str) -> String {
    input.trim().to_ascii_uppercase()
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataTypeDeclaration {
    pub name: Identifier,
    pub spec: DataTypeSpec,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataTypeSpec {
    Elementary(ElementaryType),
    Named(Identifier),
    Array {
        ranges: Vec<Subrange>,
        element_type: Box<DataTypeSpec>,
    },
    Struct {
        fields: Vec<StructField>,
    },
    Enum {
        values: Vec<Identifier>,
    },
    Subrange {
        base: ElementaryType,
        range: Subrange,
    },
    String {
        wide: bool,
        length: Option<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementaryType {
    Bool,
    Sint,
    Int,
    Dint,
    Lint,
    Usint,
    Uint,
    Udint,
    Ulint,
    Real,
    Lreal,
    Byte,
    Word,
    Dword,
    Lword,
    String,
    WString,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
}

impl ElementaryType {
    pub fn parse(input: &str) -> Option<Self> {
        match canonical_identifier(input).as_str() {
            "BOOL" => Some(Self::Bool),
            "SINT" => Some(Self::Sint),
            "INT" => Some(Self::Int),
            "DINT" => Some(Self::Dint),
            "LINT" => Some(Self::Lint),
            "USINT" => Some(Self::Usint),
            "UINT" => Some(Self::Uint),
            "UDINT" => Some(Self::Udint),
            "ULINT" => Some(Self::Ulint),
            "REAL" => Some(Self::Real),
            "LREAL" => Some(Self::Lreal),
            "BYTE" => Some(Self::Byte),
            "WORD" => Some(Self::Word),
            "DWORD" => Some(Self::Dword),
            "LWORD" => Some(Self::Lword),
            "STRING" => Some(Self::String),
            "WSTRING" => Some(Self::WString),
            "TIME" => Some(Self::Time),
            "DATE" | "D" => Some(Self::Date),
            "TIME_OF_DAY" | "TOD" => Some(Self::TimeOfDay),
            "DATE_AND_TIME" | "DT" => Some(Self::DateAndTime),
            _ => None,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            Self::Sint
                | Self::Int
                | Self::Dint
                | Self::Lint
                | Self::Usint
                | Self::Uint
                | Self::Udint
                | Self::Ulint
        )
    }

    pub fn as_iec(&self) -> &'static str {
        match self {
            Self::Bool => "BOOL",
            Self::Sint => "SINT",
            Self::Int => "INT",
            Self::Dint => "DINT",
            Self::Lint => "LINT",
            Self::Usint => "USINT",
            Self::Uint => "UINT",
            Self::Udint => "UDINT",
            Self::Ulint => "ULINT",
            Self::Real => "REAL",
            Self::Lreal => "LREAL",
            Self::Byte => "BYTE",
            Self::Word => "WORD",
            Self::Dword => "DWORD",
            Self::Lword => "LWORD",
            Self::String => "STRING",
            Self::WString => "WSTRING",
            Self::Time => "TIME",
            Self::Date => "DATE",
            Self::TimeOfDay => "TIME_OF_DAY",
            Self::DateAndTime => "DATE_AND_TIME",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subrange {
    pub low: i64,
    pub high: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Identifier,
    pub spec: DataTypeSpec,
    pub initial_value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PouKind {
    Function { return_type: DataTypeSpec },
    FunctionBlock,
    Program,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pou {
    pub name: Identifier,
    pub kind: PouKind,
    pub var_blocks: Vec<VarBlock>,
    pub body: PouBody,
}

impl Pou {
    pub fn variable_declarations(&self) -> impl Iterator<Item = &VarDecl> {
        self.var_blocks.iter().flat_map(|block| block.vars.iter())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PouBody {
    pub language: ImplementationLanguage,
    pub statements: Vec<Statement>,
    pub networks: Vec<Network>,
    pub sfc: Option<Sfc>,
}

impl PouBody {
    pub fn structured_text(statements: Vec<Statement>) -> Self {
        Self {
            language: ImplementationLanguage::StructuredText,
            statements,
            networks: Vec::new(),
            sfc: None,
        }
    }
}

impl Default for PouBody {
    fn default() -> Self {
        Self::structured_text(Vec::new())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementationLanguage {
    StructuredText,
    InstructionList,
    SequentialFunctionChart,
    LadderDiagram,
    FunctionBlockDiagram,
    External,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarBlock {
    pub kind: VarBlockKind,
    pub constant: bool,
    pub retain: Option<RetainKind>,
    pub vars: Vec<VarDecl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarBlockKind {
    Local,
    Input,
    Output,
    InOut,
    External,
    Global,
    Temp,
    Access,
    Config,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainKind {
    Retain,
    NonRetain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeQualifier {
    Rising,
    Falling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDirection {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessSpec {
    pub path: String,
    pub direction: AccessDirection,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub name: Identifier,
    pub location: Option<String>,
    pub access: Option<AccessSpec>,
    pub edge: Option<EdgeQualifier>,
    pub type_spec: DataTypeSpec,
    pub initial_value: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Empty,
    Assignment {
        target: VariableRef,
        value: Expr,
    },
    FbCall {
        name: VariableRef,
        args: Vec<ParamAssignment>,
    },
    If {
        branches: Vec<(Expr, Vec<Statement>)>,
        else_branch: Vec<Statement>,
    },
    Case {
        selector: Expr,
        cases: Vec<(Vec<CaseLabel>, Vec<Statement>)>,
        else_branch: Vec<Statement>,
    },
    For {
        control: Identifier,
        from: Expr,
        to: Expr,
        by: Option<Expr>,
        body: Vec<Statement>,
    },
    While {
        condition: Expr,
        body: Vec<Statement>,
    },
    Repeat {
        body: Vec<Statement>,
        until: Expr,
    },
    Il {
        op: IlOp,
        operand: Option<Expr>,
    },
    IlLabel(Identifier),
    Exit,
    Return,
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IlOp {
    Ld,
    Ldn,
    St,
    Stn,
    S,
    R,
    And,
    Andn,
    Or,
    Orn,
    Xor,
    Xorn,
    Not,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Gt,
    Ge,
    Eq,
    Ne,
    Le,
    Lt,
    Jmp,
    Jmpc,
    Jmpcn,
    Cal,
    Calc,
    Calcn,
    Ret,
    Retc,
    Retcn,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseLabel {
    Single(Expr),
    Range(Expr, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamAssignment {
    pub name: Option<Identifier>,
    pub output: bool,
    pub negated: bool,
    pub expr: Option<Expr>,
    pub variable: Option<VariableRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Variable(VariableRef),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: Identifier,
        args: Vec<ParamAssignment>,
    },
    ArrayLiteral(Vec<Expr>),
    StructLiteral(Vec<ParamAssignment>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Real(f64),
    Bool(bool),
    String(String),
    WString(String),
    DurationMs(i128),
    Date(String),
    TimeOfDay(String),
    DateAndTime(String),
    Typed {
        type_name: Identifier,
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableRef {
    pub path: Vec<Identifier>,
    pub indices: Vec<Vec<Expr>>,
    pub direct: Option<String>,
}

impl VariableRef {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            path: vec![Identifier::new(name)],
            indices: vec![Vec::new()],
            direct: None,
        }
    }

    pub fn direct(name: impl Into<String>) -> Self {
        Self {
            path: Vec::new(),
            indices: Vec::new(),
            direct: Some(name.into()),
        }
    }

    pub fn root_name(&self) -> Option<&Identifier> {
        self.path.first()
    }
}

impl fmt::Display for VariableRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(direct) = &self.direct {
            return f.write_str(direct);
        }

        let text = self
            .path
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let mut text = part.original.clone();
                if let Some(indices) = self
                    .indices
                    .get(index)
                    .filter(|indices| !indices.is_empty())
                {
                    text.push('[');
                    text.push_str(
                        &indices
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    text.push(']');
                }
                text
            })
            .collect::<Vec<_>>()
            .join(".");
        f.write_str(&text)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Literal(literal) => write!(f, "{literal}"),
            Expr::Variable(variable) => write!(f, "{variable}"),
            Expr::Unary { op, expr } => write!(f, "{op:?} {expr}"),
            Expr::Binary { op, left, right } => write!(f, "({left} {op:?} {right})"),
            Expr::Call { name, .. } => write!(f, "{}(...)", name.original),
            Expr::ArrayLiteral(_) => f.write_str("[...]"),
            Expr::StructLiteral(_) => f.write_str("(...)"),
        }
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Int(value) => write!(f, "{value}"),
            Literal::Real(value) => write!(f, "{value}"),
            Literal::Bool(value) => f.write_str(if *value { "TRUE" } else { "FALSE" }),
            Literal::String(value) => write!(f, "'{value}'"),
            Literal::WString(value) => write!(f, "\"{value}\""),
            Literal::DurationMs(value) => write!(f, "T#{value}ms"),
            Literal::Date(value) => write!(f, "D#{value}"),
            Literal::TimeOfDay(value) => write!(f, "TOD#{value}"),
            Literal::DateAndTime(value) => write!(f, "DT#{value}"),
            Literal::Typed { type_name, value } => write!(f, "{}#{value}", type_name.original),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    Xor,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Power,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub label: Option<String>,
    pub language: ImplementationLanguage,
    pub nodes: Vec<NetworkNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetworkNode {
    pub id: String,
    pub kind: String,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sfc {
    pub steps: Vec<SfcStep>,
    pub transitions: Vec<SfcTransition>,
    pub actions: Vec<SfcAction>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfcStep {
    pub name: Identifier,
    pub initial: bool,
    pub kind: SfcStepKind,
    pub actions: Vec<SfcActionAssociation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfcStepKind {
    Step,
    MacroStep,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfcTransition {
    pub name: Option<Identifier>,
    pub from: Vec<Identifier>,
    pub to: Vec<Identifier>,
    pub condition: Option<Expr>,
    pub priority: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfcAction {
    pub name: Identifier,
    pub qualifier: SfcActionQualifier,
    pub duration: Option<Literal>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfcActionAssociation {
    pub name: Identifier,
    pub qualifier: Option<SfcActionQualifier>,
    pub duration: Option<Literal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfcActionQualifier {
    NonStored,
    SetStored,
    ResetStored,
    Pulse,
    PulseFalling,
    TimeLimited,
    TimeDelayed,
    StoredDelayed,
    DelayedStored,
    StoredLimited,
}

impl SfcActionQualifier {
    pub fn parse(input: &str) -> Option<Self> {
        match canonical_identifier(input).as_str() {
            "N" => Some(Self::NonStored),
            "S" => Some(Self::SetStored),
            "R" => Some(Self::ResetStored),
            "P" | "P1" => Some(Self::Pulse),
            "P0" => Some(Self::PulseFalling),
            "L" => Some(Self::TimeLimited),
            "D" => Some(Self::TimeDelayed),
            "SD" => Some(Self::StoredDelayed),
            "DS" => Some(Self::DelayedStored),
            "SL" => Some(Self::StoredLimited),
            _ => None,
        }
    }

    pub fn as_iec(self) -> &'static str {
        match self {
            Self::NonStored => "N",
            Self::SetStored => "S",
            Self::ResetStored => "R",
            Self::Pulse => "P",
            Self::PulseFalling => "P0",
            Self::TimeLimited => "L",
            Self::TimeDelayed => "D",
            Self::StoredDelayed => "SD",
            Self::DelayedStored => "DS",
            Self::StoredLimited => "SL",
        }
    }

    pub fn requires_duration(self) -> bool {
        matches!(
            self,
            Self::TimeLimited
                | Self::TimeDelayed
                | Self::StoredDelayed
                | Self::DelayedStored
                | Self::StoredLimited
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    pub name: Identifier,
    pub var_blocks: Vec<VarBlock>,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub name: Identifier,
    pub var_blocks: Vec<VarBlock>,
    pub tasks: Vec<Task>,
    pub program_instances: Vec<ProgramInstance>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub name: Identifier,
    pub single: Option<Expr>,
    pub interval: Option<Expr>,
    pub priority: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramInstance {
    pub name: Identifier,
    pub program_type: Identifier,
    pub task: Option<Identifier>,
    pub args: Vec<ParamAssignment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Real(f64),
    String(String),
    WString(String),
    TimeMs(i128),
    Array(Vec<Value>),
    Struct(BTreeMap<String, Value>),
    Unit,
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            Value::Int(value) => Some(*value != 0),
            Value::Real(value) => Some(*value != 0.0),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Bool(value) => Some(if *value { 1 } else { 0 }),
            Value::Int(value) => Some(*value),
            Value::Real(value) => Some(*value as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            Value::Int(value) => Some(*value as f64),
            Value::Real(value) => Some(*value),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(value) => f.write_str(if *value { "TRUE" } else { "FALSE" }),
            Value::Int(value) => write!(f, "{value}"),
            Value::Real(value) => write!(f, "{value}"),
            Value::String(value) => write!(f, "'{value}'"),
            Value::WString(value) => write!(f, "\"{value}\""),
            Value::TimeMs(value) => write!(f, "T#{value}ms"),
            Value::Array(values) => {
                let text = values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{text}]")
            }
            Value::Struct(fields) => {
                let text = fields
                    .iter()
                    .map(|(name, value)| format!("{name} := {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "({text})")
            }
            Value::Unit => f.write_str("<unit>"),
        }
    }
}

pub fn default_value_for_type(spec: &DataTypeSpec) -> Value {
    match spec {
        DataTypeSpec::Elementary(ElementaryType::Bool) => Value::Bool(false),
        DataTypeSpec::Elementary(ElementaryType::Real | ElementaryType::Lreal) => Value::Real(0.0),
        DataTypeSpec::Elementary(ElementaryType::String)
        | DataTypeSpec::String { wide: false, .. } => Value::String(String::new()),
        DataTypeSpec::Elementary(ElementaryType::WString)
        | DataTypeSpec::String { wide: true, .. } => Value::WString(String::new()),
        DataTypeSpec::Elementary(
            ElementaryType::Time
            | ElementaryType::Date
            | ElementaryType::TimeOfDay
            | ElementaryType::DateAndTime,
        ) => Value::TimeMs(0),
        DataTypeSpec::Elementary(_) | DataTypeSpec::Subrange { .. } => Value::Int(0),
        DataTypeSpec::Named(_) => Value::Int(0),
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => Value::Unit,
        DataTypeSpec::Enum { .. } => Value::Int(0),
    }
}
