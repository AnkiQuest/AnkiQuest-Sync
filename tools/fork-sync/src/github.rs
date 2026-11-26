// SPDX-License-Identifier: AGPL-3.0-or-later

//! The GitHub calls the agents job makes, through `gh api` (REST only).

use std::process::Command;

use serde::Deserialize;

/// Reads a fork and the upstream commits in its network.
pub trait Remote {
    /// The newest upstream commit `branch` of `fork` contains: the merge
    /// base of the two branches.
    fn merge_base(
        &self,
        fork: &str,
        branch: &str,
        upstream: &str,
        upstream_branch: &str,
    ) -> Result<String, String>;

    /// A file at the root of `commit`, as git has it (a symlink as its
    /// target's name), or None when the commit has no such file.
    fn root_file(&self, repo: &str, commit: &str, name: &str) -> Result<Option<String>, String>;
}

/// `gh api`, with GH_TOKEN from the environment.
pub struct Gh;

/// Only blobs are read as files; a directory or submodule of the same
/// name is not guidance.
const BLOB: &str = "blob";

#[derive(Deserialize)]
struct Tree {
    tree: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Deserialize)]
struct Compare {
    merge_base_commit: Sha,
}

#[derive(Deserialize)]
struct Sha {
    sha: String,
}

impl Gh {
    /// Runs `gh api`; the last argument is the path.
    fn api(args: &[&str]) -> Result<String, String> {
        let path = args.last().copied().unwrap_or_default();
        let out = Command::new("gh")
            .arg("api")
            .args(args)
            .output()
            .map_err(|e| format!("gh: {e}"))?;
        if out.status.success() {
            return String::from_utf8(out.stdout).map_err(|e| format!("gh api {path}: {e}"));
        }
        Err(format!(
            "gh api {path}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

impl Remote for Gh {
    fn merge_base(
        &self,
        fork: &str,
        branch: &str,
        upstream: &str,
        upstream_branch: &str,
    ) -> Result<String, String> {
        // The fork and upstream share a network, so one compare names both;
        // one listed commit is enough, only the merge base is read
        let owner = upstream.split('/').next().unwrap_or(upstream);
        let path = format!("repos/{fork}/compare/{branch}...{owner}:{upstream_branch}?per_page=1");
        let compare: Compare = parse(&path, &Gh::api(&[&path])?)?;
        Ok(compare.merge_base_commit.sha)
    }

    fn root_file(&self, repo: &str, commit: &str, name: &str) -> Result<Option<String>, String> {
        let path = format!("repos/{repo}/git/trees/{commit}");
        let tree: Tree = parse(&path, &Gh::api(&[&path])?)?;
        let Some(entry) = tree.tree.iter().find(|e| e.path == name && e.kind == BLOB) else {
            return Ok(None);
        };

        // The raw blob: a symlink's is its target's name, as `git show` prints it
        let blob = format!("repos/{repo}/git/blobs/{}", entry.sha);
        Gh::api(&["-H", "Accept: application/vnd.github.raw+json", &blob]).map(Some)
    }
}

fn parse<T: for<'a> Deserialize<'a>>(what: &str, json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("{what}: {e}"))
}
