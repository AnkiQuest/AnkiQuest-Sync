// SPDX-License-Identifier: AGPL-3.0-or-later

//! The forks of forks.json, and the upstream guidance a fork's branch has
//! merged.

use serde::Deserialize;

use crate::github::Remote;
use crate::guidance::{self, Guidance};

/// One entry of forks.json (shared with the sync job's matrix).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Fork {
    pub fork: String,
    pub upstream: String,
    pub upstream_branch: String,
}

pub fn parse(json: &str) -> Result<Vec<Fork>, String> {
    serde_json::from_str(json).map_err(|e| format!("forks.json: {e}"))
}

/// Upstream's guidance at the newest upstream commit `branch` of `repo`
/// has merged, the text the sync job wrote when it merged that commit.
/// None when `repo` is not a fork.
pub fn merged(
    remote: &impl Remote,
    forks: &[Fork],
    repo: &str,
    branch: &str,
) -> Result<Option<Guidance>, String> {
    let Some(fork) = forks.iter().find(|f| f.fork == repo) else {
        return Ok(None);
    };
    let base = remote.merge_base(repo, branch, &fork.upstream, &fork.upstream_branch)?;

    // The commit is in the fork's history, so the fork serves its files
    let mut texts = Vec::new();
    for name in guidance::FILES {
        if let Some(text) = remote.root_file(repo, &base, name)? {
            texts.push((name, text));
        }
    }
    Ok(Some(Guidance::new(&fork.upstream, &texts)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    const FORKS: &str = r#"[
      { "fork": "AnkiQuest/AnkiQuest-Android", "upstream": "ankidroid/Anki-Android", "upstream_branch": "main" }
    ]"#;
    const BASE: &str = "abc123";

    /// Files at BASE, and the calls made.
    struct Fake {
        files: BTreeMap<&'static str, &'static str>,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(files: &[(&'static str, &'static str)]) -> Fake {
            Fake {
                files: files.iter().copied().collect(),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Remote for Fake {
        fn merge_base(
            &self,
            fork: &str,
            branch: &str,
            upstream: &str,
            upstream_branch: &str,
        ) -> Result<String, String> {
            self.calls.borrow_mut().push(format!(
                "compare {fork} {branch} {upstream} {upstream_branch}"
            ));
            Ok(BASE.to_owned())
        }

        fn root_file(
            &self,
            repo: &str,
            commit: &str,
            name: &str,
        ) -> Result<Option<String>, String> {
            self.calls
                .borrow_mut()
                .push(format!("file {repo} {commit} {name}"));
            Ok(self.files.get(name).map(|t| (*t).to_owned()))
        }
    }

    fn forks() -> Vec<Fork> {
        parse(FORKS).unwrap()
    }

    #[test]
    fn not_a_fork() {
        let fake = Fake::new(&[]);
        let g = merged(&fake, &forks(), "AnkiQuest/AnkiQuest-Server", "master").unwrap();
        assert_eq!(g, None);
        assert!(fake.calls.borrow().is_empty());
    }

    #[test]
    fn reads_the_merge_base() {
        let fake = Fake::new(&[("CLAUDE.md", "@AGENTS.md\n"), ("AGENTS.md", "- Rules.\n")]);
        let g = merged(&fake, &forks(), "AnkiQuest/AnkiQuest-Android", "master")
            .unwrap()
            .unwrap();
        assert_eq!(
            *fake.calls.borrow(),
            [
                "compare AnkiQuest/AnkiQuest-Android master ankidroid/Anki-Android main",
                "file AnkiQuest/AnkiQuest-Android abc123 CLAUDE.md",
                "file AnkiQuest/AnkiQuest-Android abc123 AGENTS.md",
            ]
        );

        // The same guidance the sync job reads from a checkout
        let texts = [
            ("CLAUDE.md", "@AGENTS.md\n".to_owned()),
            ("AGENTS.md", "- Rules.\n".to_owned()),
        ];
        assert_eq!(g, Guidance::new("ankidroid/Anki-Android", &texts));
    }

    #[test]
    fn upstream_without_guidance() {
        let fake = Fake::new(&[]);
        let g = merged(&fake, &forks(), "AnkiQuest/AnkiQuest-Android", "master")
            .unwrap()
            .unwrap();
        assert_eq!(g.section("intro"), "");
    }

    #[test]
    fn bad_json() {
        assert!(parse("{}").unwrap_err().starts_with("forks.json: "));
    }
}
