#!/usr/bin/env bash
# Turns on auto-merge for a pull request the agents or art job opened: it
# squash-merges, or joins the merge queue where the base branch has one, once
# the branch's rules are met. Fails if the repository doesn't allow
# auto-merge.
#
# usage: auto-merge.sh <owner/repo> <number>
set -euo pipefail

if ! gh pr merge "$2" --repo "$1" --auto --squash; then
  echo "::error::$1#$2: auto-merge could not be turned on (allow auto-merge in the repository's settings)"
  exit 1
fi
