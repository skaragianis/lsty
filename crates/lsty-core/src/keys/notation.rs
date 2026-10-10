//! Key notation: `gg`, `<C-w>h`, `<leader>d`.

use crate::{
    case::{lower, simple_upper},
    errs::{self, Code, quote},
};

/// The modifier keys held with a [`Stroke`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    /// No modifier.
    pub const NONE: Self = Self(0);
    /// Ctrl.
    pub const CTRL: Self = Self(1);
    /// Alt.
    pub const ALT: Self = Self(2);
    /// Shift.
    pub const SHIFT: Self = Self(4);

    /// Both sets together.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every modifier of `other` is in this set.
    pub(crate) const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    /// This set without the modifiers of `other`.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Whether no modifier is held.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A key that has a name instead of a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Named {
    /// Enter, written `<CR>`.
    Cr,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Backspace, written `<BS>`.
    Bs,
    /// Delete, written `<Del>`.
    Del,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// A function key.
    F(FunctionKey),
}

const FUNCTION_KEYS: [&str; 12] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

/// The number of a function key, 1 to 12.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FunctionKey(u8);

impl FunctionKey {
    /// The key `F<n>`, if `n` is 1 to 12.
    pub(crate) const fn new(n: u8) -> Option<Self> {
        if n >= 1 && n <= 12 {
            Some(Self(n))
        } else {
            None
        }
    }

    /// The number, 1 to 12.
    #[cfg(test)]
    pub(crate) const fn get(self) -> u8 {
        self.0
    }
}

impl Named {
    /// The function key `F<n>`, if `n` is 1 to 12.
    pub const fn function(n: u8) -> Option<Self> {
        match FunctionKey::new(n) {
            Some(f) => Some(Self::F(f)),
            None => None,
        }
    }

    /// The name inside `<…>`: `CR`, `PageUp`, `F5`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cr => "CR",
            Self::Esc => "Esc",
            Self::Tab => "Tab",
            Self::Bs => "BS",
            Self::Del => "Del",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Home => "Home",
            Self::End => "End",
            Self::PageUp => "PageUp",
            Self::PageDown => "PageDown",
            Self::F(n) => FUNCTION_KEYS[usize::from(n.0 - 1)],
        }
    }
}

/// What a key is: a character or a named key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable character; the space bar is `Char(' ')`.
    Char(char),
    /// A named key.
    Named(Named),
}

/// One key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Stroke {
    /// The key.
    pub key: Key,
    /// The modifiers held with it.
    pub mods: Mods,
}

impl Stroke {
    /// A character with no modifier.
    pub const fn char(c: char) -> Self {
        Self {
            key: Key::Char(c),
            mods: Mods::NONE,
        }
    }

    /// A named key with no modifier.
    pub const fn named(n: Named) -> Self {
        Self {
            key: Key::Named(n),
            mods: Mods::NONE,
        }
    }

    /// The same key with `mods` held.
    #[must_use]
    pub const fn with_mods(self, mods: Mods) -> Self {
        Self { mods, ..self }
    }

    /// The value of this stroke when it is a digit with no modifier.
    pub(super) fn plain_digit(self) -> Option<u32> {
        match self.key {
            Key::Char(c) if self.mods.is_empty() => c.to_digit(10),
            Key::Char(_) | Key::Named(_) => None,
        }
    }
}

/// What a name inside `<…>` stands for.
#[derive(Clone, Copy)]
enum Name {
    Space,
    Key(Named),
}

impl Name {
    fn label(self) -> &'static str {
        match self {
            Self::Space => "Space",
            Self::Key(n) => n.as_str(),
        }
    }

    const fn stroke(self) -> Stroke {
        match self {
            Self::Space => Stroke::char(' '),
            Self::Key(n) => Stroke::named(n),
        }
    }
}

