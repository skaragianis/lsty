//! `:theme` without a name: a popup of the themes. The one selected is
//! shown at once; the one chosen is saved to the config file, and
//! closing the popup brings back the theme it opened over.

use std::{fs, io, path::Path};

use lsty_core::{
    case::lower,
    errs::lossy_path,
    keys::{Action, Fed, Mode, Outcome},
    theme::{self, Slot, Theme},
};
use ratatui::{buffer::Buffer, layout::Rect};
use toml::de::{DeTable, DeValue};

use crate::{
    cmdline::full_name,
    field::Field,
    layout::{BORDER, cells},
    line::{Line, LineKind},
    messages::Tone,
    model::Model,
    motion::Step,
    store::{quoted, reason, save},
    styles::Styles,
    text::{Span, fit, pad, titled, width_of, window},
};

const TITLE: &str = " Theme ";
/// How many themes show at once.
const ROWS: usize = 5;
/// The least width inside the borders.
const MIN_INNER: usize = 24;
/// Cells from the left edge of the popup to where the filter is typed.
const PROMPT_OFFSET: usize = 3;
/// The key of the theme in the config file.
const KEY: &str = "theme";

/// The themes to choose from, the filter on them and the theme to go
/// back to.
#[derive(Debug)]
pub(crate) struct ThemePicker {
    names: Vec<String>,
    /// The names that hold the filter, as indices into `names`.
    shown: Vec<usize>,
    /// The selected entry of `shown`.
    at: usize,
    /// The first entry of `shown` in view.
    top: usize,
    filter: String,
    /// The filter while it is typed.
    field: Option<Field>,
    /// The name of the theme shown now.
    showing: String,
}

impl ThemePicker {
    fn new(names: Vec<String>, current: &str) -> Self {
        let at = names.iter().position(|n| n == current).unwrap_or(0);
        let mut picker = Self {
            shown: (0..names.len()).collect(),
            names,
            at,
            top: 0,
            filter: String::new(),
            field: None,
            showing: current.to_owned(),
        };
        picker.top =
            window(picker.top, picker.at, picker.shown.len(), picker.wanted());
        picker
    }

    /// The text the names are filtered by.
    fn query(&self) -> String {
        self.field
            .as_ref()
            .map_or_else(|| self.filter.clone(), Field::text)
    }

    /// The name selected, if any holds the filter.
    fn selected(&self) -> Option<&str> {
        let index = *self.shown.get(self.at)?;
        self.names.get(index).map(String::as_str)
    }

    /// How many rows the popup has when there is room.
    fn wanted(&self) -> usize {
        ROWS.min(self.names.len()).max(1)
    }

    /// Moves the selection, staying on the list.
    fn go(&mut self, step: Step, rows: usize) {
        let pane = (self.shown.len(), rows);
        let top = window(self.top, self.at, pane.0, rows);
        (self.at, self.top) = step.to(pane, Some(self.at), top);
    }

    /// Lists the names that hold the filter, keeping the selected one
    /// when it still does.
    fn refilter(&mut self, rows: usize) {
        let was = self.shown.get(self.at).copied();
        let query = self.query();
        self.shown = (0..self.names.len())
            .filter(|&i| self.names.get(i).is_some_and(|n| holds(n, &query)))
            .collect();
        self.at = was
            .and_then(|w| self.shown.iter().position(|&i| i == w))
            .unwrap_or(0);
        self.top = window(0, self.at, self.shown.len(), rows);
    }

    /// The filter and which of the names show in `rows` from `top`,
    /// when not all of them do.
    fn foot(&self, rows: usize, top: usize) -> String {
        let filter = if self.field.is_some() || !self.filter.is_empty() {
            format!(" /{} ", self.query())
        } else {
            String::new()
        };
        let range = if self.shown.len() > rows {
            let last = (top + rows).min(self.shown.len());
            format!(" {}-{last} of {} ", top + 1, self.shown.len())
        } else {
            String::new()
        };
        format!("{filter}{range}")
    }

