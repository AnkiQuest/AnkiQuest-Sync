// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the watch job does for each watched source, by its mode:
//!
//! - port: one issue while the source has changes past `reviewed`, closed
//!   once it has none. A target with issues disabled gets its issue in
//!   AnkiQuest-Sync instead, titled with the target.
//! - pin: one pull request moving the target's Cargo.toml pin to the
//!   source's head, from the source's sync branch.
//! - merge: nothing; the sync job merges the source (`matrix`).
//!
//! Pin and merge close an issue left from when the source was ported.

use crate::github::{GitHub, Issue, LISTED_COMMITS, NewPull, short};
use crate::pin::{self, Bump};
use crate::watched::{Entry, Kind, Mode, property};
use crate::workspace::{Push, Workspace};

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
pub fn run(gh: &impl GitHub, work: &impl Workspace, fallback: &str, entries: &[Entry]) -> Summary {
    let watch = Watch { gh, work, fallback };
    let mut summary = Summary::default();
    for entry in entries {
        let result = match &entry.mode {
            Mode::Port { reviewed } => watch.port(entry, reviewed, &mut summary.warnings),
            Mode::Pin { package } => watch.pin(entry, package),
            Mode::Merge => watch.merge(entry),
        };
        match result {
            Ok(line) => summary.lines.push(line),
            Err(error) => {
                summary.lines.push(format!("{}: failed", entry.label()));
                summary.failures.push((entry.label(), error));
            }
        }
    }
    summary
}

struct Watch<'a, G, W> {
    gh: &'a G,
    work: &'a W,
    fallback: &'a str,
}

/// An entry's open issues: in its home, and one left in the fallback from
/// before the target had issues enabled.
struct Issues {
    home: Home,
    open: Option<u64>,
    stale: Option<u64>,
}

