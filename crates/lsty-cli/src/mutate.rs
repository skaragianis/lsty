//! What a write command hands to the operations and prints after them.

use lsty_core::{
    errs::Result,
    list::{Snapshot, Task, TaskId},
    ops::{self, Outcome},
    visible::visible_bytes,
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
        let output = Output::either(
            self.json(),
            || self.mutation_fields(&outcome, &[], &[]),
            || {
                outcome
                    .messages()
                    .iter()
                    .map(|m| visible_bytes(m).into_owned())
                    .collect()
            },
        );
        Ok(Written { output, outcome })
    }

    /// The JSON fields of a finished operation, with extra fields after
    /// `tasks` and before `messages`.
    pub(crate) fn mutation_fields(
        &self,
        outcome: &Outcome,
        after_tasks: &[Field],
        before_messages: &[Field],
    ) -> Vec<Field> {
        let after = outcome.after();
        let mut fields = vec![
            Field::string("action", self.command.name),
            Field::new("tasks", self.objects(after, outcome.tasks())),
        ];
        fields.extend_from_slice(after_tasks);
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
    }

    /// With `--print-id`, the output is the ids of `tasks`, one per line,
    /// and what it said goes to standard error. Every task passed has an
    /// id (the callers make sure), so one id comes out per task.
    pub(crate) fn print_ids<'a>(
        &self,
        mut output: Output,
        snap: &Snapshot,
        tasks: impl Iterator<Item = &'a Task>,
    ) -> Output {
        if self.has(Opt::PrintId) {
            output.notes = std::mem::take(&mut output.lines);
            output.lines = tasks
                .filter_map(|i| snap.id_of(i))
                .map(|id| visible_bytes(id).into_owned())
                .collect();
        }
        output
    }

    fn objects(&self, snap: &Snapshot, ids: &[TaskId]) -> Json {
        let tasks: Vec<_> =
            ids.iter().filter_map(|id| snap.task(*id)).collect();
        self.task_json(snap).objects(&tasks)
    }
}
