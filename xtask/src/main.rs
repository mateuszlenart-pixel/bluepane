//! `cargo xtask ci` runs the same checks locally as the GitHub Actions workflow:
//! formatting, clippy, tests and the crate layering rules.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Which workspace crates each crate may depend on. Anything else fails CI.
///
/// ```text
/// app, cli -> ui-egui (app only) -> engine -> ops -> (nothing)
///                                         \-> vfs
/// ```
const ALLOWED: &[(&str, &[&str])] = &[
    ("bluepane-vfs", &[]),
    ("bluepane-ops", &[]),
    ("bluepane-archive", &["bluepane-ops"]),
    ("bluepane-sync", &["bluepane-ops"]),
    ("bluepane-rename", &["bluepane-ops"]),
    ("bluepane-remote", &["bluepane-ops", "bluepane-vfs"]),
    ("bluepane-netscan", &["bluepane-ops", "bluepane-vfs"]),
    (
        "bluepane-search",
        &["bluepane-ops", "bluepane-vfs", "bluepane-archive"],
    ),
    (
        "bluepane-engine",
        &[
            "bluepane-vfs",
            "bluepane-ops",
            "bluepane-archive",
            "bluepane-sync",
            "bluepane-rename",
            "bluepane-search",
            "bluepane-remote",
            "bluepane-netscan",
        ],
    ),
    ("bluepane-ui-egui", &["bluepane-engine"]),
    ("bluepane", &["bluepane-engine", "bluepane-ui-egui"]),
    ("bluepane-cli", &["bluepane-engine", "bluepane-vfs"]),
    ("xtask", &[]),
];

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("ci") => ci(),
        Some("layering") => report(check_layering()),
        _ => {
            eprintln!("użycie: cargo xtask <ci|layering>");
            ExitCode::from(2)
        }
    }
}

fn ci() -> ExitCode {
    let steps: &[(&str, &[&str])] = &[
        ("fmt", &["fmt", "--all", "--", "--check"]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("test", &["test", "--workspace"]),
    ];
    for (name, args) in steps {
        eprintln!("==> {name}");
        let ok = Command::new("cargo")
            .args(*args)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("xtask: krok `{name}` nie przeszedł");
            return ExitCode::FAILURE;
        }
    }
    eprintln!("==> layering");
    report(check_layering())
}

fn report(result: Result<(), Vec<String>>) -> ExitCode {
    match result {
        Ok(()) => {
            eprintln!("xtask: OK");
            ExitCode::SUCCESS
        }
        Err(problems) => {
            for p in problems {
                eprintln!("xtask: {p}");
            }
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the workspace root")
        .to_path_buf()
}

fn manifests(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.join("xtask/Cargo.toml")];
    for group in ["crates", "apps"] {
        let Ok(dir) = fs::read_dir(root.join(group)) else {
            continue;
        };
        for entry in dir.flatten() {
            let manifest = entry.path().join("Cargo.toml");
            if manifest.is_file() {
                out.push(manifest);
            }
        }
    }
    out.sort();
    out
}

/// Reads `[dependencies]` of a manifest: (package name, workspace crates it depends on).
/// Only what this workspace's manifests use is understood: `name = "..."` and
/// `bluepane-x.workspace = true` / `bluepane-x = { ... }` lines.
fn parse_manifest(text: &str) -> (String, Vec<String>) {
    let mut name = String::new();
    let mut deps = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            section = match line {
                "[package]" => "package",
                "[dependencies]" => "dependencies",
                _ => "other",
            };
            continue;
        }
        match section {
            "package" if line.starts_with("name") => {
                if let Some(value) = line.split('"').nth(1) {
                    name = value.to_string();
                }
            }
            "dependencies" if line.starts_with("bluepane") => {
                let dep: String = line
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                deps.push(dep);
            }
            _ => {}
        }
    }
    (name, deps)
}

fn check_layering() -> Result<(), Vec<String>> {
    let root = workspace_root();
    let mut problems = Vec::new();
    for manifest in manifests(&root) {
        let text = fs::read_to_string(&manifest).map_err(|e| vec![format!("{manifest:?}: {e}")])?;
        let (name, deps) = parse_manifest(&text);
        let Some((_, allowed)) = ALLOWED.iter().find(|(n, _)| *n == name) else {
            problems.push(format!(
                "crate `{name}` nie ma wpisu w ALLOWED (xtask/src/main.rs) — dodaj go razem z regułami zależności"
            ));
            continue;
        };
        for dep in deps {
            if !allowed.contains(&dep.as_str()) {
                problems.push(format!(
                    "`{name}` nie może zależeć od `{dep}` (zasady warstw w CLAUDE.md)"
                ));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_and_internal_deps() {
        let text = r#"
[package]
name = "bluepane-engine"

[dependencies]
bluepane-vfs.workspace = true
egui = "0.36"

[dev-dependencies]
bluepane-ops.workspace = true
"#;
        let (name, deps) = parse_manifest(text);
        assert_eq!(name, "bluepane-engine");
        assert_eq!(deps, ["bluepane-vfs"]);
    }

    #[test]
    fn current_workspace_respects_layering() {
        assert_eq!(check_layering(), Ok(()));
    }
}
