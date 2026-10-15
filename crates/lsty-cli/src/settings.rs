//! What the variables of todo.sh, from the environment or its config file,
//! set in lsty's config.

use lsty_core::{
    Vars,
    config::{Config, Warning},
    errs::{self, Result},
    todosh::Todosh,
};

use crate::parse::{self, NO_N};

/// A variable of todo.sh and where its value came from.
struct Setting {
    value: String,
    source: String,
    line: Option<usize>,
}

impl Setting {
    /// `name` from the environment, else from the config file.
    fn of(todosh: &Todosh, vars: &Vars<'_>, name: &str) -> Option<Self> {
        let env = vars(name).filter(|v| !v.is_empty());
        if let Some(value) = env {
            return Some(Self {
                value: errs::lossy_path(value),
                source: "the environment".to_owned(),
                line: None,
            });
        }
        let (value, line) = todosh.get(name)?;
        Some(Self {
            value: value.to_owned(),
            source: todosh.source.clone(),
            line: Some(line),
        })
    }

    fn place(&self) -> String {
        match self.line {
            Some(line) => format!("line {line} of {}", self.source),
            None => self.source.clone(),
        }
    }
}

/// Puts `TODOTXT_AUTO_ARCHIVE` and `TODOTXT_DATE_ON_ADD` in `config`. A
/// flag that is neither `0` nor `1` is a warning.
///
/// # Errors
///
/// `TODOTXT_PRESERVE_LINE_NUMBERS=0`, which is `-n`.
pub(crate) fn apply(
    config: &mut Config,
    todosh: &Todosh,
    vars: &Vars<'_>,
) -> Result<Vec<Warning>> {
    let mut warnings = Vec::new();
    let mut flag = |name: &str| {
        let set = Setting::of(todosh, vars, name)?;
        match set.value.as_str() {
            "1" => Some((true, set)),
            "0" => Some((false, set)),
            other => {
                let line =
                    set.line.map_or(String::new(), |n| format!("line {n}: "));
                let message =
                    format!("{line}{name}: expected 0 or 1, not \"{other}\"");
                warnings.push(Warning::new(&set.source, message));
                None
            }
        }
    };
    if let Some((on, _)) = flag("TODOTXT_AUTO_ARCHIVE") {
        config.auto_archive = on;
    }
    if let Some((on, _)) = flag("TODOTXT_DATE_ON_ADD") {
        config.include_creation = on;
    }
    flag("TODOTXT_PLAIN");
    if let Some((false, set)) = flag("TODOTXT_PRESERVE_LINE_NUMBERS") {
        let message = format!(
            "{NO_N} (TODOTXT_PRESERVE_LINE_NUMBERS=0 in {})",
            set.place()
        );
        return Err(parse::usage(message));
    }
    Ok(warnings)
}

/// Whether `TODOTXT_PLAIN` is `1`: colour is off unless asked for.
pub(crate) fn plain(todosh: &Todosh, vars: &Vars<'_>) -> bool {
    Setting::of(todosh, vars, "TODOTXT_PLAIN").is_some_and(|s| s.value == "1")
}
