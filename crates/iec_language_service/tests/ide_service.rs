// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::{Duration, Instant};

use iec_language_service::*;

fn counter_source() -> &'static str {
    r#"
PROGRAM Counter
VAR
    Count : INT := 0;
END_VAR
Count := Count + 1;
END_PROGRAM
"#
}

#[test]
fn source_structure_indexes_types_hover_actions_and_rename() {
    let service = LanguageService::default();
    let analysis = service.analyze_document(DocumentInput::new(
        "counter.st",
        "(* keep me *)\nPROGRAM Counter\nVAR\n    Count : INT := 0;\nEND_VAR\nCount := Missing + 1;\nEND_PROGRAM\n",
    ));

    assert!(analysis
        .source
        .tokens
        .iter()
        .any(|token| token.kind == SourceTokenKind::Comment && token.lexeme.contains("keep me")));
    assert!(analysis
        .source_map
        .objects
        .iter()
        .any(|object| object.kind == SourceMappedObjectKind::VariableDeclaration));

    let index = document_symbol_index(&analysis);
    let count = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Count")
        .and_then(|symbol| symbol.range.as_ref())
        .expect("Count range");
    let definition = index
        .definition_at("counter.st", count.start)
        .expect("definition");
    assert_eq!(definition.name, "Count");
    assert!(!index.references_for("Count").is_empty());

    let types = document_type_index(&analysis);
    assert!(types
        .entries
        .iter()
        .any(|entry| entry.name.as_deref() == Some("Count") && entry.type_name == "INT"));

    let hover = analysis.hover_at(count.start).expect("hover");
    assert!(hover.contents.contains("Variable `Count`"));

    let actions = code_actions(&analysis);
    assert!(actions
        .iter()
        .any(|action| action.title.contains("Declare local variable 'Missing'")));

    let rename = index.validate_rename("counter.st", count.start, "Total");
    assert!(rename.valid, "{}", rename.message);
    assert!(rename.edits.iter().any(|edit| edit.new_text == "Total"));

    let descriptors = diagnostic_descriptors(&analysis);
    assert!(descriptors
        .iter()
        .any(|descriptor| descriptor.subcode == "unknown-symbol"));
}

#[test]
fn workspace_merges_documents_and_exposes_call_hierarchy() {
    let service = LanguageService::default();
    let workspace = service.analyze_workspace(vec![
        DocumentInput::new(
            "lib.st",
            "FUNCTION Inc : INT\nVAR_INPUT X : INT; END_VAR\nInc := X + 1;\nEND_FUNCTION\n",
        ),
        DocumentInput::new(
            "main.st",
            "PROGRAM Main\nVAR Count : INT; END_VAR\nCount := Inc(Count);\nEND_PROGRAM\n",
        ),
    ]);

    assert_eq!(workspace_document_count(&workspace), 2);
    assert!(workspace.merged_project.find_pou("Inc").is_some());
    let symbols = workspace_symbol_index(&workspace).workspace_symbols("Inc");
    assert!(symbols.iter().any(|symbol| symbol.name == "Inc"));
    let hierarchy = call_hierarchy(&workspace);
    assert!(hierarchy
        .iter()
        .any(|item| item.name == "Main" && item.calls.contains(&"Inc".to_string())));
}

#[test]
fn incremental_cache_reuses_unchanged_documents() {
    let mut cache = IncrementalCache::new(LanguageServiceOptions::default());
    let first = cache.update_documents(
        vec![
            DocumentInput::new("a.st", "PROGRAM A END_PROGRAM"),
            DocumentInput::new("b.st", "PROGRAM B END_PROGRAM"),
        ],
        Vec::new(),
    );
    assert_eq!(first.changed_uris.len(), 2);

    let second = cache.update_documents(
        vec![DocumentInput::new(
            "a.st",
            "PROGRAM A VAR X : INT; END_VAR END_PROGRAM",
        )],
        Vec::new(),
    );
    assert_eq!(second.changed_uris, vec!["a.st".to_string()]);
    assert!(second.reused_uris.contains(&"b.st".to_string()));
}

