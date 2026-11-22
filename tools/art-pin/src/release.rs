// SPDX-License-Identifier: AGPL-3.0-or-later

//! Finding AnkiQuest-Art's latest release.

use serde::Deserialize;
use serde_json::Value;

use crate::pin;

/// The fields read from GitHub's release objects.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// The published `art-<N>` release with the highest N, from the output of
/// `gh api --paginate repos/AnkiQuest/AnkiQuest-Art/releases`: one JSON
/// array per page. None when there is no such release yet.
///
/// `at_least` is a tag from an `art-released` dispatch: the listing may
/// not show that release yet, and an older dispatch must not move a
/// consumer back.
pub fn latest(json: &str, at_least: Option<&str>) -> Result<Option<String>, String> {
    let mut best: Option<(u64, String)> = match at_least {
        None => None,
        Some(tag) => {
            let number =
                pin::tag_number(tag).ok_or_else(|| format!("{tag:?} is not an art-<N> tag"))?;
            Some((number, tag.to_owned()))
        }
    };
    for page in serde_json::Deserializer::from_str(json).into_iter::<Value>() {
        let page = page.map_err(|e| format!("releases: {e}"))?;
        let releases: Vec<Release> = match page {
            Value::Array(_) => serde_json::from_value(page),
            _ => serde_json::from_value(page).map(|r| vec![r]),
        }
        .map_err(|e| format!("releases: {e}"))?;

        // Drafts and prereleases are not for consumers
        for release in releases.into_iter().filter(|r| !r.draft && !r.prerelease) {
            let Some(number) = pin::tag_number(&release.tag_name) else {
                continue;
            };
            if best.as_ref().is_none_or(|(n, _)| number > *n) {
                best = Some((number, release.tag_name));
            }
        }
    }
    Ok(best.map(|(_, tag)| tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_release() {
        assert_eq!(latest("", None), Ok(None));
        assert_eq!(latest("[]", None), Ok(None));
        assert_eq!(latest(r#"[{"tag_name": "v1.0"}]"#, None), Ok(None));
    }

    #[test]
    fn highest_number() {
        // GitHub lists by date; numbers decide, across pages
        let pages = r#"[{"tag_name": "art-9"}, {"tag_name": "art-10"}] [{"tag_name": "art-2"}]"#;
        assert_eq!(latest(pages, None), Ok(Some("art-10".into())));
    }

    #[test]
    fn skips_drafts_and_prereleases() {
        let json = r#"[
            {"tag_name": "art-5", "draft": true},
            {"tag_name": "art-4", "prerelease": true},
            {"tag_name": "art-3", "draft": false, "prerelease": false}
        ]"#;
        assert_eq!(latest(json, None), Ok(Some("art-3".into())));
    }

    #[test]
    fn dispatched_tag() {
        // Not listed yet: the dispatched release
        assert_eq!(latest("[]", Some("art-4")), Ok(Some("art-4".into())));

        // A late dispatch for an older release: still the latest
        let json = r#"[{"tag_name": "art-5"}]"#;
        assert_eq!(latest(json, Some("art-4")), Ok(Some("art-5".into())));

        assert!(latest("[]", Some("main")).is_err());
    }

    #[test]
    fn bad_json() {
        assert!(latest("{", None).is_err());
    }
}
