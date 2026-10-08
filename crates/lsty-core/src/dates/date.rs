use std::fmt;

use jiff::civil;
use jiff::{Span, Timestamp};

use crate::todotxt::{RecUnit, is_date};

const MIN_YEAR: i16 = 0;
const MAX_YEAR: i16 = 9999;
/// Business days that span more than the years 0000 to 9999 (261 a year).
const MAX_BUSINESS_DAYS: i32 = 2_700_000;

/// A real calendar date in the years 0000 to 9999.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(civil::Date);

impl Date {
    /// The date `year-month-day`, or `None` if it is not a real date in
    /// the years 0000 to 9999.
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let year = i16::try_from(year).ok()?;
        let date = civil::Date::new(year, i8::try_from(month).ok()?, {
            i8::try_from(day).ok()?
        });
        date.ok().and_then(Self::checked)
    }

    fn checked(date: civil::Date) -> Option<Self> {
        (MIN_YEAR..=MAX_YEAR)
            .contains(&date.year())
            .then_some(Self(date))
    }

    /// Parses `YYYY-MM-DD`, a date that exists in the calendar.
    pub fn parse(s: &str) -> Option<Self> {
        let bytes: &[u8; 10] = s.as_bytes().try_into().ok()?;
        if !is_date(bytes) {
            return None;
        }
        let num = |range: std::ops::Range<usize>| -> Option<u16> {
            s.get(range)?.parse().ok()
        };
        let date = civil::Date::new(
            i16::try_from(num(0..4)?).ok()?,
            i8::try_from(num(5..7)?).ok()?,
            i8::try_from(num(8..10)?).ok()?,
        );
        date.ok().and_then(Self::checked)
    }

    /// The year, 0 to 9999.
    pub(crate) fn year(self) -> u16 {
        self.0.year().unsigned_abs()
    }

    /// The month, 1 to 12.
    pub(crate) fn month(self) -> u8 {
        self.0.month().unsigned_abs()
    }

    /// The day of the month, 1 to 31.
    pub(crate) fn day(self) -> u8 {
        self.0.day().unsigned_abs()
    }

    /// The local date of `at` in `zone`, or `None` when it falls outside
    /// the years 0000 to 9999.
    pub(crate) fn local(
        at: Timestamp,
        zone: &jiff::tz::TimeZone,
    ) -> Option<Self> {
        Self::checked(zone.to_datetime(at).date())
    }

    pub(super) fn civil(self) -> civil::Date {
        self.0
    }

    /// Whether the date is a Saturday or a Sunday.
    pub(crate) fn is_weekend(self) -> bool {
        matches!(
            self.0.weekday(),
            civil::Weekday::Saturday | civil::Weekday::Sunday
        )
    }

    /// Days of the week since Monday: Monday is 0, Sunday is 6.
    pub fn weekday_from_monday(self) -> i32 {
        i32::from(self.0.weekday().to_monday_zero_offset())
    }

    /// Moves the date by `n` units, `n` of either sign. `None` when the
    /// result falls outside the years 0000 to 9999.
    ///
    /// A month or year step keeps the day, or clamps it to the last day of
    /// the target month (Jan 31 + 1 month is Feb 28 or 29, Feb 29 + 1 year
    /// is Feb 28), and the clamp is not remembered by a later step. A
    /// business step counts only Monday to Friday, one day at a time in the
    /// direction of `n`; zero is the date itself.
    pub(crate) fn add(self, n: i32, unit: RecUnit) -> Option<Self> {
        match unit {
            RecUnit::Day => self.shift(Span::new().try_days(n).ok()?),
            RecUnit::Week => self.shift(Span::new().try_weeks(n).ok()?),
            RecUnit::Month => self.shift(Span::new().try_months(n).ok()?),
            RecUnit::Year => self.shift(Span::new().try_years(n).ok()?),
            RecUnit::Business => self.add_business(n),
        }
    }

    fn shift(self, span: Span) -> Option<Self> {
        Self::checked(self.0.checked_add(span).ok()?)
    }

    fn add_business(self, n: i32) -> Option<Self> {
        if n.unsigned_abs() > MAX_BUSINESS_DAYS.unsigned_abs() {
            return None;
        }
        let step = n.signum();
        let mut date = self;
        for _ in 0..n.abs() {
            date = date.next_weekday(step)?;
        }
        Some(date)
    }

    /// The nearest Monday-to-Friday day after (`step` 1) or before
    /// (`step` -1) the date.
    fn next_weekday(self, step: i32) -> Option<Self> {
        let span = Span::new().days(step);
        let mut date = self.shift(span)?;
        while date.is_weekend() {
            date = date.shift(span)?;
        }
        Some(date)
    }

    /// The number of days from `self` to `other`, negative when `other`
    /// is earlier.
    pub fn days_until(self, other: Self) -> i32 {
        (other.0 - self.0).get_days()
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}",
            self.year(),
            self.month(),
            self.day()
        )
    }
}
