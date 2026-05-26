use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::EditionProfile;
use iec_syntax::parse_project;

pub const PLCOPEN_TC6_0201_NS: &str = "http://www.plcopen.org/xml/tc6_0201";

#[derive(Debug, Clone)]
pub struct PlcOpenImport {
    pub project: Project,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn import_plcopen_xml(source_name: &str, xml: &str) -> PlcOpenImport {
    let mut diagnostics = Vec::new();
    if !xml.contains(PLCOPEN_TC6_0201_NS) {
        diagnostics.push(Diagnostic::warning(
            DiagnosticCode::Compliance,
            "PLCopen XML namespace tc6_0201 was not found; importing best-effort",
            None,
        ));
    }

    let mut project = Project::new(EditionProfile::Iec61131_3_2003Strict);
    let mut offset = 0;
    while let Some(pou_start) = find_pou_start(&xml[offset..]) {
        let absolute = offset + pou_start;
        let Some(tag_end_rel) = xml[absolute..].find('>') else {
            break;
        };
        let tag_end = absolute + tag_end_rel;
        let tag = &xml[absolute..=tag_end];
        let name = attr(tag, "name").unwrap_or_else(|| "UnnamedPou".to_string());
        let pou_type = attr(tag, "pouType").unwrap_or_else(|| "program".to_string());
        let close = "</pou>";
        let Some(close_rel) = xml[tag_end + 1..].find(close) else {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Syntax,
                format!("PLCopen POU '{name}' is missing closing </pou>"),
                None,
            ));
            break;
        };
        let body_xml = &xml[tag_end + 1..tag_end + 1 + close_rel];
        offset = tag_end + 1 + close_rel + close.len();

        let kind = match pou_type.to_ascii_lowercase().as_str() {
            "function" => PouKind::Function {
                return_type: DataTypeSpec::Elementary(ElementaryType::Int),
            },
            "functionblock" | "function_block" => PouKind::FunctionBlock,
            _ => PouKind::Program,
        };

        let var_blocks = extract_tag(body_xml, "interface")
            .map(|interface_xml| parse_plcopen_var_blocks(&interface_xml))
            .unwrap_or_default();

        let body = if body_xml.contains("<LD") {
            let body = plcopen_graphical_body(
                source_name,
                ImplementationLanguage::LadderDiagram,
                body_xml,
                &mut diagnostics,
            );
            if body.statements.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::Unsupported,
                    format!(
                        "LD body for POU '{name}' imported as PLCopen network nodes without execution semantics"
                    ),
                    None,
                ));
            }
            body
        } else if body_xml.contains("<FBD") {
            let body = plcopen_graphical_body(
                source_name,
                ImplementationLanguage::FunctionBlockDiagram,
                body_xml,
                &mut diagnostics,
            );
            if body.statements.is_empty() {
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::Unsupported,
                    format!(
                        "FBD body for POU '{name}' imported as PLCopen network nodes without execution semantics"
                    ),
                    None,
                ));
            }
            body
        } else if body_xml.contains("<SFC") {
            PouBody {
                language: ImplementationLanguage::SequentialFunctionChart,
                statements: Vec::new(),
                networks: Vec::new(),
                sfc: Some(parse_sfc_body(source_name, body_xml, &mut diagnostics)),
            }
        } else if let Some(st_body) = extract_tag(body_xml, "ST") {
            let text = strip_xml_tags(&st_body);
            let wrapped = wrap_st_body(&name, &kind, &text);
            let parsed = parse_project(source_name, &wrapped);
            diagnostics.extend(parsed.diagnostics);
            parsed
                .project
                .find_pou(&name)
                .map(|pou| pou.body.clone())
                .unwrap_or_default()
        } else {
            PouBody::default()
        };

        project.library_elements.push(LibraryElement::Pou(Pou {
            name: Identifier::new(name),
            kind,
            var_blocks,
            body,
        }));
    }

    project.library_elements.extend(
        parse_plcopen_data_types(xml)
            .into_iter()
            .map(LibraryElement::DataType),
    );
    project.library_elements.extend(
        parse_plcopen_configurations(xml)
            .into_iter()
            .map(LibraryElement::Configuration),
    );

    PlcOpenImport {
        project,
        diagnostics,
    }
}

pub fn export_plcopen_xml(project: &Project) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<project xmlns=\"{}\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
        PLCOPEN_TC6_0201_NS
    ));
    out.push_str("  <fileHeader companyName=\"RoboC++\" productName=\"RoboC++\" productVersion=\"0.1.0\" />\n");
    out.push_str("  <contentHeader name=\"robocpp-project\" />\n");
    out.push_str("  <types>\n");
    out.push_str(&data_types_to_xml(project));
    out.push_str("    <pous>\n");
    for pou in project.pous() {
        let pou_type = match &pou.kind {
            PouKind::Function { .. } => "function",
            PouKind::FunctionBlock => "functionBlock",
            PouKind::Program => "program",
        };
        out.push_str(&format!(
            "      <pou name=\"{}\" pouType=\"{}\">\n",
            xml_escape(&pou.name.original),
            pou_type
        ));
        out.push_str("        <interface>\n");
        for block in &pou.var_blocks {
            out.push_str(&format!(
                "          <{}>\n",
                plcopen_var_block_name(block.kind)
            ));
            for var in &block.vars {
                out.push_str(&format!(
                    "            <variable name=\"{}\"><type><derived name=\"{}\" /></type></variable>\n",
                    xml_escape(&var.name.original),
                    xml_escape(&type_name_for_xml(&var.type_spec))
                ));
            }
            out.push_str(&format!(
                "          </{}>\n",
                plcopen_var_block_name(block.kind)
            ));
        }
        out.push_str("        </interface>\n");
        out.push_str("        <body>\n");
        match pou.body.language {
            ImplementationLanguage::StructuredText => {
                out.push_str("          <ST><xhtml:p>");
                out.push_str(&xml_escape(&statements_to_st(&pou.body.statements)));
                out.push_str("</xhtml:p></ST>\n");
            }
            ImplementationLanguage::LadderDiagram => {
                out.push_str(&graphical_networks_to_xml("LD", &pou.body.networks));
            }
            ImplementationLanguage::FunctionBlockDiagram => {
                out.push_str(&graphical_networks_to_xml("FBD", &pou.body.networks));
            }
            ImplementationLanguage::SequentialFunctionChart => {
                if let Some(sfc) = &pou.body.sfc {
                    out.push_str(&sfc_to_xml(sfc));
                } else {
                    out.push_str("          <SFC />\n");
                }
            }
            ImplementationLanguage::InstructionList => out.push_str("          <IL />\n"),
            ImplementationLanguage::External => out.push_str("          <ST />\n"),
        }
        out.push_str("        </body>\n");
        out.push_str("      </pou>\n");
    }
    out.push_str("    </pous>\n  </types>\n");
    out.push_str(&configurations_to_xml(project));
    out.push_str("</project>\n");
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(xml_unescape(&tag[start..start + end]))
}

fn find_pou_start(xml: &str) -> Option<usize> {
    let mut offset = 0;
    while let Some(index) = xml[offset..].find("<pou") {
        let absolute = offset + index;
        let next = xml[absolute + 4..].chars().next();
        if next.is_some_and(|ch| ch.is_whitespace() || ch == '>') {
            return Some(absolute);
        }
        offset = absolute + 4;
    }
    None
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = xml.find(&open)?;
    let open_end = xml[start..].find('>')? + start;
    let close = format!("</{tag}>");
    let close_start = xml[open_end + 1..].find(&close)? + open_end + 1;
    Some(xml[open_end + 1..close_start].to_string())
}

