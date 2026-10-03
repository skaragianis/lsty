//! `cmpjson`: JSON compared by value, with placeholders in strings.

use serde_json::Value;

use crate::diff;
use crate::placeholder::{Bindings, match_text};

/// Compares `got` with `want`. On a match, returns the ids bound for the
/// first time. On a mismatch, returns the reason and a diff.
pub(crate) fn compare(
    want: &str,
    got: &str,
    work: &str,
    bound: &Bindings,
) -> Result<Bindings, String> {
    let want: Value = serde_json::from_str(want)
        .map_err(|e| format!("want is not JSON: {e}"))?;
    let got: Value = serde_json::from_str(got)
        .map_err(|e| format!("got is not JSON: {e}"))?;

    let mut matcher = Matcher {
        work,
        bound,
        found: Bindings::new(),
    };
    if matcher.matches(&want, &got) {
        return Ok(matcher.found);
    }
    Err(format!(
        "JSON values differ\n{}",
        diff::unified("want", &pretty(&want), "got", &pretty(&got))
    ))
}

fn pretty(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| value.to_string());
    text.push('\n');
    text
}

struct Matcher<'a> {
    work: &'a str,
    bound: &'a Bindings,
    found: Bindings,
}

impl Matcher<'_> {
    fn matches(&mut self, want: &Value, got: &Value) -> bool {
        match (want, got) {
            (Value::String(w), _) => self.matches_string(w, got),
            (Value::Object(w), Value::Object(g)) => {
                w.len() == g.len()
                    && w.iter().all(|(key, w)| {
                        g.get(key).is_some_and(|g| self.matches(w, g))
                    })
            }
            (Value::Array(w), Value::Array(g)) => {
                w.len() == g.len()
                    && w.iter().zip(g).all(|(w, g)| self.matches(w, g))
            }
            // Numbers compare as floats: 1 equals 1.0.
            (Value::Number(w), Value::Number(g)) => w.as_f64() == g.as_f64(),
            (Value::Null, Value::Null) => true,
            (Value::Bool(w), Value::Bool(g)) => w == g,
            _ => false,
        }
    }

    fn matches_string(&mut self, want: &str, got: &Value) -> bool {
        let want = want.replace("$WORK", self.work);
        if want == "<any>" {
            return true;
        }
        let Value::String(got) = got else {
            return false;
        };
        let mut all = self.bound.clone();
        all.extend(self.found.clone());
        match match_text(&want, got, &all) {
            Some(found) => {
                self.found.extend(found);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_key_order_and_whitespace() {
        let got = r#"{"b":[1,2],"a":{"x":null,"y":true},"c":"text"}"#;
        let want = "{\n \"a\": {\"y\": true, \"x\": null},\n \"c\": \"text\", \"b\": [1, 2]}";
        assert_eq!(
            compare(want, got, "/w", &Bindings::new()),
            Ok(Bindings::new())
        );
    }
    #[test]
    fn differing_value_fails_with_a_diff() {
        let err =
            compare(r#"{"a": 1}"#, r#"{"a": 2}"#, "/w", &Bindings::new())
                .unwrap_err();
        assert!(err.contains("-  \"a\": 1"), "{err}");
        assert!(err.contains("+  \"a\": 2"), "{err}");
    }
    #[test]
    fn work_and_placeholders() {
        let got = r#"{"file":"/w/todo.txt","id":"k3f9","nr":7,"more":[{"id":"k3f9"}]}"#;
        let want = r#"{"file":"$WORK/todo.txt","id":"<id1>","nr":"<any>","more":[{"id":"<id1>"}]}"#;
        let found = compare(want, got, "/w", &Bindings::new()).unwrap();
        assert_eq!(found.get("id1").map(String::as_str), Some("k3f9"));

        let got = r#"{"id":"k3f9","more":[{"id":"a1b2"}]}"#;
        let want = r#"{"id":"<id1>","more":[{"id":"<id1>"}]}"#;
        assert!(compare(want, got, "/w", &Bindings::new()).is_err());
    }

    #[test]
    fn integers_equal_floats() {
        assert!(compare("1.0", "1", "/w", &Bindings::new()).is_ok());
    }
}
