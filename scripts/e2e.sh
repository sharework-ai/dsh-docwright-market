#!/usr/bin/env bash
# End-to-end: upload the seed pack through the real API, approve it, install
# it, and re-verify the installed copy with docwright's own pack:check.
#
# Assumes: supabase stack running (~/projects/commons/supabase), docwright
# checkout ready, rust service built. Safe to re-run (unique emails per run).
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DW="${DOCWRIGHT_ROOT:-$HOME/projects/lab/docwright}"
NS=supabase
PORT=8398
PF_PORT=15433   # distinct from dev-run to allow parallel use
MARKET="http://127.0.0.1:${PORT}"
T="$(mktemp -d "${TMPDIR:-/tmp}"/dw-e2e-XXXXXX)"   # small artifacts only

get_secret() { kubectl -n "$NS" get secret "$1" -o "jsonpath={.data.$2}" | base64 -d; }
SECRET_KEY="$(get_secret supabase-jwt service-key)"
DB_PASSWORD="$(get_secret supabase-postgres-auth password)"

cleanup() {
  for pid in "${PF_PID:-}" "${VERIFIER_PID:-}" "${SERVER_PID:-}"; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  done
}
trap cleanup EXIT

fail() { echo "E2E FAIL: $*" >&2; exit 1; }

# --- 0. start the machinery ---------------------------------------------------
kubectl -n "$NS" port-forward svc/supabase-postgres "${PF_PORT}:5432" >/dev/null 2>&1 & PF_PID=$!
VERIFIER_PORT=8399 DOCWRIGHT_ROOT="$DW" node "$M/verifier/index.mjs" >/dev/null 2>&1 & VERIFIER_PID=$!
PORT=$PORT \
SUPABASE_API_URL="http://supabase.lan" \
SUPABASE_SECRET_KEY="$SECRET_KEY" \
SUPABASE_PUBLISHABLE_KEY="$(get_secret supabase-jwt publishable-key)" \
DATABASE_URL="postgres://postgres:${DB_PASSWORD}@127.0.0.1:${PF_PORT}/postgres" \
VERIFIER_URL="http://127.0.0.1:8399" \
WEB_DIR="$M/web" DATA_DIR="$M/data" \
"$M/server-rs/target/debug/docwright-market" >/dev/null 2>&1 & SERVER_PID=$!

for _ in $(seq 1 60); do
  curl -sf -m 2 "$MARKET/api/index" >/dev/null 2>&1 && break
  sleep 0.5
done
curl -sf -m 2 "$MARKET/api/index" >/dev/null || fail "market service did not come up"
echo "[1/7] services up"

# --- 1. create (or reuse) the e2e user via the GoTrue admin API ---------------
EMAIL="e2e-$(date +%s)@example.invalid"
ADMIN_AUTH=( -H "Authorization: Bearer $SECRET_KEY" -H "apikey: $SECRET_KEY" -H 'Content-Type: application/json' )
USER_JSON="$(curl -sf -m 10 "${ADMIN_AUTH[@]}" -X POST "http://supabase.lan/auth/v1/admin/users" \
  -d "{\"email\":\"$EMAIL\",\"password\":\"e2e-password-1\",\"email_confirm\":true}")" \
  || fail "cannot create GoTrue user"
USER_ID="$(echo "$USER_JSON" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')"
TOKEN_JSON="$(curl -sf -m 10 -H "apikey: $SECRET_KEY" -H 'Content-Type: application/json' \
  -X POST "http://supabase.lan/auth/v1/token?grant_type=password" \
  -d "{\"email\":\"$EMAIL\",\"password\":\"e2e-password-1\"}")" || fail "cannot log the e2e user in"
AUTH=( -H "Authorization: Bearer $(echo "$TOKEN_JSON" | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')" )
echo "[2/7] e2e user ready ($EMAIL)"

# --- 1b. a second, non-admin account for rule scenarios -----------------------
EMAIL2="e2e2-$(date +%s)@example.invalid"
curl -sf -m 10 "${ADMIN_AUTH[@]}" -X POST "http://supabase.lan/auth/v1/admin/users" \
  -d "{\"email\":\"$EMAIL2\",\"password\":\"e2e-password-1\",\"email_confirm\":true}" >/dev/null \
  || fail "cannot create the second GoTrue user"
TOKEN2_JSON="$(curl -sf -m 10 -H "apikey: $SECRET_KEY" -H 'Content-Type: application/json' \
  -X POST "http://supabase.lan/auth/v1/token?grant_type=password" \
  -d "{\"email\":\"$EMAIL2\",\"password\":\"e2e-password-1\"}")" || fail "cannot log user2 in"
AUTH2=( -H "Authorization: Bearer $(echo "$TOKEN2_JSON" | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')" )

# --- 2. make that user the admin (reviewer) -----------------------------------
~/projects/commons/supabase/setup.sh apply-migration /dev/stdin >/dev/null 2>&1 <<SQL || fail "cannot seed admin profile"
insert into profiles (id, is_admin, display_name) values ('$USER_ID', true, 'e2e-admin')
on conflict (id) do update set is_admin = true;
SQL
echo "[3/7] admin profile seeded"

# --- 2b. reset the catalog so the run is idempotent ----------------------------
~/projects/commons/supabase/setup.sh apply-migration /dev/stdin >/dev/null 2>&1 <<SQL || fail "cannot reset catalog"
truncate table submissions, packs;
SQL

