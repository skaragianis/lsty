//! The escape sequence that puts text on the system clipboard.

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The OSC 52 sequence that sets the system clipboard to `text`.
pub fn osc52(text: &[u8]) -> Vec<u8> {
    let mut out = b"\x1b]52;c;".to_vec();
    for chunk in text.chunks(3) {
        out.extend_from_slice(&encode(chunk));
    }
    out.push(0x07);
    out
}

/// One to three bytes as four characters, padded with `=`.
fn encode(chunk: &[u8]) -> [u8; 4] {
    let get = |i: usize| u32::from(chunk.get(i).copied().unwrap_or(0));
    let n = (get(0) << 16) | (get(1) << 8) | get(2);
    let sextet = |shift: u32| {
        let at = usize::try_from((n >> shift) & 63).unwrap_or(0);
        ALPHABET.get(at).copied().unwrap_or(b'=')
    };
    [
        sextet(18),
        sextet(12),
        if chunk.len() > 1 { sextet(6) } else { b'=' },
        if chunk.len() > 2 { sextet(0) } else { b'=' },
    ]
}
