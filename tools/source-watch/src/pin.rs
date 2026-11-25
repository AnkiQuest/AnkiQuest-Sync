// SPDX-License-Identifier: AGPL-3.0-or-later

//! A git dependency pinned by commit in a Cargo.toml: reading the pin,
//! moving it, and the pull request that moves it.

use crate::github::{Compare, LISTED_COMMITS, short};

/// The manifest that pins the source, and the lock file `cargo update` rewrites.
pub const MANIFEST: &str = "Cargo.toml";
pub const LOCK: &str = "Cargo.lock";

/// The lines of `text` that describe the dependency on `source`: the
/// inline table on the line naming its git URL, or the whole `[...]` table
/// that line is in.
fn table(lines: &[&str], source: &str) -> Option<(usize, usize)> {
    let url = format!("\"https://github.com/{source}");
    let at = lines.iter().position(|line| {
        let line = line.trim_start();
        line.contains(&url) && (line.starts_with("git") || line.contains("git ="))
    })?;
    if lines[at].contains("rev") {
        return Some((at, at + 1));
    }
    let header = |line: &&str| line.trim_start().starts_with('[');
    let start = lines[..at].iter().rposition(header).map_or(0, |i| i + 1);
    let end = lines[at + 1..]
        .iter()
        .position(header)
        .map_or(lines.len(), |i| at + 1 + i);
    Some((start, end))
}

/// The byte range of the value of `rev = "<value>"` on a line.
fn rev(line: &str) -> Option<(usize, usize)> {
    line.match_indices("rev").find_map(|(key, _)| {
        // A key of its own, not part of a longer word
        let before = line[..key].chars().next_back();
        if before.is_some_and(|c| !(c.is_whitespace() || c == '{' || c == ',')) {
            return None;
        }
        let rest = &line[key + "rev".len()..];
        let open = rest.find('"')?;
        if rest[..open].trim() != "=" {
            return None;
        }
        let start = key + "rev".len() + open + 1;
        let len = line[start..].find('"')?;
        Some((start, start + len))
    })
}

/// The commit `manifest` pins `source` at.
pub fn pinned(manifest: &str, source: &str) -> Option<String> {
    let lines: Vec<&str> = manifest.lines().collect();
    let (start, end) = table(&lines, source)?;
    lines[start..end].iter().find_map(|line| {
        let (from, to) = rev(line)?;
        Some(line[from..to].to_owned())
    })
}

/// `manifest` with `source` pinned at `commit` instead.
pub fn set(manifest: &str, source: &str, commit: &str) -> Result<String, String> {
    let mut lines: Vec<String> = manifest.lines().map(str::to_owned).collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (start, end) = table(&borrowed, source)
        .ok_or_else(|| format!("{MANIFEST} has no git dependency on {source}"))?;
    let found = (start..end).find_map(|i| rev(&lines[i]).map(|range| (i, range)));
    let Some((i, (from, to))) = found else {
        return Err(format!("{MANIFEST} pins no commit of {source}"));
    };
    lines[i].replace_range(from..to, commit);

    let mut out = lines.join("\n");
    if manifest.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// The pull request title, also its commit's subject.
pub fn title(package: &str, source: &str, commit: &str) -> String {
    format!("chore(deps): bump {package} to {source} {}", short(commit))
}

/// What the pull request body needs.
pub struct Bump<'a> {
    pub package: &'a str,
    pub source: &'a str,
    pub rev: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub branch: &'a str,
    /// The commits between the two pins.
    pub compare: &'a Compare,
}

