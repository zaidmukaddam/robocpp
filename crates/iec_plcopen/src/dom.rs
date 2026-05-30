// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub(crate) struct XmlNamespaceRegistry {
    prefixes_by_uri: BTreeMap<String, String>,
}

impl XmlNamespaceRegistry {
    pub(crate) fn from_document(root: Node<'_, '_>) -> Self {
        let mut prefixes_by_uri = BTreeMap::from([
            (XHTML_NS.to_string(), "xhtml".to_string()),
            (XML_NS.to_string(), "xml".to_string()),
        ]);
        let mut uri_by_prefix = BTreeMap::from([
            ("xhtml".to_string(), XHTML_NS.to_string()),
            ("xml".to_string(), XML_NS.to_string()),
        ]);

        for node in root.descendants().filter(|node| node.is_element()) {
            for namespace in node.namespaces() {
                let Some(prefix) = namespace.name() else {
                    continue;
                };
                let uri = namespace.uri();
                if uri == PLCOPEN_TC6_0201_NS || prefixes_by_uri.contains_key(uri) {
                    continue;
                }
                let prefix = unique_namespace_prefix(prefix, uri, &uri_by_prefix);
                uri_by_prefix.insert(prefix.clone(), uri.to_string());
                prefixes_by_uri.insert(uri.to_string(), prefix);
            }

            for attribute in node.attributes() {
                let Some(uri) = attribute.namespace() else {
                    continue;
                };
                if uri == PLCOPEN_TC6_0201_NS || prefixes_by_uri.contains_key(uri) {
                    continue;
                }
                let prefix = unique_namespace_prefix("ns", uri, &uri_by_prefix);
                uri_by_prefix.insert(prefix.clone(), uri.to_string());
                prefixes_by_uri.insert(uri.to_string(), prefix);
            }
        }

        Self { prefixes_by_uri }
    }

    pub(crate) fn prefix_for_uri(&self, uri: &str) -> Option<&str> {
        if uri == PLCOPEN_TC6_0201_NS {
            return None;
        }
        self.prefixes_by_uri.get(uri).map(String::as_str)
    }

    pub(crate) fn namespace_attributes(&self) -> Vec<String> {
        let mut attributes = self
            .prefixes_by_uri
            .iter()
            .filter(|(uri, _)| uri.as_str() != XHTML_NS && uri.as_str() != XML_NS)
            .map(|(uri, prefix)| (prefix.as_str(), uri.as_str()))
            .collect::<Vec<_>>();
        attributes.sort_unstable_by(|left, right| left.0.cmp(right.0));
        attributes
            .into_iter()
            .map(|(prefix, uri)| format!("xmlns:{prefix}=\"{}\"", xml_escape(uri)))
            .collect()
    }
}

pub(crate) fn unique_namespace_prefix(
    preferred: &str,
    uri: &str,
    uri_by_prefix: &BTreeMap<String, String>,
) -> String {
    let preferred = if preferred.is_empty() || preferred == "xmlns" {
        "ns"
    } else {
        preferred
    };
    if uri_by_prefix
        .get(preferred)
        .map_or(true, |known_uri| known_uri == uri)
    {
        return preferred.to_string();
    }
    for index in 1.. {
        let candidate = format!("ns{index}");
        if !uri_by_prefix.contains_key(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded namespace prefix search should always return");
}

#[cfg(test)]
pub(crate) fn canonicalize_plcopen_xml(
    root: Node<'_, '_>,
    namespaces: &XmlNamespaceRegistry,
) -> String {
    let mut out = String::new();
    canonicalize_xml_node(root, namespaces, true, &mut out);
    out
}

pub(crate) fn canonicalize_xml_node(
    node: Node<'_, '_>,
    namespaces: &XmlNamespaceRegistry,
    is_root: bool,
    out: &mut String,
) {
    if node.is_text() {
        if let Some(text) = node.text() {
            out.push_str(&xml_escape(text));
        }
        return;
    }
    if !node.is_element() {
        return;
    }

    let name = canonical_element_name(node, namespaces);
    out.push('<');
    out.push_str(&name);
    if is_root {
        out.push_str(&format!(
            " xmlns=\"{}\" xmlns:xhtml=\"{}\"",
            PLCOPEN_TC6_0201_NS, XHTML_NS
        ));
        for namespace in namespaces.namespace_attributes() {
            out.push(' ');
            out.push_str(&namespace);
        }
    }
    for attribute in node.attributes() {
        out.push(' ');
        out.push_str(&canonical_attribute_name(attribute, namespaces));
        out.push_str("=\"");
        out.push_str(&xml_escape(attribute.value()));
        out.push('"');
    }

    if !node
        .children()
        .any(|child| child.is_element() || child.is_text())
    {
        out.push_str(" />");
        return;
    }

    out.push('>');
    for child in node.children() {
        canonicalize_xml_node(child, namespaces, false, out);
    }
    out.push_str("</");
    out.push_str(&name);
    out.push('>');
}

pub(crate) fn canonical_element_name(
    node: Node<'_, '_>,
    namespaces: &XmlNamespaceRegistry,
) -> String {
    let name = node.tag_name();
    match name
        .namespace()
        .and_then(|uri| namespaces.prefix_for_uri(uri))
    {
        Some(prefix) => format!("{prefix}:{}", name.name()),
        None => name.name().to_string(),
    }
}

pub(crate) fn canonical_attribute_name(
    attribute: Attribute<'_, '_>,
    namespaces: &XmlNamespaceRegistry,
) -> String {
    match attribute
        .namespace()
        .and_then(|uri| namespaces.prefix_for_uri(uri))
    {
        Some(prefix) => format!("{prefix}:{}", attribute.name()),
        None => attribute.name().to_string(),
    }
}

pub(crate) fn canonical_xml_fragment(
    node: Node<'_, '_>,
    namespaces: &XmlNamespaceRegistry,
) -> String {
    let mut out = String::new();
    canonicalize_xml_node(node, namespaces, false, &mut out);
    out
}

pub(crate) fn canonical_xml_children(
    node: Node<'_, '_>,
    namespaces: &XmlNamespaceRegistry,
) -> String {
    let mut out = String::new();
    for child in node.children() {
        canonicalize_xml_node(child, namespaces, false, &mut out);
    }
    out
}

pub(crate) fn element_is(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(name)
}

pub(crate) fn first_child_element<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.children().find(|child| element_is(*child, name))
}

pub(crate) fn child_elements<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> Vec<Node<'a, 'input>> {
    node.children()
        .filter(|child| element_is(*child, name))
        .collect()
}

pub(crate) fn first_descendant_element<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> Option<Node<'a, 'input>> {
    node.descendants()
        .find(|descendant| element_is(*descendant, name))
}

pub(crate) fn descendant_elements<'a, 'input>(
    node: Node<'a, 'input>,
    name: &str,
) -> Vec<Node<'a, 'input>> {
    node.descendants()
        .filter(|descendant| element_is(*descendant, name))
        .collect()
}

pub(crate) fn node_text_content(node: Node<'_, '_>) -> String {
    let mut out = String::new();
    append_node_text(node, &mut out);
    out
}

pub(crate) fn append_node_text(node: Node<'_, '_>, out: &mut String) {
    for child in node.children() {
        if child.is_text() {
            if let Some(text) = child.text() {
                out.push_str(text);
            }
        } else if child.is_element() {
            let before = out.len();
            append_node_text(child, out);
            if out.len() > before && !out.ends_with('\n') {
                out.push('\n');
            }
        }
    }
}
