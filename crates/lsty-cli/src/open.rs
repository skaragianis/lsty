//! `lsty open REF [N]`: a `file:` link of an item.

use std::path::PathBuf;

use lsty_core::{
    errs::{Code, Error, Result, lossy_path},
    links::{Kind, classify, linked_files},
    ops::{Target, parse_one, resolve},
};

use crate::{
    commands::Call,
    env::Env,
    output::Output,
    parse::{needs, usage},
};

/// `lsty open`.
pub(crate) fn open(call: &Call<'_>, env: &Env<'_>) -> Result<Output> {
    let args = call.parsed.args.as_slice();
    let Some((arg, number)) = args.split_first() else {
        return Err(needs());
    };
    if number.len() > 1 {
        return Err(usage("open takes a reference and a number"));
    }
    let mut target = parse_one("open", arg)?;
    call.bind([&mut target])?;
    let paths = link_paths(call, &target)?;
    let path = pick_link(&target.handle(), &paths, number)?;
    let started = match classify(&path) {
        Kind::Missing(reason) => return Err(Error::new(Code::Io, reason)),
        Kind::Refused(reason) => {
            return Err(Error::new(Code::InvalidValue, reason));
        }
        Kind::Text => env.edit(&path),
        Kind::Other => (env.open_path)(&path),
    };
    started.map_err(|why| Error::new(Code::Io, why))?;
    Ok(Output::default())
}

/// The paths of the links of the item, in the order of the line.
fn link_paths(call: &Call<'_>, target: &Target) -> Result<Vec<PathBuf>> {
    let snap = call.load()?;
    let item = resolve(&snap, target)?;
    let files = linked_files(&snap, item, call.home);
    Ok(files.into_iter().map(|file| file.path).collect())
}

fn pick_link(
    handle: &str,
    paths: &[PathBuf],
    number: &[String],
) -> Result<PathBuf> {
    match (paths, number) {
        ([], _) => Err(usage(format!("{handle} has no file: link"))),
        (_, [given]) => numbered_link(paths, given),
        ([only], []) => Ok(only.clone()),
        _ => Err(usage(link_list(handle, paths))),
    }
}

fn link_list(handle: &str, paths: &[PathBuf]) -> String {
    let mut lines =
        vec![format!("{handle} has {} links; pass a number", paths.len())];
    for (i, path) in paths.iter().enumerate() {
        lines.push(format!("  {} {}", i + 1, lossy_path(path)));
    }
    lines.join("\n")
}

fn numbered_link(paths: &[PathBuf], given: &str) -> Result<PathBuf> {
    let digits = given.bytes().all(|b| b.is_ascii_digit());
    let n = given.parse::<usize>().ok().filter(|_| digits);
    let picked = n.filter(|n| *n >= 1).and_then(|n| paths.get(n - 1));
    picked.cloned().ok_or_else(|| {
        Error::new(
            Code::InvalidValue,
            format!("link {given} is not between 1 and {}", paths.len()),
        )
    })
}
