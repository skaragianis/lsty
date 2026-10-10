//! Theme files: the built-in ones and `themes/<name>.toml` over them.

use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

use toml::de::DeValue;

use super::{
    Colour, Slot, Style, Theme,
    builtin::{BUILTIN, builtin},
    value_text::value_text,
};
use crate::{
    config::{Table, Warning, parse_table},
    errs::quote,
    fsio,
};

/// The theme used when `config.toml` names none.
pub const DEFAULT_NAME: &str = "catppuccin-mocha";

/// The theme that fills what another theme leaves out.
const FALLBACK: &str = "ansi";
const EXTENSION: &str = ".toml";

type Styles = [Style; Slot::ALL.len()];

/// The names of the built-in themes and of the files in `themes_dir`,
/// sorted and without repeats. A relative `themes_dir` is taken from
/// `cwd`. With `no_config` the folder is not read.
pub fn names(themes_dir: &Path, cwd: &Path, no_config: bool) -> Vec<String> {
    let mut set: BTreeSet<String> =
        BUILTIN.iter().map(|&(name, _)| name.to_owned()).collect();
    if !no_config {
        set.extend(user_names(&cwd.join(themes_dir)));
    }
    set.into_iter().collect()
}

/// The stems of the `.toml` entries of `dir` that are not folders.
/// Names that are not UTF-8 are left out.
fn user_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| !e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let file = e.file_name();
            let stem = file.to_str()?.strip_suffix(EXTENSION)?;
            Some(stem.to_owned())
        })
        .collect()
}

/// Loads the theme called `name`. It never fails: each problem becomes a
/// warning and the affected part keeps its fallback. A relative
/// `themes_dir` is taken from `cwd`. With `no_config` user themes are
/// ignored.
///
/// A file in `themes_dir` with a built-in theme's name overrides it; a
/// slot it leaves out takes the built-in theme's, or `ansi`'s when there
/// is no built-in theme of that name.
pub fn load(
    mut name: &str,
    themes_dir: &Path,
    cwd: &Path,
    no_config: bool,
) -> (Theme, Vec<Warning>) {
    let file_of = |theme: &str| {
        (!no_config).then(|| themes_dir.join(format!("{theme}{EXTENSION}")))
    };
    let mut warnings = Vec::new();
    let mut user = file_of(name);
    if builtin(name).is_none() && !is_file(cwd, user.as_deref()) {
        let message = format!("theme: no theme {}", quote(name));
        warnings.push(Warning::config(message));
        name = DEFAULT_NAME;
        user = file_of(name);
    }
    let mut styles = builtin_styles(name);
    if let Some(path) = user.filter(|u| is_file(cwd, Some(u.as_path()))) {
        let source = format!("themes/{name}{EXTENSION}");
        let (read, more) = read_user(&source, cwd, &path, &styles);
        styles = read;
        warnings.extend(more);
    }
    (Theme { styles }, warnings)
}

fn is_file(cwd: &Path, user: Option<&Path>) -> bool {
    user.is_some_and(|path| {
        fs::metadata(cwd.join(path)).is_ok_and(|m| !m.is_dir())
    })
}

/// The styles of a built-in theme over those of `ansi`; `ansi` for a
/// name that is no built-in theme.
fn builtin_styles(name: &str) -> Styles {
    let ansi = builtin(FALLBACK).unwrap_or_default();
    let (base, _) = parse(FALLBACK, ansi.as_bytes(), &Styles::default());
    match builtin(name).filter(|_| name != FALLBACK) {
        Some(text) => parse(name, text.as_bytes(), &base).0,
        None => base,
    }
}

fn read_user(
    source: &str,
    cwd: &Path,
    path: &Path,
    base: &Styles,
) -> (Styles, Vec<Warning>) {
    match fsio::read_in(cwd, path) {
        Ok(Some(data)) => parse(source, &data, base),
        Ok(None) => (*base, Vec::new()),
        Err(e) => (*base, vec![Warning::new(source, e.message().to_owned())]),
    }
}

/// `base` with the slots of the file applied, and the file's warnings.
pub(super) fn parse(
    source: &str,
    data: &[u8],
    base: &Styles,
) -> (Styles, Vec<Warning>) {
    match parse_table(data) {
        Ok(table) => Reader::new(source, table.get_ref()).apply_all(base),
        Err(message) => (*base, vec![Warning::new(source, message)]),
    }
}

/// What is needed to read the slots of one file.
struct Reader<'a> {
    source: &'a str,
    /// The palette entries that are strings.
    palette: HashMap<&'a str, &'a str>,
    /// The entries of `[slots]`, in file order.
    slots: Vec<(&'a str, &'a DeValue<'a>)>,
}

impl<'a> Reader<'a> {
    fn new(source: &'a str, top: &'a Table<'a>) -> Self {
        let palette = table_in(top, "palette")
            .map(string_entries)
            .unwrap_or_default();
        let slots = table_in(top, "slots").map(in_file_order);
        Self {
            source,
            palette,
            slots: slots.unwrap_or_default(),
        }
    }

    fn warn(&self, message: String) -> Warning {
        Warning::new(self.source, message)
    }

    fn apply_all(&self, base: &Styles) -> (Styles, Vec<Warning>) {
        let mut styles = *base;
        let mut warnings = Vec::new();
        for &(name, value) in &self.slots {
            let Some(slot) = Slot::parse(name) else {
                let message = format!("unknown slot {}", quote(name));
                warnings.push(self.warn(message));
                continue;
            };
            if let Some(style) = self.style(name, value, &mut warnings) {
                styles[slot.index()] = style;
            }
        }
        (styles, warnings)
    }

