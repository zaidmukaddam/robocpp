// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

#[test]
fn imports_simple_plcopen_st() {
    let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
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
fn imports_prefixed_plcopen_elements_and_single_quoted_attributes() {
    let xml = r#"
            <plc:project xmlns:plc='http://www.plcopen.org/xml/tc6_0201' xmlns:xhtml='http://www.w3.org/1999/xhtml'>
              <plc:types><plc:pous>
                <plc:pou name='PrefixedDemo' pouType='program'>
                  <plc:body><plc:ST><xhtml:p>A := 1;</xhtml:p></plc:ST></plc:body>
                </plc:pou>
              </plc:pous></plc:types>
            </plc:project>
        "#;
    let imported = import_plcopen_xml("prefixed.xml", xml);
    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    assert!(imported
        .project
        .pous()
        .any(|pou| pou.name.original == "PrefixedDemo"));
}

#[test]
fn dom_lowering_matches_canonicalized_xml_import_shape() {
    let xml = r#"
            <plc:project xmlns:plc='http://www.plcopen.org/xml/tc6_0201'
                         xmlns:xhtml="http://www.w3.org/1999/xhtml"
                         xmlns:vendor='urn:vendor'>
              <plc:types>
                <plc:pous>
                  <plc:pou name='DomCheck' pouType='program'>
                    <plc:interface>
                      <plc:localVars>
                        <plc:variable name='A'><plc:type><plc:INT /></plc:type></plc:variable>
                        <plc:variable name='B'><plc:type><plc:INT /></plc:type></plc:variable>
                        <plc:variable name='C'><plc:type><plc:INT /></plc:type></plc:variable>
                      </plc:localVars>
                    </plc:interface>
                    <plc:body>
                      <plc:FBD>
                        <plc:inVariable localId='1'><plc:expression>A</plc:expression></plc:inVariable>
                        <plc:inVariable localId='2'><plc:expression>B</plc:expression></plc:inVariable>
                        <plc:block localId='3' typeName='ADD'>
                          <plc:inputVariables>
                            <plc:variable formalParameter='IN1'><plc:connectionPointIn><plc:connection refLocalId='1' /></plc:connectionPointIn></plc:variable>
                            <plc:variable formalParameter='IN2'><plc:connectionPointIn><plc:connection refLocalId='2' /></plc:connectionPointIn></plc:variable>
                          </plc:inputVariables>
                        </plc:block>
                        <plc:outVariable localId='4'><plc:expression>C</plc:expression><plc:connectionPointIn><plc:connection refLocalId='3' /></plc:connectionPointIn></plc:outVariable>
                      </plc:FBD>
                    </plc:body>
                    <plc:addData>
                      <plc:data name='urn:vendor:payload' handleUnknown='preserve'>
                        <vendor:Payload key='value' />
                      </plc:data>
                    </plc:addData>
                  </plc:pou>
                </plc:pous>
              </plc:types>
            </plc:project>
        "#;
    let document = Document::parse(xml).expect("fixture XML");
    let namespaces = XmlNamespaceRegistry::from_document(document.root_element());
    let canonical = canonicalize_plcopen_xml(document.root_element(), &namespaces);

    let imported = import_plcopen_xml("raw.xml", xml);
    let canonical_imported = import_plcopen_xml("canonical.xml", &canonical);

    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    assert!(
        canonical_imported.diagnostics.is_empty(),
        "{:?}",
        canonical_imported.diagnostics
    );
    assert_eq!(
        format!("{:#?}", imported.project.library_elements),
        format!("{:#?}", canonical_imported.project.library_elements)
    );
}

#[test]
fn preserves_vendor_namespaces_declared_below_project_root() {
    let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
              <types><pous>
                <pou name="Demo" pouType="program">
                  <body><ST><xhtml:p>A := 1;</xhtml:p></ST></body>
                </pou>
              </pous></types>
              <addData>
                <data name="RobotCo.NestedNamespace">
                  <vendor:Envelope xmlns:vendor="urn:robotco:plcopen">
                    <vendor:Flag enabled="true" />
                  </vendor:Envelope>
                </data>
              </addData>
            </project>
        "#;
    let imported = import_plcopen_xml("nested-namespace.xml", xml);
    assert!(
        imported.diagnostics.is_empty(),
        "{:?}",
        imported.diagnostics
    );
    assert!(imported
        .project
        .metadata
        .get("plcopen.addData")
        .is_some_and(|data| data.contains("<vendor:Flag enabled=\"true\" />")));

    let exported = export_plcopen_xml(&imported.project);
    assert!(exported.contains("xmlns:vendor=\"urn:robotco:plcopen\""));
    let reimported = import_plcopen_xml("nested-namespace-roundtrip.xml", &exported);
    assert!(
        reimported.diagnostics.is_empty(),
        "{:?}",
        reimported.diagnostics
    );
}

#[test]
fn rejects_dtd_and_entity_declarations() {
    let xml = r#"<!DOCTYPE project [<!ENTITY ext SYSTEM "file:///etc/passwd">]>
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types>&ext;</types>
            </project>
        "#;
    let imported = import_plcopen_xml("dtd.xml", xml);
    assert!(imported.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("PLCopen XML DTD declarations are not supported")));
    assert_eq!(imported.project.pous().count(), 0);
}