    /// The width inside the borders.
    fn inner(&self) -> usize {
        let widest = self
            .names
            .iter()
            .map(|n| width_of(n) + 2)
            .max()
            .unwrap_or(0);
        widest.max(MIN_INNER)
    }
}

/// Whether `name` holds `query`, ignoring case unless `query` has a
/// capital.
fn holds(name: &str, query: &str) -> bool {
    if query.chars().any(lsty_core::query::is_upper) {
        name.contains(query)
    } else {
        lower(name).contains(query)
    }
}

impl Model {
    /// `:theme` without a name: opens the popup on the theme in use.
    pub(crate) fn open_themes(&mut self) {
        let (themes, cwd) = (&self.deps.themes, &self.deps.cwd);
        let names = theme::names(themes, cwd, self.deps.config.no_config);
        self.choosing = Some(ThemePicker::new(names, &self.deps.config.theme));
        self.previewed = Some(self.deps.theme.clone());
        self.preview_theme();
        if let Some(matcher) = self.matchers.get_mut(&Mode::Picker) {
            matcher.reset();
        }
    }

    /// Whether the filter of the theme picker is being typed.
    pub(crate) fn typing_theme(&self) -> bool {
        self.choosing.as_ref().is_some_and(|p| p.field.is_some())
    }

    /// Shows theme `name` at once, with the first warning its file gave.
    pub(crate) fn show_theme(&mut self, name: &str) {
        let (themes, cwd) = (&self.deps.themes, &self.deps.cwd);
        let no_config = self.deps.config.no_config;
        let (theme, warnings) = theme::load(name, themes, cwd, no_config);
        self.use_theme(theme);
        if let Some(w) = warnings.first() {
            self.message = Some(format!("{}: {}", w.source, w.message));
        }
    }

    /// Shows the theme named by the candidate the `:` line stepped to,
    /// or brings back the one before when no name is in the text.
    pub(crate) fn preview_completed(&mut self) {
        let Some(name) = self.completed_theme() else {
            self.end_preview();
            return;
        };
        if self.previewed.is_none() {
            self.previewed = Some(self.deps.theme.clone());
        }
        self.show_theme(&name);
    }

    /// Brings back the theme in use before the `:` line or the popup
    /// showed another.
    pub(crate) fn end_preview(&mut self) {
        if let Some(theme) = self.previewed.take() {
            self.use_theme(theme);
        }
    }

    /// Draws with `theme` from now on.
    fn use_theme(&mut self, theme: Theme) {
        self.deps.theme = theme;
        self.styles = Styles::new(&self.deps.theme, self.deps.profile);
    }

    /// The theme name in the text of `:theme NAME`, when it is the
    /// candidate the completion stepped to.
    fn completed_theme(&self) -> Option<String> {
        let Some(Line {
            field,
            comp,
            kind: LineKind::Command,
        }) = &self.line
        else {
            return None;
        };
        let chosen = comp.as_ref()?.chosen()?;
        let text = field.text();
        let mut words = text.split_whitespace();
        let command = words.next()?;
        let (name, rest) = (words.next()?, words.next());
        (full_name(command).is_ok_and(|c| lower(&c) == "theme")
            && name == chosen
            && rest.is_none())
        .then(|| name.to_owned())
    }

