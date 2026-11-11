#!/usr/bin/env bash
# Suite scenarios: submission-queue lifecycle (create → approve → live),
# form validation, disable/enable visibility, edit/delete through the queue,
# and the docwright client lanes (search projection, membership guard,
# install/update/delete) against the REAL server + catalog.

SUITE_A="e2e-suite-a-$(date +%s)$RANDOM"
SUITE_B="e2e-suite-b-$(date +%s)$RANDOM"
SUITE_NAME="e2e-suite-$(date +%s)$RANDOM"
FIX_A="$T/suite-a.tgz"
FIX_A_V2="$T/suite-a-v2.tgz"
FIX_B="$T/suite-b.tgz"

## Publishes the two member packs for the suite scenarios (idempotent per
## scenario run; each scenario starts from a reset catalog).
publish_suite_members() {
  SUITE_A="e2e-suite-a-$(date +%s)$RANDOM"
  SUITE_B="e2e-suite-b-$(date +%s)$RANDOM"
  SUITE_NAME="e2e-suite-$(date +%s)$RANDOM"
  FIX_A="$T/suite-a.tgz"
  FIX_A_V2="$T/suite-a-v2.tgz"
  FIX_B="$T/suite-b.tgz"
  sample_archive_as "$SUITE_A" "0.1.0" "$FIX_A"
  sample_archive_as "$SUITE_A" "0.2.0" "$FIX_A_V2"
  sample_archive_as "$SUITE_B" "0.1.0" "$FIX_B"
  publish_fixture "$LAST_TOKEN" "$FIX_A" || return 1
  publish_fixture "$LAST_TOKEN" "$FIX_B" || return 1
}

## Creates the suite (pending) with the current author token.
submit_suite() {
  local raw code json
  raw="$(curl -s -m 10 -w '\n%{http_code}' "$MARKET/api/my/suites" "${AUTH_LAST[@]}" \
    -H 'Content-Type: application/json' \
    -d "{\"name\":\"$SUITE_NAME\",\"version\":\"1.0.0\",\"description\":{\"en\":\"e2e suite\",\"zh\":\"e2e 套件\"},\"category\":\"dev\",\"members\":[\"$SUITE_A\",\"$SUITE_B\"]}")" \
    || { sc_fail "suite create transport failure"; return 1; }
  code="$(echo "$raw" | tail -1)"
  json="$(echo "$raw" | sed '$d')"
  [ "$code" = "200" ] || { sc_fail "suite create -> $code: $(echo "$json" | head -c 300)"; return 1; }
}

## Approves the OLDEST pending submission of the suite (queue ids are UUIDs;
## the queue endpoint returns them newest-first, so take the last match).
approve_suite() {
  local queue id
  queue="$(curl -sf -m 10 "$MARKET/api/admin/queue" -H "Authorization: Bearer $LAST_TOKEN")"
  id="$(echo "$queue" | python3 -c '
import json, sys
q = json.load(sys.stdin)
rows = [r for r in q.get("queue", []) if r.get("kind") == "suite" and r.get("packName") == "'"$SUITE_NAME"'"]
assert rows, "no pending suite submission"
print(rows[-1]["id"])
')"
  [ -n "$id" ] || { sc_fail "suite submission not in queue"; return 1; }
  curl -sf -m 10 "$MARKET/api/admin/review/$id" -H "Authorization: Bearer $LAST_TOKEN" \
    -H 'Content-Type: application/json' -d '{"action":"approve","note":"e2e suite"}' >/dev/null \
    || { sc_fail "suite approve failed"; return 1; }
}

