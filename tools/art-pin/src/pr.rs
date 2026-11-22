// SPDX-License-Identifier: AGPL-3.0-or-later

//! Whether a consumer needs the new pin file, and the pull request that
//! brings it.

use std::collections::BTreeSet;

use crate::pin::{ART_REPO, FILE, Pin};

/// The pin file on a consumer's default branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Old {
    Missing,
    Unreadable,
    Pinned(Pin),
}

impl Old {
    /// The consumer's file: None if it has none.
    pub fn read(bytes: Option<&[u8]>) -> Old {
        match bytes.map(Pin::parse) {
            None => Old::Missing,
            Some(Err(_)) => Old::Unreadable,
            Some(Ok(pin)) => Old::Pinned(pin),
        }
    }
}

/// What a consumer needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Current,
    Update,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Current => "current",
            Status::Update => "update",
        }
    }
}

/// The release's pin file is copied as is, so any byte that differs,
/// or a missing file, needs a pull request.
pub fn status(new: &[u8], old: Option<&[u8]>) -> Status {
    if old == Some(new) {
        return Status::Current;
    }
    Status::Update
}

pub fn title(new: &Pin) -> String {
    format!("Use AnkiQuest-Art {}", tag(new))
}

/// The pull request body: the old and new tag, the Art commits between
/// them and every asset hash that changed.
pub fn body(new: &Pin, old: &Old) -> String {
    let new_tag = tag(new);
    let from = match old {
        Old::Missing => format!("no `{FILE}`"),
        Old::Unreadable => format!("an unreadable `{FILE}`"),
        Old::Pinned(Pin { tag: None, .. }) => "no release (`tag` is null)".to_owned(),
        Old::Pinned(Pin { tag: Some(t), .. }) => format!("`{t}`"),
    };
    let mut out = format!(
        "Moves `{FILE}` from {from} to [`{new_tag}`](https://github.com/{ART_REPO}/releases/tag/{new_tag}).\n\n"
    );

    out.push_str(&commits(new, old));
    out.push_str("\n\n");
    out.push_str(&assets(new, old));
    out.push_str(&format!(
        "\nBuilt by {ART_REPO}'s release workflow from art reviewed there. Change the art there, not here.\n\nOpened by AnkiQuest/AnkiQuest-Sync.\n"
    ));
    out
}

fn tag(pin: &Pin) -> &str {
    pin.tag.as_deref().unwrap_or("(no tag)")
}

/// The Art commit range, as a compare link where both ends are known.
fn commits(new: &Pin, old: &Old) -> String {
    let new_commit = new.commit.as_deref().unwrap_or_default();
    let old_commit = match old {
        Old::Pinned(pin) => pin.commit.as_deref(),
        _ => None,
    };
    match old_commit {
        Some(c) if c == new_commit => format!("Same Art commit, `{}`.", short(c)),
        Some(c) => format!("Art changes: https://github.com/{ART_REPO}/compare/{c}...{new_commit}"),
        None => format!("Art commit: https://github.com/{ART_REPO}/commit/{new_commit}"),
    }
}

/// A table of the asset hashes that changed.
fn assets(new: &Pin, old: &Old) -> String {
    let empty = Default::default();
    let old_assets = match old {
        Old::Pinned(pin) => &pin.assets,
        _ => &empty,
    };
    let names: BTreeSet<&String> = new.assets.keys().chain(old_assets.keys()).collect();
    let rows: Vec<String> = names
        .into_iter()
        .filter_map(|name| {
            let before = old_assets.get(name);
            let after = new.assets.get(name);
            if before == after {
                return None;
            }
            Some(format!(
                "| `{name}` | {} | {} |\n",
                cell(before, "none"),
                cell(after, "removed")
            ))
        })
        .collect();

    if rows.is_empty() {
        return "No asset changed.\n".to_owned();
    }
    let mut out = "| Asset | Old SHA-256 | New SHA-256 |\n|---|---|---|\n".to_owned();
    out.extend(rows);
    out
}

fn cell(hash: Option<&String>, absent: &str) -> String {
    hash.map_or_else(|| absent.to_owned(), |h| format!("`{h}`"))
}

fn short(commit: &str) -> &str {
    commit.get(..10).unwrap_or(commit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(tag: Option<&str>, commit: Option<&str>, assets: &[(&str, &str)]) -> Pin {
        Pin {
            tag: tag.map(str::to_owned),
            commit: commit.map(str::to_owned),
            assets: assets
                .iter()
                .map(|(n, h)| (n.to_string(), h.to_string()))
                .collect(),
        }
    }

    #[test]
    fn same_bytes_are_current() {
        assert_eq!(status(b"{}", Some(b"{}")), Status::Current);
    }

    #[test]
    fn missing_or_different_needs_update() {
        assert_eq!(status(b"{}", None), Status::Update);
        assert_eq!(status(b"{}\n", Some(b"{}")), Status::Update);
    }

    #[test]
    fn reads_old() {
        assert_eq!(Old::read(None), Old::Missing);
        assert_eq!(Old::read(Some(b"not json")), Old::Unreadable);
        assert!(matches!(
            Old::read(Some(br#"{"tag": null}"#)),
            Old::Pinned(_)
        ));
    }

    #[test]
    fn titles() {
        assert_eq!(
            title(&pin(Some("art-13"), None, &[])),
            "Use AnkiQuest-Art art-13"
        );
    }

    #[test]
    fn body_lists_range_and_changed_assets() {
        let old = pin(
            Some("art-12"),
            Some("aaa"),
            &[("masters.zip", "1"), ("poses.json", "2")],
        );
        let new = pin(
            Some("art-13"),
            Some("bbb"),
            &[("masters.zip", "3"), ("poses.json", "2")],
        );
        let text = body(&new, &Old::Pinned(old));
        assert!(text.starts_with("Moves `ankiquest-art.json` from `art-12` to [`art-13`]"));
        assert!(text.contains("https://github.com/AnkiQuest/AnkiQuest-Art/compare/aaa...bbb"));
        assert!(text.contains("| `masters.zip` | `1` | `3` |"));
        assert!(!text.contains("poses.json"));
    }

    #[test]
    fn body_from_null_tag() {
        let old = pin(None, None, &[]);
        let new = pin(Some("art-1"), Some("bbb"), &[("masters.zip", "3")]);
        let text = body(&new, &Old::Pinned(old));
        assert!(text.contains("from no release (`tag` is null)"));
        assert!(text.contains("/commit/bbb"));
        assert!(text.contains("| `masters.zip` | none | `3` |"));
    }

    #[test]
    fn body_from_missing_file() {
        let new = pin(Some("art-1"), Some("bbb"), &[("masters.zip", "3")]);
        assert!(body(&new, &Old::Missing).contains("from no `ankiquest-art.json`"));
        assert!(body(&new, &Old::Unreadable).contains("from an unreadable"));
    }

    #[test]
    fn body_same_release() {
        // A hand-edited copy of the same release: nothing in Art changed
        let new = pin(
            Some("art-13"),
            Some("0123456789abcdef"),
            &[("masters.zip", "3")],
        );
        let text = body(&new, &Old::Pinned(new.clone()));
        assert!(text.contains("Same Art commit, `0123456789`."));
        assert!(text.contains("No asset changed."));
    }

    #[test]
    fn body_removed_asset() {
        let old = pin(Some("art-1"), Some("a"), &[("old.zip", "1")]);
        let new = pin(Some("art-2"), Some("b"), &[]);
        assert!(body(&new, &Old::Pinned(old)).contains("| `old.zip` | `1` | removed |"));
    }
}
