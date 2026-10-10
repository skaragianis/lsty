//! The commands that move lines: `up`, `down` and `reorder`.

use lsty_core::{
    errs::{Code, Error, Result},
    ops::{self, Placement, Target, parse_one},
};

use crate::{
    commands::{Call, Written},
    parse::{Opt, needs, usage},
    reflist::{on_refs, ref_list},
};

/// The last argument as a count, and the arguments before it. A last
/// argument right after a comma belongs to the list.
fn split_count(args: &[String]) -> (&[String], Option<i64>) {
    let [rest @ .., last] = args else {
        return (args, None);
    };
    let Some(prev) = rest.last() else {
        return (args, None);
    };
    if prev.ends_with(',') || !last.bytes().all(|b| b.is_ascii_digit()) {
        return (args, None);
    }
    last.parse().map_or((args, None), |n| (rest, Some(n)))
}

/// `lsty up`: moves items above the ones over them.
pub(crate) fn up(call: &Call<'_>) -> Result<Written> {
    shift(call, ops::up)
}

/// `lsty down`: moves items below the ones under them.
pub(crate) fn down(call: &Call<'_>) -> Result<Written> {
    shift(call, ops::down)
}

type Shift = fn(&ops::Env<'_>, &[Target], i64) -> Result<ops::Outcome>;

fn shift(call: &Call<'_>, run: Shift) -> Result<Written> {
    let (args, n) = split_count(&call.parsed.args);
    let n = n.unwrap_or(1);
    on_refs(call, args, |env, t| run(env, t, n))
}

/// `lsty reorder`: puts items at a position or next to another item.
pub(crate) fn reorder(call: &Call<'_>) -> Result<Written> {
    let (args, pos) = split_count(&call.parsed.args);
    let anchored = call.has(Opt::After) || call.has(Opt::Before);
    if anchored && pos.is_some() {
        return Err(usage("give a position or an anchor, not both"));
    }
    if !anchored && pos.is_none() {
        return Err(needs());
    }
    if pos.is_some() && !call.ops.actor.is_human() {
        return Err(Error::new(Code::Usage, "agents place items by anchor")
            .with_hint("use --after id:\u{2026}"));
    }
    let mut targets = ref_list(args)?;
    if targets.is_empty() {
        return Err(needs());
    }
    let mut place = placement(call, pos)?;
    let anchor = match &mut place {
        Placement::After(anchor) | Placement::Before(anchor) => Some(anchor),
        Placement::Position(_) => None,
    };
    call.bind(targets.iter_mut().chain(anchor))?;
    call.mutate(|env| ops::reorder(env, &targets, &place))
}

/// The anchor given with `--after` or `--before`, else the position.
fn placement(call: &Call<'_>, pos: Option<i64>) -> Result<Placement> {
    if let Some(r) = call.option(Opt::After) {
        return Ok(Placement::After(parse_one(call.command.name, r)?));
    }
    if let Some(r) = call.option(Opt::Before) {
        return Ok(Placement::Before(parse_one(call.command.name, r)?));
    }
    Ok(Placement::Position(pos.unwrap_or(1)))
}
