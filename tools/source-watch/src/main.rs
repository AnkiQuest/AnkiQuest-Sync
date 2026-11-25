// SPDX-License-Identifier: AGPL-3.0-or-later

//! source-watch: the watched sources of AnkiQuest-Sync's sync and watch
//! jobs (.github/workflows/sync.yml).
//!
//!   source-watch matrix <forks.json> <watched.json>
//!   source-watch report <watched.json> <fallback> <summary> <workdir>
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

mod github;
mod matrix;
mod pin;
mod report;
mod watched;
mod workspace;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use github::Gh;
use workspace::Checkout;

const USAGE: &str = "usage: source-watch matrix <forks.json> <watched.json> | report <watched.json> <fallback> <summary> <workdir>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["matrix", forks, watched] => matrix(forks, watched),
        ["report", watched, fallback, summary, workdir] => {
            report(watched, fallback, summary, workdir)
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
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(summary_path)
        .map_err(|e| format!("{summary_path}: {e}"))?;
    for line in &summary.lines {
        writeln!(file, "{line}").map_err(|e| format!("{summary_path}: {e}"))?;
    }

    if summary.failures.is_empty() {
        return Ok(());
    }
    let failed: Vec<&str> = summary.failures.iter().map(|(l, _)| l.as_str()).collect();
    Err(format!(
        "{} of {} sources failed: {}",
        failed.len(),
        entries.len(),
        failed.join(", ")
    ))
}
