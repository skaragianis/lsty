//! The agent safety matrix: one row per mutation command, saying how an
//! agent names the item, what a retry does, what is checked and what
//! changes.

/// What running a command again does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryKind {
    /// The second run changes nothing.
    NoOp,
    /// The second run does the work again.
    Repeats,
    /// The second run is refused because the list moved.
    Detected,
}

/// The outcome of a retry; `when` limits a no-op, such as to the day of
/// the first call when a relative date was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    /// What the retry does.
    pub kind: RetryKind,
    /// The condition under which the kind holds.
    pub when: &'static str,
    /// The exception to the kind.
    pub unless: &'static str,
}

/// One mutation command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// The command, as `help` names it.
    pub command: &'static str,
    /// How an agent names the item.
    pub agent_ref: &'static str,
    /// What a retry by id does.
    pub retry: Retry,
    /// What the command checks before it writes.
    pub checks: &'static [&'static str],
    /// The files it may write.
    pub files: &'static [&'static str],
    /// What else it does.
    pub effects: &'static [&'static str],
    /// Whether the numbers of other items may shift.
    pub renumbers: bool,
    /// Whether it may report items in `changed`.
    pub changes: bool,
}

const fn retry(kind: RetryKind) -> Retry {
    Retry {
        kind,
        when: "",
        unless: "",
    }
}

const fn when(kind: RetryKind, when: &'static str) -> Retry {
    Retry {
        kind,
        when,
        unless: "",
    }
}

const TODO: &[&str] = &["todo"];
const RULES: &str = "rule layer";
const WARN: &str = "claimed: warn";
const NO_EFFECTS: &[&str] = &[];

const ADD_RETRY: Retry = Retry {
    kind: RetryKind::Repeats,
    when: "",
    unless: "no-op with --if-absent (by --id=KEY always; by text on \
             the same day)",
};

const ID_RETRY: Retry = Retry {
    kind: RetryKind::NoOp,
    when: "",
    unless: "by NR after the lines shifted: detected",
};

const SAME_DAY: &str = "same day";

/// The rows of the commands that write, in the order `help` lists them.
pub const MATRIX: &[Row] = &[
    Row {
        command: "add",
        agent_ref: "\u{2014}",
        retry: ADD_RETRY,
        checks: &[RULES],
        files: TODO,
        effects: &[
            "an agent's add writes an id",
            "--top renumbers everything, --after and --before the lines \
             below",
            "--if-absent --id may give the existing item an id",
        ],
        renumbers: true,
        changes: true,
    },
    Row {
        command: "addm",
        agent_ref: "\u{2014}",
        retry: ADD_RETRY,
        checks: &[RULES, "--id=ID is a usage error"],
        files: TODO,
        effects: &["as add, in one write"],
        renumbers: true,
        changes: true,
    },
    Row {
        command: "append",
        agent_ref: "id:",
        retry: retry(RetryKind::Repeats),
        checks: &[RULES, WARN],
        files: TODO,
        effects: &["fuzzy dates expanded"],
        renumbers: false,
        changes: false,
    },
    Row {
        command: "prepend",
        agent_ref: "id:",
        retry: retry(RetryKind::Repeats),
        checks: &[RULES, WARN],
        files: TODO,
        effects: &["fuzzy dates expanded"],
        renumbers: false,
        changes: false,
    },
    Row {
        command: "replace",
        agent_ref: "id:",
        retry: when(RetryKind::NoOp, SAME_DAY),
        checks: &["claim guard", RULES],
        files: TODO,
        effects: &[
            "keeps dep:, started:, owner: and rec: unless set in the new \
             text",
            "id: and the creation date are always kept",
        ],
        renumbers: false,
        changes: false,
    },
    Row {
        command: "pri",
        agent_ref: "id:",
        retry: retry(RetryKind::NoOp),
        checks: &[WARN],
        files: TODO,
        effects: NO_EFFECTS,
        renumbers: false,
        changes: false,
    },
    Row {
        command: "depri",
        agent_ref: "id:",
        retry: retry(RetryKind::NoOp),
        checks: &[WARN],
        files: TODO,
        effects: NO_EFFECTS,
        renumbers: false,
        changes: false,
    },
    Row {
        command: "due",
        agent_ref: "id:",
        retry: when(RetryKind::NoOp, SAME_DAY),
        checks: &[WARN],
        files: TODO,
        effects: NO_EFFECTS,
        renumbers: false,
        changes: false,
    },
    Row {
        command: "tag",
        agent_ref: "id:",
        retry: when(RetryKind::NoOp, "due:/t: values: same day"),
        checks: &[RULES, WARN],
        files: TODO,
        effects: &[
            "structural tags behave as their dedicated commands",
            "started: and owner: can't be changed by an agent",
        ],
        renumbers: false,
        changes: false,
    },
    Row {
        command: "untag",
        agent_ref: "id:",
        retry: retry(RetryKind::NoOp),
        checks: &[RULES, WARN],
        files: TODO,
        effects: &["untag id:\u{2026} cascades"],
        renumbers: false,
        changes: true,
    },
    Row {
        command: "id",
        agent_ref: "NR + --expect, or id:",
        retry: ID_RETRY,
        checks: &["refused if it has a different id", "id_taken"],
        files: TODO,
        effects: &["writes an id"],
        renumbers: false,
        changes: false,
    },
];
