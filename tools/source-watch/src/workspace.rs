// SPDX-License-Identifier: AGPL-3.0-or-later

//! The checkout a pin is moved in: git and cargo, in one directory.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Whether the push adds to a branch or starts it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    /// The open pull request's branch: the commit goes on top.
    Update,
    /// No open pull request: a branch left from an old one is deleted first.
    Replace,
}

pub trait Workspace {
    /// Checks out `branch` of `repo`, replacing the last checkout.
    fn checkout(&self, repo: &str, branch: &str) -> Result<(), String>;
    fn read(&self, path: &str) -> Result<String, String>;
    fn write(&self, path: &str, text: &str) -> Result<(), String>;
    /// Rewrites Cargo.lock for `package`'s new pin.
    fn lock(&self, package: &str) -> Result<(), String>;
    /// Commits every change, if any, and pushes HEAD to `branch`.
    fn push(&self, branch: &str, message: &str, push: Push) -> Result<(), String>;
}

/// The committer of the job's commits, as in the sync job.
const NAME: &str = "github-actions[bot]";
const EMAIL: &str = "41898282+github-actions[bot]@users.noreply.github.com";

/// A shallow clone in `dir`; git reads GH_TOKEN through gh's credential
/// helper (`gh auth setup-git` in the job).
pub struct Checkout {
    dir: PathBuf,
}

impl Checkout {
    pub fn new(dir: PathBuf) -> Checkout {
        Checkout { dir }
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<(), String> {
        let out = Command::new(program)
            .current_dir(&self.dir)
            .args(args)
            .output()
            .map_err(|e| format!("{program}: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        Err(format!(
            "{program} {}: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }

    fn git(&self, args: &[&str]) -> Result<(), String> {
        self.run("git", args)
    }

    /// Whether the work tree differs from HEAD.
    fn changed(&self) -> Result<bool, String> {
        let out = Command::new("git")
            .current_dir(&self.dir)
            .args(["status", "--porcelain"])
            .output()
            .map_err(|e| format!("git: {e}"))?;
        Ok(!out.stdout.is_empty())
    }
}

impl Workspace for Checkout {
    fn checkout(&self, repo: &str, branch: &str) -> Result<(), String> {
        let _ = fs::remove_dir_all(&self.dir);
        fs::create_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))?;
        let url = format!("https://github.com/{repo}.git");
        let args = ["clone", "-q", "--depth", "1", "--branch", branch, &url, "."];
        self.git(&args)
    }

    fn read(&self, path: &str) -> Result<String, String> {
        fs::read_to_string(self.dir.join(path)).map_err(|e| format!("{path}: {e}"))
    }

    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        fs::write(self.dir.join(path), text).map_err(|e| format!("{path}: {e}"))
    }

    fn lock(&self, package: &str) -> Result<(), String> {
        self.run("cargo", &["update", "-q", "-p", package])
    }

    fn push(&self, branch: &str, message: &str, push: Push) -> Result<(), String> {
        let name = format!("user.name={NAME}");
        let email = format!("user.email={EMAIL}");
        let commit = [
            "-c", &name, "-c", &email, "commit", "-q", "-a", "-m", message,
        ];
        if self.changed()? {
            self.git(&commit)?;
        }

        // Like the sync job: a stale branch is deleted, never force-pushed
        if push == Push::Replace {
            let _ = self.git(&["push", "-q", "origin", "--delete", branch]);
        }
        let refspec = format!("HEAD:refs/heads/{branch}");
        self.git(&["push", "-q", "origin", &refspec])
    }
}
