// SPDX-License-Identifier: AGPL-3.0-or-later

//! watched.json: the sources the watch and sync jobs follow, and how each
//! one's changes reach its target.

use serde::Deserialize;

/// One watched source and the fork its changes reach.
#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub source: String,
    #[serde(rename = "ref")]
    pub rev: String,
    #[serde(flatten)]
    pub kind: Kind,
    #[serde(flatten)]
    pub mode: Mode,
    pub target: String,
    pub why: String,
}

/// What is followed: the branch's commit, or a version read from a
/// `key=value` line of a file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Kind {
    Branch,
    Version { file: String, key: String },
}

/// How the source's changes reach the target.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Mode {
    /// The source shares history with the target and is carried whole: the
    /// sync job merges it on a sync branch and opens a pull request. Its
    /// commits are reviewed once they are in the target's default branch.
    Merge,
    /// The target pins the source's commit in Cargo.toml (`package`): the
    /// watch job opens a pull request moving the pin. Reviewed is what the
    /// target's default branch pins.
    Pin { package: String },
    /// Changes need a person (porting by hand, or the release steps in
    /// `why`): the watch job opens an issue until `reviewed` is moved.
    Port { reviewed: String },
}

impl Entry {
    /// How the summary and errors name the entry.
    pub fn label(&self) -> String {
        format!("{} {}", self.source, self.rev)
    }

    /// The target's branch for this source's pull request:
    /// sync/<owner>/<repo>-<ref>.
    pub fn branch(&self) -> String {
        format!("sync/{}-{}", self.source, self.rev)
    }
}

pub fn parse(json: &str) -> Result<Vec<Entry>, String> {
    let entries: Vec<Entry> =
        serde_json::from_str(json).map_err(|e| format!("watched.json: {e}"))?;

    // A merge or a pin follows commits, never a version
    for entry in &entries {
        let follows_commits = matches!(entry.mode, Mode::Port { .. }) || entry.kind == Kind::Branch;
        if !follows_commits {
            return Err(format!(
                "watched.json: {} follows a version; only mode \"port\" can",
                entry.label()
            ));
        }
    }
    Ok(entries)
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
    fn parses_every_mode() {
        let json = r#"[
          {"source": "a/b", "ref": "main", "kind": "branch", "mode": "merge", "target": "o/t", "why": "w"},
          {"source": "e/f", "ref": "main", "kind": "branch", "mode": "pin", "package": "fsrs",
           "target": "o/t", "why": "w"},
          {"source": "c/d", "ref": "main", "kind": "version", "file": "gradle.properties",
           "key": "VERSION_NAME", "mode": "port", "target": "o/t", "why": "w", "reviewed": "1.0"}
        ]"#;
        let entries = parse(json).unwrap();
        assert_eq!(entries[0].kind, Kind::Branch);
        assert_eq!(entries[0].mode, Mode::Merge);
        assert_eq!(entries[0].branch(), "sync/a/b-main");
        assert_eq!(
            entries[1].mode,
            Mode::Pin {
                package: "fsrs".into()
            }
        );
        assert_eq!(
            entries[2].kind,
            Kind::Version {
                file: "gradle.properties".into(),
                key: "VERSION_NAME".into()
            }
        );
        assert_eq!(
            entries[2].mode,
            Mode::Port {
                reviewed: "1.0".into()
            }
        );
        assert_eq!(entries[2].label(), "c/d main");
    }

    #[test]
    fn rejects_a_mode_it_cannot_run() {
        let merge_version = r#"[{"source": "c/d", "ref": "main", "kind": "version", "file": "f",
          "key": "k", "mode": "merge", "target": "o/t", "why": "w"}]"#;
        assert_eq!(
            parse(merge_version).unwrap_err(),
            "watched.json: c/d main follows a version; only mode \"port\" can"
        );

        // A mode is required, and port needs `reviewed`
        let none =
            r#"[{"source": "a/b", "ref": "main", "kind": "branch", "target": "o/t", "why": "w"}]"#;
        assert!(parse(none).is_err());
        let port = r#"[{"source": "a/b", "ref": "main", "kind": "branch", "mode": "port",
          "target": "o/t", "why": "w"}]"#;
        assert!(parse(port).is_err());
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
