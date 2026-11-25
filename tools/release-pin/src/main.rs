// SPDX-License-Identifier: AGPL-3.0-or-later

//! release-pin: the logic of AnkiQuest-Sync's release pin jobs
//! (.github/workflows/pins.yml, called by art.yml and rules.yml). The
//! workflow does the network work with gh and git; this tool decides.
//! <kind> is `art` (AnkiQuest-Art) or `rules` (AnkiQuest-Server's rules).
//!
//!   release-pin <kind> latest [<tag>]           releases JSON on stdin; prints the latest tag of
//!                                               the kind, at least <tag> (a dispatched release),
//!                                               or nothing
//!   release-pin <kind> tag <tag>                fails unless <tag> is a tag of the kind
//!   release-pin <kind> verify <pin> <sums> <tag>  checks a downloaded pin file against the
//!                                               release's SHA256SUMS
//!   release-pin <kind> file                     the pin file's name
//!   release-pin <kind> repo                     the repository that publishes the releases
//!   release-pin <kind> status <new> <old>       prints "current" or "update"; <old> may not exist
//!   release-pin <kind> title <new>              the pull request title
//!   release-pin <kind> body <new> <old>         the pull request body

mod kind;
mod pin;
mod pr;
mod release;
mod sums;

use std::io::{ErrorKind, Read};
use std::process::ExitCode;
use std::{env, fs};

use kind::Kind;
use pin::Pin;
use pr::Old;

const USAGE: &str = "usage: release-pin art|rules latest [<tag>] | tag <tag> | verify <pin> <sums> <tag> | file | repo | status <new> <old> | title <new> | body <new> <old>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("release-pin: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs one command and returns what it prints.
fn run(args: &[&str]) -> Result<String, String> {
    let [kind, args @ ..] = args else {
        return Err(USAGE.to_owned());
    };
    let kind = Kind::parse(kind)?;
    match args {
        ["latest", rest @ ..] if rest.len() <= 1 => {
            let at_least = rest.first().copied().filter(|t| !t.is_empty());
            let mut json = String::new();
            std::io::stdin()
                .read_to_string(&mut json)
                .map_err(|e| format!("stdin: {e}"))?;
            Ok(release::latest(kind, &json, at_least)?
                .map(|t| format!("{t}\n"))
                .unwrap_or_default())
        }
        ["tag", tag] => match pin::tag_number(kind, tag) {
            Some(_) => Ok(format!("{tag}\n")),
            None => Err(pin::not_a_tag(kind, tag)),
        },
        ["file"] => Ok(format!("{}\n", kind.file())),
        ["repo"] => Ok(format!("{}\n", kind.repo())),
        ["verify", pin_path, sums_path, tag] => {
            let pin = read(pin_path)?;
            let sums =
                String::from_utf8(read(sums_path)?).map_err(|e| format!("{sums_path}: {e}"))?;
            sums::verify(kind, &pin, &sums, tag)?;
            Ok(String::new())
        }
        ["status", new, old] => {
            let status = pr::status(&read(new)?, read_old(old)?.as_deref());
            Ok(format!("{}\n", status.name()))
        }
        ["title", new] => Ok(format!("{}\n", pr::title(kind, &new_pin(kind, new)?))),
        ["body", new, old] => {
            let old = Old::read(kind, read_old(old)?.as_deref());
            Ok(pr::body(kind, &new_pin(kind, new)?, &old))
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
fn new_pin(kind: Kind, path: &str) -> Result<Pin, String> {
    Pin::parse(kind, &read(path)?)
}