## Runs the docwright client lanes against the live local market: a node
## runner imports the docwright workspace sources directly (no build step),
## drives every suite lane against the REAL server, and reports through the
## exit code.
docwright_client_lanes() {
  DW="$DW" MARKET="$MARKET" SUITE_A="$SUITE_A" SUITE_B="$SUITE_B" SUITE_NAME="$SUITE_NAME" \
  node --experimental-strip-types - "$DW" "$MARKET" "$SUITE_A" "$SUITE_NAME" <<'JS' || { sc_fail "docwright client lanes failed"; return 1; }
const { mkdtemp, rm } = await import('node:fs/promises')
const { tmpdir } = await import('node:os')
const { join } = await import('node:path')
const [, , dw, market, memberA, suiteName] = process.argv
const { checkSuiteStatus, deletePack, installSuite, suiteMembershipMap, updateSuite, deleteSuite } = await import(`${dw}/src/pack-manage.ts`)
const { loadPacks } = await import(`${dw}/src/pack-loader.ts`)
const packsDir = await mkdtemp(join(tmpdir(), 'dw-e2e-client-'))
const builtinRoot = join(dw, 'packs')
const load = () => loadPacks(builtinRoot, { extra: [packsDir] })
const packs = await load()
const deps = { packs, builtinRoot, packsDir, marketUrl: market, loadPacks: load }
const fail = (msg) => { console.error(msg); process.exitCode = 1 }

// 1. suite status: the real server's list projects onto the empty packsDir.
const statuses = await checkSuiteStatus(deps)
const mine = statuses.find(s => s.name === suiteName)
if (!mine) fail(`checkSuiteStatus misses ${suiteName}`)
else if (mine.members.length !== 2 || !mine.incomplete) fail(`status wrong: ${JSON.stringify(mine)}`)

// 2. membership guard: both members are locked for single-pack installs.
const membership = await suiteMembershipMap(market, fetch)
if (!(membership.get(memberA)?.includes(suiteName))) fail(`membership map misses ${memberA}`)

// 3. full suite install lands both members (plus their consumed upstreams —
// the sample fixture carries consumes edges) and hot-reloads.
const { installed } = await installSuite(deps, suiteName)
const names = installed.map(o => o.name)
if (!names.includes(memberA) || !names.includes(process.env.SUITE_B)) fail(`install missed a member: ${names}`)
if (!packs.has(memberA) || !packs.has(process.env.SUITE_B)) fail('pack map missing members after installSuite')

// 4. suite update: nothing newer yet → all unchanged.
const upd = await updateSuite(deps, suiteName)
if (upd.updated.length !== 0 || upd.unchanged.length !== 2) fail(`updateSuite unexpected: ${JSON.stringify(upd)}`)

// 5. single-pack delete of a member is refused (suiteManaged)…
let refused = false
try { await deletePack({ ...deps }, memberA) } catch (e) {
  const text = String(e?.message ?? e)
  if (text.includes('member of suite')) refused = true
  else console.error('unexpected delete error:', text)
}
if (!refused) fail('single-pack delete of a member was NOT refused')

// 6. …while the suite lane deletes its own members.
const del = await deleteSuite(deps, suiteName)
if (JSON.stringify([...del.deleted].sort()) !== JSON.stringify([memberA, process.env.SUITE_B].sort())) fail(`deleteSuite deleted ${del.deleted}`)
if (packs.has(memberA)) fail('pack map still has the member after deleteSuite')

await rm(packsDir, { recursive: true, force: true })
JS
}

scenario suite-lifecycle scenario_suite_lifecycle
scenario_suite_lifecycle() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-suite-org-$(date +%s)$RANDOM" || return 1
  publish_suite_members || return 1

  # Before approval the suite is absent from every read surface.
  local pre
  pre="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$MARKET/api/suites/$SUITE_NAME")"
  assert_eq "$pre" "404" "unapproved suite must 404" || return 1

  submit_suite || return 1
  approve_suite || return 1

  # List + detail agree; members expand with missing flags.
  curl -sf -m 10 "$MARKET/api/suites/$SUITE_NAME" | python3 -c "