#[test]
fn rejects_unknown_namespace_prefixes_before_import() {
    let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Demo" pouType="program">
                  <body><ST><xhtml:p>A := 1;</xhtml:p></ST></body>
                </pou>
              </pous></types>
            </project>
        "#;
    let imported = import_plcopen_xml("unknown-namespace.xml", xml);
    assert!(imported.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown namespace prefix 'xhtml'")));
    assert_eq!(imported.project.pous().count(), 0);
}

#[test]
fn enforces_plcopen_xml_nesting_and_text_limits() {
    let nested =
        r#"<project xmlns="http://www.plcopen.org/xml/tc6_0201"><types><pous /></types></project>"#;
    let imported = import_plcopen_xml_with_options(
        "deep.xml",
        nested,
        &PlcOpenImportOptions {
            implementation: ImplementationParameters {
                max_plcopen_xml_depth: 2,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(imported.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("PLCopen XML nesting depth 3 exceeds maximum 2")));

    let text = r#"<project xmlns="http://www.plcopen.org/xml/tc6_0201">abcdef</project>"#;
    let imported = import_plcopen_xml_with_options(
        "text.xml",
        text,
        &PlcOpenImportOptions {
            implementation: ImplementationParameters {
                max_plcopen_xml_text_bytes: 4,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(imported.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("PLCopen XML text node is 6 bytes, exceeding maximum 4")));

    let attr =
        r#"<project xmlns="http://www.plcopen.org/xml/tc6_0201"><types name="abcdef" /></project>"#;
    let imported = import_plcopen_xml_with_options(
        "attr.xml",
        attr,
        &PlcOpenImportOptions {
            implementation: ImplementationParameters {
                max_plcopen_xml_attribute_bytes: 4,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(imported.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("PLCopen XML attribute 'name' is 6 bytes, exceeding maximum 4")));

    let imported = import_plcopen_xml_with_options(
        "nodes.xml",
        nested,
        &PlcOpenImportOptions {
            implementation: ImplementationParameters {
                max_plcopen_xml_nodes: 2,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(imported
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("nodes limit reached")));
}

#[test]
fn enforces_plcopen_array_repetition_limit_during_import() {
    let xml = r#"
            <project xmlns="http://www.plcopen.org/xml/tc6_0201">
              <types><pous>
                <pou name="Demo" pouType="program">
                  <interface>
                    <localVars>
                      <variable name="Values">
                        <type><array><dimension lower="1" upper="5" /><baseType><INT /></baseType></array></type>
                        <initialValue>
                          <arrayValue><value repetitionValue="5"><simpleValue value="1" /></value></arrayValue>
                        </initialValue>
                      </variable>
                    </localVars>
                  </interface>
                </pou>
              </pous></types>
            </project>
        "#;
    let imported = import_plcopen_xml_with_options(
        "array-repeat.xml",
        xml,
        &PlcOpenImportOptions {
            implementation: ImplementationParameters {
                max_array_elements: 4,
                ..ImplementationParameters::default()
            },
        },
    );
    assert!(imported.diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("PLCopen array repetitionValue 5 exceeds maximum 4")));
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
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
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
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml" xmlns:vendor="urn:robotco:plcopen">
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
            <project xmlns="http://www.plcopen.org/xml/tc6_0201" xmlns:xhtml="http://www.w3.org/1999/xhtml">
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
    assert!(
        xml.contains("<task name=\"Fast\" interval=\"T#10ms\" single=\"Trigger\" priority=\"2\">")
    );
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
    assert!(xml.contains("<parameter name=\"Enable\" direction=\"input\" expression=\"TRUE\" />"));
    assert!(xml
        .contains("<parameter name=\"Setpoint\" direction=\"input\" expression=\"ADD(2, 3)\" />"));
    assert!(xml.contains("<parameter name=\"Count\" direction=\"output\" target=\"Observed\" />"));

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
        let second = statements_to_st(&reparsed.project.first_program().unwrap().body.statements);
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
