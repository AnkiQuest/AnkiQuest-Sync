// SPDX-License-Identifier: AGPL-3.0-or-later

//! The GitHub calls the watch job makes, through `gh api` (REST only).

use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compare {
    pub total: u64,
    pub commits: Vec<Commit>,
}

pub trait GitHub {
    /// The commit a branch points at.
    fn commit(&self, repo: &str, rev: &str) -> Result<String, String>;
    /// A file's text at a branch.
    fn file(&self, repo: &str, path: &str, rev: &str) -> Result<String, String>;
    fn compare(&self, repo: &str, base: &str, head: &str) -> Result<Compare, String>;
    /// Whether the repository accepts issues.
    fn has_issues(&self, repo: &str) -> Result<bool, String>;
    /// Open issues, without pull requests.
    fn open_issues(&self, repo: &str) -> Result<Vec<Issue>, String>;
    /// Opens an issue and returns its URL.
    fn create_issue(&self, repo: &str, title: &str, body: &str) -> Result<String, String>;
    fn edit_issue(&self, repo: &str, number: u64, body: &str) -> Result<(), String>;
    /// Comments, then closes as completed.
    fn close_issue(&self, repo: &str, number: u64, comment: &str) -> Result<(), String>;
}

/// `gh api`, with GH_TOKEN from the environment. One repository may use
/// another token: the job's own token writes AnkiQuest-Sync's issues.
pub struct Gh {
    own: Option<(String, String)>,
}

impl Gh {
    pub fn new(own: Option<(String, String)>) -> Gh {
        Gh { own }
    }

    fn api(&self, repo: &str, args: &[&str]) -> Result<String, String> {
        let mut cmd = Command::new("gh");
        cmd.arg("api").args(args);
        if let Some((own_repo, token)) = &self.own
            && own_repo == repo
        {
            cmd.env("GH_TOKEN", token);
        }

        let out = cmd.output().map_err(|e| format!("gh: {e}"))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        Err(format!(
            "gh api {}: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[derive(Deserialize)]
struct RawCompare {
    total_commits: u64,
    commits: Vec<RawCommit>,
}

#[derive(Deserialize)]
struct RawCommit {
    sha: String,
    commit: RawMessage,
}

#[derive(Deserialize)]
struct RawMessage {
    message: String,
}

impl GitHub for Gh {
    fn commit(&self, repo: &str, rev: &str) -> Result<String, String> {
        let path = format!("repos/{repo}/commits/{rev}");
        Ok(self.api(repo, &[&path, "--jq", ".sha"])?.trim().to_owned())
    }

    fn file(&self, repo: &str, path: &str, rev: &str) -> Result<String, String> {
        let url = format!("repos/{repo}/contents/{path}?ref={rev}");
        self.api(
            repo,
            &[&url, "-H", "Accept: application/vnd.github.raw+json"],
        )
    }

    fn compare(&self, repo: &str, base: &str, head: &str) -> Result<Compare, String> {
        let path = format!("repos/{repo}/compare/{base}...{head}");
        let raw: RawCompare =
            serde_json::from_str(&self.api(repo, &[&path])?).map_err(|e| format!("{path}: {e}"))?;
        let commits = raw
            .commits
            .into_iter()
            .map(|c| Commit {
                subject: c
                    .commit
                    .message
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                sha: c.sha,
            })
            .collect();
        Ok(Compare {
            total: raw.total_commits,
            commits,
        })
    }

    fn has_issues(&self, repo: &str) -> Result<bool, String> {
        let path = format!("repos/{repo}");
        match self.api(repo, &[&path, "--jq", ".has_issues"])?.trim() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(format!("{path}: has_issues is {other:?}")),
        }
    }

    fn open_issues(&self, repo: &str) -> Result<Vec<Issue>, String> {
        let path = format!("repos/{repo}/issues?state=open&per_page=100");
        let jq = ".[] | select(.pull_request | not) | {number, title}";
        let out = self.api(repo, &["--paginate", &path, "--jq", jq])?;
        out.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|e| format!("{path}: {e}")))
            .collect()
    }

    fn create_issue(&self, repo: &str, title: &str, body: &str) -> Result<String, String> {
        let path = format!("repos/{repo}/issues");
        let (title, body) = (format!("title={title}"), format!("body={body}"));
        let args = [&path, "-f", &title, "-f", &body, "--jq", ".html_url"];
        Ok(self.api(repo, &args)?.trim().to_owned())
    }

    fn edit_issue(&self, repo: &str, number: u64, body: &str) -> Result<(), String> {
        let path = format!("repos/{repo}/issues/{number}");
        let body = format!("body={body}");
        self.api(repo, &["-X", "PATCH", &path, "-f", &body, "--silent"])?;
        Ok(())
    }

    fn close_issue(&self, repo: &str, number: u64, comment: &str) -> Result<(), String> {
        let comments = format!("repos/{repo}/issues/{number}/comments");
        let comment = format!("body={comment}");
        self.api(repo, &[&comments, "-f", &comment, "--silent"])?;

        let path = format!("repos/{repo}/issues/{number}");
        let args = [
            "-X",
            "PATCH",
            &path,
            "-f",
            "state=closed",
            "-f",
            "state_reason=completed",
            "--silent",
        ];
        self.api(repo, &args)?;
        Ok(())
    }
}
