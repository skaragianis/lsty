//! A module of `lsty-core` uses only modules above it in the order of
//! `lib.rs`.
#![expect(
    clippy::disallowed_methods,
    reason = "tests build paths of their own, which are UTF-8"
)]

use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[test]
fn modules_use_only_modules_above() -> io::Result<()> {
    let src = lsty_scripttest::workspace_root().join("crates/lsty-core/src");
    let mut problems = Vec::new();

    let modules = declared_modules(&fs::read_to_string(src.join("lib.rs"))?);
    for (rank, module) in modules.iter().enumerate() {
        for file in module_files(&src, module)? {
            let text = fs::read_to_string(&file)?;
            problems.extend(
                crate_paths(&text).iter().filter_map(|used| {
                    edge_problem(&file, &modules, rank, used)
                }),
            );
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

/// Why `modules[rank]` may not use `used`, if it may not.
fn edge_problem(
    file: &Path,
    modules: &[String],
    rank: usize,
    used: &str,
) -> Option<String> {
    let module = &modules[rank];
    match modules.iter().position(|m| m == used) {
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
fn declared_modules_ignore_visibility() {
    let lib = "pub mod a;\npub(crate) mod b;\nmod c;\n// mod d\n";
    assert_eq!(declared_modules(lib), ["a", "b", "c"]);
}

#[test]
fn crate_paths_finds_plain_and_grouped_uses() {
    let text = "use crate::errs::Error;\n\
                use crate::{dates::Date, list::{Snapshot, Task}};\n\
                fn f() -> crate::fsio::Hooks { todo!() }\n";
    assert_eq!(crate_paths(text), ["errs", "dates", "list", "fsio"]);
}

/// The `mod` declarations of the crate root.
fn declared_modules(lib: &str) -> Vec<String> {
    lib.lines()
        .filter_map(|line| {
            let (_, name) =
                line.trim().strip_suffix(';')?.rsplit_once("mod ")?;
            Some(name.trim().to_owned())
        })
        .collect()
}

/// `src/<module>.rs` and every file under `src/<module>/`.
fn module_files(src: &Path, module: &str) -> io::Result<Vec<PathBuf>> {
    let mut files = vec![src.join(format!("{module}.rs"))];
    let mut dirs = vec![src.join(module)];
    while let Some(dir) = dirs.pop() {
        let entries = match fs::read_dir(&dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            entries => entries?,
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
/// members of a `crate::{…}` group; tasks of the crate root are no module.
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
    found.retain(|m| m.starts_with(char::is_lowercase));
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
