use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use super::{Found, FoundBy, Input, TODO_NAME, locate};
use crate::errs;

const DONE_NAME: &str = "done.txt";
const REPORT_NAME: &str = "report.txt";
const DONE: &str = "DONE_FILE";
const REPORT: &str = "REPORT_FILE";

/// Whether `$DONE_FILE` and `$REPORT_FILE` count: only when the todo file
/// came from `TODO_FILE` or `TODO_DIR`, so a project list never archives
/// into a global done file.
fn global(found_by: FoundBy) -> bool {
    matches!(found_by, FoundBy::TodoFile | FoundBy::TodoDir)
}

/// The done file of `todo`, resolved.
pub(super) fn done_file(
    input: &Input<'_>,
    todo: &Path,
    found_by: FoundBy,
) -> errs::Result<PathBuf> {
    let path = match input.setting(DONE) {
        Some(set) if global(found_by) => input.abs(Path::new(&set)),
        Some(_) | None => beside(todo, DONE_NAME, "done"),
    };
    let (done, _) = locate(&path)?;
    if done == todo {
        return Err(same_file(DONE, &done, "todo"));
    }
    Ok(done)
}

/// `$REPORT_FILE`, absolute, when it counts. Only `report` looks at it.
pub(super) fn report_setting(
    input: &Input<'_>,
    found_by: FoundBy,
) -> Option<PathBuf> {
    let set = input.setting(REPORT).filter(|_| global(found_by))?;
    Some(input.abs(Path::new(&set)))
}

/// The report file of `found`, resolved: its `$REPORT_FILE`, else
/// `report.txt` beside `todo.txt` or `<name>.report.txt` beside `<name>.txt`.
///
/// # Errors
///
/// `invalid_value` when it is the todo file or the done file, and `io` when
/// it cannot be examined.
pub fn report_file(found: &Found) -> errs::Result<PathBuf> {
    let path = found
        .report_file
        .clone()
        .unwrap_or_else(|| beside(&found.todo, REPORT_NAME, "report"));
    let (report, _) = locate(&path)?;
    for (other, name) in [(&found.todo, "todo"), (&found.done, "done")] {
        if report == *other {
            return Err(same_file(REPORT, &report, name));
        }
    }
    Ok(report)
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
fn beside(todo: &Path, plain: &str, kind: &str) -> PathBuf {
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
