//! `lsty add` and `lsty addm`.

use lsty_core::errs::Result;
use lsty_core::ops::{self, AddParams, IdChoice, Place, parse_one};

use crate::commands::{Call, Written};
use crate::parse::{Opt, needs};

const ADD_SYNOPSIS: &str = "add TEXT…";
const ADDM_SYNOPSIS: &str = "addm TEXT…";

/// `lsty add`: one item from the joined arguments.
pub(crate) fn add(call: &mut Call<'_, '_>) -> Result<Written> {
    if call.parsed.args.is_empty() {
        return Err(needs(ADD_SYNOPSIS));
    }
    let texts = vec![call.parsed.args.join(" ")];
    add_texts(call, texts)
}

/// `lsty addm`: one item per line of the joined arguments.
pub(crate) fn addm(call: &mut Call<'_, '_>) -> Result<Written> {
    if call.parsed.args.is_empty() {
        return Err(needs(ADDM_SYNOPSIS));
    }
    let texts = split_lines(&call.parsed.args.join(" "));
    add_texts(call, texts)
}

/// The lines of `text` that have something other than whitespace.
fn split_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

fn add_texts(call: &Call<'_, '_>, texts: Vec<String>) -> Result<Written> {
    let params = add_params(call, texts)?;
    call.mutate(|env| ops::add(env, &params))
}

fn add_params(call: &Call<'_, '_>, texts: Vec<String>) -> Result<AddParams> {
    let mut place = placement(call)?;
    match &mut place {
        Place::After(anchor) | Place::Before(anchor) => {
            call.bind(&mut [anchor])?;
        }
        Place::End | Place::Top | Place::Beside { .. } => {
            call.bind(&mut [])?;
        }
    }
    let mut params = AddParams {
        texts,
        place,
        pri: None,
        date: call.toggle(Opt::Date, Opt::NoDate),
        id: id_choice(call),
        if_absent: call.has(Opt::IfAbsent),
        grants: Vec::new(),
    };
    // The priority is read last, as `add` itself would: a missing text
    // or a bad id is reported before a bad priority.
    params.check()?;
    params.pri = call
        .option(Opt::Pri)
        .filter(|p| !p.is_empty())
        .map(ops::parse_letter)
        .transpose()?;
    Ok(params)
}

fn id_choice(call: &Call<'_, '_>) -> IdChoice {
    match call.option(Opt::Id) {
        None => IdChoice::Implied,
        Some("") => IdChoice::Random,
        Some(id) => IdChoice::Chosen(id.to_owned()),
    }
}

fn placement(call: &Call<'_, '_>) -> Result<Place> {
    if call.has(Opt::Top) {
        return Ok(Place::Top);
    }
    if let Some(r) = call.option(Opt::After) {
        return Ok(Place::After(parse_one(call.command.name, r)?));
    }
    if let Some(r) = call.option(Opt::Before) {
        return Ok(Place::Before(parse_one(call.command.name, r)?));
    }
    Ok(Place::End)
}
