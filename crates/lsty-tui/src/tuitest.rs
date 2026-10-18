//! A headless driver for the golden-frame transcripts under
//! `testdata/tui/`.
//!
//! A transcript is a script and the files it needs, in the txtar format.
//! The model is driven directly through `Model::update`,
//! without a terminal. Commands that ask for a timer are held until
//! `tick`.
//!
//! | Command | Meaning |
//! | --- | --- |
//! | `size W H` | the terminal's size; 80 × 24 when not given |
//! | `now STAMP` | the clock; `2026-09-29T0900Z` when not given |
//! | `start [FILE]` | create the model on FILE (`todo.txt` when left out) |
//! | `keys K…` | press keys, each argument in key notation |
//! | `type TEXT` | press one key per character of TEXT |
//! | `frame NAME` | the screen as plain text must equal the section NAME |
//! | `fg NAME`, `bg NAME` | the foreground or background layer must equal the section NAME |
//! | `status PATTERN` | the status line must match the regular expression |
//! | `file PATH NAME` | the file must equal the section NAME |
//! | `tick DURATION` | move the clock forward and deliver the timers that are due |
//! | `write FILE SECTION` | replace FILE with the section, as another writer would (a temporary file, renamed), and send a watcher event |
//! | `remove FILE` | delete a file or a folder and send a watcher event |
//! | `event [error REASON]` | send a watcher event, or a watcher failure |
//! | `click X Y`, `shiftclick X Y`, `dblclick X Y`, `rightclick X Y` | click at the cell, counted from 0 |
//! | `clickon TEXT`, `shiftclickon TEXT`, `dblclickon TEXT`, `rightclickon TEXT` | click at the first cell of the first place the screen shows TEXT (quoted or one word) |
//! | `drag X1 Y1 X2 Y2`, `dragon TEXT1 TEXT2` | press, move one row at a time, release |
//! | `down X Y`, `move X Y`, `up X Y` | a press, a motion with the button held, a release, each alone |
//! | `wheel up\|down X Y [N]` | N notches of the wheel at the cell, 1 when left out |
//! | `focus gained`, `focus lost` | the terminal reports that it gained or lost focus |
//! | `lock hold`, `lock release` | hold the lock of the todo file's folder, or free it |
//! | `clipboard set NAME` | put the section NAME, without its final newline, on the fake system clipboard |
//! | `clipboard expect NAME` | what the model last wrote to the clipboard (with OSC 52) must equal the section NAME |
//! | `clipboard fail` | make reading the clipboard fail until the next `clipboard set` |
//! | `opened PATH`, `edited PATH` | the fake opener or the fake editor must have been started on PATH (`$WORK` is the work folder) and nothing else; `none` for nothing, and the record is cleared |
//! | `opener missing` | make the fake opener fail as if it were not installed |
//! | `editor exit N`, `editor hold`, `editor return` | the next editor ends with status N, or stays open until `return`; what comes meanwhile is delivered after it |
//! | `binary PATH`, `chmod MODE PATH`, `symlink NAME TARGET` | make a file that is not text, change a mode (octal), make a symbolic link |
//!
//! This feature is for the crate's own tests and is unstable.

mod legend;
mod run;
mod screen;
#[cfg(test)]
mod tests;

pub use run::script;
