#!/usr/bin/env bash
# Verify that incomplete or unsuccessful checks cannot open the CI merge gate.
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(git -C "$script_dir" rev-parse --show-toplevel)"
guard="$repository_root/scripts/check-ci-results.sh"
passed=0

assert_status() {
  local expected="$1" description="$2" event="$3" checks="$4" tests="$5" history="$6" actual output
  if output="$(
    CHECKS_RESULT="$checks" TESTS_RESULT="$tests" HISTORY_RESULT="$history" EVENT_NAME="$event" \
      bash "$guard" 2>&1
  )"; then
    actual=0
  else
    actual=$?
  fi
  if [[ "$actual" != "$expected" ]]; then
    printf 'FAIL: %s (expected %s, got %s)\n%s\n' "$description" "$expected" "$actual" "$output" >&2
    exit 1
  fi
  passed=$((passed + 1))
  printf 'PASS: %s\n' "$description"
}

assert_status 0 'A PR with all checks successful passes' pull_request success success success
assert_status 0 'A push with successful tests needs no PR history check' push success success skipped
assert_status 0 'A manual run with successful tests needs no PR history check' workflow_dispatch success success skipped

for event in pull_request push workflow_dispatch; do
  for result in failure cancelled skipped ''; do
    assert_status 1 "Tests result '${result:-missing}' blocks $event" "$event" success "$result" success
  done
done

for result in failure cancelled skipped ''; do
  assert_status 1 "Rust checks result '${result:-missing}' blocks merging" pull_request "$result" success success
  assert_status 1 "PR history result '${result:-missing}' blocks merging" pull_request success success "$result"
done

printf 'All %s CI result checks passed.\n' "$passed"
