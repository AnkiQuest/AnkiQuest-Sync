// SPDX-License-Identifier: AGPL-3.0-or-later

//! source-watch: the watched sources of AnkiQuest-Sync's sync and watch
//! jobs (.github/workflows/sync.yml).
//!
//!   source-watch matrix <forks.json> <watched.json>
//!   source-watch report <watched.json> <fallback> <summary> <workdir>
//!   source-watch mirror <mirrors.json> <base> <summary> <workdir>
//!   source-watch land <repo> <fallback> <title> <message> <body> <summary> <workdir> <path>=<file>...
//!
//! matrix prints the sync job's matrix as JSON: each fork's upstream and
//! each watched source in mode "merge".
//!
//! report acts on each entry of <watched.json> by its mode (see report.rs):
//! an issue in its target, or in <fallback> (AnkiQuest-Sync) where the
//! target has issues disabled, for "port"; a pull request moving the pin,
//! prepared in the directory <workdir>, for "pin". It appends one line per
//! entry to the file <summary>. Every entry is checked even when one fails;
//! the command fails at the end, naming the failed entries. GH_TOKEN reads
//! the sources and writes the targets' issues, branches and pull requests;
//! FALLBACK_TOKEN, if set, writes <fallback>'s issues.
//!
//! mirror moves each branch of <mirrors.json> to its source's commit (see
//! mirror.rs), fetching and pushing with git in the bare repository
//! <workdir>; repositories are <base>/<owner>/<repo>.git (https://github.com
//! in the job). Like report, it appends to <summary>, checks every entry and
//! fails at the end naming the failed ones.
//!
//! land puts each <file> at <path> on <repo>'s default branch (see land.rs):
//! one commit, <title> and <message>, pushed as a fast-forward; if refused,
//! the pull request from <fallback>, titled <title> with the text of file
//! <body>, with auto-merge on. It clones into <workdir> and appends one line
//! to <summary>. The agents and art jobs run it for each repository.

mod github;
mod land;
mod matrix;
mod mirror;
mod pin;
mod report;
mod watched;
mod workspace;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use github::Gh;
use workspace::Checkout;

const USAGE: &str = "usage: source-watch matrix <forks.json> <watched.json> | report <watched.json> <fallback> <summary> <workdir> | mirror <mirrors.json> <base> <summary> <workdir> | land <repo> <fallback> <title> <message> <body> <summary> <workdir> <path>=<file>...";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["matrix", forks, watched] => matrix(forks, watched),
        ["report", watched, fallback, summary, workdir] => {
            report(watched, fallback, summary, workdir)
        }
        ["mirror", mirrors, base, summary, workdir] => mirror(mirrors, base, summary, workdir),
        [
            "land",
            repo,
            fallback,
            title,
            message,
            body,
            summary,
            workdir,
            files @ ..,
        ] if !files.is_empty() => {
            let names = [*repo, *fallback, *title, *message, *body];
            change(names, files).and_then(|c| land(&c, summary, workdir))
        }
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            println!("::error::{message}");
            ExitCode::FAILURE
        }
    }
}

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn matrix(forks: &str, watched: &str) -> Result<(), String> {
    let entries = watched::parse(&read(watched)?)?;
    let rows = matrix::rows(&read(forks)?, &entries)?;
    let json = serde_json::to_string(&rows).map_err(|e| format!("matrix: {e}"))?;
    println!("{json}");
    Ok(())
}

fn report(watched: &str, fallback: &str, summary_path: &str, workdir: &str) -> Result<(), String> {
    let entries = watched::parse(&read(watched)?)?;
    let own = env::var("FALLBACK_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
        .map(|token| (fallback.to_owned(), token));
    let work = Checkout::new(PathBuf::from(workdir));
    let summary = report::run(&Gh::new(own), &work, fallback, &entries);

    // Annotations go to stdout, where Actions reads them
    for warning in &summary.warnings {
        println!("::warning::{warning}");
    }
    for (label, error) in &summary.failures {
        println!("::error::{label}: {error}");
    }

    // The summary already has a "failed" line for each failed entry
    append(summary_path, &summary.lines)?;
    let failed: Vec<&str> = summary.failures.iter().map(|(l, _)| l.as_str()).collect();
    failures(&failed, entries.len(), "sources")
}

fn mirror(mirrors: &str, base: &str, summary_path: &str, workdir: &str) -> Result<(), String> {
    let mirrors = mirror::parse(&read(mirrors)?)?;
    let remote = mirror::Remote::new(base, Path::new(workdir))?;

    // Every mirror is tried; a failed one is named in an annotation and the summary
    let mut lines = Vec::new();
    let mut failed = Vec::new();
    for entry in &mirrors {
        match remote.update(entry) {
            Ok(outcome) => lines.push(mirror::line(entry, &outcome)),
            Err(error) => {
                println!("::error::{}: {error}", entry.label());
                lines.push(format!("{}: failed", entry.label()));
                failed.push(entry.label());
            }
        }
    }
    append(summary_path, &lines)?;
    let failed: Vec<&str> = failed.iter().map(String::as_str).collect();
    failures(&failed, mirrors.len(), "mirrors")
}

/// The change `land` makes: repository, fallback branch, title, message,
/// body file and `<path>=<file>` arguments.
fn change(names: [&str; 5], files: &[&str]) -> Result<land::Change, String> {
    let [repo, fallback, title, message, body] = names;
    Ok(land::Change {
        repo: repo.to_owned(),
        fallback: fallback.to_owned(),
        title: title.to_owned(),
        message: message.to_owned(),
        body: read(body)?,
        files: files.iter().map(|f| file(f)).collect::<Result<_, _>>()?,
    })
}

/// A `<path>=<file>` argument: the repository path and the file's text.
fn file(arg: &str) -> Result<(String, String), String> {
    let (path, local) = arg
        .split_once('=')
        .ok_or_else(|| format!("{arg}: expected <path>=<file>"))?;
    Ok((path.to_owned(), read(local)?))
}

fn land(change: &land::Change, summary: &str, workdir: &str) -> Result<(), String> {
    let paths: Vec<&str> = change.files.iter().map(|(p, _)| p.as_str()).collect();
    let work = Checkout::sparse(PathBuf::from(workdir), &paths);
    let result = land::land(&Gh::new(None), &work, change);
    let line = match &result {
        Ok(outcome) => land::line(&change.repo, outcome),
        Err(_) => format!("{}: failed", change.repo),
    };
    append(summary, &[line])?;
    result
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", change.repo))
}

/// Appends lines to the job summary file.
fn append(path: &str, lines: &[String]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("{path}: {e}"))?;
    for line in lines {
        writeln!(file, "{line}").map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(())
}

/// Ok when nothing failed, otherwise an error naming what did.
fn failures(failed: &[&str], total: usize, what: &str) -> Result<(), String> {
    if failed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} of {total} {what} failed: {}",
        failed.len(),
        failed.join(", ")
    ))
}
