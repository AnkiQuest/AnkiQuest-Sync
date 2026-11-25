// SPDX-License-Identifier: AGPL-3.0-or-later

//! Lands generated files (AGENTS.md and CLAUDE.md from the agents job, the
//! art pin file from the art job) on a repository's default branch. The
//! commit is pushed straight to the branch, as a fast-forward: the sync
//! token's account bypasses the branch's rules. A refused push (no bypass,
//! or the branch moved) is tried once more on the latest branch; if that is
//! refused too, the files go to the fallback branch's pull request instead,
//! with auto-merge on. Nothing is ever force-pushed.

use crate::github::{NewPull, Pull};
use crate::workspace::{Push, Workspace};

/// The GitHub calls landing makes.
pub trait Pulls {
    fn default_branch(&self, repo: &str) -> Result<String, String>;
    /// The open pull request from `branch` of the same repository.
    fn open_pull(&self, repo: &str, branch: &str) -> Result<Option<Pull>, String>;
    /// Opens a pull request and returns it.
    fn create_pull(&self, repo: &str, pull: &NewPull) -> Result<Pull, String>;
    fn edit_pull(&self, repo: &str, number: u64, title: &str, body: &str) -> Result<(), String>;
    /// Comments, then closes.
    fn close_pull(&self, repo: &str, number: u64, comment: &str) -> Result<(), String>;
    /// Squash-merges, or joins the merge queue, once the branch's rules are met.
    fn auto_merge(&self, repo: &str, number: u64) -> Result<(), String>;
}

/// What to land: files (path in the repository, new text), the commit and
/// the fallback pull request.
pub struct Change {
    pub repo: String,
    pub fallback: String,
    pub title: String,
    pub message: String,
    pub body: String,
    pub files: Vec<(String, String)>,
}

/// What landing did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The default branch already has the files; an open fallback pull
    /// request, now empty, was closed.
    Current { closed: Option<u64> },
    /// Pushed to the default branch; an open fallback pull request was closed.
    Pushed { closed: Option<u64> },
    /// The push was refused (`reason`); the pull request carries the files.
    Proposed { url: String, reason: String },
}

/// Why the retry was skipped: the files changed on the branch meanwhile, so
/// a person should look at the new text.
const MOVED_FILES: &str = "the files changed on the default branch meanwhile";

/// How a closed fallback pull request is told why.
const LANDED: &str =
    "Pushed to the default branch directly; this pull request is no longer needed.";

impl Change {
    fn commit_message(&self) -> String {
        format!("{}\n\n{}", self.title, self.message)
    }

    /// Whether the checkout already has every file as given.
    fn current(&self, work: &impl Workspace) -> bool {
        self.files
            .iter()
            .all(|(path, text)| work.read(path).ok().as_deref() == Some(text.as_str()))
    }

    /// The checkout's text of each file, None where it has none.
    fn snapshot(&self, work: &impl Workspace) -> Vec<Option<String>> {
        self.files
            .iter()
            .map(|(path, _)| work.read(path).ok())
            .collect()
    }

    fn write(&self, work: &impl Workspace) -> Result<(), String> {
        for (path, text) in &self.files {
            work.write(path, text)?;
        }
        Ok(())
    }
}

pub fn land(gh: &impl Pulls, work: &impl Workspace, change: &Change) -> Result<Outcome, String> {
    let repo = change.repo.as_str();
    let base = gh.default_branch(repo)?;
    let open = gh.open_pull(repo, &change.fallback)?;

    // The default branch as it is now; nothing to do if it has the files
    work.checkout(repo, &base)?;
    if change.current(work) {
        let closed = close(gh, change, open)?;
        return Ok(Outcome::Current { closed });
    }
    let before = change.snapshot(work);

    // A fast-forward push, then once more on the latest branch
    let first = match push(work, change, &base) {
        Ok(()) => return pushed(gh, change, open),
        Err(e) => e,
    };
    work.checkout(repo, &base)?;
    if change.current(work) {
        let closed = close(gh, change, open)?;
        return Ok(Outcome::Current { closed });
    }
    let reason = if change.snapshot(work) == before {
        match push(work, change, &base) {
            Ok(()) => return pushed(gh, change, open),
            Err(e) => e,
        }
    } else {
        format!("{first}, and {MOVED_FILES}")
    };

    propose(gh, work, change, &base, open, reason)
}

