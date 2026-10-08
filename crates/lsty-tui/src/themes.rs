//! `:theme` without a name: a popup of the themes. The one selected is
//! shown at once; the one chosen is saved to the config file, and
//! closing the popup brings back the theme it opened over.

use std::fs;
use std::io;
use std::path::Path;

use lsty_core::case::lower;
use lsty_core::keys::{Action, Fed, Mode, Outcome};
use lsty_core::theme::{self, Slot, Theme};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use toml::de::{DeTable, DeValue};

use crate::field::Field;
use crate::layout::BORDER;
use crate::line::Line;
use crate::messages::Tone;
use crate::model::Model;
use crate::store::{quoted, reason, save};
use crate::styles::Styles;
use crate::text::{Span, fit, pad, put, width};

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
        picker.top = picker.window();
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

    /// How many rows the popup has.
    fn rows(&self) -> usize {
        ROWS.min(self.names.len()).max(1)
    }

    /// The first entry in view, with the selected one in it.
    fn window(&self) -> usize {
        let rows = self.rows();
        let last = self.shown.len().saturating_sub(rows);
        self.top
            .max((self.at + 1).saturating_sub(rows))
            .min(self.at)
            .min(last)
    }

    /// Moves the selection by `by`, staying on the list.
    fn shift(&mut self, by: isize) {
        let last = self.shown.len().saturating_sub(1);
        self.at = self.at.saturating_add_signed(by).min(last);
        self.top = self.window();
    }

    /// Lists the names that hold the filter, keeping the selected one
    /// when it still does.
    fn refilter(&mut self) {
        let was = self.shown.get(self.at).copied();
        let query = self.query();
        self.shown = (0..self.names.len())
            .filter(|&i| self.names.get(i).is_some_and(|n| holds(n, &query)))
            .collect();
        self.at = was
            .and_then(|w| self.shown.iter().position(|&i| i == w))
            .unwrap_or(0);
        self.top = 0;
        self.top = self.window();
    }

    /// The filter and which names show when not all of them do.
    fn foot(&self) -> String {
        let filter = if self.field.is_some() || !self.filter.is_empty() {
            format!(" /{} ", self.query())
        } else {
            String::new()
        };
        let rows = self.rows();
        let range = if self.shown.len() > rows {
            let last = (self.top + rows).min(self.shown.len());
            format!(" {}-{last} of {} ", self.top + 1, self.shown.len())
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

fn width_of(text: &str) -> usize {
    width(&[Span::new(text, None)])
}

/// `left`, `title`, dashes up to `w` cells, `right`.
fn titled(left: &str, title: &str, right: &str, w: usize) -> String {
    let used = 2 + width_of(title);
    format!("{left}{title}{}{right}", "─".repeat(w.saturating_sub(used)))
}

impl Model {
    /// `:theme` without a name: opens the popup on the theme in use.
    pub(crate) fn open_themes(&mut self) {
        let (themes, cwd) = (&self.deps.themes, &self.deps.cwd);
        let names = theme::names(themes, cwd, self.deps.config.no_config);
        self.choosing = Some(ThemePicker::new(names, &self.deps.config.theme));
        self.previewed = Some(self.deps.theme.clone());
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
        let Some(Line::Command { field, comp }) = &self.line else {
            return None;
        };
        let chosen = comp.as_ref()?.chosen()?;
        let text = field.text();
        let mut words = text.split_whitespace();
        let command = words.next()?;
        let (name, rest) = (words.next()?, words.next());
        (command.eq_ignore_ascii_case("theme")
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
        let Outcome::Matched { action, .. } = fed.outcome else {
            return;
        };
        self.theme_action(action);
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "the theme picker takes five actions"
    )]
    fn theme_action(&mut self, action: Action) {
        let Some(picker) = &mut self.choosing else {
            return;
        };
        match action {
            Action::CursorDown => picker.shift(1),
            Action::CursorUp => picker.shift(-1),
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
            _ => return,
        }
        self.preview_theme();
    }

    /// The keys of the command table while the filter is typed.
    fn theme_filter_keys(&mut self, fed: &Fed) {
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
        picker.refilter();
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
        let cells = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        let w = cells(picker.inner() + BORDER).min(pane.width);
        let h = cells(picker.rows() + BORDER).min(pane.height);
        Some(Rect::new(
            pane.x + (pane.width - w) / 2,
            pane.y + (pane.height - h) / 2,
            w,
            h,
        ))
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
        let slot = Some(Slot::PanelBorderActive);
        let top = Span::new(titled("┌", TITLE, "┐", w), slot);
        self.put_cells(buf, (at.x, at.y), w, &[top]);
        for i in 0..rows {
            let entry = picker.top + i;
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
            let y = at.y + 1 + u16::try_from(i).unwrap_or(0);
            self.put_cells(buf, (at.x, y), w, &row);
            if entry == picker.at && entry < picker.shown.len() {
                let cells = Rect::new(at.x + 1, y, at.width - 2, 1);
                buf.set_style(cells, self.styles.fill(Slot::Selection));
            }
        }
        let foot = fit(vec![Span::new(picker.foot(), None)], inner);
        let foot: String = foot.iter().map(|s| s.text.as_ref()).collect();
        let bottom = Span::new(titled("└", &foot, "┘", w), slot);
        let y = at.y + at.height.saturating_sub(1);
        self.put_cells(buf, (at.x, y), w, &[bottom]);
    }

    fn put_cells(
        &self,
        buf: &mut Buffer,
        at: (u16, u16),
        w: usize,
        spans: &[Span<'_>],
    ) {
        if at.1 < self.height.min(buf.area.height) {
            put(buf, at, w, spans, &self.styles);
        }
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
    let shown = path.display();
    let text =
        String::from_utf8(data).map_err(|_| format!("{shown}: not UTF-8"))?;
    let next = with_theme(&text, name)
        .ok_or_else(|| format!("{shown}: not valid TOML"))?;
    save(path, next.as_bytes())
}

/// `text`, a config file, with its `theme` set to `name` and everything
/// else as it was; `None` when `text` is not TOML.
pub(crate) fn with_theme(text: &str, name: &str) -> Option<String> {
    let table = DeTable::parse(text).ok()?;
    let root = table.get_ref();
    let value = quoted(name);
    let theme = root.iter().find(|(k, _)| k.get_ref().as_ref() == KEY);
    let out = if let Some((_, old)) = theme {
        let span = old.span();
        let (head, tail) = (text.get(..span.start)?, text.get(span.end..)?);
        format!("{head}{value}{tail}")
    } else {
        let (head, tail) = text.split_at_checked(insertion(text, root))?;
        let gap = if head.is_empty() || head.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        format!("{head}{gap}{KEY} = {value}\n{tail}")
    };
    let check = DeTable::parse(&out).ok()?;
    let saved = check
        .get_ref()
        .iter()
        .find(|(k, _)| k.get_ref().as_ref() == KEY)?;
    (saved.1.get_ref().as_str()? == name).then_some(out)
}

/// Where a new key of the top table goes: after the line of its last
/// plain value, else before the first table, else at the end.
fn insertion(text: &str, root: &DeTable<'_>) -> usize {
    let last = root
        .iter()
        .filter(|(_, v)| plain(v.get_ref()))
        .map(|(_, v)| v.span().end)
        .max();
    if let Some(end) = last {
        let rest = text.get(end..).unwrap_or_default();
        return rest.find('\n').map_or(text.len(), |n| end + n + 1);
    }
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            return at;
        }
        at += line.len();
    }
    text.len()
}

/// Whether a value of the top table is written on its key's line rather
/// than under a table header.
fn plain(value: &DeValue<'_>) -> bool {
    match value {
        DeValue::Table(_) => false,
        DeValue::Array(items) => !items.iter().any(|v| v.get_ref().is_table()),
        DeValue::String(_)
        | DeValue::Integer(_)
        | DeValue::Float(_)
        | DeValue::Boolean(_)
        | DeValue::Datetime(_) => true,
    }
}
