// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub fn import_plcopen_xml(source_name: &str, xml: &str) -> PlcOpenImport {
    import_plcopen_xml_with_options(source_name, xml, &PlcOpenImportOptions::default())
}

pub fn import_plcopen_xml_with_options(
    source_name: &str,
    xml: &str,
    options: &PlcOpenImportOptions,
) -> PlcOpenImport {
    let mut diagnostics = Vec::new();
    if xml.len() > options.implementation.max_plcopen_xml_bytes {
        return PlcOpenImport {
            project: Project::new(EditionProfile::Iec61131_3_2003Strict),
            diagnostics: vec![Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "PLCopen XML size {} bytes exceeds maximum {}",
                    xml.len(),
                    options.implementation.max_plcopen_xml_bytes
                ),
                None,
            )],
        };
    }

    let validation = match validate_plcopen_xml(xml, &options.implementation) {
        Ok(validation) => validation,
        Err(validation_diagnostics) => {
            return PlcOpenImport {
                project: Project::new(EditionProfile::Iec61131_3_2003Strict),
                diagnostics: validation_diagnostics,
            };
        }
    };
    diagnostics.extend(validation.diagnostics);
    let document = match parse_validated_plcopen_document(xml, &options.implementation) {
        Ok(document) => document,
        Err(validation_diagnostics) => {
            return PlcOpenImport {
                project: Project::new(EditionProfile::Iec61131_3_2003Strict),
                diagnostics: validation_diagnostics,
            };
        }
    };
    let root = document.root_element();
    let namespaces = XmlNamespaceRegistry::from_document(root);

    let mut project = Project::new(EditionProfile::Iec61131_3_2003Strict);
    if !validation.namespace_attributes.is_empty() {
        project.metadata.insert(
            "plcopen.rootNamespaces".to_string(),
            validation.namespace_attributes.join(" "),
        );
    }
    let model = PlcOpenProjectModel::from_root(
        source_name,
        root,
        &namespaces,
        &options.implementation,
        &mut diagnostics,
    );
    if let Some(file_header) = model.file_header {
        project
            .metadata
            .insert("plcopen.fileHeader".to_string(), file_header);
    }
    if let Some(content_header) = model.content_header {
        project
            .metadata
            .insert("plcopen.contentHeader".to_string(), content_header);
    }
    if let Some(add_data) = model.add_data {
        project
            .metadata
            .insert("plcopen.addData".to_string(), add_data);
    }
    project.library_elements.extend(
        model
            .pous
            .into_iter()
            .map(|pou| LibraryElement::Pou(pou.into_pou())),
    );
    project.library_elements.extend(
        model
            .data_types
            .into_iter()
            .map(|data_type| LibraryElement::DataType(data_type.into_declaration())),
    );
    project.library_elements.extend(
        model
            .configurations
            .into_iter()
            .map(|configuration| LibraryElement::Configuration(configuration.into_configuration())),
    );

    PlcOpenImport {
        project,
        diagnostics,
    }
}

