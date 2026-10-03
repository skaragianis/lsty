//! Modes, actions and what a count means to each.

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

/// Something a key can do, with the text that describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    /// The name used in `keys.toml`.
    pub name: &'static str,
    /// The group in the help.
    pub category: Category,
    /// A sentence for the help.
    pub description: &'static str,
    /// The short label of the hint row.
    pub hint: &'static str,
    /// The modes that have this action.
    pub modes: Vec<Mode>,
    /// What a count means to it.
    pub kind: Kind,
}

/// Every action, in the order of the default tables.
pub fn registry() -> Vec<Action> {
    DESCRIPTIONS
        .iter()
        .map(|&(name, category, description, hint)| Action {
            name,
            category,
            description,
            hint,
            modes: modes_of(name),
            kind: kind_of(name),
        })
        .collect()
}

fn modes_of(action: &str) -> Vec<Mode> {
    Mode::ALL
        .into_iter()
        .filter(|&m| has_action(m, action))
        .collect()
}

/// The registered name of `action` if `mode` has it.
pub(super) fn action_in(mode: Mode, action: &str) -> Option<&'static str> {
    defaults(mode)
        .iter()
        .map(|&(name, _)| name)
        .find(|&name| name == action)
}

pub(super) fn has_action(mode: Mode, action: &str) -> bool {
    action_in(mode, action).is_some()
}

/// What a count means to `action`.
pub fn kind_of(action: &str) -> Kind {
    match action {
        "cursor_down" | "cursor_up" | "half_page_down" | "half_page_up" => {
            Kind::Motion
        }
        "go_top" | "go_bottom" => Kind::Jump,
        "move_item_down" | "move_item_up" => Kind::Move,
        "toggle_complete"
        | "cycle_state"
        | "stop"
        | "delete"
        | "yank"
        | "cycle_priority"
        | "cycle_priority_back"
        | "set_due"
        | "add_tag"
        | "remove_tag" => Kind::ItemAction,
        "repeat" | "open_link" => Kind::Own,
        _ => Kind::Other,
    }
}

