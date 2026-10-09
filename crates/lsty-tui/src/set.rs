//! `:set`: changes the session's copy of the options.

use lsty_core::config::{self, Config, Value};
use lsty_core::errs::{Code, Error, Result};

use crate::model::Model;
use crate::rows::Shows;
use crate::setform::{self as form, Form, Kind, Op};
use crate::setshow::{number_flags, number_mode, value_text};

/// How the message of a refused `:set` begins.
const PREFIX: &str = ":set: ";

/// The refusal that says `problem`.
fn refusal(problem: &str) -> Error {
    Error::new(Code::InvalidValue, format!("{PREFIX}{problem}"))
}

/// The refusal of a change, naming its first problem.
fn first_problem(problems: &[String]) -> Error {
    refusal(problems.first().map_or("not accepted", String::as_str))
}

/// What `cfg` is once the word `f` has changed it, or the text of a
/// query.
fn apply(cfg: &Config, f: &Form) -> Result<(Config, Option<String>)> {
    let Some(kind) = form::kind_of(&f.name) else {
        if config::is_option(&f.name) {
            let text = format!("{} cannot be set at run time", f.name);
            return Err(refusal(&text));
        }
        let problems = config::set(cfg, &f.name, &Value::Other)
            .err()
            .unwrap_or_default();
        return Err(first_problem(&problems));
    };
    if f.op == Op::Query {
        return Ok((cfg.clone(), Some(value_text(cfg, &f.name))));
    }
    changed(cfg, f, kind)
        .map(|next| (next, None))
        .map_err(|problems| first_problem(&problems))
}

type Changed = Result<Config, Vec<String>>;

fn changed(cfg: &Config, f: &Form, kind: Kind) -> Changed {
    match kind {
        Kind::Flag => change_flag(cfg, f),
        Kind::List => change_list(cfg, f),
        Kind::Value | Kind::Number => {
            change_value(cfg, f, kind == Kind::Number)
        }
    }
}

fn change_flag(cfg: &Config, f: &Form) -> Changed {
    let on = match f.op {
        Op::On => true,
        Op::Off => false,
        Op::Assign | Op::Add | Op::Remove | Op::Query => {
            return flag_value(cfg, f);
        }
    };
    let (nu, rnu) = number_flags(cfg.line_numbers);
    let mode = match f.name.as_str() {
        "nu" => number_mode(on, rnu),
        "rnu" => number_mode(nu, on),
        _ => return config::set(cfg, &f.name, &Value::Bool(on)),
    };
    config::set(cfg, "line_numbers", &Value::Str(mode.as_str().to_owned()))
}

/// A flag given a value: the option decides whether it takes it.
fn flag_value(cfg: &Config, f: &Form) -> Changed {
    if f.name == "nu" || f.name == "rnu" {
        return Err(vec![format!("{}: expected bool", f.name)]);
    }
    config::set(cfg, &f.name, &Value::Str(f.value.clone()))
}

fn change_value(cfg: &Config, f: &Form, number: bool) -> Changed {
    match f.op {
        Op::Assign => config::set(cfg, &f.name, &typed(number, &f.value)),
        Op::Add | Op::Remove => Err(vec![format!("{} is not a list", f.name)]),
        Op::On | Op::Off | Op::Query => {
            Err(vec![format!("{} takes a value", f.name)])
        }
    }
}

/// The value `text` gives: a number for the options that are numbers,
/// when it reads as one.
fn typed(number: bool, text: &str) -> Value {
    if number && let Ok(n) = text.parse::<i64>() {
        return Value::Int(n);
    }
    Value::Str(text.to_owned())
}

fn change_list(cfg: &Config, f: &Form) -> Changed {
    let given: Vec<String> = if f.value.is_empty() {
        Vec::new()
    } else {
        f.value.split(',').map(str::to_owned).collect()
    };
    let now = || cfg.hidden_items.iter().map(|h| h.as_str().to_owned());
    let items: Vec<String> = match f.op {
        Op::Assign => given,
        Op::Add => now().chain(given).collect(),
        Op::Remove => now().filter(|item| !given.contains(item)).collect(),
        Op::On | Op::Off | Op::Query => {
            return Err(vec![format!("{} takes a list", f.name)]);
        }
    };
    config::set(cfg, &f.name, &Value::List(items))
}

impl Model {
    /// `:set WORD…`: every word changes the options or asks about one.
    /// Nothing changes when a word is refused.
    pub(crate) fn set_line(&mut self, words: &[String]) -> Result<()> {
        if words.is_empty() {
            return Err(Error::new(Code::Usage, ":set needs an option"));
        }
        let forms: Vec<Form> = words.iter().map(|w| form::parse(w)).collect();
        let mut cfg = self.deps.config.clone();
        let mut shown = Vec::new();
        for f in &forms {
            let (next, text) = apply(&cfg, f)?;
            cfg = next;
            shown.extend(text);
        }
        self.sync_shown(&forms);
        self.adopt_config(cfg);
        self.message = Some(shown.join("; ")).filter(|m| !m.is_empty());
        Ok(())
    }

    /// Makes the options held say what the screen shows for an option a
    /// word sets, so that adopting the new options applies it even when
    /// it is the same as the options held.
    fn sync_shown(&mut self, forms: &[Form]) {
        for f in forms.iter().filter(|f| f.op != Op::Query) {
            match f.name.as_str() {
                "show_sidebar" => {
                    self.deps.config.show_sidebar = self.sidebar_on;
                }
                "show_detail" => self.deps.config.show_detail = self.detail_on,
                _ => {}
            }
        }
    }

    /// Makes `cfg` the options of the session and brings the views in
    /// line with what changed.
    fn adopt_config(&mut self, cfg: Config) {
        let was = std::mem::replace(&mut self.deps.config, cfg);
        let now = &self.deps.config;
        if now.show_sidebar != was.show_sidebar {
            self.sidebar_on = now.show_sidebar;
            self.overlay = false;
        }
        if now.show_detail != was.show_detail {
            self.detail_on = now.show_detail;
        }
        let hidden = now.hidden_items != was.hidden_items;
        if hidden {
            self.shows = Shows::new(now);
        }
        let derive = now.claim_stale_after != was.claim_stale_after;
        let days = now.upcoming_days != was.upcoming_days;
        let names = now.inline_projects != was.inline_projects
            || now.inline_contexts != was.inline_contexts;
        if derive {
            self.derive_again();
        } else if hidden {
            self.refresh();
        } else if days {
            self.reshow();
        } else if names {
            self.rebuild_rows();
        }
        self.relayout();
    }
}
