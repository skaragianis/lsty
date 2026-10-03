//! Splitting a script line into words, as testscript does.

/// Splits `line` into words. Single quotes quote, `''` inside quotes is one
/// `'`, an unquoted `#` starts a comment, and `expand` is applied to every
/// unquoted chunk.
pub(crate) fn words(
    line: &str,
    expand: impl Fn(&str) -> String,
) -> Result<Vec<String>, String> {
    let bytes = line.as_bytes();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut start: Option<usize> = None;
    let mut quoted = false;
    let mut i = 0;

    loop {
        let at_end = i >= bytes.len();
        if !quoted
            && (at_end || matches!(bytes[i], b' ' | b'\t' | b'\r' | b'#'))
        {
            if let Some(s) = start.take() {
                word.push_str(&expand(&line[s..i]));
                words.push(std::mem::take(&mut word));
            }
            if at_end || bytes[i] == b'#' {
                return Ok(words);
            }
            i += 1;
            continue;
        }
        if at_end {
            return Err("unterminated quoted argument".to_owned());
        }
        if bytes[i] == b'\'' {
            // `''` inside quotes: the second quote starts the next chunk,
            // so it is kept as text.
            let doubled = quoted && bytes.get(i + 1) == Some(&b'\'');
            let chunk = &line[start.unwrap_or(i)..i];
            if quoted {
                word.push_str(chunk);
            } else if start.is_some() {
                word.push_str(&expand(chunk));
            }
            start = Some(i + 1);
            quoted = doubled || !quoted;
            i += if doubled { 2 } else { 1 };
            continue;
        }
        if start.is_none() {
            start = Some(i);
        }
        i += 1;
    }
}

/// Expands `$NAME` and `${NAME}`, and `$` followed by one of `*#$@!?-` or
/// a digit. A `$` that starts no name stays as it is. A name that
/// ends in `@R` expands to the value quoted for a regular expression.
pub(crate) fn expand(text: &str, getenv: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if after.is_empty() {
            out.push('$');
            return out;
        }
        let (name, width) = shell_name(after);
        if name.is_empty() {
            if width == 0 {
                out.push('$');
            }
        } else if let Some(plain) = name.strip_suffix("@R") {
            out.push_str(&regex::escape(&getenv(plain)));
        } else {
            out.push_str(&getenv(name));
        }
        rest = &after[width..];
    }
    out.push_str(rest);
    out
}

/// The variable name at the start of `s` and how many bytes it takes.
fn shell_name(s: &str) -> (&str, usize) {
    let bytes = s.as_bytes();
    if bytes[0] == b'{' {
        if bytes.len() > 2 && is_special(bytes[1]) && bytes[2] == b'}' {
            return (&s[1..2], 3);
        }
        return match s[1..].find('}') {
            Some(0) => ("", 2),
            Some(end) => (&s[1..=end], end + 2),
            None => ("", 1),
        };
    }
    if is_special(bytes[0]) {
        return (&s[..1], 1);
    }
    let end = s
        .find(|c: char| !(c == '_' || c.is_ascii_alphanumeric()))
        .unwrap_or(s.len());
    (&s[..end], end)
}

fn is_special(b: u8) -> bool {
    matches!(
        b,
        b'*' | b'#' | b'$' | b'@' | b'!' | b'?' | b'-' | b'0'..=b'9'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> String {
        match name {
            "WORK" => "/w".to_owned(),
            "/" => "/".to_owned(),
            "DOT" => "a.b".to_owned(),
            _ => String::new(),
        }
    }

    fn split(line: &str) -> Vec<String> {
        words(line, |s| expand(s, env)).unwrap()
    }

    #[test]
    fn splits_and_quotes() {
        assert_eq!(split("exec lsty  ls\t-x"), ["exec", "lsty", "ls", "-x"]);
        assert_eq!(split("stdout '^a b$'"), ["stdout", "^a b$"]);
        assert_eq!(split("x 'it''s' a'b c'd"), ["x", "it's", "ab cd"]);
        assert_eq!(split("x '' y"), ["x", "", "y"]);
        assert_eq!(split("x # comment"), ["x"]);
        assert_eq!(split("x '#' y"), ["x", "#", "y"]);
        assert_eq!(split("# only"), Vec::<String>::new());
        assert!(words("x 'open", str::to_owned).is_err());
    }

    #[test]
    fn expands_outside_quotes_only() {
        assert_eq!(
            split("cp $WORK/a ${WORK}b '$WORK'"),
            ["cp", "/w/a", "/wb", "$WORK"]
        );
        assert_eq!(split("x ${/}y $NOPE. $"), ["x", "/y", ".", "$"]);
        assert_eq!(split("x ${DOT@R}"), ["x", r"a\.b"]);
        assert_eq!(
            split(r#"env J={"f":"$WORK/t"}"#),
            ["env", r#"J={"f":"/w/t"}"#]
        );
    }
}
