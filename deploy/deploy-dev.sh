#!/usr/bin/env bash
# Provision + deploy the DEV market onto the local k3s (deploy/k8s-dev.yaml).
#
# A resource-light second market: schema market_dev (role-scoped search_path
# so the server's unqualified sqlx queries land there with zero code
# changes), buckets uploads-dev/tarballs-dev (UPLOADS_BUCKET/TARBALLS_BUCKET
# env), domain dm-dev.shrwk.com. Auth stays the SHARED supabase GoTrue — the
# same login works on both markets; profiles are per-schema and bootstrapped
# from production (admin included) so the existing account administers dev.
#
# One-time DB prep (idempotent, re-run safe): schema + login role + DML
# grants + migrations + admin copy. The role password is a fixed dev-only
# literal (cluster-internal role, no external exposure).
#
# Usage: deploy/deploy-dev.sh --tag <tag>
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NS=market-dev
TAG="${TAG:-}"
[ -n "$TAG" ] || { while [ $# -gt 0 ]; do
  [ "$1" = "--tag" ] && TAG="${2:?tag required}"; shift; done; }
[ -n "$TAG" ] || { echo "usage: $0 --tag <tag>" >&2; exit 1; }

get_secret() { kubectl -n supabase get secret "$1" -o "jsonpath={.data.$2}" | base64 -d; }
db() { kubectl -n supabase exec -i supabase-postgres-0 -- env PGPASSWORD="$(get_secret supabase-postgres-auth password)" psql -U postgres -d postgres "$@"; }

kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f -

# ---- one-time DB prep: schema, role, migrations, grants, admin copy ----
db <<'SQL'
CREATE SCHEMA IF NOT EXISTS market_dev AUTHORIZATION postgres;
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'market_dev') THEN
    CREATE ROLE market_dev LOGIN PASSWORD 'market-dev-local-only';
  END IF;
END $$;
GRANT USAGE ON SCHEMA market_dev TO market_dev;
ALTER ROLE market_dev SET search_path = market_dev;
SQL
# Migrations run as postgres with search_path pinned to the dev schema
# (0000_auth_stub is plain-Postgres-only and fails loud on real supabase —
# skip it, auth is the shared GoTrue). A tracker table makes re-runs no-ops
# and guarantees DDL lands BEFORE the new image rolls out.
PG_PW="$(get_secret supabase-postgres-auth password)"
db_mig() {
  kubectl -n supabase exec -i supabase-postgres-0 -- env \
    PGPASSWORD="$PG_PW" PGOPTIONS="-c search_path=market_dev" \
    psql -U postgres -d postgres -v ON_ERROR_STOP=1 "$@"
}
db_mig -qc 'create table if not exists market_dev.schema_migrations (name text primary key, applied_at timestamptz not null default now())' >/dev/null
for f in "$M"/supabase/migrations/*.sql; do
  base="$(basename "$f")"
  [ "$base" = "0000_auth_stub.sql" ] && continue
  [ "$(db_mig -tAc "select 1 from market_dev.schema_migrations where name = '$base'")" = "1" ] && continue
  echo "== migrating $base -> market_dev"
  db_mig -qf - < "$f"
  db_mig -qc "insert into market_dev.schema_migrations (name) values ('$base')"
done

db <<'SQL'
GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA market_dev TO market_dev;
GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA market_dev TO market_dev;
ALTER DEFAULT PRIVILEGES IN SCHEMA market_dev GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO market_dev;
ALTER DEFAULT PRIVILEGES IN SCHEMA market_dev GRANT USAGE,SELECT ON SEQUENCES TO market_dev;
INSERT INTO market_dev.profiles (id, is_admin, display_name, org)
SELECT id, is_admin, display_name, org FROM public.profiles
ON CONFLICT (id) DO UPDATE SET is_admin = EXCLUDED.is_admin, display_name = EXCLUDED.display_name, org = EXCLUDED.org;
SQL

# ---- secret: dev credentials + cluster-internal endpoints ----
kubectl -n "$NS" create secret generic market-dev-env \
  --from-literal=SUPABASE_API_URL="http://supabase-envoy.supabase.svc:8000" \
  --from-literal=SUPABASE_SECRET_KEY="$(get_secret supabase-jwt service-key)" \
  --from-literal=SUPABASE_PUBLISHABLE_KEY="$(get_secret supabase-jwt publishable-key)" \
  --from-literal=PUBLIC_SUPABASE_URL="https://sb.shrwk.com" \
  --from-literal=DATABASE_URL="postgres://market_dev:market-dev-local-only@supabase-postgres.supabase.svc:5432/postgres" \
  --from-literal=VERIFIER_URL="http://verifier.$NS.svc:8399" \
  --from-literal=PUBLIC_ORIGIN="https://dm-dev.shrwk.com" \
  --dry-run=client -o yaml | kubectl apply -f -

# shrwk-tls: wildcard certificate copy (IngressRoute TLS secrets are
# namespace-local).
kubectl -n dsh get secret shrwk-tls -o json \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); d["metadata"]["namespace"]="market-dev"; d["metadata"].pop("creationTimestamp",None); d["metadata"].pop("resourceVersion",None); d["metadata"].pop("uid",None); print(json.dumps(d))' \
  | kubectl apply -f -

sed -e "s/__TAG__/$TAG/g" "$M/deploy/k8s-dev.yaml" | kubectl apply -f -

echo "waiting for rollouts..."
kubectl -n "$NS" rollout status deploy/market-server --timeout=5m
kubectl -n "$NS" rollout status deploy/market-verifier --timeout=5m
kubectl -n "$NS" get pods -o wide
echo
echo "LAN/public entry: https://dm-dev.shrwk.com   (hosts: 192.168.2.1 dm-dev.shrwk.com)"
echo "smoke: curl -sk https://dm-dev.shrwk.com/api/index"