#[test]
fn incremental_cache_rechecks_only_isolated_changed_semantic_scope() {
    let mut cache = IncrementalCache::new(LanguageServiceOptions::default());
    let first = cache.update_documents(
        vec![
            DocumentInput::new(
                "a.st",
                "PROGRAM A\nVAR Count : INT; END_VAR\nCount := Count + 1;\nEND_PROGRAM\n",
            ),
            DocumentInput::new(
                "b.st",
                "PROGRAM B\nVAR Total : INT; END_VAR\nTotal := Total + 1;\nEND_PROGRAM\n",
            ),
        ],
        Vec::new(),
    );
    assert!(!first.semantic_scope_recheck);

    let second = cache.update_documents(
        vec![DocumentInput::new(
            "a.st",
            "PROGRAM A\nVAR Count : INT; END_VAR\nCount := Missing + 1;\nEND_PROGRAM\n",
        )],
        Vec::new(),
    );

    assert!(second.semantic_scope_recheck);
    assert_eq!(second.affected_uris, vec!["a.st".to_string()]);
    assert_eq!(second.reused_uris, vec!["b.st".to_string()]);
    assert!(second
        .analysis
        .diagnostics_by_uri
        .get("a.st")
        .is_some_and(|diagnostics| diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("unknown variable 'Missing'") })));
    assert!(second
        .analysis
        .diagnostics_by_uri
        .get("b.st")
        .is_some_and(Vec::is_empty));
}

#[test]
fn incremental_cache_tracks_semantic_dependency_edges() {
    let mut cache = IncrementalCache::new(LanguageServiceOptions::default());
    let update = cache.update_documents(
        vec![
            DocumentInput::new(
                "types.st",
                r#"
TYPE
    MyType : STRUCT
        Value : INT;
    END_STRUCT;
END_TYPE
"#,
            ),
            DocumentInput::new(
                "worker.st",
                r#"
FUNCTION_BLOCK Worker
VAR_INPUT
    In : INT;
END_VAR
END_FUNCTION_BLOCK
"#,
            ),
            DocumentInput::new(
                "main.st",
                r#"
PROGRAM Main
VAR
    Instance : Worker;
    State : MyType;
END_VAR
Instance(In := State.Value);
END_PROGRAM
"#,
            ),
            DocumentInput::new(
                "config.st",
                r#"
CONFIGURATION Plant
RESOURCE Cpu ON PLC
    TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
    PROGRAM MainInst WITH Fast : Main;
END_RESOURCE
END_CONFIGURATION
"#,
            ),
            DocumentInput::new(
                "access.st",
                r#"
PROGRAM AccessDemo
VAR
    Count : INT;
END_VAR
VAR_ACCESS
    PublicCount : Count : INT READ_WRITE;
END_VAR
END_PROGRAM
"#,
            ),
            DocumentInput::new(
                "import.xml",
                r#"
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types>
    <pous>
      <pou name="ImportedProgram" pouType="program">
        <body><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml"><![CDATA[]]></xhtml:p></ST></body>
      </pou>
    </pous>
  </types>
</project>
"#,
            )
            .with_language_id("xml"),
        ],
        Vec::new(),
    );

    assert_dependency_edge(
        &update.dependency_edges,
        "main.st",
        "worker.st",
        "Worker",
        "pou",
    );
    assert_dependency_edge(
        &update.dependency_edges,
        "main.st",
        "types.st",
        "MyType",
        "type",
    );
    assert_dependency_edge(
        &update.dependency_edges,
        "config.st",
        "main.st",
        "Main",
        "pou",
    );
    assert_dependency_edge(
        &update.dependency_edges,
        "config.st",
        "config.st",
        "Fast",
        "configuration",
    );
    assert_dependency_edge(
        &update.dependency_edges,
        "access.st",
        "access.st",
        "Count",
        "accessPath",
    );
    assert_dependency_edge(
        &update.dependency_edges,
        "import.xml",
        "import.xml",
        "ImportedProgram",
        "plcopenImport",
    );
}

fn assert_dependency_edge(
    edges: &[DependencyEdge],
    from_uri: &str,
    to_uri: &str,
    to_symbol: &str,
    kind: &str,
) {
    assert!(
        edges.iter().any(|edge| {
            edge.from_uri == from_uri
                && edge.to_uri.as_deref() == Some(to_uri)
                && edge.to_symbol == to_symbol
                && edge.kind == kind
        }),
        "missing dependency edge {from_uri} -> {to_uri}::{to_symbol} ({kind}); actual edges: {edges:?}"
    );
}