fn parse_plcopen_var_blocks(xml: &str) -> Vec<VarBlock> {
    [
        ("inputVars", VarBlockKind::Input),
        ("outputVars", VarBlockKind::Output),
        ("inOutVars", VarBlockKind::InOut),
        ("externalVars", VarBlockKind::External),
        ("globalVars", VarBlockKind::Global),
        ("tempVars", VarBlockKind::Temp),
        ("accessVars", VarBlockKind::Access),
        ("configVars", VarBlockKind::Config),
        ("localVars", VarBlockKind::Local),
    ]
    .into_iter()
    .filter_map(|(tag, kind)| {
        let vars = xml_elements(xml, tag)
            .into_iter()
            .flat_map(|(_, body)| parse_plcopen_variables(&body))
            .collect::<Vec<_>>();
        (!vars.is_empty()).then_some(VarBlock {
            kind,
            constant: false,
            retain: None,
            vars,
        })
    })
    .collect()
}

fn parse_plcopen_variables(xml: &str) -> Vec<VarDecl> {
    xml_elements(xml, "variable")
        .into_iter()
        .filter_map(|(tag, body)| {
            let name = attr(&tag, "name")?;
            let type_spec =
                parse_plcopen_type(&body).unwrap_or(DataTypeSpec::Elementary(ElementaryType::Bool));
            Some(VarDecl {
                name: Identifier::new(name),
                location: attr(&tag, "address").or_else(|| attr(&tag, "location")),
                type_spec,
                initial_value: None,
            })
        })
        .collect()
}

fn parse_plcopen_type(xml: &str) -> Option<DataTypeSpec> {
    if let Some((tag, _)) = xml_elements(xml, "derived").into_iter().next() {
        return attr(&tag, "name").map(type_spec_from_name);
    }
    if let Some((tag, _)) = xml_elements(xml, "string").into_iter().next() {
        return Some(DataTypeSpec::String {
            wide: false,
            length: attr(&tag, "length").and_then(|value| value.parse().ok()),
        });
    }
    if let Some((tag, _)) = xml_elements(xml, "wstring").into_iter().next() {
        return Some(DataTypeSpec::String {
            wide: true,
            length: attr(&tag, "length").and_then(|value| value.parse().ok()),
        });
    }
    [
        "BOOL",
        "SINT",
        "INT",
        "DINT",
        "LINT",
        "USINT",
        "UINT",
        "UDINT",
        "ULINT",
        "REAL",
        "LREAL",
        "BYTE",
        "WORD",
        "DWORD",
        "LWORD",
        "TIME",
        "DATE",
        "TIME_OF_DAY",
        "DATE_AND_TIME",
    ]
    .into_iter()
    .find(|name| find_element_start(xml, &name.to_ascii_lowercase()).is_some())
    .and_then(ElementaryType::parse)
    .map(DataTypeSpec::Elementary)
}

fn type_spec_from_name(name: String) -> DataTypeSpec {
    ElementaryType::parse(&name)
        .map(DataTypeSpec::Elementary)
        .unwrap_or_else(|| DataTypeSpec::Named(Identifier::new(name)))
}

fn parse_plcopen_data_types(xml: &str) -> Vec<DataTypeDeclaration> {
    let types_xml = extract_tag(xml, "types").unwrap_or_else(|| xml.to_string());
    let data_types_xml = extract_tag(&types_xml, "dataTypes").unwrap_or_default();
    xml_elements(&data_types_xml, "dataType")
        .into_iter()
        .filter_map(|(tag, body)| {
            let name = attr(&tag, "name")?;
            let spec = parse_plcopen_base_type(&body)?;
            Some(DataTypeDeclaration {
                name: Identifier::new(name),
                spec,
            })
        })
        .collect()
}

fn parse_plcopen_base_type(xml: &str) -> Option<DataTypeSpec> {
    let body = extract_tag(xml, "baseType").unwrap_or_else(|| xml.to_string());
    if let Some((tag, _)) = xml_elements(&body, "subrange").into_iter().next() {
        return Some(DataTypeSpec::Subrange {
            base: attr(&tag, "baseType")
                .and_then(|name| ElementaryType::parse(&name))
                .unwrap_or(ElementaryType::Int),
            range: Subrange {
                low: attr(&tag, "lower").and_then(|value| value.parse().ok())?,
                high: attr(&tag, "upper").and_then(|value| value.parse().ok())?,
            },
        });
    }
    if let Some((_, enum_body)) = xml_elements(&body, "enum").into_iter().next() {
        return Some(DataTypeSpec::Enum {
            values: xml_elements(&enum_body, "value")
                .into_iter()
                .filter_map(|(tag, _)| attr(&tag, "name"))
                .map(Identifier::new)
                .collect(),
        });
    }
    if let Some((_, struct_body)) = xml_elements(&body, "struct").into_iter().next() {
        return Some(DataTypeSpec::Struct {
            fields: parse_plcopen_variables(&struct_body)
                .into_iter()
                .map(|var| StructField {
                    name: var.name,
                    spec: var.type_spec,
                    initial_value: None,
                })
                .collect(),
        });
    }
    if let Some((_, array_body)) = xml_elements(&body, "array").into_iter().next() {
        let ranges = xml_elements(&array_body, "dimension")
            .into_iter()
            .filter_map(|(tag, _)| {
                Some(Subrange {
                    low: attr(&tag, "lower").and_then(|value| value.parse().ok())?,
                    high: attr(&tag, "upper").and_then(|value| value.parse().ok())?,
                })
            })
            .collect::<Vec<_>>();
        let element_type = extract_tag(&array_body, "elementType")
            .as_deref()
            .and_then(parse_plcopen_type)
            .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Int));
        return Some(DataTypeSpec::Array {
            ranges,
            element_type: Box::new(element_type),
        });
    }
    parse_plcopen_type(&body)
}

fn parse_plcopen_configurations(xml: &str) -> Vec<Configuration> {
    let instances_xml = extract_tag(xml, "instances").unwrap_or_default();
    let configurations_xml = extract_tag(&instances_xml, "configurations").unwrap_or(instances_xml);
    xml_elements(&configurations_xml, "configuration")
        .into_iter()
        .filter_map(|(tag, body)| {
            let name = attr(&tag, "name")?;
            let mut var_blocks = Vec::new();
            let mut resources = Vec::new();
            for (child_tag, child_body) in direct_child_elements(&body) {
                let tag_name = xml_tag_name(&child_tag).unwrap_or_default();
                if tag_name == "resource" {
                    if let Some(resource) = parse_plcopen_resource(&child_tag, &child_body) {
                        resources.push(resource);
                    }
                } else if let Some(kind) = plcopen_var_block_kind(&tag_name) {
                    let vars = parse_plcopen_variables(&child_body);
                    if !vars.is_empty() {
                        var_blocks.push(VarBlock {
                            kind,
                            constant: false,
                            retain: None,
                            vars,
                        });
                    }
                }
            }
            Some(Configuration {
                name: Identifier::new(name),
                var_blocks,
                resources,
            })
        })
        .collect()
}

fn parse_plcopen_resource(tag: &str, body: &str) -> Option<Resource> {
    let name = attr(tag, "name")?;
    let mut var_blocks = Vec::new();
    let mut tasks = Vec::new();
    let mut program_instances = Vec::new();
    for (child_tag, child_body) in direct_child_elements(body) {
        let tag_name = xml_tag_name(&child_tag).unwrap_or_default();
        match tag_name.as_str() {
            "task" => {
                if let Some(task) = parse_plcopen_task(&child_tag) {
                    tasks.push(task);
                }
            }
            "program" => {
                if let Some(program) = parse_plcopen_program_instance(&child_tag) {
                    program_instances.push(program);
                }
            }
            _ => {
                if let Some(kind) = plcopen_var_block_kind(&tag_name) {
                    let vars = parse_plcopen_variables(&child_body);
                    if !vars.is_empty() {
                        var_blocks.push(VarBlock {
                            kind,
                            constant: false,
                            retain: None,
                            vars,
                        });
                    }
                }
            }
        }
    }
    Some(Resource {
        name: Identifier::new(name),
        var_blocks,
        tasks,
        program_instances,
    })
}

