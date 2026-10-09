//! The commands that move an item through its life: `start`, `stop`,
//! `do`, `reopen`, `restore`, `archive` and `deduplicate`.

use lsty_core::errs::Result;
use lsty_core::ops::{self, DoOpts, StartOpts};

use crate::commands::{Call, Written};
use crate::parse::Opt;
use crate::reflist::{claim, on_refs, without_args};

const START_SYNOPSIS: &str = "start REFS\u{2026}";
const STOP_SYNOPSIS: &str = "stop REFS\u{2026}";
const DO_SYNOPSIS: &str = "do REFS\u{2026}";
const REOPEN_SYNOPSIS: &str = "reopen REFS\u{2026}";
const RESTORE_SYNOPSIS: &str = "restore REFS\u{2026}";

/// `lsty start`: marks items as underway.
pub(crate) fn start(call: &mut Call<'_>) -> Result<Written> {
    let opts = StartOpts {
        claim: claim(call),
        ignore_blocked: call.has(Opt::IgnoreBlocked),
    };
    let args = &call.parsed.args;
    on_refs(call, START_SYNOPSIS, args, |env, t| {
        ops::start(env, t, opts)
    })
}

/// `lsty stop`: gives up the claim on items.
pub(crate) fn stop(call: &mut Call<'_>) -> Result<Written> {
    let opts = claim(call);
    let args = &call.parsed.args;
    on_refs(call, STOP_SYNOPSIS, args, |env, t| ops::stop(env, t, opts))
}

/// `lsty do`: completes items.
pub(crate) fn finish(call: &mut Call<'_>) -> Result<Written> {
    let opts = DoOpts {
        claim: claim(call),
        ignore_blocked: call.has(Opt::IgnoreBlocked),
        archive: call.toggle(Opt::Archive, Opt::NoArchive),
    };
    let args = &call.parsed.args;
    on_refs(call, DO_SYNOPSIS, args, |env, t| ops::finish(env, t, opts))
}

/// `lsty reopen`: makes completed items open again.
pub(crate) fn reopen(call: &mut Call<'_>) -> Result<Written> {
    let opts = claim(call);
    let args = &call.parsed.args;
    on_refs(call, REOPEN_SYNOPSIS, args, |env, t| {
        ops::reopen(env, t, opts)
    })
}

/// `lsty restore`: brings archived items back into the todo file.
pub(crate) fn restore(call: &mut Call<'_>) -> Result<Written> {
    let opts = claim(call);
    let args = &call.parsed.args;
    on_refs(call, RESTORE_SYNOPSIS, args, |env, t| {
        ops::restore(env, t, opts)
    })
}

/// `lsty archive`: moves completed items to the done file.
pub(crate) fn archive(call: &mut Call<'_>) -> Result<Written> {
    without_args(call)?;
    call.mutate(ops::archive)
}

/// `lsty deduplicate`: blanks later copies of an item.
pub(crate) fn deduplicate(call: &mut Call<'_>) -> Result<Written> {
    without_args(call)?;
    call.mutate(ops::deduplicate)
}
