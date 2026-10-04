//! The default keys of every mode.

use super::{Action, Mode};

/// An action and the keys it is bound to, as written.
pub(super) type Entry = (Action, &'static [&'static str]);

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
    (Action::CursorDown, &["j", "<Down>"]),
    (Action::CursorUp, &["k", "<Up>"]),
    (Action::GoTop, &["gg"]),
    (Action::GoBottom, &["G"]),
    (Action::HalfPageDown, &["<C-d>"]),
    (Action::HalfPageUp, &["<C-u>"]),
    (Action::EditDescription, &["i"]),
    (Action::EditAppend, &["a", "A", "<CR>"]),
    (Action::EditLineStart, &["I"]),
    (Action::ChangeDescription, &["cc", "S"]),
    (Action::InsertBelow, &["o"]),
    (Action::InsertAbove, &["O"]),
    (Action::CycleState, &["t"]),
    (Action::Stop, &["T"]),
    (Action::ToggleComplete, &["x"]),
    (Action::Delete, &["dd"]),
    (Action::Yank, &["yy"]),
    (Action::PasteBelow, &["p"]),
    (Action::PasteAbove, &["P"]),
    (Action::MoveItemDown, &["J"]),
    (Action::MoveItemUp, &["K"]),
    (Action::CyclePriority, &["gp"]),
    (Action::CyclePriorityBack, &["gP"]),
    (Action::SetDue, &["gd"]),
    (Action::AddTag, &["gt"]),
    (Action::RemoveTag, &["gT"]),
    (Action::OpenLink, &["gx"]),
    (Action::Undo, &["u"]),
    (Action::Redo, &["<C-r>"]),
    (Action::Repeat, &["."]),
    (Action::Search, &["/"]),
    (Action::CycleSort, &["s"]),
    (Action::ToggleSidebar, &["gs"]),
    (Action::ToggleDetail, &["gi"]),
    (Action::EnterVisual, &["v", "V"]),
    (Action::FocusLeft, &["<C-w>h"]),
    (Action::FocusDown, &["<C-w>j"]),
    (Action::FocusUp, &["<C-w>k"]),
    (Action::FocusRight, &["<C-w>l"]),
    (Action::FocusNext, &["<Tab>"]),
    (Action::FocusPrev, &["<S-Tab>"]),
    (Action::CommandLine, &[":"]),
    (Action::Help, &["?"]),
    (Action::Quit, &["q", "ZZ"]),
];

const VISUAL: &[Entry] = &[
    (Action::CursorDown, &["j", "<Down>"]),
    (Action::CursorUp, &["k", "<Up>"]),
    (Action::GoTop, &["gg"]),
    (Action::GoBottom, &["G"]),
    (Action::HalfPageDown, &["<C-d>"]),
    (Action::HalfPageUp, &["<C-u>"]),
    (Action::CycleState, &["t"]),
    (Action::Stop, &["T"]),
    (Action::ToggleComplete, &["x"]),
    (Action::Delete, &["d"]),
    (Action::Yank, &["y"]),
    (Action::MoveItemDown, &["J"]),
    (Action::MoveItemUp, &["K"]),
    (Action::CyclePriority, &["gp"]),
    (Action::CyclePriorityBack, &["gP"]),
    (Action::SetDue, &["gd"]),
    (Action::AddTag, &["gt"]),
    (Action::RemoveTag, &["gT"]),
    (Action::ExitVisual, &["<Esc>", "v", "V"]),
];

const INSERT: &[Entry] = &[
    (Action::Commit, &["<CR>", "<Esc>"]),
    (Action::Cancel, &["<C-c>"]),
    (Action::CursorLeft, &["<Left>"]),
    (Action::CursorRight, &["<Right>"]),
    (Action::CursorRowUp, &["<Up>"]),
    (Action::CursorRowDown, &["<Down>"]),
    (Action::LineStart, &["<Home>"]),
    (Action::LineEnd, &["<End>"]),
    (Action::DeleteWordBack, &["<C-w>"]),
    (Action::DeleteToStart, &["<C-u>"]),
];

const COMPLETION: &[Entry] = &[
    (Action::Accept, &["<Tab>", "<CR>"]),
    (Action::Close, &["<Esc>"]),
    (Action::Next, &["<C-n>", "<Down>"]),
    (Action::Prev, &["<C-p>", "<Up>"]),
];

const COMMAND: &[Entry] = &[
    (Action::Execute, &["<CR>"]),
    (Action::Cancel, &["<Esc>", "<C-c>"]),
    (Action::CompleteNext, &["<Tab>"]),
    (Action::CompletePrev, &["<S-Tab>"]),
    (Action::HistoryPrev, &["<Up>"]),
    (Action::HistoryNext, &["<Down>"]),
    (Action::CursorLeft, &["<Left>"]),
    (Action::CursorRight, &["<Right>"]),
    (Action::LineStart, &["<Home>"]),
    (Action::LineEnd, &["<End>"]),
    (Action::DeleteWordBack, &["<C-w>"]),
    (Action::DeleteToStart, &["<C-u>"]),
];

const PANEL: &[Entry] = &[
    (Action::CursorDown, &["j", "<Down>"]),
    (Action::CursorUp, &["k", "<Up>"]),
    (Action::GoTop, &["gg"]),
    (Action::GoBottom, &["G"]),
    (Action::ToggleEntry, &["<Space>"]),
    (Action::SelectAndJump, &["<CR>"]),
    (Action::Fold, &["h"]),
    (Action::Unfold, &["l"]),
    (Action::FocusLeft, &["<C-w>h"]),
    (Action::FocusDown, &["<C-w>j"]),
    (Action::FocusUp, &["<C-w>k"]),
    (Action::FocusRight, &["<C-w>l"]),
    (Action::FocusNext, &["<Tab>"]),
    (Action::FocusPrev, &["<S-Tab>"]),
    (Action::ToggleSidebar, &["gs"]),
    (Action::ToggleDetail, &["gi"]),
    (Action::Search, &["/"]),
    (Action::CommandLine, &[":"]),
    (Action::Help, &["?"]),
    (Action::Quit, &["q", "ZZ"]),
];

const HELP: &[Entry] = &[
    (Action::CursorDown, &["j", "<Down>"]),
    (Action::CursorUp, &["k", "<Up>"]),
    (Action::HalfPageDown, &["<C-d>"]),
    (Action::HalfPageUp, &["<C-u>"]),
    (Action::Search, &["/"]),
    (Action::Close, &["<Esc>", "q", "?"]),
];

const PICKER: &[Entry] = &[
    (Action::CursorDown, &["j", "<Down>"]),
    (Action::CursorUp, &["k", "<Up>"]),
    (Action::Accept, &["<CR>"]),
    (Action::Close, &["<Esc>", "q"]),
];
