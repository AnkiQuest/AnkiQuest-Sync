// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reading wiring.json and what it records about each source.

use std::collections::BTreeSet;

use serde::Deserialize;

/// Prefix of the package nodes' ids: `maven:<group>`.
const MAVEN_NODE: &str = "maven:";

/// Maven groups built by JitPack, which the map doesn't compare.
const JITPACK_GROUP: &str = "com.github.";

/// Fact kinds that compare a version with Maven Central.
const MAVEN_FACTS: [&str; 2] = ["maven", "publishes"];

/// The length of a full commit id.
const COMMIT_LEN: usize = 40;

/// The parts of wiring.json the check reads.
#[derive(Deserialize)]
pub struct Map {
    /// When the map was generated (ISO 8601, UTC).
    pub generated: String,
    #[serde(default)]
    nodes: Vec<Node>,
    #[serde(default)]
    edges: Vec<Edge>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    /// The default branch's head, for the repositories the map read.
    head: Option<String>,
}

#[derive(Deserialize)]
struct Edge {
    from: String,
    to: String,
    #[serde(default)]
    facts: Vec<Fact>,
}

#[derive(Deserialize)]
struct Fact {
    kind: String,
    label: String,
    pinned: Option<String>,
    latest: Option<String>,
}

/// One recorded value to compare with its source now.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Check {
    /// `repo`'s default branch was at `commit`.
    Head { repo: String, commit: String },
    /// A pin was compared with `commit`, the tip of one of `repo`'s branches.
    Tip { repo: String, commit: String },
    /// The newest release of `group`:`artifacts` on Maven Central was
    /// `version`, or there was none.
    Release {
        group: String,
        artifacts: Vec<String>,
        version: Option<String>,
    },
}

impl Map {
    pub fn parse(text: &str) -> Result<Map, String> {
        serde_json::from_str(text).map_err(|e| format!("wiring.json: {e}"))
    }

    /// The most recently generated of `maps`.
    pub fn newest(maps: Vec<Map>) -> Option<Map> {
        maps.into_iter()
            .max_by(|a, b| a.generated.cmp(&b.generated))
    }

    /// Everything the map recorded that can move: heads, branch tips and
    /// Maven releases, each once.
    pub fn checks(&self) -> Vec<Check> {
        let mut checks = BTreeSet::new();

        // Heads of the repositories the map read
        for node in &self.nodes {
            if let Some(commit) = &node.head {
                checks.insert(Check::Head {
                    repo: node.id.clone(),
                    commit: commit.clone(),
                });
            }
        }

        // Pins: the branch tip or Maven release each was compared with
        for edge in &self.edges {
            for fact in &edge.facts {
                if let Some(check) = edge.check(fact) {
                    checks.insert(check);
                }
            }
        }
        checks.into_iter().collect()
    }
}

impl Edge {
    /// What `fact` was compared with, if it can move.
    fn check(&self, fact: &Fact) -> Option<Check> {
        if MAVEN_FACTS.contains(&fact.kind.as_str()) {
            return self.release(fact);
        }
        let commit = fact.latest.as_ref().filter(|v| is_commit(v))?;
        if self.from.starts_with(MAVEN_NODE) {
            return None;
        }
        Some(Check::Tip {
            repo: self.from.clone(),
            commit: commit.clone(),
        })
    }

    /// A version pin against Maven Central. Edges that only name a
    /// group's publisher pin nothing, and JitPack has no index.
    fn release(&self, fact: &Fact) -> Option<Check> {
        fact.pinned.as_ref()?;
        let group = [&self.from, &self.to]
            .into_iter()
            .find_map(|id| id.strip_prefix(MAVEN_NODE))?;
        if group.starts_with(JITPACK_GROUP) {
            return None;
        }
        let artifacts = fact
            .label
            .split(", ")
            .map(str::to_owned)
            .filter(|a| !a.is_empty())
            .collect();
        Some(Check::Release {
            group: group.to_owned(),
            artifacts,
            version: fact.latest.clone(),
        })
    }
}

