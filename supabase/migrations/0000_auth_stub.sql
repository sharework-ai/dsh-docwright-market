-- 0000_auth_stub.sql — plain-Postgres shim for GoTrue's auth schema.
-- On a real Supabase instance this is a no-op: the auth schema and auth.users
-- already exist (create ... if not exists skips). In sqlx::test scratch
-- databases (plain Postgres, no GoTrue) it provides the minimal shape the
-- market tables' foreign keys reference. Application code never writes it.
create schema if not exists auth;
create table if not exists auth.users (
  id    uuid primary key,
  email text
);
