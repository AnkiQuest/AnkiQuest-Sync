// SPDX-License-Identifier: AGPL-3.0-or-later

//! A release's `SHA256SUMS`, and checking its pin file against it.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::pin::{self, FILE, Pin, SHA256_LEN};

/// `SHA256SUMS` as asset name to hash, in `sha256sum`'s format:
/// `<hash>  <name>`, or `<hash> *<name>` for binary mode.
pub fn parse(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (hash, rest) = line
            .split_once(' ')
            .ok_or_else(|| format!("SHA256SUMS: bad line {line:?}"))?;
        let name = rest.strip_prefix([' ', '*']).unwrap_or_default();
        if !pin::is_hex(hash, SHA256_LEN) || name.is_empty() {
            return Err(format!("SHA256SUMS: bad line {line:?}"));
        }
        if sums.insert(name.to_owned(), hash.to_owned()).is_some() {
            return Err(format!("SHA256SUMS: {name} is listed twice"));
        }
    }
    Ok(sums)
}

/// Checks a downloaded pin file: its own hash in `SHA256SUMS`, the tag it
/// was downloaded as, its shape, and each asset hash against `SHA256SUMS`.
pub fn verify(pin_bytes: &[u8], sums_text: &str, tag: &str) -> Result<Pin, String> {
    let sums = parse(sums_text)?;

    // The pin file itself, before anything in it is read
    let actual = hex(&Sha256::digest(pin_bytes));
    match sums.get(FILE) {
        None => return Err(format!("SHA256SUMS does not list {FILE}")),
        Some(listed) if *listed != actual => {
            return Err(format!(
                "{FILE}: SHA-256 {actual}, SHA256SUMS says {listed}"
            ));
        }
        Some(_) => {}
    }

    let pin = Pin::parse(pin_bytes)?;
    if pin.tag.as_deref() != Some(tag) {
        return Err(format!("{FILE}: tag {:?}, release {tag}", pin.tag));
    }
    pin.check_release()?;

    // The hashes consumers will check the assets against
    for (name, hash) in &pin.assets {
        match sums.get(name) {
            None => return Err(format!("SHA256SUMS does not list {name}")),
            Some(listed) if listed != hash => {
                return Err(format!("{FILE}: {name} {hash}, SHA256SUMS says {listed}"));
            }
            Some(_) => {}
        }
    }
    Ok(pin)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pin::ASSETS;

    const TAG: &str = "art-13";

    fn pin_json(tag: &str) -> String {
        let assets: Vec<String> = ASSETS
            .iter()
            .enumerate()
            .map(|(i, n)| format!("\"{n}\": \"{}\"", hash_of(i)))
            .collect();
        format!(
            "{{\"tag\": \"{tag}\", \"commit\": \"{}\", \"assets\": {{{}}}}}\n",
            "1".repeat(40),
            assets.join(", ")
        )
    }

    fn hash_of(i: usize) -> String {
        format!("{i:x}").repeat(SHA256_LEN)
    }

    /// SHA256SUMS for `pin`, listing every asset and the pin file.
    fn sums_for(pin: &str) -> String {
        let mut text: String = ASSETS
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{}  {n}\n", hash_of(i)))
            .collect();
        text.push_str(&format!("{} *{FILE}\n", hex(&Sha256::digest(pin))));
        text
    }

    #[test]
    fn good_release() {
        let pin = pin_json(TAG);
        let checked = verify(pin.as_bytes(), &sums_for(&pin), TAG).unwrap();
        assert_eq!(checked.tag.as_deref(), Some(TAG));
    }

    #[test]
    fn pin_file_altered() {
        let pin = pin_json(TAG);
        let sums = sums_for(&pin);
        let altered = pin.replace("\"tag\"", "\"tag\" ");
        assert!(
            verify(altered.as_bytes(), &sums, TAG)
                .unwrap_err()
                .contains("SHA-256")
        );
    }

    #[test]
    fn pin_file_unlisted() {
        let pin = pin_json(TAG);
        let sums: String = sums_for(&pin)
            .lines()
            .filter(|l| !l.contains(FILE))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(
            verify(pin.as_bytes(), &sums, TAG)
                .unwrap_err()
                .contains("does not list")
        );
    }

    #[test]
    fn other_tag() {
        let pin = pin_json("art-12");
        assert!(
            verify(pin.as_bytes(), &sums_for(&pin), TAG)
                .unwrap_err()
                .contains("release art-13")
        );
    }

    #[test]
    fn asset_hash_differs() {
        let pin = pin_json(TAG);
        let sums = sums_for(&pin).replacen(&hash_of(2), &hash_of(9), 1);
        assert!(
            verify(pin.as_bytes(), &sums, TAG)
                .unwrap_err()
                .contains("masters.zip")
        );
    }

    #[test]
    fn bad_sums() {
        assert!(parse("abc  x\n").is_err());
        assert!(parse(&format!("{}\n", "a".repeat(64))).is_err());
        let twice = format!("{0}  x\n{0}  x\n", "a".repeat(64));
        assert!(parse(&twice).unwrap_err().contains("twice"));
    }
}
