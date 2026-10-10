use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use super::File;

fn interop_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/interop")
}

/// Every `.txt` file under `testdata/interop/<tool>/`, in name order.
fn fixtures() -> io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for tool in fs::read_dir(interop_dir())? {
        let tool = tool?.path();
        if !tool.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&tool)? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "txt") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The fields of every line as the golden files write them.
fn golden_of(file: &File) -> Value {
    let lines: Vec<Value> = file
        .lines
        .iter()
        .map(|line| {
            let tokens: Vec<Value> = line
                .tokens()
                .map(|t| {
                    let mut obj = json!({
                        "kind": format!("{:?}", t.kind),
                        "text": lossy(t.text),
                        "start": t.start,
                    });
                    if !t.key.is_empty() {
                        obj["key"] = lossy(t.key).into();
                    }
                    if !t.value.is_empty() {
                        obj["value"] = lossy(t.value).into();
                    }
                    obj
                })
                .collect();
            json!({
                "raw": lossy(line.raw()),
                "eol": lossy(line.eol.as_bytes()),
                "blank": line.blank,
                "done": line.is_done(),
                "priority": line
                    .priority()
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
                "completed": line.completed().unwrap_or_default(),
                "created": line.created().unwrap_or_default(),
                "description": lossy(line.description()),
                "tokens": tokens,
            })
        })
        .collect();
    json!({ "bom": file.bom, "lines": lines })
}

fn name(path: &Path) -> String {
    let dir = path.parent().and_then(Path::file_name);
    let file = path.file_name();
    format!(
        "{}/{}",
        dir.unwrap_or_default().to_string_lossy(),
        file.unwrap_or_default().to_string_lossy()
    )
}

#[test]
fn fixtures_exist() -> io::Result<()> {
    assert!(fixtures()?.len() >= 9);
    Ok(())
}

#[test]
fn round_trip() -> io::Result<()> {
    // 01.24
    for path in fixtures()? {
        let data = fs::read(&path)?;
        assert_eq!(File::parse(&data).to_bytes(), data, "{}", name(&path));
    }
    Ok(())
}

#[test]
fn golden_fields() -> io::Result<()> {
    // 01.25
    for path in fixtures()? {
        let data = fs::read(&path)?;
        let want = fs::read_to_string(path.with_extension("golden.json"))?;
        let want: Value = serde_json::from_str(&want)?;
        let got = golden_of(&File::parse(&data));
        assert_eq!(got, want, "{}", name(&path));
    }
    Ok(())
}
