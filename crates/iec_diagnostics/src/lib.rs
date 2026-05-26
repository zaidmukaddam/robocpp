use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub source: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(source: impl Into<String>, start: usize, end: usize, text: &str) -> Self {
        let mut line = 1;
        let mut column = 1;

        for ch in text[..start.min(text.len())].chars() {
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }

        Self {
            source: source.into(),
            start,
            end,
            line,
            column,
        }
    }

    pub fn unknown(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    Io,
    Lexical,
    Syntax,
    Semantic,
    Compliance,
    Runtime,
    Unsupported,
}

impl DiagnosticCode {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticCode::Io => "io",
            DiagnosticCode::Lexical => "lexical",
            DiagnosticCode::Syntax => "syntax",
            DiagnosticCode::Semantic => "semantic",
            DiagnosticCode::Compliance => "compliance",
            DiagnosticCode::Runtime => "runtime",
            DiagnosticCode::Unsupported => "unsupported",
        }
    }

    pub fn stable_id(self) -> &'static str {
        match self {
            DiagnosticCode::Io => "RBCPP-IO",
            DiagnosticCode::Lexical => "RBCPP-LEXICAL",
            DiagnosticCode::Syntax => "RBCPP-SYNTAX",
            DiagnosticCode::Semantic => "RBCPP-SEMANTIC",
            DiagnosticCode::Compliance => "RBCPP-COMPLIANCE",
            DiagnosticCode::Runtime => "RBCPP-RUNTIME",
            DiagnosticCode::Unsupported => "RBCPP-UNSUPPORTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    pub span: Option<Span>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            span,
            help: None,
        }
    }

    pub fn error(code: DiagnosticCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(Severity::Error, code, message, span)
    }

    pub fn warning(code: DiagnosticCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(Severity::Warning, code, message, span)
    }

    pub fn note(code: DiagnosticCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Self::new(Severity::Note, code, message, span)
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn render(&self) -> String {
        let mut rendered = format!(
            "{}[{}]: {}",
            self.severity.as_str(),
            self.code.as_str(),
            self.message
        );

        if let Some(span) = &self.span {
            rendered.push_str(&format!(
                "\n  --> {}:{}:{}",
                span.source, span.line, span.column
            ));
        }

        if let Some(help) = &self.help {
            rendered.push_str(&format!("\n  help: {help}"));
        }

        rendered
    }

    pub fn to_json(&self) -> String {
        let span = if let Some(span) = &self.span {
            format!(
                "{{\"source\":\"{}\",\"start\":{},\"end\":{},\"line\":{},\"column\":{}}}",
                json_escape(&span.source),
                span.start,
                span.end,
                span.line,
                span.column
            )
        } else {
            "null".to_string()
        };

        let help = self
            .help
            .as_ref()
            .map(|h| format!("\"{}\"", json_escape(h)))
            .unwrap_or_else(|| "null".to_string());

        format!(
            "{{\"severity\":\"{}\",\"code\":\"{}\",\"stableCode\":\"{}\",\"message\":\"{}\",\"span\":{},\"help\":{}}}",
            self.severity.as_str(),
            self.code.as_str(),
            self.code.stable_id(),
            json_escape(&self.message),
            span,
            help
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

#[derive(Debug, Default, Clone)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn as_slice(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

pub fn render_diagnostics(diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(Diagnostic::render)
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn diagnostics_to_json(diagnostics: &[Diagnostic]) -> String {
    format!(
        "[{}]",
        diagnostics
            .iter()
            .map(Diagnostic::to_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_json_includes_stable_code() {
        let diagnostic = Diagnostic::error(DiagnosticCode::Semantic, "bad symbol", None);
        let json = diagnostic.to_json();
        assert!(json.contains("\"code\":\"semantic\""));
        assert!(json.contains("\"stableCode\":\"RBCPP-SEMANTIC\""));
    }
}

pub fn json_escape(input: &str) -> String {
    let mut escaped = String::new();
    for ch in input.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}
