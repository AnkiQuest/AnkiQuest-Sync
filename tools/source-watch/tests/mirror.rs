// SPDX-License-Identifier: AGPL-3.0-or-later

//! Runs `source-watch mirror` against throwaway bare repositories: a source
//! (`up/app.git`, branch `main`) and a target (`aq/app.git`, default branch
//! `master`) mirroring it on `upstream-main`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const MIRRORS: &str = r#"[{"source": "up/app", "ref": "main", "target": "aq/app",
  "branch": "upstream-main", "why": "test"}]"#;

struct Scratch {
    root: PathBuf,
}

impl Scratch {
    /// Both repositories, sharing a first commit; the target's master has one of its own.
    fn new(name: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!("source-watch-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("base/up")).unwrap();
        fs::create_dir_all(root.join("base/aq")).unwrap();
        fs::write(root.join("mirrors.json"), MIRRORS).unwrap();
        let s = Scratch { root };
        s.git(&["init", "-q", "--bare", "-b", "main", "base/up/app.git"]);
        s.git(&["init", "-q", "--bare", "-b", "master", "base/aq/app.git"]);
        s.git(&["init", "-q", "-b", "main", "work"]);

        s.commit("upstream: start");
        s.work(&["push", "-q", &s.url("up"), "HEAD:refs/heads/main"]);
        s.commit("feat: ours");
        s.work(&["push", "-q", &s.url("aq"), "HEAD:refs/heads/master"]);
        s.work(&["reset", "-q", "--hard", "HEAD~1"]);
        s
    }

    fn url(&self, owner: &str) -> String {
        self.root
            .join(format!("base/{owner}/app.git"))
            .to_str()
            .unwrap()
            .to_owned()
    }

    fn git(&self, args: &[&str]) -> String {
        run(Command::new("git").current_dir(&self.root).args(args))
    }

    fn work(&self, args: &[&str]) -> String {
        run(Command::new("git")
            .current_dir(self.root.join("work"))
            .args(args))
    }

    fn commit(&self, message: &str) {
        self.work(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    /// A new upstream commit on main.
    fn upstream(&self, message: &str) {
        self.commit(message);
        self.work(&["push", "-q", &self.url("up"), "HEAD:refs/heads/main"]);
    }

    /// A branch's commit in a bare repository, or "" if it has none.
    fn head(&self, owner: &str, branch: &str) -> String {
        let out = self.git(&[
            "ls-remote",
            &self.url(owner),
            &format!("refs/heads/{branch}"),
        ]);
        out.split_whitespace().next().unwrap_or_default().to_owned()
    }

    fn mirror(&self, mirrors: &str) -> Output {
        let path = self.root.join("mirrors.json");
        fs::write(&path, mirrors).unwrap();
        let base = self.root.join("base");
        let summary = self.root.join("summary.md");
        let _ = fs::remove_file(&summary);
        env(Command::new(env!("CARGO_BIN_EXE_source-watch"))
            .arg("mirror")
            .arg(&path)
            .arg(&base)
            .arg(&summary)
            .arg(self.root.join("mirror")))
        .output()
        .unwrap()
    }

    fn summary(&self) -> String {
        fs::read_to_string(self.root.join("summary.md")).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn env(cmd: &mut Command) -> &mut Command {
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.org")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.org")
}

fn run(cmd: &mut Command) -> String {
    let out = env(cmd).output().unwrap();
    assert!(
        out.status.success(),
        "{cmd:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn ok(out: &Output) {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn creates_then_follows_the_source() {
    let s = Scratch::new("follow");
    let master = s.head("aq", "master");

    // No mirror yet: created at upstream's commit
    ok(&s.mirror(MIRRORS));
    assert_eq!(s.head("aq", "upstream-main"), s.head("up", "main"));
    assert!(s.summary().contains("aq/app upstream-main: created at "));

    // Nothing new
    ok(&s.mirror(MIRRORS));
    assert!(s.summary().contains("aq/app upstream-main: up to date at "));

    // Upstream moves: fast-forwarded
    s.upstream("upstream: next");
    ok(&s.mirror(MIRRORS));
    assert_eq!(s.head("aq", "upstream-main"), s.head("up", "main"));
    assert!(s.summary().contains("aq/app upstream-main: moved from "));

    // master is never touched
    assert_eq!(s.head("aq", "master"), master);
}

#[test]
fn refuses_to_force_push() {
    let s = Scratch::new("force");
    ok(&s.mirror(MIRRORS));
    let mirrored = s.head("aq", "upstream-main");

    // Upstream rewrites main
    s.work(&[
        "commit",
        "-q",
        "--amend",
        "--allow-empty",
        "-m",
        "upstream: rewritten",
    ]);
    s.work(&["push", "-q", "-f", &s.url("up"), "HEAD:refs/heads/main"]);

    let out = s.mirror(MIRRORS);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("::error::aq/app upstream-main: upstream-main is at "),
        "{stdout}"
    );
    assert!(stdout.contains("Not force-pushing"), "{stdout}");
    assert!(s.summary().contains("aq/app upstream-main: failed"));
    assert_eq!(s.head("aq", "upstream-main"), mirrored);
}

#[test]
fn refuses_the_default_branch() {
    let s = Scratch::new("default");
    let master = s.head("aq", "master");

    // The target's HEAD names this branch, whatever it is called
    s.git(&[
        "-C",
        &s.url("aq"),
        "symbolic-ref",
        "HEAD",
        "refs/heads/trunk",
    ]);
    s.work(&[
        "push",
        "-q",
        &s.url("aq"),
        &format!("{master}:refs/heads/trunk"),
    ]);
    let trunk = MIRRORS.replace("upstream-main", "trunk");

    let out = s.mirror(&trunk);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("trunk is the default branch"));
    assert_eq!(s.head("aq", "trunk"), master);

    // master and main are refused before anything is fetched
    let out = s.mirror(&MIRRORS.replace("upstream-main", "master"));
    assert!(!out.status.success());
    assert_eq!(s.head("aq", "master"), master);
}

#[test]
fn checks_every_mirror_when_one_fails() {
    let s = Scratch::new("every");
    let both = r#"[
      {"source": "up/missing", "ref": "main", "target": "aq/app", "branch": "gone", "why": "w"},
      {"source": "up/app", "ref": "main", "target": "aq/app", "branch": "upstream-main", "why": "w"}
    ]"#;

    let out = s.mirror(both);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 of 2 mirrors failed: aq/app gone"));
    assert_eq!(s.head("aq", "upstream-main"), s.head("up", "main"));
}
