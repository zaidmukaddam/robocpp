// SPDX-License-Identifier: MIT OR Apache-2.0

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
    if let Some(file_header) = find_start_tag(xml, "fileHeader") {
        project
            .metadata
            .insert("plcopen.fileHeader".to_string(), file_header);
    }
    if let Some(content_header) = find_start_tag(xml, "contentHeader") {
        project
            .metadata
            .insert("plcopen.contentHeader".to_string(), content_header);
    }
    if let Some(add_data) = extract_tag(xml, "addData") {
        project
            .metadata
            .insert("plcopen.addData".to_string(), add_data);
    }
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

        let interface_xml = extract_tag(body_xml, "interface");
        let return_type = interface_xml
            .as_deref()
            .and_then(|xml| extract_tag(xml, "returnType"))
            .as_deref()
            .and_then(parse_plcopen_type)
            .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Int));
        let kind = match pou_type.to_ascii_lowercase().as_str() {
            "function" => PouKind::Function { return_type },
            "functionblock" | "function_block" => PouKind::FunctionBlock,
            _ => PouKind::Program,
        };

        let mut var_blocks = interface_xml
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
        add_graphical_helper_vars(&mut var_blocks, &body);

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

fn add_graphical_helper_vars(var_blocks: &mut Vec<VarBlock>, body: &PouBody) {
    let helpers = body
        .networks
        .iter()
        .flat_map(|network| network.nodes.iter())
        .filter(|node| node.kind == "contact")
        .filter_map(|node| {
            let block_type = ld_edge_block_type(node)?;
            Some((ld_edge_instance_name(&node.id), block_type.to_string()))
        })
        .collect::<Vec<_>>();
    if helpers.is_empty() {
        return;
    }

    let mut existing = var_blocks
        .iter()
        .flat_map(|block| block.vars.iter())
        .map(|var| var.name.canonical.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let new_vars = helpers
        .into_iter()
        .filter_map(|(name, block_type)| {
            let ident = Identifier::new(name);
            existing.insert(ident.canonical.clone()).then_some(VarDecl {
                name: ident,
                type_spec: DataTypeSpec::Named(Identifier::new(block_type)),
                initial_value: None,
                location: None,
                access: None,
                edge: None,
            })
        })
        .collect::<Vec<_>>();
    if new_vars.is_empty() {
        return;
    }

    if let Some(block) = var_blocks.iter_mut().find(|block| {
        block.kind == VarBlockKind::Local && !block.constant && block.retain.is_none()
    }) {
        block.vars.extend(new_vars);
    } else {
        var_blocks.push(VarBlock {
            kind: VarBlockKind::Local,
            constant: false,
            retain: None,
            vars: new_vars,
        });
    }
}

pub fn export_plcopen_xml(project: &Project) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<project xmlns=\"{}\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
        PLCOPEN_TC6_0201_NS
    ));
    if let Some(file_header) = project.metadata.get("plcopen.fileHeader") {
        out.push_str("  ");
        out.push_str(file_header.trim());
        out.push('\n');
    } else {
        out.push_str("  <fileHeader companyName=\"RoboC++\" productName=\"RoboC++\" productVersion=\"0.1.0\" />\n");
    }
    if let Some(content_header) = project.metadata.get("plcopen.contentHeader") {
        out.push_str("  ");
        out.push_str(content_header.trim());
        out.push('\n');
    } else {
        out.push_str("  <contentHeader name=\"robocpp-project\" />\n");
    }
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
        if let PouKind::Function { return_type } = &pou.kind {
            out.push_str("          <returnType>");
            out.push_str(&type_ref_to_xml(return_type));
            out.push_str("</returnType>\n");
        }
        out.push_str(&var_blocks_to_xml(&pou.var_blocks, "          "));
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
    if let Some(add_data) = project.metadata.get("plcopen.addData") {
        out.push_str("  <addData>\n");
        out.push_str(add_data.trim());
        out.push('\n');
        out.push_str("  </addData>\n");
    }
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

fn find_start_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let start = xml.find(&open)?;
    let end = xml[start..].find('>')? + start;
    Some(xml[start..=end].to_string())
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let start = find_element_start(xml, tag)?;
    let open_end = xml[start..].find('>')? + start;
    if xml[start..=open_end].trim_end().ends_with("/>") {
        return Some(String::new());
    }
    let close = format!("</{tag}>");
    let mut depth = 1_usize;
    let mut offset = open_end + 1;
    loop {
        let next_open = find_element_start(&xml[offset..], tag).map(|index| offset + index);
        let next_close = xml[offset..].find(&close).map(|index| offset + index)?;
        if next_open.is_some_and(|index| index < next_close) {
            let nested_start = next_open.unwrap();
            let nested_open_end = xml[nested_start..].find('>')? + nested_start;
            if !xml[nested_start..=nested_open_end]
                .trim_end()
                .ends_with("/>")
            {
                depth += 1;
            }
            offset = nested_open_end + 1;
        } else {
            depth -= 1;
            if depth == 0 {
                return Some(xml[open_end + 1..next_close].to_string());
            }
            offset = next_close + close.len();
        }
    }
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
            .flat_map(|(_, body)| {
                if kind == VarBlockKind::Access {
                    parse_plcopen_access_variables(&body)
                } else if kind == VarBlockKind::Config {
                    parse_plcopen_config_variables(&body)
                } else {
                    parse_plcopen_variables(&body)
                }
            })
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
                access: None,
                edge: None,
                type_spec,
                initial_value: parse_plcopen_initial_value(&body),
            })
        })
        .collect()
}

fn parse_plcopen_access_variables(xml: &str) -> Vec<VarDecl> {
    xml_elements(xml, "accessVariable")
        .into_iter()
        .filter_map(|(tag, body)| {
            let alias = attr(&tag, "alias")?;
            let type_spec =
                parse_plcopen_type(&body).unwrap_or(DataTypeSpec::Elementary(ElementaryType::Bool));
            let direction = match attr(&tag, "direction").as_deref() {
                Some("readWrite") => AccessDirection::ReadWrite,
                _ => AccessDirection::ReadOnly,
            };
            Some(VarDecl {
                name: Identifier::new(alias),
                location: None,
                access: Some(AccessSpec {
                    path: attr(&tag, "instancePathAndName")?,
                    direction,
                }),
                edge: None,
                type_spec,
                initial_value: None,
            })
        })
        .collect()
}

fn parse_plcopen_config_variables(xml: &str) -> Vec<VarDecl> {
    let vars = xml_elements(xml, "configVariable")
        .into_iter()
        .filter_map(|(tag, body)| {
            let name = attr(&tag, "instancePathAndName")?;
            let type_spec =
                parse_plcopen_type(&body).unwrap_or(DataTypeSpec::Elementary(ElementaryType::Bool));
            Some(VarDecl {
                name: Identifier::new(name),
                location: attr(&tag, "address"),
                access: None,
                edge: None,
                type_spec,
                initial_value: parse_plcopen_initial_value(&body),
            })
        })
        .collect::<Vec<_>>();
    if vars.is_empty() {
        parse_plcopen_variables(xml)
    } else {
        vars
    }
}

fn parse_plcopen_initial_value(xml: &str) -> Option<Expr> {
    let initial = extract_tag(xml, "initialValue")?;
    parse_plcopen_value(&initial)
}

