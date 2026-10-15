use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use super::{FoundBy, Input, TODO_NAME, locate};
use crate::errs;

const DONE_NAME: &str = "done.txt";
const ENV_DONE: &str = "DONE_FILE";

/// The done file of `todo`, resolved. `$DONE_FILE` counts only when the
/// todo file came from `TODO_FILE` or `TODO_DIR`, so a project list never
/// archives into a global done file.
pub(super) fn done_file(
    input: &Input<'_>,
    todo: &Path,
    found_by: FoundBy,
) -> errs::Result<PathBuf> {
    let global = matches!(found_by, FoundBy::TodoFile | FoundBy::TodoDir);
    let path = match input.var(ENV_DONE) {
        Some(env) if global => input.abs(Path::new(&env)),
        Some(_) | None => beside(todo, DONE_NAME, "done"),
    };
    let (done, _) = locate(&path)?;
    if done == todo {
        return Err(same_file(&done));
    }
    Ok(done)
}

fn same_file(done: &Path) -> errs::Error {
    let shown = errs::lossy_path(done);
    let message = format!("{ENV_DONE} is the todo file: {shown}");
    let accepted = ["a path other than the todo file"];
    errs::Error::invalid_value(message, &shown, &accepted)
        .with_field("field", errs::Field::Str(ENV_DONE.to_owned()))
}

/// The report file of `todo`: `report.txt` beside `todo.txt`, otherwise
/// `<name>.report.txt` beside the file.
#[must_use]
pub fn report_file(todo: &Path) -> PathBuf {
    beside(todo, "report.txt", "report")
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