    fn style(
        &self,
        name: &str,
        value: &DeValue<'_>,
        warnings: &mut Vec<Warning>,
    ) -> Option<Style> {
        let shown = match value {
            DeValue::String(text) => {
                if let Some(colour) = self.resolve(text) {
                    return Some(Style::fg(colour));
                }
                quote(text.as_bytes())
            }
            DeValue::Table(table) => {
                return self.table_style(name, table, warnings);
            }
            DeValue::Integer(_)
            | DeValue::Float(_)
            | DeValue::Boolean(_)
            | DeValue::Datetime(_)
            | DeValue::Array(_) => format!("\"{}\"", value_text(value)),
        };
        warnings.push(self.warn(format!("{name}: bad colour {shown}")));
        None
    }

    /// The style a slot table makes, unless one of its values is bad.
    fn table_style(
        &self,
        name: &str,
        table: &Table<'_>,
        warnings: &mut Vec<Warning>,
    ) -> Option<Style> {
        let mut style = Style::default();
        let mut failed = false;
        for (key, value) in in_file_order(table) {
            let message = match self.set_key(&mut style, key, value) {
                Ok(()) => continue,
                Err(Problem::Unknown) => {
                    format!("{name}: unknown key {}", quote(key))
                }
                Err(Problem::Flag) => {
                    failed = true;
                    format!("{name}: {key}: expected bool")
                }
                Err(Problem::Colour) => {
                    failed = true;
                    format!("{name}: bad colour \"{}\"", value_text(value))
                }
            };
            warnings.push(self.warn(message));
        }
        (!failed).then_some(style)
    }

    fn set_key(
        &self,
        style: &mut Style,
        key: &str,
        value: &DeValue<'_>,
    ) -> Result<(), Problem> {
        let flag = match key {
            "bold" => &mut style.bold,
            "italic" => &mut style.italic,
            "underline" => &mut style.underline,
            "strike" => &mut style.strike,
            "faint" => &mut style.faint,
            "fg" | "bg" => {
                let colour = self.colour_of(value).ok_or(Problem::Colour)?;
                *(if key == "fg" {
                    &mut style.fg
                } else {
                    &mut style.bg
                }) = Some(colour);
                return Ok(());
            }
            _ => return Err(Problem::Unknown),
        };
        let DeValue::Boolean(on) = value else {
            return Err(Problem::Flag);
        };
        *flag = *on;
        Ok(())
    }

    fn colour_of(&self, value: &DeValue<'_>) -> Option<Colour> {
        let DeValue::String(text) = value else {
            return None;
        };
        self.resolve(text)
    }

    /// The colour a text stands for: a literal, or a palette entry that
    /// leads to one without going round in a circle.
    fn resolve(&self, text: &str) -> Option<Colour> {
        let mut current = text;
        for _ in 0..=self.palette.len() {
            if let Some(colour) = literal(current) {
                return Some(colour);
            }
            current = self.palette.get(current)?;
        }
        None
    }
}

/// Why a key of a slot table is not used.
enum Problem {
    Unknown,
    Flag,
    Colour,
}

/// The entries of `table` whose values are strings.
fn string_entries<'a>(table: &'a Table<'a>) -> HashMap<&'a str, &'a str> {
    table
        .iter()
        .filter_map(|(k, v)| {
            let DeValue::String(s) = v.get_ref() else {
                return None;
            };
            Some((k.get_ref().as_ref(), s.as_ref()))
        })
        .collect()
}

/// The table under `key` of `top`, if it is one.
fn table_in<'a>(top: &'a Table<'a>, key: &str) -> Option<&'a Table<'a>> {
    let DeValue::Table(table) = top.get(key)?.get_ref() else {
        return None;
    };
    Some(table)
}

/// The entries of `table` ordered by where their keys are in the file.
fn in_file_order<'a>(table: &'a Table<'a>) -> Vec<(&'a str, &'a DeValue<'a>)> {
    let mut entries: Vec<_> = table.iter().collect();
    entries.sort_by_key(|(k, _)| k.span().start);
    entries
        .into_iter()
        .map(|(k, v)| (k.get_ref().as_ref(), v.get_ref()))
        .collect()
}

/// `#rgb`, `#rrggbb` or a number from 0 to 255.
fn literal(text: &str) -> Option<Colour> {
    if let Some(hex) = text.strip_prefix('#') {
        return hex_colour(hex);
    }
    let digits = !text.is_empty()
        && text.len() <= 3
        && text.bytes().all(|b| b.is_ascii_digit());
    if !digits {
        return None;
    }
    text.parse::<u8>().ok().map(Colour::Index)
}

fn hex_colour(hex: &str) -> Option<Colour> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit =
        |i: usize| hex.get(i..=i).and_then(|d| u8::from_str_radix(d, 16).ok());
    match hex.len() {
        3 => Some(Colour::Rgb(digit(0)? * 17, digit(1)? * 17, digit(2)? * 17)),
        6 => {
            let pair = |i: usize| Some(digit(i)? * 16 + digit(i + 1)?);
            Some(Colour::Rgb(pair(0)?, pair(2)?, pair(4)?))
        }
        _ => None,
    }
}
