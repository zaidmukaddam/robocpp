// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(clippy::too_many_arguments)]

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};
use iec_syntax::parse_project;
use roxmltree::{Attribute, Document, Node, ParsingOptions};
use std::collections::{BTreeMap, BTreeSet};

pub const PLCOPEN_TC6_0201_NS: &str = "http://www.plcopen.org/xml/tc6_0201";
const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

#[derive(Debug, Clone)]
pub struct PlcOpenImport {
    pub project: Project,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Default)]
pub struct PlcOpenImportOptions {
    pub implementation: ImplementationParameters,
}

mod dom;
mod export;
mod graph;
mod import;
mod model;
mod parse;
mod sfc;
mod validate;

pub(crate) use dom::*;
pub use export::export_plcopen_xml;
pub(crate) use export::*;
pub(crate) use graph::*;
pub use import::{import_plcopen_xml, import_plcopen_xml_with_options};
pub(crate) use model::*;
pub(crate) use parse::*;
pub(crate) use sfc::*;
pub(crate) use validate::*;

#[cfg(test)]
mod tests;
