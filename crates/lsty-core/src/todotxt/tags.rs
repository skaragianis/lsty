use jiff::{Timestamp, civil::DateTime, tz::TimeZone};

use super::{
    filelink::FileLink,
    line::{Kind, Line, Priority, TokenView},
    parse::is_date,
};

const MAX_REC_N: u16 = 9999;

/// The unit of a recurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecUnit {
    /// `d`.
    Day,
    /// `w`.
    Week,
    /// `m`.
    Month,
    /// `y`.
    Year,
    /// `b`: business days.
    Business,
}

impl RecUnit {
    /// The unit's letter as written: `d`, `w`, `m`, `y` or `b`.
    pub(crate) const fn letter(self) -> u8 {
        match self {
            Self::Day => b'd',
            Self::Week => b'w',
            Self::Month => b'm',
            Self::Year => b'y',
            Self::Business => b'b',
        }
    }

    const fn from_letter(letter: u8) -> Option<Self> {
        match letter {
            b'd' => Some(Self::Day),
            b'w' => Some(Self::Week),
            b'm' => Some(Self::Month),
            b'y' => Some(Self::Year),
            b'b' => Some(Self::Business),
            _ => None,
        }
    }
}

/// A recurrence, `[+]N<unit>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rec {
    strict: bool,
    n: u16,
    unit: RecUnit,
}

impl Rec {
    /// A recurrence of `n` units, `None` unless `n` is 1 to 9999.
    pub(crate) const fn new(
        strict: bool,
        n: u16,
        unit: RecUnit,
    ) -> Option<Self> {
        if n == 0 || n > MAX_REC_N {
            return None;
        }
        Some(Self { strict, n, unit })
    }

    /// Whether the value had a leading `+`.
    pub(crate) const fn strict(self) -> bool {
        self.strict
    }

    /// How many units, 1 to 9999.
    pub(crate) const fn n(self) -> u16 {
        self.n
    }

    /// The unit.
    pub(crate) const fn unit(self) -> RecUnit {
        self.unit
    }

    /// Parses `[+]N<unit>`; `None` for anything else.
    pub(crate) fn parse(v: &[u8]) -> Option<Self> {
        let (strict, v) = match v.strip_prefix(b"+") {
            Some(rest) => (true, rest),
            None => (false, v),
        };
        let (&unit, digits) = v.split_last()?;
        let unit = RecUnit::from_letter(unit)?;
        if digits.first() == Some(&b'0')
            || digits.is_empty()
            || digits.len() > 4
            || !digits.iter().all(u8::is_ascii_digit)
        {
            return None;
        }
        let n = digits
            .iter()
            .fold(0u16, |acc, d| acc * 10 + u16::from(d - b'0'));
        Self::new(strict, n, unit)
    }
}

/// The typed view of a line's first-class tags. It borrows the line.
///
/// The first token of a single-valued key counts, and a bad value leaves
/// the field unset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tags<'a> {
    /// The `due:` date, `YYYY-MM-DD`.
    pub due: Option<&'a str>,
    /// The `t:` date, `YYYY-MM-DD`.
    pub threshold: Option<&'a str>,
    /// The `rec:` value.
    pub rec: Option<Rec>,
    /// The `pri:` letter.
    pub pri: Option<Priority>,
    /// The `started:` time, in UTC.
    pub started: Option<Timestamp>,
    /// Whether a `started:` token is present, whatever its value.
    pub underway: bool,
    /// The `owner:` value.
    pub owner: Option<&'a [u8]>,
    /// The `id:` value, as written by whichever tool wrote it.
    pub id: Option<&'a [u8]>,
    /// The `dep:` values, in order of first appearance.
    pub deps: Vec<&'a [u8]>,
    /// The `file:` values, in order.
    pub files: Vec<FileLink<'a>>,
    /// Repeats of single-valued keys.
    pub extra: Vec<TokenView<'a>>,
    /// First-class tags with a bad value.
    pub invalid: Vec<TokenView<'a>>,
}

/// The keys that count once per line. `Id` stays last: `COUNT` follows it.
#[derive(Clone, Copy)]
enum Single {
    Due,
    Threshold,
    Rec,
    Pri,
    Started,
    Owner,
    Id,
}

impl Single {
    const COUNT: usize = Self::Id as usize + 1;

