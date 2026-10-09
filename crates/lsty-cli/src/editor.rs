//! `lsty edit REF`: one item in the editor, with no lock held while it
//! is open.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::ffi::OsStringExt as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use lsty_core::errs::{self, Code, Error, Field, Result};
use lsty_core::ops::{
    self, EditPlan, IdSource as _, OsIds, joined_lines, parse_one,
    prepare_edit,
};

use crate::commands::Call;
use crate::output::Output;
use crate::parse::{needs, usage};
use crate::reflist::claim;

const EDIT_SYNOPSIS: &str = "edit REF";

/// Where temp files go when `TMPDIR` is not set.
const FALLBACK_TMP: &str = "/tmp";

/// How often a clash of temp file names is tried again.
const NAME_TRIES: usize = 8;

/// `lsty edit`.
pub(crate) fn edit(call: &mut Call<'_, '_>) -> Result<Output> {
    let [arg] = call.parsed.args.as_slice() else {
        if call.parsed.args.is_empty() {
            return Err(needs(EDIT_SYNOPSIS));
        }
        return Err(usage("edit takes one reference"));
    };
    let mut target = parse_one("edit", arg)?;
    call.bind(&mut [&mut target])?;
    let snap = call.load()?;
    let plan = prepare_edit(&snap, &call.ops.actor, &target, claim(call))?;
    run_edit(call, &plan)
}

fn run_edit(call: &mut Call<'_, '_>, plan: &EditPlan) -> Result<Output> {
    let path = temp_file(call, plan.raw())?;
    call.env
        .edit(&path)
        .map_err(|why| keep_text(Error::new(Code::Io, why), &path))?;
    let text = edited_text(&path).map_err(|e| keep_text(e, &path))?;
    if text.is_empty() || text == plan.raw() {
        discard(&path)?;
        let line = format!("{} unchanged", plan.handle());
        return Ok(lines_of(vec![line.into_bytes()]));
    }
    let outcome = ops::edit(&call.ops, plan, &text, claim(call))
        .map_err(|e| keep_text(e, &path))?;
    discard(&path)?;
    Ok(lines_of(outcome.messages().to_vec()))
}

fn lines_of(lines: Vec<Vec<u8>>) -> Output {
    Output {
        fields: Vec::new(),
        lines,
    }
}

/// Writes the raw line to a new private temp file and returns its path.
fn temp_file(call: &Call<'_, '_>, raw: &[u8]) -> Result<PathBuf> {
    let dir = match (call.env.vars)("TMPDIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        Some(_) | None => PathBuf::from(FALLBACK_TMP),
    };
    let (path, mut file) = create_unique(&dir)?;
    let written = file.write_all(&[raw, b"\n"].concat());
    drop(file);
    if let Err(e) = written {
        let _removed = fs::remove_file(&path);
        return Err(errs::io("write", &path, e));
    }
    Ok(path)
}

fn create_unique(dir: &Path) -> Result<(PathBuf, fs::File)> {
    let mut last = None;
    for _ in 0..NAME_TRIES {
        let mut name = OsString::from("lsty-edit-");
        name.push(OsString::from_vec(OsIds.next(10)?.into_bytes()));
        name.push(".txt");
        let path = dir.join(name);
        let made = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path);
        match made {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                last = Some(errs::io("open", &path, e));
            }
            Err(e) => return Err(errs::io("open", &path, e)),
        }
    }
    Err(last.unwrap_or_else(|| Error::new(Code::Io, "no temp file name")))
}

/// What the editor left: the lines trimmed and joined with spaces.
fn edited_text(path: &Path) -> Result<Vec<u8>> {
    match fs::read(path) {
        Ok(data) => Ok(joined_lines(&data)),
        Err(e) => {
            let inner = errs::io("open", path, e);
            Err(Error::new(
                Code::Io,
                format!("read the edited text: {}", inner.message()),
            ))
        }
    }
}

fn discard(path: &Path) -> Result<()> {
    fs::remove_file(path).map_err(|e| errs::io("remove", path, e))
}

/// The error with the path of the temp file in its hint and in its
/// `temp_file` field, so that nothing typed is lost.
pub(crate) fn keep_text(err: Error, path: &Path) -> Error {
    let shown = errs::lossy_path(path);
    let where_ = format!("your text is in {shown}");
    let hint = match err.hint() {
        Some(hint) if !hint.is_empty() => format!("{hint}; {where_}"),
        Some(_) | None => where_,
    };
    err.with_hint(hint)
        .with_field("temp_file", Field::Str(shown))
}
