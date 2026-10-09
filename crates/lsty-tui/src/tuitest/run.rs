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
use lsty_core::errs::Result as Fallible;
use lsty_core::keys::{self, Stroke};
use lsty_core::lock::{self, Cancel, Held, Mode, Want};
use lsty_core::ops::IdSource;
use lsty_core::theme::{self, Profile};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use regex::Regex;

use super::legend::letters;
use super::screen::{frame_lines, join, layer_lines};
use crate::model::Model;
use crate::msg::{Cmd, Msg};
use crate::{Deps, Notice, Start, Watch, Watcher};

const NO_MODEL: &str = "no model: use start first";
const DEFAULT_SIZE: (u16, u16) = (80, 24);
const DEFAULT_NOW: &str = "2026-09-29T0900Z";
const THEME: &str = "probe";
/// How long `lock hold` waits for the lock.
const HOLD_TIMEOUT: Duration = Duration::from_secs(1);

mod clipboard;
mod launch;
mod mouse;

/// How a transcript ended.
type Done = Result<(), String>;

/// The sections of a transcript: name and contents.
pub(super) type Files = Vec<(String, Vec<u8>)>;

/// Runs a transcript: its script and the sections that follow it. The
/// result says how it ended; the sections are the new ones when `update`
/// rewrote an expected section.
pub fn script(
    root: &Path,
    name: &str,
    comment: &str,
    files: Files,
    update: bool,
) -> (Done, Option<Files>) {
    let dir = match tempfile::Builder::new().prefix("lsty-tui-").tempdir() {
        Ok(dir) => dir,
        Err(e) => return (Err(format!("{name}: {e}")), None),
    };
    let work = fs::canonicalize(dir.path())
        .unwrap_or_else(|_| dir.path().to_path_buf());
    let mut harness = Harness::new(root, work, comment, files, update);
    let result = harness
        .script(comment)
        .map_err(|why| format!("{name}: {why}"));
    (result, harness.modified.then_some(harness.files))
}

/// The sections that the script compares the screen or a file with;
/// they are not files of the list.
fn expectations(comment: &str) -> Vec<String> {
    let mut names = Vec::new();
    for raw in comment.lines() {
        let (name, rest) = raw.trim().split_once(' ').unwrap_or((raw, ""));
        let words: Vec<&str> = rest.split_whitespace().collect();
        match (name, words.as_slice()) {
            ("file", [_, section])
            | ("clipboard", ["expect", section])
            | ("frame" | "fg" | "bg", [section]) => {
                names.push((*section).to_owned());
            }
            _ => {}
        }
    }
    names
}

fn stamp(text: &str) -> Result<Timestamp, String> {
    let var = |name: &str| (name == "LSTY_NOW").then(|| OsString::from(text));
    Clock::new(&var, Timestamp::UNIX_EPOCH, TimeZone::UTC)
        .map(|clock| clock.now())
        .map_err(|_| format!("now: want YYYY-MM-DDTHHMMZ, got {text:?}"))
}

/// The key that stands for `<Leader>`, when the setting is one key.
fn leader_stroke(leader: &str) -> Option<Stroke> {
    keys::parse_keys(leader, None).ok().and_then(|strokes| {
        match strokes.as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    })
}

/// Draws the ids `t001`, `t002` and so on.
#[derive(Default)]
struct Counter(Shared<usize>);

impl IdSource for Counter {
    fn next(&self, length: usize) -> Fallible<String> {
        self.0.set(self.0.get() + 1);
        let digits = length.saturating_sub(1);
        Ok(format!("t{:0digits$}", self.0.get()))
    }
}

/// What the fake watcher holds: whether the watch is dead (the folder is
/// gone, or the watch failed) until it is added again.
#[derive(Default)]
struct Fake {
    dead: Shared<bool>,
}

/// A watcher that the script feeds by hand.
struct FakeWatcher(Rc<Fake>);

