#!/usr/bin/env bash
# Owner lifecycle: org gate + cascade, version history, metadata patch,
# disable/enable (410 semantics), publish-while-disabled, withdraw, delete,
# and the forever-reserved (name, version).

ORG_D="e2e-org-d-$(date +%s)$RANDOM"
ORG_D2="e2e-org-d2-$(date +%s)$RANDOM"
FX_V1="$T/own-v1.tgz"; sample_archive_as "own-pack" "0.1.0" "$FX_V1"
FX_V2="$T/own-v2.tgz"; sample_archive_as "own-pack" "0.2.0" "$FX_V2"
FX_V3="$T/own-v3.tgz"; sample_archive_as "own-pack" "0.3.0" "$FX_V3"
FX_V4="$T/own-v4.tgz"; sample_archive_as "own-pack" "0.4.0" "$FX_V4"

scenario owner-lifecycle scenario_owner_lifecycle
scenario_owner_lifecycle() {
  init_run
  create_account author || return 1
  local token="$LAST_TOKEN"
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "$ORG_D" || return 1

  local mine
  mine="$(curl -sf -m 10 "$MARKET/api/my/packs" -H "Authorization: Bearer $token")"
  assert_contains "$mine" '"packs":[]' "fresh account owns nothing" || return 1

  # Publish two versions; the history grows and old tarballs stay served.
  publish_fixture "$token" "$FX_V1" || return 1
  publish_fixture "$token" "$FX_V2" || return 1
  curl -sf -m 10 "$MARKET/api/packs/own-pack" | python3 -c '
import json, sys
d = json.load(sys.stdin)
assert d["pack"]["version"] == "0.2.0", d
assert [v["version"] for v in d["versions"]] == ["0.2.0", "0.1.0"], d
assert d["pack"]["org"], d
' || { sc_fail "version history wrong"; return 1; }
  local old_tarball
  old_tarball="$(http_code "$MARKET/tarballs/own-pack-0.1.0.tgz")"
  assert_eq "$old_tarball" "200" "old version tarball stays downloadable" || return 1
  echo "  v1+v2 published; old tarball 200"

  # Changing the org relabels the pack in the same call.
  local changed
  changed="$(curl -sf -m 10 "$MARKET/api/account/org" -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -X PUT -d "{\"org\":\"$ORG_D2\"}")"
  assert_contains "$changed" '"packsAffected":1' "org change reports the cascade" || return 1
  assert_contains "$(curl -sf -m 10 "$MARKET/api/packs/own-pack")" "\"org\":\"$ORG_D2\"" \
    "pack relabelled to the new org" || return 1
  echo "  org change cascaded to the pack"

  # Metadata patch (owner-only surface).
  local patched
  patched="$(curl -sf -m 10 "$MARKET/api/my/packs/own-pack" -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -X PATCH \
    -d '{"category":"docs","license":"ISC","homepage":"https://example.invalid/x"}')"
  assert_contains "$patched" '"category":"docs"' "patch updates category" || return 1
  local code
  code="$(http_code "$MARKET/api/my/packs/own-pack" -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -X PATCH -d '{"category":"nope"}')"
  assert_eq "$code" "400" "patch rejects unknown category" || return 1
  echo "  metadata patched (whitelist enforced)"

  # Disable: hidden from the public surface, downloads 410, owner still sees it.
  code="$(http_code "$MARKET/api/my/packs/own-pack/disable" -H "Authorization: Bearer $token" -X POST)"
  assert_eq "$code" "200" "disable succeeds" || return 1
  code="$(http_code "$MARKET/tarballs/own-pack-0.2.0.tgz")"
  assert_eq "$code" "410" "disabled pack downloads 410" || return 1
  code="$(http_code "$MARKET/api/packs/own-pack")"
  assert_eq "$code" "404" "disabled pack hides from detail" || return 1
  assert_contains "$(curl -sf -m 10 "$MARKET/api/my/packs" -H "Authorization: Bearer $token")" \
    '"status":"disabled"' "owner still sees the disabled pack" || return 1

  # Publishing v3 while disabled must not silently re-enable.
  publish_fixture "$token" "$FX_V3" || return 1
  assert_contains "$(curl -sf -m 10 "$MARKET/api/my/packs" -H "Authorization: Bearer $token")" \
    '"status":"disabled"' "re-publish keeps the pack disabled" || return 1
  local enabled
  enabled="$(http_code "$MARKET/api/my/packs/own-pack/enable" -H "Authorization: Bearer $token" -X POST)"
  assert_eq "$enabled" "200" "enable succeeds" || return 1
  assert_eq "$(http_code "$MARKET/tarballs/own-pack-0.3.0.tgz")" "200" "enabled pack serves the new version" || return 1
  echo "  disable/enable lifecycle holds (publish-while-disabled keeps disabled)"

  # Withdraw a pending submission.
  submit_archive "$token" "$FX_V4" || return 1
  local pending="$LAST_SUB_ID"
  code="$(http_code "$MARKET/api/my/submissions/$pending" -H "Authorization: Bearer $token" -X DELETE)"
  assert_eq "$code" "200" "withdraw pending succeeds" || return 1
  assert_contains "$(curl -sf -m 10 "$MARKET/api/my/submissions" -H "Authorization: Bearer $token")" \
    '"packName":"own-pack"' "history lists own submissions" || return 1
  code="$(http_code "$MARKET/api/my/submissions/$pending" -H "Authorization: Bearer $token" -X DELETE)"
  assert_eq "$code" "404" "withdrawing twice is a 404" || return 1

  # Delete the pack: catalog + history gone, submissions history stays, so
  # the old version numbers are reserved forever.
  code="$(http_code "$MARKET/api/my/packs/own-pack" -H "Authorization: Bearer $token" -X DELETE)"
  assert_eq "$code" "200" "delete succeeds" || return 1
  code="$(http_code "$MARKET/api/packs/own-pack")"
  assert_eq "$code" "404" "deleted pack is gone" || return 1
  code="$(http_code "$MARKET/tarballs/own-pack-0.3.0.tgz")"
  assert_eq "$code" "404" "deleted pack's tarballs are gone" || return 1
  code="$(http_code "$MARKET/api/submissions" -H "Authorization: Bearer $token" \
    -F "archive=@$FX_V1" -F "author=author" -F "category=dev" -F "license=MIT")"
  assert_eq "$code" "409" "a deleted pack's version numbers stay reserved" || return 1
  echo "  delete frees the name but never the (name, version) pairs"
}
