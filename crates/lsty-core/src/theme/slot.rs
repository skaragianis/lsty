/// Declares [`Slot`] and what a theme file calls each one. The order is the
/// order of the theme file's documentation.
macro_rules! slots {
    ($($(#[$doc:meta])* $variant:ident => $name:literal;)*) => {
        /// A part of the screen that a theme colours.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Slot {
            $($(#[$doc])* $variant,)*
        }

        impl Slot {
            /// Every slot, in the order of the theme file's documentation.
            pub const ALL: [Self; [$(Self::$variant),*].len()] =
                [$(Self::$variant),*];

            /// The slot's name in a theme file.
            pub(crate) const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }
    };
}

slots! {
    /// Priority `(A)`.
    PriorityA => "priority_a";
    /// Priority `(B)`.
    PriorityB => "priority_b";
    /// Priority `(C)`.
    PriorityC => "priority_c";
    /// Every other priority.
    PriorityOther => "priority_other";
    /// `+project`.
    Project => "project";
    /// `@context`.
    Context => "context";
    /// `file:` tokens and linked files.
    Link => "link";
    /// Plain text.
    Text => "text";
    /// Numbers, footers and other quiet parts.
    Muted => "muted";
    /// A due date before today.
    DueOverdue => "due_overdue";
    /// A due date of today.
    DueToday => "due_today";
    /// A due date that is coming up.
    DueSoon => "due_soon";
    /// A completed item.
    Done => "done";
    /// An item somebody is working on.
    Doing => "doing";
    /// An item that waits for another.
    Blocked => "blocked";
    /// The label of normal mode.
    ModeNormal => "mode_normal";
    /// The label of insert mode.
    ModeInsert => "mode_insert";
    /// The label of visual mode.
    ModeVisual => "mode_visual";
    /// The label of command mode.
    ModeCommand => "mode_command";
    /// The border of a panel.
    PanelBorder => "panel_border";
    /// The border of the panel that has focus.
    PanelBorderActive => "panel_border_active";
    /// The selected row.
    Selection => "selection";
    /// The row under the cursor.
    Cursorline => "cursorline";
    /// A chip such as a filter name.
    Chip => "chip";
    /// The status line.
    StatusBg => "status_bg";
    /// The key of a hint.
    HintKey => "hint_key";
    /// The description of a hint.
    HintDesc => "hint_desc";
    /// A warning.
    Warning => "warning";
    /// An error.
    Error => "error";
}

impl Slot {
    /// The slot a theme file name stands for.
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    /// The slot's position in [`Slot::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }
}