fn push(work: &impl Workspace, change: &Change, base: &str) -> Result<(), String> {
    change.write(work)?;
    work.push(base, &change.commit_message(), Push::Update)
}

/// The push landed: a fallback pull request left open is closed.
fn pushed(gh: &impl Pulls, change: &Change, open: Option<Pull>) -> Result<Outcome, String> {
    let closed = close(gh, change, open)?;
    Ok(Outcome::Pushed { closed })
}

/// Closes the fallback pull request, if open: the default branch has its files.
fn close(gh: &impl Pulls, change: &Change, open: Option<Pull>) -> Result<Option<u64>, String> {
    let Some(pull) = open else {
        return Ok(None);
    };
    gh.close_pull(&change.repo, pull.number, LANDED)?;
    Ok(Some(pull.number))
}

/// The fallback: add to the open pull request, or open one, with auto-merge.
fn propose(
    gh: &impl Pulls,
    work: &impl Workspace,
    change: &Change,
    base: &str,
    open: Option<Pull>,
    reason: String,
) -> Result<Outcome, String> {
    let repo = change.repo.as_str();
    let message = change.commit_message();

    let pull = match open {
        Some(pull) => {
            work.checkout(repo, &change.fallback)?;
            change.write(work)?;
            work.push(&change.fallback, &message, Push::Update)?;
            gh.edit_pull(repo, pull.number, &change.title, &change.body)?;
            pull
        }
        None => {
            // The checkout is the latest default branch
            work.checkout(repo, base)?;
            change.write(work)?;
            work.push(&change.fallback, &message, Push::Replace)?;
            let new = NewPull {
                base: base.to_owned(),
                head: change.fallback.clone(),
                title: change.title.clone(),
                body: change.body.clone(),
            };
            gh.create_pull(repo, &new)?
        }
    };
    gh.auto_merge(repo, pull.number)?;
    Ok(Outcome::Proposed {
        url: pull.html_url,
        reason,
    })
}

