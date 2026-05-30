// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_language_service::{
    code_actions, debug_document, document_graph_model, document_symbol_index, document_type_index,
    format_document, generate_c_artifact, simulate_document, validate_graph_model, DebugOptions,
    DocumentInput, LanguageServiceOptions,
};
use wasm_bindgen::prelude::*;

// IDE/compiler boundary: keep these exports aligned with IDE_CONTRACT.md.
#[wasm_bindgen]
pub fn analyze_document_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }

    iec_language_service::analyze_document(input, &LanguageServiceOptions::default()).to_json()
}

#[wasm_bindgen]
pub fn completions_json(
    uri: &str,
    text: &str,
    language_id: Option<String>,
    prefix: &str,
) -> String {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }

    let completions =
        iec_language_service::analyze_document(input, &LanguageServiceOptions::default())
            .completions_with_prefix(prefix)
            .iter()
            .map(iec_language_service::CompletionItem::to_json)
            .collect::<Vec<_>>()
            .join(",");
    format!("[{completions}]")
}

#[wasm_bindgen]
pub fn run_document_json(
    uri: &str,
    text: &str,
    language_id: Option<String>,
    cycles: Option<usize>,
) -> String {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }

    simulate_document(
        input,
        &LanguageServiceOptions::default(),
        cycles.unwrap_or(5),
    )
    .to_json()
}

#[wasm_bindgen]
pub fn capabilities_json() -> String {
    iec_language_service::ServiceCapabilities::for_options(&LanguageServiceOptions::default())
        .to_json()
}

#[wasm_bindgen]
pub fn source_structure_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let analysis = analyze_input(uri, text, language_id);
    analysis.source.to_json()
}

#[wasm_bindgen]
pub fn symbol_index_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let analysis = analyze_input(uri, text, language_id);
    document_symbol_index(&analysis).to_json()
}

#[wasm_bindgen]
pub fn type_index_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let analysis = analyze_input(uri, text, language_id);
    document_type_index(&analysis).to_json()
}

#[wasm_bindgen]
pub fn graph_model_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let analysis = analyze_input(uri, text, language_id);
    document_graph_model(&analysis).to_json()
}

#[wasm_bindgen]
pub fn validate_graph_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let analysis = analyze_input(uri, text, language_id);
    let model = document_graph_model(&analysis);
    validate_graph_model(&model).to_json()
}

#[wasm_bindgen]
pub fn code_actions_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let analysis = analyze_input(uri, text, language_id);
    let actions = code_actions(&analysis)
        .iter()
        .map(iec_language_service::CodeAction::to_json)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{actions}]")
}

#[wasm_bindgen]
pub fn format_document_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }
    format_document(input).to_json()
}

#[wasm_bindgen]
pub fn debug_document_json(
    uri: &str,
    text: &str,
    language_id: Option<String>,
    cycles: Option<usize>,
) -> String {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }
    debug_document(
        input,
        &LanguageServiceOptions::default(),
        DebugOptions {
            cycles: cycles.unwrap_or(1),
            ..DebugOptions::default()
        },
    )
    .to_json()
}

#[wasm_bindgen]
pub fn generated_c_artifact_json(uri: &str, text: &str, language_id: Option<String>) -> String {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }
    generate_c_artifact(input, &LanguageServiceOptions::default()).to_json()
}

