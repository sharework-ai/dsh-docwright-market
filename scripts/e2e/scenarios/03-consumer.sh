#!/usr/bin/env bash
# Consumer side: install from the live market into a fresh packsDir (latest
# and pinned versions), then re-verify the installed copy with docwright's
# own pack:check, plus --list and the --force overwrite path.

ORG_C="e2e-org-c-$(date +%s)$RANDOM"
FIX_V1="$T/fixture-v1.tgz"
FIX_V2="$T/fixture-v2.tgz"
sample_archive_as "e2e-card" "0.1.0" "$FIX_V1"
sample_archive_as "e2e-card" "0.2.0" "$FIX_V2"

scenario consumer-install scenario_consumer_install
scenario_consumer_install() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "$ORG_C" || return 1
  publish_fixture "$LAST_TOKEN" "$FIX_V1" || return 1
  publish_fixture "$LAST_TOKEN" "$FIX_V2" || return 1

  # --list sees the market (against the live index), latest first, with the
  # version history counted.
  local listing
  listing="$(node "$M/install.mjs" --list --market "$MARKET")"
  assert_contains "$listing" "e2e-card@0.2.0" "--list shows the published pack" || return 1
  assert_contains "$listing" "(2 versions)" "--list counts the version history" || return 1

  # Install latest into a fresh packsDir.
  local into="$T/consumer-packs"
  node "$M/install.mjs" e2e-card --market "$MARKET" --into "$into" >/dev/null \
    || { sc_fail "install.mjs failed"; return 1; }
  [ "$(python3 -c "import json;print(json.load(open('$into/e2e-card/manifest.json'))['version'])")" = "0.2.0" ] \
    || { sc_fail "latest install landed the wrong version"; return 1; }

  # Pinned install of the OLD version into a second packsDir.
  local pinned="$T/consumer-pinned"
  node "$M/install.mjs" e2e-card@0.1.0 --market "$MARKET" --into "$pinned" >/dev/null \
    || { sc_fail "pinned install failed"; return 1; }
  [ "$(python3 -c "import json;print(json.load(open('$pinned/e2e-card/manifest.json'))['version'])")" = "0.1.0" ] \
    || { sc_fail "pinned install landed the wrong version"; return 1; }
  node "$M/install.mjs" e2e-card@9.9.9 --market "$MARKET" --into "$T/consumer-miss" >/dev/null 2>&1 \
    && { sc_fail "pinning an unknown version must fail"; return 1; }
  echo "  latest + pinned installs agree with the version history"

  # The installed copy must pass docwright's own gate (the market publishes
  # exactly what it verified). The pack:check script is invoked through its
  # underlying command, not `pnpm run`: the host pnpm may disagree with the
  # repo's packageManager pin (corepack gate), and tsx is already installed.
  ( cd "$DW" && node --import tsx/esm scripts/pack-check.ts "$into" --with packs ) \
    | grep -q "PASS" || { sc_fail "pack:check on installed copy failed"; return 1; }

  # Re-install without --force conflicts (exit 3); with --force succeeds.
  node "$M/install.mjs" e2e-card --market "$MARKET" --into "$into" >/dev/null 2>&1 \
    && { sc_fail "duplicate install must fail without --force"; return 1; }
  node "$M/install.mjs" e2e-card --market "$MARKET" --into "$into" --force >/dev/null \
    || { sc_fail "--force reinstall must succeed"; return 1; }
}
