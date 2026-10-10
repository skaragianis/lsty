//! Commands that take a list of references, given as several arguments,
//! comma lists, or both.

use lsty_core::{
    errs::Result,
    ops::{self, Outcome, Target, parse_refs},
};

use crate::{
    commands::{Call, Written},
    parse::{Opt, needs, usage},
};

/// The references of `args`: each argument is a comma list, and commas
/// at its ends (`3,` of `3, 5`) and empty arguments are skipped.
///
/// # Errors
///
/// A usage error for a text that is not a reference.
pub(crate) fn ref_list(args: &[String]) -> Result<Vec<Target>> {
    let mut targets = Vec::new();
    for arg in args {
        let arg = arg.trim_matches(',');
        if !arg.is_empty() {
            targets.extend(parse_refs(arg)?);
        }
    }
    Ok(targets)
}

/// The flags every claim-aware command shares.
pub(crate) fn claim(call: &Call<'_>) -> ops::ClaimOpts {
    ops::ClaimOpts {
        takeover: call.has(Opt::Takeover),
    }
}

/// Binds the references of `args` and runs `run` on them.
///
/// # Errors
///
/// A usage error for no references, and whatever binding or the operation
/// refuses.
pub(crate) fn on_refs(
    call: &Call<'_>,
    args: &[String],
    run: impl FnOnce(&ops::Env<'_>, &[Target]) -> Result<Outcome>,
) -> Result<Written> {
    let mut targets = ref_list(args)?;
    if targets.is_empty() {
        return Err(needs());
    }
    call.bind(&mut targets)?;
    call.mutate(|env| run(env, &targets))
}

/// The usage error of a command that takes no arguments but got some.
///
/// # Errors
///
/// A usage error naming the command.
pub(crate) fn without_args(call: &Call<'_>) -> Result<()> {
    if call.parsed.args.is_empty() {
        return Ok(());
    }
    Err(usage(format!("{} takes no arguments", call.command.name)))
}
