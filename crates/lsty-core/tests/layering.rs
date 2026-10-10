//! A module of `lsty-core` uses only modules above it in `LAYERS`.
#![expect(
    clippy::disallowed_methods,
    reason = "tests build paths of their own, which are UTF-8"
)]

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The modules of `lsty-core`, top first.
const LAYERS: &[&str] = &[
    "errs", "case", "todotxt", "dates", "fsio", "lock", "discover", "config",
    "list", "query", "lineid", "links", "program", "clip", "ops", "keys",
    "theme",
];

#[test]
fn modules_use_only_modules_above() -> io::Result<()> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut problems = Vec::new();

    for module in declared_modules(&src.join("lib.rs"))? {
        let Some(rank) = rank(&module) else {
            problems.push(format!("module {module} is not in LAYERS"));
            continue;
        };
        for file in module_files(&src, &module)? {
            let text = fs::read_to_string(&file)?;
            problems.extend(
                crate_paths(&text).iter().filter_map(|used| {
                    edge_problem(&file, &module, rank, used)
                }),
            );
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

/// Why `module` (at `rank`) may not use `used`, if it may not.
fn edge_problem(
    file: &Path,
    module: &str,
    rank: usize,
    used: &str,
) -> Option<String> {
    match self::rank(used) {
        Some(r) if r <= rank => None,
        Some(_) => Some(format!(
            "{}: {module} uses {used}, which is below it",
            file.display()
        )),
        None => Some(format!(
            "{}: {module} uses unknown module {used}",
            file.display()
        )),
    }
}

#[test]
fn crate_paths_finds_plain_and_grouped_uses() {
    let text = "use crate::errs::Error;\n\
                use crate::{dates::Date, list::{Snapshot, Item}};\n\
                fn f() -> crate::fsio::Hooks { todo!() }\n";
    assert_eq!(crate_paths(text), ["errs", "dates", "list", "fsio"]);
}

fn rank(module: &str) -> Option<usize> {
    LAYERS.iter().position(|m| *m == module)
}

/// The `mod` declarations of the crate root.
fn declared_modules(lib: &Path) -> io::Result<Vec<String>> {
    let text = fs::read_to_string(lib)?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("pub ").unwrap_or(line);
            let name = rest.strip_prefix("mod ")?.strip_suffix(';')?;
            Some(name.trim().to_owned())
        })
        .collect())
}

/// `src/<module>.rs` and every file under `src/<module>/`.
fn module_files(src: &Path, module: &str) -> io::Result<Vec<PathBuf>> {
    let mut files = vec![src.join(format!("{module}.rs"))];
    let mut dirs = vec![src.join(module)];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry?.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    files.retain(|f| f.is_file());
    Ok(files)
}

/// The top-level module of every `crate::` path in `text`, including the
/// members of a `crate::{…}` group.
fn crate_paths(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("crate::") {
        rest = &rest[at + "crate::".len()..];
        if let Some(group) = rest.strip_prefix('{') {
            let end = closing_brace(group);
            found.extend(group_heads(&group[..end]));
            rest = &group[end..];
        } else {
            found.push(ident(rest).to_owned());
        }
    }
    found.retain(|m| !m.is_empty());
    found
}

/// The index of the brace that closes a group whose `{` was just read.
fn closing_brace(group: &str) -> usize {
    let mut depth = 0usize;
    for (i, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => return i,
            '}' => depth -= 1,
            _ => {}
        }
    }
    group.len()
}

/// The first identifier of each top-level member of a group.
fn group_heads(group: &str) -> Vec<String> {
    let mut heads = Vec::new();
    let mut depth = 0usize;
    let mut start = true;
    for (i, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => start = true,
            c if start && depth == 0 && !c.is_whitespace() => {
                heads.push(ident(&group[i..]).to_owned());
                start = false;
            }
            _ => {}
        }
    }
    heads
}

fn ident(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    &text[..end]
}
