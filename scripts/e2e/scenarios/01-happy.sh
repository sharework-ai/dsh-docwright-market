#!/usr/bin/env bash
# Happy chain: submit → machine verify → human approve → publish → the
# catalog, index, and the author's submission view all agree.

scenario happy-chain scenario_happy
scenario_happy() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1

  # Before publish: the pack is absent from the catalog.
  local pre
  pre="$(http_code "$MARKET/api/packs/task-card")"
  assert_eq "$pre" "404" "unpublished pack must 404" || return 1

  submit_archive "$LAST_TOKEN" "$M/samples/task-card.tgz" || return 1
  local sub_id="$LAST_SUB_ID"

  # The author sees the pending state back through the API.
  local view
  view="$(curl -sf -m 10 "$MARKET/api/submissions/$sub_id" -H "Authorization: Bearer $LAST_TOKEN")"
  assert_contains "$view" '"status":"pending"' "author sees pending status" || return 1

  # Approve and publish.
  curl -sf -m 30 "$MARKET/api/admin/review/$sub_id" -H "Authorization: Bearer $LAST_TOKEN" \
    -H 'Content-Type: application/json' -d '{"action":"approve","note":"happy"}' >/dev/null \
    || { sc_fail "approve failed"; return 1; }

  local detail
  detail="$(curl -sf -m 10 "$MARKET/api/packs/task-card")"
  assert_contains "$detail" '"name":"task-card"' "catalog carries the pack" || return 1
  assert_contains "$detail" '"health":"task-card@0.1.0' "detail carries the health sheet" || return 1

  # Index is format-1 with integrity fields.
  curl -sf -m 10 "$MARKET/api/index" | python3 -c '
import json, sys
idx = json.load(sys.stdin)
assert idx["format"] == 1, idx
p = [x for x in idx["packs"] if x["name"] == "task-card"]
assert p, idx
assert p[0]["tarball"] and p[0]["tarballSha256"] and p[0]["contentHash"], p
' || { sc_fail "index incomplete"; return 1; }
}

scenario status-polling scenario_status_polling
scenario_status_polling() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  submit_archive "$LAST_TOKEN" "$M/samples/task-card.tgz" || return 1
  local sub_id="$LAST_SUB_ID"

  # Pending view includes the verification report the author can inspect.
  local view
  view="$(curl -sf -m 10 "$MARKET/api/submissions/$sub_id" -H "Authorization: Bearer $LAST_TOKEN")"
  assert_contains "$view" '"checkGreen":true' "pending view carries the green report" || return 1

  # Approve flips the author-visible status.
  curl -sf -m 30 "$MARKET/api/admin/review/$sub_id" -H "Authorization: Bearer $LAST_TOKEN" \
    -H 'Content-Type: application/json' -d '{"action":"approve"}' >/dev/null
  view="$(curl -sf -m 10 "$MARKET/api/submissions/$sub_id" -H "Authorization: Bearer $LAST_TOKEN")"
  assert_contains "$view" '"status":"approved"' "approved status visible to the author" || return 1
}
