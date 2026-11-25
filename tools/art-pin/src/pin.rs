// SPDX-License-Identifier: AGPL-3.0-or-later

//! The pin file, `ankiquest-art.json`, and AnkiQuest-Art's release tags.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The pin file's name, at each consumer's root and among a release's assets.
pub const FILE: &str = "ankiquest-art.json";

/// The repository that publishes the releases.
pub const ART_REPO: &str = "AnkiQuest/AnkiQuest-Art";

/// Every asset a release's pin file lists.
pub const ASSETS: [&str; 6] = [
    "android-drawables.zip",
    "addon-art.zip",
    "masters.zip",
    "archive.zip",
    "manifest.json",
    "poses.json",
];

/// The release index of the companion packs; a release lists it with its packs or not at all.
pub const PACKS_INDEX: &str = "packs.json";

/// A companion pack is `pack-<id>.zip`, `<id>` as AnkiQuest-Art's docs/packs.md defines it.
const PACK_PREFIX: &str = "pack-";
const PACK_SUFFIX: &str = ".zip";
const PACK_ID_MAX: usize = 32;

const TAG_PREFIX: &str = "art-";
const COMMIT_LEN: usize = 40;
pub const SHA256_LEN: usize = 64;

/// A pin file. Before the first release, a consumer's has `"tag": null`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pin {
    pub tag: Option<String>,
    pub commit: Option<String>,
    #[serde(default)]
    pub assets: BTreeMap<String, String>,
}

impl Pin {
    pub fn parse(text: &[u8]) -> Result<Pin, String> {
        serde_json::from_slice(text).map_err(|e| format!("{FILE}: {e}"))
    }

    /// Checks a release's pin file: a release tag, a commit, and a hash
    /// for every asset and nothing else.
    pub fn check_release(&self) -> Result<(), String> {
        let Some(tag) = self.tag.as_deref() else {
            return Err(format!("{FILE}: tag is null"));
        };
        if tag_number(tag).is_none() {
            return Err(format!("{FILE}: {tag:?} is not an art-<N> tag"));
        }

        let commit = self.commit.as_deref().unwrap_or_default();
        if !is_hex(commit, COMMIT_LEN) {
            return Err(format!("{FILE}: commit {commit:?} is not 40 hex digits"));
        }

        // Every consumer reads the same file, so every asset is listed.
        for name in ASSETS {
            let Some(hash) = self.assets.get(name) else {
                return Err(format!("{FILE}: {name} is missing"));
            };
            if !is_hex(hash, SHA256_LEN) {
                return Err(format!("{FILE}: {name}'s hash is not 64 hex digits"));
            }
        }
        let known = |n: &str| ASSETS.contains(&n) || n == PACKS_INDEX || is_pack(n);
        if let Some(name) = self.assets.keys().find(|n| !known(n)) {
            return Err(format!("{FILE}: unknown asset {name}"));
        }

        // The companion packs come with their index, or neither is there.
        let packs = self.assets.keys().any(|n| is_pack(n));
        if packs != self.assets.contains_key(PACKS_INDEX) {
            return Err(format!(
                "{FILE}: {PACKS_INDEX} and the pack-*.zip assets go together"
            ));
        }
        if let Some((name, _)) = self
            .assets
            .iter()
            .find(|(n, h)| is_pack(n) && !is_hex(h, SHA256_LEN))
        {
            return Err(format!("{FILE}: {name}'s hash is not 64 hex digits"));
        }
        if let Some(hash) = self.assets.get(PACKS_INDEX)
            && !is_hex(hash, SHA256_LEN)
        {
            return Err(format!("{FILE}: {PACKS_INDEX}'s hash is not 64 hex digits"));
        }
        Ok(())
    }
}

