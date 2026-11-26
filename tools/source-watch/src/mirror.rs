// SPDX-License-Identifier: AGPL-3.0-or-later

//! mirrors.json: branches of our repositories that mirror a source branch
//! (AnkiQuest-Android's `upstream-main` is ankidroid/Anki-Android `main`).
//! The sync job's `mirror` job moves each one to the source's commit with a
//! plain push, never a force push and never a pull request. A mirror that
//! is not behind its source (the source was force-pushed, or someone pushed
//! to the mirror) fails the job and is left as it is.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// One mirrored branch.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mirror {
    pub source: String,
    #[serde(rename = "ref")]
    pub rev: String,
    pub target: String,
    pub branch: String,
    pub why: String,
}

/// Branch names a mirror may never take: whatever a repository's default
/// branch is, it is also refused (see `Remote::default_branch`).
const PROTECTED: [&str; 2] = ["master", "main"];

impl Mirror {
    /// How the summary and errors name the entry.
    pub fn label(&self) -> String {
        format!("{} {}", self.target, self.branch)
    }
}

pub fn parse(json: &str) -> Result<Vec<Mirror>, String> {
    let mirrors: Vec<Mirror> =
        serde_json::from_str(json).map_err(|e| format!("mirrors.json: {e}"))?;

    // Never a default branch, and never two entries writing one branch
    for (i, mirror) in mirrors.iter().enumerate() {
        if PROTECTED.contains(&mirror.branch.as_str()) {
            return Err(format!(
                "mirrors.json: {} would overwrite a default branch",
                mirror.label()
            ));
        }
        let repeated = mirrors[..i]
            .iter()
            .any(|m| m.target == mirror.target && m.branch == mirror.branch);
        if repeated {
            return Err(format!("mirrors.json: {} is listed twice", mirror.label()));
        }
    }
    Ok(mirrors)
}

/// Where the mirror stands against its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The target has no such branch yet.
    Missing,
    /// The mirror is on the source's commit.
    Current,
    /// The source has new commits on top of the mirror's commit.
    Behind(String),
    /// The mirror's commit has commits the source lacks: the source was
    /// force-pushed, or someone pushed to the mirror.
    Diverged(String),
}

impl State {
    /// The state from the two commits and whether `mirror` is an ancestor of `source`.
    pub fn of(source: &str, mirror: Option<String>, ancestor: bool) -> State {
        match mirror {
            None => State::Missing,
            Some(m) if m == source => State::Current,
            Some(m) if ancestor => State::Behind(m),
            Some(m) => State::Diverged(m),
        }
    }
}

/// What the job did for one mirror.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Created(String),
    Current(String),
    Moved { from: String, to: String },
}

/// The git side: one bare repository per run, fetching from and pushing to
/// `<base>/<owner>/<repo>.git`.
pub struct Remote {
    base: String,
    dir: PathBuf,
}

/// Local refs the fetched commits land on.
const SOURCE_REF: &str = "refs/mirror/source";
const TARGET_REF: &str = "refs/mirror/target";

impl Remote {
    /// `base` is https://github.com in the job; tests pass a directory.
    pub fn new(base: &str, dir: &Path) -> Result<Remote, String> {
        let _ = fs::remove_dir_all(dir);
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let remote = Remote {
            base: base.trim_end_matches('/').to_owned(),
            dir: dir.to_path_buf(),
        };
        remote.git(&["init", "-q", "--bare"])?;
        Ok(remote)
    }

    fn url(&self, repo: &str) -> String {
        format!("{}/{repo}.git", self.base)
    }