impl Watcher for FakeWatcher {
    fn rewatch(&mut self) -> Result<(), String> {
        self.0.dead.set(false);
        Ok(())
    }
}

struct Harness {
    root: PathBuf,
    dir: PathBuf,
    expected: Vec<String>,
    files: Files,
    update: bool,
    modified: bool,
    size: (u16, u16),
    now: Rc<Shared<Timestamp>>,
    leader: Option<Stroke>,
    letters: HashMap<u8, char>,
    model: Option<Model>,
    timers: Vec<(Timestamp, Msg)>,
    line: usize,
    fake: Option<Rc<Fake>>,
    todo: PathBuf,
    held: Option<Held>,
    clip: clipboard::Clip,
    launch: launch::Launch,
}

/// The watch the harness starts: it dies when the folder is gone.
fn fake_watch(fake: Rc<Fake>) -> Watch {
    Box::new(move |found, _| {
        let folder = found.todo.parent();
        fake.dead.set(!folder.is_some_and(Path::is_dir));
        Ok(Box::new(FakeWatcher(Rc::clone(&fake))))
    })
}

impl Harness {
    fn new(
        root: &Path,
        dir: PathBuf,
        comment: &str,
        files: Files,
        update: bool,
    ) -> Self {
        let start = stamp(DEFAULT_NOW).unwrap_or(Timestamp::UNIX_EPOCH);
        Self {
            root: root.to_path_buf(),
            dir,
            expected: expectations(comment),
            files,
            update,
            modified: false,
            size: DEFAULT_SIZE,
            now: Rc::new(Shared::new(start)),
            leader: None,
            letters: HashMap::new(),
            model: None,
            timers: Vec::new(),
            line: 0,
            fake: None,
            todo: PathBuf::new(),
            held: None,
            clip: clipboard::Clip::default(),
            launch: launch::Launch::default(),
        }
    }

    fn script(&mut self, comment: &str) -> Done {
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
            "paste" => self.paste(args),
            "cursor" => self.expect(args, self.cursor_mark()?.as_bytes()),
            "frame" => self.expect(args, join(&self.frame()?).as_bytes()),
            "fg" => self.expect(args, self.layer(|c| c.fg)?.as_bytes()),
            "bg" => self.expect(args, self.layer(|c| c.bg)?.as_bytes()),
            "status" => self.status(args),
            "file" => self.file(args),
            "tick" => self.tick(args),
            "write" => self.write(args),
            "remove" => self.remove(args),
            "event" => self.event(args),
            "lock" => self.lock(args),
            "clipboard" => self.clipboard(args),
            _ => self
                .launch_command(name, args)
                .or_else(|| self.mouse_command(name, args))
                .unwrap_or_else(|| Err(format!("unknown command {name:?}"))),
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
        let (start, deps) = self.inputs(file)?;
        let model =
            Model::new(start, deps).map_err(|e| e.message().to_owned())?;
        let init = model.init();
        self.model = Some(model);
        self.send(Msg::Resize(self.size.0, self.size.1))?;
        self.deliver(init)
    }

