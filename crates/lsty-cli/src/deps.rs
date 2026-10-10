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

/// The items on each side of the word `on`.
struct Sides {
    waiting: Vec<Target>,
    prereqs: Vec<Target>,
}

fn sides(call: &Call<'_>) -> Result<Sides> {
    let args = call.parsed.args.as_slice();
    let Some(at) = args.iter().position(|a| a == "on") else {
        return Err(needs());
    };
    let (left, right) = args.split_at(at);
    let right = right.get(1..).unwrap_or_default();
    if right.iter().any(|a| a == "on") {
        return Err(needs());
    }
    let mut waiting = ref_list(left)?;
    let mut prereqs = ref_list(right)?;
    if waiting.is_empty() || prereqs.is_empty() {
        return Err(needs());
    }
    let mut refs: Vec<&mut Target> = waiting.iter_mut().collect();
    refs.extend(prereqs.iter_mut());
    call.bind(&mut refs)?;
    Ok(Sides { waiting, prereqs })
}

/// `lsty dep`: makes items wait for others.
pub(crate) fn dep(call: &Call<'_>) -> Result<Written> {
    let Sides { waiting, prereqs } = sides(call)?;
    let chosen = call.option(Opt::Id).filter(|id| !id.is_empty());
    call.mutate(|env| ops::dep(env, &waiting, &prereqs, chosen))
}

/// `lsty undep`: stops items waiting for others.
pub(crate) fn undep(call: &Call<'_>) -> Result<Written> {
    let Sides { waiting, prereqs } = sides(call)?;
    call.mutate(|env| ops::undep(env, &waiting, &prereqs))
}