fn parse_plcopen_value(xml: &str) -> Option<Expr> {
    if let Some((_, body)) = xml_elements(xml, "arrayValue").into_iter().next() {
        let mut elements = Vec::new();
        for (tag, value_body) in xml_elements(&body, "value") {
            let repeat = attr(&tag, "repetitionValue")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1);
            if let Some(value) = parse_plcopen_value(&value_body) {
                elements.extend((0..repeat).map(|_| value.clone()));
            }
        }
        return Some(Expr::ArrayLiteral(elements));
    }

    if let Some((_, body)) = xml_elements(xml, "structValue").into_iter().next() {
        let fields = xml_elements(&body, "value")
            .into_iter()
            .filter_map(|(tag, value_body)| {
                Some(ParamAssignment {
                    name: Some(Identifier::new(attr(&tag, "member")?)),
                    output: false,
                    negated: false,
                    expr: parse_plcopen_value(&value_body),
                    variable: None,
                })
            })
            .collect::<Vec<_>>();
        return Some(Expr::StructLiteral(fields));
    }

    if let Some((tag, _)) = xml_elements(xml, "simpleValue").into_iter().next() {
        let value = attr(&tag, "value")?;
        let mut diagnostics = Vec::new();
        return parse_st_expression("plcopen.xml", &value, &mut diagnostics);
    }

    None
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
        "TOD",
        "DT",
        "TIME_OF_DAY",
        "DATE_AND_TIME",
    ]
    .into_iter()
    .find(|name| {
        find_element_start(xml, name).is_some()
            || find_element_start(xml, &name.to_ascii_lowercase()).is_some()
    })
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
    let body = xml.to_string();
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
    if let Some((_, subrange_body)) = xml_elements(&body, "subrangeSigned")
        .into_iter()
        .chain(xml_elements(&body, "subrangeUnsigned"))
        .next()
    {
        let (range_tag, _) = xml_elements(&subrange_body, "range").into_iter().next()?;
        let base = extract_tag(&subrange_body, "baseType")
            .as_deref()
            .and_then(parse_plcopen_type)
            .and_then(|spec| {
                if let DataTypeSpec::Elementary(elementary) = spec {
                    Some(elementary)
                } else {
                    None
                }
            })
            .unwrap_or(ElementaryType::Int);
        return Some(DataTypeSpec::Subrange {
            base,
            range: Subrange {
                low: attr(&range_tag, "lower").and_then(|value| value.parse().ok())?,
                high: attr(&range_tag, "upper").and_then(|value| value.parse().ok())?,
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
        let element_type = extract_tag(&array_body, "baseType")
            .or_else(|| extract_tag(&array_body, "elementType"))
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
                    let vars = if kind == VarBlockKind::Access {
                        parse_plcopen_access_variables(&child_body)
                    } else if kind == VarBlockKind::Config {
                        parse_plcopen_config_variables(&child_body)
                    } else {
                        parse_plcopen_variables(&child_body)
                    };
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
                    let task_name = task.name.clone();
                    for (task_child_tag, task_child_body) in direct_child_elements(&child_body) {
                        let task_tag_name = xml_tag_name(&task_child_tag).unwrap_or_default();
                        if matches!(task_tag_name.as_str(), "pouInstance" | "program") {
                            if let Some(program) = parse_plcopen_program_instance(
                                &task_child_tag,
                                &task_child_body,
                                Some(&task_name),
                            ) {
                                program_instances.push(program);
                            }
                        }
                    }
                    tasks.push(task);
                }
            }
            "pouInstance" | "program" => {
                if let Some(program) = parse_plcopen_program_instance(&child_tag, &child_body, None)
                {
                    program_instances.push(program);
                }
            }
            _ => {
                if let Some(kind) = plcopen_var_block_kind(&tag_name) {
                    let vars = if kind == VarBlockKind::Access {
                        parse_plcopen_access_variables(&child_body)
                    } else if kind == VarBlockKind::Config {
                        parse_plcopen_config_variables(&child_body)
                    } else {
                        parse_plcopen_variables(&child_body)
                    };
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
    let mut diagnostics = Vec::new();
    Some(Task {
        name: Identifier::new(attr(tag, "name")?),
        single: attr(tag, "single")
            .and_then(|value| parse_st_expression("plcopen.xml", &value, &mut diagnostics)),
        interval: attr(tag, "interval")
            .map(|value| Expr::Literal(parse_plcopen_time_literal(&value))),
        priority: attr(tag, "priority").and_then(|value| {
            value
                .parse()
                .ok()
                .map(|value| Expr::Literal(Literal::Int(value)))
        }),
    })
}

fn parse_plcopen_program_instance(
    tag: &str,
    body: &str,
    task_override: Option<&Identifier>,
) -> Option<ProgramInstance> {
    Some(ProgramInstance {
        name: Identifier::new(attr(tag, "name")?),
        program_type: Identifier::new(attr(tag, "typeName")?),
        task: task_override
            .cloned()
            .or_else(|| attr(tag, "task").map(Identifier::new)),
        args: parse_plcopen_program_instance_args(body),
    })
}

fn parse_plcopen_program_instance_args(body: &str) -> Vec<ParamAssignment> {
    let add_data = extract_tag(body, "addData").unwrap_or_default();
    let sources = if add_data.is_empty() {
        vec![body.to_string()]
    } else {
        let data_sources = xml_elements(&add_data, "data")
            .into_iter()
            .filter(|(tag, _)| {
                attr(tag, "name").is_some_and(|name| {
                    matches!(
                        name.as_str(),
                        "RoboCpp.ProgramInstanceParameters"
                            | "RoboC++.ProgramInstanceParameters"
                            | "RoboCPP.ProgramInstanceParameters"
                    )
                })
            })
            .map(|(_, data_body)| data_body)
            .collect::<Vec<_>>();
        if data_sources.is_empty() {
            vec![add_data]
        } else {
            data_sources
        }
    };

    let mut diagnostics = Vec::new();
    let mut args = Vec::new();
    for source in sources {
        for (tag, _) in xml_elements(&source, "parameter") {
            let name = attr(&tag, "name")
                .or_else(|| attr(&tag, "formal"))
                .map(Identifier::new);
            let direction = attr(&tag, "direction").unwrap_or_else(|| "input".to_string());
            let negated = truthy_attr_text(attr(&tag, "negated").as_deref());
            if direction.eq_ignore_ascii_case("output") {
                let variable = attr(&tag, "target")
                    .or_else(|| attr(&tag, "variable"))
                    .and_then(|target| {
                        parse_st_expression("plcopen.xml", &target, &mut diagnostics)
                    })
                    .and_then(|expr| {
                        if let Expr::Variable(variable) = expr {
                            Some(variable)
                        } else {
                            None
                        }
                    });
                args.push(ParamAssignment {
                    name,
                    output: true,
                    negated,
                    expr: None,
                    variable,
                });
            } else {
                let expr = attr(&tag, "expression")
                    .or_else(|| attr(&tag, "value"))
                    .and_then(|expression| {
                        parse_st_expression("plcopen.xml", &expression, &mut diagnostics)
                    });
                args.push(ParamAssignment {
                    name,
                    output: false,
                    negated,
                    expr,
                    variable: None,
                });
            }
        }
    }
    args
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
                "width",
                "height",
            ] {
                if let Some(value) = attr(&tag, attr_name) {
                    attributes.insert(attr_name.to_string(), value);
                }
            }
            if let Some((position_tag, _)) = xml_elements(&body, "position").into_iter().next() {
                if let Some(x) = attr(&position_tag, "x") {
                    attributes.insert("positionX".to_string(), x);
                }
                if let Some(y) = attr(&position_tag, "y") {
                    attributes.insert("positionY".to_string(), y);
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
    let mut statements = nodes
        .iter()
        .filter(|node| node.kind == "contact" && ld_edge_block_type(node).is_some())
        .map(ld_edge_contact_call)
        .collect::<Vec<_>>();
    let mut coils = nodes
        .iter()
        .filter(|node| node.kind == "coil")
        .collect::<Vec<_>>();
    coils.sort_by_key(|node| graphical_order_key(node));

    statements.extend(
        coils
            .into_iter()
            .filter_map(|coil| {
                let coil_name = coil.attributes.get("variable")?;
                let mut visiting = Vec::new();
                let value = ld_node_expr(nodes, coil, &mut visiting).or_else(|| {
                    nodes
                        .iter()
                        .find(|node| node.kind == "contact")
                        .and_then(|node| {
                            let mut visiting = Vec::new();
                            ld_contact_expr(nodes, node, &mut visiting)
                        })
                })?;
                Some(ld_coil_statement(coil, coil_name, value))
            })
            .collect::<Vec<_>>(),
    );
    statements
}

fn ld_node_expr(
    nodes: &[NetworkNode],
    node: &NetworkNode,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    if visiting.iter().any(|id| id == &node.id) {
        return None;
    }
    visiting.push(node.id.clone());
    let refs = node_connection_refs(node);
    let result = if node.kind == "leftPowerRail" {
        Some(Expr::Literal(Literal::Bool(true)))
    } else if node.kind == "coil" || node.kind == "rightPowerRail" {
        (!refs.is_empty())
            .then(|| expr_or_refs_with_stack(nodes, &refs, visiting))
            .flatten()
    } else if node.kind == "contact" {
        ld_contact_expr(nodes, node, visiting)
    } else if node.kind == "continuation" && refs.is_empty() {
        node.attributes
            .get("name")
            .and_then(|name| {
                nodes.iter().find(|candidate| {
                    candidate.kind == "connector"
                        && candidate
                            .attributes
                            .get("name")
                            .is_some_and(|other| other == name)
                })
            })
            .and_then(|connector| ld_node_expr(nodes, connector, visiting))
    } else {
        expr_or_refs_with_stack(nodes, &refs, visiting)
    };
    visiting.pop();
    result
}

fn expr_or_refs_with_stack(
    nodes: &[NetworkNode],
    refs: &[String],
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    let mut exprs = refs
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .filter_map(|node| ld_node_expr(nodes, node, visiting))
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

fn ld_contact_expr(
    nodes: &[NetworkNode],
    node: &NetworkNode,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    let variable = node.attributes.get("variable")?;
    let mut expr = if ld_edge_block_type(node).is_some() {
        Expr::Variable(VariableRef {
            path: vec![
                Identifier::new(ld_edge_instance_name(&node.id)),
                Identifier::new("Q"),
            ],
            indices: vec![Vec::new(), Vec::new()],
            direct: None,
        })
    } else {
        Expr::Variable(VariableRef::named(variable.clone()))
    };
    if truthy_attr(node, "negated") {
        expr = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(expr),
        };
    }
    let refs = node_connection_refs(node);
    if refs.is_empty() {
        Some(expr)
    } else {
        Some(Expr::Binary {
            op: BinaryOp::And,
            left: Box::new(expr_or_refs_with_stack(nodes, &refs, visiting)?),
            right: Box::new(expr),
        })
    }
}

fn ld_edge_contact_call(node: &NetworkNode) -> Statement {
    let variable = node
        .attributes
        .get("variable")
        .cloned()
        .unwrap_or_else(|| "FALSE".to_string());
    Statement::FbCall {
        name: VariableRef::named(ld_edge_instance_name(&node.id)),
        args: vec![ParamAssignment {
            name: Some(Identifier::new("CLK")),
            output: false,
            negated: false,
            expr: Some(Expr::Variable(VariableRef::named(variable))),
            variable: None,
        }],
    }
}

fn ld_edge_block_type(node: &NetworkNode) -> Option<&'static str> {
    let edge = node.attributes.get("edge")?.to_ascii_lowercase();
    match edge.as_str() {
        "rising" | "positive" | "p" | "true" | "1" => Some("R_TRIG"),
        "falling" | "negative" | "n" => Some("F_TRIG"),
        _ => None,
    }
}

fn ld_edge_instance_name(local_id: &str) -> String {
    format!("rbcpp_ld_edge_{}", sanitize_identifier_fragment(local_id))
}

fn sanitize_identifier_fragment(input: &str) -> String {
    let mut out = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if out.is_empty() {
        out.push('x');
    }
    out
}

fn ld_coil_statement(coil: &NetworkNode, coil_name: &str, value: Expr) -> Statement {
    let target = VariableRef::named(coil_name.to_string());
    let mut value = value;
    if truthy_attr(coil, "negated") {
        value = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(value),
        };
    }

    match coil
        .attributes
        .get("storage")
        .map(|value| value.to_ascii_lowercase())
    {
        Some(storage) if matches!(storage.as_str(), "set" | "s" | "setstored") => Statement::If {
            branches: vec![(
                value,
                vec![Statement::Assignment {
                    target,
                    value: Expr::Literal(Literal::Bool(true)),
                }],
            )],
            else_branch: Vec::new(),
        },
        Some(storage) if matches!(storage.as_str(), "reset" | "r" | "resetstored") => {
            Statement::If {
                branches: vec![(
                    value,
                    vec![Statement::Assignment {
                        target,
                        value: Expr::Literal(Literal::Bool(false)),
                    }],
                )],
                else_branch: Vec::new(),
            }
        }
        _ => Statement::Assignment { target, value },
    }
}

fn lower_fbd_network(
    source_name: &str,
    nodes: &[NetworkNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Statement> {
    let mut outputs = nodes
        .iter()
        .filter(|node| node.kind == "outVariable")
        .collect::<Vec<_>>();
    outputs.sort_by_key(|node| graphical_order_key(node));

    outputs
        .into_iter()
        .filter_map(|output| {
            let target = output.attributes.get("expression")?.clone();
            let refs = node_connection_refs(output);
            let value = if refs.is_empty() {
                fbd_unwired_output_expr(source_name, nodes, diagnostics)
            } else {
                fbd_expr_from_refs(source_name, nodes, &refs, diagnostics, &mut Vec::new())
            }?;
            Some(Statement::Assignment {
                target: VariableRef::named(target),
                value,
            })
        })
        .collect()
}

fn fbd_unwired_output_expr(
    source_name: &str,
    nodes: &[NetworkNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let block = nodes.iter().find(|node| node.kind == "block")?;
    let function_name = block.attributes.get("typeName")?;
    let mut args =
        fbd_block_input_exprs_with_stack(source_name, nodes, block, diagnostics, &mut Vec::new())
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
            .collect();
    }
    Some(Expr::Call {
        name: Identifier::new(function_name.clone()),
        args,
    })
}

fn fbd_block_input_exprs_with_stack(
    source_name: &str,
    nodes: &[NetworkNode],
    block: &NetworkNode,
    diagnostics: &mut Vec<Diagnostic>,
    visiting: &mut Vec<String>,
) -> Vec<(Option<String>, Expr)> {
    parse_input_refs(block)
        .into_iter()
        .filter_map(|(formal, refs)| {
            fbd_expr_from_refs(source_name, nodes, &refs, diagnostics, visiting)
                .map(|expr| (Some(formal), expr))
        })
        .collect()
}

fn fbd_expr_from_refs(
    source_name: &str,
    nodes: &[NetworkNode],
    refs: &[String],
    diagnostics: &mut Vec<Diagnostic>,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    refs.iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .find_map(|node| fbd_node_expr_with_stack(source_name, nodes, node, diagnostics, visiting))
}

fn fbd_node_expr_with_stack(
    source_name: &str,
    nodes: &[NetworkNode],
    node: &NetworkNode,
    diagnostics: &mut Vec<Diagnostic>,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    if visiting.iter().any(|id| id == &node.id) {
        diagnostics.push(Diagnostic::warning(
            DiagnosticCode::Unsupported,
            format!(
                "FBD feedback path involving localId '{}' was preserved but not lowered",
                node.id
            ),
            None,
        ));
        return None;
    }
    visiting.push(node.id.clone());
    if let Some(expression) = node.attributes.get("expression") {
        let result = parse_st_expression(source_name, expression, diagnostics);
        visiting.pop();
        return result;
    }
    if node.kind == "block" {
        let Some(function_name) = node.attributes.get("typeName") else {
            visiting.pop();
            return None;
        };
        let args =
            fbd_block_input_exprs_with_stack(source_name, nodes, node, diagnostics, visiting)
                .into_iter()
                .map(|(name, expr)| ParamAssignment {
                    name: name.map(Identifier::new),
                    output: false,
                    negated: false,
                    expr: Some(expr),
                    variable: None,
                })
                .collect();
        visiting.pop();
        return Some(Expr::Call {
            name: Identifier::new(function_name.clone()),
            args,
        });
    }
    let refs = node_connection_refs(node);
    if node.kind == "continuation" && refs.is_empty() {
        let result = node
            .attributes
            .get("name")
            .and_then(|name| {
                nodes.iter().find(|candidate| {
                    candidate.kind == "connector"
                        && candidate
                            .attributes
                            .get("name")
                            .is_some_and(|other| other == name)
                })
            })
            .and_then(|connector| {
                fbd_node_expr_with_stack(source_name, nodes, connector, diagnostics, visiting)
            });
        visiting.pop();
        return result;
    }
    let result = fbd_expr_from_refs(source_name, nodes, &refs, diagnostics, visiting);
    visiting.pop();
    result
}

fn graphical_order_key(node: &NetworkNode) -> (i64, i64, String) {
    (
        node.attributes
            .get("executionOrderId")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(i64::MAX),
        node.id.parse::<i64>().unwrap_or(i64::MAX),
        node.id.clone(),
    )
}

fn truthy_attr(node: &NetworkNode, name: &str) -> bool {
    node.attributes
        .get(name)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"))
}

fn truthy_attr_text(value: Option<&str>) -> bool {
    value.is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"))
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
                if matches!(
                    name.as_str(),
                    "expression" | "connectionRefs" | "inputRefs" | "positionX" | "positionY"
                ) {
                    continue;
                }
                out.push_str(&format!(" {}=\"{}\"", name, xml_escape(value)));
            }
            let connection_refs = node_connection_refs(node);
            let input_refs = parse_input_refs(node);
            let position = node
                .attributes
                .get("positionX")
                .zip(node.attributes.get("positionY"));
            if node.attributes.contains_key("expression")
                || !connection_refs.is_empty()
                || !input_refs.is_empty()
                || position.is_some()
            {
                out.push_str(">\n");
                if let Some((x, y)) = position {
                    out.push_str(&format!(
                        "              <position x=\"{}\" y=\"{}\" />\n",
                        xml_escape(x),
                        xml_escape(y)
                    ));
                }
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
    let mut step_ids = std::collections::BTreeMap::new();
    let mut node_inputs = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut node_step_targets = std::collections::BTreeMap::<String, Identifier>::new();

    for (element, kind) in [
        ("step", SfcStepKind::Step),
        ("macroStep", SfcStepKind::MacroStep),
    ] {
        for (tag, body) in xml_elements(&sfc_xml, element) {
            let name = attr(&tag, "name").unwrap_or_else(|| format!("Step{}", steps.len()));
            let step_name = Identifier::new(&name);
            let local_id = attr(&tag, "localId").unwrap_or_else(|| (steps.len() + 1).to_string());
            let initial = attr(&tag, "initialStep")
                .or_else(|| attr(&tag, "initial"))
                .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"));
            let actions = extract_tag(&body, "actionBlock")
                .map(|action_block| {
                    xml_elements(&action_block, "action")
                        .into_iter()
                        .filter_map(|(action_tag, _)| {
                            let name = attr(&action_tag, "referenceName")
                                .or_else(|| attr(&action_tag, "name"))
                                .or_else(|| attr(&action_tag, "actionName"))?;
                            let qualifier = attr(&action_tag, "qualifier")
                                .and_then(|value| SfcActionQualifier::parse(&value))
                                .unwrap_or(SfcActionQualifier::NonStored);
                            let duration = attr(&action_tag, "duration")
                                .map(|value| parse_plcopen_time_literal(&value));
                            Some(SfcActionAssociation {
                                name: Identifier::new(name),
                                qualifier: Some(qualifier),
                                duration,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            step_ids.insert(local_id.clone(), step_name.clone());
            node_inputs.insert(local_id, connection_refs(&body));
            steps.push(SfcStep {
                name: step_name,
                initial,
                kind,
                actions,
            });
        }
    }

    let mut transition_ids = std::collections::BTreeMap::new();
    for (tag, body) in xml_elements(&sfc_xml, "transition") {
        let name = attr(&tag, "name").map(Identifier::new);
        let local_id = attr(&tag, "localId")
            .unwrap_or_else(|| (steps.len() + transitions.len() + 1).to_string());
        node_inputs.insert(local_id.clone(), connection_refs(&body));
        let condition = extract_tag(&body, "ST")
            .map(|st| strip_xml_tags(&st))
            .and_then(|text| parse_sfc_condition(source_name, &text, diagnostics));
        let priority = attr(&tag, "priority").and_then(|value| value.parse::<i64>().ok());
        transition_ids.insert(local_id, transitions.len());
        transitions.push(SfcTransition {
            name,
            from: Vec::new(),
            to: Vec::new(),
            condition,
            priority,
        });
    }
    for kind in [
        "selectionDivergence",
        "selectionConvergence",
        "simultaneousDivergence",
        "simultaneousConvergence",
    ] {
        for (tag, body) in xml_elements(&sfc_xml, kind) {
            if let Some(local_id) = attr(&tag, "localId") {
                node_inputs.insert(local_id, connection_refs(&body));
            }
        }
    }
    for jump_kind in ["jumpStep", "jump"] {
        for (tag, body) in xml_elements(&sfc_xml, jump_kind) {
            if let Some(local_id) = attr(&tag, "localId") {
                node_inputs.insert(local_id.clone(), connection_refs(&body));
                if let Some(target) = attr(&tag, "targetName")
                    .or_else(|| attr(&tag, "target"))
                    .or_else(|| attr(&tag, "targetNameRef"))
                {
                    node_step_targets.insert(local_id, Identifier::new(target));
                }
            }
        }
    }
    let node_outputs = sfc_node_outputs(&node_inputs);
    for (local_id, index) in &transition_ids {
        transitions[*index].from = collect_sfc_reachable_steps(
            local_id,
            &node_inputs,
            &node_outputs,
            &step_ids,
            &node_step_targets,
            true,
        );
        transitions[*index].to = collect_sfc_reachable_steps(
            local_id,
            &node_inputs,
            &node_outputs,
            &step_ids,
            &node_step_targets,
            false,
        );
    }

    for (tag, body) in xml_elements(&sfc_xml, "action") {
        if attr(&tag, "referenceName").is_some() || attr(&tag, "actionName").is_some() {
            continue;
        }
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

fn sfc_node_outputs(
    node_inputs: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut outputs = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (node_id, inputs) in node_inputs {
        for input in inputs {
            outputs
                .entry(input.clone())
                .or_default()
                .push(node_id.clone());
        }
    }
    outputs
}

fn collect_sfc_reachable_steps(
    start: &str,
    node_inputs: &std::collections::BTreeMap<String, Vec<String>>,
    node_outputs: &std::collections::BTreeMap<String, Vec<String>>,
    step_ids: &std::collections::BTreeMap<String, Identifier>,
    node_step_targets: &std::collections::BTreeMap<String, Identifier>,
    reverse: bool,
) -> Vec<Identifier> {
    fn visit(
        node_id: &str,
        node_inputs: &std::collections::BTreeMap<String, Vec<String>>,
        node_outputs: &std::collections::BTreeMap<String, Vec<String>>,
        step_ids: &std::collections::BTreeMap<String, Identifier>,
        node_step_targets: &std::collections::BTreeMap<String, Identifier>,
        reverse: bool,
        visited: &mut std::collections::BTreeSet<String>,
        steps: &mut Vec<Identifier>,
    ) {
        if !visited.insert(node_id.to_string()) {
            return;
        }
        let neighbors = if reverse {
            node_inputs.get(node_id)
        } else {
            node_outputs.get(node_id)
        };
        let Some(neighbors) = neighbors else { return };
        for neighbor in neighbors {
            if let Some(step) = step_ids
                .get(neighbor)
                .or_else(|| node_step_targets.get(neighbor))
            {
                if !steps
                    .iter()
                    .any(|existing| existing.canonical == step.canonical)
                {
                    steps.push(step.clone());
                }
            } else {
                visit(
                    neighbor,
                    node_inputs,
                    node_outputs,
                    step_ids,
                    node_step_targets,
                    reverse,
                    visited,
                    steps,
                );
            }
        }
    }

    let mut visited = std::collections::BTreeSet::new();
    let mut steps = Vec::new();
    visit(
        start,
        node_inputs,
        node_outputs,
        step_ids,
        node_step_targets,
        reverse,
        &mut visited,
        &mut steps,
    );
    steps
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
    let step_ids = sfc
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| (step.name.canonical.clone(), (index + 1).to_string()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let transition_ids = (0..sfc.transitions.len())
        .map(|index| (index + sfc.steps.len() + 1).to_string())
        .collect::<Vec<_>>();
    let mut step_incoming = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut step_outgoing = std::collections::BTreeSet::<String>::new();
    for (index, transition) in sfc.transitions.iter().enumerate() {
        let transition_id = transition_ids[index].clone();
        for from in &transition.from {
            step_outgoing.insert(from.canonical.clone());
        }
        for to in &transition.to {
            step_incoming
                .entry(to.canonical.clone())
                .or_default()
                .push(transition_id.clone());
        }
    }
    for (index, step) in sfc.steps.iter().enumerate() {
        let incoming_refs = step_incoming
            .get(&step.name.canonical)
            .cloned()
            .unwrap_or_default();
        let has_outgoing = step_outgoing.contains(&step.name.canonical);
        let step_tag = match step.kind {
            SfcStepKind::Step => "step",
            SfcStepKind::MacroStep => "macroStep",
        };
        out.push_str(&format!(
            "            <{} localId=\"{}\" name=\"{}\" initialStep=\"{}\"",
            step_tag,
            index + 1,
            xml_escape(&step.name.original),
            if step.initial { "true" } else { "false" }
        ));
        if step.actions.is_empty() && incoming_refs.is_empty() && !has_outgoing {
            out.push_str(" />\n");
        } else {
            out.push_str(">\n");
            if !incoming_refs.is_empty() {
                out.push_str("              <connectionPointIn>\n");
                for ref_id in incoming_refs {
                    out.push_str(&format!(
                        "                <connection refLocalId=\"{}\" />\n",
                        xml_escape(&ref_id)
                    ));
                }
                out.push_str("              </connectionPointIn>\n");
            }
            if has_outgoing {
                out.push_str("              <connectionPointOut />\n");
            }
            if !step.actions.is_empty() {
                out.push_str("              <actionBlock>\n");
                for (action_index, action) in step.actions.iter().enumerate() {
                    out.push_str(&format!(
                        "                <action localId=\"{}\" qualifier=\"{}\" referenceName=\"{}\"",
                        action_index + 1,
                        action
                            .qualifier
                            .unwrap_or(SfcActionQualifier::NonStored)
                            .as_iec(),
                        xml_escape(&action.name.original)
                    ));
                    if let Some(duration) = &action.duration {
                        out.push_str(&format!(
                            " duration=\"{}\"",
                            xml_escape(&literal_to_st(duration))
                        ));
                    }
                    out.push_str(" />\n");
                }
                out.push_str("              </actionBlock>\n");
            }
            out.push_str(&format!("            </{}>\n", step_tag));
        }
    }
    for (index, transition) in sfc.transitions.iter().enumerate() {
        out.push_str(&format!(
            "            <transition localId=\"{}\"",
            transition_ids[index]
        ));
        if let Some(name) = &transition.name {
            out.push_str(&format!(" name=\"{}\"", xml_escape(&name.original)));
        }
        if let Some(priority) = transition.priority {
            out.push_str(&format!(" priority=\"{priority}\""));
        }
        out.push_str(">\n");
        let from_refs = transition
            .from
            .iter()
            .filter_map(|step| step_ids.get(&step.canonical))
            .cloned()
            .collect::<Vec<_>>();
        if !from_refs.is_empty() {
            out.push_str("              <connectionPointIn>\n");
            for ref_id in from_refs {
                out.push_str(&format!(
                    "                <connection refLocalId=\"{}\" />\n",
                    xml_escape(&ref_id)
                ));
            }
            out.push_str("              </connectionPointIn>\n");
        }
        if !transition.to.is_empty() {
            out.push_str("              <connectionPointOut />\n");
        }
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
            format!("{indent}<{} />\n", plcopen_elementary_type_tag(elementary))
        }
        DataTypeSpec::Named(name) => {
            format!(
                "{indent}<derived name=\"{}\" />\n",
                xml_escape(&name.original)
            )
        }
        DataTypeSpec::Subrange { base, range } => {
            let tag = if matches!(
                base,
                ElementaryType::Usint
                    | ElementaryType::Uint
                    | ElementaryType::Udint
                    | ElementaryType::Ulint
            ) {
                "subrangeUnsigned"
            } else {
                "subrangeSigned"
            };
            format!(
                "{indent}<{tag}>\n{indent}  <range lower=\"{}\" upper=\"{}\" />\n{indent}  <baseType><{} /></baseType>\n{indent}</{tag}>\n",
                range.low,
                range.high,
                plcopen_elementary_type_tag(base)
            )
        }
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
            out.push_str(&format!("{indent}  <baseType>"));
            out.push_str(&type_ref_to_xml(element_type));
            out.push_str("</baseType>\n");
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
        DataTypeSpec::Elementary(elementary) => {
            format!("<{} />", plcopen_elementary_type_tag(elementary))
        }
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

fn plcopen_elementary_type_tag(elementary: &ElementaryType) -> &'static str {
    match elementary {
        ElementaryType::TimeOfDay => "TOD",
        ElementaryType::DateAndTime => "DT",
        _ => elementary.as_iec(),
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
                let task_programs = resource
                    .program_instances
                    .iter()
                    .filter(|program| {
                        program.task.as_ref().is_some_and(|program_task| {
                            program_task.canonical == task.name.canonical
                        })
                    })
                    .collect::<Vec<_>>();
                out.push_str(&format!(
                    "          <task name=\"{}\"",
                    xml_escape(&task.name.original)
                ));
                if let Some(interval) = &task.interval {
                    out.push_str(&format!(
                        " interval=\"{}\"",
                        xml_escape(&expr_to_st(interval))
                    ));
                }
                if let Some(single) = &task.single {
                    out.push_str(&format!(" single=\"{}\"", xml_escape(&expr_to_st(single))));
                }
                if let Some(priority) = &task.priority {
                    out.push_str(&format!(
                        " priority=\"{}\"",
                        xml_escape(&expr_to_st(priority))
                    ));
                }
                if task_programs.is_empty() {
                    out.push_str(" />\n");
                } else {
                    out.push_str(">\n");
                    for program in task_programs {
                        out.push_str(&program_instance_to_xml(program, "            "));
                    }
                    out.push_str("          </task>\n");
                }
            }
            for program in &resource.program_instances {
                if program.task.is_none() {
                    out.push_str(&program_instance_to_xml(program, "          "));
                }
            }
            out.push_str("        </resource>\n");
        }
        out.push_str("      </configuration>\n");
    }
    out.push_str("    </configurations>\n  </instances>\n");
    out
}

fn program_instance_to_xml(program: &ProgramInstance, indent: &str) -> String {
    if program.args.is_empty() {
        return format!(
            "{indent}<pouInstance name=\"{}\" typeName=\"{}\" />\n",
            xml_escape(&program.name.original),
            xml_escape(&program.program_type.original)
        );
    }

    let mut out = format!(
        "{indent}<pouInstance name=\"{}\" typeName=\"{}\">\n",
        xml_escape(&program.name.original),
        xml_escape(&program.program_type.original)
    );
    out.push_str(&format!("{indent}  <addData>\n"));
    out.push_str(&format!(
        "{indent}    <data name=\"RoboCpp.ProgramInstanceParameters\">\n"
    ));
    for arg in &program.args {
        out.push_str(&program_instance_parameter_to_xml(
            arg,
            &format!("{indent}      "),
        ));
    }
    out.push_str(&format!("{indent}    </data>\n"));
    out.push_str(&format!("{indent}  </addData>\n"));
    out.push_str(&format!("{indent}</pouInstance>\n"));
    out
}

fn program_instance_parameter_to_xml(arg: &ParamAssignment, indent: &str) -> String {
    let name = arg
        .name
        .as_ref()
        .map(|name| name.original.as_str())
        .unwrap_or("");
    if arg.output {
        let target = arg
            .variable
            .as_ref()
            .map(variable_to_st)
            .unwrap_or_default();
        let mut out = format!(
            "{indent}<parameter name=\"{}\" direction=\"output\" target=\"{}\"",
            xml_escape(name),
            xml_escape(&target)
        );
        if arg.negated {
            out.push_str(" negated=\"true\"");
        }
        out.push_str(" />\n");
        out
    } else {
        let expression = arg
            .expr
            .as_ref()
            .map(expr_to_st)
            .unwrap_or_else(|| "0".to_string());
        format!(
            "{indent}<parameter name=\"{}\" direction=\"input\" expression=\"{}\" />\n",
            xml_escape(name),
            xml_escape(&expression)
        )
    }
}

fn var_blocks_to_xml(var_blocks: &[VarBlock], indent: &str) -> String {
    let mut out = String::new();
    for block in var_blocks {
        if block.kind == VarBlockKind::Access {
            out.push_str(&access_var_block_to_xml(block, indent));
            continue;
        }
        if block.kind == VarBlockKind::Config {
            out.push_str(&config_var_block_to_xml(block, indent));
            continue;
        }
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
            out.push_str("><type>");
            out.push_str(&type_ref_to_xml(&var.type_spec));
            out.push_str("</type>");
            if let Some(initial_value) = &var.initial_value {
                out.push('\n');
                out.push_str(&initial_value_to_xml(
                    initial_value,
                    &format!("{indent}    "),
                ));
                out.push_str(&format!("{indent}  "));
            }
            out.push_str("</variable>\n");
        }
        out.push_str(&format!(
            "{indent}</{}>\n",
            plcopen_var_block_name(block.kind)
        ));
    }
    out
}

fn config_var_block_to_xml(block: &VarBlock, indent: &str) -> String {
    let mut out = format!("{indent}<configVars>\n");
    for var in &block.vars {
        out.push_str(&format!(
            "{indent}  <configVariable instancePathAndName=\"{}\"",
            xml_escape(&var.name.original)
        ));
        if let Some(location) = &var.location {
            out.push_str(&format!(" address=\"{}\"", xml_escape(location)));
        }
        out.push_str("><type>");
        out.push_str(&type_ref_to_xml(&var.type_spec));
        out.push_str("</type>");
        if let Some(initial_value) = &var.initial_value {
            out.push('\n');
            out.push_str(&initial_value_to_xml(
                initial_value,
                &format!("{indent}    "),
            ));
            out.push_str(&format!("{indent}  "));
        }
        out.push_str("</configVariable>\n");
    }
    out.push_str(&format!("{indent}</configVars>\n"));
    out
}

fn access_var_block_to_xml(block: &VarBlock, indent: &str) -> String {
    let mut out = format!("{indent}<accessVars>\n");
    for var in &block.vars {
        let Some(access) = &var.access else {
            continue;
        };
        let direction = match access.direction {
            AccessDirection::ReadOnly => "readOnly",
            AccessDirection::ReadWrite => "readWrite",
        };
        out.push_str(&format!(
            "{indent}  <accessVariable alias=\"{}\" instancePathAndName=\"{}\" direction=\"{}\"><type>",
            xml_escape(&var.name.original),
            xml_escape(&access.path),
            direction
        ));
        out.push_str(&type_ref_to_xml(&var.type_spec));
        out.push_str("</type></accessVariable>\n");
    }
    out.push_str(&format!("{indent}</accessVars>\n"));
    out
}

fn initial_value_to_xml(expr: &Expr, indent: &str) -> String {
    let mut out = format!("{indent}<initialValue>");
    match expr {
        Expr::ArrayLiteral(elements) => {
            out.push_str("<arrayValue>\n");
            for element in elements {
                out.push_str(&format!("{indent}  <value>\n"));
                out.push_str(&initial_value_body_to_xml(
                    element,
                    &format!("{indent}    "),
                ));
                out.push_str(&format!("{indent}  </value>\n"));
            }
            out.push_str(&format!("{indent}</arrayValue>"));
        }
        Expr::StructLiteral(fields) => {
            out.push_str("<structValue>\n");
            for field in fields {
                if let (Some(name), Some(expr)) = (&field.name, &field.expr) {
                    out.push_str(&format!(
                        "{indent}  <value member=\"{}\">\n",
                        xml_escape(&name.original)
                    ));
                    out.push_str(&initial_value_body_to_xml(expr, &format!("{indent}    ")));
                    out.push_str(&format!("{indent}  </value>\n"));
                }
            }
            out.push_str(&format!("{indent}</structValue>"));
        }
        _ => out.push_str(&simple_value_to_xml(expr)),
    }
    out.push_str("</initialValue>\n");
    out
}

fn initial_value_body_to_xml(expr: &Expr, indent: &str) -> String {
    match expr {
        Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => {
            initial_value_to_xml(expr, indent)
                .trim()
                .trim_start_matches("<initialValue>")
                .trim_end_matches("</initialValue>")
                .to_string()
                + "\n"
        }
        _ => format!("{indent}{}\n", simple_value_to_xml(expr)),
    }
}

fn simple_value_to_xml(expr: &Expr) -> String {
    format!(
        "<simpleValue value=\"{}\" />",
        xml_escape(&expr_to_st(expr))
    )
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
        Literal::WString(value) => format!("\"{}\"", value.replace('"', "$\"")),
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
    fn preserves_project_level_vendor_metadata() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <fileHeader companyName="RobotCo" productName="VendorSuite" productVersion="9.1" />
              <contentHeader name="robot-cell" />
              <types><pous>
                <pou name="Demo" pouType="program">
                  <body><ST><xhtml:p>A := 1;</xhtml:p></ST></body>
                </pou>
              </pous></types>
              <addData>
                <data name="RobotCo.MotionProfile" handleUnknown="preserve">
                  <RobotCoProfile axis="Arm1" />
                </data>
              </addData>
            </project>
        "#;
        let imported = import_plcopen_xml("vendor.xml", xml);
        assert_eq!(
            imported
                .project
                .metadata
                .get("plcopen.fileHeader")
                .map(String::as_str),
            Some("<fileHeader companyName=\"RobotCo\" productName=\"VendorSuite\" productVersion=\"9.1\" />")
        );
        assert!(imported
            .project
            .metadata
            .get("plcopen.addData")
            .is_some_and(|data| data.contains("RobotCo.MotionProfile")));

        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("companyName=\"RobotCo\""));
        assert!(exported.contains("contentHeader name=\"robot-cell\""));
        assert!(exported.contains("RobotCo.MotionProfile"));
        assert!(exported.contains("<RobotCoProfile axis=\"Arm1\" />"));
    }

    #[test]
    fn preserves_nested_vendor_add_data_payloads() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:vendor="urn:robotco:plcopen">
              <types><pous>
                <pou name="Demo" pouType="program">
                  <body><ST><xhtml:p>A := 1;</xhtml:p></ST></body>
                </pou>
              </pous></types>
              <addData>
                <data name="RobotCo.Outer" handleUnknown="preserve">
                  <vendor:Envelope revision="3">
                    <addData>
                      <data name="RobotCo.Inner">
                        <vendor:Flag enabled="true" />
                      </data>
                    </addData>
                  </vendor:Envelope>
                </data>
              </addData>
            </project>
        "#;
        let imported = import_plcopen_xml("nested-vendor.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let metadata = imported
            .project
            .metadata
            .get("plcopen.addData")
            .expect("addData metadata should be preserved");
        assert!(metadata.contains("RobotCo.Outer"));
        assert!(metadata.contains("RobotCo.Inner"));
        assert!(metadata.contains("<vendor:Flag enabled=\"true\" />"));

        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("RobotCo.Outer"));
        assert!(exported.contains("<addData>"));
        assert!(exported.contains("RobotCo.Inner"));
        assert!(exported.contains("<vendor:Flag enabled=\"true\" />"));

        let reimported = import_plcopen_xml("nested-vendor-roundtrip.xml", &exported);
        assert!(
            reimported.diagnostics.is_empty(),
            "{:?}",
            reimported.diagnostics
        );
        assert!(reimported
            .project
            .metadata
            .get("plcopen.addData")
            .is_some_and(|data| data.contains("RobotCo.Inner")
                && data.contains("<vendor:Flag enabled=\"true\" />")));
    }

    #[test]
    fn imports_pou_interface_var_blocks() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="InterfaceDemo" pouType="program">
                  <interface>
                    <inputVars>
                      <variable name="Start"><type><BOOL /></type></variable>
                    </inputVars>
                    <outputVars>
                      <variable name="Count"><type><INT /></type></variable>
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
                Trigger : BOOL;
              END_VAR
              RESOURCE Cpu ON PLC
                VAR_CONFIG
                  Slot AT %IW0 : INT;
                END_VAR
                TASK Fast(SINGLE := Trigger, INTERVAL := T#10ms, PRIORITY := 2);
                PROGRAM Main WITH Fast : Controller;
              END_RESOURCE
            END_CONFIGURATION
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<instances>"));
        assert!(xml.contains("<configuration name=\"Plant\">"));
        assert!(xml.contains(
            "<task name=\"Fast\" interval=\"T#10ms\" single=\"Trigger\" priority=\"2\">"
        ));
        assert!(xml.contains("<pouInstance name=\"Main\" typeName=\"Controller\" />"));
        assert!(xml.contains(
            "<configVariable instancePathAndName=\"Slot\" address=\"%IW0\"><type><INT /></type></configVariable>"
        ));
        assert!(xml.contains("</task>"));

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
        assert_eq!(resource.var_blocks[0].vars[0].name.original, "Slot");
        assert_eq!(resource.tasks[0].name.original, "Fast");
        assert!(matches!(resource.tasks[0].single, Some(Expr::Variable(_))));
        assert!(matches!(
            resource.tasks[0].priority,
            Some(Expr::Literal(Literal::Int(2)))
        ));
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
    fn imports_schema_task_nested_pou_instances() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <instances>
                <configurations>
                  <configuration name="Plant">
                    <resource name="Cpu">
                      <task name="Fast" interval="T#5ms" priority="1">
                        <pouInstance name="Main" typeName="Controller" />
                      </task>
                      <pouInstance name="Background" typeName="Monitor" />
                    </resource>
                  </configuration>
                </configurations>
              </instances>
            </project>
        "#;
        let imported = import_plcopen_xml("schema-config.xml", xml);
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
        let resource = &configuration.resources[0];
        assert_eq!(resource.program_instances.len(), 2);
        assert_eq!(resource.program_instances[0].name.original, "Main");
        assert_eq!(
            resource.program_instances[0]
                .task
                .as_ref()
                .map(|task| task.original.as_str()),
            Some("Fast")
        );
        assert_eq!(resource.program_instances[1].name.original, "Background");
        assert!(resource.program_instances[1].task.is_none());
    }

    #[test]
    fn round_trips_program_instance_parameters_through_add_data() {
        let parsed = parse_project(
            "program-instance-parameters.st",
            r#"
            PROGRAM Controller
            VAR_INPUT
                Enable : BOOL;
                Setpoint : INT;
            END_VAR
            VAR_OUTPUT
                Count : INT;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
              VAR_GLOBAL
                Observed : INT;
              END_VAR
              RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Controller(Enable := TRUE, Setpoint := ADD(2, 3), Count => Observed);
              END_RESOURCE
            END_CONFIGURATION
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<pouInstance name=\"Main\" typeName=\"Controller\">"));
        assert!(xml.contains("RoboCpp.ProgramInstanceParameters"));
        assert!(
            xml.contains("<parameter name=\"Enable\" direction=\"input\" expression=\"TRUE\" />")
        );
        assert!(xml.contains(
            "<parameter name=\"Setpoint\" direction=\"input\" expression=\"ADD(2, 3)\" />"
        ));
        assert!(
            xml.contains("<parameter name=\"Count\" direction=\"output\" target=\"Observed\" />")
        );

        let imported = import_plcopen_xml("program-instance-parameters.xml", &xml);
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
        let program = &configuration.resources[0].program_instances[0];
        assert_eq!(program.args.len(), 3);
        assert!(program.args.iter().any(|arg| {
            arg.name
                .as_ref()
                .is_some_and(|name| name.original == "Enable")
                && !arg.output
                && matches!(arg.expr, Some(Expr::Literal(Literal::Bool(true))))
        }));
        assert!(program.args.iter().any(|arg| {
            arg.name
                .as_ref()
                .is_some_and(|name| name.original == "Setpoint")
                && !arg.output
                && matches!(arg.expr, Some(Expr::Call { .. }))
        }));
        assert!(program.args.iter().any(|arg| {
            arg.name
                .as_ref()
                .is_some_and(|name| name.original == "Count")
                && arg.output
                && arg
                    .variable
                    .as_ref()
                    .is_some_and(|variable| variable.to_string() == "Observed")
        }));
    }

    #[test]
    fn round_trips_variable_initial_values() {
        let parsed = parse_project(
            "initial-values.st",
            r#"
            TYPE
                Pair : STRUCT
                    Count : INT;
                    Enabled : BOOL;
                END_STRUCT;
            END_TYPE

            PROGRAM InitDemo
            VAR
                Count : INT := 7;
                Enabled : BOOL := TRUE;
                Values : ARRAY [1..2] OF INT := [1, 2];
                State : Pair := (Count := 3, Enabled := FALSE);
            END_VAR
            END_PROGRAM
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<variable name=\"Count\"><type><INT /></type>"));
        assert!(xml.contains("<initialValue><simpleValue value=\"7\" /></initialValue>"));
        assert!(xml.contains("<arrayValue>"));
        assert!(xml.contains("<structValue>"));

        let imported = import_plcopen_xml("initial-values.xml", &xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let program = imported.project.first_program().expect("program");
        let vars = &program.var_blocks[0].vars;
        assert!(matches!(
            vars[0].initial_value,
            Some(Expr::Literal(Literal::Int(7)))
        ));
        assert!(matches!(
            vars[1].initial_value,
            Some(Expr::Literal(Literal::Bool(true)))
        ));
        assert!(
            matches!(vars[2].initial_value, Some(Expr::ArrayLiteral(ref values)) if values.len() == 2)
        );
        assert!(
            matches!(vars[3].initial_value, Some(Expr::StructLiteral(ref fields)) if fields.len() == 2)
        );
    }

    #[test]
    fn round_trips_access_variables() {
        let parsed = parse_project(
            "access-vars.st",
            r#"
            PROGRAM Controller
            VAR Count : INT; END_VAR
            END_PROGRAM

            CONFIGURATION Plant
              VAR_GLOBAL
                Shared : INT;
              END_VAR
              VAR_ACCESS
                PublicShared : Shared : INT READ_WRITE;
              END_VAR
              RESOURCE Cpu ON PLC
                PROGRAM Main : Controller;
                VAR_ACCESS
                  PublicCount : Main.Count : INT READ_ONLY;
                END_VAR
              END_RESOURCE
            END_CONFIGURATION
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<accessVariable alias=\"PublicShared\" instancePathAndName=\"Shared\" direction=\"readWrite\"><type><INT /></type></accessVariable>"));
        assert!(xml.contains("<accessVariable alias=\"PublicCount\" instancePathAndName=\"Main.Count\" direction=\"readOnly\"><type><INT /></type></accessVariable>"));

        let imported = import_plcopen_xml("access-vars.xml", &xml);
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
        let access = configuration.var_blocks[1].vars[0].access.as_ref().unwrap();
        assert_eq!(access.direction, AccessDirection::ReadWrite);
        assert_eq!(access.path.to_string(), "Shared");
        let resource_access = configuration.resources[0].var_blocks[0].vars[0]
            .access
            .as_ref()
            .unwrap();
        assert_eq!(resource_access.direction, AccessDirection::ReadOnly);
        assert_eq!(resource_access.path.to_string(), "Main.Count");
    }

    #[test]
    fn round_trips_function_return_type() {
        let parsed = parse_project(
            "function-return.st",
            r#"
            FUNCTION IsReady : BOOL
            VAR_INPUT Input : INT; END_VAR
            IsReady := Input > 0;
            END_FUNCTION
            "#,
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let xml = export_plcopen_xml(&parsed.project);
        assert!(xml.contains("<returnType><BOOL /></returnType>"));

        let imported = import_plcopen_xml("function-return.xml", &xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let function = imported
            .project
            .pous()
            .find(|pou| pou.name.original == "IsReady")
            .expect("function should be imported");
        assert!(matches!(
            &function.kind,
            PouKind::Function {
                return_type: DataTypeSpec::Elementary(ElementaryType::Bool)
            }
        ));
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
        assert!(xml.contains("<subrangeSigned>"));
        assert!(xml.contains("<range lower=\"0\" upper=\"10\" />"));
        assert!(xml.contains("<value name=\"Run\" />"));
        assert!(xml.contains("<string length=\"8\" />"));
        assert!(xml.contains("<dimension lower=\"1\" upper=\"3\" />"));
        assert!(xml.contains("<baseType><derived name=\"Small\" /></baseType>"));

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
    fn round_trips_rendered_st_statement_corpus() {
        let source = r#"
            PROGRAM RoundTrip
            VAR
                A : INT := 0;
                B : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            A := 1 + 2 * 3 ** 2;
            Flag := TRUE OR FALSE XOR TRUE AND NOT FALSE;
            IF A > 1 THEN
                B := A;
            ELSE
                B := 0;
            END_IF;
            CASE B OF
                1, 2..3: Flag := TRUE;
                ELSE Flag := FALSE;
            END_CASE;
            FOR A := 1 TO 3 BY 1 DO
                B := B + A;
            END_FOR;
        END_PROGRAM
        "#;
        let parsed = parse_project("st_roundtrip_source.st", source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let first = statements_to_st(&parsed.project.first_program().unwrap().body.statements);

        let reparsed_source = format!(
            r#"
            PROGRAM RoundTrip
            VAR
                A : INT := 0;
                B : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            {first}
            END_PROGRAM
            "#
        );
        let reparsed = parse_project("st_roundtrip_rendered.st", &reparsed_source);
        assert!(
            reparsed.diagnostics.is_empty(),
            "{:?}\n{}",
            reparsed.diagnostics,
            reparsed_source
        );
        let second = statements_to_st(&reparsed.project.first_program().unwrap().body.statements);
        assert_eq!(first, second);
    }

    #[test]
    fn round_trips_generated_st_property_corpus() {
        for index in 0..48_i64 {
            let source = format!(
                r#"
                PROGRAM GeneratedRoundTrip
                VAR
                    A : INT := {index};
                    B : INT := 0;
                    Flag : BOOL := FALSE;
                END_VAR
                A := ({index} + 1) * 2;
                IF A > {index} THEN
                    B := A - {index};
                ELSE
                    B := 0;
                END_IF;
                CASE B OF
                    0: Flag := FALSE;
                    1..200: Flag := TRUE;
                    ELSE Flag := FALSE;
                END_CASE;
                END_PROGRAM
                "#
            );
            let parsed = parse_project(format!("generated_roundtrip_{index}.st"), &source);
            assert!(
                parsed.diagnostics.is_empty(),
                "case {index}: {:?}",
                parsed.diagnostics
            );
            let first = statements_to_st(&parsed.project.first_program().unwrap().body.statements);
            let reparsed_source = format!(
                r#"
                PROGRAM GeneratedRoundTrip
                VAR
                    A : INT := {index};
                    B : INT := 0;
                    Flag : BOOL := FALSE;
                END_VAR
                {first}
                END_PROGRAM
                "#
            );
            let reparsed = parse_project(
                format!("generated_roundtrip_reparse_{index}.st"),
                &reparsed_source,
            );
            assert!(
                reparsed.diagnostics.is_empty(),
                "case {index}: {:?}\n{}",
                reparsed.diagnostics,
                reparsed_source
            );
            let second =
                statements_to_st(&reparsed.project.first_program().unwrap().body.statements);
            assert_eq!(first, second, "case {index}");
        }
    }

    #[test]
    fn imports_and_exports_sfc_structure() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Sequence" pouType="program">
                  <body><SFC>
                    <step localId="1" name="Start" initialStep="true">
                      <connectionPointOut />
                      <actionBlock>
                        <action localId="10" qualifier="P" referenceName="DoRun" />
                      </actionBlock>
                    </step>
                    <step localId="2" name="Run">
                      <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                    </step>
                    <transition localId="3" name="Go">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
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
        assert_eq!(sfc.steps[0].actions.len(), 1);
        assert_eq!(sfc.steps[0].actions[0].name.canonical, "DORUN");
        assert_eq!(
            sfc.steps[0].actions[0].qualifier,
            Some(SfcActionQualifier::Pulse)
        );
        assert_eq!(sfc.transitions.len(), 1);
        assert_eq!(sfc.transitions[0].from.len(), 1);
        assert_eq!(sfc.transitions[0].from[0].canonical, "START");
        assert_eq!(sfc.transitions[0].to.len(), 1);
        assert_eq!(sfc.transitions[0].to[0].canonical, "RUN");
        assert!(sfc.transitions[0].condition.is_some());
        assert_eq!(sfc.actions.len(), 1);
        assert_eq!(sfc.actions[0].qualifier, SfcActionQualifier::TimeLimited);
        assert_eq!(sfc.actions[0].duration, Some(Literal::DurationMs(5)));

        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<SFC>"));
        assert!(exported.contains("name=\"Start\""));
        assert!(exported.contains("name=\"Go\""));
        assert!(exported.contains("name=\"DoRun\""));
        assert!(exported.contains("<actionBlock>"));
        assert!(exported.contains("referenceName=\"DoRun\""));
        assert!(exported.contains("<connectionPointIn>"));
        assert!(exported.contains("refLocalId=\"1\""));
        assert!(exported.contains("refLocalId=\"3\""));
        assert!(exported.contains("qualifier=\"P\""));
        assert!(exported.contains("qualifier=\"L\""));
        assert!(exported.contains("duration=\"T#5ms\""));
    }

    #[test]
    fn imports_sfc_branch_connectors_as_transition_edges() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Sequence" pouType="program">
                  <body><SFC>
                    <step localId="1" name="Start" initialStep="true">
                      <connectionPointOut />
                    </step>
                    <selectionDivergence localId="2">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                    </selectionDivergence>
                    <transition localId="3" name="ToA">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                      <condition><ST><xhtml:p>TRUE</xhtml:p></ST></condition>
                    </transition>
                    <transition localId="4" name="ToB">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                      <condition><ST><xhtml:p>FALSE</xhtml:p></ST></condition>
                    </transition>
                    <step localId="5" name="A">
                      <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                    </step>
                    <step localId="6" name="B">
                      <connectionPointIn><connection refLocalId="4" /></connectionPointIn>
                    </step>
                  </SFC></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("sfc_branch.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let sfc = imported
            .project
            .first_program()
            .and_then(|pou| pou.body.sfc.as_ref())
            .expect("SFC should be imported");
        assert_eq!(sfc.transitions[0].from[0].canonical, "START");
        assert_eq!(sfc.transitions[0].to[0].canonical, "A");
        assert_eq!(sfc.transitions[1].from[0].canonical, "START");
        assert_eq!(sfc.transitions[1].to[0].canonical, "B");
    }

    #[test]
    fn imports_sfc_jump_step_as_transition_target() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Sequence" pouType="program">
                  <body><SFC>
                    <step localId="1" name="Start" initialStep="true">
                      <connectionPointOut />
                    </step>
                    <transition localId="2" name="Jump">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                      <condition><ST><xhtml:p>TRUE</xhtml:p></ST></condition>
                    </transition>
                    <jumpStep localId="3" targetName="Run">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </jumpStep>
                    <step localId="4" name="Run" />
                  </SFC></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("sfc_jump.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let sfc = imported
            .project
            .first_program()
            .and_then(|pou| pou.body.sfc.as_ref())
            .expect("SFC should be imported");
        assert_eq!(sfc.transitions[0].from[0].canonical, "START");
        assert_eq!(sfc.transitions[0].to[0].canonical, "RUN");
    }

    #[test]
    fn imports_sfc_jump_alias_as_transition_target() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Sequence" pouType="program">
                  <body><SFC>
                    <step localId="1" name="Start" initialStep="true">
                      <connectionPointOut />
                    </step>
                    <transition localId="2" name="Jump">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                      <condition><ST><xhtml:p>TRUE</xhtml:p></ST></condition>
                    </transition>
                    <jump localId="3" targetName="Run">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </jump>
                    <step localId="4" name="Run" />
                  </SFC></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("sfc_jump_alias.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let sfc = imported
            .project
            .first_program()
            .and_then(|pou| pou.body.sfc.as_ref())
            .expect("SFC should be imported");
        assert_eq!(sfc.transitions[0].from[0].canonical, "START");
        assert_eq!(sfc.transitions[0].to[0].canonical, "RUN");
    }

    #[test]
    fn imports_and_exports_sfc_macro_steps() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Sequence" pouType="program">
                  <body><SFC>
                    <step localId="1" name="Start" initialStep="true">
                      <connectionPointOut />
                    </step>
                    <transition localId="2" name="EnterMacro">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                      <condition><ST><xhtml:p>TRUE</xhtml:p></ST></condition>
                    </transition>
                    <macroStep localId="3" name="Run">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </macroStep>
                  </SFC></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("sfc_macro_step.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let sfc = imported
            .project
            .first_program()
            .and_then(|pou| pou.body.sfc.as_ref())
            .expect("SFC should be imported");
        assert!(sfc
            .steps
            .iter()
            .any(|step| { step.name.canonical == "RUN" && step.kind == SfcStepKind::MacroStep }));
        assert_eq!(sfc.transitions[0].from[0].canonical, "START");
        assert_eq!(sfc.transitions[0].to[0].canonical, "RUN");

        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<macroStep"));
        assert!(exported.contains("name=\"Run\""));
    }

    #[test]
    fn preserves_ld_and_fbd_plcopen_nodes() {
        let ld_xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Ladder" pouType="program">
                  <body><LD>
                    <leftPowerRail localId="1" />
                    <contact localId="2" variable="Start" width="30" height="20">
                      <position x="10" y="20" />
                    </contact>
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
        let contact = pou.body.networks[0]
            .nodes
            .iter()
            .find(|node| node.id == "2")
            .expect("contact should import");
        assert_eq!(
            contact.attributes.get("width").map(String::as_str),
            Some("30")
        );
        assert_eq!(
            contact.attributes.get("positionX").map(String::as_str),
            Some("10")
        );
        assert_eq!(
            contact.attributes.get("positionY").map(String::as_str),
            Some("20")
        );
        assert!(matches!(
            pou.body.statements.first(),
            Some(Statement::Assignment { .. })
        ));
        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("<LD>"));
        assert!(exported.contains("variable=\"Start\""));
        assert!(exported.contains("width=\"30\""));
        assert!(exported.contains("<position x=\"10\" y=\"20\" />"));
        assert!(exported.contains("variable=\"Motor\""));

        let fbd_xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Blocks" pouType="program">
                  <body><FBD>
                    <inVariable localId="1"><expression>A</expression></inVariable>
                    <block localId="2" typeName="ADD" width="80" height="40">
                      <position x="100" y="50" />
                    </block>
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
        let block = pou.body.networks[0]
            .nodes
            .iter()
            .find(|node| node.id == "2")
            .expect("block should import");
        assert_eq!(
            block.attributes.get("height").map(String::as_str),
            Some("40")
        );
        assert_eq!(
            block.attributes.get("positionX").map(String::as_str),
            Some("100")
        );
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
        assert!(exported.contains("height=\"40\""));
        assert!(exported.contains("<position x=\"100\" y=\"50\" />"));
        assert!(exported.contains("<expression>A</expression>"));
    }

    #[test]
    fn round_trips_full_project_graphical_configuration_and_metadata() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <fileHeader companyName="RobotCo" productName="RoboC++" productVersion="0.1.0" />
              <contentHeader name="robot-cell" modificationDateTime="2026-05-27T00:00:00" />
              <types>
                <dataTypes>
                  <dataType name="Small"><baseType><subrange baseType="INT" lower="0" upper="10" /></baseType></dataType>
                </dataTypes>
                <pous>
                  <pou name="Controller" pouType="program">
                    <interface>
                      <localVars><variable name="Count"><type><derived name="INT" /></type></variable></localVars>
                    </interface>
                    <body><ST><xhtml:p>Count := Count + 1;</xhtml:p></ST></body>
                  </pou>
                  <pou name="Ladder" pouType="program">
                    <body><LD>
                      <leftPowerRail localId="1" />
                      <contact localId="2" variable="Start">
                        <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                      </contact>
                      <coil localId="3" variable="Motor">
                        <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                      </coil>
                    </LD></body>
                  </pou>
                  <pou name="Blocks" pouType="program">
                    <body><FBD>
                      <inVariable localId="1"><expression>A</expression></inVariable>
                      <inVariable localId="2"><expression>B</expression></inVariable>
                      <block localId="3" typeName="ADD">
                        <inputVariables>
                          <variable formalParameter="IN1"><connectionPointIn><connection refLocalId="1" /></connectionPointIn></variable>
                          <variable formalParameter="IN2"><connectionPointIn><connection refLocalId="2" /></connectionPointIn></variable>
                        </inputVariables>
                      </block>
                      <outVariable localId="4">
                        <expression>C</expression>
                        <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                      </outVariable>
                    </FBD></body>
                  </pou>
                  <pou name="Sequence" pouType="program">
                    <body><SFC>
                      <step localId="1" name="Start" initialStep="true" />
                      <step localId="2" name="Run" />
                      <transition localId="3" name="Go">
                        <condition><ST><xhtml:p>Ready</xhtml:p></ST></condition>
                      </transition>
                      <action localId="4" name="Run" qualifier="P">
                        <ST><xhtml:p>Done := TRUE;</xhtml:p></ST>
                      </action>
                    </SFC></body>
                  </pou>
                </pous>
              </types>
              <instances><configurations>
                <configuration name="Plant">
                  <globalVars><variable name="Shared" address="%MW0"><type><derived name="INT" /></type></variable></globalVars>
                  <resource name="Cpu">
                    <configVars><variable name="Slot" address="%IW0"><type><derived name="INT" /></type></variable></configVars>
                    <task name="Fast" interval="T#10ms" priority="1" />
                    <program name="Main" typeName="Controller" task="Fast" />
                  </resource>
                </configuration>
              </configurations></instances>
              <addData><data name="RobotCo.Extensions"><RobotCoProfile axis="Arm1" /></data></addData>
            </project>
        "#;
        let imported = import_plcopen_xml("full-project.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        assert_eq!(imported.project.pous().count(), 4);
        assert!(imported
            .project
            .metadata
            .get("plcopen.addData")
            .is_some_and(|data| data.contains("RobotCoProfile")));
        assert!(imported.project.pous().any(|pou| pou.body.language
            == ImplementationLanguage::LadderDiagram
            && !pou.body.networks.is_empty()
            && !pou.body.statements.is_empty()));
        assert!(imported.project.pous().any(|pou| pou.body.language
            == ImplementationLanguage::FunctionBlockDiagram
            && !pou.body.networks.is_empty()
            && !pou.body.statements.is_empty()));
        assert!(imported.project.pous().any(|pou| pou.body.language
            == ImplementationLanguage::SequentialFunctionChart
            && pou
                .body
                .sfc
                .as_ref()
                .is_some_and(|sfc| sfc.steps.len() == 2)));

        let exported = export_plcopen_xml(&imported.project);
        assert!(exported.contains("companyName=\"RobotCo\""));
        assert!(exported.contains("contentHeader name=\"robot-cell\""));
        assert!(exported.contains("<dataType name=\"Small\">"));
        assert!(exported.contains("<LD>"));
        assert!(exported.contains("<FBD>"));
        assert!(exported.contains("<SFC>"));
        assert!(exported.contains("<configuration name=\"Plant\">"));
        assert!(exported.contains("<task name=\"Fast\" interval=\"T#10ms\" priority=\"1\">"));
        assert!(exported.contains("<pouInstance name=\"Main\" typeName=\"Controller\" />"));
        assert!(exported.contains("RobotCoProfile axis=\"Arm1\""));
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
    fn lowers_ld_parallel_branches_and_stored_coils() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Ladder" pouType="program">
                  <body><LD>
                    <leftPowerRail localId="1" />
                    <contact localId="2" variable="Start">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                    </contact>
                    <contact localId="3" variable="Stop" negated="true">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </contact>
                    <contact localId="4" variable="Override">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                    </contact>
                    <coil localId="5" variable="Motor">
                      <connectionPointIn>
                        <connection refLocalId="3" />
                        <connection refLocalId="4" />
                      </connectionPointIn>
                    </coil>
                    <coil localId="6" variable="Latched" storage="set">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </coil>
                    <coil localId="7" variable="Latched" storage="reset">
                      <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                    </coil>
                  </LD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("ld-branches.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        let statements = pou
            .body
            .statements
            .iter()
            .map(statement_to_st)
            .collect::<Vec<_>>();

        assert_eq!(
            statements[0],
            "Motor := (((TRUE AND Start) AND NOT Stop) OR (TRUE AND Override));"
        );
        assert_eq!(
            statements[1],
            "IF (TRUE AND Start) THEN\nLatched := TRUE;\nEND_IF;"
        );
        assert_eq!(
            statements[2],
            "IF ((TRUE AND Start) AND NOT Stop) THEN\nLatched := FALSE;\nEND_IF;"
        );
    }

    #[test]
    fn lowers_ld_edge_contacts_with_hidden_trigger_instances() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Ladder" pouType="program">
                  <interface>
                    <localVars>
                      <variable name="Start"><type><derived name="BOOL" /></type></variable>
                      <variable name="Motor"><type><derived name="BOOL" /></type></variable>
                    </localVars>
                  </interface>
                  <body><LD>
                    <leftPowerRail localId="1" />
                    <contact localId="2" variable="Start" edge="rising">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                    </contact>
                    <coil localId="3" variable="Motor">
                      <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                    </coil>
                  </LD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("ld-edge.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        assert!(pou.var_blocks.iter().any(|block| {
            block.vars.iter().any(|var| {
                var.name.original == "rbcpp_ld_edge_2"
                    && var.type_spec == DataTypeSpec::Named(Identifier::new("R_TRIG"))
            })
        }));
        let statements = pou
            .body
            .statements
            .iter()
            .map(statement_to_st)
            .collect::<Vec<_>>();
        assert_eq!(statements[0], "rbcpp_ld_edge_2(CLK := Start);");
        assert_eq!(statements[1], "Motor := (TRUE AND rbcpp_ld_edge_2.Q);");
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

    #[test]
    fn lowers_fbd_multi_output_data_flow_graph() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Blocks" pouType="program">
                  <body><FBD>
                    <inVariable localId="1"><expression>A</expression></inVariable>
                    <inVariable localId="2"><expression>B</expression></inVariable>
                    <inVariable localId="3"><expression>C</expression></inVariable>
                    <block localId="4" typeName="ADD">
                      <inputVariables>
                        <variable formalParameter="IN1">
                          <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                        </variable>
                        <variable formalParameter="IN2">
                          <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
                        </variable>
                      </inputVariables>
                    </block>
                    <block localId="5" typeName="MUL">
                      <inputVariables>
                        <variable formalParameter="IN1">
                          <connectionPointIn><connection refLocalId="4" /></connectionPointIn>
                        </variable>
                        <variable formalParameter="IN2">
                          <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                        </variable>
                      </inputVariables>
                    </block>
                    <outVariable localId="6">
                      <expression>D</expression>
                      <connectionPointIn><connection refLocalId="5" /></connectionPointIn>
                    </outVariable>
                    <outVariable localId="7">
                      <expression>E</expression>
                      <connectionPointIn><connection refLocalId="4" /></connectionPointIn>
                    </outVariable>
                  </FBD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("fbd-dag.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        let statements = pou
            .body
            .statements
            .iter()
            .map(statement_to_st)
            .collect::<Vec<_>>();

        assert_eq!(
            statements,
            vec![
                "D := MUL(IN1 := ADD(IN1 := A, IN2 := B), IN2 := C);",
                "E := ADD(IN1 := A, IN2 := B);"
            ]
        );
    }

    #[test]
    fn lowers_fbd_connector_continuation_forwarding() {
        let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Blocks" pouType="program">
                  <body><FBD>
                    <inVariable localId="1"><expression>A</expression></inVariable>
                    <connector localId="2" name="Feed">
                      <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
                    </connector>
                    <continuation localId="3" name="Feed" />
                    <inVariable localId="4"><expression>B</expression></inVariable>
                    <block localId="5" typeName="ADD">
                      <inputVariables>
                        <variable formalParameter="IN1">
                          <connectionPointIn><connection refLocalId="3" /></connectionPointIn>
                        </variable>
                        <variable formalParameter="IN2">
                          <connectionPointIn><connection refLocalId="4" /></connectionPointIn>
                        </variable>
                      </inputVariables>
                    </block>
                    <outVariable localId="6">
                      <expression>C</expression>
                      <connectionPointIn><connection refLocalId="5" /></connectionPointIn>
                    </outVariable>
                  </FBD></body>
                </pou>
              </pous></types>
            </project>
        "#;
        let imported = import_plcopen_xml("fbd-continuation.xml", xml);
        assert!(
            imported.diagnostics.is_empty(),
            "{:?}",
            imported.diagnostics
        );
        let pou = imported.project.first_program().unwrap();
        let statement = pou.body.statements.first().expect("FBD should lower");
        assert_eq!(statement_to_st(statement), "C := ADD(IN1 := A, IN2 := B);");
    }
}