pub fn body(bump: &Bump) -> String {
    let Bump {
        package,
        source,
        rev,
        from,
        to,
        branch,
        compare,
    } = bump;
    format!(
        "Moves `{package}` in `{MANIFEST}` and `{LOCK}` from `{}` to `{}`, the head of \
         [{source}](https://github.com/{source}) `{rev}`: {} new commits \
         (latest {LISTED_COMMITS} below), https://github.com/{source}/compare/{from}...{to}\n\n\
         {}\n\n\
         CI here builds against the new commit. Fix what it breaks on `{branch}`; \
         the next run moves the pin on top of the fixes.\n\n\
         Opened by AnkiQuest/AnkiQuest-Sync.",
        short(from),
        short(to),
        compare.total,
        compare.listing()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Commit;

    const OLD: &str = "bd8932bb33d68bf0ac0e4367b154eec837f7fad2";
    const NEW: &str = "0123456789abcdef0123456789abcdef01234567";

    /// AnkiQuest-Anki's Cargo.toml has the pin in a table of its own.
    const TABLE: &str = "[workspace.dependencies.linkcheck]\n\
        git = \"https://github.com/ankitects/linkcheck.git\"\n\
        rev = \"2c7c081b148f8c1053df33768bc99f05311cb9a0\"\n\
        \n\
        [workspace.dependencies.fsrs]\n\
        git = \"https://github.com/JSchoreels/fsrs-rs.git\"\n\
        rev = \"bd8932bb33d68bf0ac0e4367b154eec837f7fad2\"\n\
        \n\
        [workspace.dependencies]\n\
        anki = { path = \"rslib\" }\n";

    #[test]
    fn reads_the_pin_in_its_table() {
        assert_eq!(pinned(TABLE, "JSchoreels/fsrs-rs").as_deref(), Some(OLD));
        assert_eq!(
            pinned(TABLE, "ankitects/linkcheck").as_deref(),
            Some("2c7c081b148f8c1053df33768bc99f05311cb9a0")
        );
        assert_eq!(pinned(TABLE, "open-spaced-repetition/fsrs-rs"), None);
    }

    #[test]
    fn moves_only_that_pin() {
        let moved = set(TABLE, "JSchoreels/fsrs-rs", NEW).unwrap();
        assert_eq!(moved, TABLE.replace(OLD, NEW));
        assert_eq!(
            pinned(&moved, "ankitects/linkcheck").as_deref(),
            Some("2c7c081b148f8c1053df33768bc99f05311cb9a0")
        );
    }

    #[test]
    fn inline_table() {
        let text = format!(
            "[dependencies]\nfsrs = {{ git = \"https://github.com/JSchoreels/fsrs-rs.git\", rev = \"{OLD}\" }}\nprevious = {{ git = \"https://github.com/a/prev.git\", rev = \"x\" }}\n"
        );
        assert_eq!(pinned(&text, "JSchoreels/fsrs-rs").as_deref(), Some(OLD));
        assert_eq!(pinned(&text, "a/prev").as_deref(), Some("x"));
        assert_eq!(
            set(&text, "JSchoreels/fsrs-rs", NEW).unwrap(),
            text.replace(OLD, NEW)
        );
    }

    #[test]
    fn missing_pin_is_an_error() {
        let branch = "[dependencies.fsrs]\ngit = \"https://github.com/JSchoreels/fsrs-rs.git\"\nbranch = \"main\"\n";
        assert_eq!(pinned(branch, "JSchoreels/fsrs-rs"), None);
        assert_eq!(
            set(branch, "JSchoreels/fsrs-rs", NEW).unwrap_err(),
            "Cargo.toml pins no commit of JSchoreels/fsrs-rs"
        );
        assert_eq!(
            set("", "JSchoreels/fsrs-rs", NEW).unwrap_err(),
            "Cargo.toml has no git dependency on JSchoreels/fsrs-rs"
        );
    }

    #[test]
    fn title_and_body() {
        assert_eq!(
            title("fsrs", "JSchoreels/fsrs-rs", NEW),
            "chore(deps): bump fsrs to JSchoreels/fsrs-rs 0123456789"
        );
        let commits: Vec<Commit> = (0..60)
            .map(|n| Commit {
                sha: format!("{n:02}{NEW}"),
                subject: format!("commit {n}"),
            })
            .collect();
        let b = body(&Bump {
            package: "fsrs",
            source: "JSchoreels/fsrs-rs",
            rev: "main",
            from: OLD,
            to: NEW,
            branch: "sync/JSchoreels/fsrs-rs-main",
            compare: &Compare { total: 60, commits },
        });
        assert!(b.starts_with(
            "Moves `fsrs` in `Cargo.toml` and `Cargo.lock` from `bd8932bb33` to `0123456789`"
        ));
        assert!(b.contains("60 new commits (latest 50 below)"));
        assert!(b.contains("- 1001234567 commit 10\n"));
        assert!(!b.contains("commit 9\n"));
        assert!(b.contains("Fix what it breaks on `sync/JSchoreels/fsrs-rs-main`"));
        assert!(b.ends_with("Opened by AnkiQuest/AnkiQuest-Sync."));
    }
}