    fn write_files(&self) -> Done {
        for (name, data) in &self.files {
            if self.expected.contains(name) {
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

    fn inputs(&mut self, file: &str) -> Result<(Start, Deps), String> {
        let dir = self.dir.clone();
        let paths = Paths {
            config_file: dir.join("config.toml"),
            keys_file: dir.join("keys.toml"),
            themes_dir: dir.join("themes"),
            state_dir: dir.join("state").join("lsty"),
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
        self.leader = leader_stroke(&config.leader);
        self.letters = letters(&theme);
        self.todo.clone_from(&found.todo);
        let fake = Rc::new(Fake::default());
        self.fake = Some(Rc::clone(&fake));
        let now = Rc::clone(&self.now);
        let deps = Deps {
            config,
            keys: keymap,
            theme,
            profile: Profile::TrueColor,
            ids: Box::new(Counter::default()),
            clipboard: Box::new(self.clip.clone()),
            now: Box::new(move || now.get()),
            zone: TimeZone::UTC,
            home: Some(dir.clone()),
            themes,
            config_file: Some(paths.config_file),
            cwd: dir,
            editor: None,
            opener: self.launch.opener(),
            run_editor: launch::no_editor(),
        };
        Ok((
            Start {
                found,
                warnings,
                watch: Some(fake_watch(fake)),
                state_dir: Some(paths.state_dir),
            },
            deps,
        ))
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

    fn paste(&mut self, args: &str) -> Done {
        let text = unquote(args).ok_or_else(|| {
            format!("paste: want a quoted string, got {args:?}")
        })?;
        self.send(Msg::Paste(text))
    }

    /// The screen row of the text cursor, with a caret under the cell it
    /// is on.
    fn cursor_mark(&self) -> Result<String, String> {
        let Some(model) = &self.model else {
            return Err(NO_MODEL.to_owned());
        };
        let Some((x, y)) = model.cursor_at() else {
            return Ok("no cursor\n".to_owned());
        };
        let lines = self.frame()?;
        let Some(line) = lines.get(usize::from(y)) else {
            return Err(format!("cursor row {y} is off the screen"));
        };
        Ok(format!("{line}\n{}^\n", " ".repeat(usize::from(x))))
    }

    fn send(&mut self, msg: Msg) -> Done {
        let Some(model) = self.model.as_mut() else {
            return Err(NO_MODEL.to_owned());
        };
        if self.launch.suspended() {
            self.launch.queue(msg);
            return Ok(());
        }
        let cmds = model.update(msg);
        self.deliver(cmds)
    }

    /// Carries out what the model asked for: timers wait for `tick`, and
    /// a reading is done at once.
    fn deliver(&mut self, cmds: Vec<Cmd>) -> Done {
        for cmd in cmds {
            match cmd {
                Cmd::After { delay, msg } => {
                    let due = self.now.get().checked_add(delay);
                    self.timers.extend(due.ok().map(|at| (at, msg)));
                }
                Cmd::Load(load) => self.send(load.read())?,
                Cmd::Copy(text) => self.clip.copy(text),
                Cmd::Edit(words) => self.edit(&words)?,
                Cmd::Quit => {}
            }
        }
        Ok(())
    }

    fn buffer(&self) -> Result<Buffer, String> {
        let Some(model) = &self.model else {
            return Err(NO_MODEL.to_owned());
        };
        let (w, h) = self.size;
        let mut buf = Buffer::empty(Rect::new(0, 0, w, h));
        model.draw(&mut buf);
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
        let at = self.files.iter().position(|(n, _)| n == name);
        let Some(at) = at else {
            if !self.update {
                return Err(format!(
                    "section {name:?} is missing; run with UPDATE_SCRIPTS=1"
                ));
            }
            self.files.push((name.to_owned(), got.to_vec()));
            self.modified = true;
            return Ok(());
        };
        let Some((_, want)) = self.files.get_mut(at) else {
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

impl Harness {
    /// Tells the model what the watcher saw, unless the watch is dead.
    fn notify(&mut self, notice: Notice) -> Done {
        if self.fake.as_ref().is_some_and(|f| f.dead.get()) {
            return Ok(());
        }
        self.send(Msg::Notice(notice))
    }

    fn event(&mut self, args: &str) -> Done {
        let Some(reason) = args.strip_prefix("error") else {
            return self.notify(Notice::Changed);
        };
        self.notify(Notice::Failed(reason.trim().to_owned()))?;
        if let Some(fake) = &self.fake {
            fake.dead.set(true);
        }
        Ok(())
    }

    /// The text of the section `name`.
    fn section(&self, cmd: &str, name: &str) -> Result<Vec<u8>, String> {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data.clone())
            .ok_or_else(|| format!("{cmd}: section {name:?} is missing"))
    }

    /// Replaces a file as another writer would, with a temporary file
    /// and a rename, and sends an event.
    fn write(&mut self, args: &str) -> Done {
        let (name, section) = args.split_once(' ').ok_or_else(|| {
            format!("write: want FILE SECTION, got {args:?}")
        })?;
        let section = section.trim();
        let data = self.section("write", section)?;
        let path = self.dir.join(name);
        let mut staged = path.clone().into_os_string();
        staged.push(".tmp");
        let io = |e: std::io::Error| format!("write: {e}");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        fs::write(&staged, data).map_err(io)?;
        fs::rename(&staged, &path).map_err(io)?;
        self.notify(Notice::Changed)
    }

    fn remove(&mut self, name: &str) -> Done {
        let path = self.dir.join(name);
        let was_dir = path.is_dir();
        let gone = if was_dir {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match gone {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("remove: {e}"));
            }
            Ok(()) | Err(_) => {}
        }
        self.notify(Notice::Changed)?;
        if was_dir && let Some(fake) = &self.fake {
            fake.dead.set(true);
        }
        Ok(())
    }

    /// Holds or frees the lock of the todo file's folder.
    fn lock(&mut self, args: &str) -> Done {
        match args {
            "hold" => {
                if self.held.is_some() {
                    return Err("lock: already held".to_owned());
                }
                let want = Want {
                    file: self.todo.clone(),
                    mode: Mode::Exclusive,
                };
                let held =
                    lock::acquire(&[want], HOLD_TIMEOUT, &Cancel::new())
                        .map_err(|e| format!("lock: {}", e.message()))?;
                self.held = Some(held);
                Ok(())
            }
            "release" => {
                if let Some(mut held) = self.held.take() {
                    held.release()
                        .map_err(|e| format!("lock: {}", e.message()))?;
                }
                Ok(())
            }
            _ => Err(format!("lock: want hold or release, got {args:?}")),
        }
    }
}

/// The text of a double-quoted string with these escapes:
/// `\a \b \f \n \r \t \v \\ \"`, octal `\NNN`, `\xHH`, `\uHHHH` and
/// `\UHHHHHHHH`. `\'` is refused. A `\x` or octal escape is one byte, so
/// a string that is not UTF-8 is refused.
pub(super) fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out: Vec<u8> = Vec::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\n' => return None,
            '\\' => escape(&mut chars, &mut out)?,
            _ => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    String::from_utf8(out).ok()
}

/// Reads one escape after a backslash into `out`.
fn escape(chars: &mut std::str::Chars<'_>, out: &mut Vec<u8>) -> Option<()> {
    let c = chars.next()?;
    let simple = match c {
        'a' => Some(7),
        'b' => Some(8),
        'f' => Some(12),
        'n' => Some(b'\n'),
        'r' => Some(b'\r'),
        't' => Some(b'\t'),
        'v' => Some(11),
        '\\' => Some(b'\\'),
        '"' => Some(b'"'),
        _ => None,
    };
    if let Some(byte) = simple {
        out.push(byte);
        return Some(());
    }
    match c {
        'x' => out.push(u8::try_from(hex(chars, 2)?).ok()?),
        'u' | 'U' => {
            let c = char::from_u32(hex(chars, if c == 'u' { 4 } else { 8 })?)?;
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
        '0'..='7' => {
            let mut v = c.to_digit(8)?;
            for _ in 0..2 {
                v = v * 8 + chars.next()?.to_digit(8)?;
            }
            out.push(u8::try_from(v).ok()?);
        }
        _ => return None,
    }
    Some(())
}

/// `digits` hexadecimal digits as a number.
fn hex(chars: &mut std::str::Chars<'_>, digits: u32) -> Option<u32> {
    let mut v: u32 = 0;
    for _ in 0..digits {
        v = v
            .checked_mul(16)?
            .checked_add(chars.next()?.to_digit(16)?)?;
    }
    Some(v)
}
