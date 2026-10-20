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
    /// The detail panel.
    Detail,
    /// The key help.
    Help,
    /// A picker popup.
    Picker,
}

impl Mode {
    /// Every mode, in the order of the default tables.
    pub const ALL: [Self; 9] = [
        Self::Normal,
        Self::Visual,
        Self::Insert,
        Self::Completion,
        Self::Command,
        Self::Panel,
        Self::Detail,
        Self::Help,
        Self::Picker,
    ];

    /// The position in [`Mode::ALL`].
    pub(super) const fn index(self) -> usize {
        self as usize
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
            Self::Detail => "detail",
            Self::Help => "help",
            Self::Picker => "picker",
        }
    }

    /// The mode whose table is called `name`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == name)
    }

    /// Whether digits typed first are a count.
    pub(crate) const fn has_counts(self) -> bool {
        match self {
            Self::Normal | Self::Visual | Self::Panel | Self::Detail => true,
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
            Self::Normal
            | Self::Visual
            | Self::Panel
            | Self::Detail
            | Self::Picker => true,
            Self::Insert | Self::Completion | Self::Command | Self::Help => {
                false
            }
        }
    }
}

/// What a count typed before an action means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Nothing: the count is dropped.
    Other,
    /// A distance.
    Motion,
    /// A line number.
    Jump,
    /// A distance to move a task.
    Move,
    /// That many visible tasks from the cursor down.
    TaskAction,
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
    /// Acting on tasks.
    Tasks,
    /// Undo and redo.
    History,
    /// What the list shows.
    View,
    /// Boxes.
    Windows,
    /// Everything else.
    General,
}

impl Category {
    /// Every category, in the order of the help.
    pub const ALL: [Self; 7] = [
        Self::Navigation,
        Self::Editing,
        Self::Tasks,
        Self::History,
        Self::View,
        Self::Windows,
        Self::General,
    ];

    /// The heading of the category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::Editing => "Editing",
            Self::Tasks => "Tasks",
            Self::History => "History",
            Self::View => "View",
            Self::Windows => "Boxes",
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
            pub(crate) const fn name(self) -> &'static str {
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

            /// Whether the action moves the cursor or the view, as the
            /// motions in every mode that moves do.
            pub const fn is_motion(self) -> bool {
                matches!(self.kind(), Kind::Motion | Kind::Jump)
            }

            /// What a count typed before the action means.
            pub(crate) const fn kind(self) -> Kind {
                match self {
                    $(Self::$variant => Kind::$kind,)*
                }
            }
        }
    };
}

