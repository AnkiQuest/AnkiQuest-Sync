// SPDX-License-Identifier: AGPL-3.0-or-later

//! art-pin: the logic of AnkiQuest-Sync's art job (.github/workflows/art.yml).
//! The workflow does the network work with gh and git; this tool decides.
//!
//!   art-pin latest [<tag>]           releases JSON on stdin; prints the latest art-<N> tag, at least
//!                                    <tag> (a dispatched release), or nothing
//!   art-pin tag <tag>                fails unless <tag> is an art-<N> tag
//!   art-pin verify <pin> <sums> <tag>  checks a downloaded pin file against the release's SHA256SUMS
//!   art-pin status <new> <old>       prints "current" or "update"; <old> may not exist
//!   art-pin title <new>              the pull request title
//!   art-pin body <new> <old>         the pull request body

mod pin;
mod pr;
mod release;
mod sums;

use std::io::{ErrorKind, Read};
use std::process::ExitCode;
use std::{env, fs};

use pin::Pin;
use pr::Old;

const USAGE: &str = "usage: art-pin latest [<tag>] | tag <tag> | verify <pin> <sums> <tag> | status <new> <old> | title <new> | body <new> <old>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("art-pin: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs one command and returns what it prints.
fn run(args: &[&str]) -> Result<String, String> {
    match args {
        ["latest", rest @ ..] if rest.len() <= 1 => {
            let at_least = rest.first().copied().filter(|t| !t.is_empty());
            let mut json = String::new();
            std::io::stdin()
                .read_to_string(&mut json)
                .map_err(|e| format!("stdin: {e}"))?;
            Ok(release::latest(&json, at_least)?
                .map(|t| format!("{t}\n"))
                .unwrap_or_default())
        }
        ["tag", tag] => match pin::tag_number(tag) {
            Some(_) => Ok(format!("{tag}\n")),
            None => Err(format!("{tag:?} is not an art-<N> tag")),
        },
        ["verify", pin_path, sums_path, tag] => {
            let pin = read(pin_path)?;
            let sums =
                String::from_utf8(read(sums_path)?).map_err(|e| format!("{sums_path}: {e}"))?;
            sums::verify(&pin, &sums, tag)?;
            Ok(String::new())
        }
        ["status", new, old] => {
            let status = pr::status(&read(new)?, read_old(old)?.as_deref());
            Ok(format!("{}\n", status.name()))
        }
        ["title", new] => Ok(format!("{}\n", pr::title(&new_pin(new)?))),
        ["body", new, old] => {
            let old = Old::read(read_old(old)?.as_deref());
            Ok(pr::body(&new_pin(new)?, &old))
        }
        _ => Err(USAGE.to_owned()),
    }
}

fn read(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{path}: {e}"))
}

/// A consumer's pin file; None when it has none.
fn read_old(path: &str) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{path}: {e}")),
    }
}

/// The release's pin file, checked by `verify` already.
fn new_pin(path: &str) -> Result<Pin, String> {
    Pin::parse(&read(path)?)
}
