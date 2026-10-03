//! The default keys of every mode.

use super::Mode;

/// An action and the keys it is bound to, as written.
pub(super) type Entry = (&'static str, &'static [&'static str]);

/// The default entries of `mode`, in the order they are listed.
pub(super) const fn defaults(mode: Mode) -> &'static [Entry] {
    match mode {
        Mode::Normal => NORMAL,
        Mode::Visual => VISUAL,
        Mode::Insert => INSERT,
        Mode::Completion => COMPLETION,
        Mode::Command => COMMAND,
        Mode::Panel => PANEL,
        Mode::Help => HELP,
        Mode::Picker => PICKER,
    }
}

const NORMAL: &[Entry] = &[
    ("cursor_down", &["j", "<Down>"]),
    ("cursor_up", &["k", "<Up>"]),
    ("go_top", &["gg"]),
    ("go_bottom", &["G"]),
    ("half_page_down", &["<C-d>"]),
    ("half_page_up", &["<C-u>"]),
    ("edit_description", &["i"]),
    ("edit_append", &["a", "A", "<CR>"]),
    ("edit_line_start", &["I"]),
    ("change_description", &["cc", "S"]),
    ("insert_below", &["o"]),
    ("insert_above", &["O"]),
    ("cycle_state", &["t"]),
    ("stop", &["T"]),
    ("toggle_complete", &["x"]),
    ("delete", &["dd"]),
    ("yank", &["yy"]),
    ("paste_below", &["p"]),
    ("paste_above", &["P"]),
    ("move_item_down", &["J"]),
    ("move_item_up", &["K"]),
    ("cycle_priority", &["gp"]),
    ("cycle_priority_back", &["gP"]),
    ("set_due", &["gd"]),
    ("add_tag", &["gt"]),
    ("remove_tag", &["gT"]),
    ("open_link", &["gx"]),
    ("undo", &["u"]),
    ("redo", &["<C-r>"]),
    ("repeat", &["."]),
    ("search", &["/"]),
    ("cycle_sort", &["s"]),
    ("toggle_sidebar", &["gs"]),
    ("toggle_detail", &["gi"]),
    ("enter_visual", &["v", "V"]),
    ("focus_left", &["<C-w>h"]),
    ("focus_down", &["<C-w>j"]),
    ("focus_up", &["<C-w>k"]),
    ("focus_right", &["<C-w>l"]),
    ("focus_next", &["<Tab>"]),
    ("focus_prev", &["<S-Tab>"]),
    ("command_line", &[":"]),
    ("help", &["?"]),
    ("quit", &["q", "ZZ"]),
];

const VISUAL: &[Entry] = &[
    ("cursor_down", &["j", "<Down>"]),
    ("cursor_up", &["k", "<Up>"]),
    ("go_top", &["gg"]),
    ("go_bottom", &["G"]),
    ("half_page_down", &["<C-d>"]),
    ("half_page_up", &["<C-u>"]),
    ("cycle_state", &["t"]),
    ("stop", &["T"]),
    ("toggle_complete", &["x"]),
    ("delete", &["d"]),
    ("yank", &["y"]),
    ("move_item_down", &["J"]),
    ("move_item_up", &["K"]),
    ("cycle_priority", &["gp"]),
    ("cycle_priority_back", &["gP"]),
    ("set_due", &["gd"]),
    ("add_tag", &["gt"]),
    ("remove_tag", &["gT"]),
    ("exit_visual", &["<Esc>", "v", "V"]),
];

const INSERT: &[Entry] = &[
    ("commit", &["<CR>", "<Esc>"]),
    ("cancel", &["<C-c>"]),
    ("cursor_left", &["<Left>"]),
    ("cursor_right", &["<Right>"]),
    ("cursor_row_up", &["<Up>"]),
    ("cursor_row_down", &["<Down>"]),
    ("line_start", &["<Home>"]),
    ("line_end", &["<End>"]),
    ("delete_word_back", &["<C-w>"]),
    ("delete_to_start", &["<C-u>"]),
];

const COMPLETION: &[Entry] = &[
    ("accept", &["<Tab>", "<CR>"]),
    ("close", &["<Esc>"]),
    ("next", &["<C-n>", "<Down>"]),
    ("prev", &["<C-p>", "<Up>"]),
];

const COMMAND: &[Entry] = &[
    ("execute", &["<CR>"]),
    ("cancel", &["<Esc>", "<C-c>"]),
    ("complete_next", &["<Tab>"]),
    ("complete_prev", &["<S-Tab>"]),
    ("history_prev", &["<Up>"]),
    ("history_next", &["<Down>"]),
    ("cursor_left", &["<Left>"]),
    ("cursor_right", &["<Right>"]),
    ("line_start", &["<Home>"]),
    ("line_end", &["<End>"]),
    ("delete_word_back", &["<C-w>"]),
    ("delete_to_start", &["<C-u>"]),
];

const PANEL: &[Entry] = &[
    ("cursor_down", &["j", "<Down>"]),
    ("cursor_up", &["k", "<Up>"]),
    ("go_top", &["gg"]),
    ("go_bottom", &["G"]),
    ("toggle_entry", &["<Space>"]),
    ("select_and_jump", &["<CR>"]),
    ("fold", &["h"]),
    ("unfold", &["l"]),
    ("focus_left", &["<C-w>h"]),
    ("focus_down", &["<C-w>j"]),
    ("focus_up", &["<C-w>k"]),
    ("focus_right", &["<C-w>l"]),
    ("focus_next", &["<Tab>"]),
    ("focus_prev", &["<S-Tab>"]),
    ("toggle_sidebar", &["gs"]),
    ("toggle_detail", &["gi"]),
    ("search", &["/"]),
    ("command_line", &[":"]),
    ("help", &["?"]),
    ("quit", &["q", "ZZ"]),
];

const HELP: &[Entry] = &[
    ("cursor_down", &["j", "<Down>"]),
    ("cursor_up", &["k", "<Up>"]),
    ("half_page_down", &["<C-d>"]),
    ("half_page_up", &["<C-u>"]),
    ("search", &["/"]),
    ("close", &["<Esc>", "q", "?"]),
];

const PICKER: &[Entry] = &[
    ("cursor_down", &["j", "<Down>"]),
    ("cursor_up", &["k", "<Up>"]),
    ("accept", &["<CR>"]),
    ("close", &["<Esc>", "q"]),
];