/// The summary line for one repository.
pub fn line(repo: &str, outcome: &Outcome) -> String {
    match outcome {
        Outcome::Current { closed: None } => format!("{repo}: up to date"),
        Outcome::Current {
            closed: Some(number),
        } => format!("{repo}: up to date, closed #{number}"),
        Outcome::Pushed { closed: None } => format!("{repo}: pushed to the default branch"),
        Outcome::Pushed {
            closed: Some(number),
        } => format!("{repo}: pushed to the default branch, closed #{number}"),
        Outcome::Proposed { url, reason } => {
            let reason = reason.lines().next().unwrap_or_default();
            format!("{repo}: push refused ({reason}); pull request {url} with auto-merge")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    const REPO: &str = "AnkiQuest/AnkiQuest-Server";
    const FALLBACK: &str = "agents-sync";

    /// A remote in memory: each branch's files, pushes refused on the
    /// default branch, and what happened.
    #[derive(Default)]
    struct Remote {
        branches: RefCell<HashMap<String, HashMap<String, String>>>,
        refusals: RefCell<usize>,
        /// Files the default branch gets after the first refusal (someone merged).
        moved: Vec<(String, String)>,
        open: Option<Pull>,
        checkout: RefCell<(String, HashMap<String, String>)>,
        log: RefCell<Vec<String>>,
    }

    impl Remote {
        fn new(files: &[(&str, &str)]) -> Remote {
            let master = files
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect();
            let r = Remote::default();
            r.branches.borrow_mut().insert("master".into(), master);
            r
        }

        fn log(&self) -> Vec<String> {
            self.log.borrow().clone()
        }

        fn master(&self, path: &str) -> Option<String> {
            self.branches.borrow()["master"].get(path).cloned()
        }
    }

    impl Workspace for Remote {
        fn checkout(&self, _repo: &str, branch: &str) -> Result<(), String> {
            let files = self
                .branches
                .borrow()
                .get(branch)
                .cloned()
                .unwrap_or_default();
            *self.checkout.borrow_mut() = (branch.to_owned(), files);
            self.log.borrow_mut().push(format!("checkout {branch}"));
            Ok(())
        }

        fn read(&self, path: &str) -> Result<String, String> {
            self.checkout
                .borrow()
                .1
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        }

        fn write(&self, path: &str, text: &str) -> Result<(), String> {
            self.checkout
                .borrow_mut()
                .1
                .insert(path.into(), text.into());
            Ok(())
        }

        fn lock(&self, _package: &str) -> Result<(), String> {
            unreachable!()
        }

        fn push(&self, branch: &str, _message: &str, push: Push) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("push {branch} {push:?}"));
            if branch == "master" && *self.refusals.borrow() > 0 {
                *self.refusals.borrow_mut() -= 1;
                let mut branches = self.branches.borrow_mut();
                let master = branches.get_mut("master").unwrap();
                for (path, text) in &self.moved {
                    master.insert(path.clone(), text.clone());
                }
                return Err("rejected: protected branch".into());
            }
            let files = self.checkout.borrow().1.clone();
            self.branches.borrow_mut().insert(branch.to_owned(), files);
            Ok(())
        }
    }

    impl Pulls for Remote {
        fn default_branch(&self, _repo: &str) -> Result<String, String> {
            Ok("master".into())
        }

        fn open_pull(&self, _repo: &str, _branch: &str) -> Result<Option<Pull>, String> {
            Ok(self.open.clone())
        }

        fn create_pull(&self, _repo: &str, pull: &NewPull) -> Result<Pull, String> {
            self.log
                .borrow_mut()
                .push(format!("pull {} <- {}", pull.base, pull.head));
            Ok(Pull {
                number: 7,
                title: pull.title.clone(),
                html_url: "https://github.com/o/r/pull/7".into(),
            })
        }

        fn edit_pull(&self, _repo: &str, number: u64, _t: &str, _b: &str) -> Result<(), String> {
            self.log.borrow_mut().push(format!("edit #{number}"));
            Ok(())
        }

        fn close_pull(&self, _repo: &str, number: u64, _comment: &str) -> Result<(), String> {
            self.log.borrow_mut().push(format!("close #{number}"));
            Ok(())
        }

        fn auto_merge(&self, _repo: &str, number: u64) -> Result<(), String> {
            self.log.borrow_mut().push(format!("auto-merge #{number}"));
            Ok(())
        }
    }

    fn change() -> Change {
        Change {
            repo: REPO.into(),
            fallback: FALLBACK.into(),
            title: "chore(agents): update AGENTS.md from AnkiQuest-Agents".into(),
            message: "Generated.".into(),
            body: "Body.".into(),
            files: vec![
                ("AGENTS.md".into(), "new rules\n".into()),
                ("CLAUDE.md".into(), "@AGENTS.md\n".into()),
            ],
        }
    }

    fn open() -> Option<Pull> {
        Some(Pull {
            number: 3,
            title: "old".into(),
            html_url: "https://github.com/o/r/pull/3".into(),
        })
    }

    #[test]
    fn current_does_nothing() {
        let r = Remote::new(&[("AGENTS.md", "new rules\n"), ("CLAUDE.md", "@AGENTS.md\n")]);
        assert_eq!(
            land(&r, &r, &change()).unwrap(),
            Outcome::Current { closed: None }
        );
        assert_eq!(r.log(), ["checkout master"]);
    }

    #[test]
    fn current_closes_a_stale_pull_request() {
        let r = Remote {
            open: open(),
            ..Remote::new(&[("AGENTS.md", "new rules\n"), ("CLAUDE.md", "@AGENTS.md\n")])
        };
        let outcome = land(&r, &r, &change()).unwrap();
        assert_eq!(outcome, Outcome::Current { closed: Some(3) });
        assert_eq!(
            line(REPO, &outcome),
            format!("{REPO}: up to date, closed #3")
        );
    }

    #[test]
    fn pushes_to_the_default_branch() {
        let r = Remote::new(&[("AGENTS.md", "old\n")]);
        let outcome = land(&r, &r, &change()).unwrap();
        assert_eq!(outcome, Outcome::Pushed { closed: None });
        assert_eq!(r.log(), ["checkout master", "push master Update"]);
        assert_eq!(r.master("AGENTS.md").unwrap(), "new rules\n");
        assert_eq!(r.master("CLAUDE.md").unwrap(), "@AGENTS.md\n");
    }

    #[test]
    fn closes_the_fallback_pull_request_once_pushed() {
        let r = Remote {
            open: open(),
            ..Remote::new(&[("AGENTS.md", "old\n")])
        };
        let outcome = land(&r, &r, &change()).unwrap();
        assert_eq!(outcome, Outcome::Pushed { closed: Some(3) });
        assert_eq!(r.log().last().unwrap(), "close #3");
    }

    #[test]
    fn retries_once_on_the_latest_branch() {
        // master moved elsewhere: the second push lands
        let r = Remote::new(&[("AGENTS.md", "old\n")]);
        *r.refusals.borrow_mut() = 1;
        let outcome = land(&r, &r, &change()).unwrap();
        assert_eq!(outcome, Outcome::Pushed { closed: None });
        assert_eq!(
            r.log(),
            [
                "checkout master",
                "push master Update",
                "checkout master",
                "push master Update"
            ]
        );
        assert_eq!(r.master("AGENTS.md").unwrap(), "new rules\n");
    }

    #[test]
    fn falls_back_to_a_pull_request() {
        // No bypass: both pushes refused, master unchanged
        let r = Remote::new(&[("AGENTS.md", "old\n")]);
        *r.refusals.borrow_mut() = 2;
        let outcome = land(&r, &r, &change()).unwrap();
        assert_eq!(
            outcome,
            Outcome::Proposed {
                url: "https://github.com/o/r/pull/7".into(),
                reason: "rejected: protected branch".into()
            }
        );
        assert_eq!(
            r.log()[4..],
            [
                "checkout master",
                "push agents-sync Replace",
                "pull master <- agents-sync",
                "auto-merge #7"
            ]
        );
        assert_eq!(r.master("AGENTS.md").unwrap(), "old\n");
        assert_eq!(
            line(REPO, &outcome),
            "AnkiQuest/AnkiQuest-Server: push refused (rejected: protected branch); \
             pull request https://github.com/o/r/pull/7 with auto-merge"
        );
    }

    #[test]
    fn updates_the_open_pull_request() {
        let r = Remote {
            open: open(),
            ..Remote::new(&[("AGENTS.md", "old\n")])
        };
        *r.refusals.borrow_mut() = 2;
        land(&r, &r, &change()).unwrap();
        assert_eq!(
            r.log()[4..],
            [
                "checkout agents-sync",
                "push agents-sync Update",
                "edit #3",
                "auto-merge #3"
            ]
        );
    }

    #[test]
    fn no_retry_when_the_files_moved() {
        // The refused push raced a change to AGENTS.md itself
        let r = Remote {
            moved: vec![("AGENTS.md".into(), "someone else's\n".into())],
            ..Remote::new(&[("AGENTS.md", "old\n")])
        };
        *r.refusals.borrow_mut() = 1;
        let outcome = land(&r, &r, &change()).unwrap();
        let Outcome::Proposed { reason, .. } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(
            reason,
            format!("rejected: protected branch, and {MOVED_FILES}")
        );
        assert_eq!(
            r.log()
                .iter()
                .filter(|l| *l == "push master Update")
                .count(),
            1
        );
        assert_eq!(r.master("AGENTS.md").unwrap(), "someone else's\n");
    }
}
