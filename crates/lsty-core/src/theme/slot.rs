/// A part of the screen that a theme colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// Priority `(A)`.
    PriorityA,
    /// Priority `(B)`.
    PriorityB,
    /// Priority `(C)`.
    PriorityC,
    /// Every other priority.
    PriorityOther,
    /// `+project`.
    Project,
    /// `@context`.
    Context,
    /// `file:` tokens and linked files.
    Link,
    /// Plain text.
    Text,
    /// Numbers, footers and other quiet parts.
    Muted,
    /// A due date before today.
    DueOverdue,
    /// A due date of today.
    DueToday,
    /// A due date that is coming up.
    DueSoon,
    /// A completed item.
    Done,
    /// An item somebody is working on.
    Doing,
    /// An item that waits for another.
    Blocked,
    /// The label of normal mode.
    ModeNormal,
    /// The label of insert mode.
    ModeInsert,
    /// The label of visual mode.
    ModeVisual,
    /// The label of command mode.
    ModeCommand,
    /// The border of a panel.
    PanelBorder,
    /// The border of the panel that has focus.
    PanelBorderActive,
    /// The selected row.
    Selection,
    /// The row under the cursor.
    Cursorline,
    /// A chip such as a filter name.
    Chip,
    /// The status line.
    StatusBg,
    /// The key of a hint.
    HintKey,
    /// The description of a hint.
    HintDesc,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

impl Slot {
    /// Every slot, in the order of the theme file's documentation.
    pub const ALL: [Self; 29] = [
        Self::PriorityA,
        Self::PriorityB,
        Self::PriorityC,
        Self::PriorityOther,
        Self::Project,
        Self::Context,
        Self::Link,
        Self::Text,
        Self::Muted,
        Self::DueOverdue,
        Self::DueToday,
        Self::DueSoon,
        Self::Done,
        Self::Doing,
        Self::Blocked,
        Self::ModeNormal,
        Self::ModeInsert,
        Self::ModeVisual,
        Self::ModeCommand,
        Self::PanelBorder,
        Self::PanelBorderActive,
        Self::Selection,
        Self::Cursorline,
        Self::Chip,
        Self::StatusBg,
        Self::HintKey,
        Self::HintDesc,
        Self::Warning,
        Self::Error,
    ];

    /// The slot's name in a theme file.
    pub const fn name(self) -> &'static str {
        match self {
            Self::PriorityA => "priority_a",
            Self::PriorityB => "priority_b",
            Self::PriorityC => "priority_c",
            Self::PriorityOther => "priority_other",
            Self::Project => "project",
            Self::Context => "context",
            Self::Link => "link",
            Self::Text => "text",
            Self::Muted => "muted",
            Self::DueOverdue => "due_overdue",
            Self::DueToday => "due_today",
            Self::DueSoon => "due_soon",
            Self::Done => "done",
            Self::Doing => "doing",
            Self::Blocked => "blocked",
            Self::ModeNormal => "mode_normal",
            Self::ModeInsert => "mode_insert",
            Self::ModeVisual => "mode_visual",
            Self::ModeCommand => "mode_command",
            Self::PanelBorder => "panel_border",
            Self::PanelBorderActive => "panel_border_active",
            Self::Selection => "selection",
            Self::Cursorline => "cursorline",
            Self::Chip => "chip",
            Self::StatusBg => "status_bg",
            Self::HintKey => "hint_key",
            Self::HintDesc => "hint_desc",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// The slot a theme file name stands for.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    /// The slot's position in [`Slot::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }
}
