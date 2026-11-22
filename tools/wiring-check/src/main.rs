// SPDX-License-Identifier: AGPL-3.0-or-later

//! wiring-check: the logic of AnkiQuest-Sync's wiring job
//! (.github/workflows/wiring.yml). Tells whether AnkiQuest.github.io's wiring
//! map is out of date, with no state of its own: the map records each
//! repository's head, the branch tip each pin was compared with and the
//! Maven releases it saw, and this compares them with the sources now.
//!
//!   wiring-check <map-repo> <wiring.json>...
//!
//! Reads the most recently generated of the given maps (missing or empty
//! files are skipped) and prints one line per change, or nothing.

mod check;
mod map;
mod remote;

use std::io::ErrorKind;
use std::process::ExitCode;
use std::{env, fs};

use check::Checker;
use map::Map;
use remote::Live;

const USAGE: &str = "usage: wiring-check <map-repo> <wiring.json>...";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("wiring-check: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String, String> {
    let [own, paths @ ..] = args else {
        return Err(USAGE.to_owned());
    };
    if paths.is_empty() {
        return Err(USAGE.to_owned());
    }

    // The newest map is the last build, merged or not
    let mut maps = Vec::new();
    for path in paths {
        if let Some(text) = read(path)? {
            maps.push(Map::parse(&text).map_err(|e| format!("{path}: {e}"))?);
        }
    }
    let map = Map::newest(maps).ok_or("no map to compare")?;

    let changes = Checker::new(&mut Live, own).changes(&map.checks());
    Ok(changes.iter().map(|c| format!("{c}\n")).collect())
}

/// A map file; None when it is missing or empty.
fn read(path: &str) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(None),
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{path}: {e}")),
    }
}
