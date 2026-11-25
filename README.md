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

Each fork's default branch carries upstream plus our changes. The `sync` job
merges upstream's branch into `sync/<owner>/<repo>` on the fork and opens one
pull request from it, `chore(sync): merge upstream <owner>/<repo> at <commit>`.
Later runs merge newer upstream commits into the same branch and update that
pull request; it stays open until merged.

- **Conflicts** never stop the merge. Conflicting files are committed with
  their markers and submodules keep our commit; the pull request lists both
  and opens as a draft. Fix them on the `sync/...` branch and push. The next
  run adds to the branch and rewrites the list.
- **Merging**: with a merge commit, outside the merge queue (the queue
  squashes), so the fork keeps upstream's history.
- **AGENTS.md**: the merge writes `AGENTS.md` and `CLAUDE.md` as the `agents`
  job does, with the "Upstream guidance" section taken from the merged
  upstream commit. When upstream changes its own `CLAUDE.md` or `AGENTS.md`,
  the pull request says so.

The forks are listed in `forks.json`, read by `sync.yml` and `agents.yml`. The
logic is `tools/fork-sync`, a Rust tool tested by the `check` workflow
(`cargo test` in `tools/fork-sync`); the workflows only call `gh` and `git`
around it.

## AGENTS.md

The `agents` job (`.github/workflows/agents.yml`) writes each repository's
`AGENTS.md` from [AnkiQuest-Agents](https://github.com/AnkiQuest/AnkiQuest-Agents):
`shared.md`, a blank line, `repos/<repository>.md`. A fork's `AGENTS.md` ends
with an "Upstream guidance" section: upstream's own `CLAUDE.md` and
`AGENTS.md`, verbatim, at the upstream commit the fork last merged (files that
only point at another, such as `@AGENTS.md` or a symlink, and duplicates are
left out). The `sync` job writes that section; this job keeps it as the fork's
branch has it, so the two never change the same text. `CLAUDE.md` is
`@AGENTS.md` in every repository. Where either differs, it pushes branch
`agents-sync` and opens or updates one pull request,
`chore(agents): update AGENTS.md from AnkiQuest-Agents`. It runs daily, on
demand, and on a `repository_dispatch` of type `agents-changed`, which
AnkiQuest-Agents sends when its default branch changes. The repositories are
the `TARGETS` list in that workflow; `AGENTS.md` lives on each one's default
branch, read from GitHub.

## AnkiQuest-Art releases

The `art` job (`.github/workflows/art.yml`) keeps every repository that uses
companion art on the latest [AnkiQuest-Art](https://github.com/AnkiQuest/AnkiQuest-Art)
release. The repositories are listed in `art-consumers.json`: AnkiQuest-Server,
AnkiQuest-Desktop, AnkiQuest-Android and AnkiQuest.github.io.

Each carries the pin file `ankiquest-art.json` at its root: the release's
`tag`, the Art `commit` it was built from, and the SHA-256 of every release
asset (`android-drawables.zip`, `addon-art.zip`, `masters.zip`,
`archive.zip`, `manifest.json`, `poses.json`) and, from the releases that
publish companion packs, of `packs.json` and every `pack-<id>.zip` (the app
bundles or downloads them). Every release publishes its own
pin file as an asset, listed in its `SHA256SUMS`. A repository reads the
entries it needs and checks each asset's hash before using it; before the
first release its file may have `"tag": null`.

The job downloads the release's `ankiquest-art.json` and `SHA256SUMS`, checks
the pin file against them, and compares it byte for byte with each
repository's copy on its default branch (a missing file differs). Where it
differs, it pushes branch `art-release` with only that file replaced and opens
or updates one pull request, `chore(art): use AnkiQuest-Art <tag>`, listing the old and
new tag, the Art compare link and the asset hashes that changed. With that
pull request open, it adds to the branch instead of starting it again.

It runs on a `repository_dispatch` of type `art-released` with client payload
`{"tag": "art-<N>"}`, which AnkiQuest-Art's release workflow sends after
publishing; daily, taking the latest `art-<N>` release; and on demand, with an
optional tag. One run at a time, never cancelled. The logic is
`tools/art-pin`, a Rust tool tested by the `check` workflow
(`cargo test` in `tools/art-pin`).

## Watched sources

The forks' default branches also carry changes from sources that aren't our
upstream: JSchoreels' FSRS-7 fork and fsrs-rs fork, kuma3's RWKV core, backend
and app, and new upstream backend versions. Their changes need porting by
hand, so the `watch` job only reports them and opens no merge pull request:

- JSchoreels/anki and kuma3's core, backend and app are carried as rewritten
  commits ("Carry the fork's FSRS-7 scheduler"), not merged; none of their
  reviewed commits is in our history. A merge would bring all their commits
  again, hundreds for JSchoreels/anki, on top of our ports.
- JSchoreels/fsrs-rs is a dependency pinned by commit in AnkiQuest-Anki's
  `Cargo.toml`, and open-spaced-repetition/fsrs-rs is its upstream; there is
  no fork of ours to merge into. Moving the pin is a code change.
- ankidroid/Anki-Android-Backend's version is a release step, not a merge;
  its code already comes through the `sync` job.

For each entry in `watched.json` whose source has moved past `reviewed`, the
job opens (or updates) one issue in the
fork it feeds, listing the new commits or the new version. After porting or
skipping them, set that entry's `reviewed` to the reported commit or version;
the issue closes on the next run.

## Token

Needs the secret `FORK_SYNC_TOKEN`: a fine-grained token for the AnkiQuest
organisation with Contents, Workflows, Pull requests and Issues read and write
on the six forks, Contents and Pull requests read and write on the other
repositories in `TARGETS` and in `art-consumers.json`, and Contents read on
AnkiQuest-Agents and AnkiQuest-Art.

## Licence

AGPL-3.0-or-later ([LICENSE](LICENSE)).
