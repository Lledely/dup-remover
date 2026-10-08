#!/usr/bin/env bash
# Require the real PR head to descend linearly from the current base commit.
set -euo pipefail

if [[ "$#" != 2 ]]; then
  printf 'Usage: bash scripts/check-pr-base.sh <current-base> <pr-head>\n' >&2
  exit 1
fi

if ! base="$(git rev-parse --verify --end-of-options "$1^{commit}" 2>/dev/null)"; then
  printf 'Cannot resolve the current base commit.\n' >&2
  exit 1
fi
if ! head="$(git rev-parse --verify --end-of-options "$2^{commit}" 2>/dev/null)"; then
  printf 'Cannot resolve the PR head commit.\n' >&2
  exit 1
fi

if ! git merge-base --is-ancestor "$base" "$head"; then
  printf 'The PR branch is behind master. Run git fetch origin and git rebase origin/master.\n' >&2
  exit 1
fi

if [[ -n "$(git rev-list --merges "$base..$head")" ]]; then
  printf 'The PR branch contains merge commits. Rebase it onto master instead of merging master.\n' >&2
  exit 1
fi

printf 'PR history is linear and includes the current master commit %s.\n' "$base"
