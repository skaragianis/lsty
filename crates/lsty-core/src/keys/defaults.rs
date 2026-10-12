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
            (Action::GoTop, &["gg", "<Home>"]),
            (Action::GoBottom, &["G", "<End>"]),
            (Action::HalfPageDown, &["<C-d>"]),
            (Action::HalfPageUp, &["<C-u>"]),
            (Action::PageDown, &["<C-f>", "<PageDown>"]),
            (Action::PageUp, &["<C-b>", "<PageUp>"]),
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
    (Action::EditDescription, &["i", "<F2>"]),
    (Action::EditAppend, &["a", "A", "<CR>"]),
    (Action::EditLineStart, &["I"]),
    (Action::ChangeDescription, &["cc", "S"]),
    (Action::InsertBelow, &["o"]),
    (Action::InsertAbove, &["O"]),
    (Action::CycleState, &["t"]),
    (Action::Stop, &["T"]),
    (Action::ToggleComplete, &["x"]),
    (Action::Delete, &["dd", "<Del>"]),
    (Action::Yank, &["yy"]),
    (Action::PasteBelow, &["p"]),
    (Action::PasteAbove, &["P"]),
    (Action::MoveItemDown, &["J", "<A-Down>"]),
    (Action::MoveItemUp, &["K", "<A-Up>"]),
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
    (Action::ClearSearch, &["<C-l>", "<Esc>", "<C-c>"]),
    (Action::CycleSort, &["s"]),
    (Action::ToggleSidebar, &["gs"]),
    (Action::ToggleDetail, &["gi"]),
    (Action::EnterVisual, &["v", "V"]),
    (Action::FocusLeft, &["<C-w>h", "<C-w><Left>"]),
    (Action::FocusDown, &["<C-w>j", "<C-w><Down>"]),
    (Action::FocusUp, &["<C-w>k", "<C-w><Up>"]),
    (Action::FocusRight, &["<C-w>l", "<C-w><Right>"]),
    (Action::FocusNext, &["<Tab>"]),
    (Action::FocusPrev, &["<S-Tab>"]),
    (Action::FocusList, &["<C-w>1"]),
    (Action::FocusDetail, &["<C-w>2"]),
    (Action::FocusProjects, &["<C-w>3"]),
    (Action::FocusContexts, &["<C-w>4"]),
    (Action::FocusFilters, &["<C-w>5"]),
    (Action::CommandLine, &[":"]),
    (Action::Help, &["?", "<F1>"]),
    (Action::Quit, &["q", "ZZ", "ZQ", "<C-q>"]),
];

const VISUAL: &[Entry] = moving![
    (Action::CycleState, &["t"]),
    (Action::Stop, &["T"]),
    (Action::ToggleComplete, &["x"]),
    (Action::Delete, &["d", "<Del>"]),
    (Action::Yank, &["y"]),
    (Action::MoveItemDown, &["J", "<A-Down>"]),
    (Action::MoveItemUp, &["K", "<A-Up>"]),
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
    (Action::LineStart, &["<Home>", "<C-a>"]),
    (Action::LineEnd, &["<End>", "<C-e>"]),
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
    (Action::HistoryPrev, &["<Up>", "<C-p>"]),
    (Action::HistoryNext, &["<Down>", "<C-n>"]),
    (Action::CursorLeft, &["<Left>"]),
    (Action::CursorRight, &["<Right>"]),
    (Action::LineStart, &["<Home>", "<C-a>"]),
    (Action::LineEnd, &["<End>", "<C-e>"]),
    (Action::DeleteWordBack, &["<C-w>"]),
    (Action::DeleteToStart, &["<C-u>"]),
];

const PANEL: &[Entry] = moving![
    (Action::ToggleEntry, &["x"]),
    (Action::SelectAndJump, &["<CR>"]),
    (Action::Fold, &["h", "<Left>"]),
    (Action::Unfold, &["l", "<Right>"]),
    (Action::FocusLeft, &["<C-w>h", "<C-w><Left>"]),
    (Action::FocusDown, &["<C-w>j", "<C-w><Down>"]),
    (Action::FocusUp, &["<C-w>k", "<C-w><Up>"]),
    (Action::FocusRight, &["<C-w>l", "<C-w><Right>"]),
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
    (Action::Help, &["?", "<F1>"]),
    (Action::Quit, &["q", "ZZ", "ZQ", "<C-q>"]),
];

/// What the panel adds to the normal table or takes from it: the keys of
/// every other action of normal apply there too, but not those that begin
/// or continue one of these keys.
const DETAIL: &[Entry] = &[
    (Action::NextLink, &["<Tab>"]),
    (Action::PrevLink, &["<S-Tab>"]),
    (Action::OpenLink, &["<CR>", "<Space>", "gx"]),
    (Action::FocusList, &["<C-w>1", "<Esc>", "<C-c>"]),
    (Action::CycleSort, &[]),
    (Action::EnterVisual, &[]),
    (Action::FocusRight, &[]),
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
