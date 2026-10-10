//! What a write command hands to the operations and prints after them.

use lsty_core::{
    errs::Result,
    list::{Item, ItemId, Snapshot},
    ops::{self, Outcome},
};

use crate::{
    commands::{Call, Written},
    output::{Field, Json, Output},
    parse::Opt,
};

impl Call<'_> {
    /// Runs an operation and renders what it did.
    ///
    /// # Errors
    ///
    /// Whatever the operation refuses with.
    pub(crate) fn mutate(
        &self,
        run: impl FnOnce(&ops::Env<'_>) -> Result<Outcome>,
    ) -> Result<Written> {
        let outcome = run(&self.ops)?;
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
        Output::either(
            self.json(),
            || {
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
                    Field::new(
                        "spawned",
                        self.objects(after, outcome.spawned()),
                    ),
                    Field::new(
                        "changed",
                        self.objects(after, outcome.changed()),
                    ),
                    Field::bool("renumbered", outcome.renumbered()),
                ]);
                fields.extend_from_slice(before_messages);
                fields.push(Field::array(
                    "messages",
                    outcome.messages().iter().map(|m| Json::bytes(m)),
                ));
                fields
            },
            || outcome.messages().to_vec(),
        )
    }

    /// With `--print-id`, the output is the ids of `items`, one per line,
    /// and what it said goes to standard error. Every item passed has an
    /// id (the callers make sure), so one id comes out per item.
    pub(crate) fn print_ids<'a>(
        &self,
        mut output: Output,
        snap: &Snapshot,
        items: impl Iterator<Item = &'a Item>,
    ) -> Output {
        if self.has(Opt::PrintId) {
            output.notes = std::mem::take(&mut output.lines);
            output.lines = items
                .filter_map(|i| snap.id_of(i))
                .map(<[u8]>::to_vec)
                .collect();
        }
        output
    }

    fn objects(&self, snap: &Snapshot, ids: &[ItemId]) -> Json {
        let items: Vec<_> =
            ids.iter().filter_map(|id| snap.item(*id)).collect();
        self.item_json(snap).objects(&items)
    }
}
