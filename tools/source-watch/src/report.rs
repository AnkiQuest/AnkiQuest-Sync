// SPDX-License-Identifier: AGPL-3.0-or-later

//! One issue per watched source: opened or updated while the source has
//! changes past `reviewed`, closed once it has none. A target with issues
//! disabled gets its issue in AnkiQuest-Sync instead, titled with the target.

use crate::github::{GitHub, Issue};
use crate::watched::{Entry, Kind, property};

/// How many of the new commits an issue lists.
const LISTED_COMMITS: usize = 50;
const SHORT_SHA: usize = 10;

/// Where an entry's issue lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Home {
    Target,
    Fallback,
}

/// What a run did, for the step summary and the annotations.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// One line per entry, in watched.json's order.
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
    /// Each failed entry's label and error.
    pub failures: Vec<(String, String)>,
}

/// Checks every entry; one entry's failure is recorded and the rest still run.
pub fn run(gh: &impl GitHub, fallback: &str, entries: &[Entry]) -> Summary {
    let watch = Watch { gh, fallback };
    let mut summary = Summary::default();
    for entry in entries {
        match watch.check(entry, &mut summary.warnings) {
            Ok(line) => summary.lines.push(line),
            Err(error) => {
                summary.lines.push(format!("{}: failed", entry.label()));
                summary.failures.push((entry.label(), error));
            }
        }
    }
    summary
}

struct Watch<'a, G> {
    gh: &'a G,
    fallback: &'a str,
}

impl<G: GitHub> Watch<'_, G> {
    /// Reports one entry and returns its summary line.
    fn check(&self, entry: &Entry, warnings: &mut Vec<String>) -> Result<String, String> {
        let current = self.current(entry)?;

        // The issue's home, and a fallback issue left from before the target
        // had issues enabled
        let home = match self.gh.has_issues(&entry.target)? {
            true => Home::Target,
            false => Home::Fallback,
        };
        let repo = self.repo(entry, home);
        let open = self.find(repo, &title(entry, home))?;
        let stale = match home {
            Home::Target => self.find(self.fallback, &title(entry, Home::Fallback))?,
            Home::Fallback => None,
        };

        // Reviewed: close what is left
        if current == entry.reviewed {
            let comment =
                format!("Reviewed up to {current} in AnkiQuest/AnkiQuest-Sync watched.json.");
            if let Some(number) = open {
                self.gh.close_issue(repo, number, &comment)?;
            }
            if let Some(number) = stale {
                self.gh.close_issue(self.fallback, number, &comment)?;
            }
            return Ok(format!("{}: nothing new", entry.label()));
        }

        // New: update the open issue or open one
        let body = self.body(entry, &current, home)?;
        let url = match open {
            Some(number) => {
                self.gh.edit_issue(repo, number, &body)?;
                format!("https://github.com/{repo}/issues/{number}")
            }
            None => self.gh.create_issue(repo, &title(entry, home), &body)?,
        };
        if let Some(number) = stale {
            let comment = format!("Moved to {url}.");
            self.gh.close_issue(self.fallback, number, &comment)?;
        }
        if home == Home::Fallback {
            warnings.push(format!(
                "{} has issues disabled; reported {} in {url}",
                entry.target,
                entry.label()
            ));
        }
        Ok(format!(
            "{}: new ({current}), reported in {url}",
            entry.label()
        ))
    }

    /// The source's commit, or its version.
    fn current(&self, entry: &Entry) -> Result<String, String> {
        let current = match &entry.kind {
            Kind::Branch => self.gh.commit(&entry.source, &entry.rev)?,
            Kind::Version { file, key } => {
                let text = self.gh.file(&entry.source, file, &entry.rev)?;
                property(&text, key).unwrap_or_default()
            }
        };
        if current.is_empty() {
            return Err(format!("could not read {}", entry.label()));
        }
        Ok(current)
    }

    fn repo<'r>(&'r self, entry: &'r Entry, home: Home) -> &'r str {
        match home {
            Home::Target => &entry.target,
            Home::Fallback => self.fallback,
        }
    }

    fn find(&self, repo: &str, title: &str) -> Result<Option<u64>, String> {
        let issues = self.gh.open_issues(repo)?;
        Ok(issues
            .iter()
            .find(|Issue { title: t, .. }| t == title)
            .map(|issue| issue.number))
    }

    fn body(&self, entry: &Entry, current: &str, home: Home) -> Result<String, String> {
        let (source, reviewed) = (&entry.source, &entry.reviewed);
        let details = match entry.kind {
            Kind::Version { .. } => {
                format!("Upstream version is now `{current}` (reviewed: `{reviewed}`).")
            }
            Kind::Branch => {
                let cmp = self.gh.compare(source, reviewed, current)?;
                let skip = cmp.commits.len().saturating_sub(LISTED_COMMITS);
                let list: Vec<String> = cmp.commits[skip..]
                    .iter()
                    .map(|c| format!("- {} {}", short(&c.sha), c.subject))
                    .collect();
                format!(
                    "{} new commits since `{}` (latest {LISTED_COMMITS} below): \
                     https://github.com/{source}/compare/{reviewed}...{current}\n\n{}",
                    cmp.total,
                    short(reviewed),
                    list.join("\n")
                )
            }
        };
        let filed_here = match home {
            Home::Target => String::new(),
            Home::Fallback => format!(
                "Filed in {} because {} has issues disabled.\n\n",
                self.fallback, entry.target
            ),
        };
        Ok(format!(
            "Source: https://github.com/{source} ({}). Why we watch it: {}.\n\n\
             {filed_here}{details}\n\n\
             Port what we need (or decide to skip it), then set `reviewed` for this source \
             in AnkiQuest/AnkiQuest-Sync `watched.json` to `{current}`. \
             This issue closes on the next run.\n\n\
             Opened by AnkiQuest/AnkiQuest-Sync.",
            entry.rev, entry.why
        ))
    }
}

