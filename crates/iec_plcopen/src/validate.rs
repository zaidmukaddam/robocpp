// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub(crate) fn validate_plcopen_xml(
    xml: &str,
    implementation: &ImplementationParameters,
) -> Result<PlcOpenXmlValidation, Vec<Diagnostic>> {
    if let Some(markup) = forbidden_xml_markup(xml) {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Unsupported,
            format!("PLCopen XML {markup} declarations are not supported"),
            None,
        )]);
    }

    let nodes_limit = implementation
        .max_plcopen_xml_nodes
        .min(u32::MAX as usize)
        .max(1) as u32;
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit,
        entity_resolver: None,
    };
    let document = match Document::parse_with_options(xml, options) {
        Ok(document) => document,
        Err(err) => {
            return Err(vec![Diagnostic::error(
                DiagnosticCode::Syntax,
                format!("PLCopen XML is not well-formed: {err}"),
                None,
            )]);
        }
    };

    let mut diagnostics = Vec::new();
    let root = document.root_element();
    if root.tag_name().name() != "project" {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Syntax,
            format!(
                "PLCopen XML root element must be <project>, found <{}>",
                root.tag_name().name()
            ),
            None,
        )]);
    }
    if root.tag_name().namespace() != Some(PLCOPEN_TC6_0201_NS) {
        diagnostics.push(Diagnostic::warning(
            DiagnosticCode::Compliance,
            "PLCopen XML namespace tc6_0201 was not found; importing best-effort",
            None,
        ));
    }

    let max_depth = implementation.max_plcopen_xml_depth.max(1);
    let max_text = implementation.max_plcopen_xml_text_bytes;
    let max_attr = implementation.max_plcopen_xml_attribute_bytes;
    for node in document.descendants() {
        if node.is_element() {
            let depth = xml_element_depth(node);
            if depth > max_depth {
                return Err(vec![Diagnostic::error(
                    DiagnosticCode::Compliance,
                    format!("PLCopen XML nesting depth {depth} exceeds maximum {max_depth}"),
                    None,
                )]);
            }
            for attr in node.attributes() {
                if attr.value().len() > max_attr {
                    return Err(vec![Diagnostic::error(
                        DiagnosticCode::Compliance,
                        format!(
                            "PLCopen XML attribute '{}' is {} bytes, exceeding maximum {}",
                            attr.name(),
                            attr.value().len(),
                            max_attr
                        ),
                        None,
                    )]);
                }
            }
        }
        if node.is_text() {
            let text_len = node.text().map(str::len).unwrap_or(0);
            if text_len > max_text {
                return Err(vec![Diagnostic::error(
                    DiagnosticCode::Compliance,
                    format!(
                        "PLCopen XML text node is {text_len} bytes, exceeding maximum {max_text}"
                    ),
                    None,
                )]);
            }
        }
    }

    let namespaces = XmlNamespaceRegistry::from_document(root);
    let namespace_attributes = namespaces.namespace_attributes();

    Ok(PlcOpenXmlValidation {
        diagnostics,
        namespace_attributes,
    })
}

pub(crate) fn forbidden_xml_markup(xml: &str) -> Option<&'static str> {
    [
        ("DTD", "<!DOCTYPE"),
        ("entity", "<!ENTITY"),
        ("element", "<!ELEMENT"),
        ("attribute-list", "<!ATTLIST"),
        ("notation", "<!NOTATION"),
    ]
    .into_iter()
    .find_map(|(label, pattern)| contains_ascii_case_insensitive(xml, pattern).then_some(label))
}

pub(crate) fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

pub(crate) fn xml_element_depth(node: Node<'_, '_>) -> usize {
    let mut depth = 0;
    let mut current = Some(node);
    while let Some(node) = current {
        if node.is_element() {
            depth += 1;
        }
        current = node.parent();
    }
    depth
}
