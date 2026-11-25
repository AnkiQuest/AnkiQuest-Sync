// SPDX-License-Identifier: AGPL-3.0-or-later

//! The commands as art.yml runs them, on fixtures that follow the contract:
//! a consumer on art-12 and the art-13 release, which changes masters.zip
//! and poses.json.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_art-pin");

fn fixture(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "fixtures", name]
        .iter()
        .collect();
    path.to_string_lossy().into_owned()
}

/// Runs art-pin; returns whether it succeeded and its stdout.
fn run(args: &[&str], stdin: Option<&str>) -> (bool, String) {
    let input = match stdin {
        Some(name) => Stdio::from(fs::File::open(fixture(name)).unwrap()),
        None => Stdio::null(),
    };
    let out = Command::new(BIN).args(args).stdin(input).output().unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn no_release() {
    assert_eq!(
        run(&["latest"], Some("releases-none.json")),
        (true, String::new())
    );
}

#[test]
fn latest_skips_drafts() {
    assert_eq!(
        run(&["latest"], Some("releases.json")),
        (true, "art-13\n".into())
    );
}

#[test]
fn latest_with_dispatched_tag() {
    let (ok, out) = run(&["latest", "art-14"], Some("releases.json"));
    assert_eq!(
        (ok, out.as_str()),
        (
            true, "art-14
"
        )
    );
    let (ok, out) = run(&["latest", "art-12"], Some("releases.json"));
    assert_eq!(
        (ok, out.as_str()),
        (
            true, "art-13
"
        )
    );
    let (ok, out) = run(&["latest", ""], Some("releases-none.json"));
    assert_eq!((ok, out.as_str()), (true, ""));
}

#[test]
fn tag_check() {
    assert!(run(&["tag", "art-13"], None).0);
    assert!(!run(&["tag", "art-13; rm -rf /"], None).0);
    assert!(!run(&["tag", ""], None).0);
}

#[test]
fn verifies_release() {
    let (pin, sums) = (fixture("art-13.json"), fixture("SHA256SUMS"));
    assert!(run(&["verify", &pin, &sums, "art-13"], None).0);
    assert!(!run(&["verify", &pin, &sums, "art-12"], None).0);

    // The consumer's art-12 file is not what SHA256SUMS lists
    assert!(!run(&["verify", &fixture("art-12.json"), &sums, "art-12"], None).0);
}

#[test]
fn same_tag_is_current() {
    let new = fixture("art-13.json");
    assert_eq!(
        run(&["status", &new, &new], None),
        (true, "current\n".into())
    );
}

#[test]
fn older_tag_needs_update() {
    let (new, old) = (fixture("art-13.json"), fixture("art-12.json"));
    assert_eq!(
        run(&["status", &new, &old], None),
        (true, "update\n".into())
    );
}

#[test]
fn missing_file_needs_update() {
    let new = fixture("art-13.json");
    let (ok, out) = run(&["status", &new, &fixture("does-not-exist.json")], None);
    assert_eq!((ok, out.as_str()), (true, "update\n"));
}

#[test]
fn title() {
    let (_, out) = run(&["title", &fixture("art-13.json")], None);
    assert_eq!(out, "chore(art): use AnkiQuest-Art art-13\n");
}

#[test]
fn body_from_art_12() {
    let (ok, body) = run(
        &["body", &fixture("art-13.json"), &fixture("art-12.json")],
        None,
    );
    assert!(ok);
    assert!(body.starts_with("Moves `ankiquest-art.json` from `art-12` to [`art-13`](https://github.com/AnkiQuest/AnkiQuest-Art/releases/tag/art-13)."));
    assert!(body.contains(
        "https://github.com/AnkiQuest/AnkiQuest-Art/compare/1f0e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c...9a8b7c6d5e4f30211203f4e5d6c7b8a99a8b7c6d"
    ));

    // Only the two assets that changed, with both hashes
    let rows: Vec<&str> = body.lines().filter(|l| l.starts_with("| `")).collect();
    assert_eq!(rows.len(), 2, "{body}");
    assert!(rows[0].starts_with("| `masters.zip` | `"));
    assert!(rows[0].contains("`33533edeee1f23bf0a8e49c08e45edbc10d58d760c1c91cd4da573719cd5e1cf`"));
    assert!(rows[1].starts_with("| `poses.json` | `"));
}

#[test]
fn body_from_null_tag() {
    let (_, body) = run(
        &["body", &fixture("art-13.json"), &fixture("null.json")],
        None,
    );
    assert!(body.contains("from no release (`tag` is null)"));
    assert_eq!(body.lines().filter(|l| l.starts_with("| `")).count(), 6);
}
