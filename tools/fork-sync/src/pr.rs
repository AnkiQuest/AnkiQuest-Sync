// SPDX-License-Identifier: AGPL-3.0-or-later

//! The title and body of a sync pull request.

use crate::status::{Guidance, Status};

/// Characters of a commit shown in titles and bodies.
const SHORT: usize = 10;
/// The fewest characters a title shortens a commit to.
const SHORTEST: usize = 7;
/// The longest title the commit conventions allow.
const TITLE_MAX: usize = 72;

/// The pull request title, also the merge commit's subject. A long source
/// name shortens the commit, down to 7 characters, to keep it within 72.
pub fn title(upstream: &str, commit: &str) -> String {
    let prefix = format!("chore(sync): merge upstream {upstream} at ");
    let room = TITLE_MAX
        .saturating_sub(prefix.len())
        .clamp(SHORTEST, SHORT);
    format!("{prefix}{}", &commit[..commit.len().min(room)])
}

/// "1 new commit", "12 new commits".
pub fn commits(count: u64) -> String {
    if count == 1 {
        return "1 new commit".to_owned();
    }
    format!("{count} new commits")
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(SHORT)]
}

/// Names the body needs besides the branch's status.
pub struct Names<'a> {
    pub upstream: &'a str,
    pub upstream_branch: &'a str,
    pub base: &'a str,
    /// The sync branch the pull request is opened from.
    pub branch: &'a str,
}

/// The pull request body: what it brings, how to merge it, and what is
/// left to do on the branch.
pub fn body(names: &Names, status: &Status) -> String {
    let Names {
        upstream: up,
        upstream_branch,
        base,
        branch,
    } = names;
    let sha = &status.commit;
    let mut out = format!(
        "Merges {} from [{up}](https://github.com/{up}) `{upstream_branch}`, up to [`{}`](https://github.com/{up}/commit/{sha}), into `{base}`.\n\n\
         Merge with a merge commit, outside the merge queue. Never squash or rebase it: `{base}` keeps upstream's history.\n",
        commits(status.commits),
        short(sha),
    );

    // Left to do: conflicts first, they block the merge
    if !status.markers.is_empty() || !status.submodules.is_empty() {
        out.push_str(&format!(
            "\n## Conflicts\n\nThe merge is committed unresolved. Fix it on `{branch}` and push; the next run adds to this branch.\n"
        ));
    }
    if !status.markers.is_empty() {
        out.push_str("\nFiles with conflict markers:\n\n");
        for path in &status.markers {
            out.push_str(&format!("- `{path}`\n"));
        }
    }
    if !status.submodules.is_empty() {
        out.push_str("\nSubmodules upstream moved:\n\n");
        for s in &status.submodules {
            let ours = s.ours.as_deref().map(short).unwrap_or("none");
            out.push_str(&format!(
                "- `{}`: ours `{ours}`, upstream `{}`\n",
                s.path,
                short(&s.theirs)
            ));
        }
    }
    if status.guidance == Guidance::Changed {
        out.push_str(&format!(
            "\n## Upstream guidance\n\n{up} changed its own `CLAUDE.md` or `AGENTS.md`. The \"Upstream guidance\" section of `AGENTS.md` has the new text. Check it against the rules above it.\n"
        ));
    }

    out.push_str("\nOpened by AnkiQuest/AnkiQuest-Sync.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::Submodule;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    fn names() -> Names<'static> {
        Names {
            upstream: "ankidroid/Anki-Android",
            upstream_branch: "main",
            base: "master",
            branch: "sync/ankidroid/Anki-Android",
        }
    }

    fn clean() -> Status {
        Status {
            commit: SHA.to_owned(),
            commits: 12,
            markers: Vec::new(),
            submodules: Vec::new(),
            guidance: Guidance::Same,
        }
    }

    #[test]
    fn title_is_conventional() {
        let t = title("ankidroid/Anki-Android", SHA);
        assert_eq!(
            t,
            "chore(sync): merge upstream ankidroid/Anki-Android at 0123456789"
        );

        // The longest upstream still fits the 72-character limit
        assert!(title("ankidroid/Anki-Android-Backend", SHA).len() <= 72);
    }

    #[test]
    fn long_source_shortens_the_commit() {
        let t = title("Pedrubik2000/kuma3-anki-backend", SHA);
        assert_eq!(
            t,
            "chore(sync): merge upstream Pedrubik2000/kuma3-anki-backend at 012345678"
        );
        assert_eq!(t.len(), 72);
    }

    #[test]
    fn clean_body() {
        let b = body(&names(), &clean());
        assert!(b.starts_with("Merges 12 new commits from [ankidroid/Anki-Android](https://github.com/ankidroid/Anki-Android) `main`, up to [`0123456789`](https://github.com/ankidroid/Anki-Android/commit/0123456789abcdef0123456789abcdef01234567), into `master`.\n"));
        assert!(b.contains("outside the merge queue"));
        assert!(!b.contains("## Conflicts"));
        assert!(!b.contains("## Upstream guidance"));
        assert!(b.ends_with("Opened by AnkiQuest/AnkiQuest-Sync.\n"));
    }

    #[test]
    fn conflicts_listed() {
        let mut s = clean();
        s.markers = vec!["AnkiDroid/build.gradle".to_owned()];
        s.submodules = vec![Submodule {
            path: "anki".to_owned(),
            ours: Some("a".repeat(40)),
            theirs: "b".repeat(40),
        }];
        let b = body(&names(), &s);
        assert!(b.contains("Fix it on `sync/ankidroid/Anki-Android` and push"));
        assert!(b.contains("- `AnkiDroid/build.gradle`\n"));
        assert!(b.contains("- `anki`: ours `aaaaaaaaaa`, upstream `bbbbbbbbbb`\n"));
    }

    #[test]
    fn guidance_change_noted() {
        let mut s = clean();
        s.guidance = Guidance::Changed;
        assert!(
            body(&names(), &s)
                .contains("ankidroid/Anki-Android changed its own `CLAUDE.md` or `AGENTS.md`.")
        );
    }
}
