// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub(crate) struct PlcOpenXmlValidation {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) namespace_attributes: Vec<String>,
}

pub(crate) struct PlcOpenProjectModel {
    pub(crate) file_header: Option<String>,
    pub(crate) content_header: Option<String>,
    pub(crate) add_data: Option<String>,
    pub(crate) data_types: Vec<PlcOpenDataTypeModel>,
    pub(crate) pous: Vec<PlcOpenPouModel>,
    pub(crate) configurations: Vec<PlcOpenConfigurationModel>,
}

pub(crate) struct PlcOpenDataTypeModel {
    pub(crate) declaration: DataTypeDeclaration,
}

pub(crate) struct PlcOpenConfigurationModel {
    pub(crate) configuration: Configuration,
}

pub(crate) struct PlcOpenPouModel {
    pub(crate) name: Identifier,
    pub(crate) kind: PouKind,
    pub(crate) interface: PlcOpenInterfaceModel,
    pub(crate) body: PlcOpenBodyModel,
}

pub(crate) struct PlcOpenInterfaceModel {
    pub(crate) var_blocks: Vec<VarBlock>,
}

pub(crate) struct PlcOpenBodyModel {
    pub(crate) body: PouBody,
}

pub(crate) struct PlcOpenGraphModel {
    pub(crate) language: ImplementationLanguage,
    pub(crate) nodes: Vec<NetworkNode>,
    pub(crate) statements: Vec<Statement>,
}

impl PlcOpenDataTypeModel {
    pub(crate) fn into_declaration(self) -> DataTypeDeclaration {
        self.declaration
    }
}

impl PlcOpenConfigurationModel {
    pub(crate) fn into_configuration(self) -> Configuration {
        self.configuration
    }
}

impl PlcOpenPouModel {
    pub(crate) fn into_pou(self) -> Pou {
        let mut var_blocks = self.interface.var_blocks;
        let body = self.body.into_body();
        add_graphical_helper_vars(&mut var_blocks, &body);
        Pou {
            name: self.name,
            kind: self.kind,
            var_blocks,
            body,
        }
    }
}

impl PlcOpenBodyModel {
    pub(crate) fn empty() -> Self {
        Self {
            body: PouBody::default(),
        }
    }

    pub(crate) fn from_body(body: PouBody) -> Self {
        Self { body }
    }

    pub(crate) fn from_graph(graph: PlcOpenGraphModel) -> Self {
        Self {
            body: PouBody {
                language: graph.language,
                statements: graph.statements,
                networks: vec![Network {
                    label: None,
                    language: graph.language,
                    nodes: graph.nodes,
                }],
                sfc: None,
            },
        }
    }

    pub(crate) fn into_body(self) -> PouBody {
        self.body
    }
}
