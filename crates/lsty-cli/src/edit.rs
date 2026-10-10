//! The commands that change items already in the list: `append`,
//! `prepend`, `replace`, `del`, `pri`, `depri`, `due`, `id`, `tag` and
//! `untag`.

use lsty_core::{
    errs::Result,
    ops::{self, PriPair, parse_refs},
};

use crate::{
    commands::{Call, Written},
    parse::{needs, too_many},
    reflist::{claim, on_refs},
};

/// `lsty pri`: sets the priority of each reference.
pub(crate) fn pri(call: &Call<'_>) -> Result<Written> {
    let (chunks @ [_, ..], []) = call.parsed.args.as_chunks::<2>() else {
        return Err(needs());
    };
    let mut pairs = Vec::new();
    for [refs, letter] in chunks {
        for target in parse_refs(refs)? {
            pairs.push((target, letter));
        }
    }
    call.bind(pairs.iter_mut().map(|(t, _)| t))?;
    let pairs = pairs
        .into_iter()
        .map(|(target, letter)| {
            let priority = Some(ops::parse_letter(letter)?);
            Ok(PriPair { target, priority })
        })
        .collect::<Result<Vec<_>>>()?;
    call.mutate(|env| ops::pri(env, &pairs))
}

/// `lsty id`: gives each reference an id, chosen or random.
pub(crate) fn id(call: &Call<'_>) -> Result<Written> {
    if call.parsed.args.len() > 2 {
        return Err(too_many());
    }
    let bound = call.refs_and_rest(0)?;
    let chosen = bound
        .rest
        .first()
        .map(String::as_str)
        .filter(|id| !id.is_empty());
    call.mutate(|env| ops::id(env, &bound.targets, chosen))
}

/// `lsty depri`: removes the priority of each reference.
pub(crate) fn depri(call: &Call<'_>) -> Result<Written> {
    on_refs(call, &call.parsed.args, ops::depri)
}

/// `lsty due`: sets or clears the due date.
pub(crate) fn due(call: &Call<'_>) -> Result<Written> {
    if call.parsed.args.len() > 2 {
        return Err(too_many());
    }
    let bound = call.refs_and_rest(1)?;
    let date = bound.rest.first().map_or("", String::as_str);
    call.mutate(|env| ops::due(env, &bound.targets, date))
}

/// `lsty append`: adds text at the end of each item.
pub(crate) fn append(call: &Call<'_>) -> Result<Written> {
    let bound = call.refs_and_rest(1)?;
    let text = bound.rest.join(" ");
    call.mutate(|env| ops::append(env, &bound.targets, &text))
}

/// `lsty prepend`: puts text in front of each item.
pub(crate) fn prepend(call: &Call<'_>) -> Result<Written> {
    let bound = call.refs_and_rest(1)?;
    let text = bound.rest.join(" ");
    call.mutate(|env| ops::prepend(env, &bound.targets, &text))
}

/// `lsty replace`: gives each item new text.
pub(crate) fn replace(call: &Call<'_>) -> Result<Written> {
    let bound = call.refs_and_rest(1)?;
    let text = bound.rest.join(" ");
    let opts = claim(call);
    call.mutate(|env| ops::replace(env, &bound.targets, &text, opts))
}

/// `lsty tag`: adds tags to each item.
pub(crate) fn tag(call: &Call<'_>) -> Result<Written> {
    let bound = call.refs_and_rest(1)?;
    call.mutate(|env| ops::tag(env, &bound.targets, bound.rest))
}

/// `lsty untag`: removes tags from each item.
pub(crate) fn untag(call: &Call<'_>) -> Result<Written> {
    let bound = call.refs_and_rest(1)?;
    call.mutate(|env| ops::untag(env, &bound.targets, bound.rest))
}

/// `lsty del`: blanks items, or removes a term from them.
pub(crate) fn del(call: &Call<'_>) -> Result<Written> {
    let bound = call.refs_and_rest(0)?;
    if bound.rest.is_empty() {
        let opts = claim(call);
        return call.mutate(|env| ops::del(env, &bound.targets, opts));
    }
    let term = bound.rest.join(" ");
    call.mutate(|env| ops::del_term(env, &bound.targets, &term))
}
