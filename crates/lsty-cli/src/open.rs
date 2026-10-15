//! `lsty open REF [N]`: a link of a task, a `file:` tag or a URL
//! in its text.

use std::{ffi::OsStr, os::unix::ffi::OsStrExt as _, path::Path};

use lsty_core::{
    errs::{Code, Error, Result, lossy, lossy_path},
    links::{HookResult, Kind, Link, classify, links},
    ops::{Target, parse_one, resolve},
    visible::visible,
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
    let all = task_links(call, &target)?;
    let started = match pick_link(&target.handle(), &all, number)? {
        Link::File(file) => open_file(env, &file.path)?,
        // One argument for the opener, never a shell or the editor.
        Link::Url(url) => (env.open_path)(Path::new(OsStr::from_bytes(url))),
    };
    started.map_err(|why| Error::new(Code::Io, why))?;
    Ok(Output::default())
}

/// Opens `path` as [`classify`] decides.
fn open_file(env: &Env<'_>, path: &Path) -> Result<HookResult> {
    Ok(match classify(path) {
        Kind::Missing(reason) => return Err(Error::new(Code::Io, reason)),
        Kind::Refused(reason) => {
            return Err(Error::new(Code::InvalidValue, reason));
        }
        Kind::Text => env.edit(path),
        Kind::Other => (env.open_path)(path),
    })
}

/// The links of the task, in the order of the line.
fn task_links(call: &Call<'_>, target: &Target) -> Result<Vec<Link>> {
    let snap = call.load()?;
    let task = resolve(&snap, target)?;
    Ok(links(&snap, task, call.home))
}

fn pick_link<'a>(
    handle: &str,
    all: &'a [Link],
    number: &[String],
) -> Result<&'a Link> {
    match (all, number) {
        ([], _) => Err(usage(format!("{handle} has no link"))),
        (_, [given]) => numbered_link(all, given),
        ([only], []) => Ok(only),
        _ => Err(usage(link_list(handle, all)).with_lines()),
    }
}

fn link_list(handle: &str, all: &[Link]) -> String {
    let mut lines =
        vec![format!("{handle} has {} links; pass a number", all.len())];
    for (i, link) in all.iter().enumerate() {
        let shown = match link {
            Link::File(file) => lossy_path(&file.path),
            Link::Url(url) => lossy(url),
        };
        lines.push(format!("  {} {}", i + 1, visible(shown.into())));
    }
    lines.join("\n")
}

fn numbered_link<'a>(all: &'a [Link], given: &str) -> Result<&'a Link> {
    let digits = given.bytes().all(|b| b.is_ascii_digit());
    let n = given.parse::<usize>().ok().filter(|_| digits);
    let picked = n.filter(|n| *n >= 1).and_then(|n| all.get(n - 1));
    picked.ok_or_else(|| {
        Error::new(
            Code::InvalidValue,
            format!("link {given} is not between 1 and {}", all.len()),
        )
    })
}
