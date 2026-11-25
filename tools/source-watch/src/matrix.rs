// SPDX-License-Identifier: AGPL-3.0-or-later

//! The sync job's matrix: every fork's upstream (forks.json) and every
//! watched source in mode "merge" (watched.json), one row each.

use serde::{Deserialize, Serialize};

use crate::watched::{Entry, Mode};

/// A forks.json entry.
#[derive(Debug, Clone, Deserialize)]
pub struct Fork {
    pub fork: String,
    pub upstream: String,
    pub upstream_branch: String,
}

/// Which upstream guidance the merge writes into AGENTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Guidance {
    /// The merged upstream's own CLAUDE.md and AGENTS.md: the fork's upstream.
    Upstream,
    /// The fork's AGENTS.md as it is: a watched source is not its upstream.
    Keep,
}

/// One sync job run: merge `upstream_branch` of `upstream` into `fork` on `branch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    pub fork: String,
    pub upstream: String,
    pub upstream_branch: String,
    pub branch: String,
    pub guidance: Guidance,
}

pub fn rows(forks: &str, entries: &[Entry]) -> Result<Vec<Row>, String> {
    let forks: Vec<Fork> = serde_json::from_str(forks).map_err(|e| format!("forks.json: {e}"))?;
    let upstreams = forks.into_iter().map(|f| Row {
        branch: format!("sync/{}", f.upstream),
        fork: f.fork,
        upstream: f.upstream,
        upstream_branch: f.upstream_branch,
        guidance: Guidance::Upstream,
    });
    let merged = entries
        .iter()
        .filter(|e| e.mode == Mode::Merge)
        .map(|e| Row {
            fork: e.target.clone(),
            upstream: e.source.clone(),
            upstream_branch: e.rev.clone(),
            branch: e.branch(),
            guidance: Guidance::Keep,
        });
    Ok(upstreams.chain(merged).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watched;

    #[test]
    fn upstreams_then_merged_sources() {
        let forks = r#"[{"fork": "AnkiQuest/AnkiQuest-Anki", "upstream": "ankitects/anki", "upstream_branch": "main"}]"#;
        let entries = watched::parse(
            r#"[
              {"source": "JSchoreels/anki", "ref": "main", "kind": "branch", "mode": "merge",
               "target": "AnkiQuest/AnkiQuest-Anki", "why": "w"},
              {"source": "a/b", "ref": "main", "kind": "branch", "mode": "port",
               "target": "AnkiQuest/AnkiQuest-Android", "why": "w", "reviewed": "x"}
            ]"#,
        )
        .unwrap();

        let rows = rows(forks, &entries).unwrap();

        let json = serde_json::to_string(&rows).unwrap();
        assert_eq!(
            json,
            r#"[{"fork":"AnkiQuest/AnkiQuest-Anki","upstream":"ankitects/anki","upstream_branch":"main","branch":"sync/ankitects/anki","guidance":"upstream"},{"fork":"AnkiQuest/AnkiQuest-Anki","upstream":"JSchoreels/anki","upstream_branch":"main","branch":"sync/JSchoreels/anki-main","guidance":"keep"}]"#
        );
    }

    #[test]
    fn committed_files() {
        let entries = watched::parse(include_str!("../../../watched.json")).unwrap();
        let rows = rows(include_str!("../../../forks.json"), &entries).unwrap();
        let merged: Vec<&str> = rows
            .iter()
            .filter(|r| r.guidance == Guidance::Keep)
            .map(|r| r.branch.as_str())
            .collect();
        assert_eq!(
            merged,
            [
                "sync/JSchoreels/anki-main",
                "sync/Pedrubik2000/kuma3-anki-core-rwkv",
                "sync/Pedrubik2000/kuma3-anki-backend-rwkv"
            ]
        );
    }
}
