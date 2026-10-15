/// Declares [`Slot`], what a theme file calls each one and what it paints
/// (its doc line). The order is the order of the theme file's documentation.
macro_rules! slots {
    ($(#[doc = $doc:literal] $variant:ident => $name:literal;)*) => {
        /// A part of the screen that a theme colours.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Slot {
            $(#[doc = $doc] $variant,)*
        }

        impl Slot {
            /// Every slot, in the order of the theme file's documentation.
            pub const ALL: [Self; [$(Self::$variant),*].len()] =
                [$(Self::$variant),*];

            /// The slot's name in a theme file.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            /// What the slot paints, in a few words.
            pub fn about(self) -> &'static str {
                match self {
                    $(Self::$variant => $doc.trim(),)*
                }
            }
        }
    };
}

slots! {
    /// Priority (A)
    PriorityA => "priority_a";
    /// Priority (B)
    PriorityB => "priority_b";
    /// Priority (C)
    PriorityC => "priority_c";
    /// Any other priority
    PriorityOther => "priority_other";
    /// A +project
    Project => "project";
    /// An @context
    Context => "context";
    /// A file: tag
    Link => "link";
    /// Plain task text
    Text => "text";
    /// Numbers, footers and other quiet parts
    Muted => "muted";
    /// A due: date before today
    DueOverdue => "due_overdue";
    /// A due: date of today
    DueToday => "due_today";
    /// A due: date that is coming up
    DueSoon => "due_soon";
    /// Text between backticks
    Code => "code";
    /// A completed task
    Done => "done";
    /// The started: and owner: tags and the doing marker
    Doing => "doing";
    /// A blocked task's text and marker
    Blocked => "blocked";
    /// Label of normal mode and modes with no slot of their own
    ModeNormal => "mode_normal";
    /// Label of insert mode
    ModeInsert => "mode_insert";
    /// Label of visual mode
    ModeVisual => "mode_visual";
    /// Label of command mode
    ModeCommand => "mode_command";
    /// Border of a panel
    PanelBorder => "panel_border";
    /// Border of the panel with focus
    PanelBorderActive => "panel_border_active";
    /// Fill of the selected row or choice
    Selection => "selection";
    /// Fill of the row under the cursor
    Cursorline => "cursorline";
    /// A filter chip
    Chip => "chip";
    /// Fill of the status line
    StatusBg => "status_bg";
    /// Key shown in a hint
    HintKey => "hint_key";
    /// Text shown in a hint
    HintDesc => "hint_desc";
    /// Warnings and stale claims
    Warning => "warning";
    /// Errors
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
