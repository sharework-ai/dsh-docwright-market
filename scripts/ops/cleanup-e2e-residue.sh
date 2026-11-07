#!/usr/bin/env bash
# One-time ops: remove e2e residue accounts and everything they own from the
# live supabase (packs, version history, submissions, Storage objects,
# profiles, auth.users), so the market starts from a clean catalog and the
# real pack (task-card) can be re-published under the real account.
#
# Usage:
#   scripts/ops/cleanup-e2e-residue.sh                # dry run (default)
#   scripts/ops/cleanup-e2e-residue.sh --apply        # actually delete
#   scripts/ops/cleanup-e2e-residue.sh --protect-email a@b.c   # extra guard
#
# Safety rails:
#   - only emails matching the throwaway domains are selected (e2e residue is
#     admin-flagged too, so is_admin is deliberately NOT a guard here)
#   - --protect-email entries are skipped even if they match the pattern
set -euo pipefail

NS=supabase
MODE=dry
PROTECT_EMAILS=("peng_long@hisuntech.com")
while [ $# -gt 0 ]; do
  case "$1" in
    --apply) MODE=apply ;;
    --protect-email) PROTECT_EMAILS+=("${2:?--protect-email needs an address}"); shift ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
  shift
done

dbpod="$(kubectl -n "$NS" get pods --no-headers -o custom-columns=':metadata.name' \
  | grep -E 'single.*db|postgres' | head -1)"
[ -n "$dbpod" ] || { echo "supabase postgres pod not found" >&2; exit 1; }
# SQL comes in as $1; stderr is suppressed (kubectl's "Defaulted container"
# noise would otherwise corrupt the captured output).
db_sql() {
  kubectl -n "$NS" exec -i "$dbpod" -- psql -U postgres -d postgres -Atq 2>/dev/null <<<"$1"
}
service_key="$(kubectl -n "$NS" get secret supabase-jwt -o jsonpath='{.data.service-key}' | base64 -d)"
storage_delete() {
  curl -sf -m 10 -X DELETE "http://supabase.lan/storage/v1/object/$1/$2" \
    -H "Authorization: Bearer $service_key" >/dev/null
}

# --- select residue ------------------------------------------------------------
protect_clause=""
for email in "${PROTECT_EMAILS[@]}"; do
  protect_clause+="'$email',"
done
protect_clause="${protect_clause%,}"

USERS="$(db_sql "select id, email from auth.users
where (email like '%@example.invalid' or email like '%@x.invalid')
  and coalesce(email, '') not in ($protect_clause)
order by created_at;")"

COUNT="$(echo "$USERS" | grep -c . || true)"
echo "residue accounts: $COUNT  (mode: $MODE)"
[ "$COUNT" -gt 0 ] || exit 0

# --- per-account plan ----------------------------------------------------------
while IFS='|' read -r uid email; do
  [ -n "$uid" ] || continue
  packs="$(db_sql "select count(*) from packs where author_id = '$uid';")"
  subs="$(db_sql "select count(*) from submissions where uploader = '$uid';")"
  echo "  $email  packs=$packs submissions=$subs"
  if [ "$MODE" = apply ]; then
    # Storage objects first (they reference nothing), then DB children, then
    # the auth user (profiles cascades; packs/submissions do not).
    db_sql "select replace(v.tarball_path, 'tarballs/', '') from pack_versions v
        join packs p on p.name = v.name where p.author_id = '$uid'
        union all
      select tarball_path from packs where author_id = '$uid' and tarball_path like 'tarballs/%';" \
      | while read -r object; do [ -n "$object" ] && storage_delete tarballs "$object" || true; done
    db_sql "select archive_path from submissions where uploader = '$uid' and archive_path is not null;" \
      | while read -r object; do [ -n "$object" ] && storage_delete uploads "$object" || true; done
    db_sql "delete from pack_versions where name in (select name from packs where author_id = '$uid');
      delete from packs where author_id = '$uid';
      delete from submissions where uploader = '$uid';
      delete from profiles where id = '$uid';" >/dev/null
    curl -sf -m 10 -X DELETE "http://supabase.lan/auth/v1/admin/users/$uid" \
      -H "Authorization: Bearer $service_key" -H "apikey: $service_key" >/dev/null \
      || echo "    (GoTrue delete failed for $uid — check manually)"
  fi
done <<<"$USERS"

if [ "$MODE" = apply ]; then
  echo "done. deleted $COUNT residue accounts and their data."
else
  echo "dry run — nothing deleted. Re-run with --apply to delete."
fi
echo
echo "next: on https://dm.shrwk.com — sign in as the real account, set the org"
echo "on /account.html, then re-submit samples/task-card.tgz via /submit.html."