/// An action's name, category, description and hint.
type Description = (&'static str, Category, &'static str, &'static str);

/// Every action, in the order of the default tables.
const DESCRIPTIONS: &[Description] = &[
    (
        "cursor_down",
        Category::Navigation,
        "Move the cursor down",
        "down",
    ),
    (
        "cursor_up",
        Category::Navigation,
        "Move the cursor up",
        "up",
    ),
    (
        "go_top",
        Category::Navigation,
        "Go to the first item",
        "top",
    ),
    (
        "go_bottom",
        Category::Navigation,
        "Go to the last item",
        "bottom",
    ),
    (
        "half_page_down",
        Category::Navigation,
        "Scroll half a page down",
        "page down",
    ),
    (
        "half_page_up",
        Category::Navigation,
        "Scroll half a page up",
        "page up",
    ),
    (
        "edit_description",
        Category::Editing,
        "Edit at the description",
        "edit",
    ),
    (
        "edit_append",
        Category::Editing,
        "Edit at the end of the line",
        "append",
    ),
    (
        "edit_line_start",
        Category::Editing,
        "Edit at the priority",
        "line start",
    ),
    (
        "change_description",
        Category::Editing,
        "Replace the description",
        "change",
    ),
    (
        "insert_below",
        Category::Editing,
        "Add an item below",
        "new",
    ),
    (
        "insert_above",
        Category::Editing,
        "Add an item above",
        "new above",
    ),
    (
        "cycle_state",
        Category::Items,
        "Cycle todo, doing and done",
        "state",
    ),
    ("stop", Category::Items, "Stop doing an item", "stop"),
    (
        "toggle_complete",
        Category::Items,
        "Complete or reopen an item",
        "done",
    ),
    ("delete", Category::Items, "Delete an item", "delete"),
    ("yank", Category::Items, "Copy an item", "yank"),
    (
        "paste_below",
        Category::Items,
        "Paste below the cursor",
        "paste",
    ),
    (
        "paste_above",
        Category::Items,
        "Paste above the cursor",
        "paste above",
    ),
    (
        "move_item_down",
        Category::Items,
        "Move the item down",
        "move down",
    ),
    (
        "move_item_up",
        Category::Items,
        "Move the item up",
        "move up",
    ),
    (
        "cycle_priority",
        Category::Items,
        "Raise or lower the priority",
        "priority",
    ),
    (
        "cycle_priority_back",
        Category::Items,
        "Cycle the priority back",
        "pri back",
    ),
    ("set_due", Category::Items, "Set the due date", "due"),
    ("add_tag", Category::Items, "Add a tag", "tag"),
    ("remove_tag", Category::Items, "Remove a tag", "untag"),
    (
        "open_link",
        Category::Items,
        "Open a link of the item",
        "link",
    ),
    ("undo", Category::History, "Undo the last change", "undo"),
    ("redo", Category::History, "Redo the undone change", "redo"),
    (
        "repeat",
        Category::History,
        "Repeat the last change",
        "repeat",
    ),
    ("search", Category::View, "Search the list", "search"),
    ("cycle_sort", Category::View, "Cycle the sort order", "sort"),
    (
        "toggle_sidebar",
        Category::View,
        "Show or hide the sidebar",
        "sidebar",
    ),
    (
        "toggle_detail",
        Category::View,
        "Show or hide the detail pane",
        "detail",
    ),
    ("enter_visual", Category::View, "Select items", "select"),
    ("exit_visual", Category::View, "Leave the selection", "exit"),
    (
        "focus_left",
        Category::Windows,
        "Focus the pane to the left",
        "left",
    ),
    (
        "focus_down",
        Category::Windows,
        "Focus the pane below",
        "below",
    ),
    (
        "focus_up",
        Category::Windows,
        "Focus the pane above",
        "above",
    ),
    (
        "focus_right",
        Category::Windows,
        "Focus the pane to the right",
        "right",
    ),
    (
        "focus_next",
        Category::Windows,
        "Focus the next pane",
        "next",
    ),
    (
        "focus_prev",
        Category::Windows,
        "Focus the previous pane",
        "prev",
    ),
    (
        "command_line",
        Category::General,
        "Open the command line",
        "command",
    ),
    ("help", Category::General, "Show the key bindings", "help"),
    ("quit", Category::General, "Quit lsty", "quit"),
    (
        "commit",
        Category::Editing,
        "Finish editing and keep the text",
        "commit",
    ),
    (
        "cancel",
        Category::Editing,
        "Discard what was typed",
        "cancel",
    ),
    (
        "cursor_left",
        Category::Editing,
        "Move the cursor left",
        "left",
    ),
    (
        "cursor_right",
        Category::Editing,
        "Move the cursor right",
        "right",
    ),
    (
        "cursor_row_up",
        Category::Editing,
        "Move up a wrapped row",
        "row up",
    ),
    (
        "cursor_row_down",
        Category::Editing,
        "Move down a wrapped row",
        "row down",
    ),
    (
        "line_start",
        Category::Editing,
        "Go to the start of the line",
        "start",
    ),
    (
        "line_end",
        Category::Editing,
        "Go to the end of the line",
        "end",
    ),
    (
        "delete_word_back",
        Category::Editing,
        "Delete the word before",
        "del word",
    ),
    (
        "delete_to_start",
        Category::Editing,
        "Delete to the start",
        "del line",
    ),
    ("accept", Category::General, "Accept the choice", "accept"),
    ("close", Category::General, "Close the popup", "close"),
    (
        "next",
        Category::Navigation,
        "Select the next candidate",
        "next",
    ),
    (
        "prev",
        Category::Navigation,
        "Select the previous candidate",
        "prev",
    ),
    ("execute", Category::General, "Run the line", "run"),
    (
        "complete_next",
        Category::Editing,
        "Complete with the next candidate",
        "next",
    ),
    (
        "complete_prev",
        Category::Editing,
        "Complete with the previous one",
        "prev",
    ),
    (
        "history_prev",
        Category::Editing,
        "Recall the previous line",
        "history",
    ),
    (
        "history_next",
        Category::Editing,
        "Recall the next line",
        "history",
    ),
    (
        "toggle_entry",
        Category::Navigation,
        "Toggle the entry in the filter",
        "toggle",
    ),
    (
        "select_and_jump",
        Category::Navigation,
        "Select the entry and go to the list",
        "go",
    ),
    (
        "fold",
        Category::Navigation,
        "Collapse the tree node",
        "fold",
    ),
    (
        "unfold",
        Category::Navigation,
        "Expand the tree node",
        "unfold",
    ),
];
