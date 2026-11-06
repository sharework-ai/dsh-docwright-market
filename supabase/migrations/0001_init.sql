-- 0001_init.sql — docwright pack market schema (tables only).
-- Apply via: ~/projects/commons/supabase/setup.sh apply-migration <this file>
--
-- Access model: the market service connects with the service_role-level secret
-- key and talks to Postgres directly (sqlx). RLS is intentionally NOT enabled
-- for v1 — the anon/publishable key is never exposed to untrusted callers.
-- Revisit RLS when moving reads to supabase-js direct.
--
-- Storage buckets live in ../provision/storage-buckets.sql (kept out of this
-- file so the schema is testable against a plain Postgres via sqlx::test; the
-- service also idempotently ensures buckets at boot).

-- Author profiles: admin flag + display name. One row per signup; the admin
-- bootstrap is a manual UPDATE after your own signup (see README):
--   insert into profiles (id, is_admin, display_name)
--   values ('<your-auth-users-id>', true, '<name>')
--   on conflict (id) do update set is_admin = true;
create table if not exists profiles (
  id           uuid primary key references auth.users(id) on delete cascade,
  is_admin     boolean not null default false,
  display_name text
);

-- Accepted packs (one row per pack name; updates overwrite in place).
create table if not exists packs (
  name               text primary key,
  version            text not null,
  description        jsonb not null,          -- {en: ..., zh?: ...} from manifest
  category           text not null,
  author             text not null,           -- display author (submission form)
  author_id          uuid not null references auth.users(id),  -- owning account
  license            text not null,           -- SPDX id
  homepage           text,
  consumes           jsonb not null default '[]',
  languages          jsonb not null default '[]',
  requires_topic     boolean not null default false,
  artifact_prefix    text,
  has_function_rules boolean not null default false,
  content_hash       text not null,           -- sha256 over sorted (relpath, bytes)
  tarball_path       text not null,           -- Storage object path in 'tarballs'
  tarball_sha256     text not null,
  added              timestamptz not null default now(),
  updated            timestamptz not null default now()
);

-- Submissions: pending review queue + audit trail of approved/rejected.
-- Form metadata (category/author/license/homepage) is stored here because the
-- approve action moves it into `packs` — pack authors never edit manifests'
-- market fields by hand.
create table if not exists submissions (
  id           uuid primary key default gen_random_uuid(),
  pack_name    text not null,
  version      text not null,
  uploader     uuid not null references auth.users(id),
  status       text not null default 'pending'
               check (status in ('pending', 'approved', 'rejected')),
  report       jsonb not null default '{}',  -- verifier output (shape + pack:check)
  archive_path text,                         -- raw tgz object path in 'uploads'
  category     text,
  author       text,
  license      text,
  homepage     text,
  review_note  text,
  reviewed_by  uuid references auth.users(id),
  reviewed_at  timestamptz,
  created_at   timestamptz not null default now(),
  unique (pack_name, version)
);

create index if not exists submissions_pending_idx
  on submissions (created_at) where status = 'pending';
