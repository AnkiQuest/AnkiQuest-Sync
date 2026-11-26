// SPDX-License-Identifier: AGPL-3.0-or-later

//! The GitHub calls the watch job makes, through `gh api` (REST only).

use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
}

/// An open pull request.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pull {
    pub number: u64,
    pub title: String,
    pub html_url: String,
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

/// How many of the new commits an issue or pull request lists.
pub const LISTED_COMMITS: usize = 50;
const SHORT_SHA: usize = 10;

impl Compare {
    /// The latest LISTED_COMMITS commits, one `- <sha> <subject>` line each.
    pub fn listing(&self) -> String {
        let skip = self.commits.len().saturating_sub(LISTED_COMMITS);
        let lines: Vec<String> = self.commits[skip..]
            .iter()
            .map(|c| format!("- {} {}", short(&c.sha), c.subject))
            .collect();
        lines.join("\n")
    }
}

/// A commit as titles and lists show it.
pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(SHORT_SHA)]
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
    fn default_branch(&self, repo: &str) -> Result<String, String>;
    /// The open pull request from `branch` of the same repository.
    fn open_pull(&self, repo: &str, branch: &str) -> Result<Option<Pull>, String>;
    /// Opens a pull request and returns its URL.
    fn create_pull(&self, repo: &str, pull: &NewPull) -> Result<String, String>;
    fn edit_pull(&self, repo: &str, number: u64, title: &str, body: &str) -> Result<(), String>;
}

/// What a new pull request says, and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPull {
    pub base: String,
    pub head: String,
    pub title: String,
    pub body: String,
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

    fn default_branch(&self, repo: &str) -> Result<String, String> {
        let path = format!("repos/{repo}");
        let branch = self.api(repo, &[&path, "--jq", ".default_branch"])?;
        Ok(branch.trim().to_owned())
    }

    fn open_pull(&self, repo: &str, branch: &str) -> Result<Option<Pull>, String> {
        let owner = repo.split('/').next().unwrap_or_default();
        let path = format!("repos/{repo}/pulls?head={owner}:{branch}&state=open");
        let out = self.api(repo, &[&path, "--jq", ".[0] // empty"])?;
        if out.trim().is_empty() {
            return Ok(None);
        }
        serde_json::from_str(&out).map_err(|e| format!("{path}: {e}"))
    }

    fn create_pull(&self, repo: &str, pull: &NewPull) -> Result<String, String> {
        let path = format!("repos/{repo}/pulls");
        let fields = [
            format!("base={}", pull.base),
            format!("head={}", pull.head),
            format!("title={}", pull.title),
            format!("body={}", pull.body),
        ];
        let mut args = vec![path.as_str()];
        for field in &fields {
            args.extend(["-f", field.as_str()]);
        }
        args.extend(["--jq", ".html_url"]);
        Ok(self.api(repo, &args)?.trim().to_owned())
    }

    fn edit_pull(&self, repo: &str, number: u64, title: &str, body: &str) -> Result<(), String> {
        let path = format!("repos/{repo}/pulls/{number}");
        let (title, body) = (format!("title={title}"), format!("body={body}"));
        let args = ["-X", "PATCH", &path, "-f", &title, "-f", &body, "--silent"];
        self.api(repo, &args)?;
        Ok(())
    }
}

impl crate::land::Pulls for Gh {
    fn default_branch(&self, repo: &str) -> Result<String, String> {
        GitHub::default_branch(self, repo)
    }

    fn open_pull(&self, repo: &str, branch: &str) -> Result<Option<Pull>, String> {
        GitHub::open_pull(self, repo, branch)
    }

    fn create_pull(&self, repo: &str, pull: &NewPull) -> Result<Pull, String> {
        let url = GitHub::create_pull(self, repo, pull)?;
        let number = url
            .rsplit('/')
            .next()
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| format!("{repo}: no pull request number in {url}"))?;
        Ok(Pull {
            number,
            title: pull.title.clone(),
            html_url: url,
        })
    }

    fn edit_pull(&self, repo: &str, number: u64, title: &str, body: &str) -> Result<(), String> {
        GitHub::edit_pull(self, repo, number, title, body)
    }

    fn close_pull(&self, repo: &str, number: u64, comment: &str) -> Result<(), String> {
        let comments = format!("repos/{repo}/issues/{number}/comments");
        let comment = format!("body={comment}");
        self.api(repo, &[&comments, "-f", &comment, "--silent"])?;

        let path = format!("repos/{repo}/pulls/{number}");
        self.api(
            repo,
            &["-X", "PATCH", &path, "-f", "state=closed", "--silent"],
        )?;
        Ok(())
    }

    /// `gh pr merge --auto`: auto-merge has no REST endpoint. Where the base
    /// branch has a merge queue, the pull request joins it and the queue's
    /// merge method applies.
    fn auto_merge(&self, repo: &str, number: u64) -> Result<(), String> {
        let number = number.to_string();
        let args = ["pr", "merge", &number, "--repo", repo, "--auto", "--squash"];
        let out = Command::new("gh")
            .args(args)
            .output()
            .map_err(|e| format!("gh: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        Err(format!(
            "#{number}: auto-merge could not be turned on (allow auto-merge in the \
             repository's settings): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
