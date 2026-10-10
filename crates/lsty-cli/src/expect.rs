//! The rules for naming items: agents use ids, and `--expect` guards
//! each number.

use lsty_core::errs::{Code, Error, Result, Source};
use lsty_core::ops::{Actor, Target, parse_refs};

use crate::commands::Call;
use crate::parse::{Opt, needs, usage};

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
    pub(crate) fn bind(&self, refs: &mut [&mut Target]) -> Result<()> {
        let expects = self.parsed.opts.all(Opt::Expect);
        let mut nrs: Vec<&mut Target> = refs
            .iter_mut()
            .map(|t| &mut **t)
            .filter(|t| t.id().is_none())
            .collect();
        check_expects(expects, nrs.len())?;
        self.check_by_id(&nrs, expects.len())?;
        let exact = !self.ops.actor.is_human();
        for (target, text) in nrs.iter_mut().zip(expects) {
            target.set_expect(text.as_str(), exact);
        }
        Ok(())
    }

    /// Reads the references of the first argument and binds them: the
    /// shape of `due REFS DATE`, `tag REFS TAG…` and the like. Returns
    /// the targets and the arguments after the references.
    ///
    /// # Errors
    ///
    /// A usage error for fewer than `1 + min_rest` arguments (with the
    /// synopsis as hint), a bad reference, or whatever [`bind`](Self::bind)
    /// refuses.
    pub(crate) fn refs_and_rest(
        &self,
        synopsis: &str,
        min_rest: usize,
    ) -> Result<Bound<'_>> {
        let args = self.parsed.args.as_slice();
        let (Some(refs), true) = (args.first(), args.len() > min_rest) else {
            return Err(needs(synopsis));
        };
        let mut targets = parse_refs(refs)?;
        self.bind(&mut targets.iter_mut().collect::<Vec<_>>())?;
        let rest = args.get(1..).unwrap_or_default();
        Ok(Bound { targets, rest })
    }

    fn check_by_id(&self, nrs: &[&mut Target], expects: usize) -> Result<()> {
        let Actor::Agent(_) = self.ops.actor else {
            return Ok(());
        };
        let Some(first) = nrs.first() else {
            return Ok(());
        };
        let allowed = BY_NR_WITH_EXPECT.contains(&self.command.name)
            && expects == nrs.len();
        if !self.command.writes || allowed {
            return Ok(());
        }
        let verb = if matches!(first.line(), Some((Source::Done, _))) {
            "restore"
        } else {
            "id"
        };
        Err(
            Error::new(Code::Usage, "agents name items by id").with_hint(
                format!(
                    "use id: \u{2014} lsty {verb} {} --expect \"Call client\"",
                    first.given()
                ),
            ),
        )
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
