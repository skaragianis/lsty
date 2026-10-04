//! Modes, actions and what a count means to each.

use std::fmt;

use super::defaults::defaults;

/// A key context: a screen mode or a popup. Each has a table in
/// `keys.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Mode {
    /// The list.
    Normal,
    /// A selection in the list.
    Visual,
    /// Editing text in place.
    Insert,
    /// The completion popup over `Insert`.
    Completion,
    /// The command line.
    Command,
    /// A panel of the sidebar.
    Panel,
    /// The key help.
    Help,
    /// A picker popup.
    Picker,
}

impl Mode {
    /// Every mode, in the order of the default tables.
    pub const ALL: [Self; 8] = [
        Self::Normal,
        Self::Visual,
        Self::Insert,
        Self::Completion,
        Self::Command,
        Self::Panel,
        Self::Help,
        Self::Picker,
    ];

    /// The position in [`Mode::ALL`].
    pub(super) const fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Visual => 1,
            Self::Insert => 2,
            Self::Completion => 3,
            Self::Command => 4,
            Self::Panel => 5,
            Self::Help => 6,
            Self::Picker => 7,
        }
    }

    /// The table name in `keys.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Visual => "visual",
            Self::Insert => "insert",
            Self::Completion => "completion",
            Self::Command => "command",
            Self::Panel => "panel",
            Self::Help => "help",
            Self::Picker => "picker",
        }
    }

    /// The mode whose table is called `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == name)
    }

    /// Whether digits typed first are a count.
    pub const fn has_counts(self) -> bool {
        match self {
            Self::Normal | Self::Visual | Self::Panel => true,
            Self::Insert
            | Self::Completion
            | Self::Command
            | Self::Help
            | Self::Picker => false,
        }
    }

    /// Whether a binding may start with the digits 1 to 9 here.
    pub(super) const fn reserves_digits(self) -> bool {
        match self {
            Self::Normal | Self::Visual | Self::Panel | Self::Picker => true,
            Self::Insert | Self::Completion | Self::Command | Self::Help => {
                false
            }
        }
    }
}

/// What a count typed before an action means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Nothing: the count is dropped.
    Other,
    /// A distance.
    Motion,
    /// A line number.
    Jump,
    /// A distance to move an item.
    Move,
    /// That many visible items from the cursor down.
    ItemAction,
    /// Whatever the action says for itself.
    Own,
}

/// The group an action is listed under in the help.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Moving the cursor.
    Navigation,
    /// Editing text.
    Editing,
    /// Acting on items.
    Items,
    /// Undo and redo.
    History,
    /// What the list shows.
    View,
    /// Panes.
    Windows,
    /// Everything else.
    General,
}

impl Category {
    /// The heading of the category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Editing => "Editing",
            Self::Items => "Items",
            Self::History => "History",
            Self::View => "View",
            Self::Windows => "Windows",
            Self::General => "General",
        }
    }
}

/// Declares [`Action`] and what the help says about each one: its name in
/// `keys.toml`, its group, a sentence, the short hint label and what a
/// count means to it. The order is the order of the default tables.
macro_rules! actions {
    ($($variant:ident => (
        $name:literal, $category:ident, $description:literal,
        $hint:literal, $kind:ident
    );)*) => {
        /// Something a key can do.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Action {
            $(
                #[doc = $description]
                $variant,
            )*
        }

        impl Action {
            /// Every action, in the order of the default tables.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            /// The name used in `keys.toml`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// The group in the help.
            pub const fn category(self) -> Category {
                match self {
                    $(Self::$variant => Category::$category,)*
                }
            }

            /// A sentence for the help.
            pub const fn description(self) -> &'static str {
                match self {
                    $(Self::$variant => $description,)*
                }
            }

            /// The short label of the hint row.
            pub const fn hint(self) -> &'static str {
                match self {
                    $(Self::$variant => $hint,)*
                }
            }

            /// What a count typed before the action means.
            pub const fn kind(self) -> Kind {
                match self {
                    $(Self::$variant => Kind::$kind,)*
                }
            }
        }
    };
}

