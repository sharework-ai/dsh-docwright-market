#!/usr/bin/env bash
# Provision + deploy the market onto the local k3s.
#
# Creates (idempotently) the market namespace, the market-env secret
# (credentials lifted from the supabase namespace — the cluster-internal
# service endpoints are baked here too), and a copy of the shrwk-tls
# certificate; then applies k8s.yaml (TAG placeholder swapped for the image
# tag) and waits for both rollouts.
#
# Usage: deploy/deploy.sh --tag <tag>
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NS=market
TAG="${TAG:-}"
[ -n "$TAG" ] || { while [ $# -gt 0 ]; do
  [ "$1" = "--tag" ] && TAG="${2:?tag required}"; shift; done; }
[ -n "$TAG" ] || { echo "usage: $0 --tag <tag>" >&2; exit 1; }

get_secret() { kubectl -n supabase get secret "$1" -o "jsonpath={.data.$2}" | base64 -d; }

kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f -

# market-env: server credentials + cluster-internal endpoints.
kubectl -n "$NS" create secret generic market-env \
  --from-literal=SUPABASE_API_URL="http://supabase-envoy.supabase.svc:8000" \
  --from-literal=SUPABASE_SECRET_KEY="$(get_secret supabase-jwt service-key)" \
  --from-literal=SUPABASE_PUBLISHABLE_KEY="$(get_secret supabase-jwt publishable-key)" \
  --from-literal=PUBLIC_SUPABASE_URL="https://sb.shrwk.com" \
  --from-literal=DATABASE_URL="postgres://postgres:$(get_secret supabase-postgres-auth password)@supabase-postgres.supabase.svc:5432/postgres" \
  --from-literal=VERIFIER_URL="http://verifier.$NS.svc:8399" \
  --from-literal=PUBLIC_ORIGIN="https://dm.shrwk.com" \
  --dry-run=client -o yaml | kubectl apply -f -

# shrwk-tls: wildcard certificate copy (IngressRoute TLS secrets are
# namespace-local).
kubectl -n dsh get secret shrwk-tls -o json \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); d["metadata"]["namespace"]="market"; d["metadata"].pop("creationTimestamp",None); d["metadata"].pop("resourceVersion",None); d["metadata"].pop("uid",None); print(json.dumps(d))' \
  | kubectl apply -f -

# ---- migrations FIRST (a new image whose columns are missing answers 500
# until the DDL lands — this bit us twice: run DDL before the rollout) ----
PG_PW="$(get_secret supabase-postgres-auth password)"
db_psql() {
  kubectl -n supabase exec -i supabase-postgres-0 -- env PGPASSWORD="$PG_PW" \
    psql -U postgres -d postgres -v ON_ERROR_STOP=1 "$@"
}
echo "== applying pending migrations -> public"
db_psql -qc 'create table if not exists public.schema_migrations (name text primary key, applied_at timestamptz not null default now())' >/dev/null
for f in "$M"/supabase/migrations/*.sql; do
  base="$(basename "$f")"
  [ "$base" = "0000_auth_stub.sql" ] && continue
  [ "$(db_psql -tAc "select 1 from public.schema_migrations where name = '$base'")" = "1" ] && continue
  echo "== migrating $base -> public"
  db_psql -qf - < "$f"
  db_psql -qc "insert into public.schema_migrations (name) values ('$base')"
done

sed -e "s/__TAG__/$TAG/g" -e "s/__VTAG__/${VTAG:-$TAG}/g" "$M/deploy/k8s.yaml" | kubectl apply -f -

echo "waiting for rollouts..."
kubectl -n "$NS" rollout status deploy/market-server --timeout=5m
kubectl -n "$NS" rollout status deploy/market-verifier --timeout=5m
kubectl -n "$NS" get pods -o wide
echo
echo "LAN/public entry: https://dm.shrwk.com   (hosts: 192.168.2.1 dm.shrwk.com)"
echo "smoke: curl -sk https://dm.shrwk.com/api/index"
