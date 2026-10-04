use super::defaults::defaults;
use super::{Action, Category, Kind, Mode};

#[test]
fn actions_are_complete() {
    for a in Action::ALL {
        assert!(!a.description().is_empty(), "28.36 {a}");
        assert!(!a.hint().is_empty(), "28.36 {a}");
        assert!(!a.modes().is_empty(), "28.36 {a}");
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
    const JUMP: &[Action] = &[Action::GoTop, Action::GoBottom];
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
fn an_action_lists_the_modes_that_bind_it_by_default() {
    let rows: [(Action, &[Mode]); 5] = [
        (Action::GoTop, &[Mode::Normal, Mode::Visual, Mode::Panel]),
        (Action::Quit, &[Mode::Normal, Mode::Panel]),
        (Action::Cancel, &[Mode::Insert, Mode::Command]),
        (Action::Accept, &[Mode::Completion, Mode::Picker]),
        (Action::Commit, &[Mode::Insert]),
    ];
    for (action, modes) in rows {
        assert_eq!(action.modes(), modes, "{action}");
    }
}
