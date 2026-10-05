//! The commands that change items already in the list: `append`,
//! `prepend`, `replace`, `del`, `pri`, `depri`, `due`, `id`, `tag` and
//! `untag`.

use lsty_core::errs::Result;
use lsty_core::ops::{self, ClaimOpts, PriPair, parse_refs};

use crate::commands::{Call, Written};
use crate::parse::{Opt, needs, too_many};
use crate::reflist::claim;

const PRI_SYNOPSIS: &str = "pri REFS PRI [REFS PRI…]";
const DEPRI_SYNOPSIS: &str = "depri REFS…";
const DUE_SYNOPSIS: &str = "due REFS DATE";
const APPEND_SYNOPSIS: &str = "append REFS TEXT…";
const PREPEND_SYNOPSIS: &str = "prepend REFS TEXT…";
const REPLACE_SYNOPSIS: &str = "replace REFS TEXT…";
const TAG_SYNOPSIS: &str = "tag REFS TAG…";
const UNTAG_SYNOPSIS: &str = "untag REFS TAG…";
const ID_SYNOPSIS: &str = "id REFS [ID]";
const DEL_SYNOPSIS: &str = "del REFS [TERM\u{2026}]";

/// `lsty pri`: sets the priority of each reference.
pub(crate) fn pri(call: &mut Call<'_, '_>) -> Result<Written> {
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
            pairs.push((target, letter));
        }
    }
    call.bind(&mut pairs.iter_mut().map(|(t, _)| t).collect::<Vec<_>>())?;
    let pairs = pairs
        .into_iter()
        .map(|(target, letter)| {
            let letter = Some(ops::parse_letter(letter)?);
            Ok(PriPair { target, letter })
        })
        .collect::<Result<Vec<_>>>()?;
    call.mutate(|env| ops::pri(env, &pairs))
}

/// `lsty id`: gives each reference an id, chosen or random.
pub(crate) fn id(call: &mut Call<'_, '_>) -> Result<Written> {
    if call.parsed.args.len() > 2 {
        return Err(too_many(ID_SYNOPSIS));
    }
    let bound = call.refs_and_rest(ID_SYNOPSIS, 0)?;
    let chosen = bound
        .rest
        .first()
        .map(String::as_str)
        .filter(|id| !id.is_empty());
    call.mutate(|env| ops::id(env, &bound.targets, chosen))
}

/// `lsty depri`: removes the priority of each reference.
pub(crate) fn depri(call: &mut Call<'_, '_>) -> Result<Written> {
    let args = &call.parsed.args;
    if args.is_empty() {
        return Err(needs(DEPRI_SYNOPSIS));
    }
    let mut targets = Vec::new();
    for arg in args {
        targets.extend(parse_refs(arg)?);
    }
    call.bind(&mut targets.iter_mut().collect::<Vec<_>>())?;
    call.mutate(|env| ops::depri(env, &targets))
}

/// `lsty due`: sets or clears the due date.
pub(crate) fn due(call: &mut Call<'_, '_>) -> Result<Written> {
    if call.parsed.args.len() > 2 {
        return Err(too_many(DUE_SYNOPSIS));
    }
    let bound = call.refs_and_rest(DUE_SYNOPSIS, 1)?;
    let date = bound.rest.first().map_or("", String::as_str);
    call.mutate(|env| ops::due(env, &bound.targets, date))
}

/// `lsty append`: adds text at the end of each item.
pub(crate) fn append(call: &mut Call<'_, '_>) -> Result<Written> {
    let bound = call.refs_and_rest(APPEND_SYNOPSIS, 1)?;
    let text = bound.rest.join(" ");
    call.mutate(|env| ops::append(env, &bound.targets, &text))
}

/// `lsty prepend`: puts text in front of each item.
pub(crate) fn prepend(call: &mut Call<'_, '_>) -> Result<Written> {
    let bound = call.refs_and_rest(PREPEND_SYNOPSIS, 1)?;
    let text = bound.rest.join(" ");
    call.mutate(|env| ops::prepend(env, &bound.targets, &text))
}

/// `lsty replace`: gives each item new text.
pub(crate) fn replace(call: &mut Call<'_, '_>) -> Result<Written> {
    let bound = call.refs_and_rest(REPLACE_SYNOPSIS, 1)?;
    let text = bound.rest.join(" ");
    let opts = ClaimOpts {
        takeover: call.has(Opt::Takeover),
    };
    call.mutate(|env| ops::replace(env, &bound.targets, &text, opts))
}

/// `lsty tag`: adds tags to each item.
pub(crate) fn tag(call: &mut Call<'_, '_>) -> Result<Written> {
    let bound = call.refs_and_rest(TAG_SYNOPSIS, 1)?;
    call.mutate(|env| ops::tag(env, &bound.targets, bound.rest))
}

/// `lsty untag`: removes tags from each item.
pub(crate) fn untag(call: &mut Call<'_, '_>) -> Result<Written> {
    let bound = call.refs_and_rest(UNTAG_SYNOPSIS, 1)?;
    call.mutate(|env| ops::untag(env, &bound.targets, bound.rest))
}

/// `lsty del`: blanks items, or removes a term from them.
pub(crate) fn del(call: &mut Call<'_, '_>) -> Result<Written> {
    let bound = call.refs_and_rest(DEL_SYNOPSIS, 0)?;
    if bound.rest.is_empty() {
        let opts = claim(call);
        return call.mutate(|env| ops::del(env, &bound.targets, opts));
    }
    let term = bound.rest.join(" ");
    call.mutate(|env| ops::del_term(env, &bound.targets, &term))
}
