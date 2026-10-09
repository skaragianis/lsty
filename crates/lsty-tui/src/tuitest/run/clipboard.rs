//! The system clipboard of a transcript, which the script fills by
//! hand and which takes what the model writes.

use std::cell::RefCell;
use std::rc::Rc;

use lsty_core::clip::Reader;
use lsty_core::errs::{Code, Error, Result};

use super::{Done, Harness};

/// What the clipboard holds and what was last written to it.
#[derive(Default)]
struct State {
    text: Option<Vec<u8>>,
    failing: bool,
    written: Vec<u8>,
}

/// A clipboard that is read by the model and filled by the script, or by
/// the model as a terminal takes OSC 52.
#[derive(Clone, Default)]
pub(super) struct Clip(Rc<RefCell<State>>);

impl Clip {
    /// Takes `text` as a terminal takes OSC 52.
    pub(super) fn copy(&self, text: Vec<u8>) {
        let mut state = self.0.borrow_mut();
        state.text = Some(text.clone());
        state.written = text;
    }

    fn set(&self, text: Vec<u8>) {
        let mut state = self.0.borrow_mut();
        state.text = Some(text);
        state.failing = false;
    }

    fn fail(&self) {
        self.0.borrow_mut().failing = true;
    }

    fn written(&self) -> Vec<u8> {
        self.0.borrow().written.clone()
    }
}

impl Reader for Clip {
    fn read(&self) -> Result<Vec<u8>> {
        let state = self.0.borrow();
        match (&state.text, state.failing) {
            (Some(text), false) => Ok(text.clone()),
            _ => Err(Error::new(Code::Io, "the clipboard is not readable")),
        }
    }
}

impl Harness {
    /// `clipboard set NAME`, `clipboard expect NAME` and
    /// `clipboard fail`.
    pub(super) fn clipboard(&mut self, args: &str) -> Done {
        let (verb, name) = args.split_once(' ').unwrap_or((args, ""));
        let name = name.trim();
        match verb {
            "set" => {
                let mut text = self.section(name)?;
                if text.last() == Some(&b'\n') {
                    text.pop();
                }
                self.clip.set(text);
                Ok(())
            }
            "expect" => {
                let mut written = self.clip.written();
                written.push(b'\n');
                self.expect(name, &written)
            }
            "fail" => {
                self.clip.fail();
                Ok(())
            }
            _ => Err(format!(
                "clipboard: want set, expect or fail, got {args:?}"
            )),
        }
    }

    /// The text of the section `name`.
    fn section(&self, name: &str) -> Result<Vec<u8>, String> {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data.clone())
            .ok_or_else(|| format!("clipboard: section {name:?} is missing"))
    }
}
