//! The fake opener and the fake editor of a transcript: they record what
//! the model starts and never run a program.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::Path;
use std::rc::Rc;

use lsty_core::errs::lossy;

use super::{Done, Harness};
use crate::msg::Msg;
use crate::{EditorRun, Opener};

/// What the fake opener says when it is not installed.
const NO_OPENER: &str = "executable file not found in $PATH";
/// The first bytes of a file that is not text.
const BINARY: &[u8] = b"%PDF\x00\x01";
const FILE_MODE: u32 = 0o644;

/// What the opener and the editor were asked for, and how the editor
/// behaves.
#[derive(Default)]
pub(super) struct Launch {
    started: Rc<RefCell<Vec<String>>>,
    missing: Rc<Cell<bool>>,
    /// How the next editor ends, `None` for a success.
    exit: Option<String>,
    /// Whether the next editor stays open until `editor return`.
    holding: bool,
    /// The result of the editor that is open.
    done: Option<Msg>,
    /// What came while the editor was open.
    queued: Vec<Msg>,
}

impl Launch {
    /// Whether an editor is open.
    pub(super) const fn suspended(&self) -> bool {
        self.done.is_some()
    }

    pub(super) fn queue(&mut self, msg: Msg) {
        self.queued.push(msg);
    }

    /// The opener that records into this launch.
    pub(super) fn opener(&self) -> Opener {
        let started = Rc::clone(&self.started);
        let missing = Rc::clone(&self.missing);
        Box::new(move |path: &Path| {
            if missing.get() {
                return Err(NO_OPENER.to_owned());
            }
            let shown = lossy(path.as_os_str().as_encoded_bytes());
            started.borrow_mut().push(format!("opened {shown}"));
            Ok(())
        })
    }
}

/// The editor of the real terminal, which a transcript never runs: the
/// harness records the command instead.
pub(super) fn no_editor() -> EditorRun {
    Box::new(|_| Err("no editor in a transcript".to_owned()))
}

impl Harness {
    /// Runs one of the commands about opening files; `None` when `name`
    /// is not one.
    pub(super) fn launch_command(
        &mut self,
        name: &str,
        args: &str,
    ) -> Option<Done> {
        Some(match name {
            "opened" | "edited" => self.expect_started(name, args),
            "opener" => self.opener_command(args),
            "editor" => self.editor_command(args),
            "binary" => self.binary(args),
            "chmod" => self.chmod(args),
            "symlink" => self.symlink(args),
            _ => return None,
        })
    }

    /// The model asked for the editor: it is recorded, and it ends at
    /// once unless the script holds it.
    pub(super) fn edit(&mut self, words: &[OsString]) -> Done {
        let path = words
            .last()
            .map_or_else(String::new, |w| lossy(w.as_encoded_bytes()));
        self.launch
            .started
            .borrow_mut()
            .push(format!("edited {path}"));
        let result = match self.launch.exit.clone() {
            Some(why) => Err(why),
            None => Ok(()),
        };
        let msg = Msg::Edited(result);
        if self.launch.holding {
            self.launch.holding = false;
            self.launch.done = Some(msg);
            return Ok(());
        }
        self.send(msg)
    }

    fn where_is(&self, text: &str) -> String {
        text.replace("$WORK", &self.dir.to_string_lossy())
    }

    fn expect_started(&mut self, verb: &str, args: &str) -> Done {
        let want = if args == "none" {
            Vec::new()
        } else {
            vec![format!("{verb} {}", self.where_is(args))]
        };
        let got = self.launch.started.take();
        if got == want {
            Ok(())
        } else {
            Err(format!("started {got:?}, want {want:?}"))
        }
    }

    fn opener_command(&mut self, args: &str) -> Done {
        if args != "missing" {
            return Err(format!("opener: want missing, got {args:?}"));
        }
        self.launch.missing.set(true);
        Ok(())
    }

    fn editor_command(&mut self, args: &str) -> Done {
        let (verb, status) = args.split_once(' ').unwrap_or((args, ""));
        match verb {
            "exit" => {
                let n: u32 = status.trim().parse().map_err(|_| {
                    format!("editor: want exit N, got {args:?}")
                })?;
                self.launch.exit = Some(format!("exit status {n}"));
                Ok(())
            }
            "hold" => {
                self.launch.holding = true;
                Ok(())
            }
            "return" => self.resume(),
            _ => {
                Err(format!("editor: want exit, hold or return, got {args:?}"))
            }
        }
    }

    /// Ends the editor: the model hears the result, then everything that
    /// came while it ran.
    fn resume(&mut self) -> Done {
        let done = self.launch.done.take();
        let queued = std::mem::take(&mut self.launch.queued);
        if let Some(msg) = done {
            self.send(msg)?;
        }
        for msg in queued {
            self.send(msg)?;
        }
        Ok(())
    }

    fn binary(&mut self, name: &str) -> Done {
        let path = self.dir.join(name);
        let io = |e: std::io::Error| format!("binary: {e}");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        fs::write(&path, BINARY).map_err(io)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(FILE_MODE))
            .map_err(io)
    }

    fn chmod(&mut self, args: &str) -> Done {
        let (mode, name) = args
            .split_once(' ')
            .ok_or_else(|| format!("chmod: want MODE PATH, got {args:?}"))?;
        let bits = u32::from_str_radix(mode, 8)
            .map_err(|_| format!("chmod: want MODE PATH, got {args:?}"))?;
        fs::set_permissions(
            self.dir.join(name),
            fs::Permissions::from_mode(bits),
        )
        .map_err(|e| format!("chmod: {e}"))
    }

    fn symlink(&mut self, args: &str) -> Done {
        let (name, target) = args.split_once(' ').ok_or_else(|| {
            format!("symlink: want NAME TARGET, got {args:?}")
        })?;
        let target = self.where_is(target);
        symlink(target, self.dir.join(name))
            .map_err(|e| format!("symlink: {e}"))
    }
}
