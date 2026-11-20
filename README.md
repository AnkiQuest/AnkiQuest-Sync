# AnkiQuest-Sync

Keeps AnkiQuest's forks up to date with upstream, once a day and on demand
(Actions, "Sync forks with upstream", Run workflow).

| Fork | Upstream | Mode |
|---|---|---|
| AnkiQuest/AnkiQuest-Android `main` | ankidroid/Anki-Android | pr |
| AnkiQuest/anki `main` | ankitects/anki | mirror |
| AnkiQuest/AnkiQuest-Android-Backend `main` | ankidroid/Anki-Android-Backend | mirror |
| AnkiQuest/garmankiquest `master` | sypianski/garmanki | pr |
| AnkiQuest/ankiquest-android-mcp `main` | ankimcp/ankidroid-mcp | pr |
| AnkiQuest/AnkiQuestconnectAndroid `master` | KamWithK/AnkiconnectAndroid | pr |

- **mirror:** the branch is a pure copy of upstream and is fast-forwarded. Our
  changes live on other branches (for example `rwkv`). If the branch has commits
  of its own, the job opens an issue in the fork and pushes nothing.
- **pr:** the branch carries our changes. The job opens one pull request from
  upstream's branch, lists conflicting files in the run summary, and leaves it
  open until merged. Merge with a merge commit.

The list of forks is the matrix in `.github/workflows/sync.yml`.

## AGENTS.md

The `agents` job (`.github/workflows/agents.yml`) writes each repository's
`AGENTS.md` from [AnkiQuest-Agents](https://github.com/AnkiQuest/AnkiQuest-Agents):
`shared.md`, a blank line, `repos/<repository>.md`. `CLAUDE.md` becomes
`@AGENTS.md`; a fork whose upstream has its own `CLAUDE.md` keeps it with
`@AGENTS.md` as its first line. Where either differs, it pushes branch
`agents-sync` and opens or updates one pull request, "Update AGENTS.md from
AnkiQuest-Agents". It runs daily, on demand, and on a `repository_dispatch` of
type `agents-changed`, which AnkiQuest-Agents sends when its `main` changes.
The repositories and their branches are the `TARGETS` map in that workflow;
for the mirror forks the branch is `ankiquest`.

## Watched sources

Our `ankiquest` branches also carry changes from sources that aren't our
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
repositories in `TARGETS`, and Contents read on AnkiQuest-Agents.

Needs the secret `FORK_SYNC_TOKEN`: a fine-grained token for the AnkiQuest
organisation with Contents, Workflows, Pull requests and Issues read and write
on the six forks, Contents and Pull requests read and write on the other
repositories in `TARGETS`, and Contents read on AnkiQuest-Agents.

## Licence

AGPL-3.0-or-later ([LICENSE](LICENSE)).
