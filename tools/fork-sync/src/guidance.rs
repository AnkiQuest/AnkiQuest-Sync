// SPDX-License-Identifier: AGPL-3.0-or-later

//! An upstream's own agent instructions, kept verbatim at the end of a
//! fork's AGENTS.md, after an introduction from AnkiQuest-Agents
//! (`upstream.md`).

use std::fs;
use std::path::Path;

/// The heading that starts the section; nothing above it contains it.
pub const HEADING: &str = "## Upstream guidance";

/// The files read from an upstream, in this order.
pub const FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

/// Stands for the upstream's `owner/repo` in the introduction.
pub const UPSTREAM: &str = "{upstream}";

/// One upstream's guidance files, pointers and duplicates left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guidance {
    upstream: String,
    files: Vec<(String, String)>,
}

impl Guidance {
    /// Reads FILES from `dir`, where a missing file means upstream has none.
    pub fn read(upstream: &str, dir: &Path) -> Result<Guidance, String> {
        let mut texts = Vec::new();
        for name in FILES {
            let path = dir.join(name);
            if !path.exists() {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            texts.push((name, text));
        }
        Ok(Guidance::new(upstream, &texts))
    }

    pub fn new(upstream: &str, texts: &[(&str, String)]) -> Guidance {
        let mut files: Vec<(String, String)> = Vec::new();
        for (name, text) in texts {
            let text = text.trim_end();

            // "@AGENTS.md", or a symlink read as its target's name, adds nothing
            if text.is_empty() || is_pointer(text) {
                continue;
            }

            // A symlink read through the API arrives as its target's text
            if files.iter().any(|(_, kept)| kept == text) {
                continue;
            }
            files.push(((*name).to_owned(), text.to_owned()));
        }
        Guidance {
            upstream: upstream.to_owned(),
            files,
        }
    }

    /// The section, or nothing when upstream has no guidance of its own:
    /// HEADING, `intro` with UPSTREAM replaced, then each file.
    pub fn section(&self, intro: &str) -> String {
        if self.files.is_empty() {
            return String::new();
        }
        let up = &self.upstream;
        let intro = intro.trim().replace(UPSTREAM, up);
        let mut out = format!("{HEADING}\n\n{intro}\n");
        for (name, text) in &self.files {
            out.push_str(&format!("\n<!-- {up}: {name} -->\n\n{text}\n"));
        }
        out
    }
}

/// A file that only names another: "@AGENTS.md", or "CLAUDE.md" as git
/// shows a symlink.
fn is_pointer(text: &str) -> bool {
    if text.lines().count() != 1 {
        return false;
    }
    let target = text.strip_prefix('@').unwrap_or(text);
    !target.contains(char::is_whitespace) && target.ends_with(".md")
}

/// The guidance section of an AGENTS.md: from HEADING to the end.
pub fn of(agents: &str) -> &str {
    let start = agents
        .match_indices(HEADING)
        .find(|(i, _)| *i == 0 || agents.as_bytes()[i - 1] == b'\n')
        .map(|(i, _)| i);
    match start {
        Some(i) => &agents[i..],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTRO: &str = "[{upstream}](https://github.com/{upstream})'s own instructions.\n";

    fn guidance(texts: &[(&str, &str)]) -> Guidance {
        let texts: Vec<(&str, String)> = texts.iter().map(|(n, t)| (*n, (*t).to_owned())).collect();
        Guidance::new("ankidroid/Anki-Android", &texts)
    }

    #[test]
    fn skips_pointers() {
        // Anki-Android: CLAUDE.md is "@AGENTS.md"
        let g = guidance(&[
            ("CLAUDE.md", "@AGENTS.md\n"),
            ("AGENTS.md", "- Follow the AI policy.\n"),
        ]);
        let section = g.section(INTRO);
        assert!(
            section.contains(
                "<!-- ankidroid/Anki-Android: AGENTS.md -->\n\n- Follow the AI policy.\n"
            )
        );
        assert!(!section.contains("CLAUDE.md"));
    }

    #[test]
    fn skips_symlink_by_name() {
        // ankitects/anki: AGENTS.md is a symlink, which git shows as "CLAUDE.md"
        let g = guidance(&[
            ("CLAUDE.md", "# Claude Code\n\nRun just check.\n"),
            ("AGENTS.md", "CLAUDE.md"),
        ]);
        assert!(
            !g.section(INTRO)
                .contains("<!-- ankidroid/Anki-Android: AGENTS.md -->")
        );
    }

    #[test]
    fn skips_symlink_by_text() {
        // The contents API follows the symlink and returns the same text
        let text = "# Claude Code\n\nRun just check.\n";
        let g = guidance(&[("CLAUDE.md", text), ("AGENTS.md", text)]);
        assert_eq!(g.section(INTRO).matches("Run just check.").count(), 1);
    }

    #[test]
    fn keeps_both_when_different() {
        let g = guidance(&[
            ("CLAUDE.md", "Claude rules\n"),
            ("AGENTS.md", "Agent rules\n"),
        ]);
        let section = g.section(INTRO);
        let claude = section.find("Claude rules").unwrap();
        let agents = section.find("Agent rules").unwrap();
        assert!(claude < agents);
    }

    #[test]
    fn keeps_one_line_prose() {
        let g = guidance(&[("AGENTS.md", "Run the tests before you push.")]);
        assert!(g.section(INTRO).contains("Run the tests before you push."));
    }

    #[test]
    fn empty_without_files() {
        assert_eq!(guidance(&[]).section(INTRO), "");
        assert_eq!(guidance(&[("CLAUDE.md", "@AGENTS.md")]).section(INTRO), "");
    }

    #[test]
    fn names_the_upstream() {
        let g = guidance(&[("CLAUDE.md", "rules\n")]);
        assert!(g.section(INTRO).starts_with(
            "## Upstream guidance\n\n[ankidroid/Anki-Android](https://github.com/ankidroid/Anki-Android)'s own instructions.\n\n<!-- "
        ));
    }

    #[test]
    fn finds_the_section() {
        let agents = "# Agent instructions\n\nSee ## Upstream guidance below.\n\n## Upstream guidance\n\ntext\n";
        assert_eq!(of(agents), "## Upstream guidance\n\ntext\n");
        assert_eq!(of("# Agent instructions\n"), "");
    }
}
