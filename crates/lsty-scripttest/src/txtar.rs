//! The txtar archive format: a comment, then `-- name --` sections.

/// A parsed archive.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Archive {
    /// Everything before the first section: the script.
    pub(crate) comment: String,
    /// The sections, in order: name and contents.
    pub(crate) files: Vec<(String, String)>,
}

impl Archive {
    pub(crate) fn parse(text: &str) -> Self {
        let mut archive = Self::default();
        let mut current: Option<(String, String)> = None;

        for line in text.split_inclusive('\n') {
            if let Some(name) = marker(line) {
                archive.push(current.take());
                current = Some((name.to_owned(), String::new()));
                continue;
            }
            match &mut current {
                Some((_, data)) => data.push_str(line),
                None => archive.comment.push_str(line),
            }
        }
        archive.push(current);
        archive
    }

    fn push(&mut self, file: Option<(String, String)>) {
        if let Some((name, mut data)) = file {
            if !data.is_empty() && !data.ends_with('\n') {
                data.push('\n');
            }
            self.files.push((name, data));
        }
    }

    pub(crate) fn format(&self) -> String {
        let mut out = self.comment.clone();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        for (name, data) in &self.files {
            out.push_str("-- ");
            out.push_str(name);
            out.push_str(" --\n");
            out.push_str(data);
            if !data.is_empty() && !data.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }
}

/// The name of a `-- name --` line.
fn marker(line: &str) -> Option<&str> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let name = line.strip_prefix("-- ")?.strip_suffix(" --")?.trim();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format() {
        let text = "exec lsty 0\n\n-- a.txt --\none\n-- b/c --\n-- d --\nlast";
        let archive = Archive::parse(text);
        assert_eq!(archive.comment, "exec lsty 0\n\n");
        assert_eq!(
            archive.files,
            [
                ("a.txt".to_owned(), "one\n".to_owned()),
                ("b/c".to_owned(), String::new()),
                ("d".to_owned(), "last\n".to_owned()),
            ]
        );
        assert_eq!(archive.format(), format!("{text}\n"));
    }

    #[test]
    fn marker_needs_both_dashes() {
        assert_eq!(marker("-- x --\n"), Some("x"));
        assert_eq!(marker("-- x\n"), None);
        assert_eq!(marker("--  --\n"), None);
    }
}
