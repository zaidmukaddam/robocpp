// SPDX-License-Identifier: MIT OR Apache-2.0

mod calls;
mod configuration;
mod declarations;
mod expressions;
mod statements;
mod support;
mod types;

#[cfg(test)]
mod tests;

use iec_diagnostics::{Diagnostic, DiagnosticBag};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};

#[cfg(test)]
pub(crate) use support::{GenericFamily, SimpleType};

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    pub profile: EditionProfile,
    pub implementation: ImplementationParameters,
}

pub fn check_project(project: &Project, options: &CheckOptions) -> Vec<Diagnostic> {
    let mut checker = Checker {
        options: options.clone(),
        diagnostics: DiagnosticBag::new(),
    };
    checker.check(project);
    checker.diagnostics.into_vec()
}

pub(crate) struct Checker {
    pub(crate) options: CheckOptions,
    pub(crate) diagnostics: DiagnosticBag,
}
