//! The rules for naming tasks: agents use ids, and `--expect` guards
//! each number.

use lsty_core::{
    errs::{Nr, Result, Source},
    ops::{Target, parse_refs},
};

use crate::{
    commands::Call,
    parse::{Opt, needs, usage},
};

/// The commands an agent may give a number to, when each number has an
/// `--expect`.
const BY_NR_WITH_EXPECT: [&str; 2] = ["id", "restore"];

/// The references of a command, bound, and the arguments after them.
pub(crate) struct Bound<'a> {
    /// The references of the first argument, with their guards.
    pub targets: Vec<Target>,
    /// The arguments after the references.
    pub rest: &'a [String],
}

impl Call<'_> {
    /// Applies the identity rules and `--expect` to the references of a
    /// command, given in command-line order: the targets first, then the
    /// anchor of `--after` or `--before`. An `id:` reference is not
    /// counted and takes no guard.
    ///
    /// # Errors
    ///
    /// A usage error for an empty guard text, a count of guards that is
    /// not zero or one per number, or an agent's number.
    pub(crate) fn bind<'t>(
        &self,
        refs: impl IntoIterator<Item = &'t mut Target>,
    ) -> Result<()> {
        let expects = self.parsed.opts.all(Opt::Expect);
        let mut nrs: Vec<&mut Target> =
            refs.into_iter().filter(|t| t.id().is_none()).collect();
        for target in nrs.iter_mut().filter(|_| self.places.is_some()) {
            **target = target.clone().placed(|n| self.line_at(n, None))?;
        }
        check_expects(expects, nrs.len())?;
        self.check_by_id(&nrs, expects.len())?;
        let exact = !self.ops.actor.is_human();
        for (target, text) in nrs.iter_mut().zip(expects) {
            target.set_expect(text.as_str(), exact);
        }
        Ok(())
    }

    /// The task at place `n` of the list a caller shows, as a line, of the
    /// `only` file when given.
    fn line_at(&self, n: u32, only: Option<Source>) -> Option<(Source, Nr)> {
        let n = usize::try_from(n).ok()?;
        let found = self.places?.iter().find(|(_, place)| **place == n)?;
        Some(*found.0).filter(|(source, _)| only.is_none_or(|s| s == *source))
    }

    /// The position of `reorder`: a line of the todo file, or the place of
    /// a task of the list a caller shows, which is the line that task has;
    /// a place past the last task is past the end, as a line is.
    pub(crate) fn position(&self, n: i64) -> i64 {
        let at = u32::try_from(n)
            .ok()
            .and_then(|n| self.line_at(n, Some(Source::Todo)));
        match (self.places, at) {
            (Some(_), Some((_, nr))) => i64::from(nr.get()),
            (Some(_), None) if n >= 1 => i64::MAX,
            _ => n,
        }
    }

    /// Reads the references of the first argument and binds them: the
    /// shape of `due REFS DATE`, `tag REFS TAG…` and the like. Returns
    /// the targets and the arguments after the references.
    ///
    /// # Errors
    ///
    /// A usage error for fewer than `1 + min_rest` arguments (with the
    /// synopsis added as hint), a bad reference, or whatever
    /// [`bind`](Self::bind) refuses.
    pub(crate) fn refs_and_rest(&self, min_rest: usize) -> Result<Bound<'_>> {
        let args = self.parsed.args.as_slice();
        let Some((refs, rest)) = args
            .split_first()
            .filter(|(_, rest)| rest.len() >= min_rest)
        else {
            return Err(needs());
        };
        let mut targets = parse_refs(refs)?;
        self.bind(&mut targets)?;
        Ok(Bound { targets, rest })
    }

    fn check_by_id(&self, nrs: &[&mut Target], expects: usize) -> Result<()> {
        let allowed = BY_NR_WITH_EXPECT.contains(&self.command.name)
            && expects == nrs.len();
        if self.ops.actor.is_human() || !self.command.writes || allowed {
            return Ok(());
        }
        let Some(first) = nrs.first() else {
            return Ok(());
        };
        let verb = if matches!(first.line(), Some((Source::Done, _))) {
            "restore"
        } else {
            "id"
        };
        Err(usage("agents name tasks by id").with_hint(format!(
            "use id: \u{2014} lsty {verb} {} --expect \"Call client\"",
            first.handle()
        )))
    }
}

fn check_expects(expects: &[String], nrs: usize) -> Result<()> {
    if expects.iter().any(|e| e.trim().is_empty()) {
        return Err(usage("--expect needs a text"));
    }
    if expects.is_empty() || expects.len() == nrs {
        return Ok(());
    }
    Err(usage(format!(
        "{} --expect for {nrs} NR references",
        expects.len()
    )))
}
