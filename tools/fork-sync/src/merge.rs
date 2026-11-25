// SPDX-License-Identifier: AGPL-3.0-or-later

//! Merges an upstream commit into the checked-out sync branch. Conflicts
//! never stop the merge: files are committed with their markers and
//! submodules keep our commit, so people resolve them on the branch.
//! AGENTS.md and CLAUDE.md are always written from AnkiQuest-Agents and
//! the merged upstream's guidance.

use std::collections::BTreeMap;
use std::fs;

use crate::agents::CLAUDE;
use crate::git::{Failure, GITLINK, Repo};
use crate::pr;

/// The generated files; a conflict in them is resolved by writing them.
const GENERATED: [&str; 2] = ["AGENTS.md", "CLAUDE.md"];

/// What the merge did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The branch already has the upstream commit.
    UpToDate,
    /// A merge commit was made; it lists the conflicts it committed.
    Merged(Vec<Conflict>),
}

/// A path git could not merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// Committed as the work tree has it: with markers for text.
    File(String),
    /// Kept at our commit; upstream's is `theirs` (None: upstream removed it).
    Submodule {
        path: String,
        theirs: Option<String>,
    },
}

impl Conflict {
    fn line(&self) -> String {
        match self {
            Conflict::File(path) => format!("- {path}"),
            Conflict::Submodule {
                path,
                theirs: Some(sha),
            } => {
                format!("- {path} (submodule kept at ours; upstream: {sha})")
            }
            Conflict::Submodule { path, theirs: None } => {
                format!("- {path} (submodule kept; upstream removed it)")
            }
        }
    }
}

/// Merges `rev` (upstream's commit) into HEAD with a merge commit.
pub fn merge(repo: &Repo, rev: &str, upstream: &str, agents: &str) -> Result<Outcome, String> {
    let commit = repo.commit(rev)?;
    if repo.is_ancestor(&commit, "HEAD")? {
        return Ok(Outcome::UpToDate);
    }
    let count: u64 = repo
        .run(&["rev-list", "--count", &format!("HEAD..{commit}")])?
        .trim()
        .parse()
        .map_err(|e| format!("rev-list --count: {e}"))?;

    // A conflicting merge fails here but leaves MERGE_HEAD; anything else is an error
    let merged = repo.status(
        &["merge", "--no-ff", "--no-commit", &commit],
        Failure::Answer,
    )?;
    if merged.is_none()
        && repo
            .status(
                &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
                Failure::Answer,
            )?
            .is_none()
    {
        return Err(format!("git merge {commit} failed before merging"));
    }

    // Resolve everything except the generated files, which are written below
    let mut conflicts = Vec::new();
    for (path, stages) in unmerged(repo)? {
        if GENERATED.contains(&path.as_str()) {
            continue;
        }
        conflicts.push(resolve(repo, &path, &stages)?);
    }

    // Upstream may make AGENTS.md a symlink; ours is always a file
    for name in GENERATED {
        let path = repo.dir().join(name);
        if path.symlink_metadata().is_ok() {
            fs::remove_file(&path).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    fs::write(repo.dir().join("AGENTS.md"), agents).map_err(|e| format!("AGENTS.md: {e}"))?;
    fs::write(repo.dir().join("CLAUDE.md"), CLAUDE).map_err(|e| format!("CLAUDE.md: {e}"))?;
    repo.run(&["add", "-A", "--", "AGENTS.md", "CLAUDE.md"])?;

    let subject = pr::title(upstream, &commit);
    let mut body = format!("Brings {} from {upstream}.", pr::commits(count));
    if !conflicts.is_empty() {
        let lines: Vec<String> = conflicts.iter().map(Conflict::line).collect();
        body.push_str(&format!(
            "\n\nConflicts, committed unresolved:\n{}",
            lines.join("\n")
        ));
    }
    repo.run(&["commit", "-q", "--no-verify", "-m", &subject, "-m", &body])?;
    Ok(Outcome::Merged(conflicts))
}

/// One index entry of an unmerged path: its mode and object per stage
/// (1 base, 2 ours, 3 theirs).
struct Stage {
    mode: String,
    object: String,
    number: u8,
}

fn unmerged(repo: &Repo) -> Result<BTreeMap<String, Vec<Stage>>, String> {
    let list = repo.run(&["ls-files", "-u", "-z"])?;
    let mut paths: BTreeMap<String, Vec<Stage>> = BTreeMap::new();
    for entry in list.split('\0').filter(|e| !e.is_empty()) {
        let Some((meta, path)) = entry.split_once('\t') else {
            return Err(format!("unexpected ls-files entry {entry:?}"));
        };
        let fields: Vec<&str> = meta.split(' ').collect();
        let [mode, object, number] = fields[..] else {
            return Err(format!("unexpected ls-files entry {entry:?}"));
        };
        let number = number
            .parse()
            .map_err(|_| format!("unexpected stage in {entry:?}"))?;
        paths.entry(path.to_owned()).or_default().push(Stage {
            mode: mode.to_owned(),
            object: object.to_owned(),
            number,
        });
    }
    Ok(paths)
}

/// Stages one conflicted path so the merge can be committed.
fn resolve(repo: &Repo, path: &str, stages: &[Stage]) -> Result<Conflict, String> {
    let stage = |n: u8| stages.iter().find(|s| s.number == n);

    // Files: commit the work tree's version, markers and all
    if !stages.iter().any(|s| s.mode == GITLINK) {
        repo.run(&["add", "-A", "--", path])?;
        return Ok(Conflict::File(path.to_owned()));
    }

    // Submodules: the sync job checks none out, so keep our commit
    let theirs = stage(3).map(|s| s.object.clone());
    match stage(2) {
        Some(ours) => {
            let info = format!("{},{},{path}", ours.mode, ours.object);
            repo.run(&["update-index", "--cacheinfo", &info])?;
        }
        None => {
            repo.run(&["update-index", "--force-remove", "--", path])?;
        }
    }
    Ok(Conflict::Submodule {
        path: path.to_owned(),
        theirs,
    })
}
