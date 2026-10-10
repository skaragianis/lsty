//! `dep` and `undep`: make items wait for others, or stop.

use super::{
    claim::{ClaimMode, Guard},
    env::Env,
    id::check_chosen,
    ids::Taken,
    lifecycle::ClaimOpts,
    refs::{Target, no_item},
    result::Outcome,
    run::run,
    staged::Path,
    tx::{Picked, Tx, gone, wrong_state},
};
use crate::{
    errs::{Result, Source},
    list::ItemId,
    todotxt::TokenView,
};

/// Makes each waiting item wait for each prerequisite. A prerequisite
/// without an id gets one: `chosen`, or a random one.
///
/// # Errors
///
/// A usage error when `chosen` is given for several prerequisites,
/// `wrong_state` for a prerequisite that has another id or is archived
/// without one, `cycle` for a dependency that closes a loop, and
/// whatever the rules refuse.
pub fn dep(
    env: &Env<'_>,
    waiting: &[Target],
    prereqs: &[Target],
    chosen: Option<&str>,
) -> Result<Outcome> {
    check_chosen(chosen, prereqs.len())?;
    run(env, "dep", false, |tx| {
        let pres = prerequisites(tx, prereqs)?;
        let mut ids: Option<Vec<Vec<u8>>> = None;
        tx.each_edit(waiting, &env.actor, |tx, item| {
            if ids.is_none() {
                ids = Some(prerequisite_ids(tx, env, &pres, chosen)?);
            }
            let ids = ids.as_deref().unwrap_or_default();
            add_deps(tx, item, &pres, ids)
        })
    })
}

/// The items the prerequisite targets name, once each. A prerequisite
/// may be in the done file.
fn prerequisites(tx: &mut Tx, targets: &[Target]) -> Result<Vec<ItemId>> {
    let mut out = Vec::new();
    for target in targets {
        let target = target.clone().archived();
        match tx.target(&target)? {
            Picked::Gone(_) => return Err(no_item(&target)),
            Picked::Item(id) if !out.contains(&id) => out.push(id),
            Picked::Item(_) => {}
        }
    }
    Ok(out)
}

fn prerequisite_ids(
    tx: &mut Tx,
    env: &Env<'_>,
    pres: &[ItemId],
    chosen: Option<&str>,
) -> Result<Vec<Vec<u8>>> {
    let mut taken = Taken::new([]);
    let mut ids = Vec::with_capacity(pres.len());
    for &pre in pres {
        ids.push(prerequisite_id(tx, env, pre, chosen, &mut taken)?);
    }
    Ok(ids)
}

/// The id a prerequisite has, or gets.
fn prerequisite_id(
    tx: &mut Tx,
    env: &Env<'_>,
    pre: ItemId,
    chosen: Option<&str>,
    taken: &mut Taken,
) -> Result<Vec<u8>> {
    let handle = tx.handle(pre);
    let item = tx.item(pre).ok_or_else(gone)?;
    let have = tx.snapshot().id_of(item).map(<[u8]>::to_vec);
    if let Some(have) = have {
        let text = String::from_utf8_lossy(&have).into_owned();
        if chosen.is_some_and(|c| c.as_bytes() != have) {
            return Err(wrong_state(
                tx.snapshot(),
                item,
                item.state(),
                format!("{handle} already has id:{text}"),
                "an id is never changed".to_owned(),
            ));
        }
        return Ok(have);
    }
    if item.source() == Source::Done {
        return Err(wrong_state(
            tx.snapshot(),
            item,
            item.state(),
            format!("{handle} has no id and is archived"),
            "restore it first, then give it an id".to_owned(),
        ));
    }
    let id = match chosen {
        Some(chosen) => chosen.to_owned(),
        None => taken
            .draw(tx.snapshot(), || tx.edited_ids(), env.ids)?
            .to_string(),
    };
    give_id(tx, env, pre, &id)?;
    Ok(id.into_bytes())
}

/// Appends `id:` to a prerequisite, as a metadata edit on it.
pub(super) fn give_id(
    tx: &mut Tx,
    env: &Env<'_>,
    pre: ItemId,
    id: &str,
) -> Result<()> {
    let guard = Guard {
        actor: &env.actor,
        mode: ClaimMode::Edit,
        opts: ClaimOpts::default(),
    };
    tx.guarded(guard, pre, |tx| {
        let line = tx.current(pre).ok_or_else(gone)?;
        let line = tx.append_tok(pre, &line, format!("id:{id}").as_bytes())?;
        tx.set(pre, line.raw(), Path::Action);
        let handle = tx.handle(pre);
        tx.say(format!("{handle} is id:{id}"));
        if let Some(line_ref) = tx.line_ref(pre) {
            tx.side(&[line_ref]);
        }
        Ok(())
    })
}

fn add_deps(
    tx: &mut Tx,
    item: ItemId,
    pres: &[ItemId],
    ids: &[Vec<u8>],
) -> Result<()> {
    let handle = tx.handle(item);
    let mut line = tx.current(item).ok_or_else(gone)?;
    for (&pre, id) in pres.iter().zip(ids) {
        let prereq = tx.handle(pre);
        if line.tags().deps.contains(&id.as_slice()) {
            tx.say(format!("{handle} already waits for {prereq}"));
            continue;
        }
        line =
            tx.append_tok(item, &line, &[b"dep:", id.as_slice()].concat())?;
        tx.say(format!("{handle} now waits for {prereq}"));
    }
    tx.set(item, line.raw(), Path::Action);
    Ok(())
}

/// Stops each waiting item waiting for each prerequisite.
///
/// # Errors
///
/// `no_item` for a prerequisite that is not there, and whatever the
/// rules refuse.
pub fn undep(
    env: &Env<'_>,
    waiting: &[Target],
    prereqs: &[Target],
) -> Result<Outcome> {
    run(env, "undep", false, |tx| {
        let pres = prerequisites(tx, prereqs)?;
        tx.each_edit(waiting, &env.actor, |tx, item| {
            drop_deps(tx, item, &pres)
        })
    })
}

fn drop_deps(tx: &mut Tx, item: ItemId, pres: &[ItemId]) -> Result<()> {
    let handle = tx.handle(item);
    let mut line = tx.current(item).ok_or_else(gone)?;
    for &pre in pres {
        let prereq = tx.handle(pre);
        let id = tx
            .item(pre)
            .and_then(|p| tx.snapshot().id_of(p))
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        let (next, removed) =
            tx.remove_where(item, &line, |t| names_dep(t, &id))?;
        line = next;
        if removed == 0 {
            tx.say(format!("{handle} does not wait for {prereq}"));
        } else {
            tx.say(format!("{handle} no longer waits for {prereq}"));
        }
    }
    tx.set(item, line.raw(), Path::Action);
    Ok(())
}

/// Whether `tok` is a `dep:` of `id`.
fn names_dep(tok: &TokenView<'_>, id: &[u8]) -> bool {
    tok.is_tag(b"dep") && tok.value == id
}
