//! Handing the terminal to the editor and taking it back.

use std::ffi::OsString;

use crate::model::Model;

impl Model {
    /// Runs the editor `words` with the terminal handed over, and waits
    /// for it.
    ///
    /// Fails with how the editor failed.
    pub(crate) fn run_editor(&self, words: &[OsString]) -> Result<(), String> {
        (self.deps.run_editor)(words)
    }

    /// The editor has ended; says so when it failed.
    pub(crate) fn editor_exited(&mut self, result: Result<(), String>) {
        if let Err(why) = result {
            self.alert(format!("editor exited: {why}"));
        }
    }
}