    /// The keys of the theme picker.
    pub(crate) fn theme_keys(&mut self, fed: &Fed) {
        if self.typing_theme() {
            self.theme_filter_keys(fed);
            return;
        }
        let Outcome::Matched { action, count } = fed.outcome else {
            return;
        };
        self.theme_action(action, count);
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the theme picker takes four actions and the motions"
    )]
    fn theme_action(&mut self, action: Action, count: usize) {
        let rows = self.theme_rows();
        let Some(picker) = &mut self.choosing else {
            return;
        };
        match action {
            Action::Search => {
                picker.field = Some(Field::new(&picker.filter));
                if let Some(matcher) = self.matchers.get_mut(&Mode::Command) {
                    matcher.reset();
                }
                return;
            }
            Action::Accept => {
                self.choose_theme();
                return;
            }
            Action::Close => {
                self.close_themes();
                return;
            }
            _ => match Step::of(action, count, rows) {
                Some(step) => picker.go(step, rows),
                None => return,
            },
        }
        self.preview_theme();
    }

    /// The keys of the command table while the filter is typed.
    fn theme_filter_keys(&mut self, fed: &Fed) {
        let rows = self.theme_rows();
        let Some(picker) = &mut self.choosing else {
            return;
        };
        if let Some(field) = &mut picker.field {
            for stroke in &fed.text {
                field.typed(*stroke);
            }
        }
        if let Outcome::Matched { action, .. } = fed.outcome {
            filter_edit(picker, action);
        }
        picker.refilter(rows);
        self.preview_theme();
    }

    /// Shows the selected theme, unless it is shown already.
    fn preview_theme(&mut self) {
        let Some(picker) = &mut self.choosing else {
            return;
        };
        let Some(name) = picker.selected().map(str::to_owned) else {
            return;
        };
        if picker.showing != name {
            picker.showing.clone_from(&name);
            self.show_theme(&name);
        }
    }

    /// Closes the popup and goes back to the theme it opened over.
    fn close_themes(&mut self) {
        self.choosing = None;
        self.end_preview();
    }

    /// Keeps the selected theme and saves it to the config file.
    fn choose_theme(&mut self) {
        let Some(name) = self
            .choosing
            .as_ref()
            .and_then(ThemePicker::selected)
            .map(str::to_owned)
        else {
            return;
        };
        self.choosing = None;
        self.previewed = None;
        self.deps.config.theme.clone_from(&name);
        let file = self
            .deps
            .config_file
            .as_ref()
            .filter(|_| !self.deps.config.no_config);
        let Some(file) = file else {
            self.message = Some(format!("theme {name} for this session"));
            return;
        };
        let path = self.deps.cwd.join(file);
        match write_theme(&path, &name) {
            Ok(()) => self.message = Some(format!("theme {name} saved")),
            Err(why) => {
                self.tone = Tone::Alert;
                self.message = Some(format!("can't save the theme: {why}"));
            }
        }
    }

    /// The popup, centred over the list.
    fn themes_box(&self) -> Option<Rect> {
        let picker = self.choosing.as_ref()?;
        let pane = self.panes(self.dims()).list;
        let w = cells(picker.inner() + BORDER).min(pane.width);
        let h = cells(picker.wanted() + BORDER).min(pane.height);
        Some(Rect::new(
            pane.x + (pane.width - w) / 2,
            pane.y + (pane.height - h) / 2,
            w,
            h,
        ))
    }

    /// How many rows of names the popup draws.
    fn theme_rows(&self) -> usize {
        self.themes_box().map_or(1, |at| {
            usize::from(at.height).saturating_sub(BORDER).max(1)
        })
    }

    /// Draws the theme picker over the list.
    pub(crate) fn draw_themes(&self, buf: &mut Buffer) {
        let (Some(picker), Some(at)) = (&self.choosing, self.themes_box())
        else {
            return;
        };
        crate::draw::clear(buf, at);
        let w = usize::from(at.width);
        let rows = usize::from(at.height).saturating_sub(BORDER);
        let inner = w.saturating_sub(BORDER);
        let first =
            window(picker.top, picker.at, picker.shown.len(), rows.max(1));
        let slot = Some(Slot::PanelBorderActive);
        let top = Span::new(titled("┌", TITLE, "┐", w), slot);
        self.put_at(buf, (at.x, at.y), w, &[top]);
        for i in 0..rows {
            let entry = first + i;
            let name = picker
                .shown
                .get(entry)
                .and_then(|&n| picker.names.get(n))
                .map_or_else(String::new, |n| format!(" {n}"));
            let mut spans =
                fit(vec![Span::new(name, Some(Slot::Text))], inner);
            pad(&mut spans, inner, None);
            let mut row = vec![Span::new("│", slot)];
            row.extend(spans);
            row.push(Span::new("│", slot));
            let y = at.y + 1 + cells(i);
            self.put_at(buf, (at.x, y), w, &row);
            if entry == picker.at && entry < picker.shown.len() {
                let cells = Rect::new(at.x + 1, y, at.width - 2, 1);
                buf.set_style(cells, self.styles.fill(Slot::Selection));
            }
        }
        let foot = fit(vec![Span::new(picker.foot(rows, first), None)], inner);
        let foot: String = foot.iter().map(|s| s.text.as_ref()).collect();
        let bottom = Span::new(titled("└", &foot, "┘", w), slot);
        let y = at.y + at.height.saturating_sub(1);
        self.put_at(buf, (at.x, y), w, &[bottom]);
    }

    /// Where the text cursor is while the filter is typed: in the prompt
    /// on the bottom line of the popup.
    pub(crate) fn theme_cursor(&self) -> Option<(u16, u16)> {
        let field = self.choosing.as_ref()?.field.as_ref()?;
        let at = self.themes_box()?;
        let x = usize::from(at.x) + PROMPT_OFFSET + field.cells();
        let last = usize::from(at.x + at.width).saturating_sub(2);
        let y = at.y + at.height.saturating_sub(1);
        Some((u16::try_from(x.min(last)).ok()?, y))
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "the filter takes only the editing actions"
)]
fn filter_edit(picker: &mut ThemePicker, action: Action) {
    match action {
        Action::Execute => {
            if let Some(field) = picker.field.take() {
                picker.filter = field.text();
            }
        }
        Action::Cancel => picker.field = None,
        _ => {
            if let Some(field) = &mut picker.field {
                field.act(action);
            }
        }
    }
}

