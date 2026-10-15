//! The config file of todo.sh: found, read as text and never run.
//!
//! A line is `NAME=value` or `export NAME=value`. The value takes quotes,
//! `$NAME`, `${NAME}`, a leading `~` and a trailing ` # comment`; nothing
//! is executed. A line that is anything else, or an assignment to a
//! variable lsty reads whose value cannot be worked out, is skipped with a
//! warning and sets nothing.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{
    Vars,
    errs::{self, Code, Error},
    fsio,
};

/// The variables lsty reads. A line that mentions one of these and cannot
/// be worked out is warned about; any other line that cannot is skipped.
const READ: [&str; 8] = [
    "TODO_DIR",
    "TODO_FILE",
    "DONE_FILE",
    "REPORT_FILE",
    "TODOTXT_AUTO_ARCHIVE",
    "TODOTXT_DATE_ON_ADD",
    "TODOTXT_PLAIN",
    "TODOTXT_PRESERVE_LINE_NUMBERS",
];

/// What the config file of todo.sh sets. The default sets nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Todosh {
    /// The path of the file as given; empty when there is none.
    pub source: String,
    /// One message per skipped line, naming its number and the line.
    pub warnings: Vec<String>,
    values: BTreeMap<String, (String, usize)>,
}

impl Todosh {
    /// Reads the file `named` (`-d`), else `$TODOTXT_CFG_FILE`, else the
    /// first of todo.sh's places that exists. With none, nothing is set.
    ///
    /// # Errors
    ///
    /// The file that was named cannot be read, or a place that exists
    /// cannot.
    pub fn read(
        named: Option<&Path>,
        vars: &Vars<'_>,
        home: Option<&Path>,
        cwd: &Path,
    ) -> errs::Result<Self> {
        let named = named.map(Path::to_path_buf).or_else(|| {
            fsio::non_empty(vars, "TODOTXT_CFG_FILE").map(PathBuf::from)
        });
        if let Some(path) = named {
            let message = format!("{} not found", errs::lossy_path(&path));
            return match fsio::read_in(cwd, &path)? {
                Some(data) => Ok(Self::parse(&path, &data, vars, home)),
                None => Err(Error::new(Code::Io, message)),
            };
        }
        for path in places(vars, home) {
            if let Some(data) = fsio::read_in(cwd, &path)? {
                return Ok(Self::parse(&path, &data, vars, home));
            }
        }
        Ok(Self::default())
    }

    /// The value the file gives `name`, with its line number.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<(&str, usize)> {
        let (text, line) = self.values.get(name)?;
        Some((text, *line))
    }

    pub(crate) fn parse(
        path: &Path,
        data: &[u8],
        vars: &Vars<'_>,
        home: Option<&Path>,
    ) -> Self {
        let home = home.and_then(Path::to_str);
        let mut file = Self {
            source: errs::lossy_path(path),
            ..Self::default()
        };
        for (at, raw) in data.split(|&b| b == b'\n').enumerate() {
            file.line(at + 1, raw, vars, home);
        }
        file
    }

    fn line(
        &mut self,
        at: usize,
        raw: &[u8],
        vars: &Vars<'_>,
        home: Option<&str>,
    ) {
        let Ok(text) = std::str::from_utf8(raw) else {
            self.warnings.push(format!("line {at}: not UTF-8"));
            return;
        };
        let text = text.trim_ascii();
        if text.is_empty() || text.starts_with('#') {
            return;
        }
        let body = text
            .strip_prefix("export")
            .filter(|rest| rest.starts_with(BLANK))
            .map_or(text, |rest| rest.trim_start_matches(BLANK));
        let Some((name, value)) =
            body.split_once('=').filter(|(name, _)| is_name(name))
        else {
            self.warnings
                .push(format!("line {at}: not an assignment: {text}"));
            return;
        };
        let earlier = |n: &str| {
            let own = self.values.get(n).map(|(text, _)| text.clone());
            own.or_else(|| vars(n).map(errs::lossy_path))
        };
        match evaluate(value, home, &earlier) {
            Ok(text) => {
                self.values.insert(name.to_owned(), (text, at));
            }
            Err(why) if READ.iter().any(|r| text.contains(r)) => {
                self.warnings.push(format!("line {at}: {why}: {text}"));
            }
            Err(_) => {}
        }
    }
}

