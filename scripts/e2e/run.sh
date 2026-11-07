#!/usr/bin/env bash
# e2e entrypoint: discovers scenarios, runs each in isolation (catalog reset
# + its own accounts), aggregates a PASS/FAIL report, exits non-zero on any
# failure.
#
# Usage:
#   scripts/e2e/run.sh                  # all scenarios, cluster mode
#   scripts/e2e/run.sh --only rules     # subset (comma-separated names)
#   scripts/e2e/run.sh --list           # list scenarios and exit
#   scripts/e2e/run.sh --local          # host-process mode instead of the
#                                       # deployed cluster market
#
# Preconditions: supabase stack running (~/projects/commons/supabase); in
# cluster mode also `deploy/deploy.sh --tag <tag>` applied.
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ONLY=""
LIST_ONLY=false
while [ $# -gt 0 ]; do
  case "$1" in
    --list) LIST_ONLY=true ;;
    --only) ONLY="${2:?--only requires a comma-separated list}"; shift ;;
    --only=*) ONLY="${1#--only=}" ;;
    --local) LOCAL_MODE=true ;;
    *) echo "unknown arg: $1 (use --only <names> / --list / --local)" >&2; exit 1 ;;
  esac
  shift
done

# --- discover scenarios (source files in name order; each registers itself) --
for file in "$E2E_ROOT"/scenarios/*.sh; do
  # shellcheck disable=SC1090
  source "$file"
done

if [ "${LIST_ONLY:-}" = true ]; then
  for name in "${SCENARIOS[@]}"; do echo "$name"; done
  exit 0
fi

load_admin_tools
ensure_services || exit 1
trap 'teardown_accounts; stop_services' EXIT

PASSED=()
# Run selected scenarios; one failure never masks the rest.
for i in "${!SCENARIOS[@]}"; do
  name="${SCENARIOS[$i]}"
  if [ -n "$ONLY" ]; then
    case ",$ONLY," in
      *",${name},"*) ;;
      *) continue ;;
    esac
  fi
  run_scenario "$name" "${SCENARIO_FN[$i]}"
done

echo
echo "════ e2e summary ════"
if [ "${#PASSED[@]}" -gt 0 ]; then
  for name in "${PASSED[@]}"; do echo "  PASS $name"; done
fi
if [ "${#FAILED[@]}" -gt 0 ]; then
  for name in "${FAILED[@]}"; do echo "  FAIL $name"; done
  echo "${#PASSED[@]} passed, ${#FAILED[@]} failed"
  exit 1
fi
echo "${#PASSED[@]} passed, all green"
