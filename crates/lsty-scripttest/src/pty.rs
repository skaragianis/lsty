//! A pseudo-terminal for `ttyin -stdin`.

use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::thread::{self, JoinHandle};

use rustix::fs::{Mode, OFlags};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};

/// End of transmission: ends the input as testscript does.
const EOT: u8 = 4;

/// The controlling side of a pty whose terminal side is a child's stdin.
pub(crate) struct Pty {
    writer: JoinHandle<()>,
    reader: JoinHandle<Vec<u8>>,
}

impl Pty {
    /// Opens a pty, feeds it `input` and returns it with the terminal side
    /// to pass to the child.
    pub(crate) fn open(input: Vec<u8>) -> io::Result<(Self, OwnedFd)> {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        let name = ptsname(&master, Vec::new())?;
        let tty = rustix::fs::open(
            name.as_c_str(),
            OFlags::RDWR | OFlags::NOCTTY,
            Mode::empty(),
        )?;

        let mut out = File::from(master);
        let mut input_side = out.try_clone()?;
        let writer = thread::spawn(move || {
            // The child may exit without reading; that is not an error.
            let fed = input_side
                .write_all(&input)
                .and_then(|()| input_side.write_all(&[EOT]));
            drop(fed);
        });
        let reader = thread::spawn(move || {
            let mut echoed = Vec::new();
            // Reading ends with EIO once the terminal side is closed.
            let read = out.read_to_end(&mut echoed);
            drop(read);
            echoed
        });

        Ok((Self { writer, reader }, tty))
    }

    /// Waits for the feeding to end, once every copy of the terminal side
    /// is closed, and returns what the terminal echoed.
    pub(crate) fn finish(self) -> Vec<u8> {
        let fed = self.writer.join();
        drop(fed);
        self.reader.join().unwrap_or_default()
    }
}
