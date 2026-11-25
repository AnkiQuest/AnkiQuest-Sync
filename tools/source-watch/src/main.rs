// SPDX-License-Identifier: AGPL-3.0-or-later

//! source-watch: the watch job of AnkiQuest-Sync (.github/workflows/sync.yml).
//!
//!   source-watch <watched.json> <fallback> <summary>
//!
//! Reports each entry of <watched.json> in an issue in its target, or in
//! <fallback> (AnkiQuest-Sync) where the target has issues disabled; appends
//! one line per entry to the file <summary>. Every entry is checked even when
//! one fails; the command fails at the end, naming the failed entries.
//! GH_TOKEN reads the sources and writes the targets' issues; FALLBACK_TOKEN,
//! if set, writes <fallback>'s.

mod github;
mod report;
mod watched;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::ExitCode;

use github::Gh;

const USAGE: &str = "usage: source-watch <watched.json> <fallback> <summary>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let [watched, fallback, summary] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    match run(watched, fallback, summary) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            println!("::error::{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(watched: &str, fallback: &str, summary_path: &str) -> Result<(), String> {
    let text = fs::read_to_string(watched).map_err(|e| format!("{watched}: {e}"))?;
    let entries = watched::parse(&text)?;
    let own = env::var("FALLBACK_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
        .map(|token| (fallback.to_owned(), token));
    let summary = report::run(&Gh::new(own), fallback, &entries);

    // Annotations go to stdout, where Actions reads them
    for warning in &summary.warnings {
        println!("::warning::{warning}");
    }
    for (label, error) in &summary.failures {
        println!("::error::{label}: {error}");
    }

    let mut lines: Vec<String> = summary.lines.clone();
    lines.extend(
        summary
            .failures
            .iter()
            .map(|(label, _)| format!("{label}: failed")),
    );
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