    fn git(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        Err(format!(
            "git {}: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }

    /// The branch HEAD names in `repo`.
    fn default_branch(&self, repo: &str) -> Result<String, String> {
        let out = self.git(&["ls-remote", "--symref", &self.url(repo), "HEAD"])?;
        out.lines()
            .find_map(|line| line.strip_prefix("ref: refs/heads/"))
            .and_then(|rest| rest.split('\t').next())
            .map(str::to_owned)
            .ok_or_else(|| format!("{repo} has no default branch"))
    }

    /// Fetches `branch` of `repo` into `local`; None if it has no such branch.
    fn fetch(&self, repo: &str, branch: &str, local: &str) -> Result<Option<String>, String> {
        let url = self.url(repo);
        let head = format!("refs/heads/{branch}");
        let listed = self.git(&["ls-remote", &url, &head])?;
        if listed.trim().is_empty() {
            return Ok(None);
        }
        let refspec = format!("+{head}:{local}");
        self.git(&["fetch", "-q", "--no-tags", &url, &refspec])?;
        Ok(Some(self.git(&["rev-parse", local])?.trim().to_owned()))
    }

    fn is_ancestor(&self, ancestor: &str, of: &str) -> bool {
        self.git(&["merge-base", "--is-ancestor", ancestor, of])
            .is_ok()
    }

    /// Moves the mirror to its source's commit.
    pub fn update(&self, mirror: &Mirror) -> Result<Outcome, String> {
        if self.default_branch(&mirror.target)? == mirror.branch {
            return Err(format!("{} is the default branch", mirror.branch));
        }

        let source = self
            .fetch(&mirror.source, &mirror.rev, SOURCE_REF)?
            .ok_or_else(|| format!("{} has no branch {}", mirror.source, mirror.rev))?;
        let current = self.fetch(&mirror.target, &mirror.branch, TARGET_REF)?;
        let ancestor = current
            .as_deref()
            .is_some_and(|c| self.is_ancestor(c, &source));

        // A plain push: the target refuses anything but a fast-forward
        let refspec = format!("{source}:refs/heads/{}", mirror.branch);
        let target = self.url(&mirror.target);
        match State::of(&source, current, ancestor) {
            State::Current => Ok(Outcome::Current(source)),
            State::Missing => {
                self.git(&["push", "-q", &target, &refspec])?;
                Ok(Outcome::Created(source))
            }
            State::Behind(from) => {
                self.git(&["push", "-q", &target, &refspec])?;
                Ok(Outcome::Moved { from, to: source })
            }
            State::Diverged(at) => Err(format!(
                "{} is at {at}, which {} {} does not contain: the source was \
                 force-pushed or someone pushed to the mirror. Not force-pushing; \
                 reset the branch by hand",
                mirror.branch, mirror.source, mirror.rev,
            )),
        }
    }
}

/// The summary line for one mirror.
pub fn line(mirror: &Mirror, outcome: &Outcome) -> String {
    let short = |sha: &str| sha.chars().take(10).collect::<String>();
    match outcome {
        Outcome::Created(sha) => format!("{}: created at {}", mirror.label(), short(sha)),
        Outcome::Current(sha) => format!("{}: up to date at {}", mirror.label(), short(sha)),
        Outcome::Moved { from, to } => format!(
            "{}: moved from {} to {}",
            mirror.label(),
            short(from),
            short(to)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = r#"{"source": "ankidroid/Anki-Android", "ref": "main",
      "target": "AnkiQuest/AnkiQuest-Android", "branch": "upstream-main", "why": "w"}"#;

    #[test]
    fn parses_an_entry() {
        let mirrors = parse(&format!("[{ENTRY}]")).unwrap();
        assert_eq!(mirrors[0].rev, "main");
        assert_eq!(
            mirrors[0].label(),
            "AnkiQuest/AnkiQuest-Android upstream-main"
        );
    }

    #[test]
    fn refuses_default_branches_and_repeats() {
        for branch in PROTECTED {
            let json = format!("[{}]", ENTRY.replace("upstream-main", branch));
            assert!(
                parse(&json).unwrap_err().contains("default branch"),
                "{branch}"
            );
        }
        let twice = format!("[{ENTRY}, {ENTRY}]");
        assert!(parse(&twice).unwrap_err().contains("listed twice"));

        // A misspelt field is an error, not a default
        let typo = ENTRY.replace("\"branch\"", "\"brnch\"");
        assert!(parse(&format!("[{typo}]")).is_err());
    }

    #[test]
    fn state_of_each_case() {
        let a = || Some("a".to_owned());
        assert_eq!(State::of("a", None, false), State::Missing);
        assert_eq!(State::of("a", a(), true), State::Current);
        assert_eq!(State::of("b", a(), true), State::Behind("a".into()));
        assert_eq!(State::of("b", a(), false), State::Diverged("a".into()));
    }
}
