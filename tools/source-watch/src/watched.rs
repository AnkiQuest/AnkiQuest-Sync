// SPDX-License-Identifier: AGPL-3.0-or-later

//! watched.json: the sources the watch job reports on.

use serde::Deserialize;

/// One watched source and the fork its changes are ported to.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub source: String,
    #[serde(rename = "ref")]
    pub rev: String,
    #[serde(flatten)]
    pub kind: Kind,
    pub target: String,
    pub why: String,
    pub reviewed: String,
}

/// What is compared with `reviewed`: the branch's commit, or a version
/// read from a `key=value` line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Kind {
    Branch,
    Version { file: String, key: String },
}

impl Entry {
    /// How the summary and errors name the entry.
    pub fn label(&self) -> String {
        format!("{} {}", self.source, self.rev)
    }
}

pub fn parse(json: &str) -> Result<Vec<Entry>, String> {
    serde_json::from_str(json).map_err(|e| format!("watched.json: {e}"))
}

/// The value of the first `key=` line in a properties file.
pub fn property(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_kinds() {
        let json = r#"[
          {"source": "a/b", "ref": "main", "kind": "branch", "target": "o/t", "why": "w", "reviewed": "abc"},
          {"source": "c/d", "ref": "main", "kind": "version", "file": "gradle.properties",
           "key": "VERSION_NAME", "target": "o/t", "why": "w", "reviewed": "1.0"}
        ]"#;
        let entries = parse(json).unwrap();
        assert_eq!(entries[0].kind, Kind::Branch);
        assert_eq!(
            entries[1].kind,
            Kind::Version {
                file: "gradle.properties".into(),
                key: "VERSION_NAME".into()
            }
        );
        assert_eq!(entries[1].label(), "c/d main");
    }

    #[test]
    fn reads_the_first_property() {
        let text = "# c\nVERSION_NAME_OLD=0\nVERSION_NAME=0.1.78 \nVERSION_NAME=2\n";
        assert_eq!(property(text, "VERSION_NAME").as_deref(), Some("0.1.78"));
        assert_eq!(property("VERSION_NAME=\n", "VERSION_NAME"), None);
        assert_eq!(property("", "VERSION_NAME"), None);
    }

    #[test]
    fn reads_the_committed_file() {
        let text = include_str!("../../../watched.json");
        assert!(!parse(text).unwrap().is_empty());
    }
}
