//! What a write command hands to the operations and prints after them.

use lsty_core::errs::Result;
use lsty_core::list::{ItemId, Snapshot};
use lsty_core::ops::{self, Outcome};

use crate::commands::{Base, Call, Written};
use crate::output::{Field, Json, Output};

impl Call<'_, '_> {
    /// The environment of an operation: the files, the clock and who is
    /// acting.
    pub(crate) fn ops_env(&self) -> ops::Env<'_> {
        let mut env = match &self.base {
            Base::Process(own) => {
                let mut env =
                    ops::Env::new(&own.found, &own.config, &own.clock);
                env.create = own.create;
                env
            }
            Base::Caller(env) => (*env).clone(),
        };
        env.actor = self.actor.clone();
        env
    }

    /// Runs an operation and renders what it did.
    ///
    /// # Errors
    ///
    /// Whatever the operation refuses with.
    pub(crate) fn mutate(
        &self,
        run: impl FnOnce(&ops::Env<'_>) -> Result<Outcome>,
    ) -> Result<Written> {
        let outcome = run(&self.ops_env())?;
        let output = self.mutation_output(&outcome);
        Ok(Written { output, outcome })
    }

    /// The JSON fields and the lines of a finished operation.
    pub(crate) fn mutation_output(&self, outcome: &Outcome) -> Output {
        self.mutation_output_with(outcome, &[], &[])
    }

    /// As [`Call::mutation_output`], with extra JSON fields after `items`
    /// and before `messages`.
    pub(crate) fn mutation_output_with(
        &self,
        outcome: &Outcome,
        after_items: &[Field],
        before_messages: &[Field],
    ) -> Output {
        let after = outcome.after();
        let fields = if self.json() {
            let mut fields = vec![
                Field::string("action", self.command.name),
                Field::new("items", self.objects(after, outcome.items())),
            ];
            fields.extend_from_slice(after_items);
            fields.extend([
                Field::new(
                    "before",
                    self.objects(outcome.read(), outcome.before()),
                ),
                Field::new("spawned", self.objects(after, outcome.spawned())),
                Field::new("changed", self.objects(after, outcome.changed())),
                Field::bool("renumbered", outcome.renumbered()),
            ]);
            fields.extend_from_slice(before_messages);
            fields.push(Field::array(
                "messages",
                outcome.messages().iter().map(|m| Json::bytes(m)),
            ));
            fields
        } else {
            Vec::new()
        };
        Output {
            fields,
            lines: outcome.messages().to_vec(),
        }
    }

    fn objects(&self, snap: &Snapshot, ids: &[ItemId]) -> Json {
        let items: Vec<_> =
            ids.iter().filter_map(|id| snap.item(*id)).collect();
        self.item_json(snap).objects(&items)
    }
}