actions! {
    CursorDown => ("cursor_down", Navigation, "Move the cursor down", "down", Motion);
    CursorUp => ("cursor_up", Navigation, "Move the cursor up", "up", Motion);
    GoTop => ("go_top", Navigation, "Go to the first item", "top", Jump);
    GoBottom => ("go_bottom", Navigation, "Go to the last item", "bottom", Jump);
    HalfPageDown => ("half_page_down", Navigation, "Scroll half a page down", "page down", Motion);
    HalfPageUp => ("half_page_up", Navigation, "Scroll half a page up", "page up", Motion);
    EditDescription => ("edit_description", Editing, "Edit at the description", "edit", Other);
    EditAppend => ("edit_append", Editing, "Edit at the end of the line", "append", Other);
    EditLineStart => ("edit_line_start", Editing, "Edit at the priority", "line start", Other);
    ChangeDescription => ("change_description", Editing, "Replace the description", "change", Other);
    InsertBelow => ("insert_below", Editing, "Add an item below", "new", Other);
    InsertAbove => ("insert_above", Editing, "Add an item above", "new above", Other);
    CycleState => ("cycle_state", Items, "Cycle todo, doing and done", "state", ItemAction);
    Stop => ("stop", Items, "Stop doing an item", "stop", ItemAction);
    ToggleComplete => ("toggle_complete", Items, "Complete or reopen an item", "done", ItemAction);
    Delete => ("delete", Items, "Delete an item", "delete", ItemAction);
    Yank => ("yank", Items, "Copy an item", "yank", ItemAction);
    PasteBelow => ("paste_below", Items, "Paste below the cursor", "paste", Other);
    PasteAbove => ("paste_above", Items, "Paste above the cursor", "paste above", Other);
    MoveItemDown => ("move_item_down", Items, "Move the item down", "move down", Move);
    MoveItemUp => ("move_item_up", Items, "Move the item up", "move up", Move);
    CyclePriority => ("cycle_priority", Items, "Raise or lower the priority", "priority", ItemAction);
    CyclePriorityBack => ("cycle_priority_back", Items, "Cycle the priority back", "pri back", ItemAction);
    SetDue => ("set_due", Items, "Set the due date", "due", ItemAction);
    AddTag => ("add_tag", Items, "Add a tag", "tag", ItemAction);
    RemoveTag => ("remove_tag", Items, "Remove a tag", "untag", ItemAction);
    OpenLink => ("open_link", Items, "Open a link of the item", "link", Own);
    Undo => ("undo", History, "Undo the last change", "undo", Other);
    Redo => ("redo", History, "Redo the undone change", "redo", Other);
    Repeat => ("repeat", History, "Repeat the last change", "repeat", Own);
    Search => ("search", View, "Search the list", "search", Other);
    CycleSort => ("cycle_sort", View, "Cycle the sort order", "sort", Other);
    ToggleSidebar => ("toggle_sidebar", View, "Show or hide the sidebar", "sidebar", Other);
    ToggleDetail => ("toggle_detail", View, "Show or hide the detail pane", "detail", Other);
    EnterVisual => ("enter_visual", View, "Select items", "select", Other);
    ExitVisual => ("exit_visual", View, "Leave the selection", "exit", Other);
    FocusLeft => ("focus_left", Windows, "Focus the pane to the left", "left", Other);
    FocusDown => ("focus_down", Windows, "Focus the pane below", "below", Other);
    FocusUp => ("focus_up", Windows, "Focus the pane above", "above", Other);
    FocusRight => ("focus_right", Windows, "Focus the pane to the right", "right", Other);
    FocusNext => ("focus_next", Windows, "Focus the next pane", "next", Other);
    FocusPrev => ("focus_prev", Windows, "Focus the previous pane", "prev", Other);
    CommandLine => ("command_line", General, "Open the command line", "command", Other);
    Help => ("help", General, "Show the key bindings", "help", Other);
    Quit => ("quit", General, "Quit lsty", "quit", Other);
    Commit => ("commit", Editing, "Finish editing and keep the text", "commit", Other);
    Cancel => ("cancel", Editing, "Discard what was typed", "cancel", Other);
    CursorLeft => ("cursor_left", Editing, "Move the cursor left", "left", Other);
    CursorRight => ("cursor_right", Editing, "Move the cursor right", "right", Other);
    CursorRowUp => ("cursor_row_up", Editing, "Move up a wrapped row", "row up", Other);
    CursorRowDown => ("cursor_row_down", Editing, "Move down a wrapped row", "row down", Other);
    LineStart => ("line_start", Editing, "Go to the start of the line", "start", Other);
    LineEnd => ("line_end", Editing, "Go to the end of the line", "end", Other);
    DeleteWordBack => ("delete_word_back", Editing, "Delete the word before", "del word", Other);
    DeleteToStart => ("delete_to_start", Editing, "Delete to the start", "del line", Other);
    Accept => ("accept", General, "Accept the choice", "accept", Other);
    Close => ("close", General, "Close the popup", "close", Other);
    Next => ("next", Navigation, "Select the next candidate", "next", Other);
    Prev => ("prev", Navigation, "Select the previous candidate", "prev", Other);
    Execute => ("execute", General, "Run the line", "run", Other);
    CompleteNext => ("complete_next", Editing, "Complete with the next candidate", "next", Other);
    CompletePrev => ("complete_prev", Editing, "Complete with the previous one", "prev", Other);
    HistoryPrev => ("history_prev", Editing, "Recall the previous line", "history", Other);
    HistoryNext => ("history_next", Editing, "Recall the next line", "history", Other);
    ToggleEntry => ("toggle_entry", Navigation, "Toggle the entry in the filter", "toggle", Other);
    SelectAndJump => ("select_and_jump", Navigation, "Select the entry and go to the list", "go", Other);
    Fold => ("fold", Navigation, "Collapse the tree node", "fold", Other);
    Unfold => ("unfold", Navigation, "Expand the tree node", "unfold", Other);
}

impl Action {
    /// The action called `name` in `keys.toml`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.name() == name)
    }

    /// The modes that have this action.
    pub fn modes(self) -> Vec<Mode> {
        Mode::ALL
            .into_iter()
            .filter(|&m| has_action(m, self))
            .collect()
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The action called `name` if `mode` has it.
pub(super) fn action_in(mode: Mode, name: &str) -> Option<Action> {
    defaults(mode)
        .iter()
        .map(|&(action, _)| action)
        .find(|action| action.name() == name)
}

pub(super) fn has_action(mode: Mode, action: Action) -> bool {
    defaults(mode).iter().any(|&(a, _)| a == action)
}