# --- 3. submit the seed pack ---------------------------------------------------
SUBMIT_JSON="$(curl -sf -m 120 "$MARKET/api/submissions" "${AUTH[@]}" \
  -F "archive=@$M/samples/task-card.tgz" \
  -F "author=sharework-ai" -F "category=dev" -F "license=MIT" \
  -F "homepage=https://github.com/sharework-ai/dsh-docwright")" || fail "submit call failed"
OK="$(echo "$SUBMIT_JSON" | python3 -c 'import json,sys;print(json.load(sys.stdin).get("ok"))')"
[ "$OK" = "True" ] || fail "verification rejected the seed pack: $(echo "$SUBMIT_JSON" | head -c 400)"
SUB_ID="$(echo "$SUBMIT_JSON" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')"
echo "[4/7] submission verified green ($SUB_ID)"

# --- rule scenarios: the machine-rejected paths around the happy path --------
http_code() { curl -s -m 60 -o /dev/null -w '%{http_code}' "$@"; }

# R1: no token -> 401
[ "$(http_code "$MARKET/api/submissions/$SUB_ID")" = "401" ] || fail "R1 unauthenticated read must 401"
echo "R1  unauthenticated access -> 401"

# R2: bad category -> 400 before anything is stored or verified
[ "$(http_code "$MARKET/api/submissions" "${AUTH[@]}" \
    -F "archive=@$M/samples/task-card.tgz" -F "author=x" -F "category=nope" -F "license=MIT")" = "400" ] \
  || fail "R2 bad category must 400"
echo "R2  invalid category -> 400"

# R3: same-name pending duplicate -> 409
CODE="$(http_code "$MARKET/api/submissions" "${AUTH[@]}" \
  -F "archive=@$M/samples/task-card.tgz" -F "author=sharework-ai" -F "category=dev" -F "license=MIT")"
[ "$CODE" = "409" ] || fail "R3 duplicate pending submission must 409, got $CODE"
echo "R3  pending duplicate -> 409"

# --- 4. approve and publish ----------------------------------------------------
curl -sf -m 30 "$MARKET/api/admin/review/$SUB_ID" "${AUTH[@]}" \
  -H 'Content-Type: application/json' -d '{"action":"approve","note":"e2e"}' >/dev/null \
  || fail "approve failed"
curl -sf -m 10 "$MARKET/api/packs/task-card" | grep -q '"name":"task-card"' || fail "pack not in catalog"
curl -sf -m 10 "$MARKET/api/index" | python3 -c '
import json,sys
idx=json.load(sys.stdin)
assert idx["format"]==1
p=[x for x in idx["packs"] if x["name"]=="task-card"]
assert p and p[0]["tarball"] and p[0]["tarballSha256"], p
' || fail "index incomplete"
echo "[5/7] published: catalog + index carry task-card"

# R4: non-admin cannot see the review queue -> 403
[ "$(http_code "$MARKET/api/admin/queue" "${AUTH2[@]}")" = "403" ] || fail "R4 non-admin queue read must 403"
echo "R4  non-admin review access -> 403"

# R5: a different account cannot update someone else's pack -> 403
#     (a real verifier-green archive at a bumped version, so the rejection
#     lands on the ownership rule, not on shape checking)
V2_DIR="$T/v2pack"; mkdir -p "$V2_DIR"
tar xzf "$M/samples/task-card.tgz" -C "$V2_DIR"
python3 - "$V2_DIR/task-card/manifest.json" <<'PY'
import json, sys
p = sys.argv[1]
m = json.load(open(p)); m["version"] = "0.2.0"; json.dump(m, open(p, "w"))
PY
tar czf "$T/task-card-020.tgz" -C "$V2_DIR" task-card
CODE="$(http_code "$MARKET/api/submissions" "${AUTH2[@]}" \
  -F "archive=@$T/task-card-020.tgz" -F "author=intruder" -F "category=dev" -F "license=MIT")"
[ "$CODE" = "403" ] || fail "R5 cross-account update must 403, got $CODE"
echo "R5  cross-account update -> 403"

# R6: an already-used (name, version) pair is rejected forever -> 409
CODE="$(http_code "$MARKET/api/submissions" "${AUTH[@]}" \
  -F "archive=@$M/samples/task-card.tgz" -F "author=sharework-ai" -F "category=dev" -F "license=MIT")"
[ "$CODE" = "409" ] || fail "R6 reused version must 409, got $CODE"
echo "R6  reused version -> 409"

# --- 5. install into a fresh packsDir ------------------------------------------
INSTALL_DIR="$T/into"
node "$M/install.mjs" task-card --market "$MARKET" --into "$INSTALL_DIR" >/dev/null || fail "install.mjs failed"
[ -f "$INSTALL_DIR/task-card/manifest.json" ] || fail "pack not landed in packsDir"
echo "[6/7] installed into $INSTALL_DIR/task-card"

# --- 6. the installed copy must pass docwright's own gate ----------------------
( cd "$DW" && pnpm run --silent pack:check -- "$INSTALL_DIR" --with packs ) | grep -q "PASS" \
  || fail "pack:check on the installed copy did not pass"
echo "[7/7] docwright pack:check GREEN on the installed copy"

echo
echo "E2E PASS: submit -> verify -> review -> publish -> install -> re-check"
