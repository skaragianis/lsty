//! `lsty pri` and `lsty id`.

use lsty_core::errs::Result;
use lsty_core::ops::{self, PriPair, parse_refs};

use crate::commands::Call;
use crate::output::Output;
use crate::parse::{needs, too_many};

const PRI_SYNOPSIS: &str = "pri REFS PRI [REFS PRI…]";
const ID_SYNOPSIS: &str = "id REFS [ID]";

/// `lsty pri`: sets the priority of each reference.
pub fn pri(call: &mut Call<'_, '_>) -> Result<Output> {
    let args = &call.parsed.args;
    if args.is_empty() || !args.len().is_multiple_of(2) {
        return Err(needs(PRI_SYNOPSIS));
    }
    let mut pairs = Vec::new();
    for chunk in args.chunks(2) {
        let [refs, letter] = chunk else {
            return Err(needs(PRI_SYNOPSIS));
        };
        for target in parse_refs(refs)? {
            pairs.push(PriPair {
                target,
                letter: letter.clone(),
            });
        }
    }
    call.bind(
        &mut pairs.iter_mut().map(|p| &mut p.target).collect::<Vec<_>>(),
    )?;
    call.mutate(|env| ops::pri(env, &pairs))
}

/// `lsty id`: gives each reference an id, chosen or random.
pub fn id(call: &mut Call<'_, '_>) -> Result<Output> {
    if call.parsed.args.len() > 2 {
        return Err(too_many(ID_SYNOPSIS));
    }
    let bound = call.refs_and_rest(ID_SYNOPSIS, 0)?;
    let chosen = bound.rest.first().map(String::as_str);
    call.mutate(|env| ops::id(env, &bound.targets, chosen))
}
