use std::path::{Path, PathBuf};

use super::{Paths, find_paths};

/// Env, then the expected config file, keys file, themes folder, state
/// folder and whether `LSTY_CONFIG` was set.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str)],
    [&'static str; 4],
    bool,
);

const USER_STATE: &str = "/home/u/.local/state/lsty";
const USER_CONFIG: [&str; 3] = [
    "/home/u/.config/lsty/config.toml",
    "/home/u/.config/lsty/keys.toml",
    "/home/u/.config/lsty/themes",
];

const fn user(state: &'static str) -> [&'static str; 4] {
    [USER_CONFIG[0], USER_CONFIG[1], USER_CONFIG[2], state]
}

const ROWS: &[Row] = &[
    (
        "07.4 XDG_CONFIG_HOME set",
        &[("XDG_CONFIG_HOME", "/x")],
        [
            "/x/lsty/config.toml",
            "/x/lsty/keys.toml",
            "/x/lsty/themes",
            USER_STATE,
        ],
        false,
    ),
    ("07.5 unset", &[], user(USER_STATE), false),
    (
        "07.5 empty",
        &[("XDG_CONFIG_HOME", "")],
        user(USER_STATE),
        false,
    ),
    (
        "07.5 relative",
        &[("XDG_CONFIG_HOME", "relative/path")],
        user(USER_STATE),
        false,
    ),
    (
        "07.6 LSTY_CONFIG moves only the config file",
        &[("LSTY_CONFIG", "/y/c.toml"), ("XDG_CONFIG_HOME", "/x")],
        [
            "/y/c.toml",
            "/x/lsty/keys.toml",
            "/x/lsty/themes",
            USER_STATE,
        ],
        true,
    ),
    (
        "07.6 relative LSTY_CONFIG is kept as given",
        &[("LSTY_CONFIG", "c.toml")],
        ["c.toml", USER_CONFIG[1], USER_CONFIG[2], USER_STATE],
        true,
    ),
    (
        "empty LSTY_CONFIG is unset",
        &[("LSTY_CONFIG", "")],
        user(USER_STATE),
        false,
    ),
    (
        "07.7 XDG_STATE_HOME empty",
        &[("XDG_STATE_HOME", "")],
        user(USER_STATE),
        false,
    ),
    (
        "XDG_STATE_HOME set",
        &[("XDG_STATE_HOME", "/s")],
        user("/s/lsty"),
        false,
    ),
    (
        "XDG_STATE_HOME relative",
        &[("XDG_STATE_HOME", "s")],
        user(USER_STATE),
        false,
    ),
    (
        "a trailing slash and dots are cleaned",
        &[("XDG_CONFIG_HOME", "/x/./a/../")],
        [
            "/x/lsty/config.toml",
            "/x/lsty/keys.toml",
            "/x/lsty/themes",
            USER_STATE,
        ],
        false,
    ),
];

fn expected(files: [&str; 4], explicit: bool) -> Paths {
    Paths {
        config_file: PathBuf::from(files[0]),
        keys_file: PathBuf::from(files[1]),
        themes_dir: PathBuf::from(files[2]),
        state_dir: PathBuf::from(files[3]),
        explicit,
    }
}

#[test]
fn find_paths_rows() {
    for (name, env, files, explicit) in ROWS {
        let vars = |k: &str| {
            env.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        };
        let got = find_paths(&vars, Some(Path::new("/home/u")));
        assert_eq!(got, expected(*files, *explicit), "{name}");
    }
}

#[test]
fn find_paths_without_a_home_uses_relative_folders() {
    let got = find_paths(&|_| None, None);
    assert_eq!(got.config_file, Path::new(".config/lsty/config.toml"));
    assert_eq!(got.state_dir, Path::new(".local/state/lsty"));
}