import json, sys
d = json.load(sys.stdin)
assert d['suite']['name'] == '$SUITE_NAME', d
names = [m['name'] for m in d['members']]
assert names == ['$SUITE_A', '$SUITE_B'], names
assert all(m.get('missing') is False for m in d['members']), d['members']
" || { sc_fail "detail members wrong"; return 1; }

  # Pack cards and the index carry the suite membership; index stays format 2.
  local pack_detail
  pack_detail="$(curl -sf -m 10 "$MARKET/api/packs/$SUITE_A")"
  assert_contains "$pack_detail" "\"suites\":[\"$SUITE_NAME\"]" "pack detail carries suites" || return 1

  # Members are HIDDEN from the market listing and search, but the index (the
  # install lane's machine catalog) keeps them, and detail stays reachable.
  curl -sf -m 10 "$MARKET/api/packs" | python3 -c "
import json, sys
names = [p['name'] for p in json.load(sys.stdin)['packs']]
assert '$SUITE_A' not in names and '$SUITE_B' not in names, names
" || { sc_fail "member still listed in the pack listing"; return 1; }
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$MARKET/api/packs?q=$SUITE_A")"
  body="$(curl -sf -m 10 "$MARKET/api/packs?q=$SUITE_A")"
  assert_contains "$body" '"packs":[]' "member-name search returns no packs" || return 1
  curl -sf -m 10 "$MARKET/api/index" | python3 -c "
import json, sys
idx = json.load(sys.stdin)
names = [p['name'] for p in idx['packs']]
assert '$SUITE_A' in names and '$SUITE_B' in names, names
" || { sc_fail "index lost the members"; return 1; }
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$MARKET/api/packs/$SUITE_A")"
  assert_eq "$code" "200" "member detail stays reachable" || return 1

  curl -sf -m 10 "$MARKET/api/index" | python3 -c "
import json, sys
idx = json.load(sys.stdin)
assert idx['format'] == 2, idx['format']
suites = idx.get('suites') or []
row = [s for s in suites if s['name'] == '$SUITE_NAME']
assert row, suites
assert '$SUITE_A' in row[0]['members'], row
" || { sc_fail "index suites wrong"; return 1; }
}

scenario suite-validation scenario_suite_validation
scenario_suite_validation() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-suite-org-b-$(date +%s)$RANDOM" || return 1
  publish_suite_members || return 1

  # Unknown member → loud rejection, nothing queued.
  local code
  code="$(http_code "$MARKET/api/my/suites" "${AUTH_LAST[@]}" -H 'Content-Type: application/json' -X POST \
    -d "{\"name\":\"$SUITE_NAME-bad\",\"version\":\"1.0.0\",\"description\":{\"en\":\"x\"},\"category\":\"dev\",\"members\":[\"$SUITE_A\",\"ghost-pack\"]}")"
  assert_eq "$code" "400" "unknown member must be rejected" || return 1

  # Bad category → rejected.
  code="$(http_code "$MARKET/api/my/suites" "${AUTH_LAST[@]}" -H 'Content-Type: application/json' -X POST \
    -d "{\"name\":\"$SUITE_NAME-bad\",\"version\":\"1.0.0\",\"description\":{\"en\":\"x\"},\"category\":\"not-a-cat\",\"members\":[\"$SUITE_A\"]}")"
  assert_eq "$code" "400" "bad category must be rejected" || return 1

  # Valid create, then a duplicate name → rejected.
  submit_suite || return 1
  code="$(http_code "$MARKET/api/my/suites" "${AUTH_LAST[@]}" -H 'Content-Type: application/json' -X POST \
    -d "{\"name\":\"$SUITE_NAME\",\"version\":\"1.0.0\",\"description\":{\"en\":\"dup\"},\"category\":\"dev\",\"members\":[\"$SUITE_A\"]}")"
  assert_eq "$code" "409" "duplicate suite name must be rejected" || return 1
}

scenario suite-disable-enable scenario_suite_disable
scenario_suite_disable() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-suite-org-c-$(date +%s)$RANDOM" || return 1
  publish_suite_members || return 1
  submit_suite || return 1
  approve_suite || return 1

  curl -sf -m 10 "$MARKET/api/my/suites/$SUITE_NAME/disable" "${AUTH_LAST[@]}" -X POST >/dev/null \
    || { sc_fail "disable failed"; return 1; }
  local code
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$MARKET/api/suites/$SUITE_NAME")"
  assert_eq "$code" "404" "disabled suite must vanish from detail" || return 1
  curl -sf -m 10 "$MARKET/api/suites" | python3 -c "