/// The places todo.sh looks, in its order, except the one beside its
/// script.
fn places(vars: &Vars<'_>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut places = Vec::new();
    if let Some(home) = home {
        places.extend(
            [".todo/config", "todo.cfg", ".todo.cfg"].map(|p| home.join(p)),
        );
    }
    let config = fsio::non_empty(vars, "XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".config")));
    places.extend(config.map(|dir| dir.join("todo/config")));
    places.push(
        fsio::non_empty(vars, "TODOTXT_GLOBAL_CFG_FILE")
            .map_or_else(|| PathBuf::from("/etc/todo/config"), PathBuf::from),
    );
    places
}

fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// What separates words in a line; any other white space is part of one.
const BLANK: [char; 2] = [' ', '\t'];

type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;
type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// The value `text` stands for, or why it cannot be worked out.
fn evaluate(
    text: &str,
    home: Option<&str>,
    lookup: Lookup<'_>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    if let Some(after) = text.strip_prefix('~') {
        if !(after.is_empty() || after.starts_with(['/', ' ', '\t'])) {
            return Err("cannot work out ~".to_owned());
        }
        out.push_str(home.ok_or("no home folder for ~")?);
        rest = after;
    }
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => single(&mut chars, &mut out)?,
            '"' => double(&mut chars, &mut out, lookup)?,
            '\\' => out.push(chars.next().ok_or("ends in a backslash")?),
            '$' => dollar(&mut chars, &mut out, lookup)?,
            '`' => return Err("cannot run a command".to_owned()),
            ';' | '&' | '|' | '<' | '>' | '(' | ')' => {
                return Err(format!("cannot work out {c}"));
            }
            ':' if chars.peek() == Some(&'~') => {
                return Err("cannot work out :~".to_owned());
            }
            ' ' | '\t' => {
                let after: String = chars.collect();
                let after = after.trim_start_matches(BLANK);
                return if after.is_empty() || after.starts_with('#') {
                    Ok(out)
                } else {
                    Err("text after the value".to_owned())
                };
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

fn single(chars: &mut Chars<'_>, out: &mut String) -> Result<(), String> {
    for c in chars.by_ref() {
        if c == '\'' {
            return Ok(());
        }
        out.push(c);
    }
    Err("quote not closed".to_owned())
}

fn double(
    chars: &mut Chars<'_>,
    out: &mut String,
    lookup: Lookup<'_>,
) -> Result<(), String> {
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(()),
            '\\' => match chars.next_if(|n| "$`\"\\".contains(*n)) {
                Some(escaped) => out.push(escaped),
                None => out.push('\\'),
            },
            '$' => dollar(chars, out, lookup)?,
            '`' => return Err("cannot run a command".to_owned()),
            c => out.push(c),
        }
    }
    Err("quote not closed".to_owned())
}

/// The text after a `$`: `NAME`, `{NAME}`, or a `$` that stands alone.
fn dollar(
    chars: &mut Chars<'_>,
    out: &mut String,
    lookup: Lookup<'_>,
) -> Result<(), String> {
    let name: String = match chars.peek() {
        Some('{') => {
            chars.next();
            let mut closed = false;
            let name: String = chars
                .by_ref()
                .take_while(|&c| {
                    closed = c == '}';
                    !closed
                })
                .collect();
            if !closed || !is_name(&name) {
                return Err(format!("cannot work out ${{{name}}}"));
            }
            name
        }
        Some('(') => return Err("cannot run a command".to_owned()),
        Some(c) if c.is_ascii_digit() || "@*#?$!-'\"[".contains(*c) => {
            return Err(format!("cannot work out ${c}"));
        }
        Some(c) if c.is_ascii_alphabetic() || *c == '_' => {
            let mut name = String::new();
            while let Some(c) =
                chars.next_if(|c| c.is_ascii_alphanumeric() || *c == '_')
            {
                name.push(c);
            }
            name
        }
        _ => {
            out.push('$');
            return Ok(());
        }
    };
    out.push_str(&lookup(&name).ok_or_else(|| format!("{name} is not set"))?);
    Ok(())
}

#[cfg(test)]
mod tests;