fn analyze_input(
    uri: &str,
    text: &str,
    language_id: Option<String>,
) -> iec_language_service::DocumentAnalysis {
    let mut input = DocumentInput::new(uri, text);
    if let Some(language_id) = language_id {
        input = input.with_language_id(language_id);
    }
    iec_language_service::analyze_document(input, &LanguageServiceOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const IDE_CONTRACT_SCHEMA: &str = include_str!("../tests/fixtures/ide_contract_schema.json");
    const PLCOPEN_IDE_GRAPH_XML: &str = include_str!("../tests/fixtures/plcopen_ide_graph.xml");
    const PLCOPEN_IDE_GRAPH_EXPECTED: &str =
        include_str!("../tests/fixtures/plcopen_ide_graph_expected.json");

    const SAMPLE_ST: &str = r#"
PROGRAM Counter
VAR
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR

IF Count < 3 THEN
    Count := Count + 1;
ELSE
    Done := TRUE;
END_IF;
END_PROGRAM
"#;

    const SAMPLE_LD: &str = r#"
PROGRAM NativeLd
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
"#;

    #[test]
    fn ide_consumed_wasm_json_exports_keep_stable_contract() {
        let contract = json(IDE_CONTRACT_SCHEMA.to_string());

        let analysis = json(analyze_document_json(
            "counter.st",
            SAMPLE_ST,
            Some("st".to_string()),
        ));
        assert_export_keys(&contract, "analyze_document_json", &analysis);
        assert_export_array_item_keys(&contract, "analyze_document_json", &analysis, "symbols");
        assert_export_array_item_keys(&contract, "analyze_document_json", &analysis, "completions");

        let graph = json(graph_model_json(
            "native_ladder.ld",
            SAMPLE_LD,
            Some("ld".to_string()),
        ));
        assert_export_keys(&contract, "graph_model_json", &graph);
        let first_pou = graph["pous"]
            .as_array()
            .and_then(|items| items.first())
            .expect("graph model should expose at least one POU");
        assert_object_keys(first_pou, &["name", "language", "networks", "sfc"]);

        let validation = json(validate_graph_json(
            "native_ladder.ld",
            SAMPLE_LD,
            Some("ld".to_string()),
        ));
        assert_export_keys(&contract, "validate_graph_json", &validation);

        let run = json(run_document_json(
            "counter.st",
            SAMPLE_ST,
            Some("st".to_string()),
            Some(2),
        ));
        assert_export_keys(&contract, "run_document_json", &run);
        assert_export_array_item_keys(&contract, "run_document_json", &run, "cycles");

        let debug = json(debug_document_json(
            "counter.st",
            SAMPLE_ST,
            Some("st".to_string()),
            Some(2),
        ));
        assert_export_keys(&contract, "debug_document_json", &debug);
        assert_export_array_item_keys(&contract, "debug_document_json", &debug, "cycles");

        let artifact = json(generated_c_artifact_json(
            "counter.st",
            SAMPLE_ST,
            Some("st".to_string()),
        ));
        assert_export_keys(&contract, "generated_c_artifact_json", &artifact);
        assert_export_object_keys(
            &contract,
            "generated_c_artifact_json",
            &artifact,
            "metadata",
        );

        let capabilities = json(capabilities_json());
        assert_export_keys(&contract, "capabilities_json", &capabilities);
        assert_eq!(capabilities["profile"], "2003-strict");
        for feature in expected_keys(&contract["capabilities_json"], "featureFlags") {
            assert_eq!(
                capabilities["features"][feature.as_str()],
                true,
                "capabilities should keep IDE feature flag '{feature}'"
            );
        }
    }

    #[test]
    fn plcopen_graph_model_keeps_ide_layout_contract() {
        let expected = json(PLCOPEN_IDE_GRAPH_EXPECTED.to_string());
        let graph = json(graph_model_json(
            "plcopen_ide_graph.xml",
            PLCOPEN_IDE_GRAPH_XML,
            Some("xml".to_string()),
        ));
        let validation = json(validate_graph_json(
            "plcopen_ide_graph.xml",
            PLCOPEN_IDE_GRAPH_XML,
            Some("xml".to_string()),
        ));

        assert_eq!(validation["valid"], true, "graph validation: {validation}");
        let layout = &graph["plcopenLayout"];
        assert_eq!(layout["nodeIds"], expected["nodeIds"]);
        assert_eq!(layout["connectorIds"], expected["connectorIds"]);
        assert_eq!(layout["branchGeometry"], expected["branchGeometry"]);

        for (stable_id, position) in expected["nodePositions"]
            .as_object()
            .expect("expected nodePositions object")
        {
            assert_eq!(
                &node_by_stable_id(&graph, stable_id)["position"],
                position,
                "PLCopen node {stable_id} position changed"
            );
        }
        for (stable_id, size) in expected["nodeSizes"]
            .as_object()
            .expect("expected nodeSizes object")
        {
            assert_eq!(
                &node_by_stable_id(&graph, stable_id)["size"],
                size,
                "PLCopen node {stable_id} size changed"
            );
        }

        let vendor_add_data = layout["vendorAddData"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(Value::as_str)
            .expect("PLCopen vendor addData should be preserved");
        for snippet in expected_keys(&expected, "vendorAddDataContains") {
            assert!(
                vendor_add_data.contains(&snippet),
                "PLCopen vendor addData missing snippet '{snippet}': {vendor_add_data}"
            );
        }
    }

    fn json(payload: String) -> Value {
        serde_json::from_str(&payload).expect("WASM JSON export should produce valid JSON")
    }

    fn assert_object_keys(value: &Value, keys: &[&str]) {
        for key in keys {
            assert!(
                value.get(*key).is_some(),
                "JSON object is missing expected key '{key}': {value}"
            );
        }
    }

    fn assert_export_keys(contract: &Value, export: &str, value: &Value) {
        assert_expected_object_keys(value, &expected_keys(&contract[export], "topLevel"));
    }

    fn assert_export_array_item_keys(
        contract: &Value,
        export: &str,
        value: &Value,
        array_key: &str,
    ) {
        let item = value[array_key]
            .as_array()
            .and_then(|items| items.first())
            .unwrap_or_else(|| panic!("'{array_key}' should contain at least one item: {value}"));
        assert_expected_object_keys(
            item,
            &expected_keys(&contract[export]["arrayItems"], array_key),
        );
    }

    fn assert_export_object_keys(contract: &Value, export: &str, value: &Value, object_key: &str) {
        assert_expected_object_keys(
            &value[object_key],
            &expected_keys(&contract[export]["objectKeys"], object_key),
        );
    }

    fn assert_expected_object_keys(value: &Value, keys: &[String]) {
        for key in keys {
            assert!(
                value.get(key).is_some(),
                "JSON object is missing expected key '{key}': {value}"
            );
        }
    }

    fn expected_keys(value: &Value, key: &str) -> Vec<String> {
        value[key]
            .as_array()
            .unwrap_or_else(|| panic!("contract key '{key}' should be an array: {value}"))
            .iter()
            .map(|item| {
                item.as_str()
                    .unwrap_or_else(|| panic!("contract key '{key}' contains non-string: {value}"))
                    .to_string()
            })
            .collect()
    }

    fn node_by_stable_id<'a>(graph: &'a Value, stable_id: &str) -> &'a Value {
        graph["pous"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|pou| {
                pou["networks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|network| network["nodes"].as_array().into_iter().flatten())
            })
            .find(|node| node["stableId"] == stable_id)
            .unwrap_or_else(|| panic!("graph is missing node '{stable_id}': {graph}"))
    }
}
