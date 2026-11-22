// SPDX-License-Identifier: AGPL-3.0-or-later

//! Comparing what the map recorded with the sources now.

use std::collections::HashMap;
use std::fmt;

use crate::map::Check;
use crate::remote::{Refs, Remote};

/// The files the wiring workflow writes in the map's repository. Commits
/// that only change these (merging the map's own pull request) don't
/// change what the map shows.
const MAP_FILES: [&str; 4] = ["wiring.json", "wiring.mmd", "wiring.svg", "wiring-dark.svg"];

/// Something that moved since the map was generated.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Head {
        repo: String,
        old: String,
        new: String,
    },
    Tip {
        repo: String,
        commit: String,
    },
    Release {
        group: String,
        artifacts: Vec<String>,
        old: Option<String>,
        new: String,
    },
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Change::Head { repo, old, new } => {
                write!(
                    f,
                    "{repo}: default branch moved from {} to {}",
                    short(old),
                    short(new)
                )
            }
            Change::Tip { repo, commit } => {
                write!(f, "{repo}: {} is no longer a branch tip", short(commit))
            }
            Change::Release {
                group,
                artifacts,
                old,
                new,
            } => {
                let old = old.as_deref().unwrap_or("none");
                write!(f, "{group}:{}: release {old} -> {new}", artifacts.join(","))
            }
        }
    }
}

fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// Compares each check with `remote`; `own` is the map's own repository.
/// A source that can't be read is reported on stderr and skipped: the
/// daily build still covers it.
pub struct Checker<'a, R: Remote> {
    remote: &'a mut R,
    own: &'a str,
    refs: HashMap<String, Option<Refs>>,
}

impl<'a, R: Remote> Checker<'a, R> {
    pub fn new(remote: &'a mut R, own: &'a str) -> Self {
        Checker {
            remote,
            own,
            refs: HashMap::new(),
        }
    }

    pub fn changes(&mut self, checks: &[Check]) -> Vec<Change> {
        let mut changes = Vec::new();
        for check in checks {
            let change = match check {
                Check::Head { repo, commit } => self.head(repo, commit),
                Check::Tip { repo, commit } => self.tip(repo, commit),
                Check::Release {
                    group,
                    artifacts,
                    version,
                } => self.release(group, artifacts, version.as_deref()),
            };
            changes.extend(change);
        }
        changes
    }

    /// A repository's branches, read once.
    fn refs(&mut self, repo: &str) -> Option<&Refs> {
        if !self.refs.contains_key(repo) {
            let refs = self.remote.refs(repo).map_err(warn).ok();
            self.refs.insert(repo.to_owned(), refs);
        }
        self.refs.get(repo)?.as_ref()
    }

    fn head(&mut self, repo: &str, old: &str) -> Option<Change> {
        let new = self.refs(repo)?.head.clone()?;
        if new == old {
            return None;
        }

        // The map's own repository moves with every merged map
        if repo == self.own && self.only_map(repo, old, &new) {
            return None;
        }
        Some(Change::Head {
            repo: repo.to_owned(),
            old: old.to_owned(),
            new,
        })
    }

    /// Whether `base..head` of the map's repository only touches the map's
    /// files. When GitHub can't tell, it counts as a change.
    fn only_map(&mut self, repo: &str, base: &str, head: &str) -> bool {
        match self.remote.files(repo, base, head) {
            Ok(files) => files.iter().all(|f| MAP_FILES.contains(&f.as_str())),
            Err(e) => {
                warn(e);
                false
            }
        }
    }

    fn tip(&mut self, repo: &str, commit: &str) -> Option<Change> {
        let refs = self.refs(repo)?;
        if refs.tips.iter().chain(&refs.head).any(|t| t == commit) {
            return None;
        }
        Some(Change::Tip {
            repo: repo.to_owned(),
            commit: commit.to_owned(),
        })
    }

    /// The map recorded the newest release of one of `artifacts`. It has
    /// moved when no artifact's newest release is that version any more,
    /// or, when there was none, once every artifact has one.
    fn release(&mut self, group: &str, artifacts: &[String], old: Option<&str>) -> Option<Change> {
        let mut newest = Vec::new();
        for artifact in artifacts {
            match self.remote.release(group, artifact) {
                Ok(version) => newest.push(version),
                Err(e) => {
                    warn(e);
                    return None;
                }
            }
        }

        let moved = match old {
            Some(old) => !newest.iter().flatten().any(|v| v == old),
            None => newest.iter().all(Option::is_some),
        };
        let new = newest.into_iter().flatten().next()?;
        if !moved {
            return None;
        }
        Some(Change::Release {
            group: group.to_owned(),
            artifacts: artifacts.to_vec(),
            old: old.map(str::to_owned),
            new,
        })
    }
}