fn parse_plcopen_task(tag: &str) -> Option<Task> {
    Some(Task {
        name: Identifier::new(attr(tag, "name")?),
        interval: attr(tag, "interval").map(|value| parse_plcopen_time_literal(&value)),
        priority: attr(tag, "priority").and_then(|value| value.parse().ok()),
    })
}

fn parse_plcopen_program_instance(tag: &str) -> Option<ProgramInstance> {
    Some(ProgramInstance {
        name: Identifier::new(attr(tag, "name")?),
        program_type: Identifier::new(attr(tag, "typeName")?),
        task: attr(tag, "task").map(Identifier::new),
    })
}

fn parse_plcopen_time_literal(value: &str) -> Literal {
    let mut diagnostics = Vec::new();
    parse_st_expression("plcopen.xml", value, &mut diagnostics)
        .and_then(|expr| {
            if let Expr::Literal(literal) = expr {
                Some(literal)
            } else {
                None
            }
        })
        .unwrap_or_else(|| Literal::Typed {
            type_name: Identifier::new("TIME"),
            value: value.to_string(),
        })
}

fn direct_child_elements(xml: &str) -> Vec<(String, String)> {
    let mut elements = Vec::new();
    let mut offset = 0;
    while let Some(start_rel) = find_next_element_start(&xml[offset..]) {
        let start = offset + start_rel;
        let Some(open_end_rel) = xml[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel;
        let open_tag = xml[start..=open_end].to_string();
        let Some(tag_name) = xml_tag_name(&open_tag) else {
            offset = open_end + 1;
            continue;
        };

        if open_tag.trim_end().ends_with("/>") {
            elements.push((open_tag, String::new()));
            offset = open_end + 1;
            continue;
        }

        let close = format!("</{tag_name}>");
        let Some(close_rel) = xml[open_end + 1..].find(&close) else {
            break;
        };
        let close_start = open_end + 1 + close_rel;
        elements.push((open_tag, xml[open_end + 1..close_start].to_string()));
        offset = close_start + close.len();
    }
    elements
}

fn find_next_element_start(xml: &str) -> Option<usize> {
    let mut offset = 0;
    while let Some(index) = xml[offset..].find('<') {
        let absolute = offset + index;
        let next = xml[absolute + 1..].chars().next();
        if next.is_some_and(|ch| ch != '/' && ch != '?' && ch != '!') {
            return Some(absolute);
        }
        offset = absolute + 1;
    }
    None
}

fn xml_tag_name(tag: &str) -> Option<String> {
    let name = tag
        .trim_start()
        .strip_prefix('<')?
        .trim_start_matches('/')
        .split_whitespace()
        .next()?
        .trim_end_matches('>')
        .trim_end_matches('/')
        .to_string();
    Some(name)
}

fn plcopen_var_block_kind(tag: &str) -> Option<VarBlockKind> {
    match tag {
        "inputVars" => Some(VarBlockKind::Input),
        "outputVars" => Some(VarBlockKind::Output),
        "inOutVars" => Some(VarBlockKind::InOut),
        "externalVars" => Some(VarBlockKind::External),
        "globalVars" => Some(VarBlockKind::Global),
        "tempVars" => Some(VarBlockKind::Temp),
        "accessVars" => Some(VarBlockKind::Access),
        "configVars" => Some(VarBlockKind::Config),
        "localVars" => Some(VarBlockKind::Local),
        _ => None,
    }
}

fn strip_xml_tags(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push('\n');
            }
            ch if !in_tag => out.push(ch),
            _ => {}
        }
    }
    xml_unescape(&out)
}

fn wrap_st_body(name: &str, kind: &PouKind, body: &str) -> String {
    match kind {
        PouKind::Function { .. } => format!("FUNCTION {name} : INT\n{body}\nEND_FUNCTION"),
        PouKind::FunctionBlock => format!("FUNCTION_BLOCK {name}\n{body}\nEND_FUNCTION_BLOCK"),
        PouKind::Program => format!("PROGRAM {name}\n{body}\nEND_PROGRAM"),
    }
}

fn plcopen_graphical_body(
    source_name: &str,
    language: ImplementationLanguage,
    xml: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> PouBody {
    let section = match language {
        ImplementationLanguage::LadderDiagram => "LD",
        ImplementationLanguage::FunctionBlockDiagram => "FBD",
        _ => "",
    };
    let graph_xml = extract_tag(xml, section).unwrap_or_else(|| xml.to_string());
    let node_kinds = match language {
        ImplementationLanguage::LadderDiagram => &[
            "leftPowerRail",
            "rightPowerRail",
            "contact",
            "coil",
            "block",
            "inVariable",
            "outVariable",
            "connector",
            "continuation",
        ][..],
        ImplementationLanguage::FunctionBlockDiagram => &[
            "block",
            "inVariable",
            "outVariable",
            "connector",
            "continuation",
        ][..],
        _ => &[][..],
    };
    let mut nodes = Vec::new();
    for kind in node_kinds {
        for (tag, body) in xml_elements(&graph_xml, kind) {
            let mut attributes = std::collections::BTreeMap::new();
            for attr_name in [
                "localId",
                "executionOrderId",
                "name",
                "typeName",
                "variable",
                "formalParameter",
                "negated",
                "edge",
                "storage",
            ] {
                if let Some(value) = attr(&tag, attr_name) {
                    attributes.insert(attr_name.to_string(), value);
                }
            }
            if let Some(expression) = extract_tag(&body, "expression")
                .map(|text| strip_xml_tags(&text).trim().to_string())
                .filter(|text| !text.is_empty())
            {
                attributes.insert("expression".to_string(), expression);
            }
            let connection_refs = connection_refs(&body);
            if !connection_refs.is_empty() {
                attributes.insert("connectionRefs".to_string(), connection_refs.join(","));
            }
            if *kind == "block" {
                let input_refs = block_input_refs(&body);
                if !input_refs.is_empty() {
                    attributes.insert(
                        "inputRefs".to_string(),
                        input_refs
                            .into_iter()
                            .map(|(formal, refs)| format!("{formal}={}", refs.join("|")))
                            .collect::<Vec<_>>()
                            .join(";"),
                    );
                }
            }
            nodes.push(NetworkNode {
                id: attributes
                    .get("localId")
                    .cloned()
                    .unwrap_or_else(|| format!("{}_{}", kind, nodes.len() + 1)),
                kind: (*kind).to_string(),
                attributes,
            });
        }
    }
    if nodes.is_empty() {
        let mut attributes = std::collections::BTreeMap::new();
        attributes.insert("raw_length".to_string(), xml.len().to_string());
        nodes.push(NetworkNode {
            id: "placeholder".to_string(),
            kind: "raw-plcopen-network".to_string(),
            attributes,
        });
    }

    let statements = match language {
        ImplementationLanguage::LadderDiagram => lower_ld_network(&nodes),
        ImplementationLanguage::FunctionBlockDiagram => {
            lower_fbd_network(source_name, &nodes, diagnostics)
        }
        _ => Vec::new(),
    };

    PouBody {
        language,
        statements,
        networks: vec![Network {
            label: None,
            language,
            nodes,
        }],
        sfc: None,
    }
}

fn lower_ld_network(nodes: &[NetworkNode]) -> Vec<Statement> {
    let Some(coil) = nodes.iter().find(|node| node.kind == "coil") else {
        return Vec::new();
    };
    let Some(coil_name) = coil.attributes.get("variable") else {
        return Vec::new();
    };
    let value = ld_node_expr(nodes, coil).or_else(|| {
        nodes
            .iter()
            .find(|node| node.kind == "contact")
            .and_then(ld_contact_expr)
    });
    let Some(value) = value else {
        return Vec::new();
    };

    vec![Statement::Assignment {
        target: VariableRef::named(coil_name.clone()),
        value,
    }]
}

fn ld_node_expr(nodes: &[NetworkNode], node: &NetworkNode) -> Option<Expr> {
    let refs = node_connection_refs(node);
    if node.kind == "coil" {
        return (!refs.is_empty())
            .then(|| expr_or_refs(nodes, &refs))
            .flatten();
    }

    if node.kind == "contact" {
        let contact = ld_contact_expr(node)?;
        return if refs.is_empty() {
            Some(contact)
        } else {
            Some(Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr_or_refs(nodes, &refs)?),
                right: Box::new(contact),
            })
        };
    }

    expr_or_refs(nodes, &refs)
}