/// The issue title; in the fallback repository it names the target.
fn title(entry: &Entry, home: Home) -> String {
    let base = format!("New upstream changes: {}", entry.label());
    match home {
        Home::Target => base,
        Home::Fallback => format!("{base} for {}", entry.target),
    }
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(SHORT_SHA)]
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::github::{Commit, Compare};

    const SYNC: &str = "AnkiQuest/AnkiQuest-Sync";
    const ANKI: &str = "AnkiQuest/AnkiQuest-Anki";
    const ANDROID: &str = "AnkiQuest/AnkiQuest-Android";
    const OLD: &str = "8c2bd5300cdbcf9e70f1b26b0b61c06190aca71e";
    const NEW: &str = "617683ad03aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    /// GitHub in memory: branch heads, files, repositories without issues,
    /// repositories whose reads fail, open issues, and what was written.
    #[derive(Default)]
    struct Fake {
        heads: HashMap<String, String>,
        files: HashMap<String, String>,
        no_issues: HashSet<String>,
        broken: HashSet<String>,
        issues: RefCell<HashMap<String, Vec<Issue>>>,
        writes: RefCell<Vec<String>>,
    }

    impl Fake {
        fn check(&self, repo: &str) -> Result<(), String> {
            match self.broken.contains(repo) {
                true => Err(format!("{repo}: HTTP 502")),
                false => Ok(()),
            }
        }

        fn writes(&self) -> Vec<String> {
            self.writes.borrow().clone()
        }
    }

    impl GitHub for Fake {
        fn commit(&self, repo: &str, rev: &str) -> Result<String, String> {
            self.check(repo)?;
            Ok(self
                .heads
                .get(&format!("{repo}@{rev}"))
                .cloned()
                .unwrap_or_default())
        }

        fn file(&self, repo: &str, path: &str, _rev: &str) -> Result<String, String> {
            self.check(repo)?;
            Ok(self
                .files
                .get(&format!("{repo}/{path}"))
                .cloned()
                .unwrap_or_default())
        }

        fn compare(&self, repo: &str, _base: &str, head: &str) -> Result<Compare, String> {
            self.check(repo)?;
            let commits = (0..60)
                .map(|n| Commit {
                    sha: format!("{n:02}{head}"),
                    subject: format!("commit {n}"),
                })
                .collect();
            Ok(Compare { total: 60, commits })
        }

        fn has_issues(&self, repo: &str) -> Result<bool, String> {
            self.check(repo)?;
            Ok(!self.no_issues.contains(repo))
        }

        fn open_issues(&self, repo: &str) -> Result<Vec<Issue>, String> {
            self.check(repo)?;
            if self.no_issues.contains(repo) {
                return Err(format!("the '{repo}' repository has disabled issues"));
            }
            Ok(self.issues.borrow().get(repo).cloned().unwrap_or_default())
        }

        fn create_issue(&self, repo: &str, title: &str, _body: &str) -> Result<String, String> {
            if self.no_issues.contains(repo) {
                return Err(format!("the '{repo}' repository has disabled issues"));
            }
            let mut issues = self.issues.borrow_mut();
            let list = issues.entry(repo.to_owned()).or_default();
            let number = 100 + list.len() as u64;
            list.push(Issue {
                number,
                title: title.to_owned(),
            });
            self.writes
                .borrow_mut()
                .push(format!("create {repo} {title}"));
            Ok(format!("https://github.com/{repo}/issues/{number}"))
        }

        fn edit_issue(&self, repo: &str, number: u64, _body: &str) -> Result<(), String> {
            self.writes
                .borrow_mut()
                .push(format!("edit {repo}#{number}"));
            Ok(())
        }

        fn close_issue(&self, repo: &str, number: u64, comment: &str) -> Result<(), String> {
            self.writes
                .borrow_mut()
                .push(format!("close {repo}#{number}: {comment}"));
            Ok(())
        }
    }

    fn branch(source: &str, target: &str, reviewed: &str) -> Entry {
        Entry {
            source: source.into(),
            rev: "rwkv".into(),
            kind: Kind::Branch,
            target: target.into(),
            why: "w".into(),
            reviewed: reviewed.into(),
        }
    }

    fn issue(number: u64, title: &str) -> Issue {
        Issue {
            number,
            title: title.into(),
        }
    }

    /// The failure in the watch job: AnkiQuest-Anki has issues disabled, and
    /// the entries after its first one were never checked.
    #[test]
    fn target_without_issues_does_not_stop_later_entries() {
        let mut gh = Fake::default();
        gh.no_issues.insert(ANKI.into());
        gh.heads
            .insert("Pedrubik2000/kuma3-anki-core@rwkv".into(), "c1".into());
        gh.heads
            .insert("Pedrubik2000/kuma3-anki-app@rwkv".into(), NEW.into());
        let entries = [
            branch("Pedrubik2000/kuma3-anki-core", ANKI, "c1"),
            branch("Pedrubik2000/kuma3-anki-app", ANDROID, OLD),
        ];

        let summary = run(&gh, SYNC, &entries);

        assert_eq!(summary.failures, vec![]);
        assert_eq!(
            gh.writes(),
            vec![
                "create AnkiQuest/AnkiQuest-Android New upstream changes: Pedrubik2000/kuma3-anki-app rwkv"
            ]
        );
        assert_eq!(
            summary.lines[0],
            "Pedrubik2000/kuma3-anki-core rwkv: nothing new"
        );
    }

    #[test]
    fn failed_entry_is_recorded_and_the_rest_run() {
        let mut gh = Fake::default();
        gh.broken.insert("JSchoreels/anki".into());
        gh.heads
            .insert("Pedrubik2000/kuma3-anki-app@rwkv".into(), NEW.into());
        let entries = [
            branch("JSchoreels/anki", ANKI, "x"),
            branch("Pedrubik2000/kuma3-anki-app", ANDROID, OLD),
        ];

        let summary = run(&gh, SYNC, &entries);

        assert_eq!(
            summary.failures,
            vec![(
                "JSchoreels/anki rwkv".to_owned(),
                "JSchoreels/anki: HTTP 502".to_owned()
            )]
        );
        assert_eq!(summary.lines[0], "JSchoreels/anki rwkv: failed");
        assert!(summary.lines[1].starts_with("Pedrubik2000/kuma3-anki-app rwkv: new"));
        assert_eq!(gh.writes().len(), 1);
    }

    #[test]
    fn unreadable_source_fails() {
        let gh = Fake::default();
        let summary = run(&gh, SYNC, &[branch("a/b", ANDROID, OLD)]);
        assert_eq!(
            summary.failures,
            vec![("a/b rwkv".to_owned(), "could not read a/b rwkv".to_owned())]
        );
    }

    #[test]
    fn target_without_issues_reports_in_sync() {
        let mut gh = Fake::default();
        gh.no_issues.insert(ANKI.into());
        gh.heads.insert("JSchoreels/anki@rwkv".into(), NEW.into());
        let entry = branch("JSchoreels/anki", ANKI, OLD);

        let summary = run(&gh, SYNC, std::slice::from_ref(&entry));

        let title = "New upstream changes: JSchoreels/anki rwkv for AnkiQuest/AnkiQuest-Anki";
        assert_eq!(gh.writes(), vec![format!("create {SYNC} {title}")]);
        assert_eq!(
            summary.warnings,
            vec![format!(
                "{ANKI} has issues disabled; reported JSchoreels/anki rwkv in https://github.com/{SYNC}/issues/100"
            )]
        );

        // The next run updates the same issue
        let summary = run(&gh, SYNC, &[entry]);
        assert_eq!(summary.failures, vec![]);
        assert_eq!(gh.writes()[1], format!("edit {SYNC}#100"));
    }

    #[test]
    fn fallback_body_says_why() {
        let gh = Fake::default();
        let watch = Watch {
            gh: &gh,
            fallback: SYNC,
        };
        let body = watch
            .body(&branch("JSchoreels/anki", ANKI, OLD), NEW, Home::Fallback)
            .unwrap();
        assert!(body.contains("Filed in AnkiQuest/AnkiQuest-Sync because AnkiQuest/AnkiQuest-Anki has issues disabled."));
        assert!(body.contains("60 new commits since `8c2bd5300c` (latest 50 below)"));
        assert!(body.contains("- 10617683ad commit 10\n"));
        assert!(!body.contains("commit 9\n"));
        assert!(body.ends_with("Opened by AnkiQuest/AnkiQuest-Sync."));
    }

    #[test]
    fn reviewed_closes_the_open_issue() {
        let mut gh = Fake::default();
        gh.heads.insert("a/b@rwkv".into(), OLD.into());
        gh.issues.borrow_mut().insert(
            ANDROID.into(),
            vec![issue(7, "New upstream changes: a/b rwkv")],
        );

        let summary = run(&gh, SYNC, &[branch("a/b", ANDROID, OLD)]);

        assert_eq!(summary.lines, vec!["a/b rwkv: nothing new"]);
        assert_eq!(
            gh.writes(),
            vec![format!(
                "close {ANDROID}#7: Reviewed up to {OLD} in AnkiQuest/AnkiQuest-Sync watched.json."
            )]
        );
    }

    #[test]
    fn issues_enabled_again_moves_the_fallback_issue() {
        let mut gh = Fake::default();
        gh.heads.insert("a/b@rwkv".into(), NEW.into());
        gh.issues.borrow_mut().insert(
            SYNC.into(),
            vec![issue(
                3,
                &format!("New upstream changes: a/b rwkv for {ANKI}"),
            )],
        );

        run(&gh, SYNC, &[branch("a/b", ANKI, OLD)]);

        assert_eq!(
            gh.writes(),
            vec![
                format!("create {ANKI} New upstream changes: a/b rwkv"),
                format!("close {SYNC}#3: Moved to https://github.com/{ANKI}/issues/100."),
            ]
        );
    }

    #[test]
    fn new_version_is_reported() {
        let mut gh = Fake::default();
        gh.files.insert(
            "ankidroid/Anki-Android-Backend/gradle.properties".into(),
            "VERSION_NAME=0.1.78\n".into(),
        );
        let entry = Entry {
            source: "ankidroid/Anki-Android-Backend".into(),
            rev: "main".into(),
            kind: Kind::Version {
                file: "gradle.properties".into(),
                key: "VERSION_NAME".into(),
            },
            target: "AnkiQuest/AnkiQuest-Android-Backend".into(),
            why: "w".into(),
            reviewed: "0.1.77".into(),
        };

        let summary = run(&gh, SYNC, std::slice::from_ref(&entry));
        assert_eq!(summary.failures, vec![]);
        let watch = Watch {
            gh: &gh,
            fallback: SYNC,
        };
        let body = watch.body(&entry, "0.1.78", Home::Target).unwrap();
        assert!(body.contains("Upstream version is now `0.1.78` (reviewed: `0.1.77`)."));
    }
}
