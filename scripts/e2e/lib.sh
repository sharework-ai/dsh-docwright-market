#!/usr/bin/env bash
# e2e shared layer: credentials, service lifecycle, scenario framework, API
# helpers. Sourced by run.sh (and by scenario files through run.sh) — never
# executed directly.
#
# Scenario contract: a scenario file calls `scenario <name> <fn-name>` to
# register; run.sh executes each registered function in its own subshell with
# `set -e`, after a catalog reset, and collects PASS/FAIL — one failing
# scenario never masks the others.

E2E_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
M="$(cd "$E2E_ROOT/../.." && pwd)"
DW="${DOCWRIGHT_ROOT:-$HOME/projects/lab/docwright}"
NS=supabase
# Cluster mode (default): the deployed market on the local k3s, reached via
# the hosts entry `192.168.2.1 dm.shrwk.com` through traefik. `--local` falls
# back to spawning host processes for development before a cluster deploy.
MARKET="${E2E_BASE:-https://dm.shrwk.com}"
LOCAL_MODE=false
MARKET_PORT="${E2E_MARKET_PORT:-18438}"
PF_PORT="${E2E_PF_PORT:-18433}"
VERIFIER_PORT="${E2E_VERIFIER_PORT:-18439}"
SERVICES_LOG="${E2E_SERVICES_LOG:-/tmp/dw-e2e-services.log}"
T="$(mktemp -d "${TMPDIR:-/tmp}"/dw-e2e-XXXXXX)"   # per-run artifacts (small)

SCENARIOS=()      # ordered list of scenario names
SCENARIO_FN=()    # parallel array: function names
CURRENT_SCENARIO=""
FAILED=()
CREATED_USERS_FILE="$T/created-users.txt"   # shared across scenario subshells
touch "$CREATED_USERS_FILE"

get_secret() { kubectl -n "$NS" get secret "$1" -o "jsonpath={.data.$2}" | base64 -d; }

# Direct SQL against the supabase Postgres (tuples-only output), so helpers
# can both mutate and SELECT without parsing apply-migration's preamble.
db_sql() {
  local dbpod
  dbpod="$(kubectl -n "$NS" get pods --no-headers -o custom-columns=':metadata.name' \
    | grep -E 'single.*db|postgres' | head -1)"
  kubectl -n "$NS" exec -i "$dbpod" -- psql -U postgres -d postgres -Atq <<SQL
$1
SQL
}

# --- service lifecycle ---------------------------------------------------------
# Cluster mode (default): verify the deployed market answers; nothing to start.
# --local: spawn host processes (pg-forward + verifier + debug binary).

