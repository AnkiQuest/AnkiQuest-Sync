// SPDX-License-Identifier: AGPL-3.0-or-later

//! Runs fork-sync against throwaway git repositories: an upstream and a
//! fork that carries generated AGENTS.md and CLAUDE.md.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const UPSTREAM: &str = "ankidroid/Anki-Android";
const SHARED: &str = "# Agent instructions\n\nShared rules.\n";
const SECTION: &str = "## This repository\n\nFork rules.\n";

/// A scratch directory with a fork checkout (`fork/`) whose remote-tracking
/// ref `upstream/main` stands for upstream.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!("fork-sync-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("fork")).unwrap();
        fs::write(root.join("shared.md"), SHARED).unwrap();
        fs::write(root.join("section.md"), SECTION).unwrap();
        let s = Scratch { root };
        s.git(&["init", "-q", "-b", "main"]);

        // Upstream's history: a README and its own guidance
        s.write("README.md", "upstream\n");
        s.write("CLAUDE.md", "@AGENTS.md\n");
        s.write("AGENTS.md", "- Follow the AI policy.\n");
        s.write("app.txt", "one\ntwo\nthree\n");
        s.commit("upstream: start");
        s.git(&["branch", "upstream"]);

        // The fork: master with our AGENTS.md
        s.git(&["checkout", "-q", "-b", "master"]);
        s.git(&["branch", "-D", "main"]);
        let agents = s.agents();
        s.write("AGENTS.md", &agents);
        s.write("CLAUDE.md", "@AGENTS.md\n");
        s.write("ours.txt", "ours\n");
        s.commit("feat: add ours");
        s
    }

    fn fork(&self) -> PathBuf {
        self.root.join("fork")
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.fork().join(path);
        let _ = fs::remove_file(&path);
        fs::write(path, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.fork().join(path)).unwrap()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    /// Runs a commit on upstream's branch and comes back to `branch`.
    fn upstream(&self, branch: &str, change: impl FnOnce(&Scratch)) {
        self.git(&["checkout", "-q", "upstream"]);
        change(self);
        self.git(&["checkout", "-q", branch]);
    }

    fn git(&self, args: &[&str]) -> String {
        let out = env(Command::new("git").arg("-C").arg(self.fork()).args(args))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn tool(&self, args: &[&str]) -> Output {
        env(Command::new(env!("CARGO_BIN_EXE_fork-sync"))
            .current_dir(self.fork())
            .args(args))
        .output()
        .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.tool(args);
        assert!(
            out.status.success(),
            "fork-sync {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// AGENTS.md for upstream's guidance at `rev`, as the sync job writes it.
    fn agents_at(&self, rev: &str) -> String {
        let dir = self.root.join("guidance");
        self.ok(&["guidance", rev, dir.to_str().unwrap()]);
        self.compose(Some(&dir))
    }

    fn agents(&self) -> String {
        self.agents_at("HEAD")
    }

    fn compose(&self, dir: Option<&Path>) -> String {
        let shared = self.root.join("shared.md");
        let section = self.root.join("section.md");
        let mut args = vec![
            "agents",
            shared.to_str().unwrap(),
            section.to_str().unwrap(),
        ];
        if let Some(dir) = dir {
            args.extend(["upstream", UPSTREAM, dir.to_str().unwrap()]);
        }
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.ok(&args)
    }

    /// The sync job's steps: branch, guidance, AGENTS.md, merge.
    fn sync(&self) -> String {
        let branch = self.ok(&["branch", UPSTREAM]).trim().to_owned();
        if self.git(&["branch", "--list", &branch]).is_empty() {
            self.git(&["checkout", "-q", "-b", &branch, "master"]);
        } else {
            self.git(&["checkout", "-q", &branch]);
        }
        let agents = self.root.join("agents.md");
        fs::write(&agents, self.agents_at("upstream")).unwrap();
        self.ok(&["merge", "upstream", UPSTREAM, agents.to_str().unwrap()])
    }

    fn body(&self) -> String {
        self.ok(&["body", UPSTREAM, "main", "master", "upstream"])
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Isolates git from the user's configuration and names the committer.
fn env(cmd: &mut Command) -> &mut Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.org")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.org")
}

#[test]
fn clean_merge() {
    let s = Scratch::new("clean");
    s.upstream("master", |s| {
        s.write("new.txt", "new\n");
        s.commit("upstream: add new");
    });

    assert_eq!(s.sync(), "merged\n");
    let parents = s.git(&["log", "-1", "--format=%P"]);
    assert_eq!(parents.split_whitespace().count(), 2);
    let subject = s.git(&["log", "-1", "--format=%s"]);
    assert!(
        subject.starts_with("chore(sync): merge upstream ankidroid/Anki-Android at "),
        "{subject}"
    );
    assert_eq!(s.read("new.txt"), "new\n");
    assert_eq!(s.read("CLAUDE.md"), "@AGENTS.md\n");
    assert!(s.read("AGENTS.md").starts_with(SHARED));

    let body = s.body();
    assert!(body.starts_with("Merges 1 new commit from "), "{body}");
    assert!(!body.contains("## Conflicts"));
    assert!(!body.contains("## Upstream guidance"));

    // A second run has nothing to merge
    assert_eq!(s.sync(), "up-to-date\n");
}

#[test]
fn keeps_guidance_from_upstream() {
    let s = Scratch::new("guidance");
    let agents = s.read("AGENTS.md");
    assert!(agents.contains("## Upstream guidance"));
    assert!(
        agents.contains("<!-- ankidroid/Anki-Android: AGENTS.md -->\n\n- Follow the AI policy.\n")
    );
    assert!(!agents.contains("<!-- ankidroid/Anki-Android: CLAUDE.md -->"));
}

#[test]
fn guidance_change_is_merged() {
    let s = Scratch::new("guidance-change");
    s.upstream("master", |s| {
        s.write("AGENTS.md", "- Follow the AI policy.\n- Run ktlint.\n");
        s.write("CLAUDE.md", "# Claude\n\nUse the gradle wrapper.\n");
        s.commit("upstream: new guidance");
    });

    // Both files conflict with ours; neither is left as a conflict
    assert_eq!(s.sync(), "merged\n");
    let agents = s.read("AGENTS.md");
    assert!(agents.contains("- Run ktlint.\n"));
    assert!(agents.contains(
        "<!-- ankidroid/Anki-Android: CLAUDE.md -->\n\n# Claude\n\nUse the gradle wrapper.\n"
    ));
    assert!(!agents.contains("<<<<<<<"));
    assert_eq!(s.read("CLAUDE.md"), "@AGENTS.md\n");
    assert!(!s.git(&["log", "-1", "--format=%b"]).contains("Conflicts"));

    let body = s.body();
    assert!(body.contains("## Upstream guidance"), "{body}");
    assert!(!body.contains("## Conflicts"));
}

#[test]
fn symlinked_agents_md() {
    let s = Scratch::new("symlink");
    s.upstream("master", |s| {
        // ankitects/anki: AGENTS.md links to CLAUDE.md
        s.write("CLAUDE.md", "# Claude Code\n\nRun just check.\n");
        fs::remove_file(s.fork().join("AGENTS.md")).unwrap();
        std::os::unix::fs::symlink("CLAUDE.md", s.fork().join("AGENTS.md")).unwrap();
        s.commit("upstream: link AGENTS.md");
    });

    assert_eq!(s.sync(), "merged\n");
    let meta = fs::symlink_metadata(s.fork().join("AGENTS.md")).unwrap();
    assert!(meta.file_type().is_file());
    let agents = s.read("AGENTS.md");
    assert_eq!(agents.matches("Run just check.").count(), 1);
    assert!(!agents.contains("<!-- ankidroid/Anki-Android: AGENTS.md -->"));
    assert_eq!(
        s.git(&["ls-files", "-s", "AGENTS.md"]).split(' ').next(),
        Some("100644")
    );
}

#[test]
fn conflict_is_committed_with_markers() {
    let s = Scratch::new("conflict");
    s.write("app.txt", "one\nours\nthree\n");
    s.commit("feat: change app");
    s.upstream("master", |s| {
        s.write("app.txt", "one\ntheirs\nthree\n");
        s.commit("upstream: change app");
    });

    assert_eq!(s.sync(), "merged\n");
    assert!(s.read("app.txt").contains("<<<<<<< "));
    assert!(s.git(&["status", "--porcelain"]).is_empty());
    let message = s.git(&["log", "-1", "--format=%b"]);
    assert!(
        message.contains("Conflicts, committed unresolved:\n- app.txt"),
        "{message}"
    );

    let body = s.body();
    assert!(body.contains("## Conflicts"));
    assert!(body.contains("- `app.txt`\n"));

    // Someone resolves it on the branch; the next upstream commit merges on top
    s.write("app.txt", "one\nboth\nthree\n");
    s.commit("fix: resolve app.txt");
    s.upstream("sync/ankidroid/Anki-Android", |s| {
        s.write("later.txt", "later\n");
        s.commit("upstream: later");
    });
    assert_eq!(s.sync(), "merged\n");
    assert_eq!(s.read("app.txt"), "one\nboth\nthree\n");
    let body = s.body();
    assert!(!body.contains("## Conflicts"), "{body}");
    assert!(body.starts_with("Merges 2 new commits"), "{body}");
}

#[test]
fn submodule_conflict_keeps_ours() {
    let s = Scratch::new("submodule");
    let gitlink = |sha: &str| format!("160000,{sha},anki");
    let base = "1".repeat(40);
    s.upstream("master", |s| {
        s.git(&["update-index", "--add", "--cacheinfo", &gitlink(&base)]);
        s.git(&["commit", "-q", "-m", "upstream: add anki"]);
    });
    s.git(&["merge", "-q", "--no-edit", "upstream"]);
    let agents = s.agents_at("HEAD");
    s.write("AGENTS.md", &agents);
    s.write("CLAUDE.md", "@AGENTS.md\n");
    s.commit("chore: regenerate");

    let ours = "2".repeat(40);
    let theirs = "3".repeat(40);
    s.git(&["update-index", "--cacheinfo", &gitlink(&ours)]);
    s.git(&["commit", "-q", "-m", "feat: move anki"]);
    s.upstream("master", |s| {
        s.git(&["update-index", "--cacheinfo", &gitlink(&theirs)]);
        s.git(&["commit", "-q", "-m", "upstream: move anki"]);
    });

    assert_eq!(s.sync(), "merged\n");
    let entry = s.git(&["ls-tree", "HEAD", "anki"]);
    assert!(entry.contains(&ours), "{entry}");
    let body = s.body();
    assert!(
        body.contains("- `anki`: ours `2222222222`, upstream `3333333333`\n"),
        "{body}"
    );
}

#[test]
fn agents_without_upstream_matches_cat() {
    let s = Scratch::new("plain");
    assert_eq!(s.compose(None), format!("{SHARED}\n{SECTION}"));
}

#[test]
fn agents_keeps_the_section() {
    let s = Scratch::new("keep");
    let current = s.fork().join("AGENTS.md");
    let shared = s.root.join("shared.md");
    let section = s.root.join("section.md");
    let kept = s.ok(&[
        "agents",
        shared.to_str().unwrap(),
        section.to_str().unwrap(),
        "keep",
        current.to_str().unwrap(),
    ]);
    assert_eq!(kept, s.read("AGENTS.md"));

    // A repository without an AGENTS.md gets the plain one
    let missing = s.root.join("missing.md");
    let plain = s.ok(&[
        "agents",
        shared.to_str().unwrap(),
        section.to_str().unwrap(),
        "keep",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(plain, format!("{SHARED}\n{SECTION}"));
}

#[test]
fn usage_on_bad_arguments() {
    let s = Scratch::new("usage");
    let out = s.tool(&["nonsense"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: fork-sync"));
}
