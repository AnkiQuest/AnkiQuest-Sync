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
`chore(agents): update AGENTS.md from AnkiQuest-Agents`, and turns on
auto-merge (`tools/auto-merge.sh`): the pull request squash-merges, or joins
the merge queue where the base branch has one, once its checks pass. The
rules were reviewed in AnkiQuest-Agents, so nobody merges it by hand; the
repository must allow auto-merge, and its rules must not wait on a review
(see "Token"). It runs daily, on
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
pull request open, it adds to the branch instead of starting it again. Like
the `agents` job, it turns on auto-merge: the art was reviewed in
AnkiQuest-Art.

It runs on a `repository_dispatch` of type `art-released` with client payload
`{"tag": "art-<N>"}`, which AnkiQuest-Art's release workflow sends after
publishing; daily, taking the latest `art-<N>` release; and on demand, with an
optional tag. One run at a time, never cancelled. The logic is
`tools/art-pin`, a Rust tool tested by the `check` workflow
(`cargo test` in `tools/art-pin`).

## Watched sources

The forks' default branches also carry changes from sources that aren't our
upstream: JSchoreels' FSRS-7 fork and fsrs-rs fork, kuma3's RWKV core, backend
and app, and new upstream backend versions. Each entry in `watched.json` has a
`mode` saying how its changes reach the fork:

| Mode | Sources | What happens | Reviewed |
|---|---|---|---|
| `merge` | JSchoreels/anki `main`, kuma3's core `rwkv` (AnkiQuest-Anki); kuma3's backend `rwkv` (AnkiQuest-Android-Backend) | The `sync` job merges it like an upstream, on `sync/<owner>/<repo>-<ref>`, with the same pull request title, body and conflict handling. The fork's `AGENTS.md` is kept as it is. | Once the pull request is merged, the source's commits are in the fork's branch and the next run finds nothing new. No `reviewed` field. |
| `pin` | JSchoreels/fsrs-rs `main` (`fsrs` in AnkiQuest-Anki's `Cargo.toml`) | The `watch` job moves the `rev` in `Cargo.toml` to the source's head, runs `cargo update -p <package>`, pushes `sync/<owner>/<repo>-<ref>` and opens or updates one pull request, `chore(deps): bump <package> to <owner>/<repo> <commit>`. With that pull request open, it adds to its branch, so fixes pushed there stay. | The commit the fork's default branch pins. No `reviewed` field. |
| `port` | open-spaced-repetition/fsrs-rs (no fork or pin of ours: it reaches us through JSchoreels/fsrs-rs); kuma3's app `rwkv` (a different app, ported by hand); ankidroid/Anki-Android-Backend's version (`kind: version`, a release step: see its `why`) | The `watch` job opens or updates one issue in the fork it feeds, listing the new commits or the new version. A fork with issues disabled gets the issue here, in AnkiQuest-Sync, titled with the fork's name. | `reviewed`: after porting or skipping, set it to the reported commit or version; the issue closes on the next run. |

A `merge` or `pin` entry closes an issue left from when it was ported. The
first merge of a source whose commits we carried as rewritten ports brings
its whole history and conflicts where the ports differ; resolve them on the
branch once, and later pull requests bring only new commits. Every source is
checked even when one fails; the job then fails, naming the failed sources.
The logic is `tools/source-watch` (also the `sync` job's matrix, `source-watch
matrix forks.json watched.json`), a Rust tool tested by the `check` workflow.

## Mirrors

Some branches of ours mirror a source branch: AnkiQuest-Android's
`upstream-main` is ankidroid/Anki-Android `main`, the base of the
`upstream/*` branches proposed to AnkiDroid. They are listed in
`mirrors.json` (`source`, `ref`, `target`, `branch`, `why`). The `mirror` job
in `sync.yml` pushes each one to the source's commit directly, without a pull
request. It never forces the push and never writes a default branch (`master`
and `main` are refused, and so is whatever branch the target's `HEAD` names).
If the source was force-pushed or someone pushed to the mirror, the job fails
naming the mirror and leaves the branch as it is; reset it by hand. A
missing mirror is created. The logic is `source-watch mirror` in
`tools/source-watch`.

## Token

Needs the secret `FORK_SYNC_TOKEN`: a fine-grained token for the AnkiQuest
organisation with Contents, Workflows, Pull requests and Issues read and write
on the six forks (the `sync` job pushes upstream and watched sources' commits,
which may change workflows, to `sync/...` branches; the `watch` job pushes pin
branches and opens their pull requests in the `pin` entries' targets), Contents and Pull requests read
and write on the other repositories in `TARGETS` and in `art-consumers.json`, and Contents read on
AnkiQuest-Agents and AnkiQuest-Art. The `watch` job writes AnkiQuest-Sync's own
issues with the job's `GITHUB_TOKEN`.

- **Mirrors**: Contents and Workflows write on each target in `mirrors.json`
  (the mirrored upstream commits may change workflows); the six forks
  already have both.
- **Auto-merge**: Pull requests write on every repository in `TARGETS` and
  `art-consumers.json`, which they already have. Each of those repositories
  must allow auto-merge, and squash merging where it has no merge queue.
  Auto-merge waits for every rule of the base branch, review included, and a
  ruleset bypass does not apply to it: where a ruleset requires an approving
  review, the `agents-sync` and `art-release` pull requests still wait for one.

## Licence

AGPL-3.0-or-later ([LICENSE](LICENSE)).
