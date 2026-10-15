use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use super::{FoundBy, Input, TODO_NAME, locate};
use crate::errs;

const DONE_NAME: &str = "done.txt";
const REPORT_NAME: &str = "report.txt";

/// The done and report files of `todo`, resolved. `$DONE_FILE` and
/// `$REPORT_FILE` count only when the todo file came from `TODO_FILE` or
/// `TODO_DIR`, so a project list never archives into a global done file.
pub(super) fn companions(
    input: &Input<'_>,
    todo: &Path,
    found_by: FoundBy,
) -> errs::Result<(PathBuf, PathBuf)> {
    let global = matches!(found_by, FoundBy::TodoFile | FoundBy::TodoDir);
    let companion = |var: &str, plain: &str, kind: &str| {
        let path = match input.setting(var) {
            Some(set) if global => input.abs(Path::new(&set)),
            Some(_) | None => beside(todo, plain, kind),
        };
        locate(&path).map(|(path, _)| path)
    };
    let done = companion("DONE_FILE", DONE_NAME, "done")?;
    let report = companion("REPORT_FILE", REPORT_NAME, "report")?;
    for (var, path, other, name) in [
        ("DONE_FILE", &done, todo, "todo"),
        ("REPORT_FILE", &report, todo, "todo"),
        ("REPORT_FILE", &report, &done, "done"),
    ] {
        if path == other {
            return Err(same_file(var, path, name));
        }
    }
    Ok((done, report))
}

fn same_file(var: &str, path: &Path, name: &str) -> errs::Error {
    let shown = errs::lossy_path(path);
    let message = format!("{var} is the {name} file: {shown}");
    let accepted = format!("a path other than the {name} file");
    errs::Error::invalid_value(message, &shown, &[&accepted])
        .with_field("field", errs::Field::Str(var.to_owned()))
}

/// `plain` beside `todo.txt`, otherwise `<name>.<kind>.txt` beside the
/// file, without a `.txt` of its own.
pub(super) fn beside(todo: &Path, plain: &str, kind: &str) -> PathBuf {
    let dir = todo.parent().unwrap_or_else(|| Path::new(""));
    let name = todo.file_name().map_or(&[][..], |n| n.as_bytes());
    if name == TODO_NAME.as_bytes() {
        return dir.join(plain);
    }
    let stem = name.strip_suffix(b".txt").unwrap_or(name);
    let mut path = stem.to_vec();
    path.extend_from_slice(format!(".{kind}.txt").as_bytes());
    dir.join(OsString::from_vec(path))
}
