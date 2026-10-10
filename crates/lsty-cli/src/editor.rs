//! `lsty edit REF`: one item in the editor, with no lock held while it
//! is open.

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write as _},
    os::unix::{ffi::OsStringExt as _, fs::OpenOptionsExt as _},
    path::{Path, PathBuf},
};

use lsty_core::{
    errs::{self, Code, Error, Result},
    ops::{
        self, EditPlan, IdSource as _, OsIds, joined_lines, parse_one,
        prepare_edit,
    },
};

use crate::{
    commands::Call,
    env::Env,
    output::Output,
    parse::{needs, usage},
    reflist::claim,
};

/// Where temp files go when `TMPDIR` is not set.
const FALLBACK_TMP: &str = "/tmp";

/// How often a clash of temp file names is tried again.
const NAME_TRIES: usize = 8;

/// `lsty edit`.
pub(crate) fn edit(call: &Call<'_>, env: &Env<'_>) -> Result<Output> {
    let [arg] = call.parsed.args.as_slice() else {
        if call.parsed.args.is_empty() {
            return Err(needs());
        }
        return Err(usage("edit takes one reference"));
    };
    let mut target = parse_one("edit", arg)?;
    call.bind(&mut [&mut target])?;
    let snap = call.load()?;
    let plan = prepare_edit(&snap, &call.ops.actor, &target, claim(call))?;
    run_edit(call, env, &plan)
}

fn run_edit(
    call: &Call<'_>,
    env: &Env<'_>,
    plan: &EditPlan,
) -> Result<Output> {
    let path = temp_file(env, plan.raw())?;
    env.edit(&path)
        .map_err(|why| keep_text(Error::new(Code::Io, why), &path))?;
    let text = edited_text(&path).map_err(|e| keep_text(e, &path))?;
    if text.is_empty() || text == plan.raw() {
        drop(fs::remove_file(&path));
        let line = format!("{} unchanged", plan.handle());
        return Ok(Output::lines([line]));
    }
    let outcome = ops::edit(&call.ops, plan, &text, claim(call))
        .map_err(|e| keep_text(e, &path))?;
    // The edit is saved; a temp file that stays is only litter.
    drop(fs::remove_file(&path));
    Ok(Output::lines(outcome.messages()))
}

/// Writes the raw line to a new private temp file and returns its path.
fn temp_file(env: &Env<'_>, raw: &[u8]) -> Result<PathBuf> {
    let dir = match (env.vars)("TMPDIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        Some(_) | None => PathBuf::from(FALLBACK_TMP),
    };
    let (path, mut file) = create_unique(&dir)?;
    let written = file.write_all(&[raw, b"\n"].concat());
    drop(file);
    if let Err(e) = written {
        drop(fs::remove_file(&path));
        return Err(errs::io("write", &path, e));
    }
    Ok(path)
}

fn create_unique(dir: &Path) -> Result<(PathBuf, fs::File)> {
    let mut tries = 1;
    loop {
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
            Err(e)
                if e.kind() == io::ErrorKind::AlreadyExists
                    && tries < NAME_TRIES =>
            {
                tries += 1;
            }
            Err(e) => return Err(errs::io("open", &path, e)),
        }
    }
}

/// What the editor left: the lines trimmed and joined with spaces.
fn edited_text(path: &Path) -> Result<Vec<u8>> {
    match fs::read(path) {
        Ok(data) => Ok(joined_lines(&data)),
        Err(e) => Err(Error::new(
            Code::Io,
            format!(
                "read the edited text: open {}: {}",
                errs::lossy_path(path),
                errs::io_text(&e)
            ),
        )),
    }
}

/// The error with the path of the temp file in its hint, so that nothing
/// typed is lost.
pub(crate) fn keep_text(err: Error, path: &Path) -> Error {
    let shown = errs::lossy_path(path);
    let where_ = format!("your text is in {shown}");
    let hint = match err.hint() {
        Some(hint) if !hint.is_empty() => format!("{hint}; {where_}"),
        Some(_) | None => where_,
    };
    err.with_hint(hint)
}