/// Sets the theme in the config file at `path`, which is made when it
/// is missing.
fn write_theme(path: &Path, name: &str) -> Result<(), String> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(reason(path, &e)),
    };
    let shown = lossy_path(path);
    let text =
        String::from_utf8(data).map_err(|_| format!("{shown}: not UTF-8"))?;
    let next = with_theme(&text, name).map_err(|why| match why {
        Unset::NotToml => format!("{shown}: not valid TOML"),
        Unset::Table => format!("{shown}: theme is a table, not a name"),
    })?;
    save(path, next.as_bytes())
}

/// Why the theme of a config file could not be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unset {
    /// The file is not TOML.
    NotToml,
    /// `theme` is a table or an array of tables, which a name cannot
    /// replace.
    Table,
}

/// `text`, a config file, with its `theme` set to `name` and everything
/// else as it was.
fn with_theme(text: &str, name: &str) -> Result<String, Unset> {
    let table = DeTable::parse(text).map_err(|_| Unset::NotToml)?;
    let root = table.get_ref();
    let value = quoted(name);
    let theme = root.iter().find(|(k, _)| k.get_ref().as_ref() == KEY);
    if let Some((_, old)) = theme {
        if tables(old.get_ref()) {
            return Err(Unset::Table);
        }
        let span = old.span();
        let (head, tail) = (text.get(..span.start), text.get(span.end..));
        let (Some(head), Some(tail)) = (head, tail) else {
            return Err(Unset::NotToml);
        };
        return checked(format!("{head}{value}{tail}"), name);
    }
    let line = format!("{KEY} = {value}\n");
    let (bom, rest) = text
        .strip_prefix(BOM)
        .map_or(("", text), |r| (&text[..BOM.len_utf8()], r));
    checked(format!("{bom}{line}{rest}"), name)
}

/// The byte order mark a file may start with.
const BOM: char = '\u{feff}';

/// Whether `value` is a table or an array of tables, which a name
/// cannot replace.
fn tables(value: &DeValue<'_>) -> bool {
    value.is_table()
        || value.as_array().is_some_and(|tasks| {
            !tasks.is_empty() && tasks.iter().all(|v| v.get_ref().is_table())
        })
}

/// `out`, when it reads back with `theme` set to `name`.
fn checked(out: String, name: &str) -> Result<String, Unset> {
    let check = DeTable::parse(&out).map_err(|_| Unset::NotToml)?;
    let saved = check
        .get_ref()
        .iter()
        .find(|(k, _)| k.get_ref().as_ref() == KEY)
        .and_then(|(_, v)| v.get_ref().as_str());
    (saved == Some(name)).then_some(out).ok_or(Unset::NotToml)
}

#[cfg(test)]
mod themes_tests;
