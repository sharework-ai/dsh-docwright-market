#!/usr/bin/env bash
# Compatibility shim: the e2e suite lives in scripts/e2e/ (run.sh selects and
# aggregates scenarios; scenarios/ holds one file per theme).
exec "$(dirname "$0")/e2e/run.sh" "$@"
