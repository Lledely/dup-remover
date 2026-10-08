#!/usr/bin/env bash
# Exercise the PR history check against real Git histories without changing master.
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
mkdir -p "$repository_root/target/ci-history-tests"
fixture="$(mktemp -d "$repository_root/target/ci-history-tests/run.XXXXXXXX")"
guard="$repository_root/scripts/check-pr-base.sh"

git -C "$fixture" init --quiet --initial-branch=master
git -C "$fixture" config user.name 'CI history tests'
git -C "$fixture" config user.email 'ci@example.invalid'
git -C "$fixture" commit --quiet --allow-empty -m 'Initial commit'
initial="$(git -C "$fixture" rev-parse HEAD)"
git -C "$fixture" switch --quiet -c feature
git -C "$fixture" commit --quiet --allow-empty -m 'Feature commit'
feature="$(git -C "$fixture" rev-parse HEAD)"

assert_status() {
  local expected="$1" description="$2" base="$3" head="$4" actual output
  if output="$(cd "$fixture" && bash "$guard" "$base" "$head" 2>&1)"; then
    actual=0
  else
    actual=$?
  fi
  if [[ "$actual" != "$expected" ]]; then
    printf 'FAIL: %s (expected %s, got %s)\n%s\n' "$description" "$expected" "$actual" "$output" >&2
    exit 1
  fi
  printf 'PASS: %s\n' "$description"
}

assert_status 0 'A linear branch on current master passes' "$initial" "$feature"
assert_status 0 'Identical base and head pass' "$initial" "$initial"
assert_status 1 'An unknown base fails' missing-base "$feature"
assert_status 1 'An unknown head fails' "$initial" missing-head

git -C "$fixture" switch --quiet master
git -C "$fixture" commit --quiet --allow-empty -m 'Master advances'
master="$(git -C "$fixture" rev-parse HEAD)"
assert_status 1 'A branch behind master fails' "$master" "$feature"

git -C "$fixture" switch --quiet feature
git -C "$fixture" rebase --quiet master
rebased="$(git -C "$fixture" rev-parse HEAD)"
assert_status 0 'Rebasing onto current master fixes the check' "$master" "$rebased"

git -C "$fixture" switch --quiet master
git -C "$fixture" commit --quiet --allow-empty -m 'Master advances again'
master="$(git -C "$fixture" rev-parse HEAD)"
git -C "$fixture" switch --quiet feature
git -C "$fixture" merge --quiet --no-ff master -m 'Merge master instead of rebasing'
merged="$(git -C "$fixture" rev-parse HEAD)"
assert_status 1 'Merging master instead of rebasing fails' "$master" "$merged"

printf 'All 7 PR history checks passed. Fixture: %s\n' "$fixture"