fn warn(message: String) {
    eprintln!("wiring-check: skipped: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: &str = "AnkiQuest/AnkiQuest.github.io";

    /// Sources as fixed data; a missing entry fails like a network error.
    #[derive(Default)]
    struct Fake {
        refs: HashMap<&'static str, (&'static str, Vec<&'static str>)>,
        releases: HashMap<&'static str, Option<&'static str>>,
        files: Option<Vec<&'static str>>,
    }

    impl Remote for Fake {
        fn refs(&mut self, repo: &str) -> Result<Refs, String> {
            let (head, tips) = self.refs.get(repo).ok_or("unreadable")?;
            Ok(Refs {
                head: Some(head.to_string()),
                tips: tips.iter().map(|t| t.to_string()).collect(),
            })
        }

        fn release(&mut self, _group: &str, artifact: &str) -> Result<Option<String>, String> {
            let found = self.releases.get(artifact).ok_or("no answer")?;
            Ok(found.map(str::to_owned))
        }

        fn files(&mut self, _repo: &str, _base: &str, _head: &str) -> Result<Vec<String>, String> {
            let files = self.files.as_ref().ok_or("no answer")?;
            Ok(files.iter().map(|f| f.to_string()).collect())
        }
    }

    fn s(value: &str) -> String {
        value.to_owned()
    }

    fn head(repo: &str, commit: &str) -> Check {
        Check::Head {
            repo: s(repo),
            commit: s(commit),
        }
    }

    fn release(artifacts: &[&str], version: Option<&str>) -> Check {
        Check::Release {
            group: s("org.ankiquest"),
            artifacts: artifacts.iter().map(|a| s(a)).collect(),
            version: version.map(s),
        }
    }

    fn changes(fake: &mut Fake, checks: &[Check]) -> Vec<Change> {
        Checker::new(fake, OWN).changes(checks)
    }

    #[test]
    fn head_moved() {
        let mut fake = Fake::default();
        fake.refs
            .insert("AnkiQuest/AnkiQuest-Dev", ("bbb", vec!["bbb"]));
        let found = changes(&mut fake, &[head("AnkiQuest/AnkiQuest-Dev", "aaa")]);
        assert_eq!(
            found,
            [Change::Head {
                repo: s("AnkiQuest/AnkiQuest-Dev"),
                old: s("aaa"),
                new: s("bbb")
            }]
        );
        assert_eq!(
            found[0].to_string(),
            "AnkiQuest/AnkiQuest-Dev: default branch moved from aaa to bbb"
        );
    }

    #[test]
    fn head_current() {
        let mut fake = Fake::default();
        fake.refs
            .insert("AnkiQuest/AnkiQuest-Dev", ("aaa", vec!["aaa"]));
        assert_eq!(
            changes(&mut fake, &[head("AnkiQuest/AnkiQuest-Dev", "aaa")]),
            []
        );
    }

    #[test]
    fn unreadable_is_skipped() {
        let mut fake = Fake::default();
        assert_eq!(
            changes(&mut fake, &[head("AnkiQuest/AnkiQuest-Art", "aaa")]),
            []
        );
    }

    #[test]
    fn own_map_merge_is_no_change() {
        let mut fake = Fake::default();
        fake.refs.insert(OWN, ("bbb", vec!["bbb"]));
        fake.files = Some(vec!["wiring.json", "wiring.svg"]);
        assert_eq!(changes(&mut fake, &[head(OWN, "aaa")]), []);

        fake.files = Some(vec!["wiring.json", ".gitmodules"]);
        assert_eq!(changes(&mut fake, &[head(OWN, "aaa")]).len(), 1);

        // GitHub can't compare (a force-push): rebuild
        fake.files = None;
        assert_eq!(changes(&mut fake, &[head(OWN, "aaa")]).len(), 1);
    }

    #[test]
    fn tips() {
        let mut fake = Fake::default();
        fake.refs
            .insert("ankitects/anki", ("aaa", vec!["aaa", "bbb"]));
        let tip = |commit: &str| Check::Tip {
            repo: s("ankitects/anki"),
            commit: s(commit),
        };
        assert_eq!(changes(&mut fake, &[tip("bbb")]), []);
        assert_eq!(
            changes(&mut fake, &[tip("ccc")]),
            [Change::Tip {
                repo: s("ankitects/anki"),
                commit: s("ccc")
            }]
        );
    }

    #[test]
    fn releases() {
        let mut fake = Fake::default();
        fake.releases.insert("core", Some("1.1"));
        fake.releases.insert("android", Some("1.0"));
        fake.releases.insert("testing", None);

        // The recorded version is still one artifact's newest
        assert_eq!(
            changes(&mut fake, &[release(&["core", "android"], Some("1.0"))]),
            []
        );

        // None has it any more
        assert_eq!(
            changes(&mut fake, &[release(&["core"], Some("1.0"))]),
            [Change::Release {
                group: s("org.ankiquest"),
                artifacts: vec![s("core")],
                old: Some(s("1.0")),
                new: s("1.1")
            }]
        );

        // Unpublished then: changed once every artifact is published
        assert_eq!(
            changes(&mut fake, &[release(&["core", "testing"], None)]),
            []
        );
        assert_eq!(
            changes(&mut fake, &[release(&["core", "android"], None)]).len(),
            1
        );
        assert_eq!(changes(&mut fake, &[release(&["testing"], None)]), []);

        // Maven Central didn't answer
        assert_eq!(changes(&mut fake, &[release(&["gone"], Some("1.0"))]), []);
    }
}