import json, sys
names = [s['name'] for s in json.load(sys.stdin)['suites']]
assert '$SUITE_NAME' not in names, names
" || { sc_fail "disabled suite still listed"; return 1; }
  curl -sf -m 10 "$MARKET/api/index" | python3 -c "
import json, sys
names = [s['name'] for s in (json.load(sys.stdin).get('suites') or [])]
assert '$SUITE_NAME' not in names, names
" || { sc_fail "disabled suite still in index"; return 1; }

  curl -sf -m 10 "$MARKET/api/my/suites/$SUITE_NAME/enable" "${AUTH_LAST[@]}" -X POST >/dev/null \
    || { sc_fail "enable failed"; return 1; }
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$MARKET/api/suites/$SUITE_NAME")"
  assert_eq "$code" "200" "enabled suite is back" || return 1
}

scenario suite-edit-delete-queue scenario_suite_edit_delete
scenario_suite_edit_delete() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-suite-org-d-$(date +%s)$RANDOM" || return 1
  publish_suite_members || return 1
  submit_suite || return 1
  approve_suite || return 1

  # A member-version bump (0.1.0 → 0.2.0) does not touch the suite…
  publish_fixture "$LAST_TOKEN" "$FIX_A_V2" || return 1
  local detail
  detail="$(curl -sf -m 10 "$MARKET/api/suites/$SUITE_NAME")"
  assert_contains "$detail" '"version":"1.0.0"' "suite version unchanged by member bump" || return 1

  # …an EDIT goes through the queue: bump the suite, shrink members, approve.
  local raw code
  raw="$(curl -s -m 10 -w '\n%{http_code}' "$MARKET/api/my/suites/$SUITE_NAME" "${AUTH_LAST[@]}" \
    -H 'Content-Type: application/json' -X PATCH \
    -d '{"version":"1.1.0","description":{"en":"e2e suite v2"},"category":"dev","members":["'$SUITE_B'"]}')"
  code="$(echo "$raw" | tail -1)"
  assert_eq "$code" "200" "suite edit accepted" || return 1
  assert_contains "$detail" '"members"' "live suite still serves the old shape meanwhile" || return 1
  approve_suite || return 1
  detail="$(curl -sf -m 10 "$MARKET/api/suites/$SUITE_NAME")"
  assert_contains "$detail" '"version":"1.1.0"' "approved edit applies the version" || return 1
  curl -sf -m 10 "$MARKET/api/suites/$SUITE_NAME" | python3 -c "
import json, sys
names = [m['name'] for m in json.load(sys.stdin)['members']]
assert names == ['$SUITE_B'], names
" || { sc_fail "approved edit applies the members"; return 1; }

  # DELETE goes through the queue too: refused until approved.
  code="$(http_code "$MARKET/api/my/suites/$SUITE_NAME" "${AUTH_LAST[@]}" -X DELETE)"
  assert_eq "$code" "200" "delete accepted as a queued proposal" || return 1
  assert_contains "$(curl -sf -m 10 "$MARKET/api/suites/$SUITE_NAME")" "\"name\":\"$SUITE_NAME\"" "suite stays live until the delete is approved" || return 1
  approve_suite || return 1
  code="$(curl -s -m 10 -o /dev/null -w '%{http_code}' "$MARKET/api/suites/$SUITE_NAME")"
  assert_eq "$code" "404" "approved delete removes the suite" || return 1
}

scenario suite-docwright-client scenario_suite_docwright_client
scenario_suite_docwright_client() {
  init_run
  create_account author || return 1
  seed_admin_profile "$LAST_USER_ID" || return 1
  set_org "e2e-suite-org-e-$(date +%s)$RANDOM" || return 1
  publish_suite_members || return 1
  submit_suite || return 1
  approve_suite || return 1
  docwright_client_lanes
}
