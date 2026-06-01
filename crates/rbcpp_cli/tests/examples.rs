// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn example_files() -> Vec<PathBuf> {
    let examples_dir = workspace_root().join("examples");
    let mut files = fs::read_dir(&examples_dir)
        .expect("examples directory should exist")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "st" | "il" | "sfc" | "ld" | "fbd" | "xml"
                    )
                })
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

#[test]
fn shipped_examples_cover_supported_source_extensions() {
    let extensions = example_files()
        .into_iter()
        .filter_map(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| extension.to_ascii_lowercase())
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(
        extensions,
        BTreeSet::from([
            "il".to_string(),
            "fbd".to_string(),
            "ld".to_string(),
            "sfc".to_string(),
            "st".to_string(),
            "xml".to_string()
        ])
    );
}

#[test]
fn shipped_examples_pass_cli_check() {
    let files = example_files();
    assert!(!files.is_empty(), "expected shipped examples");

    for file in files {
        let output = Command::new(env!("CARGO_BIN_EXE_rbcpp"))
            .arg("check")
            .arg(&file)
            .output()
            .unwrap_or_else(|err| panic!("failed to run rbcpp for {}: {err}", file.display()));
        assert!(
            output.status.success(),
            "rbcpp check failed for {}\nstdout:\n{}\nstderr:\n{}",
            file.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn cli_reports_package_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_rbcpp"))
        .arg("--version")
        .output()
        .expect("failed to run rbcpp --version");

    assert!(
        output.status.success(),
        "rbcpp --version failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("rbcpp {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn plcopen_examples_import_with_cli() {
    for file in example_files()
        .into_iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "xml"))
    {
        let output = Command::new(env!("CARGO_BIN_EXE_rbcpp"))
            .arg("import-plcopen")
            .arg(&file)
            .output()
            .unwrap_or_else(|err| {
                panic!(
                    "failed to run rbcpp import-plcopen for {}: {err}",
                    file.display()
                )
            });
        assert!(
            output.status.success(),
            "rbcpp import-plcopen failed for {}\nstdout:\n{}\nstderr:\n{}",
            file.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