#[test]
fn graph_debug_and_generated_c_metadata_are_structured() {
    let service = LanguageService::default();
    let source = r#"
PROGRAM Demo
VAR
    Count : INT := 0;
END_VAR
VAR_ACCESS
    PublicCount : Count : INT READ_WRITE;
END_VAR
Count := Count + 1;
END_PROGRAM
"#;
    let graph = service.graph_model(DocumentInput::new("demo.st", source));
    let validation = validate_graph_model(&graph);
    assert!(validation.valid, "{:?}", validation.diagnostics);

    let trace = service.debug_document(
        DocumentInput::new("demo.st", source),
        DebugOptions {
            cycles: 2,
            watches: vec!["Count".to_string()],
            access_writes: vec![DebugAccessWrite {
                cycle: 0,
                name: "PublicCount".to_string(),
                value: "10".to_string(),
            }],
            ..DebugOptions::default()
        },
    );
    assert!(trace.diagnostics.is_empty(), "{:?}", trace.diagnostics);
    assert_eq!(trace.cycles.len(), 2);
    assert!(trace.cycles[0]
        .watches
        .iter()
        .any(|watch| watch.name.eq_ignore_ascii_case("Count")));
    assert!(trace.cycles[0]
        .access_paths
        .iter()
        .any(|access| access.name == "PublicCount"));

    let artifact = service.generate_document_c_artifact(DocumentInput::new("demo.st", source));
    assert!(
        artifact.diagnostics.is_empty(),
        "{:?}",
        artifact.diagnostics
    );
    assert!(artifact
        .metadata
        .scan_entrypoints
        .iter()
        .any(|entry| entry.name == "demo_scan"));
    assert!(artifact
        .metadata
        .access_paths
        .iter()
        .any(|access| access.name == "PublicCount"));
}

#[test]
fn plcopen_layout_and_graph_validation_are_exposed() {
    let xml = r#"
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types>
    <pous>
      <pou name="LdDemo" pouType="program">
        <interface>
          <localVars>
            <variable name="Start"><type><BOOL /></type></variable>
            <variable name="Motor"><type><BOOL /></type></variable>
          </localVars>
        </interface>
        <body>
          <LD>
            <leftPowerRail localId="1" />
            <contact localId="2" variable="Start" width="30" height="20">
              <position x="10" y="20" />
              <connectionPointIn><connection refLocalId="1" /></connectionPointIn>
            </contact>
            <coil localId="3" variable="Motor">
              <connectionPointIn><connection refLocalId="2" /></connectionPointIn>
            </coil>
          </LD>
        </body>
      </pou>
    </pous>
  </types>
  <addData><data name="Vendor.Layout"><extra /></data></addData>
</project>
"#;
    let service = LanguageService::default();
    let analysis =
        service.analyze_document(DocumentInput::new("ld.xml", xml).with_language_id("xml"));
    let model = document_graph_model(&analysis);
    assert!(model.plcopen_layout.node_ids.contains(&"2".to_string()));
    assert!(model
        .plcopen_layout
        .vendor_add_data
        .iter()
        .any(|data| data.contains("Vendor.Layout")));
    assert!(validate_graph_model(&model).valid);
}

#[test]
fn formatter_and_refactor_helpers_produce_source_edits() {
    let service = LanguageService::default();
    let input = DocumentInput::new(
        "format.st",
        "PROGRAM Demo\nVAR\nCount:INT;\nEND_VAR\nCount:=Count+1;\nEND_PROGRAM\n",
    );
    let formatted = service.format_document(input.clone());
    assert!(formatted.text.contains("Count : INT;") || formatted.text.contains("Count:INT;"));
    assert!(!formatted.edits.is_empty());

    let analysis = service.analyze_document(input);
    let count = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Count")
        .and_then(|symbol| symbol.range.as_ref())
        .expect("Count range");
    let rename = rename_symbol_plan(&analysis, count.start, "Total");
    assert!(!rename.edits.is_empty());
    let change_type = change_variable_type_plan(&analysis, "Count", "DINT");
    assert!(!change_type.edits.is_empty());
    let introduced = introduce_variable_plan(&analysis, "Count+1", "NextCount", "INT");
    assert!(!introduced.edits.is_empty());
    let extracted = extract_pou_plan(&analysis, count.start, count.end, "ExtractedPou");
    assert_eq!(extracted.edits.len(), 2);
}