impl<G: GitHub, W: Workspace> Watch<'_, G, W> {
    /// Reports one ported entry and returns its summary line.
    fn port(
        &self,
        entry: &Entry,
        reviewed: &str,
        warnings: &mut Vec<String>,
    ) -> Result<String, String> {
        let current = self.current(entry)?;
        let issues = self.issues(entry)?;
        let repo = self.repo(entry, issues.home);

        // Reviewed: close what is left
        if current == reviewed {
            let comment =
                format!("Reviewed up to {current} in AnkiQuest/AnkiQuest-Sync watched.json.");
            self.close(entry, &issues, &comment)?;
            return Ok(format!("{}: nothing new", entry.label()));
        }

        // New: update the open issue or open one
        let body = self.body(entry, reviewed, &current, issues.home)?;
        let url = match issues.open {
            Some(number) => {
                self.gh.edit_issue(repo, number, &body)?;
                format!("https://github.com/{repo}/issues/{number}")
            }
            None => self
                .gh
                .create_issue(repo, &title(entry, issues.home), &body)?,
        };
        if let Some(number) = issues.stale {
            let comment = format!("Moved to {url}.");
            self.gh.close_issue(self.fallback, number, &comment)?;
        }
        if issues.home == Home::Fallback {
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

    /// Moves a pinned entry's pin in a pull request and returns its summary line.
    fn pin(&self, entry: &Entry, package: &str) -> Result<String, String> {
        let current = self.current(entry)?;
        let base = self.gh.default_branch(&entry.target)?;
        let manifest = self.gh.file(&entry.target, pin::MANIFEST, &base)?;
        let pinned = pin::pinned(&manifest, &entry.source).ok_or_else(|| {
            format!(
                "{} {} pins no commit of {}",
                entry.target,
                pin::MANIFEST,
                entry.source
            )
        })?;

        // Pinned at the head: reviewed
        if pinned == current {
            let comment = format!("{} pins {current}.", entry.target);
            self.close(entry, &self.issues(entry)?, &comment)?;
            return Ok(format!("{}: nothing new", entry.label()));
        }

        // An open pull request already at the head
        let branch = entry.branch();
        let title = pin::title(package, &entry.source, &current);
        let open = self.gh.open_pull(&entry.target, &branch)?;
        if let Some(pull) = open.as_ref().filter(|pull| pull.title == title) {
            return Ok(format!(
                "{}: new ({current}), pull request {} is up to date",
                entry.label(),
                pull.html_url
            ));
        }

        // Move the pin: on the open pull request's branch, else on the default branch
        let (start, push) = match &open {
            Some(_) => (branch.as_str(), Push::Update),
            None => (base.as_str(), Push::Replace),
        };
        self.work.checkout(&entry.target, start)?;
        let moved = pin::set(&self.work.read(pin::MANIFEST)?, &entry.source, &current)?;
        self.work.write(pin::MANIFEST, &moved)?;
        self.work.lock(package)?;
        self.work.push(&branch, &title, push)?;

        // Open or update the pull request
        let cmp = self.gh.compare(&entry.source, &pinned, &current)?;
        let body = pin::body(&Bump {
            package,
            source: &entry.source,
            rev: &entry.rev,
            from: &pinned,
            to: &current,
            branch: &branch,
            compare: &cmp,
        });
        let url = match open {
            Some(pull) => {
                self.gh
                    .edit_pull(&entry.target, pull.number, &title, &body)?;
                pull.html_url
            }
            None => {
                let new = NewPull {
                    base,
                    head: branch,
                    title,
                    body,
                };
                self.gh.create_pull(&entry.target, &new)?
            }
        };
        self.close(entry, &self.issues(entry)?, &format!("Moved to {url}."))?;
        Ok(format!(
            "{}: new ({current}), pull request {url}",
            entry.label()
        ))
    }

    /// A merged entry: the sync job opens its pull request.
    fn merge(&self, entry: &Entry) -> Result<String, String> {
        let branch = entry.branch();
        let comment = format!(
            "The sync job of AnkiQuest/AnkiQuest-Sync now merges {} into {} in pull requests from `{branch}`.",
            entry.label(),
            entry.target
        );
        self.close(entry, &self.issues(entry)?, &comment)?;
        Ok(format!(
            "{}: merged by the sync job from {branch}",
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

    fn issues(&self, entry: &Entry) -> Result<Issues, String> {
        let home = match self.gh.has_issues(&entry.target)? {
            true => Home::Target,
            false => Home::Fallback,
        };
        let open = self.find(self.repo(entry, home), &title(entry, home))?;
        let stale = match home {
            Home::Target => self.find(self.fallback, &title(entry, Home::Fallback))?,
            Home::Fallback => None,
        };
        Ok(Issues { home, open, stale })
    }

    /// Closes the entry's open issues with `comment`.
    fn close(&self, entry: &Entry, issues: &Issues, comment: &str) -> Result<(), String> {
        if let Some(number) = issues.open {
            self.gh
                .close_issue(self.repo(entry, issues.home), number, comment)?;
        }
        if let Some(number) = issues.stale {
            self.gh.close_issue(self.fallback, number, comment)?;
        }
        Ok(())
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

    fn body(
        &self,
        entry: &Entry,
        reviewed: &str,
        current: &str,
        home: Home,
    ) -> Result<String, String> {
        let source = &entry.source;
        let details = match entry.kind {
            Kind::Version { .. } => {
                format!("Upstream version is now `{current}` (reviewed: `{reviewed}`).")
            }
            Kind::Branch => {
                let cmp = self.gh.compare(source, reviewed, current)?;
                format!(
                    "{} new commits since `{}` (latest {LISTED_COMMITS} below): \
                     https://github.com/{source}/compare/{reviewed}...{current}\n\n{}",
                    cmp.total,
                    short(reviewed),
                    cmp.listing()
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::github::{Commit, Compare, Pull};

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
        pulls: RefCell<HashMap<String, Pull>>,
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

        fn default_branch(&self, repo: &str) -> Result<String, String> {
            self.check(repo)?;
            Ok("master".into())
        }

        fn open_pull(&self, repo: &str, branch: &str) -> Result<Option<Pull>, String> {
            self.check(repo)?;
            Ok(self
                .pulls
                .borrow()
                .get(&format!("{repo}:{branch}"))
                .cloned())
        }

        fn create_pull(&self, repo: &str, pull: &NewPull) -> Result<String, String> {
            let html_url = format!("https://github.com/{repo}/pull/200");
            self.pulls.borrow_mut().insert(
                format!("{repo}:{}", pull.head),
                Pull {
                    number: 200,
                    title: pull.title.clone(),
                    html_url: html_url.clone(),
                },
            );
            self.writes.borrow_mut().push(format!(
                "pull {repo} {} <- {}: {}",
                pull.base, pull.head, pull.title
            ));
            Ok(html_url)
        }

        fn edit_pull(
            &self,
            repo: &str,
            number: u64,
            title: &str,
            _body: &str,
        ) -> Result<(), String> {
            if let Some(pull) = self
                .pulls
                .borrow_mut()
                .values_mut()
                .find(|p| p.number == number)
            {
                pull.title = title.to_owned();
            }
            self.writes
                .borrow_mut()
                .push(format!("edit pull {repo}#{number}: {title}"));
            Ok(())
        }
    }

    /// A checkout in memory: files by path, and what was done to it.
    #[derive(Default)]
    struct Work {
        files: RefCell<HashMap<String, String>>,
        /// The Cargo.toml every checkout starts with.
        manifest: String,
        log: RefCell<Vec<String>>,
    }

    impl Work {
        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }
    }

    impl Workspace for Work {
        fn checkout(&self, repo: &str, branch: &str) -> Result<(), String> {
            let mut files = self.files.borrow_mut();
            files.clear();
            files.insert(pin::MANIFEST.into(), self.manifest.clone());
            self.log
                .borrow_mut()
                .push(format!("checkout {repo} {branch}"));
            Ok(())
        }

        fn read(&self, path: &str) -> Result<String, String> {
            self.files
                .borrow()
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        }

        fn write(&self, path: &str, text: &str) -> Result<(), String> {
            self.files.borrow_mut().insert(path.into(), text.into());
            Ok(())
        }

        fn lock(&self, package: &str) -> Result<(), String> {
            self.log.borrow_mut().push(format!("lock {package}"));
            Ok(())
        }

        fn push(&self, branch: &str, message: &str, push: Push) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("push {branch} {push:?}: {message}"));
            Ok(())
        }
    }

    /// An entry in mode "port".
    fn branch(source: &str, target: &str, reviewed: &str) -> Entry {
        Entry {
            source: source.into(),
            rev: "rwkv".into(),
            kind: Kind::Branch,
            mode: Mode::Port {
                reviewed: reviewed.into(),
            },
            target: target.into(),
            why: "w".into(),
        }
    }

    fn with_mode(source: &str, target: &str, mode: Mode) -> Entry {
        Entry {
            rev: "main".into(),
            mode,
            ..branch(source, target, "")
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

        let summary = run(&gh, &Work::default(), SYNC, &entries);

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

        let summary = run(&gh, &Work::default(), SYNC, &entries);

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
        let summary = run(&gh, &Work::default(), SYNC, &[branch("a/b", ANDROID, OLD)]);
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

        let summary = run(&gh, &Work::default(), SYNC, std::slice::from_ref(&entry));

        let title = "New upstream changes: JSchoreels/anki rwkv for AnkiQuest/AnkiQuest-Anki";
        assert_eq!(gh.writes(), vec![format!("create {SYNC} {title}")]);
        assert_eq!(
            summary.warnings,
            vec![format!(
                "{ANKI} has issues disabled; reported JSchoreels/anki rwkv in https://github.com/{SYNC}/issues/100"
            )]
        );

        // The next run updates the same issue
        let summary = run(&gh, &Work::default(), SYNC, &[entry]);
        assert_eq!(summary.failures, vec![]);
        assert_eq!(gh.writes()[1], format!("edit {SYNC}#100"));
    }

    #[test]
    fn fallback_body_says_why() {
        let gh = Fake::default();
        let work = Work::default();
        let watch = Watch {
            gh: &gh,
            work: &work,
            fallback: SYNC,
        };
        let body = watch
            .body(
                &branch("JSchoreels/anki", ANKI, OLD),
                OLD,
                NEW,
                Home::Fallback,
            )
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

        let summary = run(&gh, &Work::default(), SYNC, &[branch("a/b", ANDROID, OLD)]);

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

        run(&gh, &Work::default(), SYNC, &[branch("a/b", ANKI, OLD)]);

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
            mode: Mode::Port {
                reviewed: "0.1.77".into(),
            },
            target: "AnkiQuest/AnkiQuest-Android-Backend".into(),
            why: "w".into(),
        };

        let summary = run(&gh, &Work::default(), SYNC, std::slice::from_ref(&entry));
        assert_eq!(summary.failures, vec![]);
        let work = Work::default();
        let watch = Watch {
            gh: &gh,
            work: &work,
            fallback: SYNC,
        };
        let body = watch
            .body(&entry, "0.1.77", "0.1.78", Home::Target)
            .unwrap();
        assert!(body.contains("Upstream version is now `0.1.78` (reviewed: `0.1.77`)."));
    }

    const FSRS: &str = "JSchoreels/fsrs-rs";
    const FSRS_BRANCH: &str = "sync/JSchoreels/fsrs-rs-main";

    fn manifest(commit: &str) -> String {
        format!(
            "[workspace.dependencies.fsrs]\ngit = \"https://github.com/{FSRS}.git\"\nrev = \"{commit}\"\n"
        )
    }

    /// GitHub with fsrs-rs at NEW and AnkiQuest-Anki pinning `pinned`.
    fn pinned_at(pinned: &str) -> (Fake, Work, Entry) {
        let mut gh = Fake::default();
        gh.heads.insert(format!("{FSRS}@main"), NEW.into());
        gh.files
            .insert(format!("{ANKI}/Cargo.toml"), manifest(pinned));
        let work = Work {
            manifest: manifest(pinned),
            ..Work::default()
        };
        let entry = with_mode(
            FSRS,
            ANKI,
            Mode::Pin {
                package: "fsrs".into(),
            },
        );
        (gh, work, entry)
    }

    #[test]
    fn pin_at_the_head_is_reviewed() {
        let (mut gh, work, entry) = pinned_at(NEW);
        gh.no_issues.insert(ANKI.into());
        gh.issues.borrow_mut().insert(
            SYNC.into(),
            vec![issue(
                4,
                &format!("New upstream changes: {FSRS} main for {ANKI}"),
            )],
        );

        let summary = run(&gh, &work, SYNC, &[entry]);

        assert_eq!(summary.lines, vec![format!("{FSRS} main: nothing new")]);
        assert_eq!(
            gh.writes(),
            vec![format!("close {SYNC}#4: {ANKI} pins {NEW}.")]
        );
        assert_eq!(work.log(), Vec::<String>::new());
    }

    #[test]
    fn new_commit_opens_a_pin_pull_request() {
        let (gh, work, entry) = pinned_at(OLD);

        let summary = run(&gh, &work, SYNC, std::slice::from_ref(&entry));

        let title = "chore(deps): bump fsrs to JSchoreels/fsrs-rs 617683ad03";
        assert_eq!(summary.failures, vec![]);
        assert_eq!(
            work.log(),
            vec![
                format!("checkout {ANKI} master"),
                "lock fsrs".to_owned(),
                format!("push {FSRS_BRANCH} Replace: {title}"),
            ]
        );
        assert_eq!(work.read("Cargo.toml").unwrap(), manifest(NEW));
        assert_eq!(
            gh.writes(),
            vec![format!("pull {ANKI} master <- {FSRS_BRANCH}: {title}")]
        );
        assert_eq!(
            summary.lines,
            vec![format!(
                "{FSRS} main: new ({NEW}), pull request https://github.com/{ANKI}/pull/200"
            )]
        );

        // The next run finds the pull request at the head and leaves it
        let summary = run(&gh, &work, SYNC, &[entry]);
        assert!(summary.lines[0].ends_with(
            "pull request https://github.com/AnkiQuest/AnkiQuest-Anki/pull/200 is up to date"
        ));
        assert_eq!(work.log().len(), 3);
    }

    #[test]
    fn newer_commit_moves_the_open_pull_request() {
        let (gh, work, entry) = pinned_at(OLD);
        gh.pulls.borrow_mut().insert(
            format!("{ANKI}:{FSRS_BRANCH}"),
            Pull {
                number: 9,
                title: "chore(deps): bump fsrs to JSchoreels/fsrs-rs 1111111111".into(),
                html_url: format!("https://github.com/{ANKI}/pull/9"),
            },
        );

        let summary = run(&gh, &work, SYNC, &[entry]);

        let title = "chore(deps): bump fsrs to JSchoreels/fsrs-rs 617683ad03";
        assert_eq!(summary.failures, vec![]);
        assert_eq!(
            work.log(),
            vec![
                format!("checkout {ANKI} {FSRS_BRANCH}"),
                "lock fsrs".to_owned(),
                format!("push {FSRS_BRANCH} Update: {title}"),
            ]
        );
        assert_eq!(gh.writes(), vec![format!("edit pull {ANKI}#9: {title}")]);
    }

    #[test]
    fn pin_moves_a_ported_issue() {
        let (gh, work, entry) = pinned_at(OLD);
        gh.issues.borrow_mut().insert(
            ANKI.into(),
            vec![issue(5, &format!("New upstream changes: {FSRS} main"))],
        );

        run(&gh, &work, SYNC, &[entry]);

        assert_eq!(
            gh.writes()[1],
            format!("close {ANKI}#5: Moved to https://github.com/{ANKI}/pull/200.")
        );
    }

    #[test]
    fn target_without_the_pin_fails() {
        let (mut gh, work, entry) = pinned_at(OLD);
        gh.files.insert(format!("{ANKI}/Cargo.toml"), String::new());

        let summary = run(&gh, &work, SYNC, &[entry]);

        assert_eq!(
            summary.failures,
            vec![(
                format!("{FSRS} main"),
                format!("{ANKI} Cargo.toml pins no commit of {FSRS}")
            )]
        );
        assert_eq!(gh.writes(), Vec::<String>::new());
    }

    #[test]
    fn merge_leaves_it_to_the_sync_job() {
        let mut gh = Fake::default();
        gh.no_issues.insert(ANKI.into());
        gh.issues.borrow_mut().insert(
            SYNC.into(),
            vec![issue(
                6,
                &format!("New upstream changes: JSchoreels/anki main for {ANKI}"),
            )],
        );
        let work = Work::default();
        let entry = with_mode("JSchoreels/anki", ANKI, Mode::Merge);

        let summary = run(&gh, &work, SYNC, std::slice::from_ref(&entry));

        assert_eq!(
            summary.lines,
            vec!["JSchoreels/anki main: merged by the sync job from sync/JSchoreels/anki-main"]
        );
        assert_eq!(
            gh.writes(),
            vec![format!(
                "close {SYNC}#6: The sync job of AnkiQuest/AnkiQuest-Sync now merges JSchoreels/anki main into {ANKI} in pull requests from `sync/JSchoreels/anki-main`."
            )]
        );

        // A merged source never needs a checkout here
        let summary = run(&gh, &work, SYNC, &[entry]);
        assert_eq!(summary.failures, vec![]);
        assert_eq!(work.log(), Vec::<String>::new());
    }
}
