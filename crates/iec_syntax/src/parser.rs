// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use iec_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticCode};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};

use crate::il::{il_op_from_upper, il_op_name, il_op_needs_operand};
use crate::literal::{parse_hash_literal_checked, parse_number_literal_checked};
use crate::token::{Symbol, Token, TokenKind};

pub(crate) struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    source: &'a str,
    implementation: ImplementationParameters,
    expression_stop: Option<usize>,
    expression_depth: usize,
    pub(crate) diagnostics: DiagnosticBag,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(
        _source_name: String,
        source: &'a str,
        tokens: Vec<Token>,
        implementation: ImplementationParameters,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            source,
            implementation,
            expression_stop: None,
            expression_depth: 0,
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(crate) fn parse_project(&mut self) -> Project {
        let mut project = Project::new(EditionProfile::Iec61131_3_2003Strict);

        while !self.is_eof() {
            if self.match_keyword("TYPE") {
                project.library_elements.extend(
                    self.parse_type_section()
                        .into_iter()
                        .map(LibraryElement::DataType),
                );
            } else if self.match_keyword("FUNCTION") {
                project
                    .library_elements
                    .push(LibraryElement::Pou(self.parse_pou(PouStart::Function)));
            } else if self.match_keyword("FUNCTION_BLOCK") {
                project
                    .library_elements
                    .push(LibraryElement::Pou(self.parse_pou(PouStart::FunctionBlock)));
            } else if self.match_keyword("PROGRAM") {
                project
                    .library_elements
                    .push(LibraryElement::Pou(self.parse_pou(PouStart::Program)));
            } else if self.match_keyword("CONFIGURATION") {
                project
                    .library_elements
                    .push(LibraryElement::Configuration(self.parse_configuration()));
            } else if self.check_symbol(Symbol::Semicolon) {
                self.advance();
            } else {
                let token = self.current().clone();
                self.error_at(
                    &token,
                    format!("expected TYPE, FUNCTION, FUNCTION_BLOCK, PROGRAM, or CONFIGURATION; found '{}'", token.lexeme),
                );
                self.advance();
            }
        }

        project
    }

    fn parse_type_section(&mut self) -> Vec<DataTypeDeclaration> {
        let mut declarations = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_TYPE") {
            let Some(name) = self.expect_identifier("expected type name") else {
                self.synchronize_to_semicolon();
                continue;
            };
            self.expect_symbol(Symbol::Colon, "expected ':' after type name");
            let spec = self.parse_type_spec();
            if self.match_symbol(Symbol::Assign) {
                let _ = self.parse_expression();
            }
            self.expect_symbol(Symbol::Semicolon, "expected ';' after type declaration");
            declarations.push(DataTypeDeclaration { name, spec });
        }
        declarations
    }

    fn parse_type_spec(&mut self) -> DataTypeSpec {
        if self.match_keyword("ARRAY") {
            self.expect_symbol(Symbol::LBracket, "expected '[' after ARRAY");
            let mut ranges = Vec::new();
            loop {
                let low = self.expect_signed_integer("expected array lower bound");
                self.expect_symbol(Symbol::Range, "expected '..' in array range");
                let high = self.expect_signed_integer("expected array upper bound");
                ranges.push(Subrange { low, high });
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RBracket, "expected ']' after array ranges");
            self.expect_keyword("OF", "expected OF after ARRAY range");
            let element_type = Box::new(self.parse_type_spec());
            return DataTypeSpec::Array {
                ranges,
                element_type,
            };
        }

        if self.match_keyword("STRUCT") {
            let mut fields = Vec::new();
            while !self.is_eof() && !self.match_keyword("END_STRUCT") {
                let Some(name) = self.expect_identifier("expected structure field name") else {
                    self.synchronize_to_semicolon();
                    continue;
                };
                self.expect_symbol(Symbol::Colon, "expected ':' after field name");
                let spec = self.parse_type_spec();
                let initial_value = if self.match_symbol(Symbol::Assign) {
                    Some(self.parse_expression())
                } else {
                    None
                };
                self.expect_symbol(Symbol::Semicolon, "expected ';' after structure field");
                fields.push(StructField {
                    name,
                    spec,
                    initial_value,
                });
            }
            return DataTypeSpec::Struct { fields };
        }

        if self.match_symbol(Symbol::LParen) {
            let mut values = Vec::new();
            loop {
                if let Some(value) = self.expect_identifier("expected enumerated value") {
                    values.push(value);
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after enumerated values");
            return DataTypeSpec::Enum { values };
        }

        let Some(name) = self.expect_identifier("expected type specification") else {
            return DataTypeSpec::Named(Identifier::new("<error>"));
        };

        if self.match_symbol(Symbol::LParen) {
            if let Some(base) = ElementaryType::parse(&name.original) {
                let low = self.expect_signed_integer("expected subrange lower bound");
                self.expect_symbol(Symbol::Range, "expected '..' in subrange");
                let high = self.expect_signed_integer("expected subrange upper bound");
                self.expect_symbol(Symbol::RParen, "expected ')' after subrange");
                return DataTypeSpec::Subrange {
                    base,
                    range: Subrange { low, high },
                };
            }
        }

        if matches!(
            canonical_identifier(&name.original).as_str(),
            "STRING" | "WSTRING"
        ) {
            let wide = canonical_identifier(&name.original) == "WSTRING";
            let length = if self.match_symbol(Symbol::LBracket) {
                let value = self.expect_unsigned_integer("expected string length");
                self.expect_symbol(Symbol::RBracket, "expected ']' after string length");
                Some(value)
            } else {
                None
            };
            return DataTypeSpec::String { wide, length };
        }

        if let Some(elementary) = ElementaryType::parse(&name.original) {
            DataTypeSpec::Elementary(elementary)
        } else {
            DataTypeSpec::Named(name)
        }
    }

    fn parse_pou(&mut self, start: PouStart) -> Pou {
        let name = self
            .expect_identifier("expected POU name")
            .unwrap_or_else(|| Identifier::new("<error>"));

        let kind = match start {
            PouStart::Function => {
                self.expect_symbol(Symbol::Colon, "expected ':' after function name");
                let return_type = self.parse_type_spec();
                PouKind::Function { return_type }
            }
            PouStart::FunctionBlock => PouKind::FunctionBlock,
            PouStart::Program => PouKind::Program,
        };

        let mut var_blocks = Vec::new();
        while self.is_var_block_start() {
            var_blocks.push(self.parse_var_block());
        }

        let end_keyword = match start {
            PouStart::Function => "END_FUNCTION",
            PouStart::FunctionBlock => "END_FUNCTION_BLOCK",
            PouStart::Program => "END_PROGRAM",
        };
        let body = if self.match_keyword("LADDER") {
            let body = self.parse_textual_ladder_body("END_LADDER");
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            body
        } else if self.match_keyword("FBD") {
            let body = self.parse_textual_fbd_body("END_FBD");
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            body
        } else if self.is_sfc_statement_start() {
            let sfc = self.parse_sfc_body(end_keyword);
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            PouBody {
                language: ImplementationLanguage::SequentialFunctionChart,
                statements: Vec::new(),
                networks: Vec::new(),
                sfc: Some(sfc),
            }
        } else {
            let statements = self.parse_statement_list(&[end_keyword]);
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            PouBody::structured_text(statements)
        };

        Pou {
            name,
            kind,
            var_blocks,
            body,
        }
    }

    fn parse_textual_ladder_body(&mut self, end_keyword: &str) -> PouBody {
        let mut statements = Vec::new();
        let mut networks = Vec::new();

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let label = if self.match_keyword("RUNG") || self.match_keyword("NETWORK") {
                self.parse_optional_network_label()
            } else {
                None
            };
            let rung = self.parse_textual_ladder_rung(&["END_RUNG", "END_NETWORK", end_keyword]);
            statements.extend(rung.statements);
            networks.push(Network {
                label: label.or(rung.label),
                language: ImplementationLanguage::LadderDiagram,
                nodes: rung.nodes,
            });

            if self.match_keyword("END_RUNG") || self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }

        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        PouBody {
            language: ImplementationLanguage::LadderDiagram,
            statements,
            networks,
            sfc: None,
        }
    }

    fn parse_textual_fbd_body(&mut self, end_keyword: &str) -> PouBody {
        let mut statements = Vec::new();
        let mut networks = Vec::new();

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let label = if self.match_keyword("NETWORK") {
                self.parse_optional_network_label()
            } else {
                None
            };
            let network = self.parse_textual_fbd_network(&["END_NETWORK", end_keyword]);
            statements.extend(network.statements);
            networks.push(Network {
                label: label.or(network.label),
                language: ImplementationLanguage::FunctionBlockDiagram,
                nodes: network.nodes,
            });

            if self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }

        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        PouBody {
            language: ImplementationLanguage::FunctionBlockDiagram,
            statements,
            networks,
            sfc: None,
        }
    }

    fn parse_sfc_body(&mut self, end_keyword: &str) -> Sfc {
        let mut sfc = Sfc {
            steps: Vec::new(),
            transitions: Vec::new(),
            actions: Vec::new(),
        };

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_keyword("INITIAL_STEP") {
                sfc.steps.push(self.parse_sfc_step(true, None));
            } else if self.match_keyword("STEP") {
                sfc.steps.push(self.parse_sfc_step(false, None));
            } else if self.is_labeled_sfc_step() {
                let name = self.current_identifier();
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after SFC step label");
                let initial = if self.match_keyword("INITIAL_STEP") {
                    true
                } else {
                    self.expect_keyword("STEP", "expected STEP or INITIAL_STEP after SFC label");
                    false
                };
                sfc.steps.push(self.parse_sfc_step(initial, name));
            } else if self.match_keyword("TRANSITION") {
                sfc.transitions.push(self.parse_sfc_transition(None));
            } else if self.is_labeled_sfc_transition() {
                let name = self.current_identifier();
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after SFC transition label");
                self.expect_keyword("TRANSITION", "expected TRANSITION after SFC label");
                sfc.transitions.push(self.parse_sfc_transition(name));
            } else if self.match_keyword("ACTION") {
                sfc.actions.push(self.parse_sfc_action(None));
            } else if self.is_labeled_sfc_action() {
                let name = self.current_identifier();
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after SFC action label");
                self.expect_keyword("ACTION", "expected ACTION after SFC label");
                sfc.actions.push(self.parse_sfc_action(name));
            } else {
                let token = self.current().clone();
                self.error_at(
                    &token,
                    format!("unsupported or invalid SFC element '{}'", token.lexeme),
                );
                self.synchronize_to_semicolon();
                self.match_symbol(Symbol::Semicolon);
            }
        }

        sfc
    }

    fn parse_sfc_action(&mut self, labeled_name: Option<Identifier>) -> SfcAction {
        let is_labeled_form = labeled_name.is_some();
        let name = labeled_name.unwrap_or_else(|| {
            self.expect_identifier("expected action name")
                .unwrap_or_else(|| Identifier::new("<error>"))
        });
        let (qualifier, duration) = self.parse_sfc_action_qualifier();
        if is_labeled_form {
            self.match_symbol(Symbol::Colon);
        } else {
            self.expect_symbol(Symbol::Colon, "expected ':' after action name");
        }
        let body = self.parse_statement_list(&["END_ACTION"]);
        self.expect_keyword("END_ACTION", "expected END_ACTION");
        self.match_symbol(Symbol::Semicolon);
        SfcAction {
            name,
            qualifier,
            duration,
            body,
        }
    }

    fn parse_sfc_step(&mut self, initial: bool, labeled_name: Option<Identifier>) -> SfcStep {
        let is_labeled_form = labeled_name.is_some();
        let name = labeled_name.unwrap_or_else(|| {
            self.expect_identifier(if initial {
                "expected initial step name"
            } else {
                "expected step name"
            })
            .unwrap_or_else(|| Identifier::new("<error>"))
        });
        let actions = if self.match_symbol(Symbol::Colon)
            || is_labeled_form && !self.check_symbol(Symbol::Semicolon)
        {
            let actions = self.parse_sfc_action_associations();
            self.expect_keyword("END_STEP", "expected END_STEP after SFC step actions");
            self.match_symbol(Symbol::Semicolon);
            actions
        } else {
            self.expect_symbol(
                Symbol::Semicolon,
                if initial {
                    "expected ';' after initial step"
                } else {
                    "expected ';' after step"
                },
            );
            Vec::new()
        };
        SfcStep {
            name,
            initial,
            kind: SfcStepKind::Step,
            actions,
        }
    }

    fn parse_sfc_action_associations(&mut self) -> Vec<SfcActionAssociation> {
        let mut actions = Vec::new();
        while !self.is_eof() && !self.check_keyword("END_STEP") {
            let start_pos = self.pos;
            let Some(name) = self.expect_identifier("expected SFC action association name") else {
                self.synchronize_to_sfc_action_association_boundary();
                if self.pos == start_pos && !self.is_eof() {
                    self.advance();
                }
                continue;
            };
            let (qualifier, duration) = self.parse_sfc_action_qualifier();
            self.expect_symbol(
                Symbol::Semicolon,
                "expected ';' after SFC action association",
            );
            actions.push(SfcActionAssociation {
                name,
                qualifier: Some(qualifier),
                duration,
            });
            if self.pos == start_pos && !self.is_eof() {
                self.advance();
            }
        }
        actions
    }

    fn synchronize_to_sfc_action_association_boundary(&mut self) {
        while !self.is_eof()
            && !self.check_keyword("END_STEP")
            && !self.check_symbol(Symbol::Semicolon)
        {
            self.advance();
        }
        self.match_symbol(Symbol::Semicolon);
    }

    fn parse_sfc_transition(&mut self, prefixed_name: Option<Identifier>) -> SfcTransition {
        let name = if prefixed_name.is_some() {
            prefixed_name
        } else if self.current_identifier().is_some()
            && !self.check_keyword("FROM")
            && (self.peek_symbol(Symbol::Assign)
                || self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|token| matches!(&token.kind, TokenKind::Ident(value) if canonical_identifier(value) == "FROM"))
                || self.peek_symbol(Symbol::LParen))
        {
            let name = self.current_identifier();
            self.advance();
            name
        } else {
            None
        };

        let priority = if self.match_symbol(Symbol::LParen) {
            let mut priority = None;
            if self.match_keyword("PRIORITY") {
                self.expect_symbol(Symbol::Assign, "expected ':=' in SFC transition priority");
                priority = self.parse_integer_token("expected integer SFC transition priority");
            }
            while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
                self.advance();
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after SFC transition options");
            priority
        } else {
            None
        };

        if self.match_keyword("FROM") {
            let from = self.parse_sfc_step_list("expected SFC transition predecessor step");
            self.expect_keyword("TO", "expected TO in SFC transition");
            let to = self.parse_sfc_step_list("expected SFC transition successor step");
            let condition = if self.match_symbol(Symbol::Assign) {
                let condition = Some(self.parse_expression());
                self.expect_symbol(Symbol::Semicolon, "expected ';' after transition condition");
                condition
            } else if self.match_symbol(Symbol::Colon) {
                self.parse_sfc_transition_body_condition()
            } else {
                self.error_at(
                    &self.current().clone(),
                    "expected ':=' or ':' in SFC transition",
                );
                None
            };
            self.expect_keyword("END_TRANSITION", "expected END_TRANSITION");
            self.match_symbol(Symbol::Semicolon);
            SfcTransition {
                name,
                from,
                to,
                condition,
                priority,
            }
        } else {
            self.expect_symbol(Symbol::Assign, "expected ':=' in transition");
            let condition = Some(self.parse_expression());
            self.expect_symbol(Symbol::Semicolon, "expected ';' after transition");
            SfcTransition {
                name,
                from: Vec::new(),
                to: Vec::new(),
                condition,
                priority,
            }
        }
    }

    fn parse_sfc_transition_body_condition(&mut self) -> Option<Expr> {
        if self.match_keyword("LADDER") {
            return self.parse_textual_ladder_transition_condition("END_LADDER");
        }
        if self.match_keyword("FBD") {
            return self.parse_textual_fbd_transition_condition("END_FBD");
        }

        let mut accumulator = None;
        while !self.is_eof() && !self.check_keyword("END_TRANSITION") {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let token = self.current().clone();
            if let Some(op) = self.current_il_op() {
                self.advance();
                let operand = if il_op_needs_operand(op)
                    && !self.check_keyword("END_TRANSITION")
                    && !self.check_symbol(Symbol::Semicolon)
                {
                    Some(self.parse_il_operand())
                } else {
                    None
                };
                accumulator = self.fold_il_expression(accumulator, op, operand, &token);
                self.match_symbol(Symbol::Semicolon);
                continue;
            }

            if accumulator.is_some() {
                self.error_at(
                    &token,
                    format!(
                        "expected IL instruction or END_TRANSITION in SFC transition body, found '{}'",
                        token.lexeme
                    ),
                );
                self.synchronize_to_keyword("END_TRANSITION");
                break;
            }

            let stop = self.line_end_after(token.span.start);
            accumulator = Some(self.parse_expression_until(stop));
            self.match_symbol(Symbol::Semicolon);
        }

        if accumulator.is_none() {
            self.error_at(
                &self.current().clone(),
                "SFC transition body requires an expression or IL accumulator body",
            );
        }
        accumulator
    }

    fn parse_textual_ladder_transition_condition(&mut self, end_keyword: &str) -> Option<Expr> {
        let mut conditions = Vec::new();
        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }
            if self.match_keyword("RUNG") || self.match_keyword("NETWORK") {
                self.parse_optional_network_label();
            }
            let rung = self.parse_textual_ladder_rung(&["END_RUNG", "END_NETWORK", end_keyword]);
            conditions.push(rung.condition);
            if self.match_keyword("END_RUNG") || self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }
        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        self.or_exprs(conditions)
    }

    fn parse_textual_fbd_transition_condition(&mut self, end_keyword: &str) -> Option<Expr> {
        let mut conditions = Vec::new();
        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }
            if self.match_keyword("NETWORK") {
                self.parse_optional_network_label();
            }

            while !self.is_eof() && !self.check_any_keyword(&["END_NETWORK", end_keyword]) {
                if self.match_symbol(Symbol::Semicolon) {
                    continue;
                }
                if self.match_keyword("OUT") || self.match_keyword("CONDITION") {
                    self.expect_symbol(Symbol::Assign, "expected ':=' after FBD transition OUT");
                    let expr = self.parse_expression();
                    self.expect_symbol(
                        Symbol::Semicolon,
                        "expected ';' after FBD transition output",
                    );
                    conditions.push(expr);
                } else {
                    let stop = self.line_end_after(self.current().span.start);
                    let expr = self.parse_expression_until(stop);
                    self.expect_symbol(
                        Symbol::Semicolon,
                        "expected ';' after FBD transition expression",
                    );
                    conditions.push(expr);
                }
            }

            if self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }
        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        self.and_exprs(conditions)
    }

    fn parse_optional_network_label(&mut self) -> Option<String> {
        let label = self.current_identifier()?;
        if self.peek_symbol(Symbol::Colon) {
            self.advance();
            self.expect_symbol(Symbol::Colon, "expected ':' after network label");
            Some(label.original)
        } else {
            None
        }
    }

    fn parse_textual_ladder_rung(&mut self, stop_keywords: &[&str]) -> TextualLadderRung {
        let label = self.parse_optional_network_label();
        let mut condition = Expr::Literal(Literal::Bool(true));
        let mut statements = Vec::new();
        let mut nodes = vec![textual_network_node("leftPowerRail", 1, &[])];
        let mut node_index = 1usize;
        let mut last_node_id = "1".to_string();

        while !self.is_eof() && !self.check_any_keyword(stop_keywords) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            if self.match_keyword("CONTACT") {
                let expr = self.parse_expression();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD CONTACT");
                condition = self.and_expr(condition, expr.clone());
                node_index += 1;
                let node_id = node_index.to_string();
                let mut attributes = vec![("connectionRefs", last_node_id.clone())];
                if let Some(variable) = expr_variable_name(&expr) {
                    attributes.push(("variable", variable));
                } else {
                    attributes.push(("expression", expr.to_string()));
                }
                nodes.push(textual_network_node("contact", node_index, &attributes));
                last_node_id = node_id;
                continue;
            }

            if self.match_keyword("CONTACT_NOT") || self.match_keyword("CONTACTN") {
                let raw_expr = self.parse_expression();
                let expr = Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(raw_expr.clone()),
                };
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD CONTACT_NOT");
                condition = self.and_expr(condition, expr.clone());
                node_index += 1;
                let node_id = node_index.to_string();
                let mut attributes = vec![
                    ("connectionRefs", last_node_id.clone()),
                    ("negated", "true".to_string()),
                ];
                if let Some(variable) = expr_variable_name(&raw_expr) {
                    attributes.push(("variable", variable));
                } else {
                    attributes.push(("expression", raw_expr.to_string()));
                }
                nodes.push(textual_network_node("contact", node_index, &attributes));
                last_node_id = node_id;
                continue;
            }

            if self.match_keyword("COIL") || self.match_keyword("COIL_NOT") {
                let negated = canonical_identifier(&self.previous().lexeme) == "COIL_NOT";
                let target = self.parse_variable_ref();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD COIL");
                let value = if negated {
                    Expr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(condition.clone()),
                    }
                } else {
                    condition.clone()
                };
                statements.push(Statement::Assignment {
                    target: target.clone(),
                    value,
                });
                node_index += 1;
                nodes.push(textual_network_node(
                    "coil",
                    node_index,
                    &[
                        ("connectionRefs", last_node_id.clone()),
                        ("variable", target.to_string()),
                    ],
                ));
                continue;
            }

            if self.match_keyword("SET") || self.match_keyword("RESET") {
                let set = canonical_identifier(&self.previous().lexeme) == "SET";
                let target = self.parse_variable_ref();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD SET/RESET coil");
                statements.push(Statement::If {
                    branches: vec![(
                        condition.clone(),
                        vec![Statement::Assignment {
                            target: target.clone(),
                            value: Expr::Literal(Literal::Bool(set)),
                        }],
                    )],
                    else_branch: Vec::new(),
                });
                node_index += 1;
                nodes.push(textual_network_node(
                    "coil",
                    node_index,
                    &[
                        ("connectionRefs", last_node_id.clone()),
                        ("variable", target.to_string()),
                        ("storage", if set { "set" } else { "reset" }.to_string()),
                    ],
                ));
                continue;
            }

            let token = self.current().clone();
            self.error_at(
                &token,
                format!(
                    "unsupported or invalid textual LD element '{}'",
                    token.lexeme
                ),
            );
            self.synchronize_to_semicolon();
        }

        TextualLadderRung {
            label,
            condition,
            statements,
            nodes,
        }
    }

    fn parse_textual_fbd_network(&mut self, stop_keywords: &[&str]) -> TextualFbdNetwork {
        let label = self.parse_optional_network_label();
        let mut statements = Vec::new();
        let mut nodes = Vec::new();
        let mut node_index = 0usize;

        while !self.is_eof() && !self.check_any_keyword(stop_keywords) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            if self.match_keyword("OUT") || self.match_keyword("BLOCK") {
                let target = self.parse_variable_ref();
                self.expect_symbol(Symbol::Assign, "expected ':=' after FBD output target");
                let value = self.parse_expression();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after FBD output");
                statements.push(Statement::Assignment {
                    target: target.clone(),
                    value: value.clone(),
                });
                node_index += 1;
                nodes.push(textual_network_node(
                    "outVariable",
                    node_index,
                    &[
                        ("expression", target.to_string()),
                        ("value", value.to_string()),
                    ],
                ));
                continue;
            }

            let statement = self.parse_statement();
            statements.push(statement);
            self.match_symbol(Symbol::Semicolon);
        }

        TextualFbdNetwork {
            label,
            statements,
            nodes,
        }
    }

    fn and_expr(&self, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            op: BinaryOp::And,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn and_exprs(&self, mut exprs: Vec<Expr>) -> Option<Expr> {
        if exprs.is_empty() {
            return None;
        }
        let first = exprs.remove(0);
        Some(
            exprs
                .into_iter()
                .fold(first, |left, right| self.and_expr(left, right)),
        )
    }

    fn or_exprs(&self, mut exprs: Vec<Expr>) -> Option<Expr> {
        if exprs.is_empty() {
            return None;
        }
        let first = exprs.remove(0);
        Some(exprs.into_iter().fold(first, |left, right| Expr::Binary {
            op: BinaryOp::Or,
            left: Box::new(left),
            right: Box::new(right),
        }))
    }

    fn parse_sfc_step_list(&mut self, message: &str) -> Vec<Identifier> {
        if self.match_symbol(Symbol::LParen) {
            let mut steps = Vec::new();
            while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
                if let Some(step) = self.expect_identifier(message) {
                    steps.push(step);
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after SFC step list");
            steps
        } else {
            self.expect_identifier(message)
                .map(|step| vec![step])
                .unwrap_or_default()
        }
    }

    fn parse_integer_token(&mut self, message: &str) -> Option<i64> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Number(text) => {
                self.advance();
                text.parse::<i64>().ok()
            }
            _ => {
                self.error_at(&token, message);
                None
            }
        }
    }

    fn parse_sfc_action_qualifier(&mut self) -> (SfcActionQualifier, Option<Literal>) {
        if !self.match_symbol(Symbol::LParen) {
            return (SfcActionQualifier::NonStored, None);
        }

        let qualifier = if let Some(name) = self.expect_identifier("expected SFC action qualifier")
        {
            SfcActionQualifier::parse(&name.original).unwrap_or_else(|| {
                let token = self.previous().clone();
                self.error_at(
                    &token,
                    format!("unknown SFC action qualifier '{}'", name.original),
                );
                SfcActionQualifier::NonStored
            })
        } else {
            SfcActionQualifier::NonStored
        };

        let duration = if self.match_symbol(Symbol::Comma) {
            let expr = self.parse_expression();
            if let Expr::Literal(literal) = expr {
                Some(literal)
            } else {
                let token = self.previous().clone();
                self.error_at(&token, "expected literal duration in SFC action qualifier");
                None
            }
        } else {
            None
        };
        self.expect_symbol(Symbol::RParen, "expected ')' after SFC action qualifier");
        (qualifier, duration)
    }

    fn parse_var_block(&mut self) -> VarBlock {
        let kind = match self.current_ident_upper().as_deref() {
            Some("VAR_INPUT") => VarBlockKind::Input,
            Some("VAR_OUTPUT") => VarBlockKind::Output,
            Some("VAR_IN_OUT") => VarBlockKind::InOut,
            Some("VAR_EXTERNAL") => VarBlockKind::External,
            Some("VAR_GLOBAL") => VarBlockKind::Global,
            Some("VAR_TEMP") => VarBlockKind::Temp,
            Some("VAR_ACCESS") => VarBlockKind::Access,
            Some("VAR_CONFIG") => VarBlockKind::Config,
            _ => VarBlockKind::Local,
        };
        self.advance();

        let mut constant = false;
        let mut retain = None;
        loop {
            if self.match_keyword("CONSTANT") {
                constant = true;
            } else if self.match_keyword("RETAIN") {
                retain = Some(RetainKind::Retain);
            } else if self.match_keyword("NON_RETAIN") {
                retain = Some(RetainKind::NonRetain);
            } else {
                break;
            }
        }

        let mut vars = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_VAR") {
            if kind == VarBlockKind::Access && self.peek_symbol(Symbol::Colon) {
                vars.push(self.parse_access_decl());
            } else {
                vars.extend(self.parse_var_decl());
            }
        }

        VarBlock {
            kind,
            constant,
            retain,
            vars,
        }
    }

    fn parse_var_decl(&mut self) -> Vec<VarDecl> {
        let mut names = Vec::new();
        loop {
            let Some(name) = self.expect_identifier("expected variable name") else {
                self.synchronize_to_semicolon();
                return Vec::new();
            };
            let location = if self.match_keyword("AT") {
                match &self.current().kind {
                    TokenKind::DirectVariable(value)
                    | TokenKind::Ident(value)
                    | TokenKind::HashLiteral(value) => {
                        let value = value.clone();
                        self.advance();
                        Some(value)
                    }
                    _ => {
                        let token = self.current().clone();
                        self.error_at(&token, "expected location after AT");
                        None
                    }
                }
            } else {
                None
            };
            names.push((name, location));
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }

        self.expect_symbol(Symbol::Colon, "expected ':' in variable declaration");
        let type_spec = self.parse_type_spec();
        let edge = if self.match_keyword("R_EDGE") {
            Some(EdgeQualifier::Rising)
        } else if self.match_keyword("F_EDGE") {
            Some(EdgeQualifier::Falling)
        } else {
            None
        };
        let initial_value = if self.match_symbol(Symbol::Assign) {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect_symbol(Symbol::Semicolon, "expected ';' after variable declaration");

        names
            .into_iter()
            .map(|(name, location)| VarDecl {
                name,
                location,
                access: None,
                edge,
                type_spec: type_spec.clone(),
                initial_value: initial_value.clone(),
            })
            .collect()
    }

    fn parse_access_decl(&mut self) -> VarDecl {
        let name = self
            .expect_identifier("expected access path name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        self.expect_symbol(Symbol::Colon, "expected ':' after access path name");
        let mut path = String::new();
        while !self.is_eof() && !self.check_symbol(Symbol::Colon) {
            path.push_str(&self.current().lexeme);
            self.advance();
        }
        if path.is_empty() {
            let token = self.current().clone();
            self.error_at(&token, "expected access path target");
        }
        self.expect_symbol(Symbol::Colon, "expected ':' after access path target");
        let type_spec = self.parse_type_spec();
        let direction = if self.match_keyword("READ_WRITE") {
            AccessDirection::ReadWrite
        } else {
            let _ = self.match_keyword("READ_ONLY");
            AccessDirection::ReadOnly
        };
        self.expect_symbol(Symbol::Semicolon, "expected ';' after access declaration");
        VarDecl {
            name,
            location: None,
            access: Some(AccessSpec { path, direction }),
            edge: None,
            type_spec,
            initial_value: None,
        }
    }

    fn parse_configuration(&mut self) -> Configuration {
        let name = self
            .expect_identifier("expected configuration name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let mut var_blocks = Vec::new();
        let mut resources = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_CONFIGURATION") {
            if self.match_keyword("RESOURCE") {
                resources.push(self.parse_resource());
            } else if self.is_var_block_start() {
                var_blocks.push(self.parse_var_block());
            } else {
                self.advance();
            }
        }
        Configuration {
            name,
            var_blocks,
            resources,
        }
    }

    fn parse_resource(&mut self) -> Resource {
        let name = self
            .expect_identifier("expected resource name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        if self.match_keyword("ON") {
            let _ = self.expect_identifier("expected resource type after ON");
        }

        let mut tasks = Vec::new();
        let mut var_blocks = Vec::new();
        let mut program_instances = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_RESOURCE") {
            if self.match_keyword("TASK") {
                tasks.push(self.parse_task());
            } else if self.match_keyword("PROGRAM") {
                program_instances.push(self.parse_program_instance());
            } else if self.is_var_block_start() {
                var_blocks.push(self.parse_var_block());
            } else {
                self.advance();
            }
        }

        Resource {
            name,
            var_blocks,
            tasks,
            program_instances,
        }
    }

    fn parse_task(&mut self) -> Task {
        let name = self
            .expect_identifier("expected task name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let mut single = None;
        let mut interval = None;
        let mut priority = None;

        if self.match_symbol(Symbol::LParen) {
            while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
                let param = self
                    .expect_identifier("expected task parameter")
                    .unwrap_or_else(|| Identifier::new("<error>"));
                self.expect_symbol(Symbol::Assign, "expected ':=' in task parameter");
                let value = self.parse_expression();
                match param.canonical.as_str() {
                    "SINGLE" => {
                        single = Some(value);
                    }
                    "INTERVAL" => {
                        interval = Some(value);
                    }
                    "PRIORITY" => {
                        priority = Some(value);
                    }
                    _ => {}
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after task parameters");
        }
        self.expect_symbol(Symbol::Semicolon, "expected ';' after TASK declaration");

        Task {
            name,
            single,
            interval,
            priority,
        }
    }

    fn parse_program_instance(&mut self) -> ProgramInstance {
        let name = self
            .expect_identifier("expected program instance name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let task = if self.match_keyword("WITH") {
            self.expect_identifier("expected task name after WITH")
        } else {
            None
        };
        self.expect_symbol(
            Symbol::Colon,
            "expected ':' in program instance declaration",
        );
        let program_type = self
            .expect_identifier("expected program type name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let args = if self.match_symbol(Symbol::LParen) {
            let args = self.parse_param_assignment_list(Symbol::RParen);
            self.expect_symbol(
                Symbol::RParen,
                "expected ')' after PROGRAM instance parameters",
            );
            args
        } else {
            Vec::new()
        };
        self.expect_symbol(
            Symbol::Semicolon,
            "expected ';' after PROGRAM instance declaration",
        );

        ProgramInstance {
            name,
            program_type,
            task,
            args,
        }
    }

    fn parse_statement_list(&mut self, stop_keywords: &[&str]) -> Vec<Statement> {
        let mut statements = Vec::new();
        while !self.is_eof() && !self.check_any_keyword(stop_keywords) {
            let statement = self.parse_statement();
            statements.push(statement);
            self.match_symbol(Symbol::Semicolon);
        }
        statements
    }

    fn parse_statement(&mut self) -> Statement {
        if self.match_symbol(Symbol::Semicolon) {
            return Statement::Empty;
        }

        if let Some(label) = self.current_identifier() {
            if self.peek_symbol(Symbol::Colon) {
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after IL label");
                return Statement::IlLabel(label);
            }
        }

        if self.current_il_op().is_some()
            && !self.peek_symbol(Symbol::Assign)
            && !self.peek_attached_lparen()
        {
            return self.parse_il_instruction();
        }

        if self.match_keyword("IF") {
            return self.parse_if_statement();
        }
        if self.match_keyword("CASE") {
            return self.parse_case_statement();
        }
        if self.match_keyword("FOR") {
            return self.parse_for_statement();
        }
        if self.match_keyword("WHILE") {
            let condition = self.parse_expression();
            self.expect_keyword("DO", "expected DO in WHILE statement");
            let body = self.parse_statement_list(&["END_WHILE"]);
            self.expect_keyword("END_WHILE", "expected END_WHILE");
            return Statement::While { condition, body };
        }
        if self.match_keyword("REPEAT") {
            let body = self.parse_statement_list(&["UNTIL"]);
            self.expect_keyword("UNTIL", "expected UNTIL in REPEAT statement");
            let until = self.parse_expression();
            self.expect_keyword("END_REPEAT", "expected END_REPEAT");
            return Statement::Repeat { body, until };
        }
        if self.match_keyword("EXIT") {
            return Statement::Exit;
        }
        if self.match_keyword("RETURN") {
            return Statement::Return;
        }

        if matches!(
            &self.current().kind,
            TokenKind::Ident(_) | TokenKind::DirectVariable(_)
        ) {
            let target = self.parse_variable_ref();
            if self.match_symbol(Symbol::Assign) {
                let value = self.parse_expression();
                return Statement::Assignment { target, value };
            }
            if self.match_symbol(Symbol::LParen) {
                let args = self.parse_param_assignment_list(Symbol::RParen);
                self.expect_symbol(Symbol::RParen, "expected ')' after function block call");
                return Statement::FbCall { name: target, args };
            }
        }

        let token = self.current().clone();
        self.error_at(
            &token,
            format!("unsupported or invalid statement '{}'", token.lexeme),
        );
        self.synchronize_to_semicolon();
        Statement::Unsupported(token.lexeme)
    }

    fn parse_il_instruction(&mut self) -> Statement {
        let token = self.current().clone();
        let Some(op) = self.current_il_op() else {
            self.error_at(&token, "expected IL operator");
            self.advance();
            return Statement::Unsupported(token.lexeme);
        };
        self.advance();

        let needs_operand = !matches!(op, IlOp::Not | IlOp::Ret | IlOp::Retc | IlOp::Retcn);
        let operand = if needs_operand && !self.is_eof() && !self.next_token_starts_statement() {
            Some(self.parse_il_operand())
        } else {
            None
        };

        Statement::Il { op, operand }
    }

    fn parse_il_operand(&mut self) -> Expr {
        if self.check_symbol(Symbol::LParen) && self.next_token_is_il_op() {
            self.parse_il_parenthesized_expression()
        } else {
            let stop = self.line_end_after(self.current().span.start);
            self.parse_expression_until(stop)
        }
    }

    fn parse_il_parenthesized_expression(&mut self) -> Expr {
        self.expect_symbol(
            Symbol::LParen,
            "expected '(' in IL parenthesized expression",
        );
        let mut accumulator = None;

        while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let token = self.current().clone();
            let Some(op) = self.current_il_op() else {
                self.error_at(
                    &token,
                    format!(
                        "expected IL instruction inside parenthesized expression, found '{}'",
                        token.lexeme
                    ),
                );
                self.synchronize_to_il_expression_boundary();
                continue;
            };
            self.advance();

            let operand = if il_op_needs_operand(op)
                && !self.check_symbol(Symbol::Semicolon)
                && !self.check_symbol(Symbol::RParen)
            {
                Some(self.parse_il_operand())
            } else {
                None
            };

            accumulator = self.fold_il_expression(accumulator, op, operand, &token);
            self.match_symbol(Symbol::Semicolon);
        }

        self.expect_symbol(
            Symbol::RParen,
            "expected ')' after IL parenthesized expression",
        );
        accumulator.unwrap_or_else(|| {
            let token = self.previous().clone();
            self.error_at(&token, "empty IL parenthesized expression");
            Expr::Literal(Literal::Int(0))
        })
    }

    fn fold_il_expression(
        &mut self,
        accumulator: Option<Expr>,
        op: IlOp,
        operand: Option<Expr>,
        token: &Token,
    ) -> Option<Expr> {
        match op {
            IlOp::Ld => self.il_required_operand(op, operand, token),
            IlOp::Ldn => self
                .il_required_operand(op, operand, token)
                .map(|expr| Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                }),
            IlOp::Not => accumulator.map(|expr| Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            }),
            IlOp::And | IlOp::Andn | IlOp::Or | IlOp::Orn | IlOp::Xor | IlOp::Xorn => {
                let left = self.il_required_accumulator(accumulator, token)?;
                let mut right = self.il_required_operand(op, operand, token)?;
                if matches!(op, IlOp::Andn | IlOp::Orn | IlOp::Xorn) {
                    right = Expr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(right),
                    };
                }
                let binary = match op {
                    IlOp::And | IlOp::Andn => BinaryOp::And,
                    IlOp::Or | IlOp::Orn => BinaryOp::Or,
                    IlOp::Xor | IlOp::Xorn => BinaryOp::Xor,
                    _ => unreachable!(),
                };
                Some(Expr::Binary {
                    op: binary,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            IlOp::Add
            | IlOp::Sub
            | IlOp::Mul
            | IlOp::Div
            | IlOp::Mod
            | IlOp::Gt
            | IlOp::Ge
            | IlOp::Eq
            | IlOp::Ne
            | IlOp::Le
            | IlOp::Lt => {
                let left = self.il_required_accumulator(accumulator, token)?;
                let right = self.il_required_operand(op, operand, token)?;
                let binary = match op {
                    IlOp::Add => BinaryOp::Add,
                    IlOp::Sub => BinaryOp::Sub,
                    IlOp::Mul => BinaryOp::Mul,
                    IlOp::Div => BinaryOp::Div,
                    IlOp::Mod => BinaryOp::Mod,
                    IlOp::Gt => BinaryOp::Greater,
                    IlOp::Ge => BinaryOp::GreaterEqual,
                    IlOp::Eq => BinaryOp::Equal,
                    IlOp::Ne => BinaryOp::NotEqual,
                    IlOp::Le => BinaryOp::LessEqual,
                    IlOp::Lt => BinaryOp::Less,
                    _ => unreachable!(),
                };
                Some(Expr::Binary {
                    op: binary,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            IlOp::St
            | IlOp::Stn
            | IlOp::S
            | IlOp::R
            | IlOp::Jmp
            | IlOp::Jmpc
            | IlOp::Jmpcn
            | IlOp::Cal
            | IlOp::Calc
            | IlOp::Calcn
            | IlOp::Ret
            | IlOp::Retc
            | IlOp::Retcn => {
                self.error_at(
                    token,
                    format!(
                        "IL {} instruction is not valid inside a parenthesized expression",
                        il_op_name(op)
                    ),
                );
                accumulator
            }
        }
    }

    fn il_required_accumulator(
        &mut self,
        accumulator: Option<Expr>,
        token: &Token,
    ) -> Option<Expr> {
        if accumulator.is_none() {
            self.error_at(
                token,
                "IL parenthesized expression operator requires a preceding accumulator",
            );
        }
        accumulator
    }

    fn il_required_operand(
        &mut self,
        op: IlOp,
        operand: Option<Expr>,
        token: &Token,
    ) -> Option<Expr> {
        if operand.is_none() {
            self.error_at(
                token,
                format!("IL {} instruction requires an operand", il_op_name(op)),
            );
        }
        operand
    }

    fn synchronize_to_il_expression_boundary(&mut self) {
        while !self.is_eof()
            && !self.check_symbol(Symbol::Semicolon)
            && !self.check_symbol(Symbol::RParen)
        {
            self.advance();
        }
        self.match_symbol(Symbol::Semicolon);
    }

    fn next_token_is_il_op(&self) -> bool {
        self.tokens
            .get(self.pos + 1)
            .and_then(|token| match &token.kind {
                TokenKind::Ident(value) => il_op_from_upper(&canonical_identifier(value)),
                _ => None,
            })
            .is_some()
    }

    fn current_il_op(&self) -> Option<IlOp> {
        let op = self.current_ident_upper()?;
        il_op_from_upper(&op)
    }
}

impl<'a> Parser<'a> {
    fn next_token_starts_statement(&self) -> bool {
        self.check_symbol(Symbol::Semicolon)
            || self.current_ident_upper().is_some_and(|keyword| {
                matches!(
                    keyword.as_str(),
                    "IF" | "CASE"
                        | "FOR"
                        | "WHILE"
                        | "REPEAT"
                        | "EXIT"
                        | "RETURN"
                        | "END_IF"
                        | "END_CASE"
                        | "END_FOR"
                        | "END_WHILE"
                        | "END_REPEAT"
                        | "END_PROGRAM"
                        | "END_FUNCTION"
                        | "END_FUNCTION_BLOCK"
                ) || self.current_il_op().is_some()
            })
    }

    fn parse_if_statement(&mut self) -> Statement {
        let mut branches = Vec::new();
        let first_condition = self.parse_expression();
        self.expect_keyword("THEN", "expected THEN in IF statement");
        let first_body = self.parse_statement_list(&["ELSIF", "ELSE", "END_IF"]);
        branches.push((first_condition, first_body));

        while self.match_keyword("ELSIF") {
            let condition = self.parse_expression();
            self.expect_keyword("THEN", "expected THEN after ELSIF");
            let body = self.parse_statement_list(&["ELSIF", "ELSE", "END_IF"]);
            branches.push((condition, body));
        }

        let else_branch = if self.match_keyword("ELSE") {
            self.parse_statement_list(&["END_IF"])
        } else {
            Vec::new()
        };
        self.expect_keyword("END_IF", "expected END_IF");

        Statement::If {
            branches,
            else_branch,
        }
    }

    fn parse_case_statement(&mut self) -> Statement {
        let selector = self.parse_expression();
        self.expect_keyword("OF", "expected OF in CASE statement");
        let mut cases = Vec::new();

        while !self.is_eof() && !self.check_any_keyword(&["ELSE", "END_CASE"]) {
            let mut labels = Vec::new();
            loop {
                let low = self.parse_expression();
                if self.match_symbol(Symbol::Range) {
                    let high = self.parse_expression();
                    labels.push(CaseLabel::Range(low, high));
                } else {
                    labels.push(CaseLabel::Single(low));
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::Colon, "expected ':' after CASE labels");
            let body = self.parse_case_body();
            cases.push((labels, body));
        }

        let else_branch = if self.match_keyword("ELSE") {
            self.parse_statement_list(&["END_CASE"])
        } else {
            Vec::new()
        };
        self.expect_keyword("END_CASE", "expected END_CASE");

        Statement::Case {
            selector,
            cases,
            else_branch,
        }
    }

    fn parse_case_body(&mut self) -> Vec<Statement> {
        let mut statements = Vec::new();
        while !self.is_eof()
            && !self.check_any_keyword(&["ELSE", "END_CASE"])
            && !self.current_starts_case_clause()
        {
            let statement = self.parse_statement();
            statements.push(statement);
            self.match_symbol(Symbol::Semicolon);
        }
        statements
    }

    fn current_starts_case_clause(&self) -> bool {
        let mut pos = self.pos;
        while pos < self.tokens.len() {
            match &self.tokens[pos].kind {
                TokenKind::Symbol(Symbol::Colon) => return true,
                TokenKind::Symbol(Symbol::Assign | Symbol::Semicolon) | TokenKind::Eof => {
                    return false;
                }
                TokenKind::Ident(value)
                    if matches!(
                        canonical_identifier(value).as_str(),
                        "ELSE"
                            | "END_CASE"
                            | "IF"
                            | "CASE"
                            | "FOR"
                            | "WHILE"
                            | "REPEAT"
                            | "EXIT"
                            | "RETURN"
                    ) =>
                {
                    return false;
                }
                _ => {
                    pos += 1;
                }
            }
        }
        false
    }

    fn parse_for_statement(&mut self) -> Statement {
        let control = self
            .expect_identifier("expected FOR control variable")
            .unwrap_or_else(|| Identifier::new("<error>"));
        self.expect_symbol(Symbol::Assign, "expected ':=' in FOR statement");
        let from = self.parse_expression();
        self.expect_keyword("TO", "expected TO in FOR statement");
        let to = self.parse_expression();
        let by = if self.match_keyword("BY") {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect_keyword("DO", "expected DO in FOR statement");
        let body = self.parse_statement_list(&["END_FOR"]);
        self.expect_keyword("END_FOR", "expected END_FOR");
        Statement::For {
            control,
            from,
            to,
            by,
            body,
        }
    }

    fn parse_expression(&mut self) -> Expr {
        if !self.enter_expression_depth() {
            return Expr::Literal(Literal::Int(0));
        }
        let expr = self.parse_or();
        self.leave_expression_depth();
        expr
    }

    fn parse_expression_until(&mut self, stop: usize) -> Expr {
        let previous = self.expression_stop;
        self.expression_stop = Some(previous.map_or(stop, |existing| existing.min(stop)));
        let expr = self.parse_expression();
        self.expression_stop = previous;
        expr
    }

    fn at_expression_stop(&self) -> bool {
        self.expression_stop
            .is_some_and(|stop| self.current().span.start >= stop)
    }

    fn line_end_after(&self, offset: usize) -> usize {
        self.source[offset..]
            .find('\n')
            .map(|relative| offset + relative)
            .unwrap_or(self.source.len())
    }

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_xor();
        while !self.at_expression_stop() && self.match_keyword("OR") {
            let right = self.parse_xor();
            expr = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_xor(&mut self) -> Expr {
        let mut expr = self.parse_and();
        while !self.at_expression_stop() && self.match_keyword("XOR") {
            let right = self.parse_and();
            expr = Expr::Binary {
                op: BinaryOp::Xor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_and(&mut self) -> Expr {
        let mut expr = self.parse_equality();
        while !self.at_expression_stop()
            && (self.match_keyword("AND") || self.match_symbol(Symbol::Amp))
        {
            let right = self.parse_equality();
            expr = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_equality(&mut self) -> Expr {
        let mut expr = self.parse_comparison();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Eq) {
                Some(BinaryOp::Equal)
            } else if self.match_symbol(Symbol::Ne) {
                Some(BinaryOp::NotEqual)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_comparison();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_add();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Lt) {
                Some(BinaryOp::Less)
            } else if self.match_symbol(Symbol::Le) {
                Some(BinaryOp::LessEqual)
            } else if self.match_symbol(Symbol::Gt) {
                Some(BinaryOp::Greater)
            } else if self.match_symbol(Symbol::Ge) {
                Some(BinaryOp::GreaterEqual)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_add();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_add(&mut self) -> Expr {
        let mut expr = self.parse_term();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Plus) {
                Some(BinaryOp::Add)
            } else if self.match_symbol(Symbol::Minus) {
                Some(BinaryOp::Sub)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_term();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_term(&mut self) -> Expr {
        let mut expr = self.parse_unary();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Star) {
                Some(BinaryOp::Mul)
            } else if self.match_symbol(Symbol::Slash) {
                Some(BinaryOp::Div)
            } else if self.match_keyword("MOD") {
                Some(BinaryOp::Mod)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_unary();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_power(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        if !self.at_expression_stop() && self.match_symbol(Symbol::Power) {
            let right = if self.enter_expression_depth() {
                let right = self.parse_unary();
                self.leave_expression_depth();
                right
            } else {
                Expr::Literal(Literal::Int(0))
            };
            expr = Expr::Binary {
                op: BinaryOp::Power,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_unary(&mut self) -> Expr {
        if self.match_symbol(Symbol::Plus) {
            return if self.enter_expression_depth() {
                let expr = self.parse_unary();
                self.leave_expression_depth();
                expr
            } else {
                Expr::Literal(Literal::Int(0))
            };
        }
        if self.match_symbol(Symbol::Minus) {
            return if self.enter_expression_depth() {
                let expr = self.parse_unary();
                self.leave_expression_depth();
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                }
            } else {
                Expr::Literal(Literal::Int(0))
            };
        }
        if self.match_keyword("NOT") {
            return if self.enter_expression_depth() {
                let expr = self.parse_unary();
                self.leave_expression_depth();
                Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                }
            } else {
                Expr::Literal(Literal::Int(0))
            };
        }
        self.parse_power()
    }

    fn parse_primary(&mut self) -> Expr {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(ref value) => {
                self.advance();
                self.parse_number_literal_token(&token, value)
            }
            TokenKind::StringLiteral(value) => {
                self.advance();
                Expr::Literal(Literal::String(value))
            }
            TokenKind::WStringLiteral(value) => {
                self.advance();
                Expr::Literal(Literal::WString(value))
            }
            TokenKind::HashLiteral(ref value) => {
                self.advance();
                Expr::Literal(self.parse_hash_literal_token(&token, value))
            }
            TokenKind::DirectVariable(_) => Expr::Variable(self.parse_variable_ref()),
            TokenKind::Ident(value) => {
                let upper = canonical_identifier(&value);
                if upper == "TRUE" || upper == "FALSE" {
                    self.advance();
                    return Expr::Literal(Literal::Bool(upper == "TRUE"));
                }

                self.advance();
                let ident = Identifier::new(value);
                if self.match_symbol(Symbol::LParen) {
                    let args = self.parse_param_assignment_list(Symbol::RParen);
                    self.expect_symbol(Symbol::RParen, "expected ')' after function call");
                    Expr::Call { name: ident, args }
                } else {
                    Expr::Variable(self.finish_variable_ref(ident))
                }
            }
            TokenKind::Symbol(Symbol::LParen) => {
                self.advance();
                if self
                    .current_identifier()
                    .is_some_and(|_| self.peek_symbol(Symbol::Assign))
                {
                    return self.parse_struct_literal();
                }
                let expr = self.parse_expression();
                self.expect_symbol(Symbol::RParen, "expected ')' after expression");
                expr
            }
            TokenKind::Symbol(Symbol::LBracket) => self.parse_array_literal(),
            _ => {
                self.error_at(
                    &token,
                    format!("expected expression, found '{}'", token.lexeme),
                );
                self.advance();
                Expr::Literal(Literal::Int(0))
            }
        }
    }

    fn parse_array_literal(&mut self) -> Expr {
        self.expect_symbol(Symbol::LBracket, "expected '[' in array literal");
        let mut elements = Vec::new();
        if self.check_symbol(Symbol::RBracket) {
            self.advance();
            return Expr::ArrayLiteral(elements);
        }
        loop {
            self.parse_array_literal_element(&mut elements);
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }
        self.expect_symbol(Symbol::RBracket, "expected ']' after array literal");
        Expr::ArrayLiteral(elements)
    }

    fn parse_array_literal_element(&mut self, elements: &mut Vec<Expr>) {
        if matches!(self.current().kind, TokenKind::Number(_)) && self.peek_symbol(Symbol::LParen) {
            let count_token = self.current().clone();
            let count = self.expect_unsigned_integer("expected array repetition count");
            self.expect_symbol(Symbol::LParen, "expected '(' after array repetition count");
            let value = self.parse_expression();
            self.expect_symbol(Symbol::RParen, "expected ')' after array repetition value");
            self.extend_array_literal(elements, count, value, &count_token);
            return;
        }

        let expr = self.parse_expression();
        self.push_array_literal_element(elements, expr);
    }

    fn push_array_literal_element(&mut self, elements: &mut Vec<Expr>, expr: Expr) {
        let max = self.implementation.max_array_elements;
        if elements.len() >= max {
            let token = self.previous().clone();
            self.limit_error_at(
                &token,
                format!("array literal has more than {max} elements"),
            );
            return;
        }
        if elements.try_reserve(1).is_err() {
            let token = self.previous().clone();
            self.limit_error_at(&token, "array literal element storage exhausted");
            return;
        }
        elements.push(expr);
    }

    fn extend_array_literal(
        &mut self,
        elements: &mut Vec<Expr>,
        count: usize,
        value: Expr,
        count_token: &Token,
    ) {
        let max = self.implementation.max_array_elements;
        if count > max {
            self.limit_error_at(
                count_token,
                format!("array literal repetition count {count} exceeds maximum {max}"),
            );
            return;
        }
        let Some(new_len) = elements.len().checked_add(count) else {
            self.limit_error_at(
                count_token,
                format!("array literal repetition count {count} exceeds maximum {max}"),
            );
            return;
        };
        if new_len > max {
            self.limit_error_at(
                count_token,
                format!("array literal has {new_len} elements, exceeding maximum {max}"),
            );
            return;
        }
        if elements.try_reserve(count).is_err() {
            self.limit_error_at(count_token, "array literal element storage exhausted");
            return;
        }
        elements.extend((0..count).map(|_| value.clone()));
    }

    fn parse_struct_literal(&mut self) -> Expr {
        let mut fields = Vec::new();
        if self.check_symbol(Symbol::RParen) {
            self.advance();
            return Expr::StructLiteral(fields);
        }

        loop {
            let Some(name) = self.expect_identifier("expected structure initializer field name")
            else {
                self.synchronize_to_semicolon();
                break;
            };
            self.expect_symbol(Symbol::Assign, "expected ':=' in structure initializer");
            let expr = self.parse_expression();
            fields.push(ParamAssignment {
                name: Some(name),
                output: false,
                negated: false,
                expr: Some(expr),
                variable: None,
            });
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }

        self.expect_symbol(Symbol::RParen, "expected ')' after structure initializer");
        Expr::StructLiteral(fields)
    }

    fn parse_param_assignment_list(&mut self, end: Symbol) -> Vec<ParamAssignment> {
        let mut args = Vec::new();
        if self.check_symbol(end) {
            return args;
        }

        loop {
            let negated = self.match_keyword("NOT");
            if let Some(name) = self.current_identifier() {
                if self.peek_symbol(Symbol::Assign) {
                    self.advance();
                    self.advance();
                    let expr = self.parse_expression();
                    args.push(ParamAssignment {
                        name: Some(name),
                        output: false,
                        negated,
                        expr: Some(expr),
                        variable: None,
                    });
                } else if self.peek_symbol(Symbol::Arrow) {
                    self.advance();
                    self.advance();
                    let variable = self.parse_variable_ref();
                    args.push(ParamAssignment {
                        name: Some(name),
                        output: true,
                        negated,
                        expr: None,
                        variable: Some(variable),
                    });
                } else {
                    let expr = self.parse_expression();
                    args.push(ParamAssignment {
                        name: None,
                        output: false,
                        negated,
                        expr: Some(expr),
                        variable: None,
                    });
                }
            } else {
                let expr = self.parse_expression();
                args.push(ParamAssignment {
                    name: None,
                    output: false,
                    negated,
                    expr: Some(expr),
                    variable: None,
                });
            }

            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }
        args
    }

    fn parse_variable_ref(&mut self) -> VariableRef {
        let token = self.current().clone();
        match token.kind {
            TokenKind::DirectVariable(value) => {
                self.advance();
                VariableRef::direct(value)
            }
            TokenKind::Ident(value) => {
                self.advance();
                self.finish_variable_ref(Identifier::new(value))
            }
            _ => {
                self.error_at(&token, "expected variable reference");
                self.advance();
                VariableRef::named("<error>")
            }
        }
    }

    fn finish_variable_ref(&mut self, first: Identifier) -> VariableRef {
        let mut path = vec![first];
        let mut indices = vec![self.parse_index_suffix()];
        while self.match_symbol(Symbol::Dot) {
            if let Some(part) = self.expect_identifier("expected field name after '.'") {
                path.push(part);
                indices.push(self.parse_index_suffix());
            } else {
                break;
            }
        }
        VariableRef {
            path,
            indices,
            direct: None,
        }
    }

    fn parse_index_suffix(&mut self) -> Vec<Expr> {
        let mut indices = Vec::new();
        while self.match_symbol(Symbol::LBracket) {
            if self.check_symbol(Symbol::RBracket) {
                self.advance();
                continue;
            }
            loop {
                indices.push(self.parse_expression());
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RBracket, "expected ']' after array index");
        }
        indices
    }

    fn is_var_block_start(&self) -> bool {
        matches!(
            self.current_ident_upper().as_deref(),
            Some(
                "VAR"
                    | "VAR_INPUT"
                    | "VAR_OUTPUT"
                    | "VAR_IN_OUT"
                    | "VAR_EXTERNAL"
                    | "VAR_GLOBAL"
                    | "VAR_TEMP"
                    | "VAR_ACCESS"
                    | "VAR_CONFIG"
            )
        )
    }

    fn is_sfc_statement_start(&self) -> bool {
        matches!(
            self.current_ident_upper().as_deref(),
            Some("INITIAL_STEP" | "STEP" | "TRANSITION" | "ACTION")
        ) || self.is_labeled_sfc_step()
            || self.is_labeled_sfc_transition()
            || self.is_labeled_sfc_action()
    }

    fn is_labeled_sfc_step(&self) -> bool {
        self.current_identifier().is_some()
            && self.peek_symbol(Symbol::Colon)
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|token| match &token.kind {
                    TokenKind::Ident(value) => matches!(
                        canonical_identifier(value).as_str(),
                        "STEP" | "INITIAL_STEP"
                    ),
                    _ => false,
                })
    }

    fn is_labeled_sfc_transition(&self) -> bool {
        self.current_identifier().is_some()
            && self.peek_symbol(Symbol::Colon)
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|token| match &token.kind {
                    TokenKind::Ident(value) => canonical_identifier(value) == "TRANSITION",
                    _ => false,
                })
    }

    fn is_labeled_sfc_action(&self) -> bool {
        self.current_identifier().is_some()
            && self.peek_symbol(Symbol::Colon)
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|token| match &token.kind {
                    TokenKind::Ident(value) => canonical_identifier(value) == "ACTION",
                    _ => false,
                })
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn is_eof(&self) -> bool {
        matches!(&self.current().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_eof() {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn current_identifier(&self) -> Option<Identifier> {
        match &self.current().kind {
            TokenKind::Ident(value) => Some(Identifier::new(value.clone())),
            _ => None,
        }
    }

    fn current_ident_upper(&self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Ident(value) => Some(canonical_identifier(value)),
            _ => None,
        }
    }

    fn check_keyword(&self, keyword: &str) -> bool {
        self.current_ident_upper()
            .is_some_and(|value| value == keyword)
    }

    fn check_any_keyword(&self, keywords: &[&str]) -> bool {
        keywords.iter().any(|keyword| self.check_keyword(keyword))
    }

    fn match_keyword(&mut self, keyword: &str) -> bool {
        if self.check_keyword(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, keyword: &str, message: impl Into<String>) -> bool {
        if self.match_keyword(keyword) {
            true
        } else {
            let token = self.current().clone();
            self.error_at(&token, message);
            false
        }
    }

    fn check_symbol(&self, symbol: Symbol) -> bool {
        matches!(&self.current().kind, TokenKind::Symbol(current) if *current == symbol)
    }

    fn peek_symbol(&self, symbol: Symbol) -> bool {
        matches!(
            self.tokens.get(self.pos + 1).map(|token| &token.kind),
            Some(TokenKind::Symbol(current)) if *current == symbol
        )
    }

    fn peek_attached_lparen(&self) -> bool {
        matches!(
            self.tokens.get(self.pos + 1),
            Some(Token {
                kind: TokenKind::Symbol(Symbol::LParen),
                span,
                ..
            }) if self.current().span.end == span.start
        )
    }

    fn match_symbol(&mut self, symbol: Symbol) -> bool {
        if self.check_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: Symbol, message: impl Into<String>) -> bool {
        if self.match_symbol(symbol) {
            true
        } else {
            let token = self.current().clone();
            self.error_at(&token, message);
            false
        }
    }

    fn expect_identifier(&mut self, message: impl Into<String>) -> Option<Identifier> {
        match &self.current().kind {
            TokenKind::Ident(value) => {
                let value = Identifier::new(value.clone());
                self.advance();
                Some(value)
            }
            _ => {
                let token = self.current().clone();
                self.error_at(&token, message);
                None
            }
        }
    }

    fn expect_signed_integer(&mut self, message: impl Into<String>) -> i64 {
        let negative = self.match_symbol(Symbol::Minus);
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(ref value) => {
                self.advance();
                let parsed = value.replace('_', "").parse::<i64>().unwrap_or_else(|_| {
                    self.error_at(&token, "invalid integer literal");
                    0
                });
                if negative {
                    -parsed
                } else {
                    parsed
                }
            }
            _ => {
                self.error_at(&token, message);
                0
            }
        }
    }

    fn expect_unsigned_integer(&mut self, message: impl Into<String>) -> usize {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(ref value) => {
                self.advance();
                value.replace('_', "").parse::<usize>().unwrap_or_else(|_| {
                    self.error_at(&token, "invalid unsigned integer literal");
                    0
                })
            }
            _ => {
                self.error_at(&token, message);
                0
            }
        }
    }

    fn synchronize_to_semicolon(&mut self) {
        while !self.is_eof() && !self.check_symbol(Symbol::Semicolon) {
            self.advance();
        }
        self.match_symbol(Symbol::Semicolon);
    }

    fn synchronize_to_keyword(&mut self, keyword: &str) {
        while !self.is_eof() && !self.check_keyword(keyword) {
            self.advance();
        }
    }

    fn enter_expression_depth(&mut self) -> bool {
        let max = self.implementation.max_expression_depth;
        if self.expression_depth >= max {
            let token = self.current().clone();
            self.limit_error_at(&token, format!("expression depth exceeds maximum {max}"));
            if !self.is_eof() {
                self.advance();
            }
            return false;
        }
        self.expression_depth += 1;
        true
    }

    fn leave_expression_depth(&mut self) {
        self.expression_depth = self.expression_depth.saturating_sub(1);
    }

    fn error_at(&mut self, token: &Token, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Syntax,
            message,
            Some(token.span.clone()),
        ));
    }

    fn limit_error_at(&mut self, token: &Token, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Compliance,
            message,
            Some(token.span.clone()),
        ));
    }

    fn parse_number_literal_token(&mut self, token: &Token, raw: &str) -> Expr {
        let (expr, diagnostics) = parse_number_literal_checked(raw);
        for message in diagnostics {
            self.error_at(token, message);
        }
        expr
    }

    fn parse_hash_literal_token(&mut self, token: &Token, raw: &str) -> Literal {
        let (literal, diagnostics) = parse_hash_literal_checked(raw);
        for message in diagnostics {
            self.error_at(token, message);
        }
        literal
    }
}

#[derive(Debug, Clone, Copy)]
enum PouStart {
    Function,
    FunctionBlock,
    Program,
}

struct TextualLadderRung {
    label: Option<String>,
    condition: Expr,
    statements: Vec<Statement>,
    nodes: Vec<NetworkNode>,
}

struct TextualFbdNetwork {
    label: Option<String>,
    statements: Vec<Statement>,
    nodes: Vec<NetworkNode>,
}

fn textual_network_node(
    kind: impl Into<String>,
    index: usize,
    attributes: &[(&str, String)],
) -> NetworkNode {
    let mut map = BTreeMap::new();
    map.insert("localId".to_string(), index.to_string());
    for (name, value) in attributes {
        map.insert((*name).to_string(), value.clone());
    }
    NetworkNode {
        id: index.to_string(),
        kind: kind.into(),
        attributes: map,
    }
}

fn expr_variable_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Variable(variable) => Some(variable.to_string()),
        _ => None,
    }
}
