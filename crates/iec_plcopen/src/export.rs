// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub fn export_plcopen_xml(project: &Project) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<project xmlns=\"{}\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\"",
        PLCOPEN_TC6_0201_NS
    ));
    if let Some(namespaces) = project.metadata.get("plcopen.rootNamespaces") {
        if !namespaces.trim().is_empty() {
            out.push(' ');
            out.push_str(namespaces.trim());
        }
    }
    out.push_str(">\n");
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

pub(crate) fn sfc_to_xml(sfc: &Sfc) -> String {
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

pub(crate) fn data_types_to_xml(project: &Project) -> String {
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

pub(crate) fn data_type_spec_to_xml(spec: &DataTypeSpec, indent: &str) -> String {
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

pub(crate) fn type_ref_to_xml(spec: &DataTypeSpec) -> String {
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

pub(crate) fn plcopen_elementary_type_tag(elementary: &ElementaryType) -> &'static str {
    match elementary {
        ElementaryType::TimeOfDay => "TOD",
        ElementaryType::DateAndTime => "DT",
        _ => elementary.as_iec(),
    }
}

pub(crate) fn configurations_to_xml(project: &Project) -> String {
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

pub(crate) fn program_instance_to_xml(program: &ProgramInstance, indent: &str) -> String {
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

pub(crate) fn program_instance_parameter_to_xml(arg: &ParamAssignment, indent: &str) -> String {
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

pub(crate) fn var_blocks_to_xml(var_blocks: &[VarBlock], indent: &str) -> String {
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

pub(crate) fn config_var_block_to_xml(block: &VarBlock, indent: &str) -> String {
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

pub(crate) fn access_var_block_to_xml(block: &VarBlock, indent: &str) -> String {
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

pub(crate) fn initial_value_to_xml(expr: &Expr, indent: &str) -> String {
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

pub(crate) fn initial_value_body_to_xml(expr: &Expr, indent: &str) -> String {
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

pub(crate) fn simple_value_to_xml(expr: &Expr) -> String {
    format!(
        "<simpleValue value=\"{}\" />",
        xml_escape(&expr_to_st(expr))
    )
}

pub(crate) fn plcopen_var_block_name(kind: VarBlockKind) -> &'static str {
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

pub(crate) fn type_name_for_xml(spec: &DataTypeSpec) -> String {
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

pub(crate) fn statements_to_st(statements: &[Statement]) -> String {
    statements
        .iter()
        .map(statement_to_st)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn statement_to_st(statement: &Statement) -> String {
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

pub(crate) fn il_op_to_st(op: IlOp) -> &'static str {
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

pub(crate) fn param_to_st(param: &ParamAssignment) -> String {
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

pub(crate) fn case_label_to_st(label: &CaseLabel) -> String {
    match label {
        CaseLabel::Single(expr) => expr_to_st(expr),
        CaseLabel::Range(low, high) => format!("{}..{}", expr_to_st(low), expr_to_st(high)),
    }
}

pub(crate) fn expr_to_st(expr: &Expr) -> String {
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

pub(crate) fn literal_to_st(literal: &Literal) -> String {
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

pub(crate) fn variable_to_st(variable: &VariableRef) -> String {
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

pub(crate) fn binary_op_to_st(op: BinaryOp) -> &'static str {
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

pub(crate) fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
