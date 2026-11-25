// SPDX-License-Identifier: AGPL-3.0-or-later

//! The releases consumers pin: AnkiQuest-Art's pictures and AnkiQuest-Server's
//! shared rules. Each has its own pin file, tags, assets and pull request.

/// Which release a command is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// AnkiQuest-Art's `art-<N>` releases, pinned in `ankiquest-art.json`.
    Art,
    /// AnkiQuest-Server's `rules-<N>` releases, pinned in `ankiquest-rules.json`.
    Rules,
}

/// Every asset an Art release's pin file lists.
const ART_ASSETS: [&str; 6] = [
    "android-drawables.zip",
    "addon-art.zip",
    "masters.zip",
    "archive.zip",
    "manifest.json",
    "poses.json",
];

/// Every asset a rules release's pin file lists: the WebAssembly module, the
/// Python bindings and the native library of each desktop platform.
const RULES_ASSETS: [&str; 7] = [
    "ankiquest_rules.wasm",
    "ankiquest_rules.py",
    "libankiquest_rules-linux-x86_64.so",
    "libankiquest_rules-linux-aarch64.so",
    "ankiquest_rules-windows-x86_64.dll",
    "libankiquest_rules-macos-arm64.dylib",
    "libankiquest_rules-macos-x86_64.dylib",
];

impl Kind {
    /// The kind named on the command line.
    pub fn parse(name: &str) -> Result<Kind, String> {
        match name {
            "art" => Ok(Kind::Art),
            "rules" => Ok(Kind::Rules),
            _ => Err(format!("{name:?} is not a release kind (art, rules)")),
        }
    }

    /// The pin file's name, at each consumer's root and among a release's assets.
    pub fn file(self) -> &'static str {
        match self {
            Kind::Art => "ankiquest-art.json",
            Kind::Rules => "ankiquest-rules.json",
        }
    }

    /// The repository that publishes the releases.
    pub fn repo(self) -> &'static str {
        match self {
            Kind::Art => "AnkiQuest/AnkiQuest-Art",
            Kind::Rules => "AnkiQuest/AnkiQuest-Server",
        }
    }

    /// The publisher's short name, for the commit line of a pull request.
    pub fn source(self) -> &'static str {
        match self {
            Kind::Art => "Art",
            Kind::Rules => "Server",
        }
    }

    /// What every release tag starts with.
    pub fn prefix(self) -> &'static str {
        match self {
            Kind::Art => "art-",
            Kind::Rules => "rules-",
        }
    }

    /// Every asset a release's pin file lists.
    pub const fn assets(self) -> &'static [&'static str] {
        match self {
            Kind::Art => &ART_ASSETS,
            Kind::Rules => &RULES_ASSETS,
        }
    }

    /// The pull request title for release `tag`.
    pub fn title(self, tag: &str) -> String {
        match self {
            Kind::Art => format!("chore(art): use AnkiQuest-Art {tag}"),
            Kind::Rules => format!("chore(rules): use AnkiQuest-Server {tag}"),
        }
    }

    /// Where the release's contents were reviewed, for the pull request body.
    pub fn reviewed(self) -> &'static str {
        match self {
            Kind::Art => "from art reviewed there. Change the art there, not here.",
            Kind::Rules => "from `crates/rules`, reviewed there. Change the rules there, not here.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(Kind::parse("art"), Ok(Kind::Art));
        assert_eq!(Kind::parse("rules"), Ok(Kind::Rules));
        assert!(Kind::parse("Art").is_err());
    }

    #[test]
    fn titles() {
        assert_eq!(
            Kind::Art.title("art-13"),
            "chore(art): use AnkiQuest-Art art-13"
        );
        assert_eq!(
            Kind::Rules.title("rules-2"),
            "chore(rules): use AnkiQuest-Server rules-2"
        );
    }
}
