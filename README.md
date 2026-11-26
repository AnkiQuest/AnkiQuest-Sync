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
left out), introduced by AnkiQuest-Agents' `upstream.md`. The forks are those
in `forks.json`; `fork-sync agents ... merged` finds the commit (the merge base
of the fork's branch and upstream's) and reads the files through the GitHub
API. The `sync` job writes the same section when it merges that commit, so this
job changes it only when `upstream.md` changes or a branch's copy is out of
date (edited by hand, or written in an older format); new upstream text
arrives with the sync pull request. `CLAUDE.md` is
`@AGENTS.md` in every repository. Where either differs, it commits both,
`chore(agents): update AGENTS.md from AnkiQuest-Agents`, and pushes the commit
straight to the default branch as a fast-forward: the rules were reviewed in
AnkiQuest-Agents, so nobody reviews or merges it again, and the token's
account bypasses the branch's ruleset. See "Landing" below. It runs daily, on
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
differs, it commits only that file, `chore(art): use AnkiQuest-Art <tag>`, and
pushes it straight to the default branch, as the `agents` job does: the art was
reviewed in AnkiQuest-Art. A fallback pull request (see "Landing") lists the
old and new tag, the Art compare link and the asset hashes that changed.

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

## Landing

`source-watch land` puts the `agents` and `art` jobs' files on each
repository's default branch:

1. It clones the default branch; if it already has the files, nothing
   happens (an open fallback pull request is closed).
2. It commits them, as `github-actions[bot]` like the jobs' other commits,
   and pushes the commit to the default branch as a plain fast-forward.
3. If the push is refused (the token's account isn't a bypass actor yet, or
   the branch moved), it clones the latest branch and tries once more. It
   skips that retry if the files themselves changed meanwhile.
4. Refused again, it pushes the commit to the fallback branch (`agents-sync`,
   `art-release`) and opens or updates one pull request from it with
   auto-merge on: it squash-merges, or joins the merge queue, once the
   branch's rules are met. The job summary says the push was refused and why.

Nothing is force-pushed, and the default branch is never deleted or rewritten.
Once a push lands, an open fallback pull request is closed.

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
- **Landing**: Contents write on every repository in `TARGETS` and
  `art-consumers.json` (already granted), and the token's account must be a
  bypass actor, with mode "always" so direct pushes pass, of each of those
  repositories' default-branch ruleset. Repositories without a ruleset need
  nothing. Without the bypass the push is refused and the fallback pull
  request is used: that needs Pull requests write (already granted) and
  auto-merge allowed in the repository, and still waits for any review the
  ruleset requires.

## Licence

AGPL-3.0-or-later ([LICENSE](LICENSE)).
