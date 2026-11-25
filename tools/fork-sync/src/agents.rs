// SPDX-License-Identifier: AGPL-3.0-or-later

//! A repository's AGENTS.md and CLAUDE.md, as AnkiQuest-Agents defines them.

/// CLAUDE.md in every repository: Claude Code reads AGENTS.md through it.
pub const CLAUDE: &str = "@AGENTS.md\n";

/// shared.md, a blank line, the repository's section, then a fork's
/// upstream guidance section (empty for other repositories).
pub fn compose(shared: &str, section: &str, guidance: &str) -> String {
    let mut out = format!("{shared}\n{section}");
    if guidance.is_empty() {
        return out;
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(guidance);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guidance::{self, Guidance};

    #[test]
    fn without_upstream() {
        // The same bytes as `{ cat shared.md; echo; cat section; }`
        assert_eq!(
            compose("shared\n", "## This repository\n", ""),
            "shared\n\n## This repository\n"
        );
    }

    #[test]
    fn with_upstream() {
        let g = Guidance::new("a/b", &[("CLAUDE.md", "rules\n".to_owned())]);
        let out = compose("shared\n", "## This repository\n", &g.section());
        assert!(out.starts_with("shared\n\n## This repository\n\n## Upstream guidance\n\n"));
        assert!(out.ends_with("<!-- a/b: CLAUDE.md -->\n\nrules\n"));
    }

    #[test]
    fn keeps_a_section() {
        // The agents job keeps what the sync job wrote, with new rules above it
        let g = Guidance::new("a/b", &[("CLAUDE.md", "rules\n".to_owned())]);
        let old = compose("old shared\n", "old section\n", &g.section());
        let new = compose("new shared\n", "new section\n", guidance::of(&old));
        assert_eq!(new, compose("new shared\n", "new section\n", &g.section()));
    }
}
