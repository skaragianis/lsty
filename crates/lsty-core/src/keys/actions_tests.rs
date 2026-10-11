use super::{Action, Category, Mode, actions::Kind, defaults::defaults};

fn has_action(mode: Mode, action: Action) -> bool {
    defaults(mode).iter().any(|&(a, _)| a == action)
}

#[test]
fn a_mode_sits_at_its_index_in_all() {
    for (i, mode) in Mode::ALL.into_iter().enumerate() {
        assert_eq!(mode.index(), i, "{mode:?}");
    }
}

#[test]
fn actions_are_complete() {
    for a in Action::ALL {
        assert!(!a.description().is_empty(), "28.36 {a}");
        assert!(!a.hint().is_empty(), "28.36 {a}");
        assert!(Mode::ALL.iter().any(|&m| has_action(m, *a)), "28.36 {a}");
    }
}

#[test]
fn every_default_is_an_action() {
    for mode in Mode::ALL {
        for &(action, _) in defaults(mode) {
            assert!(
                Action::ALL.contains(&action),
                "28.36 [{}] {action} missing",
                mode.as_str()
            );
        }
    }
}

#[test]
fn every_entry_with_escape_has_ctrl_c() {
    for mode in Mode::ALL {
        for &(action, keys) in defaults(mode) {
            if keys.contains(&"<Esc>") {
                let name = mode.as_str();
                assert!(keys.contains(&"<C-c>"), "[{name}] {action}");
            }
        }
    }
}

#[test]
fn names_are_unique_and_round_trip() {
    for (i, a) in Action::ALL.iter().enumerate() {
        let earlier = &Action::ALL[..i];
        assert!(earlier.iter().all(|b| b.name() != a.name()), "{a}");
        assert_eq!(Action::from_name(a.name()), Some(*a));
        assert_eq!(a.to_string(), a.name());
    }
    assert_eq!(Action::from_name("frobnicate"), None);
    assert_eq!(Action::from_name("Quit"), None);
}

const MOTION: &[Action] = &[
    Action::CursorDown,
    Action::CursorUp,
    Action::HalfPageDown,
    Action::HalfPageUp,
    Action::PageDown,
    Action::PageUp,
    Action::ScrollDown,
    Action::ScrollUp,
    Action::CentreView,
];
const ITEM: &[Action] = &[
    Action::ToggleComplete,
    Action::CycleState,
    Action::Stop,
    Action::Delete,
    Action::Yank,
    Action::CyclePriority,
    Action::CyclePriorityBack,
    Action::SetDue,
    Action::AddTag,
    Action::RemoveTag,
];

fn want_kind(action: Action) -> Kind {
    const JUMP: &[Action] = &[
        Action::GoTop,
        Action::GoBottom,
        Action::GoViewTop,
        Action::GoViewMiddle,
        Action::GoViewBottom,
    ];
    const MOVE: &[Action] = &[Action::MoveItemDown, Action::MoveItemUp];
    const OWN: &[Action] = &[Action::Repeat, Action::OpenLink];
    let table = [
        (JUMP, Kind::Jump),
        (MOVE, Kind::Move),
        (OWN, Kind::Own),
        (MOTION, Kind::Motion),
        (ITEM, Kind::ItemAction),
    ];
    table
        .iter()
        .find(|(list, _)| list.contains(&action))
        .map_or(Kind::Other, |&(_, kind)| kind)
}

#[test]
fn kinds() {
    for &a in Action::ALL {
        assert_eq!(a.kind(), want_kind(a), "28.36 {a}");
    }
}

#[test]
fn mode_names_round_trip() {
    for mode in Mode::ALL {
        assert_eq!(Mode::from_name(mode.as_str()), Some(mode));
    }
    assert_eq!(Mode::from_name("vizual"), None);
}

#[test]
fn categories_are_headed_in_words() {
    let rows = [
        (Category::Navigation, "Navigation"),
        (Category::Editing, "Editing"),
        (Category::Items, "Items"),
        (Category::History, "History"),
        (Category::View, "View"),
        (Category::Windows, "Windows"),
        (Category::General, "General"),
    ];
    for (category, heading) in rows {
        assert_eq!(category.as_str(), heading);
    }
}

#[test]
fn an_action_lists_the_modes_that_have_it() {
    let rows: [(Action, &[Mode]); 6] = [
        (
            Action::GoTop,
            &[
                Mode::Normal,
                Mode::Visual,
                Mode::Panel,
                Mode::Detail,
                Mode::Help,
                Mode::Picker,
            ],
        ),
        (Action::Quit, &[Mode::Normal, Mode::Panel, Mode::Detail]),
        (Action::NextLink, &[Mode::Detail]),
        (Action::Cancel, &[Mode::Insert, Mode::Command]),
        (Action::Accept, &[Mode::Completion, Mode::Picker]),
        (Action::Commit, &[Mode::Insert]),
    ];
    for (action, modes) in rows {
        let got: Vec<Mode> = Mode::ALL
            .into_iter()
            .filter(|&m| has_action(m, action))
            .collect();
        assert_eq!(got, modes, "{action}");
    }
}

/// The modes that have a list or a text to move in; the others edit a
/// line.
pub(super) const MOVING: [Mode; 6] = [
    Mode::Normal,
    Mode::Visual,
    Mode::Panel,
    Mode::Detail,
    Mode::Help,
    Mode::Picker,
];

pub(super) fn motions() -> Vec<Action> {
    let moves = |a: &&Action| a.is_motion();
    Action::ALL.iter().filter(moves).copied().collect()
}

#[test]
fn every_mode_that_moves_binds_every_motion() {
    for mode in MOVING {
        for action in motions() {
            assert!(has_action(mode, action), "[{}] {action}", mode.as_str());
        }
    }
    for mode in Mode::ALL.into_iter().filter(|m| !MOVING.contains(m)) {
        for action in motions() {
            assert!(!has_action(mode, action), "[{}] {action}", mode.as_str());
        }
    }
}
