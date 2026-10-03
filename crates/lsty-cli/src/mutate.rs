//! What a write command hands to the operations and prints after them.

use lsty_core::errs::Result;
use lsty_core::list::{ItemId, Snapshot};
use lsty_core::ops::{self, Outcome};

use crate::commands::Call;
use crate::output::{Field, Json, Output};

impl Call<'_, '_> {
    /// The environment of an operation: the files, the clock and who is
    /// acting.
    pub fn ops_env(&self) -> ops::Env<'_> {
        let mut env = ops::Env::new(&self.found, &self.config, &self.clock);
        env.actor = self.actor.clone();
        env.create = self.create;
        env
    }

    /// Runs an operation and renders what it did.
    ///
    /// # Errors
    ///
    /// Whatever the operation refuses with.
    pub fn mutate(
        &self,
        run: impl FnOnce(&ops::Env<'_>) -> Result<Outcome>,
    ) -> Result<Output> {
        let outcome = run(&self.ops_env())?;
        Ok(self.mutation_output(&outcome))
    }

    /// The JSON fields and the lines of a finished operation.
    pub fn mutation_output(&self, outcome: &Outcome) -> Output {
        let after = outcome.after();
        let fields = if self.json() {
            vec![
                Field::string("action", self.command.name),
                Field::new("items", self.objects(after, &outcome.items)),
                Field::new(
                    "before",
                    self.objects(outcome.read(), &outcome.before),
                ),
                Field::new("spawned", self.objects(after, &outcome.spawned)),
                Field::new("changed", self.objects(after, &outcome.changed)),
                Field::bool("renumbered", outcome.renumbered),
                Field::array(
                    "messages",
                    outcome.messages.iter().map(|m| Json::bytes(m)),
                ),
            ]
        } else {
            Vec::new()
        };
        Output {
            fields,
            lines: outcome.messages.clone(),
        }
    }

    fn objects(&self, snap: &Snapshot, ids: &[ItemId]) -> Json {
        let items: Vec<_> =
            ids.iter().filter_map(|id| snap.item(*id)).collect();
        self.item_json(snap).objects(&items)
    }
}
