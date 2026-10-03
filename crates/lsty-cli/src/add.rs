//! `lsty add` and `lsty addm`.

use lsty_core::errs::Result;
use lsty_core::ops::{self, AddParams, IdChoice, Place, parse_one};

use crate::commands::Call;
use crate::output::Output;
use crate::parse::needs;

const ADD_SYNOPSIS: &str = "add TEXT…";
const ADDM_SYNOPSIS: &str = "addm TEXT…";

/// `lsty add`: one item from the joined arguments.
pub fn add(call: &mut Call<'_, '_>) -> Result<Output> {
    if call.parsed.args.is_empty() {
        return Err(needs(ADD_SYNOPSIS));
    }
    let texts = vec![call.parsed.args.join(" ")];
    add_texts(call, texts)
}

/// `lsty addm`: one item per line of the joined arguments.
pub fn addm(call: &mut Call<'_, '_>) -> Result<Output> {
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

fn add_texts(call: &Call<'_, '_>, texts: Vec<String>) -> Result<Output> {
    let params = add_params(call, texts)?;
    call.mutate(|env| ops::add(env, &params))
}

fn add_params(call: &Call<'_, '_>, texts: Vec<String>) -> Result<AddParams> {
    let mut place = placement(call)?;
    match &mut place {
        Place::After(anchor) | Place::Before(anchor) => {
            call.bind(&mut [anchor])?;
        }
        Place::End | Place::Top => call.bind(&mut [])?,
    }
    Ok(AddParams {
        texts,
        place,
        pri: call
            .option("pri")
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
        date: call.toggle("date", "no-date"),
        id: id_choice(call),
        if_absent: call.has("if-absent"),
    })
}

fn id_choice(call: &Call<'_, '_>) -> IdChoice {
    match call.option("id") {
        None => IdChoice::Implied,
        Some("") => IdChoice::Random,
        Some(id) => IdChoice::Chosen(id.to_owned()),
    }
}

fn placement(call: &Call<'_, '_>) -> Result<Place> {
    if call.has("top") {
        return Ok(Place::Top);
    }
    if let Some(r) = call.option("after") {
        return Ok(Place::After(parse_one(call.command.name, r)?));
    }
    if let Some(r) = call.option("before") {
        return Ok(Place::Before(parse_one(call.command.name, r)?));
    }
    Ok(Place::End)
}
