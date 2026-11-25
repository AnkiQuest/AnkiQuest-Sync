// SPDX-License-Identifier: AGPL-3.0-or-later

//! fork-sync: the logic of AnkiQuest-Sync's sync and agents jobs
//! (.github/workflows/sync.yml, agents.yml). The workflows do the network
//! work with gh and git; this tool merges and decides. Commands marked (git)
//! run in the current directory's checkout.
//!
//!   fork-sync title <upstream> <commit>          the sync pull request title
//!   fork-sync agents <shared> <section>          AGENTS.md of a repository without upstream
//!   fork-sync agents <shared> <section> keep <current>
//!                                                AGENTS.md keeping file <current>'s upstream guidance
//!   fork-sync agents <shared> <section> upstream <upstream> <dir>
//!                                                AGENTS.md with upstream's guidance from <dir>,
//!                                                which holds its CLAUDE.md and AGENTS.md
//!   fork-sync guidance <rev> <dir>               (git) writes <rev>'s CLAUDE.md and AGENTS.md to <dir>
//!   fork-sync merge <rev> <upstream> <agents>    (git) merges <rev> into HEAD with AGENTS.md from
//!                                                file <agents>; prints "merged" or "up-to-date"
//!   fork-sync body <upstream> <upstream-branch> <base> <rev> <branch>
//!                                                (git) the body of the pull request from
//!                                                <branch> (HEAD)

mod agents;
mod git;
mod guidance;
mod merge;
mod pr;
mod status;

use std::path::Path;
use std::process::ExitCode;
use std::{env, fs};

use git::Repo;
use guidance::Guidance;
use merge::Outcome;

const USAGE: &str = "usage: fork-sync title <upstream> <commit> | agents <shared> <section> [keep <current> | upstream <upstream> <dir>] | guidance <rev> <dir> | merge <rev> <upstream> <agents> | body <upstream> <upstream-branch> <base> <rev> <branch>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("fork-sync: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs one command and returns what it prints.
fn run(args: &[&str]) -> Result<String, String> {
    let here = Repo::new(Path::new("."));
    match args {
        ["title", upstream, commit] => Ok(format!("{}\n", pr::title(upstream, commit))),
        ["agents", shared, section] => Ok(agents::compose(&read(shared)?, &read(section)?, "")),
        ["agents", shared, section, "keep", current] => {
            // A repository without AGENTS.md yet keeps nothing
            let current = fs::read_to_string(current).unwrap_or_default();
            Ok(agents::compose(
                &read(shared)?,
                &read(section)?,
                guidance::of(&current),
            ))
        }
        ["agents", shared, section, "upstream", upstream, dir] => {
            let g = Guidance::read(upstream, Path::new(dir))?;
            Ok(agents::compose(
                &read(shared)?,
                &read(section)?,
                &g.section(),
            ))
        }
        ["guidance", rev, dir] => {
            write_guidance(&here, rev, Path::new(dir))?;
            Ok(String::new())
        }
        ["merge", rev, upstream, agents] => {
            match merge::merge(&here, rev, upstream, &read(agents)?)? {
                Outcome::UpToDate => Ok("up-to-date\n".to_owned()),
                Outcome::Merged(_) => Ok("merged\n".to_owned()),
            }
        }
        ["body", upstream, upstream_branch, base, rev, branch] => {
            let status = status::read(&here, base, rev)?;
            let names = pr::Names {
                upstream,
                upstream_branch,
                base,
                branch,
            };
            Ok(pr::body(&names, &status))
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

/// Upstream's guidance files at `rev`, as git has them (a symlink as its
/// target's name); a file upstream lacks is left out.
fn write_guidance(repo: &Repo, rev: &str, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for name in guidance::FILES {
        let path = dir.join(name);
        match repo.show(rev, name)? {
            Some(text) => fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?,
            None => {
                let _ = fs::remove_file(&path);
            }
        }
    }
    Ok(())
}
