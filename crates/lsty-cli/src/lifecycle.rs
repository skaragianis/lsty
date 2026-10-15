//! The commands that move a task through its life: `start`, `stop`,
//! `do`, `reopen`, `restore`, `archive` and `deduplicate`.

use lsty_core::{
    errs::Result,
    ops::{self, DoOpts, StartOpts},
};

use crate::{
    commands::{Call, Written},
    parse::Opt,
    reflist::{claim, on_refs, without_args},
};

/// `lsty start`: marks tasks as underway.
pub(crate) fn start(call: &Call<'_>) -> Result<Written> {
    let opts = StartOpts {
        claim: claim(call),
        ignore_blocked: call.has(Opt::IgnoreBlocked),
    };
    on_refs(call, &call.parsed.args, |env, t| ops::start(env, t, opts))
}

/// `lsty stop`: gives up the claim on tasks.
pub(crate) fn stop(call: &Call<'_>) -> Result<Written> {
    let opts = claim(call);
    on_refs(call, &call.parsed.args, |env, t| ops::stop(env, t, opts))
}

/// `lsty do`: completes tasks.
pub(crate) fn finish(call: &Call<'_>) -> Result<Written> {
    let opts = DoOpts {
        claim: claim(call),
        ignore_blocked: call.has(Opt::IgnoreBlocked),
        archive: call.toggle(Opt::Archive, Opt::NoArchive),
    };
    on_refs(call, &call.parsed.args, |env, t| ops::finish(env, t, opts))
}

/// `lsty reopen`: makes completed tasks open again.
pub(crate) fn reopen(call: &Call<'_>) -> Result<Written> {
    let opts = claim(call);
    on_refs(call, &call.parsed.args, |env, t| ops::reopen(env, t, opts))
}

/// `lsty restore`: brings archived tasks back into the todo file.
pub(crate) fn restore(call: &Call<'_>) -> Result<Written> {
    let opts = claim(call);
    on_refs(call, &call.parsed.args, |env, t| ops::restore(env, t, opts))
}

/// `lsty archive`: moves completed tasks to the done file.
pub(crate) fn archive(call: &Call<'_>) -> Result<Written> {
    without_args(call)?;
    call.mutate(ops::archive)
}

/// `lsty deduplicate`: blanks later copies of a task.
pub(crate) fn deduplicate(call: &Call<'_>) -> Result<Written> {
    without_args(call)?;
    call.mutate(ops::deduplicate)
}
