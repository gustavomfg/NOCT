//! The artifacts an assistant reply carries, read from its metadata. Port of the `saveAssistant` handler in
//! `electron/ipc/registerAiIpc.ts` and of `artifactType` in `electron/ipc/fileAccess.ts`.

use serde_json::Value;

/// An artifact derived from a reply's metadata, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnArtifact {
    pub kind: &'static str,
    pub title: String,
    pub file_path: Option<String>,
    pub content: Option<String>,
}

/// The artifacts of a reply. Each entry of the metadata's `files` list that is an object with a non-empty `path` gives a
/// file artifact; a non-empty `diff` gives a report. A metadata that is not an object gives none, and the other keys are
/// not read.
pub fn turn_artifacts(metadata: Option<&Value>) -> Vec<TurnArtifact> {
    let Some(Value::Object(fields)) = metadata else {
        return Vec::new();
    };
    let mut artifacts = Vec::new();
    if let Some(Value::Array(files)) = fields.get("files") {
        for file in files {
            let Value::Object(file) = file else {
                continue;
            };
            let path = match file.get("path") {
                Some(Value::String(path)) => path.as_str(),
                _ => "",
            };
            if path.is_empty() {
                continue;
            }
            artifacts.push(TurnArtifact {
                kind: artifact_kind(path),
                title: basename(path).to_string(),
                file_path: Some(path.to_string()),
                content: None,
            });
        }
    }
    if let Some(Value::String(diff)) = fields.get("diff")
        && !diff.is_empty()
    {
        artifacts.push(TurnArtifact {
            kind: "report",
            title: "Alterações do turno".to_string(),
            file_path: None,
            content: Some(diff.clone()),
        });
    }
    artifacts
}

/// `artifactType`: the kind of a file artifact, from the extension of its path, compared without case.
pub fn artifact_kind(file_path: &str) -> &'static str {
    match extension_of(basename(file_path)).to_lowercase().as_str() {
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" | ".svg" => "image",
        ".md" => "markdown",
        ".json" | ".yaml" | ".yml" | ".toml" | ".env" | ".ini" => "configuration",
        ".docx" | ".pdf" | ".html" => "document",
        _ => "code",
    }
}

/// `path.basename`: the last part of a path, without trailing separators. The separator is `/`, and `\` as well on
/// Windows, as in Node's `path` module of that system. A Windows drive prefix is not removed.
pub fn basename(path: &str) -> &str {
    let separators: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
    let trimmed = path.trim_end_matches(separators);
    match trimmed.rfind(separators) {
        Some(index) => &trimmed[index + 1..],
        None => trimmed,
    }
}

/// `path.extname` of a basename: the part from its last dot, unless that dot is the first character.
fn extension_of(base: &str) -> &str {
    match base.rfind('.') {
        Some(index) if index > 0 => &base[index..],
        _ => "",
    }
}
