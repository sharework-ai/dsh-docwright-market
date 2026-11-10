-- 0002_org_versions.sql — publisher org identity + full version history +
-- owner-managed lifecycle state.
--
-- Pure SQL on purpose: sqlx::test applies the whole migrations directory to
-- scratch Postgres databases that have no `storage` schema (see
-- 0001_init.sql's header). Storage bucket provisioning stays in
-- provision/storage-buckets.sql / server-side ensure_buckets.

-- ---------------------------------------------------------------------------
-- Publisher org on profiles: nullable (legacy accounts keep working), but
-- submitting requires one. Unique among accounts that HAVE set one — many
-- NULLs coexist, so the partial unique index does the job.
-- (Idempotent: Postgres has no ADD CONSTRAINT IF NOT EXISTS, so the shape
-- check guards by name — re-running apply-migration must stay safe.)
alter table profiles add column if not exists org text;
do $$ begin
  if not exists (select from pg_constraint where conname = 'profiles_org_shape' and conrelid = 'profiles'::regclass) then
    alter table profiles add constraint profiles_org_shape
      check (org is null or org ~ '^[a-z0-9][a-z0-9-]{0,38}$');
  end if;
end $$;
create unique index if not exists profiles_org_unique_idx
  on profiles (org) where org is not null;

-- ---------------------------------------------------------------------------
-- Catalog lifecycle + publisher org on packs. `status` is owner-managed
-- (active | disabled); disabled packs are hidden from the public surface and
-- their tarballs serve 410.
alter table packs add column if not exists status text not null default 'active'
  check (status in ('active', 'disabled'));
alter table packs add column if not exists org text;

-- ---------------------------------------------------------------------------
-- Full version history: one row per approved publish. `packs` stays the
-- latest-version projection the read surface already uses. `name` cascades
-- from packs so deleting a pack removes its whole history in one statement;
-- `submission_id` is provenance only (deliberately no FK: submissions history
-- outlives packs and must never block a delete).
create table if not exists pack_versions (
  name               text not null references packs(name) on delete cascade,
  version            text not null,
  description        jsonb not null,
  content_hash       text not null,
  tarball_path       text not null,
  tarball_sha256     text not null,
  has_function_rules boolean not null default false,
  consumes           jsonb not null default '[]',
  languages          jsonb not null default '[]',
  requires_topic     boolean not null default false,
  artifact_prefix    text,
  submission_id      uuid,
  published_at       timestamptz not null default now(),
  primary key (name, version)
);
create index if not exists pack_versions_name_published_idx
  on pack_versions (name, published_at desc);

-- ---------------------------------------------------------------------------
-- Backfill: every currently-published pack becomes the first history entry.
-- published_at = packs.updated (best-known publish time); submission_id from
-- the matching approved submission when one exists.
insert into pack_versions
  (name, version, description, content_hash, tarball_path, tarball_sha256,
   has_function_rules, consumes, languages, requires_topic, artifact_prefix,
   published_at, submission_id)
select p.name, p.version, p.description, p.content_hash, p.tarball_path, p.tarball_sha256,
       p.has_function_rules, p.consumes, p.languages, p.requires_topic, p.artifact_prefix,
       p.updated,
       (select s.id from submissions s
         where s.pack_name = p.name and s.version = p.version and s.status = 'approved'
         limit 1)
from packs p
on conflict (name, version) do nothing;