ensure_services() {
  command -v kubectl >/dev/null || { echo "kubectl required" >&2; return 1; }
  kubectl -n "$NS" get svc supabase-postgres >/dev/null 2>&1 \
    || { echo "supabase stack not reachable via kubectl (start ~/projects/commons/supabase first)" >&2; return 1; }

  if ! $LOCAL_MODE; then
    for _ in $(seq 1 30); do
      curl -sf -m 3 "$MARKET/api/index" >/dev/null 2>&1 && break
      sleep 1
    done
    curl -sf -m 3 "$MARKET/api/index" >/dev/null \
      || { echo "cluster market unreachable at $MARKET — deploy first: deploy/deploy.sh (or use --local)" >&2; return 1; }
    echo "mode: cluster ($MARKET)"
    return 0
  fi

  echo "mode: local host processes"
  MARKET="http://127.0.0.1:${MARKET_PORT}"
  [ -x "$M/server-rs/target/debug/docwright-market" ] \
    || { echo "server binary missing — build it: podman run --rm --userns=keep-id -v \"$M\":/workspace -w /workspace/server-rs -v ~/devcache/cargo/registry:/home/app/.cargo/registry localhost/rust-dev:1.3 cargo build" >&2; return 1; }
  [ -d "$DW/packs" ] || { echo "DOCWRIGHT_ROOT ($DW) missing packs/" >&2; return 1; }

  local db_password secret_key
  db_password="$(get_secret supabase-postgres-auth password)"
  secret_key="$(get_secret supabase-jwt service-key)"

  kubectl -n "$NS" port-forward svc/supabase-postgres "${PF_PORT}:5432" >>"$SERVICES_LOG" 2>&1 & E2E_PF_PID=$!
  VERIFIER_PORT="$VERIFIER_PORT" DOCWRIGHT_ROOT="$DW" node "$M/verifier/index.mjs" >>"$SERVICES_LOG" 2>&1 & E2E_VERIFIER_PID=$!
  PORT="$MARKET_PORT" \
  SUPABASE_API_URL="http://supabase.lan" \
  PUBLIC_SUPABASE_URL="http://supabase.lan" \
  SUPABASE_SECRET_KEY="$secret_key" \
  SUPABASE_PUBLISHABLE_KEY="$(get_secret supabase-jwt publishable-key)" \
  DATABASE_URL="postgres://postgres:${db_password}@127.0.0.1:${PF_PORT}/postgres" \
  VERIFIER_URL="http://127.0.0.1:${VERIFIER_PORT}" \
  WEB_DIR="$M/web" \
  "$M/server-rs/target/debug/docwright-market" >>"$SERVICES_LOG" 2>&1 & E2E_SERVER_PID=$!

  for _ in $(seq 1 60); do
    curl -sf -m 2 "$MARKET/api/index" >/dev/null 2>&1 && break
    sleep 0.5
  done
  curl -sf -m 2 "$MARKET/api/index" >/dev/null \
    || { echo "market service did not come up — see $SERVICES_LOG" >&2; return 1; }
}

stop_services() {
  $LOCAL_MODE || return 0
  for pid in "${E2E_PF_PID:-}" "${E2E_VERIFIER_PID:-}" "${E2E_SERVER_PID:-}"; do
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  done
}

# --- accounts (per scenario; GoTrue accounts accumulate harmlessly) -----------

load_admin_tools() {
  ADMIN_AUTH=( -H "Authorization: Bearer $(get_secret supabase-jwt service-key)" \
               -H "apikey: $(get_secret supabase-jwt service-key)" \
               -H 'Content-Type: application/json' )
}

# Creates a confirmed GoTrue account and logs it in; sets LAST_TOKEN (Bearer
# header array in AUTH_LAST) and LAST_USER_ID. Registers the id for teardown.
create_account() {
  local email="e2e-$1-$(date +%s)$RANDOM@example.invalid"
  local user_json token_json
  user_json="$(curl -sf -m 10 "${ADMIN_AUTH[@]}" -X POST "http://supabase.lan/auth/v1/admin/users" \
    -d "{\"email\":\"$email\",\"password\":\"e2e-password-1\",\"email_confirm\":true}")" \
    || { sc_fail "cannot create GoTrue account $email"; return 1; }
  LAST_USER_ID="$(echo "$user_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')"
  echo "$LAST_USER_ID" >>"$CREATED_USERS_FILE"
  token_json="$(curl -sf -m 10 -H "apikey: $(get_secret supabase-jwt service-key)" \
    -H 'Content-Type: application/json' \
    -X POST "http://supabase.lan/auth/v1/token?grant_type=password" \
    -d "{\"email\":\"$email\",\"password\":\"e2e-password-1\"}")" \
    || { sc_fail "cannot log $email in"; return 1; }
  LAST_TOKEN="$(echo "$token_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')"
  AUTH_LAST=( -H "Authorization: Bearer $LAST_TOKEN" )
}

# PUT the publisher org for the account that last logged in (AUTH_LAST).
set_org() {
  local org="$1" code
  code="$(http_code "$MARKET/api/account/org" "${AUTH_LAST[@]}" \
    -H 'Content-Type: application/json' -X PUT -d "{\"org\":\"$org\"}")"
  assert_eq "$code" "200" "set_org $org must succeed" || return 1
}

