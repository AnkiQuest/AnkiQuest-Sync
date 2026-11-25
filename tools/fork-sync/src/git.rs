// SPDX-License-Identifier: AGPL-3.0-or-later

//! The git commands the tool runs, in one checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A git work tree; every command runs in it.
pub struct Repo {
    dir: PathBuf,
}

/// Whether a command's failure is an error or an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Error,
    Answer,
}

/// The mode git gives a submodule's entry (a gitlink).
pub const GITLINK: &str = "160000";

impl Repo {
    pub fn new(dir: &Path) -> Repo {
        Repo {
            dir: dir.to_path_buf(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Runs git and returns its stdout; a failure is an error.
    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        match self.status(args, Failure::Error)? {
            Some(out) => Ok(out),
            None => unreachable!("Failure::Error never answers None"),
        }
    }

    /// Runs git; with Failure::Answer a failure returns None instead of an error.
    pub fn status(&self, args: &[&str], failure: Failure) -> Result<Option<String>, String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .map_err(|e| format!("git: {e}"))?;
        if out.status.success() {
            return Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()));
        }
        if failure == Failure::Answer {
            return Ok(None);
        }
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }

    /// The commit a revision names.
    pub fn commit(&self, rev: &str) -> Result<String, String> {
        Ok(self
            .run(&["rev-parse", "--verify", &format!("{rev}^{{commit}}")])?
            .trim()
            .to_owned())
    }

    pub fn is_ancestor(&self, ancestor: &str, of: &str) -> Result<bool, String> {
        Ok(self
            .status(
                &["merge-base", "--is-ancestor", ancestor, of],
                Failure::Answer,
            )?
            .is_some())
    }

    /// A file's text at a revision; None if it has none.
    pub fn show(&self, rev: &str, path: &str) -> Result<Option<String>, String> {
        self.status(&["show", &format!("{rev}:{path}")], Failure::Answer)
    }

    /// Submodule paths and their commits at a revision.
    pub fn gitlinks(&self, rev: &str) -> Result<Vec<(String, String)>, String> {
        let tree = self.run(&["ls-tree", "-r", "-z", rev])?;
        let mut links = Vec::new();
        for entry in tree.split('\0').filter(|e| !e.is_empty()) {
            let Some((meta, path)) = entry.split_once('\t') else {
                continue;
            };
            let fields: Vec<&str> = meta.split(' ').collect();
            if fields.first() == Some(&GITLINK) && fields.len() == 3 {
                links.push((path.to_owned(), fields[2].to_owned()));
            }
        }
        Ok(links)
    }
}
