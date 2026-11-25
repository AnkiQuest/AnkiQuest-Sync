// SPDX-License-Identifier: AGPL-3.0-or-later

//! What the sync branch holds compared with the fork's branch: read from
//! git on every run, so the pull request describes the whole branch, not
//! only the last merge.

use crate::git::Repo;
use crate::guidance;

/// Whether the branch changes the upstream guidance in AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guidance {
    Same,
    Changed,
}

/// A submodule upstream moved that the branch doesn't have at upstream's commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    pub path: String,
    pub ours: Option<String>,
    pub theirs: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The upstream commit merged last.
    pub commit: String,
    /// Upstream commits the fork's branch doesn't have yet.
    pub commits: u64,
    /// Files the branch adds conflict markers to.
    pub markers: Vec<String>,
    pub submodules: Vec<Submodule>,
    pub guidance: Guidance,
}

/// The sync branch (HEAD) against `base`, the fork's branch, after merging `rev`.
pub fn read(repo: &Repo, base: &str, rev: &str) -> Result<Status, String> {
    let commit = repo.commit(rev)?;
    let commits = repo
        .run(&["rev-list", "--count", &format!("{base}..{commit}")])?
        .trim()
        .parse()
        .map_err(|e| format!("rev-list --count: {e}"))?;

    let old = repo.show(base, "AGENTS.md")?.unwrap_or_default();
    let new = repo.show("HEAD", "AGENTS.md")?.unwrap_or_default();
    let guidance = if guidance::of(&old) == guidance::of(&new) {
        Guidance::Same
    } else {
        Guidance::Changed
    };

    Ok(Status {
        markers: markers(repo, base)?,
        submodules: submodules(repo, base, &commit)?,
        commit,
        commits,
        guidance,
    })
}

/// Files whose lines added on the branch include a conflict marker.
fn markers(repo: &Repo, base: &str) -> Result<Vec<String>, String> {
    let diff = repo.run(&[
        "diff",
        "-U0",
        "--no-color",
        "--no-ext-diff",
        &format!("{base}...HEAD"),
    ])?;
    let mut files: Vec<String> = Vec::new();
    let mut current: Option<&str> = None;
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current = Some(path);
            continue;
        }
        if line.starts_with("+++ ") {
            current = None;
            continue;
        }
        let marker = line.starts_with("+<<<<<<< ") || line.starts_with("+>>>>>>> ");
        let Some(path) = current.filter(|_| marker) else {
            continue;
        };
        if !files.iter().any(|f| f == path) {
            files.push(path.to_owned());
        }
    }
    Ok(files)
}

/// Submodules upstream moved since the fork's last merge that the branch
/// has at another commit.
fn submodules(repo: &Repo, base: &str, commit: &str) -> Result<Vec<Submodule>, String> {
    let merged = repo.run(&["merge-base", base, commit])?;
    let before = repo.gitlinks(merged.trim())?;
    let ours = repo.gitlinks("HEAD")?;
    let mut out = Vec::new();
    for (path, theirs) in repo.gitlinks(commit)? {
        let moved = !before.iter().any(|(p, sha)| *p == path && *sha == theirs);
        let ours = ours
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, sha)| sha.clone());
        if moved && ours.as_deref() != Some(theirs.as_str()) {
            out.push(Submodule { path, ours, theirs });
        }
    }
    Ok(out)
}
