//! The default keys of every mode.

use super::{Action, Mode};

/// An action and the keys it is bound to, as written.
pub type Entry = (Action, &'static [&'static str]);

/// The default entries of `mode`, in the order they are listed.
pub const fn defaults(mode: Mode) -> &'static [Entry] {
    match mode {
        Mode::Normal => NORMAL,
        Mode::Visual => VISUAL,
        Mode::Insert => INSERT,
        Mode::Completion => COMPLETION,
        Mode::Command => COMMAND,
        Mode::Panel => PANEL,
        Mode::Detail => DETAIL,
        Mode::Help => HELP,
        Mode::Picker => PICKER,
    }
}

/// A table that starts with the motions, which every mode that moves
/// binds the same: a motion added here is bound in all of them.
macro_rules! moving {
    ($($entry:expr),* $(,)?) => {
        &[
            (Action::CursorDown, &["j", "<Down>"]),
            (Action::CursorUp, &["k", "<Up>"]),
            (Action::GoTop, &["gg"]),
            (Action::GoBottom, &["G"]),
            (Action::HalfPageDown, &["<C-d>"]),
            (Action::HalfPageUp, &["<C-u>"]),
            (Action::PageDown, &["<C-f>"]),
            (Action::PageUp, &["<C-b>"]),
            (Action::ScrollDown, &["<C-e>"]),
            (Action::ScrollUp, &["<C-y>"]),
            (Action::CentreView, &["zz"]),
            (Action::TopView, &["zt"]),
            (Action::BottomView, &["zb"]),
            (Action::GoViewTop, &["H"]),
            (Action::GoViewMiddle, &["M"]),
            (Action::GoViewBottom, &["L"]),
            $($entry,)*
        ]
    };
}

const NORMAL: &[Entry] = moving![
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
    (Action::ClearSearch, &["<C-l>"]),
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
    (Action::FocusList, &["<C-w>1"]),
    (Action::FocusDetail, &["<C-w>2"]),
    (Action::FocusProjects, &["<C-w>3"]),
    (Action::FocusContexts, &["<C-w>4"]),
    (Action::FocusFilters, &["<C-w>5"]),
    (Action::CommandLine, &[":"]),
    (Action::Help, &["?"]),
    (Action::Quit, &["q", "ZZ", "ZQ"]),
];

const VISUAL: &[Entry] = moving![
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
    (Action::ExitVisual, &["<Esc>", "<C-c>", "v", "V"]),
];

const INSERT: &[Entry] = &[
    (Action::Commit, &["<CR>", "<Esc>", "<C-c>"]),
    (Action::Cancel, &[]),
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
    (Action::Close, &["<Esc>", "<C-c>"]),
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

const PANEL: &[Entry] = moving![
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
    (Action::FocusList, &["<C-w>1", "<Esc>", "<C-c>"]),
    (Action::FocusDetail, &["<C-w>2"]),
    (Action::FocusProjects, &["<C-w>3"]),
    (Action::FocusContexts, &["<C-w>4"]),
    (Action::FocusFilters, &["<C-w>5"]),
    (Action::ToggleSidebar, &["gs"]),
    (Action::ToggleDetail, &["gi"]),
    (Action::Search, &["/"]),
    (Action::ClearSearch, &["<C-l>"]),
    (Action::CommandLine, &[":"]),
    (Action::Help, &["?"]),
    (Action::Quit, &["q", "ZZ", "ZQ"]),
];

const DETAIL: &[Entry] = moving![
    (Action::NextLink, &["<Tab>"]),
    (Action::PrevLink, &["<S-Tab>"]),
    (Action::OpenLink, &["<CR>", "<Space>", "gx"]),
    (Action::FocusLeft, &["<C-w>h"]),
    (Action::FocusUp, &["<C-w>k"]),
    (Action::FocusList, &["<C-w>1", "<Esc>", "<C-c>"]),
    (Action::FocusDetail, &["<C-w>2"]),
    (Action::FocusProjects, &["<C-w>3"]),
    (Action::FocusContexts, &["<C-w>4"]),
    (Action::FocusFilters, &["<C-w>5"]),
    (Action::ToggleSidebar, &["gs"]),
    (Action::ToggleDetail, &["gi"]),
    (Action::Search, &["/"]),
    (Action::ClearSearch, &["<C-l>"]),
    (Action::CommandLine, &[":"]),
    (Action::Help, &["?"]),
    (Action::Quit, &["q", "ZZ", "ZQ"]),
];

const HELP: &[Entry] = moving![
    (Action::Search, &["/"]),
    (Action::Close, &["<Esc>", "<C-c>", "q", "?"]),
];

const PICKER: &[Entry] = moving![
    (Action::Accept, &["<CR>"]),
    (Action::Search, &["/"]),
    (Action::Close, &["<Esc>", "<C-c>", "q"]),
];
