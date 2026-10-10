//! `del`: blank items, keeping every line number.

use super::{
    claim::{ClaimMode, Guard},
    env::Env,
    lifecycle::ClaimOpts,
    refs::Target,
    result::Outcome,
    run::run,
};
use crate::errs::Result;

/// Blanks each target. An agent that does not hold an item needs
/// `takeover` to delete it. Dependents of a deleted id stop waiting for
/// it in the same write.
///
/// # Errors
///
/// `no_item` for a line that is not there, `wrong_state` for an item of
/// the done file, `claimed` for an item another agent holds.
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
        tx.each_guarded(targets, guard, |tx, id| {
            let held = tx
                .item(id)
                .and_then(|item| tx.snapshot().id_of(item))
                .map(<[u8]>::to_vec)
                .unwrap_or_default();
            let handle = tx.handle(id);
            tx.blank(id);
            tx.say_lost(&held, format!("{handle} deleted"));
            Ok(())
        })
    })
}