fn expr_or_refs(nodes: &[NetworkNode], refs: &[String]) -> Option<Expr> {
    let mut exprs = refs
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .filter_map(|node| ld_node_expr(nodes, node))
        .collect::<Vec<_>>();
    let first = exprs
        .is_empty()
        .then(|| Expr::Literal(Literal::Bool(true)))
        .or_else(|| {
            let first = exprs.remove(0);
            Some(exprs.into_iter().fold(first, |left, right| Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            }))
        })?;
    Some(first)
}

fn ld_contact_expr(node: &NetworkNode) -> Option<Expr> {
    let variable = node.attributes.get("variable")?;
    let expr = Expr::Variable(VariableRef::named(variable.clone()));
    if node
        .attributes
        .get("negated")
        .is_some_and(|value| matches!(value.as_str(), "true" | "1"))
    {
        Some(Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(expr),
        })
    } else {
        Some(expr)
    }
}

fn lower_fbd_network(
    source_name: &str,
    nodes: &[NetworkNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Statement> {
    let Some(block) = nodes.iter().find(|node| node.kind == "block") else {
        return Vec::new();
    };
    let Some(function_name) = block.attributes.get("typeName") else {
        return Vec::new();
    };
    let mut args = fbd_block_input_exprs(source_name, nodes, block, diagnostics)
        .into_iter()
        .map(|(name, expr)| ParamAssignment {
            name: name.map(Identifier::new),
            output: false,
            negated: false,
            expr: Some(expr),
            variable: None,
        })
        .collect::<Vec<_>>();
    if args.is_empty() {
        args = nodes
            .iter()
            .filter(|node| node.kind == "inVariable")
            .filter_map(|node| node.attributes.get("expression"))
            .filter_map(|expression| parse_st_expression(source_name, expression, diagnostics))
            .map(|expr| ParamAssignment {
                name: None,
                output: false,
                negated: false,
                expr: Some(expr),
                variable: None,
            })
            .collect::<Vec<_>>();
    }
    let Some(output) = nodes
        .iter()
        .find(|node| {
            node.kind == "outVariable"
                && (node_connection_refs(node).is_empty()
                    || node_connection_refs(node).iter().any(|id| id == &block.id))
        })
        .or_else(|| nodes.iter().find(|node| node.kind == "outVariable"))
        .and_then(|node| node.attributes.get("expression"))
    else {
        return Vec::new();
    };
    vec![Statement::Assignment {
        target: VariableRef::named(output.clone()),
        value: Expr::Call {
            name: Identifier::new(function_name.clone()),
            args,
        },
    }]
}

fn fbd_block_input_exprs(
    source_name: &str,
    nodes: &[NetworkNode],
    block: &NetworkNode,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(Option<String>, Expr)> {
    parse_input_refs(block)
        .into_iter()
        .filter_map(|(formal, refs)| {
            refs.iter()
                .filter_map(|id| nodes.iter().find(|node| &node.id == id))
                .find_map(|node| fbd_node_expr(source_name, nodes, node, diagnostics))
                .map(|expr| (Some(formal), expr))
        })
        .collect()
}

fn fbd_node_expr(
    source_name: &str,
    nodes: &[NetworkNode],
    node: &NetworkNode,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    if let Some(expression) = node.attributes.get("expression") {
        return parse_st_expression(source_name, expression, diagnostics);
    }
    if node.kind == "block" {
        let function_name = node.attributes.get("typeName")?;
        let args = fbd_block_input_exprs(source_name, nodes, node, diagnostics)
            .into_iter()
            .map(|(name, expr)| ParamAssignment {
                name: name.map(Identifier::new),
                output: false,
                negated: false,
                expr: Some(expr),
                variable: None,
            })
            .collect();
        return Some(Expr::Call {
            name: Identifier::new(function_name.clone()),
            args,
        });
    }
    None
}

fn connection_refs(xml: &str) -> Vec<String> {
    xml_elements(xml, "connection")
        .into_iter()
        .filter_map(|(tag, _)| attr(&tag, "refLocalId"))
        .collect()
}

fn block_input_refs(xml: &str) -> Vec<(String, Vec<String>)> {
    xml_elements(xml, "variable")
        .into_iter()
        .filter_map(|(tag, body)| {
            let formal = attr(&tag, "formalParameter")?;
            let refs = connection_refs(&body);
            (!refs.is_empty()).then_some((formal, refs))
        })
        .collect()
}

fn node_connection_refs(node: &NetworkNode) -> Vec<String> {
    node.attributes
        .get("connectionRefs")
        .map(|refs| {
            refs.split(',')
                .filter(|part| !part.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_input_refs(node: &NetworkNode) -> Vec<(String, Vec<String>)> {
    node.attributes
        .get("inputRefs")
        .map(|refs| {
            refs.split(';')
                .filter_map(|binding| {
                    let (formal, ids) = binding.split_once('=')?;
                    let ids = ids
                        .split('|')
                        .filter(|id| !id.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    (!ids.is_empty()).then_some((formal.to_string(), ids))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn graphical_networks_to_xml(tag: &str, networks: &[Network]) -> String {
    let mut out = String::new();
    out.push_str(&format!("          <{tag}>\n"));
    for network in networks {
        for node in &network.nodes {
            if node.kind == "raw-plcopen-network" {
                continue;
            }
            out.push_str(&format!("            <{}", node.kind));
            for (name, value) in &node.attributes {
                if matches!(name.as_str(), "expression" | "connectionRefs" | "inputRefs") {
                    continue;
                }
                out.push_str(&format!(" {}=\"{}\"", name, xml_escape(value)));
            }
            let connection_refs = node_connection_refs(node);
            let input_refs = parse_input_refs(node);
            if node.attributes.contains_key("expression")
                || !connection_refs.is_empty()
                || !input_refs.is_empty()
            {
                out.push_str(">\n");
                if let Some(expression) = node.attributes.get("expression") {
                    out.push_str("              <expression>");
                    out.push_str(&xml_escape(expression));
                    out.push_str("</expression>\n");
                }
                if !connection_refs.is_empty() {
                    out.push_str("              <connectionPointIn>\n");
                    for id in connection_refs {
                        out.push_str(&format!(
                            "                <connection refLocalId=\"{}\" />\n",
                            xml_escape(&id)
                        ));
                    }
                    out.push_str("              </connectionPointIn>\n");
                }
                if !input_refs.is_empty() {
                    out.push_str("              <inputVariables>\n");
                    for (formal, refs) in input_refs {
                        out.push_str(&format!(
                            "                <variable formalParameter=\"{}\">\n",
                            xml_escape(&formal)
                        ));
                        out.push_str("                  <connectionPointIn>\n");
                        for id in refs {
                            out.push_str(&format!(
                                "                    <connection refLocalId=\"{}\" />\n",
                                xml_escape(&id)
                            ));
                        }
                        out.push_str("                  </connectionPointIn>\n");
                        out.push_str("                </variable>\n");
                    }
                    out.push_str("              </inputVariables>\n");
                }
                out.push_str(&format!("            </{}>\n", node.kind));
            } else {
                out.push_str(" />\n");
            }
        }
    }
    out.push_str(&format!("          </{tag}>\n"));
    out
}

fn parse_sfc_body(source_name: &str, body_xml: &str, diagnostics: &mut Vec<Diagnostic>) -> Sfc {
    let sfc_xml = extract_tag(body_xml, "SFC").unwrap_or_else(|| body_xml.to_string());
    let mut steps = Vec::new();
    let mut transitions = Vec::new();
    let mut actions = Vec::new();

    for (tag, _) in xml_elements(&sfc_xml, "step") {
        let name = attr(&tag, "name").unwrap_or_else(|| format!("Step{}", steps.len()));
        let initial = attr(&tag, "initialStep")
            .or_else(|| attr(&tag, "initial"))
            .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"));
        steps.push(SfcStep {
            name: Identifier::new(name),
            initial,
        });
    }

    for (tag, body) in xml_elements(&sfc_xml, "transition") {
        let name = attr(&tag, "name").map(Identifier::new);
        let condition = extract_tag(&body, "ST")
            .map(|st| strip_xml_tags(&st))
            .and_then(|text| parse_sfc_condition(source_name, &text, diagnostics));
        transitions.push(SfcTransition { name, condition });
    }

    for (tag, body) in xml_elements(&sfc_xml, "action") {
        let name = attr(&tag, "name").unwrap_or_else(|| format!("Action{}", actions.len()));
        let qualifier = attr(&tag, "qualifier")
            .and_then(|value| SfcActionQualifier::parse(&value))
            .unwrap_or(SfcActionQualifier::NonStored);
        let duration = attr(&tag, "duration").map(|value| parse_plcopen_time_literal(&value));
        let action_body = extract_tag(&body, "ST")
            .map(|st| strip_xml_tags(&st))
            .map(|text| parse_sfc_action_body(source_name, &name, &text, diagnostics))
            .unwrap_or_default();
        actions.push(SfcAction {
            name: Identifier::new(name),
            qualifier,
            duration,
            body: action_body,
        });
    }

    Sfc {
        steps,
        transitions,
        actions,
    }
}

fn parse_sfc_condition(
    source_name: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let condition = text.trim().trim_end_matches(';');
    if condition.is_empty() {
        return None;
    }
    parse_st_expression(source_name, condition, diagnostics)
}

fn parse_st_expression(
    source_name: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let wrapped = format!(
        "PROGRAM RbcppExpression\nVAR RbcppValue : INT; END_VAR\nRbcppValue := {text};\nEND_PROGRAM"
    );
    let parsed = parse_project(source_name, &wrapped);
    diagnostics.extend(parsed.diagnostics);
    parsed
        .project
        .first_program()
        .and_then(|pou| pou.body.statements.first())
        .and_then(|statement| {
            if let Statement::Assignment { value, .. } = statement {
                Some(value.clone())
            } else {
                None
            }
        })
}

fn parse_sfc_action_body(
    source_name: &str,
    action_name: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Statement> {
    let wrapped = format!("PROGRAM RbcppSfcAction{action_name}\n{text}\nEND_PROGRAM");
    let parsed = parse_project(source_name, &wrapped);
    diagnostics.extend(parsed.diagnostics);
    parsed
        .project
        .first_program()
        .map(|pou| pou.body.statements.clone())
        .unwrap_or_default()
}

fn xml_elements(xml: &str, tag: &str) -> Vec<(String, String)> {
    let mut elements = Vec::new();
    let mut offset = 0;
    while let Some(start_rel) = find_element_start(&xml[offset..], tag) {
        let start = offset + start_rel;
        let Some(open_end_rel) = xml[start..].find('>') else {
            break;
        };
        let open_end = start + open_end_rel;
        let open_tag = xml[start..=open_end].to_string();
        if open_tag.trim_end().ends_with("/>") {
            elements.push((open_tag, String::new()));
            offset = open_end + 1;
            continue;
        }

        let close = format!("</{tag}>");
        let Some(close_rel) = xml[open_end + 1..].find(&close) else {
            break;
        };
        let close_start = open_end + 1 + close_rel;
        elements.push((open_tag, xml[open_end + 1..close_start].to_string()));
        offset = close_start + close.len();
    }
    elements
}

fn find_element_start(xml: &str, tag: &str) -> Option<usize> {
    let open = format!("<{tag}");
    let mut offset = 0;
    while let Some(index) = xml[offset..].find(&open) {
        let absolute = offset + index;
        let next = xml[absolute + open.len()..].chars().next();
        if next.is_some_and(|ch| ch.is_whitespace() || ch == '>' || ch == '/') {
            return Some(absolute);
        }
        offset = absolute + open.len();
    }
    None
}

fn sfc_to_xml(sfc: &Sfc) -> String {
    let mut out = String::new();
    out.push_str("          <SFC>\n");
    for (index, step) in sfc.steps.iter().enumerate() {
        out.push_str(&format!(
            "            <step localId=\"{}\" name=\"{}\" initialStep=\"{}\" />\n",
            index + 1,
            xml_escape(&step.name.original),
            if step.initial { "true" } else { "false" }
        ));
    }
    for (index, transition) in sfc.transitions.iter().enumerate() {
        out.push_str(&format!(
            "            <transition localId=\"{}\"",
            index + 1 + sfc.steps.len()
        ));
        if let Some(name) = &transition.name {
            out.push_str(&format!(" name=\"{}\"", xml_escape(&name.original)));
        }
        out.push_str(">\n");
        if let Some(condition) = &transition.condition {
            out.push_str("              <condition><ST><xhtml:p>");
            out.push_str(&xml_escape(&expr_to_st(condition)));
            out.push_str("</xhtml:p></ST></condition>\n");
        }
        out.push_str("            </transition>\n");
    }
    for (index, action) in sfc.actions.iter().enumerate() {
        out.push_str(&format!(
            "            <action localId=\"{}\" name=\"{}\" qualifier=\"{}\"",
            index + 1 + sfc.steps.len() + sfc.transitions.len(),
            xml_escape(&action.name.original),
            action.qualifier.as_iec()
        ));
        if let Some(duration) = &action.duration {
            out.push_str(&format!(
                " duration=\"{}\"",
                xml_escape(&literal_to_st(duration))
            ));
        }
        out.push_str(">\n");
        out.push_str("              <ST><xhtml:p>");
        out.push_str(&xml_escape(&statements_to_st(&action.body)));
        out.push_str("</xhtml:p></ST>\n");
        out.push_str("            </action>\n");
    }
    out.push_str("          </SFC>\n");
    out
}

fn data_types_to_xml(project: &Project) -> String {
    let data_types = project.data_types().collect::<Vec<_>>();
    if data_types.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("    <dataTypes>\n");
    for data_type in data_types {
        out.push_str(&format!(
            "      <dataType name=\"{}\">\n        <baseType>\n",
            xml_escape(&data_type.name.original)
        ));
        out.push_str(&data_type_spec_to_xml(&data_type.spec, "          "));
        out.push_str("        </baseType>\n      </dataType>\n");
    }
    out.push_str("    </dataTypes>\n");
    out
}

fn data_type_spec_to_xml(spec: &DataTypeSpec, indent: &str) -> String {
    match spec {
        DataTypeSpec::Elementary(elementary) => {
            format!("{indent}<derived name=\"{}\" />\n", elementary.as_iec())
        }
        DataTypeSpec::Named(name) => {
            format!(
                "{indent}<derived name=\"{}\" />\n",
                xml_escape(&name.original)
            )
        }
        DataTypeSpec::Subrange { base, range } => format!(
            "{indent}<subrange baseType=\"{}\" lower=\"{}\" upper=\"{}\" />\n",
            base.as_iec(),
            range.low,
            range.high
        ),
        DataTypeSpec::Enum { values } => {
            let mut out = format!("{indent}<enum>\n");
            for value in values {
                out.push_str(&format!(
                    "{indent}  <value name=\"{}\" />\n",
                    xml_escape(&value.original)
                ));
            }
            out.push_str(&format!("{indent}</enum>\n"));
            out
        }
        DataTypeSpec::Struct { fields } => {
            let mut out = format!("{indent}<struct>\n");
            for field in fields {
                out.push_str(&format!(
                    "{indent}  <variable name=\"{}\"><type>",
                    xml_escape(&field.name.original)
                ));
                out.push_str(&type_ref_to_xml(&field.spec));
                out.push_str("</type></variable>\n");
            }
            out.push_str(&format!("{indent}</struct>\n"));
            out
        }
        DataTypeSpec::Array {
            ranges,
            element_type,
        } => {
            let mut out = format!("{indent}<array>\n");
            for range in ranges {
                out.push_str(&format!(
                    "{indent}  <dimension lower=\"{}\" upper=\"{}\" />\n",
                    range.low, range.high
                ));
            }
            out.push_str(&format!("{indent}  <elementType>"));
            out.push_str(&type_ref_to_xml(element_type));
            out.push_str("</elementType>\n");
            out.push_str(&format!("{indent}</array>\n"));
            out
        }
        DataTypeSpec::String { wide, length } => {
            let tag = if *wide { "wstring" } else { "string" };
            length
                .map(|length| format!("{indent}<{tag} length=\"{length}\" />\n"))
                .unwrap_or_else(|| format!("{indent}<{tag} />\n"))
        }
    }
}

fn type_ref_to_xml(spec: &DataTypeSpec) -> String {
    match spec {
        DataTypeSpec::String { wide, length } => {
            let tag = if *wide { "wstring" } else { "string" };
            length
                .map(|length| format!("<{tag} length=\"{length}\" />"))
                .unwrap_or_else(|| format!("<{tag} />"))
        }
        _ => format!(
            "<derived name=\"{}\" />",
            xml_escape(&type_name_for_xml(spec))
        ),
    }
}

fn configurations_to_xml(project: &Project) -> String {
    let configurations = project
        .library_elements
        .iter()
        .filter_map(|element| {
            if let LibraryElement::Configuration(configuration) = element {
                Some(configuration)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if configurations.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("  <instances>\n    <configurations>\n");
    for configuration in configurations {
        out.push_str(&format!(
            "      <configuration name=\"{}\">\n",
            xml_escape(&configuration.name.original)
        ));
        out.push_str(&var_blocks_to_xml(&configuration.var_blocks, "        "));
        for resource in &configuration.resources {
            out.push_str(&format!(
                "        <resource name=\"{}\">\n",
                xml_escape(&resource.name.original)
            ));
            out.push_str(&var_blocks_to_xml(&resource.var_blocks, "          "));
            for task in &resource.tasks {
                out.push_str(&format!(
                    "          <task name=\"{}\"",
                    xml_escape(&task.name.original)
                ));
                if let Some(interval) = &task.interval {
                    out.push_str(&format!(
                        " interval=\"{}\"",
                        xml_escape(&literal_to_st(interval))
                    ));
                }
                if let Some(priority) = task.priority {
                    out.push_str(&format!(" priority=\"{}\"", priority));
                }
                out.push_str(" />\n");
            }
            for program in &resource.program_instances {
                out.push_str(&format!(
                    "          <program name=\"{}\" typeName=\"{}\"",
                    xml_escape(&program.name.original),
                    xml_escape(&program.program_type.original)
                ));
                if let Some(task) = &program.task {
                    out.push_str(&format!(" task=\"{}\"", xml_escape(&task.original)));
                }
                out.push_str(" />\n");
            }
            out.push_str("        </resource>\n");
        }
        out.push_str("      </configuration>\n");
    }
    out.push_str("    </configurations>\n  </instances>\n");
    out
}

fn var_blocks_to_xml(var_blocks: &[VarBlock], indent: &str) -> String {
    let mut out = String::new();
    for block in var_blocks {
        out.push_str(&format!(
            "{indent}<{}>\n",
            plcopen_var_block_name(block.kind)
        ));
        for var in &block.vars {
            out.push_str(&format!(
                "{indent}  <variable name=\"{}\"",
                xml_escape(&var.name.original)
            ));
            if let Some(location) = &var.location {
                out.push_str(&format!(" address=\"{}\"", xml_escape(location)));
            }
            out.push_str(&format!(
                "><type><derived name=\"{}\" /></type></variable>\n",
                xml_escape(&type_name_for_xml(&var.type_spec))
            ));
        }
        out.push_str(&format!(
            "{indent}</{}>\n",
            plcopen_var_block_name(block.kind)
        ));
    }
    out
}

fn plcopen_var_block_name(kind: VarBlockKind) -> &'static str {
    match kind {
        VarBlockKind::Input => "inputVars",
        VarBlockKind::Output => "outputVars",
        VarBlockKind::InOut => "inOutVars",
        VarBlockKind::External => "externalVars",
        VarBlockKind::Global => "globalVars",
        VarBlockKind::Temp => "tempVars",
        VarBlockKind::Access => "accessVars",
        VarBlockKind::Config => "configVars",
        VarBlockKind::Local => "localVars",
    }
}

fn type_name_for_xml(spec: &DataTypeSpec) -> String {
    match spec {
        DataTypeSpec::Elementary(elementary) => elementary.as_iec().to_string(),
        DataTypeSpec::Named(name) => name.original.clone(),
        DataTypeSpec::String { wide, .. } => {
            if *wide {
                "WSTRING".to_string()
            } else {
                "STRING".to_string()
            }
        }
        DataTypeSpec::Subrange { base, .. } => base.as_iec().to_string(),
        DataTypeSpec::Array { .. } => "ARRAY".to_string(),
        DataTypeSpec::Struct { .. } => "STRUCT".to_string(),
        DataTypeSpec::Enum { .. } => "ENUM".to_string(),
    }
}

fn statements_to_st(statements: &[Statement]) -> String {
    statements
        .iter()
        .map(statement_to_st)
        .collect::<Vec<_>>()
        .join("\n")
}

fn statement_to_st(statement: &Statement) -> String {
    match statement {
        Statement::Empty => ";".to_string(),
        Statement::Assignment { target, value } => {
            format!("{} := {};", variable_to_st(target), expr_to_st(value))
        }
        Statement::FbCall { name, args } => {
            let args = args.iter().map(param_to_st).collect::<Vec<_>>().join(", ");
            format!("{}({});", variable_to_st(name), args)
        }
        Statement::If {
            branches,
            else_branch,
        } => {
            let mut out = String::new();
            for (index, (condition, body)) in branches.iter().enumerate() {
                if index == 0 {
                    out.push_str(&format!("IF {} THEN\n", expr_to_st(condition)));
                } else {
                    out.push_str(&format!("ELSIF {} THEN\n", expr_to_st(condition)));
                }
                out.push_str(&statements_to_st(body));
                out.push('\n');
            }
            if !else_branch.is_empty() {
                out.push_str("ELSE\n");
                out.push_str(&statements_to_st(else_branch));
                out.push('\n');
            }
            out.push_str("END_IF;");
            out
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => {
            let mut out = format!("CASE {} OF\n", expr_to_st(selector));
            for (labels, body) in cases {
                let labels = labels
                    .iter()
                    .map(case_label_to_st)
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!("{labels}:\n"));
                out.push_str(&statements_to_st(body));
                out.push('\n');
            }
            if !else_branch.is_empty() {
                out.push_str("ELSE\n");
                out.push_str(&statements_to_st(else_branch));
                out.push('\n');
            }
            out.push_str("END_CASE;");
            out
        }
        Statement::For {
            control,
            from,
            to,
            by,
            body,
        } => {
            let by = by
                .as_ref()
                .map(|expr| format!(" BY {}", expr_to_st(expr)))
                .unwrap_or_default();
            format!(
                "FOR {} := {} TO {}{} DO\n{}\nEND_FOR;",
                control.original,
                expr_to_st(from),
                expr_to_st(to),
                by,
                statements_to_st(body)
            )
        }
        Statement::While { condition, body } => {
            format!(
                "WHILE {} DO\n{}\nEND_WHILE;",
                expr_to_st(condition),
                statements_to_st(body)
            )
        }
        Statement::Repeat { body, until } => {
            format!(
                "REPEAT\n{}\nUNTIL {}\nEND_REPEAT;",
                statements_to_st(body),
                expr_to_st(until)
            )
        }
        Statement::Il { op, operand } => {
            let operand = operand
                .as_ref()
                .map(|expr| format!(" {}", expr_to_st(expr)))
                .unwrap_or_default();
            format!("{}{};", il_op_to_st(*op), operand)
        }
        Statement::IlLabel(label) => format!("{}:", label.original),
        Statement::Exit => "EXIT;".to_string(),
        Statement::Return => "RETURN;".to_string(),
        Statement::Unsupported(text) => format!("(* unsupported: {} *)", text.replace("*)", "")),
    }
}

fn il_op_to_st(op: IlOp) -> &'static str {
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

fn param_to_st(param: &ParamAssignment) -> String {
    if param.output {
        let name = param
            .name
            .as_ref()
            .map(|name| name.original.as_str())
            .unwrap_or("");
        let target = param
            .variable
            .as_ref()
            .map(variable_to_st)
            .unwrap_or_default();
        if param.negated {
            format!("NOT {name} => {target}")
        } else {
            format!("{name} => {target}")
        }
    } else if let Some(name) = &param.name {
        format!(
            "{} := {}",
            name.original,
            param
                .expr
                .as_ref()
                .map(expr_to_st)
                .unwrap_or_else(|| "0".to_string())
        )
    } else {
        param
            .expr
            .as_ref()
            .map(expr_to_st)
            .unwrap_or_else(|| "0".to_string())
    }
}

fn case_label_to_st(label: &CaseLabel) -> String {
    match label {
        CaseLabel::Single(expr) => expr_to_st(expr),
        CaseLabel::Range(low, high) => format!("{}..{}", expr_to_st(low), expr_to_st(high)),
    }
}

fn expr_to_st(expr: &Expr) -> String {
    match expr {
        Expr::Literal(literal) => literal_to_st(literal),
        Expr::Variable(variable) => variable_to_st(variable),
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => format!("-{}", expr_to_st(expr)),
            UnaryOp::Not => format!("NOT {}", expr_to_st(expr)),
        },
        Expr::Binary { op, left, right } => {
            format!(
                "({} {} {})",
                expr_to_st(left),
                binary_op_to_st(*op),
                expr_to_st(right)
            )
        }
        Expr::Call { name, args } => {
            let args = args.iter().map(param_to_st).collect::<Vec<_>>().join(", ");
            format!("{}({})", name.original, args)
        }
        Expr::ArrayLiteral(elements) => {
            let elements = elements
                .iter()
                .map(expr_to_st)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{elements}]")
        }
        Expr::StructLiteral(fields) => {
            let fields = fields
                .iter()
                .map(param_to_st)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({fields})")
        }
    }
}

fn literal_to_st(literal: &Literal) -> String {
    match literal {
        Literal::Int(value) => value.to_string(),
        Literal::Real(value) => value.to_string(),
        Literal::Bool(value) => {
            if *value {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Literal::String(value) => format!("'{}'", value.replace('\'', "$'")),
        Literal::DurationMs(value) => format!("T#{value}ms"),
        Literal::Date(value) => format!("DATE#{value}"),
        Literal::TimeOfDay(value) => format!("TOD#{value}"),
        Literal::DateAndTime(value) => format!("DT#{value}"),
        Literal::Typed { type_name, value } => format!("{}#{}", type_name.original, value),
    }
}

fn variable_to_st(variable: &VariableRef) -> String {
    if let Some(direct) = &variable.direct {
        direct.clone()
    } else {
        variable
            .path
            .iter()
            .map(|part| part.original.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }
}

fn binary_op_to_st(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "OR",
        BinaryOp::Xor => "XOR",
        BinaryOp::And => "AND",
        BinaryOp::Equal => "=",
        BinaryOp::NotEqual => "<>",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "MOD",
        BinaryOp::Power => "**",
    }
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_simple_plcopen_st() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Demo" pouType="program">
                  <body><ST><xhtml:p>A := 1;</xhtml:p></ST></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("test.xml", xml);
        assert_eq!(imported.project.pous().count(), 1);
    }

    #[test]
    fn exports_robocpp_plcopen_header() {
        let parsed = parse_project(
            "test.st",
            r#"
            PROGRAM Demo
            VAR A : INT := 1; END_VAR
            A := A + 1;
            END_PROGRAM
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("productName=\"RoboC++\""));
        assert!(xml.contains("contentHeader name=\"robocpp-project\""));
        assert!(xml.contains("pou name=\"Demo\""));
        let imported = import_plcopen_xml("roundtrip.xml", &xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        assert_eq!(imported.project.pous().count(), 1);
    }

    #[test]
    fn imports_pou_interface_var_blocks() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="InterfaceDemo" pouType="program">
                  <interface>
                    <inputVars>
                      <variable name="Start"><type><derived name="BOOL" /></type></variable>
                    </inputVars>
                    <outputVars>
                      <variable name="Count"><type><derived name="INT" /></type></variable>
                    </outputVars>
                  </interface>
                  <body><ST><xhtml:p>Count := Count + 1;</xhtml:p></ST></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("interface.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        assert_eq!(pou.var_blocks.len(), 2);
        assert_eq!(pou.var_blocks[0].kind, VarBlockKind::Input);
        assert_eq!(pou.var_blocks[0].vars[0].name.original, "Start");
        assert_eq!(pou.var_blocks[1].kind, VarBlockKind::Output);
        assert_eq!(
            pou.var_blocks[1].vars[0].type_spec,
            DataTypeSpec::Elementary(ElementaryType::Int)
        );
    }

    #[test]
    fn round_trips_configurations_resources_tasks_and_instances() {
        let parsed = parse_project(
            "config.st",
            r#"
            PROGRAM Controller
            VAR_INPUT Enable : BOOL; END_VAR
            END_PROGRAM

            CONFIGURATION Plant
              VAR_GLOBAL
                Shared AT %MW0 : INT;
              END_VAR
              RESOURCE Cpu ON PLC
                VAR_CONFIG
                  Slot AT %IW0 : INT;
                END_VAR
                TASK Fast(INTERVAL := T#10ms, PRIORITY := 2);
                PROGRAM Main WITH Fast : Controller;
              END_RESOURCE
            END_CONFIGURATION
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<instances>"));
        assert!(xml.contains("<configuration name=\"Plant\">"));
        assert!(xml.contains("<task name=\"Fast\" interval=\"T#10ms\" priority=\"2\" />"));
        assert!(xml.contains("<program name=\"Main\" typeName=\"Controller\" task=\"Fast\" />"));

        let imported = import_plcopen_xml("roundtrip-config.xml", &xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let configuration = imported
            .project
            .library_elements
            .iter()
            .find_map(|element| {
                if let LibraryElement::Configuration(configuration) = element {
                    Some(configuration)
                } else {
                    None
                }
            })
            .expect("configuration should be imported");
        assert_eq!(configuration.name.original, "Plant");
        assert_eq!(configuration.var_blocks[0].kind, VarBlockKind::Global);
        assert_eq!(
            configuration.var_blocks[0].vars[0].location.as_deref(),
            Some("%MW0")
        );
        let resource = &configuration.resources[0];
        assert_eq!(resource.name.original, "Cpu");
        assert_eq!(resource.var_blocks[0].kind, VarBlockKind::Config);
        assert_eq!(resource.tasks[0].name.original, "Fast");
        assert_eq!(resource.tasks[0].priority, Some(2));
        assert_eq!(
            resource.program_instances[0].program_type.original,
            "Controller"
        );
        assert_eq!(
            resource.program_instances[0]
                .task
                .as_ref()
                .map(|task| task.original.as_str()),
            Some("Fast")
        );
    }

    #[test]
    fn round_trips_user_data_types() {
        let parsed = parse_project(
            "types.st",
            r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                Pair : STRUCT
                    Low : Small;
                    Label : STRING[8];
                END_STRUCT;
                Buffer : ARRAY [1..3] OF Small;
            END_TYPE

            PROGRAM Demo
            VAR Value : Buffer; END_VAR
            END_PROGRAM
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<dataTypes>"));
        assert!(xml.contains("<subrange baseType=\"INT\" lower=\"0\" upper=\"10\" />"));
        assert!(xml.contains("<value name=\"Run\" />"));
        assert!(xml.contains("<string length=\"8\" />"));
        assert!(xml.contains("<dimension lower=\"1\" upper=\"3\" />"));

        let imported = import_plcopen_xml("types.xml", &xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let data_types = imported.project.data_types().collect::<Vec<_>>();
        assert_eq!(data_types.len(), 4);
        assert!(matches!(
            data_types
                .iter()
                .find(|data_type| data_type.name.original == "Small")
                .map(|data_type| &data_type.spec),
            Some(DataTypeSpec::Subrange {
                base: ElementaryType::Int,
                range: Subrange { low: 0, high: 10 }
            })
        ));
        assert!(matches!(
            data_types
                .iter()
                .find(|data_type| data_type.name.original == "Mode")
                .map(|data_type| &data_type.spec),
            Some(DataTypeSpec::Enum { values }) if values.len() == 3
        ));
        assert!(matches!(
            data_types
                .iter()
                .find(|data_type| data_type.name.original == "Pair")
                .map(|data_type| &data_type.spec),
            Some(DataTypeSpec::Struct { fields }) if fields.len() == 2
        ));
        assert!(matches!(
            data_types
                .iter()
                .find(|data_type| data_type.name.original == "Buffer")
                .map(|data_type| &data_type.spec),
            Some(DataTypeSpec::Array { ranges, .. }) if ranges == &vec![Subrange { low: 1, high: 3 }]
        ));
    }

    #[test]
    fn imports_and_exports_sfc_structure() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Sequence" pouType="program">
                  <body><SFC>
                    <step localId="1" name="Start" initialStep="true" />
                    <step localId="2" name="Run" />
                    <transition localId="3" name="Go">
                      <condition><ST><xhtml:p>Ready</xhtml:p></ST></condition>
                    </transition>
                    <action localId="4" name="DoRun" qualifier="L" duration="T#5ms">
                      <ST><xhtml:p>Count := Count + 1;</xhtml:p></ST>
                    </action>
                  </SFC></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("sfc.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        let sfc = pou.body.sfc.as_ref().expect("SFC should be imported");
        assert_eq!(sfc.steps.len(), 2);
        assert!(sfc.steps[0].initial);
        assert_eq!(sfc.transitions.len(), 1);
        assert!(sfc.transitions[0].condition.is_some());
        assert_eq!(sfc.actions.len(), 1);
        assert_eq!(sfc.actions[0].qualifier, SfcActionQualifier::TimeLimited);
        assert_eq!(sfc.actions[0].duration, Some(Literal::DurationMs(5)));

        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<SFC>"));
        assert!(exported.contains("name=\"Start\""));
        assert!(exported.contains("name=\"Go\""));
        assert!(exported.contains("name=\"DoRun\""));
        assert!(exported.contains("qualifier=\"L\""));
        assert!(exported.contains("duration=\"T#5ms\""));
    }

    #[test]
    fn preserves_ld_and_fbd_plcopen_nodes() {
        let ld_xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Ladder" pouType="program">
                  <body><LD>
                    <leftPowerRail localId="1" />
                    <contact localId="2" variable="Start" />
                    <coil localId="3" variable="Motor" />
                  </LD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("ld.xml", ld_xml);
        let pou = imported.project.first_program().unwrap();
        assert_eq!(pou.body.language, ImplementationLanguage::LadderDiagram);
        assert_eq!(pou.body.networks[0].nodes.len(), 3);
        assert!(matches!(
            pou.body.statements.first(),
            Some(Statement::Assignment { .. })
        ));
        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<LD>"));
        assert!(exported.contains("variable=\"Start\""));
        assert!(exported.contains("variable=\"Motor\""));

        let fbd_xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Blocks" pouType="program">
                  <body><FBD>
                    <inVariable localId="1"><expression>A</expression></inVariable>
                    <block localId="2" typeName="ADD" />
                    <outVariable localId="3"><expression>C</expression></outVariable>
                  </FBD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("fbd.xml", fbd_xml);
        let pou = imported.project.first_program().unwrap();
        assert_eq!(
            pou.body.language,
            ImplementationLanguage::FunctionBlockDiagram
        );
        assert_eq!(pou.body.networks[0].nodes.len(), 3);
        assert!(matches!(
            pou.body.statements.first(),
            Some(Statement::Assignment {
                value: Expr::Call { .. },
                ..
            })
        ));
        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<FBD>"));
        assert!(exported.contains("typeName=\"ADD\""));
        assert!(exported.contains("<expression>A</expression>"));
    }

    #[test]
    fn lowers_ld_power_flow_connections() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Ladder" pouType="program">
                  <body><LD>
                    <leftPowerRail localId="1" />
                    <contact localId="2" variable="Start">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                    </contact>
                    <contact localId="3" variable="Permissive" negated="true">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </contact>
                    <coil localId="4" variable="Motor">
                      <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                    </coil>
                  </LD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("ld-flow.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        let statement = pou.body.statements.first().expect("LD should lower");
        assert_eq!(
            statement_to_st(statement),
            "Motor := ((TRUE AND Start) AND NOT Permissive);"
        );
        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<connectionPointIn>"));
        assert!(exported.contains("refLocalId=\"3\""));
    }

    #[test]
    fn lowers_fbd_data_flow_connections() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Blocks" pouType="program">
                  <body><FBD>
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
                  </FBD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("fbd-flow.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        let statement = pou.body.statements.first().expect("FBD should lower");
        assert_eq!(statement_to_st(statement), "C := ADD(IN1 := A, IN2 := B);");
        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<inputVariables>"));
        assert!(exported.contains("formalParameter=\"IN1\""));
        assert!(exported.contains("refLocalId=\"3\""));
    }
}
