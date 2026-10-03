use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use super::{FoundBy, Input, TODO_NAME, resolve};
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
        Some(_) | None => derived(todo),
    };
    let (done, _) = resolve(input, &path)?;
    if done == todo {
        return Err(same_file(&done));
    }
    Ok(done)
}

fn same_file(done: &Path) -> errs::Error {
    let shown = done.to_string_lossy().into_owned();
    errs::Error::new(
        errs::Code::InvalidValue,
        format!("{ENV_DONE} is the todo file: {shown}"),
    )
    .field("field", errs::Field::Str(ENV_DONE.to_owned()))
    .field("value", errs::Field::Str(shown))
    .field(
        "accepted",
        errs::Field::Strs(vec!["a path other than the todo file".to_owned()]),
    )
}

/// `done.txt` beside `todo.txt`, otherwise `<name>.done.txt` beside the
/// file, without a `.txt` of its own.
fn derived(todo: &Path) -> PathBuf {
    let dir = todo.parent().unwrap_or_else(|| Path::new(""));
    let name = todo.file_name().map_or(&[][..], |n| n.as_bytes());
    if name == TODO_NAME.as_bytes() {
        return dir.join(DONE_NAME);
    }
    let stem = name.strip_suffix(b".txt").unwrap_or(name);
    let mut done = stem.to_vec();
    done.extend_from_slice(b".done.txt");
    dir.join(OsString::from_vec(done))
}
