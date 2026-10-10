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

const UP_SYNOPSIS: &str = "up REFS [N]";
const DOWN_SYNOPSIS: &str = "down REFS [N]";
const REORDER_SYNOPSIS: &str = "reorder REFS POS | --after REF | --before REF";

/// The last argument as a count, and the arguments before it. A last
/// argument right after a comma belongs to the list.
fn split_count(args: &[String]) -> (&[String], Option<i64>) {
    let [.., prev, last] = args else {
        return (args, None);
    };
    if prev.ends_with(',') || !last.bytes().all(|b| b.is_ascii_digit()) {
        return (args, None);
    }
    match (last.parse::<i64>(), args.split_last()) {
        (Ok(n), Some((_, rest))) => (rest, Some(n)),
        _ => (args, None),
    }
}

/// `lsty up`: moves items above the ones over them.
pub(crate) fn up(call: &Call<'_>) -> Result<Written> {
    shift(call, UP_SYNOPSIS, ops::up)
}

/// `lsty down`: moves items below the ones under them.
pub(crate) fn down(call: &Call<'_>) -> Result<Written> {
    shift(call, DOWN_SYNOPSIS, ops::down)
}

type Shift = fn(&ops::Env<'_>, &[Target], i64) -> Result<ops::Outcome>;

fn shift(call: &Call<'_>, synopsis: &str, run: Shift) -> Result<Written> {
    let (args, n) = split_count(&call.parsed.args);
    let n = n.unwrap_or(1);
    on_refs(call, synopsis, args, |env, t| run(env, t, n))
}

/// `lsty reorder`: puts items at a position or next to another item.
pub(crate) fn reorder(call: &Call<'_>) -> Result<Written> {
    let (args, pos) = split_count(&call.parsed.args);
    let anchored = call.has(Opt::After) || call.has(Opt::Before);
    if anchored && pos.is_some() {
        return Err(usage("give a position or an anchor, not both"));
    }
    if !anchored && pos.is_none() {
        return Err(needs(REORDER_SYNOPSIS));
    }
    if pos.is_some() && !call.ops.actor.is_human() {
        return Err(Error::new(Code::Usage, "agents place items by anchor")
            .with_hint("use --after id:\u{2026}"));
    }
    let mut targets = ref_list(args)?;
    if targets.is_empty() {
        return Err(needs(REORDER_SYNOPSIS));
    }
    let mut place = placement(call, pos)?;
    let mut refs: Vec<&mut Target> = targets.iter_mut().collect();
    if let Placement::After(anchor) | Placement::Before(anchor) = &mut place {
        refs.push(anchor);
    }
    call.bind(&mut refs)?;
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
