use std::{ffi::OsString, fs, path::Path};

use lsty_core::{config::Config, errs::Code, todosh::Todosh};

use crate::settings::apply;

fn file(dir: &Path, text: &str) -> Todosh {
    let path = dir.join("cfg");
    fs::write(&path, text).unwrap();
    let none = |_: &str| None;
    Todosh::read(Some(&path), &none, None, dir).unwrap()
}

fn env<'a>(
    pairs: &'a [(&'a str, &'a str)],
) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| OsString::from(*v))
    }
}

#[test]
fn the_flags_set_the_config_and_the_environment_wins() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = file(
        dir.path(),
        "TODOTXT_AUTO_ARCHIVE=0\nTODOTXT_DATE_ON_ADD=1\n",
    );
    let mut config = Config {
        auto_archive: true,
        include_creation: false,
        ..Config::default()
    };
    let warnings = apply(&mut config, &cfg, &env(&[])).unwrap().0;
    assert_eq!(warnings.len(), 0);
    assert!(!config.auto_archive && config.include_creation);

    let vars =
        env(&[("TODOTXT_AUTO_ARCHIVE", "1"), ("TODOTXT_DATE_ON_ADD", "0")]);
    apply(&mut config, &cfg, &vars).unwrap();
    assert!(config.auto_archive && !config.include_creation);
}

#[test]
fn a_flag_that_is_not_0_or_1_warns_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = file(dir.path(), "\nTODOTXT_DATE_ON_ADD=yes\n");
    let mut config = Config::default();
    let before = config.include_creation;
    let warnings = apply(&mut config, &cfg, &env(&[])).unwrap().0;
    assert_eq!(config.include_creation, before);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].message,
        "line 2: TODOTXT_DATE_ON_ADD: expected 0 or 1, not \"yes\""
    );
    let warnings = apply(
        &mut config,
        &Todosh::default(),
        &env(&[("TODOTXT_PLAIN", "2")]),
    )
    .unwrap()
    .0;
    assert_eq!(warnings[0].source, "the environment");
}

#[test]
fn zero_line_numbers_are_refused_with_the_place() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = file(dir.path(), "\nTODOTXT_PRESERVE_LINE_NUMBERS=0\n");
    let e = apply(&mut Config::default(), &cfg, &env(&[])).unwrap_err();
    assert_eq!(e.code(), Code::Usage);
    assert!(e.message().contains("line 2 of "), "{}", e.message());
    let vars = env(&[("TODOTXT_PRESERVE_LINE_NUMBERS", "0")]);
    let e =
        apply(&mut Config::default(), &Todosh::default(), &vars).unwrap_err();
    assert!(e.message().ends_with("in the environment)"));
    let vars = env(&[("TODOTXT_PRESERVE_LINE_NUMBERS", "1")]);
    apply(&mut Config::default(), &cfg, &vars).unwrap();
}

#[test]
fn plain_is_only_one() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = file(dir.path(), "TODOTXT_PLAIN=1\n");
    let plain = |todosh: &Todosh, vars: &[(&str, &str)]| {
        apply(&mut Config::default(), todosh, &env(vars)).unwrap().1
    };
    assert!(plain(&cfg, &[]));
    assert!(!plain(&cfg, &[("TODOTXT_PLAIN", "0")]));
    assert!(plain(&Todosh::default(), &[("TODOTXT_PLAIN", "1")]));
    assert!(!plain(&Todosh::default(), &[]));
}
