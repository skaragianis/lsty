//! `lsty help keys`: the actions, modes and default keys that `keys.toml`
//! and the `[hints]` of `config.toml` take, from the tables the program
//! itself uses.

use lsty_core::{
    config::Config,
    keys::{Action, Category, Mode, about, defaults, key_names},
};

use super::{GUIDE_WIDTH, wrap};

const INTRO: &str = "\
Actions are what keys.toml binds to keys and what [hints] in config.toml \
shows in the hint row. The keys listed are the defaults, the same for \
everyone; your own keys.toml is not read.";

/// The notation paragraph; the key names come from the parser's table.
fn notation() -> String {
    let names: Vec<String> =
        key_names().map(|name| format!("<{name}>")).collect();
    format!(
        "A key is one character (j, G, /); several in a row are a sequence \
(gg, dd). A name in angle brackets is a special key: {}; <lt> is < (a bare \
< is not a key) and <gt> is >. Names ignore case. A modifier goes before the \
key inside the brackets: <C-d> is Ctrl, <A-x> Alt, <S-Tab> Shift, and they \
combine (<C-A-x>). <leader> is the leader option of config.toml, {} unless \
set.",
        names.join(" "),
        Config::default().leader,
    )
}

const MODES: &str = "\
Each mode is a table of keys.toml, and a name in [hints]; an action can only \
be bound in the modes below that list it, but detail takes any action of \
normal. An empty list unbinds an action. The detail panel uses the keys of \
normal, except for an action the detail table binds or unbinds, which keeps \
only its detail keys; a detail key wins over a normal key it starts or \
continues.";

/// The example of the page; it loads without a warning.
pub(super) const EXAMPLE: [&str; 9] = [
    "keys.toml:",
    "  [normal]",
    "  toggle_complete = [\"x\", \"<leader>d\"]",
    "  delete = [\"dd\", \"<Del>\"]",
    "  cycle_sort = []",
    "",
    "config.toml:",
    "  [hints]",
    "  normal = [\"toggle_complete\", \"insert_below\", \"undo\", \"help\"]",
];

/// The text of `lsty help keys`.
pub(super) fn keys_page() -> Vec<String> {
    let prose = |text| wrap("", "", text, GUIDE_WIDTH);
    let mut lines =
        vec!["lsty help keys: actions and default keys".to_owned()];
    let notation = notation();
    for text in [INTRO, &notation, MODES] {
        lines.push(String::new());
        lines.extend(prose(text));
    }
    let modes: Vec<&str> = Mode::ALL.map(Mode::as_str).into();
    lines.extend(prose(&format!("Modes: {}", modes.join(" "))));
    let name_width = Action::ALL
        .iter()
        .map(|a| a.to_string().len())
        .max()
        .unwrap_or(0);
    let keys_width = Mode::ALL
        .into_iter()
        .flat_map(defaults)
        .map(|(_, keys)| keys.join(", ").len())
        .max()
        .unwrap_or(0);
    for mode in Mode::ALL {
        lines.extend([String::new(), mode.as_str().to_owned()]);
        for category in Category::ALL {
            let entries: Vec<_> = Action::ALL
                .iter()
                .filter(|action| action.category() == category)
                .filter_map(|a| defaults(mode).iter().find(|(d, _)| d == a))
                .collect();
            if entries.is_empty() {
                continue;
            }
            lines.push(format!("  {}", category.as_str()));
            for (action, keys) in entries {
                let (name, keys) = (action.to_string(), keys.join(", "));
                let action = *action;
                let head =
                    format!("    {name:name_width$}  {keys:keys_width$}  ");
                let hang = " ".repeat(head.len());
                lines.extend(wrap(
                    &head,
                    &hang,
                    about(mode, action),
                    GUIDE_WIDTH,
                ));
            }
        }
    }
    lines.extend([String::new(), "example:".to_owned()]);
    lines.extend(EXAMPLE.map(str::to_owned));
    lines
}

#[cfg(test)]
mod keys_tests;