fn is_commit(value: &str) -> bool {
    value.len() == COMMIT_LEN && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"{
      "generated": "2026-11-21T18:24:59.768Z",
      "nodes": [
        { "id": "AnkiQuest/AnkiQuest-Dev", "kind": "repo", "head": "1bc4175426e4ea8578e196763cbdf503900e950c" },
        { "id": "AnkiQuest/AnkiQuest-Hosting", "kind": "repo", "private": true },
        { "id": "maven:org.ankiquest", "kind": "package" }
      ],
      "edges": [
        { "from": "AnkiQuest/AnkiQuest-Dev", "to": "AnkiQuest/AnkiQuest-Sync", "facts": [
          { "kind": "flake", "label": "ankiquest-dev", "pinned": "16ac7913f4e1ab51bbd05c924f3384034bc09804", "latest": "1bc4175426e4ea8578e196763cbdf503900e950c" },
          { "kind": "workflow", "label": "workflows/labels.yml@master", "pinned": "master" }
        ] },
        { "from": "ankidroid/Anki-Android-Backend", "to": "AnkiQuest/AnkiQuest-Android-Backend", "facts": [
          { "kind": "watched", "label": "VERSION_NAME on main", "pinned": "0.1.77-anki26.09.4", "latest": "0.1.77-anki26.09.4" }
        ] },
        { "from": "AnkiQuest/AnkiQuest-Android-Backend", "to": "maven:org.ankiquest", "facts": [
          { "kind": "publishes", "label": "anki-backend-android, anki-backend-core", "pinned": "0.1.77-anki26.09.4-ankiquest.3" }
        ] },
        { "from": "maven:io.github.david-allison", "to": "AnkiQuest/AnkiQuest-Android", "facts": [
          { "kind": "maven", "label": "anki-backend-android", "pinned": "0.1.77-anki26.09.4", "latest": "0.1.77-anki26.09.4" }
        ] },
        { "from": "ankidroid/Anki-Android", "to": "maven:com.github.ankidroid", "facts": [
          { "kind": "publishes", "label": "publishes com.github.ankidroid" }
        ] },
        { "from": "maven:com.github.ankidroid", "to": "AnkiQuest/AnkiQuest-AnkiConnectAndroid", "facts": [
          { "kind": "maven", "label": "Anki-Android", "pinned": "2.17alpha14" }
        ] }
      ]
    }"#;

    fn s(value: &str) -> String {
        value.to_owned()
    }

    #[test]
    fn checks() {
        let checks = Map::parse(MAP).unwrap().checks();
        assert_eq!(
            checks,
            vec![
                Check::Head {
                    repo: s("AnkiQuest/AnkiQuest-Dev"),
                    commit: s("1bc4175426e4ea8578e196763cbdf503900e950c"),
                },
                Check::Tip {
                    repo: s("AnkiQuest/AnkiQuest-Dev"),
                    commit: s("1bc4175426e4ea8578e196763cbdf503900e950c"),
                },
                Check::Release {
                    group: s("io.github.david-allison"),
                    artifacts: vec![s("anki-backend-android")],
                    version: Some(s("0.1.77-anki26.09.4")),
                },
                Check::Release {
                    group: s("org.ankiquest"),
                    artifacts: vec![s("anki-backend-android"), s("anki-backend-core")],
                    version: None,
                },
            ]
        );
    }

    #[test]
    fn newest() {
        let old = Map::parse(r#"{ "generated": "2026-11-20T04:41:00.000Z" }"#).unwrap();
        let new = Map::parse(r#"{ "generated": "2026-11-21T18:24:59.768Z" }"#).unwrap();
        let picked = Map::newest(vec![new, old]).unwrap();
        assert_eq!(picked.generated, "2026-11-21T18:24:59.768Z");
        assert!(Map::newest(Vec::new()).is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(Map::parse("{").is_err());
    }
}