    fn of(key: &[u8]) -> Option<Self> {
        match key {
            b"due" => Some(Self::Due),
            b"t" => Some(Self::Threshold),
            b"rec" => Some(Self::Rec),
            b"pri" => Some(Self::Pri),
            b"started" => Some(Self::Started),
            b"owner" => Some(Self::Owner),
            b"id" => Some(Self::Id),
            _ => None,
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// Whether only the first tag of `key` counts.
pub fn is_single_valued(key: &[u8]) -> bool {
    Single::of(key).is_some()
}

/// Whether a tag has no line of its own in `show` and the detail panel:
/// a single-valued key, `dep` or `file`, and `pri` on a done item.
pub fn is_structural(key: &[u8], done: bool) -> bool {
    if key == b"pri" {
        done
    } else {
        is_single_valued(key) || key == b"dep" || key == b"file"
    }
}

fn date(v: &[u8]) -> Option<&str> {
    let head: &[u8; 10] = v.try_into().ok()?;
    if is_date(head) {
        std::str::from_utf8(v).ok()
    } else {
        None
    }
}

/// `YYYY-MM-DDTHHMMZ` as a civil date and time, when it is a real one.
pub(crate) fn parse_stamp(v: &[u8]) -> Option<DateTime> {
    let [
        y1,
        y2,
        y3,
        y4,
        b'-',
        m1,
        m2,
        b'-',
        d1,
        d2,
        b'T',
        h1,
        h2,
        i1,
        i2,
        b'Z',
    ] = *v
    else {
        return None;
    };
    let num = |a: u8, b: u8| -> Option<i8> {
        (a.is_ascii_digit() && b.is_ascii_digit())
            .then(|| i8::try_from((a - b'0') * 10 + (b - b'0')).ok())?
    };
    let year = i16::from(num(y1, y2)?) * 100 + i16::from(num(y3, y4)?);
    DateTime::new(
        year,
        num(m1, m2)?,
        num(d1, d2)?,
        num(h1, h2)?,
        num(i1, i2)?,
        0,
        0,
    )
    .ok()
}

/// The instant of a `started:` value.
fn started(v: &[u8]) -> Option<Timestamp> {
    let at = parse_stamp(v)?;
    at.to_zoned(TimeZone::UTC).ok().map(|z| z.timestamp())
}

impl<'a> Tags<'a> {
    fn add(&mut self, tok: TokenView<'a>, seen: &mut [bool; Single::COUNT]) {
        if tok.key == b"dep" {
            if !self.deps.contains(&tok.value) {
                self.deps.push(tok.value);
            }
            return;
        }
        if tok.key == b"file" {
            self.files.push(FileLink::new(tok.value));
            return;
        }
        let Some(single) = Single::of(tok.key) else {
            return;
        };
        let flag = &mut seen[single.index()];
        if *flag {
            self.extra.push(tok);
            return;
        }
        *flag = true;
        if !self.set(single, tok.value) {
            self.invalid.push(tok);
        }
    }

    /// Stores a value; false when it is a bad one.
    fn set(&mut self, key: Single, v: &'a [u8]) -> bool {
        match key {
            Single::Due => {
                self.due = date(v);
                self.due.is_some()
            }
            Single::Threshold => {
                self.threshold = date(v);
                self.threshold.is_some()
            }
            Single::Rec => {
                self.rec = Rec::parse(v);
                self.rec.is_some()
            }
            Single::Pri => {
                self.pri = match v {
                    [letter] => Priority::new(*letter),
                    _ => None,
                };
                self.pri.is_some()
            }
            Single::Started => {
                self.underway = true;
                self.started = started(v);
                self.started.is_some()
            }
            Single::Owner => {
                self.owner = Some(v);
                true
            }
            Single::Id => {
                self.id = Some(v);
                true
            }
        }
    }
}

impl Line {
    /// The typed view of the line's first-class tags.
    pub fn tags(&self) -> Tags<'_> {
        let mut tags = Tags::default();
        let mut seen = [false; Single::COUNT];
        for tok in self.tokens().filter(|t| t.kind == Kind::Tag) {
            tags.add(tok, &mut seen);
        }
        tags
    }

    /// The priority of the item whatever its state: the `(A)` field, or
    /// for a done line without one the `pri:` tag.
    pub fn effective_priority(&self) -> Option<Priority> {
        if self.priority.is_some() || !self.done {
            self.priority
        } else {
            self.tags().pri
        }
    }
}
