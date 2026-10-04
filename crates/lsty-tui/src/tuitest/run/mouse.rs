//! The mouse commands of a transcript.

use unicode_width::UnicodeWidthStr;

use super::{Done, Harness, unquote};
use crate::{Button, Gesture, Mouse, Msg};

/// The button and modifier of a click, and how many clicks.
#[derive(Clone, Copy)]
struct Click {
    button: Button,
    shift: bool,
    times: usize,
}

const CLICKS: [(&str, Click); 4] = [
    (
        "click",
        Click {
            button: Button::Left,
            shift: false,
            times: 1,
        },
    ),
    (
        "shiftclick",
        Click {
            button: Button::Left,
            shift: true,
            times: 1,
        },
    ),
    (
        "dblclick",
        Click {
            button: Button::Left,
            shift: false,
            times: 2,
        },
    ),
    (
        "rightclick",
        Click {
            button: Button::Other,
            shift: false,
            times: 1,
        },
    ),
];

type Cell = (u16, u16);

impl Harness {
    /// Runs the mouse command `name`; `None` when it is not one.
    pub(super) fn mouse_command(
        &mut self,
        name: &str,
        args: &str,
    ) -> Option<Done> {
        if let Some((_, click)) = CLICKS.iter().find(|(n, _)| *n == name) {
            return Some(self.click_at(*click, args));
        }
        let on = name.strip_suffix("on");
        if let Some((_, click)) = CLICKS.iter().find(|(n, _)| Some(*n) == on) {
            return Some(self.click_on(*click, args));
        }
        match name {
            "drag" => Some(self.drag(args)),
            "dragon" => Some(self.drag_on(args)),
            "wheel" => Some(self.wheel(args)),
            "down" => Some(self.button(args, Gesture::Down(Button::Left))),
            "move" => Some(self.button(args, Gesture::Drag(Button::Left))),
            "up" => Some(self.button(args, Gesture::Up(Button::Left))),
            _ => None,
        }
    }

    fn mouse(&mut self, gesture: Gesture, at: Cell, shift: bool) -> Done {
        let (x, y) = at;
        self.send(Msg::Mouse(Mouse {
            gesture,
            x,
            y,
            shift,
        }))
    }

    fn click_at(&mut self, click: Click, args: &str) -> Done {
        let at = cell(args)?;
        self.click(click, at)
    }

    fn click_on(&mut self, click: Click, args: &str) -> Done {
        let [text] = texts::<1>(args)?;
        let at = self.find(&text)?;
        self.click(click, at)
    }

    fn click(&mut self, click: Click, at: Cell) -> Done {
        for _ in 0..click.times {
            let down = Gesture::Down(click.button);
            self.mouse(down, at, click.shift)?;
            self.mouse(Gesture::Up(click.button), at, click.shift)?;
        }
        Ok(())
    }

    fn button(&mut self, args: &str, gesture: Gesture) -> Done {
        let at = cell(args)?;
        self.mouse(gesture, at, false)
    }

    fn drag(&mut self, args: &str) -> Done {
        let words: Vec<&str> = args.split_whitespace().collect();
        let [x1, y1, x2, y2] = words.as_slice() else {
            return Err(format!("drag: want X1 Y1 X2 Y2, got {args:?}"));
        };
        let num = |text: &str| {
            text.parse::<u16>()
                .map_err(|_| format!("drag: want X1 Y1 X2 Y2, got {args:?}"))
        };
        let from = (num(x1)?, num(y1)?);
        let to = (num(x2)?, num(y2)?);
        self.drag_between(from, to)
    }

    fn drag_on(&mut self, args: &str) -> Done {
        let [from, to] = texts::<2>(args)?;
        let from = self.find(&from)?;
        let to = self.find(&to)?;
        self.drag_between(from, to)
    }

    /// Presses at `from`, moves one row at a time to `to`, releases.
    fn drag_between(&mut self, from: Cell, to: Cell) -> Done {
        let held = Gesture::Drag(Button::Left);
        self.mouse(Gesture::Down(Button::Left), from, false)?;
        let mut y = from.1;
        if y == to.1 {
            self.mouse(held, (to.0, y), false)?;
        }
        while y != to.1 {
            y = if to.1 > y { y + 1 } else { y - 1 };
            self.mouse(held, (to.0, y), false)?;
        }
        self.mouse(Gesture::Up(Button::Left), to, false)
    }

    fn wheel(&mut self, args: &str) -> Done {
        let words: Vec<&str> = args.split_whitespace().collect();
        let want = || format!("wheel: want up|down X Y [N], got {args:?}");
        let (dir, rest) = words.split_first().ok_or_else(want)?;
        let up = match *dir {
            "up" => true,
            "down" => false,
            _ => return Err(format!("wheel: want up or down, got {dir:?}")),
        };
        let at = match rest {
            [x, y] | [x, y, _] => cell(&format!("{x} {y}"))?,
            _ => return Err(want()),
        };
        let notches = match rest {
            [_, _, n] => n
                .parse::<usize>()
                .map_err(|_| format!("wheel: bad count {n:?}"))?,
            _ => 1,
        };
        for _ in 0..notches {
            self.mouse(Gesture::Wheel { up }, at, false)?;
        }
        Ok(())
    }

    /// The first cell, reading by rows, where the screen shows `text`.
    fn find(&self, text: &str) -> Result<Cell, String> {
        for (y, line) in self.frame()?.iter().enumerate() {
            if let Some((before, _)) = line.split_once(text) {
                let x =
                    u16::try_from(before.width()).map_err(|e| e.to_string());
                let y = u16::try_from(y).map_err(|e| e.to_string());
                return Ok((x?, y?));
            }
        }
        Err(format!("the screen does not show {text:?}"))
    }
}

/// `X Y` as a cell.
fn cell(args: &str) -> Result<Cell, String> {
    let words: Vec<&str> = args.split_whitespace().collect();
    let want = || format!("want X Y, got {args:?}");
    let [x, y] = words.as_slice() else {
        return Err(want());
    };
    let x = x.parse().map_err(|_| want())?;
    let y = y.parse().map_err(|_| want())?;
    Ok((x, y))
}

/// The `N` quoted or bare words of `args`.
fn texts<const N: usize>(args: &str) -> Result<[String; N], String> {
    let mut out = Vec::new();
    let mut rest = args.trim();
    while !rest.is_empty() {
        let (word, tail) =
            next_word(rest).ok_or_else(|| format!("bad text in {args:?}"))?;
        out.push(word);
        rest = tail.trim();
    }
    out.try_into()
        .map_err(|_| format!("want {N} texts, got {args:?}"))
}

/// The first word of `rest`, and what follows it.
fn next_word(rest: &str) -> Option<(String, &str)> {
    if !rest.starts_with('"') {
        let (word, tail) = rest.split_once(' ').unwrap_or((rest, ""));
        return Some((word.to_owned(), tail));
    }
    let mut escaped = false;
    for (at, c) in rest.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => {
                let (quoted, tail) = rest.split_at(at + 1);
                return Some((unquote(quoted)?, tail));
            }
            _ => {}
        }
    }
    None
}