/// The release number of a tag such as `art-12`; None for anything else.
pub fn tag_number(tag: &str) -> Option<u64> {
    let digits = tag.strip_prefix(TAG_PREFIX)?;
    if digits.is_empty() || digits.starts_with('0') {
        return None;
    }
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Whether `name` is a companion pack, `pack-<id>.zip`: `<id>` is a lower-case
/// letter, then up to 31 lower-case letters, digits or underscores.
fn is_pack(name: &str) -> bool {
    let Some(id) = name
        .strip_prefix(PACK_PREFIX)
        .and_then(|n| n.strip_suffix(PACK_SUFFIX))
    else {
        return false;
    };
    let mut bytes = id.bytes();
    let first_ok = bytes.next().is_some_and(|b| b.is_ascii_lowercase());
    first_ok
        && id.len() <= PACK_ID_MAX
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

/// Whether `text` is `len` lowercase hex digits.
pub fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Pin {
        Pin {
            tag: Some(tag.to_owned()),
            commit: Some("c".repeat(COMMIT_LEN)),
            assets: ASSETS
                .iter()
                .map(|n| (n.to_string(), "a".repeat(SHA256_LEN)))
                .collect(),
        }
    }

    #[test]
    fn tags() {
        assert_eq!(tag_number("art-1"), Some(1));
        assert_eq!(tag_number("art-12"), Some(12));
        for bad in [
            "art-", "art-01", "art-1a", "art--1", "v1", "art-1 ", "ART-1",
        ] {
            assert_eq!(tag_number(bad), None, "{bad}");
        }
    }

    #[test]
    fn release_pin() {
        assert_eq!(release("art-3").check_release(), Ok(()));
    }

    #[test]
    fn null_tag() {
        let pin = Pin::parse(br#"{"_about": "x", "tag": null}"#).unwrap();
        assert_eq!(pin.tag, None);
        assert!(pin.check_release().is_err());
    }

    #[test]
    fn missing_asset() {
        let mut pin = release("art-3");
        pin.assets.remove("archive.zip");
        assert!(pin.check_release().unwrap_err().contains("archive.zip"));
    }

    #[test]
    fn unknown_asset() {
        let mut pin = release("art-3");
        pin.assets
            .insert("extra.zip".into(), "a".repeat(SHA256_LEN));
        assert!(pin.check_release().unwrap_err().contains("extra.zip"));
    }

    /// A release pin with the companion packs: the index and one zip per companion.
    fn with_packs(tag: &str) -> Pin {
        let mut pin = release(tag);
        for name in ["packs.json", "pack-aki.zip", "pack-anki_tan.zip"] {
            pin.assets.insert(name.into(), "a".repeat(SHA256_LEN));
        }
        pin
    }

    #[test]
    fn packs() {
        assert_eq!(with_packs("art-9").check_release(), Ok(()));
    }

    #[test]
    fn packs_without_index() {
        let mut pin = with_packs("art-9");
        pin.assets.remove("packs.json");
        assert!(pin.check_release().unwrap_err().contains("packs.json"));
    }

    #[test]
    fn index_without_packs() {
        let mut pin = with_packs("art-9");
        pin.assets.remove("pack-aki.zip");
        pin.assets.remove("pack-anki_tan.zip");
        assert!(pin.check_release().unwrap_err().contains("packs.json"));
    }

    #[test]
    fn bad_pack_names() {
        for bad in [
            "pack-.zip",
            "pack-Aki.zip",
            "pack-1aki.zip",
            "pack-a/b.zip",
            "pack-aki.json",
        ] {
            let mut pin = with_packs("art-9");
            pin.assets.insert(bad.into(), "a".repeat(SHA256_LEN));
            assert!(pin.check_release().unwrap_err().contains(bad), "{bad}");
        }
    }

    #[test]
    fn bad_pack_hash() {
        let mut pin = with_packs("art-9");
        pin.assets.insert("pack-aki.zip".into(), "a".repeat(63));
        assert!(pin.check_release().unwrap_err().contains("pack-aki.zip"));
    }

    #[test]
    fn bad_hashes() {
        let mut pin = release("art-3");
        pin.commit = Some("C".repeat(COMMIT_LEN));
        assert!(pin.check_release().is_err());

        let mut pin = release("art-3");
        pin.assets.insert("poses.json".into(), "a".repeat(63));
        assert!(pin.check_release().is_err());
    }
}
