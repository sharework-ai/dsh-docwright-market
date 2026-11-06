#!/usr/bin/env bash
# Consumer side: install from the live market into a fresh packsDir, then
# re-verify the installed copy with docwright's own pack:check, plus --list
# and the --force overwrite path.

scenario consumer-install scenario_consumer_install
scenario_consumer_install() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  publish_sample_pack "$LAST_TOKEN" || return 1

  # --list sees the market (against the live index).
  local listing
  listing="$(node "$M/install.mjs" --list --market "$MARKET")"
  assert_contains "$listing" "task-card@0.1.0" "--list shows the published pack" || return 1

  # Install into a fresh packsDir.
  local into="$T/consumer-packs"
  node "$M/install.mjs" task-card --market "$MARKET" --into "$into" >/dev/null \
    || { sc_fail "install.mjs failed"; return 1; }
  [ -f "$into/task-card/manifest.json" ] \
    || { sc_fail "pack not landed in packsDir"; return 1; }

  # The installed copy must pass docwright's own gate (the market publishes
  # exactly what it verified).
  ( cd "$DW" && pnpm run --silent pack:check -- "$into" --with packs ) \
    | grep -q "PASS" || { sc_fail "pack:check on installed copy failed"; return 1; }

  # Re-install without --force conflicts (exit 3); with --force succeeds.
  node "$M/install.mjs" task-card --market "$MARKET" --into "$into" >/dev/null 2>&1 \
    && { sc_fail "duplicate install must fail without --force"; return 1; }
  node "$M/install.mjs" task-card --market "$MARKET" --into "$into" --force >/dev/null \
    || { sc_fail "--force reinstall must succeed"; return 1; }
}
