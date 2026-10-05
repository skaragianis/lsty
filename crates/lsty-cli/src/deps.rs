//! `lsty dep` and `lsty undep`: `REFS on REFS`.

use lsty_core::errs::Result;
use lsty_core::ops::{self, Target};

use crate::commands::{Call, Written};
use crate::parse::{Opt, needs};
use crate::reflist::ref_list;

const DEP_SYNOPSIS: &str = "dep REFS on REFS";
const UNDEP_SYNOPSIS: &str = "undep REFS on REFS";

/// The items on each side of the word `on`.
struct Sides {
    waiting: Vec<Target>,
    prereqs: Vec<Target>,
}

fn sides(call: &Call<'_, '_>, synopsis: &str) -> Result<Sides> {
    let args = call.parsed.args.as_slice();
    let Some(at) = args.iter().position(|a| a == "on") else {
        return Err(needs(synopsis));
    };
    let (left, right) = args.split_at(at);
    let right = right.get(1..).unwrap_or_default();
    if right.iter().any(|a| a == "on") {
        return Err(needs(synopsis));
    }
    let mut waiting = ref_list(left)?;
    let mut prereqs = ref_list(right)?;
    if waiting.is_empty() || prereqs.is_empty() {
        return Err(needs(synopsis));
    }
    let mut refs: Vec<&mut Target> = waiting.iter_mut().collect();
    refs.extend(prereqs.iter_mut());
    call.bind(&mut refs)?;
    Ok(Sides { waiting, prereqs })
}

/// `lsty dep`: makes items wait for others.
pub(crate) fn dep(call: &mut Call<'_, '_>) -> Result<Written> {
    let Sides { waiting, prereqs } = sides(call, DEP_SYNOPSIS)?;
    let chosen = call.option(Opt::Id).filter(|id| !id.is_empty());
    call.mutate(|env| ops::dep(env, &waiting, &prereqs, chosen))
}

/// `lsty undep`: stops items waiting for others.
pub(crate) fn undep(call: &mut Call<'_, '_>) -> Result<Written> {
    let Sides { waiting, prereqs } = sides(call, UNDEP_SYNOPSIS)?;
    call.mutate(|env| ops::undep(env, &waiting, &prereqs))
}
