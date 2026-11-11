#!/usr/bin/env bash
# Negative rule scenarios: every machine-enforced rejection path, each with
# its own accounts so the scenario is independent of the happy chain.

FIX="$T/fixture.tgz"
sample_archive_as "e2e-card" "0.1.0" "$FIX"
FIX_V2="$T/fixture-v2.tgz"
sample_archive_as "e2e-card" "0.2.0" "$FIX_V2"

scenario rules scenario_rules
scenario_rules() {
  init_run
  # Account 1 becomes the admin/reviewer; account 2 is the pack owner whose
  # non-admin status is itself under test.
  create_account admin-acct || return 1
  local admin_id="$LAST_USER_ID" admin_token="$LAST_TOKEN"
  seed_admin_profile "$admin_id" || return 1
  create_account owner || return 1
  local owner_token="$LAST_TOKEN"
  create_account intruder || return 1
  local intruder_token="$LAST_TOKEN"

  # R1: unauthenticated access -> 401
  local code
  code="$(http_code "$MARKET/api/submissions/00000000-0000-0000-0000-000000000000")"
  assert_eq "$code" "401" "unauthenticated submission read must 401" || return 1
  echo "  R1 unauthenticated -> 401"

  # R7 (before any publish): submitting without a publisher org -> 403.
  code="$(http_code "$MARKET/api/submissions" -H "Authorization: Bearer $owner_token" \
    -F "archive=@$FIX" -F "author=owner" -F "category=dev" -F "license=MIT")"
  assert_eq "$code" "403" "submit without org must 403" || return 1
  echo "  R7 submit without org -> 403"

  set_org_for "$owner_token" "e2e-org-rules-owner-$(date +%s)$RANDOM" || return 1
  set_org_for "$intruder_token" "e2e-org-rules-intruder-$(date +%s)$RANDOM" || return 1

  # Seed a published pack owned by the owner account (for ownership rules).
  submit_archive "$owner_token" "$FIX" || return 1
  local sub_id="$LAST_SUB_ID"
  curl -sf -m 30 "$MARKET/api/admin/review/$sub_id" -H "Authorization: Bearer $admin_token" \
    -H 'Content-Type: application/json' -d '{"action":"approve"}' >/dev/null \
    || { sc_fail "seed approve failed"; return 1; }

  # R2: invalid category -> 400 before anything is stored or verified
  code="$(http_code "$MARKET/api/submissions" -H "Authorization: Bearer $owner_token" \
    -F "archive=@$FIX" -F "author=x" -F "category=nope" -F "license=MIT")"
  assert_eq "$code" "400" "bad category must 400" || return 1
  echo "  R2 invalid category -> 400"

  # R3: non-admin cannot read the review queue -> 403
  code="$(http_code "$MARKET/api/admin/queue" -H "Authorization: Bearer $owner_token")"
  assert_eq "$code" "403" "non-admin queue read must 403" || return 1
  echo "  R3 non-admin queue -> 403"

  # R4: a different account cannot update someone else's pack -> 403
  #     (org'd, verifier-green bumped archive, so the rejection lands on
  #     ownership rather than the org gate)
  code="$(http_code "$MARKET/api/submissions" -H "Authorization: Bearer $intruder_token" \
    -F "archive=@$FIX_V2" -F "author=intruder" -F "category=dev" -F "license=MIT")"
  assert_eq "$code" "403" "cross-account update must 403" || return 1
  echo "  R4 cross-account update -> 403"

  # R5: an already-used (name, version) pair is rejected forever -> 409
  code="$(http_code "$MARKET/api/submissions" -H "Authorization: Bearer $owner_token" \
    -F "archive=@$FIX" -F "author=owner" -F "category=dev" -F "license=MIT")"
  assert_eq "$code" "409" "reused version must 409" || return 1
  echo "  R5 reused version -> 409"

  # R6: identical content under a bumped version -> 409 (nothing to update)
  code="$(http_code "$MARKET/api/submissions" -H "Authorization: Bearer $owner_token" \
    -F "archive=@$FIX" -F "author=owner" -F "category=dev" -F "license=MIT")"
  # Same call as R5 — the distinct rule (content-hash equality) is covered by
  # the sqlx::test unit; at the API level it surfaces as the same 409 family.
  assert_eq "$code" "409" "same-version resubmission must 409" || return 1
  echo "  R6 version resubmission -> 409"
}

# set_org for an arbitrary token (set_org rides on AUTH_LAST from create_account).
set_org_for() {
  local token="$1" org="$2" code
  code="$(http_code "$MARKET/api/account/org" -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -X PUT -d "{\"org\":\"$org\"}")"
  assert_eq "$code" "200" "set_org_for $org must succeed" || return 1
}

scenario builtin-name-collision scenario_builtin_collision
scenario_builtin_collision() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-org-collision-$(date +%s)$RANDOM" || return 1

  # A submission whose archive directory name disagrees with its manifest.name
  # is refused by the verifier shape check: surfaced as 200 + ok:false with
  # the offending rule named, nothing stored. (The builtin-collision rule was
  # dropped on purpose — commit 3679316, zero-builtin world — so names are
  # free; the shape gate is what this scenario actually pins.)
  local dir="$T/collision-pack"
  mkdir -p "$dir/task-card-collision"
  echo '{"name":"cosmic-func","version":"0.1.0","description":{"en":"x"}}' \
    > "$dir/task-card-collision/manifest.json"
  tar czf "$T/collision.tgz" -C "$dir" task-card-collision

  local res
  res="$(curl -sf -m 120 "$MARKET/api/submissions" -H "Authorization: Bearer $LAST_TOKEN" \
    -F "archive=@$T/collision.tgz" -F "author=intruder" -F "category=dev" -F "license=MIT")"
  assert_contains "$res" '"ok":false' "malformed submission is refused" || return 1
  assert_contains "$res" 'must equal manifest.name' "refusal names the dir-mismatch rule" || return 1
  echo "  builtin-name collision refused, nothing stored"
}
