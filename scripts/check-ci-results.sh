#!/usr/bin/env bash
# Require successful Rust checks and tests; PRs also require a current linear base.
set -euo pipefail

require_success() {
  local name="$1" result="$2"
  if [[ "$result" != success ]]; then
    printf '%s must succeed; received %s.\n' "$name" "${result:-missing}" >&2
    exit 1
  fi
}

require_success 'Rust checks' "${CHECKS_RESULT:-}"
require_success 'Tests' "${TESTS_RESULT:-}"
if [[ "${EVENT_NAME:-}" == pull_request ]]; then
  require_success 'PR rebase' "${HISTORY_RESULT:-}"
fi
