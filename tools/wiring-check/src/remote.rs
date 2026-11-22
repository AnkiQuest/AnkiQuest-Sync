// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reading the sources now: git for branches, curl for Maven Central and gh
//! for GitHub's compare API.

use std::process::Command;

/// Maven Central's repository.
const CENTRAL: &str = "https://repo1.maven.org/maven2";

/// HTTP status codes curl reports.
const HTTP_OK: &str = "200";
const HTTP_NOT_FOUND: &str = "404";

/// A repository's branches.
pub struct Refs {
    /// The default branch's head.
    pub head: Option<String>,
    /// Every branch's tip.
    pub tips: Vec<String>,
}

/// What the check reads from the network.
pub trait Remote {
    fn refs(&mut self, repo: &str) -> Result<Refs, String>;

    /// The newest release of `group:artifact` on Maven Central; None when
    /// it has none.
    fn release(&mut self, group: &str, artifact: &str) -> Result<Option<String>, String>;

    /// The files that differ between two commits of `repo`.
    fn files(&mut self, repo: &str, base: &str, head: &str) -> Result<Vec<String>, String>;
}

/// The real sources.
pub struct Live;

impl Remote for Live {
    fn refs(&mut self, repo: &str) -> Result<Refs, String> {
        let url = format!("https://github.com/{repo}.git");
        let out = run("git", &["ls-remote", &url, "HEAD", "refs/heads/*"])?;
        Ok(parse_refs(&out))
    }

    fn release(&mut self, group: &str, artifact: &str) -> Result<Option<String>, String> {
        let path = group.replace('.', "/");
        let url = format!("{CENTRAL}/{path}/{artifact}/maven-metadata.xml");
        let out = run("curl", &["-sSL", "-w", "\n%{http_code}", &url])?;
        let (body, code) = out.rsplit_once('\n').unwrap_or(("", out.as_str()));
        match code.trim() {
            HTTP_NOT_FOUND => Ok(None),
            HTTP_OK => newest(body)
                .map(Some)
                .ok_or_else(|| format!("{url}: no version")),
            other => Err(format!("{url}: HTTP {other}")),
        }
    }

    fn files(&mut self, repo: &str, base: &str, head: &str) -> Result<Vec<String>, String> {
        let path = format!("repos/{repo}/compare/{base}...{head}");
        let out = run("gh", &["api", &path, "--jq", ".files[].filename"])?;
        Ok(out.lines().map(str::to_owned).collect())
    }
}

/// Runs a command and returns its stdout.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("failed");
        return Err(format!("{program} {}: {first}", args.join(" ")));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("{program}: {e}"))
}

/// `git ls-remote` output: `<commit>\t<ref>` lines.
fn parse_refs(out: &str) -> Refs {
    let mut refs = Refs {
        head: None,
        tips: Vec::new(),
    };
    for line in out.lines() {
        let Some((commit, name)) = line.split_once('\t') else {
            continue;
        };
        if name == "HEAD" {
            refs.head = Some(commit.to_owned());
        } else {
            refs.tips.push(commit.to_owned());
        }
    }
    refs
}

/// The newest version in a maven-metadata.xml: its release, else its latest.
fn newest(xml: &str) -> Option<String> {
    tag(xml, "release").or_else(|| tag(xml, "latest"))
}

fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find('<')? + start;
    Some(xml[start..end].trim().to_owned()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs() {
        let refs = parse_refs("aaa\tHEAD\nbbb\trefs/heads/main\nccc\trefs/heads/rwkv\n");
        assert_eq!(refs.head.as_deref(), Some("aaa"));
        assert_eq!(refs.tips, ["bbb", "ccc"]);
    }

    #[test]
    fn metadata() {
        let xml = "<metadata><versioning><latest>2.0-SNAPSHOT</latest>\
                   <release>1.9</release></versioning></metadata>";
        assert_eq!(newest(xml).as_deref(), Some("1.9"));
        let xml = "<metadata><versioning><latest>2.0</latest></versioning></metadata>";
        assert_eq!(newest(xml).as_deref(), Some("2.0"));
        assert_eq!(newest("<metadata/>"), None);
    }
}
