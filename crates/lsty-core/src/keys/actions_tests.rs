use super::actions::{Kind, kind_of};
use super::defaults::defaults;
use super::{Mode, registry};

#[test]
fn registry_actions_are_complete() {
    for a in registry() {
        assert!(!a.description.is_empty(), "28.36 {}", a.name);
        assert!(!a.hint.is_empty(), "28.36 {}", a.name);
        assert!(!a.modes.is_empty(), "28.36 {}", a.name);
    }
}

#[test]
fn registry_covers_the_defaults() {
    let names: Vec<_> = registry().iter().map(|a| a.name).collect();
    for mode in Mode::ALL {
        for &(action, _) in defaults(mode) {
            assert!(
                names.contains(&action),
                "28.36 [{}] {action} missing",
                mode.as_str()
            );
        }
    }
}

#[test]
fn registry_names_are_unique() {
    let reg = registry();
    for (i, a) in reg.iter().enumerate() {
        assert!(reg[..i].iter().all(|b| b.name != a.name), "{}", a.name);
    }
}

const MOTION: &[&str] =
    &["cursor_down", "cursor_up", "half_page_down", "half_page_up"];
const ITEM: &[&str] = &[
    "toggle_complete",
    "cycle_state",
    "stop",
    "delete",
    "yank",
    "cycle_priority",
    "cycle_priority_back",
    "set_due",
    "add_tag",
    "remove_tag",
];

fn want_kind(name: &str) -> Kind {
    match name {
        "go_top" | "go_bottom" => Kind::Jump,
        "move_item_down" | "move_item_up" => Kind::Move,
        "repeat" | "open_link" => Kind::Own,
        _ if MOTION.contains(&name) => Kind::Motion,
        _ if ITEM.contains(&name) => Kind::ItemAction,
        _ => Kind::Other,
    }
}

#[test]
fn registry_kinds() {
    for a in registry() {
        assert_eq!(a.kind, want_kind(a.name), "28.36 {}", a.name);
        assert_eq!(kind_of(a.name), a.kind);
    }
    for name in MOTION.iter().chain(ITEM) {
        assert!(registry().iter().any(|a| a.name == *name), "{name}");
    }
}

#[test]
fn mode_names_round_trip() {
    for mode in Mode::ALL {
        assert_eq!(Mode::from_name(mode.as_str()), Some(mode));
    }
    assert_eq!(Mode::from_name("vizual"), None);
}