seed_admin_profile() {
  local user_id="$1"
  db_sql "insert into profiles (id, is_admin, display_name) values ('$user_id', true, 'e2e-admin')
          on conflict (id) do update set is_admin = true;" >/dev/null \
    || { sc_fail "cannot seed admin profile"; return 1; }
}

# Residue-scoped catalog reset. NEVER truncate: the live catalog belongs to
# real accounts now — this only clears rows owned by throwaway e2e accounts.
reset_catalog() {
  db_sql "
    delete from pack_versions where name in (
      select name from packs where author_id in (
        select id from auth.users where email like '%@example.invalid' or email like '%@x.invalid'));
    delete from packs where author_id in (
      select id from auth.users where email like '%@example.invalid' or email like '%@x.invalid');
    delete from suites where author_id in (
      select id from auth.users where email like '%@example.invalid' or email like '%@x.invalid');
    delete from submissions where uploader in (
      select id from auth.users where email like '%@example.invalid' or email like '%@x.invalid');
  " >/dev/null || { echo "cannot reset catalog" >&2; return 1; }
  # Best-effort storage cleanup for the same rows (orphans are unreachable,
  # but don't let them pile up).
  local path
  while IFS='|' read -r bucket object; do
    [ -n "$object" ] || continue
    storage_delete "$bucket" "$object" || true
  done < <(db_sql "
    select 'tarballs', replace(tarball_path, 'tarballs/', '') from pack_versions where name in (
      select name from packs where author_id in (
        select id from auth.users where email like '%@example.invalid' or email like '%@x.invalid'));
  " 2>/dev/null; db_sql "
    select 'uploads', archive_path from submissions where uploader in (
      select id from auth.users where email like '%@example.invalid' or email like '%@x.invalid')
      and archive_path is not null;
  " 2>/dev/null)
}

# Storage REST delete with the service key (Storage rejects the sb_secret_ key).
storage_delete() {
  local bucket="$1" object="$2"
  curl -sf -m 10 -X DELETE "http://supabase.lan/storage/v1/object/$bucket/$object" \
    -H "Authorization: Bearer $(get_secret supabase-jwt service-key)" >/dev/null
}

# Removes every account this run created, with their packs/submissions and
# Storage objects — children before auth.users (packs.author_id and
# submissions.uploader have no ON DELETE CASCADE). Registered in run.sh's
# EXIT trap so runs leave no residue behind.
teardown_accounts() {
  local uid
  while IFS= read -r uid; do
    [ -n "$uid" ] || continue
    # Storage objects owned by this account (pack tarballs + upload archives).
    while IFS='|' read -r bucket object; do
      [ -n "$object" ] || continue
      storage_delete "$bucket" "$object" || true
    done < <(db_sql "
      select 'tarballs', replace(v.tarball_path, 'tarballs/', '') from pack_versions v
        join packs p on p.name = v.name where p.author_id = '$uid';
      select 'uploads', s.archive_path from submissions s
        where s.uploader = '$uid' and s.archive_path is not null;
    " 2>/dev/null)
    db_sql "
      delete from pack_versions where name in (select name from packs where author_id = '$uid');
      delete from packs where author_id = '$uid';
      delete from suites where author_id = '$uid';
      delete from submissions where uploader = '$uid';
      delete from profiles where id = '$uid';
    " >/dev/null 2>&1 || true
    curl -sf -m 10 "${ADMIN_AUTH[@]}" -X DELETE \
      "http://supabase.lan/auth/v1/admin/users/$uid" >/dev/null 2>&1 || true
  done <"$CREATED_USERS_FILE"
  : >"$CREATED_USERS_FILE"
}

# --- scenario framework --------------------------------------------------------

sc_fail() { echo "  ✗ $CURRENT_SCENARIO: $*" >&2; return 1; }

scenario() { SCENARIOS+=("$1"); SCENARIO_FN+=("$2"); }

# Common per-scenario preamble (hook point for future per-scenario setup).
init_run() { :; }

assert_eq() {
  local got="$1" want="$2" msg="$3"
  if [ "$got" != "$want" ]; then sc_fail "$msg (got: $got, want: $want)"; return 1; fi
}

assert_contains() {
  local hay="$1" needle="$2" msg="$3"
  case "$hay" in
    *"$needle"*) ;;
    *) sc_fail "$msg (missing: $needle)"; return 1 ;;
  esac
}