#[test]
fn performance_smoke_covers_large_projects_incremental_completion_plcopen_and_simulation() {
    let service = LanguageService::default();
    let start = Instant::now();
    let inputs = (0..120)
        .map(|index| {
            DocumentInput::new(
                format!("p{index}.st"),
                format!(
                    "PROGRAM P{index}\nVAR Count : INT := {index}; END_VAR\nCount := Count + 1;\nEND_PROGRAM\n"
                ),
            )
        })
        .collect::<Vec<_>>();
    let workspace = service.analyze_workspace(inputs.clone());
    assert_eq!(workspace_document_count(&workspace), 120);

    let changed_input = DocumentInput::new(
        "p0.st",
        "PROGRAM P0\nVAR Count : INT := 0; END_VAR\nCount := Count + 2;\nEND_PROGRAM\n",
    );
    let full_edit_inputs = (0..120)
        .map(|index| {
            if index == 0 {
                changed_input.clone()
            } else {
                DocumentInput::new(
                    format!("p{index}.st"),
                    format!(
                        "PROGRAM P{index}\nVAR Count : INT := {index}; END_VAR\nCount := Count + 1;\nEND_PROGRAM\n"
                    ),
                )
            }
        })
        .collect::<Vec<_>>();
    let full_edit_start = Instant::now();
    let full_edit_workspace = service.analyze_workspace(full_edit_inputs);
    let full_edit_elapsed = full_edit_start.elapsed();
    assert_eq!(workspace_document_count(&full_edit_workspace), 120);

    let completion_start = Instant::now();
    let completions = workspace_symbol_index(&workspace).workspace_symbols("P1");
    assert!(!completions.is_empty());
    assert!(completion_start.elapsed().as_secs() < 5);

    let mut cache = IncrementalCache::new(LanguageServiceOptions::default());
    let update = cache.update_documents(inputs, Vec::new());
    assert_eq!(update.changed_uris.len(), 120);
    let incremental_edit_start = Instant::now();
    let rapid = cache.update_documents(vec![changed_input], Vec::new());
    let incremental_edit_elapsed = incremental_edit_start.elapsed();
    assert_eq!(rapid.changed_uris, vec!["p0.st".to_string()]);
    assert_eq!(rapid.reused_uris.len(), 119);
    assert!(
        incremental_edit_elapsed
            <= full_edit_elapsed.saturating_mul(5) + Duration::from_millis(100),
        "incremental edit took {:?}, full workspace edit took {:?}",
        incremental_edit_elapsed,
        full_edit_elapsed
    );

    let plcopen = r#"<project xmlns="http://www.plcopen.org/xml/tc6_0201"><types><pous><pou name="XmlDemo" pouType="program"><interface /><body><ST><xhtml xmlns="http://www.w3.org/1999/xhtml"></xhtml></ST></body></pou></pous></types></project>"#;
    let plcopen_start = Instant::now();
    let imported =
        service.analyze_document(DocumentInput::new("xml.xml", plcopen).with_language_id("xml"));
    assert!(imported.project.find_pou("XmlDemo").is_some());
    assert!(plcopen_start.elapsed().as_secs() < 5);

    let simulation =
        service.simulate_document(DocumentInput::new("counter.st", counter_source()), 25);
    assert_eq!(simulation.cycles.len(), 25);
    assert!(simulation.to_json().len() < 200_000);
    assert!(start.elapsed().as_secs() < 10);
}

#[test]
fn malformed_and_incomplete_inputs_remain_recoverable_and_idempotent() {
    let service = LanguageService::default();
    let samples = [
        "PROGRAM Half\nVAR\n    A : INT\n",
        "PROGRAM Bad\nA := ;\nEND_PROGRAM\n",
        "FUNCTION_BLOCK Fb\nVAR_INPUT Run : BOOL; END_VAR\n",
        "<project><types><pous><pou name=\"Broken\"",
    ];
    for (index, sample) in samples.iter().enumerate() {
        let uri = if sample.starts_with('<') {
            format!("fuzz{index}.xml")
        } else {
            format!("fuzz{index}.st")
        };
        let analysis = service.analyze_document(DocumentInput::new(uri.clone(), *sample));
        assert_eq!(analysis.uri, uri);
        assert!(!analysis.source.tokens.is_empty());
        let index = document_symbol_index(&analysis);
        assert!(index.to_json().contains("definitions"));
        let formatted_once = service.format_document(DocumentInput::new(uri.clone(), *sample));
        let formatted_twice =
            service.format_document(DocumentInput::new(uri, formatted_once.text.clone()));
        assert_eq!(formatted_once.text, formatted_twice.text);
    }
}
