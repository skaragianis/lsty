//! `lsty dep` and `lsty undep`: `REFS on REFS`.

use lsty_core::{
    errs::Result,
    ops::{self, Target},
};

use crate::{
    commands::{Call, Written},
    parse::{Opt, needs},
    reflist::ref_list,
};

/// The tasks on each side of the word `on`.
struct Sides {
    waiting: Vec<Target>,
    prereqs: Vec<Target>,
}

fn sides(call: &Call<'_>) -> Result<Sides> {
    let mut parts = call.parsed.args.split(|a| a == "on");
    let (Some(left), Some(right), None) =
        (parts.next(), parts.next(), parts.next())
    else {
        return Err(needs());
    };
    let mut waiting = ref_list(left)?;
    let mut prereqs = ref_list(right)?;
    if waiting.is_empty() || prereqs.is_empty() {
        return Err(needs());
    }
    call.bind(waiting.iter_mut().chain(&mut prereqs))?;
    Ok(Sides { waiting, prereqs })
}

/// `lsty dep`: makes tasks wait for others.
pub(crate) fn dep(call: &Call<'_>) -> Result<Written> {
    let Sides { waiting, prereqs } = sides(call)?;
    let chosen = call.option(Opt::Id).filter(|id| !id.is_empty());
    call.mutate(|env| ops::dep(env, &waiting, &prereqs, chosen))
}

/// `lsty undep`: stops tasks waiting for others.
pub(crate) fn undep(call: &Call<'_>) -> Result<Written> {
    let Sides { waiting, prereqs } = sides(call)?;
    call.mutate(|env| ops::undep(env, &waiting, &prereqs))
}
