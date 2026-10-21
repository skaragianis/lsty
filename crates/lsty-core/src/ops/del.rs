//! `del`: blank tasks, keeping every line number.

use super::{
    claim::{ClaimMode, Guard},
    env::Env,
    lifecycle::ClaimOpts,
    refs::Target,
    result::Outcome,
    run::run,
};
use crate::errs::Result;

/// Blanks each target; a line of only spaces and tabs is emptied. An
/// agent that does not hold a task needs `takeover` to delete it.
/// Dependents of a deleted id stop waiting for it in the same write.
///
/// # Errors
///
/// `no_task` for a line that is not there, `wrong_state` for a task of
/// the done file, `claimed` for a task another agent holds.
pub fn del(
    env: &Env<'_>,
    targets: &[Target],
    opts: ClaimOpts,
) -> Result<Outcome> {
    run(env, "del", false, |tx| {
        let guard = Guard {
            actor: &env.actor,
            mode: ClaimMode::Ends,
            opts,
        };
        for target in targets {
            tx.clear_spaces(target);
        }
        tx.each_guarded(targets, guard, |tx, id| {
            let held = tx.id_bytes(id).unwrap_or_default();
            let handle = tx.handle(id);
            tx.blank(id);
            tx.say_lost(&held, format!("{handle} deleted"));
            Ok(())
        })
    })
}