actions! {
    CursorDown => (
        "cursor_down", Navigation, "Move the cursor down", "down", Motion
    );
    CursorUp => ("cursor_up", Navigation, "Move the cursor up", "up", Motion);
    GoTop => ("go_top", Navigation, "Go to the top", "top", Jump);
    GoBottom => (
        "go_bottom", Navigation, "Go to the bottom", "bottom", Jump
    );
    HalfPageDown => (
        "half_page_down", Navigation,
        "Scroll half a page down",
        "half page down", Motion
    );
    HalfPageUp => (
        "half_page_up", Navigation, "Scroll half a page up", "half page up",
        Motion
    );
    PageDown => (
        "page_down", Navigation, "Scroll a page down", "page down", Motion
    );
    PageUp => (
        "page_up", Navigation, "Scroll a page up", "page up", Motion
    );
    ScrollDown => (
        "scroll_down", Navigation, "Scroll the view down a row", "scroll down",
        Motion
    );
    ScrollUp => (
        "scroll_up", Navigation, "Scroll the view up a row", "scroll up",
        Motion
    );
    CentreView => (
        "centre_view", Navigation, "Centre the cursor row in the view",
        "centre", Motion
    );
    TopView => (
        "top_view", Navigation, "Put the cursor row at the top of the view",
        "to top", Motion
    );
    BottomView => (
        "bottom_view", Navigation,
        "Put the cursor row at the bottom of the view", "to bottom", Motion
    );
    GoViewTop => (
        "go_view_top", Navigation, "Go to the top row of the view", "view top",
        Jump
    );
    GoViewMiddle => (
        "go_view_middle", Navigation, "Go to the middle row of the view",
        "view middle", Jump
    );
    GoViewBottom => (
        "go_view_bottom", Navigation, "Go to the bottom row of the view",
        "view bottom", Jump
    );
    EditDescription => (
        "edit_description", Editing, "Edit at the description", "edit", Other
    );
    EditAppend => (
        "edit_append", Editing, "Edit at the end of the line, before an id that ends it", "append", Other
    );
    EditLineStart => (
        "edit_line_start", Editing, "Edit at the priority", "line start", Other
    );
    ChangeDescription => (
        "change_description", Editing,
        "Replace the description",
        "change", Other
    );
    InsertBelow => (
        "insert_below", Editing, "Add a task below", "new", Other
    );
    InsertAbove => (
        "insert_above", Editing, "Add a task above", "new above", Other
    );
    InsertDependent => (
        "insert_dependent", Editing,
        "Add a task below that waits for this one",
        "new waiting", Other
    );
    InsertPrerequisite => (
        "insert_prerequisite", Editing,
        "Add a task above that this one waits for",
        "new blocker", Other
    );
    CycleState => (
        "cycle_state", Tasks, "Cycle todo, doing and done", "state", TaskAction
    );
    Stop => ("stop", Tasks, "Stop doing a task", "stop", TaskAction);
    ToggleComplete => (
        "toggle_complete", Tasks,
        "Complete or reopen a task",
        "done", TaskAction
    );
    Delete => ("delete", Tasks, "Delete a task", "delete", TaskAction);
    Yank => ("yank", Tasks, "Copy a task", "yank", TaskAction);
    PasteBelow => (
        "paste_below", Tasks, "Paste below the cursor", "paste", Other
    );
    PasteAbove => (
        "paste_above", Tasks, "Paste above the cursor", "paste above", Other
    );
    MoveTaskDown => (
        "move_task_down", Tasks, "Move the task down", "move down", Move
    );
    MoveTaskUp => ("move_task_up", Tasks, "Move the task up", "move up", Move);
    CyclePriority => (
        "cycle_priority", Tasks,
        "Raise or lower the priority",
        "priority", TaskAction
    );
    CyclePriorityBack => (
        "cycle_priority_back", Tasks,
        "Cycle the priority back",
        "pri back", TaskAction
    );
    SetDue => ("set_due", Tasks, "Set the due date", "due", TaskAction);
    AddTag => ("add_tag", Tasks, "Add a tag", "tag", TaskAction);
    RemoveTag => ("remove_tag", Tasks, "Remove a tag", "untag", TaskAction);
    OpenLink => ("open_link", Tasks, "Open a link of the task", "link", Own);
    Undo => ("undo", History, "Undo the last change", "undo", Other);
    Redo => ("redo", History, "Redo the undone change", "redo", Other);
    Repeat => ("repeat", History, "Repeat the last change", "repeat", Own);
    Search => ("search", View, "Search the list", "search", Other);
    ClearSearch => (
        "clear_search", View, "Clear the search", "clear search", Other
    );
    ClearFilters => (
        "clear_filters", View, "Clear the filters", "clear filters", Other
    );
    CycleSort => ("cycle_sort", View, "Cycle the sort order", "sort", Other);
    ToggleSidebar => (
        "toggle_sidebar", View, "Show or hide the sidebar", "sidebar", Other
    );
    ToggleDetail => (
        "toggle_detail", View, "Show or hide the detail panel", "detail", Other
    );
    ToggleWrap => (
        "toggle_wrap", View, "Wrap or cut long tasks in the list", "wrap", Other
    );
    EnterVisual => ("enter_visual", View, "Select tasks", "select", Other);
    ExitVisual => ("exit_visual", View, "Leave the selection", "exit", Other);
    FocusLeft => (
        "focus_left", Windows, "Focus the box to the left", "left", Other
    );
    FocusDown => (
        "focus_down", Windows, "Focus the box below", "below", Other
    );
    FocusUp => ("focus_up", Windows, "Focus the box above", "above", Other);
    FocusRight => (
        "focus_right", Windows, "Focus the box to the right", "right", Other
    );
    FocusNext => ("focus_next", Windows, "Focus the next box", "next", Other);
    FocusPrev => (
        "focus_prev", Windows, "Focus the previous box", "prev", Other
    );
    FocusList => ("focus_list", Windows, "Focus the list", "list", Other);
    FocusDetail => (
        "focus_detail", Windows, "Focus the Detail panel", "detail", Other
    );
    FocusProjects => (
        "focus_projects", Windows, "Focus the Projects panel", "projects",
        Other
    );
    FocusContexts => (
        "focus_contexts", Windows, "Focus the Contexts panel", "contexts",
        Other
    );
    FocusFilters => (
        "focus_filters", Windows, "Focus the Filters panel", "filters", Other
    );
    CommandLine => (
        "command_line", General, "Open the command line", "command", Other
    );
    Help => ("help", General, "Show the key bindings", "help", Other);
    Quit => ("quit", General, "Quit lsty", "quit", Other);
    Commit => (
        "commit", Editing, "Finish editing and keep the text", "commit", Other
    );
    Cancel => ("cancel", Editing, "Discard what was typed", "cancel", Other);
    CursorRowUp => (
        "cursor_row_up", Editing, "Move up a wrapped row", "row up", Other
    );
    CursorRowDown => (
        "cursor_row_down", Editing,
        "Move down a wrapped row",
        "row down", Other
    );
    CursorLeft => (
        "cursor_left", Editing, "Move the cursor left", "left", Other
    );
    CursorRight => (
        "cursor_right", Editing, "Move the cursor right", "right", Other
    );
    WordBack => (
        "word_back", Editing, "Move back one word", "word back", Other
    );
    WordForward => (
        "word_forward", Editing, "Move forward one word", "word forward",
        Other
    );
    LineStart => (
        "line_start", Editing, "Go to the start of the line", "start", Other
    );
    LineEnd => (
        "line_end", Editing, "Go to the end of the line", "end", Other
    );
    DeleteChar => (
        "delete_char", Editing, "Delete the character under the cursor",
        "del char", Other
    );
    DeleteWordBack => (
        "delete_word_back", Editing,
        "Delete the word before",
        "del word", Other
    );
    DeleteWordForward => (
        "delete_word_forward", Editing,
        "Delete the word after",
        "del word after", Other
    );
    DeleteToStart => (
        "delete_to_start", Editing, "Delete to the start",
        "del to start", Other
    );
    DeleteToEnd => (
        "delete_to_end", Editing, "Delete to the end", "del to end", Other
    );
    Accept => ("accept", General, "Accept the choice", "accept", Other);
    Close => ("close", General, "Close the popup", "close", Other);
    NextLink => (
        "next_link", Navigation, "Select the next link, then the next box",
        "next link", Other
    );
    PrevLink => (
        "prev_link", Navigation,
        "Select the previous link, then the previous box", "prev link", Other
    );
    Next => ("next", Navigation, "Select the next candidate", "next", Other);
    Prev => (
        "prev", Navigation, "Select the previous candidate", "prev", Other
    );
    Execute => ("execute", General, "Run the line", "run", Other);
    CompleteNext => (
        "complete_next", Editing,
        "Complete with the next candidate",
        "next", Other
    );
    CompletePrev => (
        "complete_prev", Editing,
        "Complete with the previous one",
        "prev", Other
    );
    HistoryPrev => (
        "history_prev", Editing, "Recall the previous line", "history", Other
    );
    HistoryNext => (
        "history_next", Editing, "Recall the next line", "history", Other
    );
    ToggleEntry => (
        "toggle_entry", Navigation,
        "Select, exclude or deselect the entry; turn a switch on or off",
        "filter", Other
    );
    SelectAndJump => (
        "select_and_jump", Navigation,
        "Go to the list",
        "list", Other
    );
    Fold => ("fold", Navigation, "Collapse the tree node", "fold", Other);
    Unfold => ("unfold", Navigation, "Expand the tree node", "unfold", Other);
}

impl Action {
    /// The action called `name` in `keys.toml`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.name() == name)
    }
}

/// What `action` does in `mode`.
pub fn about(mode: Mode, action: Action) -> &'static str {
    if mode == Mode::Picker && action == Action::Search {
        "Filter the themes (theme picker)"
    } else {
        action.description()
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The action called `name` if `mode` has it: the detail panel has those
/// of the normal table too.
pub(super) fn action_in(mode: Mode, name: &str) -> Option<Action> {
    let normal = defaults(Mode::Normal)
        .iter()
        .filter(|_| mode == Mode::Detail);
    defaults(mode)
        .iter()
        .chain(normal)
        .map(|&(action, _)| action)
        .find(|action| action.name() == name)
}
