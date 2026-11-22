# AnkiQuest-Sync

Keeps AnkiQuest's forks up to date with upstream, once a day and on demand
(Actions, "Sync forks with upstream", Run workflow).

| Fork (default branch) | Upstream |
|---|---|
| AnkiQuest/AnkiQuest-Android | ankidroid/Anki-Android `main` |
| AnkiQuest/AnkiQuest-Anki | ankitects/anki `main` |
| AnkiQuest/AnkiQuest-Android-Backend | ankidroid/Anki-Android-Backend `main` |
| AnkiQuest/AnkiQuest-GarmAnki | sypianski/garmanki `master` |
| AnkiQuest/AnkiQuest-Android-MCP | ankimcp/ankidroid-mcp `main` |
| AnkiQuest/AnkiQuest-AnkiConnectAndroid | KamWithK/AnkiconnectAndroid `master` |

Each fork's default branch carries upstream plus our changes. The job opens one pull
request from upstream's branch, lists conflicting files in the run summary,
and leaves it open until merged. Merge with a merge commit.

The list of forks is the matrix in `.github/workflows/sync.yml`.

## AGENTS.md

The `agents` job (`.github/workflows/agents.yml`) writes each repository's
`AGENTS.md` from [AnkiQuest-Agents](https://github.com/AnkiQuest/AnkiQuest-Agents):
`shared.md`, a blank line, `repos/<repository>.md`. `CLAUDE.md` becomes
`@AGENTS.md`; a fork whose upstream has its own `CLAUDE.md` keeps it with
`@AGENTS.md` as its first line. Where either differs, it pushes branch
`agents-sync` and opens or updates one pull request, "Update AGENTS.md from
AnkiQuest-Agents". It runs daily, on demand, and on a `repository_dispatch` of
type `agents-changed`, which AnkiQuest-Agents sends when its default branch changes.
The repositories are the `TARGETS` list in that workflow; `AGENTS.md` lives on
each one's default branch, read from GitHub.

## AnkiQuest-Art releases

The `art` job (`.github/workflows/art.yml`) keeps every repository that uses
companion art on the latest [AnkiQuest-Art](https://github.com/AnkiQuest/AnkiQuest-Art)
release. The repositories are listed in `art-consumers.json`: AnkiQuest-Server,
AnkiQuest-Desktop, AnkiQuest-Android and AnkiQuest.github.io.

Each carries the pin file `ankiquest-art.json` at its root: the release's
`tag`, the Art `commit` it was built from, and the SHA-256 of every release
asset (`android-drawables.zip`, `addon-art.zip`, `masters.zip`,
`archive.zip`, `manifest.json`, `poses.json`). Every release publishes its own
pin file as an asset, listed in its `SHA256SUMS`. A repository reads the
entries it needs and checks each asset's hash before using it; before the
first release its file may have `"tag": null`.

The job downloads the release's `ankiquest-art.json` and `SHA256SUMS`, checks
the pin file against them, and compares it byte for byte with each
repository's copy on its default branch (a missing file differs). Where it
differs, it pushes branch `art-release` with only that file replaced and opens
or updates one pull request, "Use AnkiQuest-Art <tag>", listing the old and
new tag, the Art compare link and the asset hashes that changed. With that
pull request open, it adds to the branch instead of starting it again.

It runs on a `repository_dispatch` of type `art-released` with client payload
`{"tag": "art-<N>"}`, which AnkiQuest-Art's release workflow sends after
publishing; daily, taking the latest `art-<N>` release; and on demand, with an
optional tag. One run at a time, never cancelled. The logic is
`tools/art-pin`, a Rust tool tested by the `check` workflow
(`cargo test` in `tools/art-pin`).

## Wiring map

The `wiring` job (`.github/workflows/wiring.yml`) keeps
[AnkiQuest.github.io](https://github.com/AnkiQuest/AnkiQuest.github.io)'s wiring
map current. Every 30 minutes it reads the last map built (`wiring.json` on that
repository's default branch or its `wiring-update` branch, whichever was
generated last) and compares what it recorded with the sources now: each
repository's default-branch head, the branch tip each pin was compared with,
and the newest Maven Central release of each pinned artifact. Commits there
that only change the map's own files don't count. If anything moved and no
map build is queued or running, it sends a `repository_dispatch` of type
`wiring-changed` there, which rebuilds the map. It keeps no state: the
rebuilt map records the new values. The `agents` and `art` jobs send the
same dispatch after opening or updating pull requests. The logic is
`tools/wiring-check`, a Rust tool tested by the `check` workflow.

## Watched sources

The forks' default branches also carry changes from sources that aren't our
upstream: JSchoreels' FSRS-7 fork and fsrs-rs fork, kuma3's RWKV core, backend
and app, and new upstream backend versions. Their changes need porting by
hand, so the `watch` job only reports them: for each entry in `watched.json`
whose source has moved past `reviewed`, it opens (or updates) one issue in the
fork it feeds, listing the new commits or the new version. After porting or
skipping them, set that entry's `reviewed` to the reported commit or version;
the issue closes on the next run.

## Token

Needs the secret `FORK_SYNC_TOKEN`: a fine-grained token for the AnkiQuest
organisation with Contents, Workflows, Pull requests and Issues read and write
on the six forks, Contents and Pull requests read and write on the other
repositories in `TARGETS` and in `art-consumers.json`, and Contents read on
AnkiQuest-Agents and AnkiQuest-Art. The `wiring-changed` dispatch needs
Contents write on AnkiQuest.github.io, which `TARGETS` already grants.

## Licence

AGPL-3.0-or-later ([LICENSE](LICENSE)).
