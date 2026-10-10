use std::os::unix::ffi::OsStrExt as _;

use jiff::{
    Timestamp,
    tz::{Offset, TimeZone},
};

use super::date::Date;
use crate::{
    Vars,
    errs::{Error, Field, Result, lossy},
    todotxt::parse_stamp,
};

const ENV_NOW: &str = "LSTY_NOW";
const ENV_TODAY: &str = "LSTY_TODAY";
const FORM_NOW: &str = "YYYY-MM-DDTHHMMZ";
const FORM_TODAY: &str = "YYYY-MM-DD";
const NOON: i8 = 12;
const RANGE_NOW: &str = "an instant from 0000-01-01T0000Z to 9999-12-30T2200Z";
const RANGE_TODAY: &str =
    "a date whose 12:00 local time is at most 9999-12-30T2200Z";
const RANGE_LOCAL_DATE: &str =
    "an instant whose local date is in the years 0000 to 9999";

/// The single reading of time for one action.
#[derive(Clone, Debug)]
pub struct Clock {
    now: Timestamp,
    today: Date,
    zone: TimeZone,
    fixed: bool,
}

impl Clock {
    /// Reads the `LSTY_NOW` and `LSTY_TODAY` overrides through `var` and
    /// falls back to `real`. `zone` is the local zone. An empty variable
    /// counts as unset.
    pub fn new(
        var: &Vars<'_>,
        real: Timestamp,
        zone: TimeZone,
    ) -> Result<Self> {
        let value = |name| {
            var(name)
                .filter(|v| !v.is_empty())
                .map(|v| lossy(v.as_bytes()))
        };
        let now_var = value(ENV_NOW);
        let today_var = value(ENV_TODAY);

        let today_text = today_var.as_deref().unwrap_or_default();
        let now_text = now_var.as_deref().unwrap_or_default();

        let today_given = today_var
            .as_deref()
            .map(|v| {
                Date::parse(v).ok_or_else(|| invalid(ENV_TODAY, v, FORM_TODAY))
            })
            .transpose()?;
        let now_given = now_var.as_deref().map(parse_now).transpose()?;

        let now = match (now_given, today_given) {
            (Some(now), _) => now,
            (None, Some(day)) => noon(day, &zone).ok_or_else(|| {
                out_of_range(ENV_TODAY, today_text, RANGE_TODAY)
            })?,
            (None, None) => real,
        };
        let today = Date::local(now, &zone).ok_or_else(|| {
            out_of_range(ENV_NOW, now_text, RANGE_LOCAL_DATE)
        })?;
        if let (Some(_), Some(day)) = (now_given, today_given)
            && day != today
        {
            return Err(invalid(ENV_TODAY, today_text, FORM_TODAY));
        }
        let fixed = now_var.is_some() || today_var.is_some();
        Ok(Self {
            now,
            today,
            zone,
            fixed,
        })
    }

    /// The clock's instant.
    pub fn now(&self) -> Timestamp {
        self.now
    }

    /// The local date of [`Clock::now`].
    pub fn today(&self) -> Date {
        self.today
    }

    /// [`Clock::now`] as `YYYY-MM-DDTHHMMZ`, truncated to the minute.
    pub(crate) fn stamp(&self) -> String {
        self.now.strftime("%Y-%m-%dT%H%MZ").to_string()
    }

    /// Whether `LSTY_NOW` or `LSTY_TODAY` fixed the instant.
    pub const fn is_fixed(&self) -> bool {
        self.fixed
    }

    /// The local zone of the clock.
    pub fn zone(&self) -> &TimeZone {
        &self.zone
    }
}

fn noon(day: Date, zone: &TimeZone) -> Option<Timestamp> {
    zone.to_timestamp(day.civil().at(NOON, 0, 0, 0)).ok()
}

/// Parses `YYYY-MM-DDTHHMMZ`; a well-formed instant that `Timestamp`
/// cannot hold is out of range.
fn parse_now(s: &str) -> Result<Timestamp> {
    let time = parse_stamp(s.as_bytes())
        .ok_or_else(|| invalid(ENV_NOW, s, FORM_NOW))?;
    Offset::UTC
        .to_timestamp(time)
        .map_err(|_| out_of_range(ENV_NOW, s, RANGE_NOW))
}

fn invalid(field: &'static str, value: &str, form: &str) -> Error {
    rejected(
        field,
        value,
        format!("{field} must be {form}: {value}"),
        form,
    )
}

/// A well-formed override the clock cannot represent.
fn out_of_range(field: &'static str, value: &str, accepted: &str) -> Error {
    let message = format!("{field} is out of range: {value}");
    rejected(field, value, message, accepted)
}

fn rejected(
    field: &'static str,
    value: &str,
    message: String,
    accepted: &str,
) -> Error {
    Error::invalid_value(message, value, &[accepted])
        .with_field("field", Field::Str(field.to_owned()))
}