run_scenario() {
  local name="$1" fn="$2"
  CURRENT_SCENARIO="$name"
  reset_catalog || { FAILED+=("$name"); echo "FAIL $name (catalog reset)"; return 0; }
  echo "--- scenario: $name ---"
  if ( set -e; $fn ); then
    echo "  PASS $name"; PASSED+=("$name")
  else
    echo "  FAIL $name"; FAILED+=("$name")
  fi
}

# --- API helpers ----------------------------------------------------------------

http_code() { curl -s -m 90 -o /dev/null -w '%{http_code}' "$@"; }

# Submits the given archive with the given token; sets LAST_SUB_ID. Requires
# the verifier to be green on it.
submit_archive() {
  local token="$1" archive="$2"
  local raw code json
  raw="$(curl -s -m 120 -w '\n%{http_code}' "$MARKET/api/submissions" -H "Authorization: Bearer $token" \
    -F "archive=@${archive:-$M/samples/task-card.tgz}" \
    -F "author=sharework-ai" -F "category=dev" -F "license=MIT")" \
    || { sc_fail "submit call failed (transport)"; return 1; }
  code="$(echo "$raw" | tail -1)"
  json="$(echo "$raw" | sed '$d')"
  [ "$code" = "200" ] || { sc_fail "submit call failed -> $code: $(echo "$json" | head -c 200)"; return 1; }
  local ok sub_id
  ok="$(echo "$json" | python3 -c 'import json,sys;print(json.load(sys.stdin).get("ok"))')"
  [ "$ok" = "True" ] || { sc_fail "verification rejected the archive: $(echo "$json" | head -c 300)"; return 1; }
  LAST_SUB_ID="$(echo "$json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')"
}

# Submits + approves task-card at its sample version; sets LAST_SUB_ID.
publish_sample_pack() {
  local token="$1"
  submit_archive "$token" "$M/samples/task-card.tgz" || return 1
  curl -sf -m 30 "$MARKET/api/admin/review/$LAST_SUB_ID" -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -d '{"action":"approve","note":"e2e"}' >/dev/null \
    || { sc_fail "approve failed"; return 1; }
}

# Builds a byte-real pack fixture from the task-card sample with a rewritten
# manifest name + version. Scenarios publish these renamed fixtures — never
# the real task-card, which belongs to a live account once the market has
# real users (ownership + version-uniqueness rules would reject it).
sample_archive_as() {
  local name="$1" version="$2" out="$3"
  local dir
  dir="$(mktemp -d "$T/fx-$name-$version-XXXXXX")"
  tar xzf "$M/samples/task-card.tgz" -C "$dir"
  [ "$name" = "task-card" ] || mv "$dir/task-card" "$dir/$name"
  python3 - "$dir/$name/manifest.json" "$name" "$version" <<'PY'
import json, sys
p, n, v = sys.argv[1], sys.argv[2], sys.argv[3]
m = json.load(open(p)); m["name"] = n; m["version"] = v; json.dump(m, open(p, "w"))
PY
  tar czf "$out" -C "$dir" "$name"
}

# Bumps the sample archive to the given version (verifier still greens on it:
# no rule inspects the version field).
sample_archive_at_version() {
  local version="$1" out="$2"
  sample_archive_as "task-card" "$version" "$out"
}

# Submits + approves the given fixture archive; sets LAST_SUB_ID.
publish_fixture() {
  local token="$1" archive="$2"
  submit_archive "$token" "$archive" || return 1
  curl -sf -m 30 "$MARKET/api/admin/review/$LAST_SUB_ID" -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' -d '{"action":"approve","note":"e2e"}' >/dev/null \
    || { sc_fail "approve failed"; return 1; }
}