/// Lower-case spellings of the named keys.
const NAMES: [(&str, Name); 16] = [
    ("cr", Name::Key(Named::Cr)),
    ("enter", Name::Key(Named::Cr)),
    ("return", Name::Key(Named::Cr)),
    ("esc", Name::Key(Named::Esc)),
    ("tab", Name::Key(Named::Tab)),
    ("bs", Name::Key(Named::Bs)),
    ("del", Name::Key(Named::Del)),
    ("up", Name::Key(Named::Up)),
    ("down", Name::Key(Named::Down)),
    ("left", Name::Key(Named::Left)),
    ("right", Name::Key(Named::Right)),
    ("home", Name::Key(Named::Home)),
    ("end", Name::Key(Named::End)),
    ("pageup", Name::Key(Named::PageUp)),
    ("pagedown", Name::Key(Named::PageDown)),
    ("space", Name::Space),
];

fn name_of(lower: &str) -> Option<Name> {
    NAMES
        .iter()
        .find(|(spelling, _)| *spelling == lower)
        .map(|&(_, name)| name)
        .or_else(|| {
            let n = FUNCTION_KEYS
                .iter()
                .position(|f| f.eq_ignore_ascii_case(lower))?;
            Named::function(u8::try_from(n + 1).ok()?).map(Name::Key)
        })
}

fn invalid(message: String) -> errs::Error {
    errs::Error::new(Code::InvalidValue, message)
}

/// Reads a binding written in key notation into its strokes. `leader` is
/// what `<leader>` stands for; `None` makes `<leader>` invalid.
pub fn parse_keys(
    s: &str,
    leader: Option<Stroke>,
) -> errs::Result<Vec<Stroke>> {
    if s.is_empty() {
        return Err(invalid(format!("invalid key {}", quote(s))));
    }
    if let Some(name) = name_of(&lower(s)) {
        let letters: Vec<String> = s.chars().map(String::from).collect();
        return Err(invalid(format!(
            "{} reads as the keys {}; write {}",
            quote(s),
            letters.join("-"),
            quote(format!("<{}>", name.label())),
        )));
    }
    let mut strokes = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        let Some((stroke, size)) = next_stroke(rest, leader) else {
            return Err(invalid(format!("invalid key {}", quote(s))));
        };
        strokes.push(stroke);
        rest = rest.get(size..).unwrap_or_default();
    }
    Ok(strokes)
}

/// The stroke at the start of `s` and the bytes it takes.
fn next_stroke(s: &str, leader: Option<Stroke>) -> Option<(Stroke, usize)> {
    let first = s.chars().next()?;
    if first != '<' {
        return Some((Stroke::char(first), first.len_utf8()));
    }
    let end = s.find('>')?;
    let stroke = bracket(s.get(1..end)?, leader)?;
    Some((stroke, end + 1))
}

/// The key written between `<` and `>`.
fn bracket(inner: &str, leader: Option<Stroke>) -> Option<Stroke> {
    let name = lower(inner);
    if name == "leader" {
        return leader;
    }
    let (mods, rest) = split_modifiers(&name);
    if let Some(stroke) = named_stroke(rest) {
        return Some(stroke.with_mods(mods));
    }
    let mut chars = rest.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if mods.is_empty() {
        return None;
    }
    if mods.has(Mods::SHIFT) {
        let upper = simple_upper(c);
        return Some(Stroke::char(upper).with_mods(mods.without(Mods::SHIFT)));
    }
    Some(Stroke::char(c).with_mods(mods))
}

/// The leading `C-`, `A-` and `S-` of a lower-case name.
fn split_modifiers(mut s: &str) -> (Mods, &str) {
    let mut mods = Mods::NONE;
    while let Some((letter, rest)) = modifier(s) {
        mods = mods.with(letter);
        s = rest;
    }
    (mods, s)
}

/// One modifier prefix of `s` and what follows it.
fn modifier(s: &str) -> Option<(Mods, &str)> {
    let bytes = s.as_bytes();
    if bytes.len() <= 2 || bytes.get(1) != Some(&b'-') {
        return None;
    }
    let mods = match bytes.first()? {
        b'c' => Mods::CTRL,
        b'a' => Mods::ALT,
        b's' => Mods::SHIFT,
        _ => return None,
    };
    Some((mods, s.get(2..)?))
}

fn named_stroke(name: &str) -> Option<Stroke> {
    match name {
        "lt" => Some(Stroke::char('<')),
        "gt" => Some(Stroke::char('>')),
        _ => name_of(name).map(Name::stroke),
    }
}
