//! Running one transcript.

use std::cell::Cell as Shared;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use lsty_core::config::{self, Paths};
use lsty_core::dates::{Clock, parse_duration};
use lsty_core::discover::{self, Input};
use lsty_core::keys::{self, Stroke};
use lsty_core::theme::{self, Profile};
use lsty_scripttest::txtar::ByteArchive;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use regex::Regex;

use super::legend::letters;
use super::screen::{frame_lines, join, layer_lines};
use crate::{Cmd, Deps, Model, Msg};

const DEFAULT_SIZE: (u16, u16) = (80, 24);
const DEFAULT_NOW: &str = "2026-09-29T0900Z";
const THEME: &str = "probe";

type Done = Result<(), String>;

/// Runs a transcript. The result says how it ended; the bytes are the new
/// file when `update` rewrote an expected section.
pub(super) fn script(
    root: &Path,
    name: &str,
    source: &[u8],
    update: bool,
) -> (Done, Option<Vec<u8>>) {
    let dir = match tempfile::Builder::new().prefix("lsty-tui-").tempdir() {
        Ok(dir) => dir,
        Err(e) => return (Err(format!("{name}: {e}")), None),
    };
    let work = fs::canonicalize(dir.path())
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let mut harness = Harness::new(root, work, source, update);
    let result = harness.script().map_err(|why| format!("{name}: {why}"));
    let text = harness.modified.then(|| harness.archive.format());
    (result, text)
}

fn stamp(text: &str) -> Result<Timestamp, String> {
    let var = |name: &str| (name == "LSTY_NOW").then(|| OsString::from(text));
    Clock::new(&var, Timestamp::UNIX_EPOCH, TimeZone::UTC)
        .map(|clock| clock.now())
        .map_err(|_| format!("now: want YYYY-MM-DDTHHMMZ, got {text:?}"))
}

struct Harness {
    root: PathBuf,
    dir: PathBuf,
    archive: ByteArchive,
    update: bool,
    modified: bool,
    size: (u16, u16),
    now: Rc<Shared<Timestamp>>,
    leader: Option<Stroke>,
    letters: HashMap<u8, char>,
    model: Option<Model>,
    timers: Vec<(Timestamp, Msg)>,
    line: usize,
}

impl Harness {
    fn new(root: &Path, dir: PathBuf, source: &[u8], update: bool) -> Self {
        let start = stamp(DEFAULT_NOW).unwrap_or(Timestamp::UNIX_EPOCH);
        Self {
            root: root.to_path_buf(),
            dir,
            archive: ByteArchive::parse(source),
            update,
            modified: false,
            size: DEFAULT_SIZE,
            now: Rc::new(Shared::new(start)),
            leader: None,
            letters: HashMap::new(),
            model: None,
            timers: Vec::new(),
            line: 0,
        }
    }

