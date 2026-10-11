//! `lsty help themes`: how a custom theme file is written, the slots it
//! sets and the built-in themes, from the tables the theme loader uses.

use std::path::Path;

use lsty_core::theme::{
    COLOUR_KEYS, DEFAULT_NAME, FALLBACK, FLAG_KEYS, Slot, names,
};

use super::{GUIDE_WIDTH, wrap};

/// What the overview says of the page.
pub(super) const ABOUT: &str = "custom themes and the built-in ones";

/// The example of the page; it loads without a warning.
pub(super) const EXAMPLE: [&str; 11] = [
    "[palette]",
    "ink    = \"#d8dee9\"",
    "accent = \"#ff8800\"",
    "alert  = \"accent\"",
    "",
    "[slots]",
    "text        = \"ink\"",
    "project     = \"accent\"",
    "muted       = \"244\"",
    "priority_a  = { fg = \"alert\", bold = true }",
    "due_overdue = { fg = \"#f00\", underline = true }",
];

/// The first paragraph; the names come from the loader.
fn intro() -> String {
    format!(
        "A theme gives each part of the screen a look. Name the one to use \
in the theme option of config.toml, or with :theme in the TUI. Besides the \
built-in themes, each file NAME.toml in $XDG_CONFIG_HOME/lsty/themes \
(~/.config/lsty/themes when the variable is unset or not absolute) is a theme \
called NAME. LSTY_CONFIG moves config.toml, not that folder; --no-config \
ignores it. A slot a file leaves out looks like the same slot of the \
built-in theme of that name, so a file named like one adjusts it; a file with \
any other name starts from {FALLBACK}. A name that is neither falls back to \
{DEFAULT_NAME} with a warning."
    )
}

const PALETTE: &str = "[palette] names colours: ink = \"#d8dee9\". A name may \
stand for another name; a loop is a bad colour. A value that is not a string \
is dropped without a warning. Only the palette of the file itself is known, \
not those of the built-in themes.";

const COLOURS: &str = "A colour is always a string in quotes: \"#rgb\" or \
\"#rrggbb\" in hex, \"0\" to \"255\" for the terminal's 256 colours (0 to 15 \
are its own 16), or a palette name. A number without quotes is a bad colour.";

const SLOTS: &str = "[slots] gives a slot a colour, its foreground: text = \
\"ink\"; or a table of keys: priority_a = { fg = \"alert\", bold = true }. \
Either replaces the look the slot would have had, so a key left out is off. \
The slots selection, cursorline and status_bg are fills: they paint their bg, \
or their fg when there is no bg. The keys:";

const WARNINGS: &str = "A bad value does not stop a theme from loading. A \
slot with a bad colour, or a key that is not true or false, keeps the look it \
would have had; an unknown slot or key is skipped; a file that is not valid \
TOML is ignored. Each of these is a warning. The commands print all of them \
on stderr, \"lsty: warning: themes/NAME.toml: ...\", when they print in \
colour. The TUI shows them one at a time, a key press apart, as \
\"themes/NAME.toml: ... (1 of 3)\"; :theme NAME shows only the first. An \
unknown top-level key or table gives no warning.";

/// The text of `lsty help themes`.
pub(super) fn themes_page() -> Vec<String> {
    let prose = |text: &str| wrap("", "", text, GUIDE_WIDTH);
    let mut lines = vec![format!("lsty help themes: {ABOUT}")];
    for text in [&intro(), PALETTE, COLOURS, SLOTS] {
        lines.push(String::new());
        lines.extend(prose(text));
    }
    let keys = [
        (COLOUR_KEYS.map(|(k, _)| k).join(", "), "a colour"),
        (FLAG_KEYS.map(|(k, _)| k).join(", "), "true or false"),
    ];
    let width = keys.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    lines.extend(keys.map(|(k, what)| format!("  {k:width$}  {what}")));
    let width = Slot::ALL.map(|s| s.name().len()).into_iter().max();
    lines.extend([String::new(), "slots:".to_owned()]);
    lines.extend(Slot::ALL.map(|s| {
        format!("  {:w$}  {}", s.name(), s.about(), w = width.unwrap_or(0))
    }));
    let themes = names(Path::new(""), Path::new(""), true).join(" ");
    lines.extend([String::new(), "built-in themes:".to_owned()]);
    lines.extend(wrap("  ", "  ", &themes, GUIDE_WIDTH));
    lines.extend([String::new(), "example, themes/mine.toml:".to_owned()]);
    lines.extend(EXAMPLE.map(|l| format!("  {l}").trim_end().to_owned()));
    lines.push(String::new());
    lines.extend(prose(WARNINGS));
    lines
}

#[cfg(test)]
mod themes_tests;
