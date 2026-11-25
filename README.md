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
`shared.md`, a blank line, `repos/<repository>.md`; a fork's AGENTS.md then
carries its upstream's own guidance (the upstream's `CLAUDE.md`, or `AGENTS.md`
where `CLAUDE.md` only points at it; nothing when the upstream has neither)
under an "Upstream guidance" heading, so upstream's rules are kept and tracked
rather than overwritten. `CLAUDE.md` becomes `@AGENTS.md` in every repository,
since the upstream guidance now lives in `AGENTS.md`. Where either differs, it
pushes branch `agents-sync` and opens or updates one pull request, "Update AGENTS.md from
AnkiQuest-Agents". It runs daily, on demand, and on a `repository_dispatch` of
type `agents-changed`, which AnkiQuest-Agents sends when its default branch changes.
The repositories are the `TARGETS` list in that workflow; `AGENTS.md` lives on
each one's default branch, read from GitHub.

## Release pins

Two kinds of release are pinned in the repositories that use them, each by a
pin file at the repository's root:

| Kind | Publisher | Tags | Pin file | Pull request | Job |
| --- | --- | --- | --- | --- | --- |
| `art` | [AnkiQuest-Art](https://github.com/AnkiQuest/AnkiQuest-Art) | `art-<N>` | `ankiquest-art.json` | "Use AnkiQuest-Art <tag>" | `art` (`.github/workflows/art.yml`) |
| `rules` | [AnkiQuest-Server](https://github.com/AnkiQuest/AnkiQuest-Server) (`crates/rules`) | `rules-<N>` | `ankiquest-rules.json` | "chore: use rules-<N>" | `rules` (`.github/workflows/rules.yml`) |

`release-consumers.json` lists the repositories of each kind: for `art`,
AnkiQuest-Server, AnkiQuest-Desktop, AnkiQuest-Android and AnkiQuest.github.io;
for `rules`, AnkiQuest-Desktop.

A pin file holds the release's `tag`, the publisher `commit` it was built
from, and the SHA-256 of every release asset. Art's assets are
`android-drawables.zip`, `addon-art.zip`, `masters.zip`, `archive.zip`,
`manifest.json` and `poses.json`; the rules' are `ankiquest_rules.wasm`,
`ankiquest_rules.py` and the native library of each desktop platform. Every
release publishes its own pin file as an asset, listed in its `SHA256SUMS`.
A repository reads the entries it needs and checks each asset's hash before
using it; before the first release its file may have `"tag": null`.

Both jobs call `.github/workflows/pins.yml`. It downloads the release's pin
file and `SHA256SUMS`, checks the pin file against them, and compares it byte
for byte with each repository's copy on its default branch (a missing file
differs). Where it differs, it pushes branch `<kind>-release` with only that
file replaced and opens or updates one pull request, listing the old and new
tag, the publisher's compare link and the asset hashes that changed. With
that pull request open, it adds to the branch instead of starting it again.

Each job runs on a `repository_dispatch` (`art-released` or `rules-released`,
client payload `{"tag": "<tag>"}`) that the publisher's release workflow sends
after publishing; daily, taking the latest release of its kind; and on demand,
with an optional tag. One run of each kind at a time, never cancelled. The
logic is `tools/release-pin` (`release-pin <kind> <command>`), a Rust tool
tested by the `check` workflow (`cargo test` in `tools/release-pin`).

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
repositories in `TARGETS` and in `release-consumers.json`, and Contents read on
AnkiQuest-Agents, AnkiQuest-Art and AnkiQuest-Server.

## Licence

AGPL-3.0-or-later ([LICENSE](LICENSE)).