    fn script(&mut self) -> Done {
        let comment = self.archive.comment.clone();
        for (i, raw) in comment.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            self.line = i + 1;
            let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
            self.command(name, rest.trim())
                .map_err(|why| format!("line {}: {why}", self.line))?;
        }
        Ok(())
    }

    fn command(&mut self, name: &str, args: &str) -> Done {
        match name {
            "size" => self.set_size(args),
            "now" => self.set_now(args),
            "start" => self.start(args),
            "keys" => self.press_all(args),
            "type" => self.type_text(args),
            "frame" => self.expect(args, join(&self.frame()?).as_bytes()),
            "fg" => self.expect(args, self.layer(|c| c.fg)?.as_bytes()),
            "bg" => self.expect(args, self.layer(|c| c.bg)?.as_bytes()),
            "status" => self.status(args),
            "file" => self.file(args),
            "tick" => self.tick(args),
            _ => Err(format!("unknown command {name:?}")),
        }
    }

    fn set_size(&mut self, args: &str) -> Done {
        let mut nums = args.split_whitespace().map(str::parse::<u16>);
        let (Some(Ok(w)), Some(Ok(h)), None) =
            (nums.next(), nums.next(), nums.next())
        else {
            return Err(format!("size: want W H, got {args:?}"));
        };
        self.size = (w, h);
        if self.model.is_some() {
            self.send(Msg::Resize(w, h))?;
        }
        Ok(())
    }

    fn set_now(&mut self, args: &str) -> Done {
        self.now.set(stamp(args)?);
        Ok(())
    }

    fn start(&mut self, args: &str) -> Done {
        self.write_files()?;
        let file = if args.is_empty() { "todo.txt" } else { args };
        let deps = self.deps(file)?;
        let model = Model::new(deps).map_err(|e| e.message().to_owned())?;
        let init = model.init();
        self.model = Some(model);
        self.send(Msg::Resize(self.size.0, self.size.1))?;
        self.deliver(init);
        Ok(())
    }

    /// The sections that the script compares the screen or a file with;
    /// they are not files of the list.
    fn expectations(&self) -> Vec<String> {
        let mut names = Vec::new();
        for raw in self.archive.comment.lines() {
            let (name, rest) = raw.trim().split_once(' ').unwrap_or((raw, ""));
            let words: Vec<&str> = rest.split_whitespace().collect();
            match (name, words.as_slice()) {
                ("file", [_, section])
                | ("frame" | "fg" | "bg", [section]) => {
                    names.push((*section).to_owned());
                }
                _ => {}
            }
        }
        names
    }

    fn write_files(&self) -> Done {
        let expected = self.expectations();
        for (name, data) in &self.archive.files {
            if expected.contains(name) {
                continue;
            }
            let path = self.dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&path, data).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn deps(&mut self, file: &str) -> Result<Deps, String> {
        let dir = self.dir.clone();
        let paths = Paths {
            config_file: dir.join("config.toml"),
            keys_file: dir.join("keys.toml"),
            themes_dir: dir.join("themes"),
            state_dir: dir.join("state"),
            explicit: false,
        };
        let (config, mut warnings) = config::load(&paths, &dir, false);
        let (keymap, more) =
            keys::load(&paths.keys_file, &dir, false, &config.leader);
        warnings.extend(more);
        let themes = self.root.join("testdata").join("themes");
        let (theme, more) = theme::load(THEME, &themes, &dir, false);
        warnings.extend(more);
        let explicit = Path::new(file);
        let vars = |_: &str| None;
        let found = discover::find(&Input {
            explicit: Some(explicit),
            vars: &vars,
            cwd: &dir,
            home: Some(&dir),
            default_file: None,
        })
        .map_err(|e| format!("find {file}: {}", e.message()))?;
        self.leader =
            keys::parse_keys(&config.leader, None)
                .ok()
                .and_then(|strokes| match strokes.as_slice() {
                    [one] => Some(*one),
                    _ => None,
                });
        self.letters = letters(&theme);
        let now = Rc::clone(&self.now);
        Ok(Deps {
            found,
            config,
            keys: keymap,
            theme,
            profile: Profile::TrueColor,
            warnings,
            now: Box::new(move || now.get()),
            zone: TimeZone::UTC,
            home: Some(dir),
        })
    }

    fn press_all(&mut self, args: &str) -> Done {
        for arg in args.split_whitespace() {
            let strokes = keys::parse_keys(arg, self.leader)
                .map_err(|e| format!("keys: {}", e.message()))?;
            for stroke in strokes {
                self.send(Msg::Key(stroke))?;
            }
        }
        Ok(())
    }

    fn type_text(&mut self, text: &str) -> Done {
        for c in text.replace('␣', " ").chars() {
            self.send(Msg::Key(Stroke::char(c)))?;
        }
        Ok(())
    }

    fn send(&mut self, msg: Msg) -> Done {
        let Some(model) = self.model.as_mut() else {
            return Err("no model: use start first".to_owned());
        };
        let cmds = model.update(msg);
        self.deliver(cmds);
        Ok(())
    }

    /// Carries out what the model asked for: timers wait for `tick`.
    fn deliver(&mut self, cmds: Vec<Cmd>) {
        for cmd in cmds {
            if let Cmd::After { delay, msg } = cmd {
                let due = self.now.get().checked_add(delay);
                self.timers.extend(due.ok().map(|at| (at, msg)));
            }
        }
    }

    fn buffer(&self) -> Result<Buffer, String> {
        let Some(model) = &self.model else {
            return Err("no model: use start first".to_owned());
        };
        let (w, h) = self.size;
        let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
        model.view(&mut buf);
        Ok(buf)
    }

    fn frame(&self) -> Result<Vec<String>, String> {
        Ok(frame_lines(&self.buffer()?))
    }

    fn layer(
        &self,
        pick: fn(&ratatui::buffer::Cell) -> ratatui::style::Color,
    ) -> Result<String, String> {
        Ok(join(&layer_lines(&self.buffer()?, &self.letters, pick)))
    }

    fn status(&mut self, pattern: &str) -> Done {
        let re = Regex::new(pattern).map_err(|e| format!("status: {e}"))?;
        let lines = self.frame()?;
        let last = lines.last().map_or("", String::as_str);
        if re.is_match(last) {
            Ok(())
        } else {
            Err(format!("status line {last:?} does not match {pattern:?}"))
        }
    }

    fn file(&mut self, args: &str) -> Done {
        let (path, name) = args
            .split_once(' ')
            .ok_or_else(|| format!("file: want PATH NAME, got {args:?}"))?;
        let data =
            fs::read(self.dir.join(path)).map_err(|e| format!("file: {e}"))?;
        self.expect(name.trim(), &data)
    }

    fn expect(&mut self, name: &str, got: &[u8]) -> Done {
        let name = name.trim();
        let at = self.archive.files.iter().position(|(n, _)| n == name);
        let Some(at) = at else {
            if !self.update {
                return Err(format!(
                    "section {name:?} is missing; run with UPDATE_SCRIPTS=1"
                ));
            }
            self.archive.files.push((name.to_owned(), got.to_vec()));
            self.modified = true;
            return Ok(());
        };
        let Some((_, want)) = self.archive.files.get_mut(at) else {
            return Ok(());
        };
        if want == got {
            return Ok(());
        }
        if !self.update {
            let (want, got) =
                (String::from_utf8_lossy(want), String::from_utf8_lossy(got));
            return Err(format!("{name} differs\nwant:\n{want}\ngot:\n{got}"));
        }
        got.clone_into(want);
        self.modified = true;
        Ok(())
    }

    fn tick(&mut self, args: &str) -> Done {
        let by: Duration = parse_duration(args)
            .map_err(|e| format!("tick: {}", e.message()))?;
        let target = self
            .now
            .get()
            .checked_add(by)
            .map_err(|e| format!("tick: {e}"))?;
        self.advance(target)
    }

    /// Moves the clock to `target`, delivering the timers that fall due
    /// on the way in due order.
    fn advance(&mut self, target: Timestamp) -> Done {
        while let Some(at) = self.next_due(target) {
            let (due, msg) = self.timers.remove(at);
            self.now.set(due);
            self.send(msg)?;
        }
        self.now.set(target);
        Ok(())
    }

    fn next_due(&self, target: Timestamp) -> Option<usize> {
        self.timers
            .iter()
            .enumerate()
            .filter(|(_, (due, _))| *due <= target)
            .min_by_key(|(_, (due, _))| *due)
            .map(|(i, _)| i)
    }
}