impl PlcOpenProjectModel {
    fn from_root(
        source_name: &str,
        root: Node<'_, '_>,
        namespaces: &XmlNamespaceRegistry,
        implementation: &ImplementationParameters,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let file_header = first_child_element(root, "fileHeader")
            .map(|node| canonical_xml_fragment(node, namespaces));
        let content_header = first_child_element(root, "contentHeader")
            .map(|node| canonical_xml_fragment(node, namespaces));
        let add_data = first_child_element(root, "addData")
            .map(|node| canonical_xml_children(node, namespaces))
            .filter(|text| !text.trim().is_empty());

        let types = first_child_element(root, "types");
        let data_types = types
            .and_then(|node| first_child_element(node, "dataTypes"))
            .map(|node| parse_plcopen_data_types(node, implementation, diagnostics))
            .unwrap_or_default();
        let pous = types
            .and_then(|node| first_child_element(node, "pous"))
            .map(|node| {
                child_elements(node, "pou")
                    .into_iter()
                    .filter_map(|pou| {
                        parse_plcopen_pou(source_name, pou, implementation, diagnostics)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let configurations = first_child_element(root, "instances")
            .and_then(|node| first_child_element(node, "configurations").or(Some(node)))
            .map(|node| parse_plcopen_configurations(node, implementation, diagnostics))
            .unwrap_or_default();

        Self {
            file_header,
            content_header,
            add_data,
            data_types,
            pous,
            configurations,
        }
    }
}

pub(crate) fn parse_validated_plcopen_document<'a>(
    xml: &'a str,
    implementation: &ImplementationParameters,
) -> Result<Document<'a>, Vec<Diagnostic>> {
    let nodes_limit = implementation
        .max_plcopen_xml_nodes
        .min(u32::MAX as usize)
        .max(1) as u32;
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit,
        entity_resolver: None,
    };
    Document::parse_with_options(xml, options).map_err(|err| {
        vec![Diagnostic::error(
            DiagnosticCode::Syntax,
            format!("PLCopen XML is not well-formed after validation: {err}"),
            None,
        )]
    })
}

pub(crate) fn parse_plcopen_pou(
    source_name: &str,
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<PlcOpenPouModel> {
    let name = node.attribute("name").unwrap_or("UnnamedPou").to_string();
    let pou_type = node.attribute("pouType").unwrap_or("program");
    let interface = first_child_element(node, "interface");
    let return_type = interface
        .and_then(|node| first_child_element(node, "returnType"))
        .and_then(parse_plcopen_type)
        .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Int));
    let kind = match pou_type.to_ascii_lowercase().as_str() {
        "function" => PouKind::Function { return_type },
        "functionblock" | "function_block" => PouKind::FunctionBlock,
        _ => PouKind::Program,
    };
    let var_blocks = interface
        .map(|node| parse_plcopen_var_blocks(node, implementation, diagnostics))
        .unwrap_or_default();
    let body = first_child_element(node, "body")
        .map(|node| parse_plcopen_pou_body(source_name, &name, &kind, node, diagnostics))
        .unwrap_or_else(PlcOpenBodyModel::empty);

    Some(PlcOpenPouModel {
        name: Identifier::new(name),
        kind,
        interface: PlcOpenInterfaceModel { var_blocks },
        body,
    })
}

pub(crate) fn parse_plcopen_pou_body(
    source_name: &str,
    name: &str,
    kind: &PouKind,
    body: Node<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> PlcOpenBodyModel {
    if let Some(ld) = first_child_element(body, "LD") {
        let graph = plcopen_graphical_body(
            source_name,
            ImplementationLanguage::LadderDiagram,
            ld,
            diagnostics,
        );
        if graph.statements.is_empty() {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::Unsupported,
                format!(
                    "LD body for POU '{name}' imported as PLCopen network nodes without execution semantics"
                ),
                None,
            ));
        }
        return PlcOpenBodyModel::from_graph(graph);
    }
    if let Some(fbd) = first_child_element(body, "FBD") {
        let graph = plcopen_graphical_body(
            source_name,
            ImplementationLanguage::FunctionBlockDiagram,
            fbd,
            diagnostics,
        );
        if graph.statements.is_empty() {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::Unsupported,
                format!(
                    "FBD body for POU '{name}' imported as PLCopen network nodes without execution semantics"
                ),
                None,
            ));
        }
        return PlcOpenBodyModel::from_graph(graph);
    }
    if let Some(sfc) = first_child_element(body, "SFC") {
        return PlcOpenBodyModel::from_body(PouBody {
            language: ImplementationLanguage::SequentialFunctionChart,
            statements: Vec::new(),
            networks: Vec::new(),
            sfc: Some(parse_sfc_body(source_name, sfc, diagnostics)),
        });
    }
    if let Some(st) = first_child_element(body, "ST") {
        let text = node_text_content(st);
        let wrapped = wrap_st_body(name, kind, &text);
        let parsed = parse_project(source_name, &wrapped);
        diagnostics.extend(parsed.diagnostics);
        let body = parsed
            .project
            .find_pou(name)
            .map(|pou| pou.body.clone())
            .unwrap_or_default();
        return PlcOpenBodyModel::from_body(body);
    }

    PlcOpenBodyModel::empty()
}
