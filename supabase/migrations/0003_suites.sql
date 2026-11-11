-- 0003_suites.sql — pack suites: a named, curated group of packs installable
-- as one unit. A suite is metadata only (no tarball, no verification); the
-- packs inside stay independent catalog rows.
--
-- Pure SQL and idempotent, same discipline as 0002: sqlx::test applies the
-- whole migrations directory to scratch databases, so re-running
-- apply-migration must stay safe.

-- The catalog of suites. `members` is a jsonb string array of pack names;
-- membership is validated at submission time against `packs`, never by FK —
-- packs can be deleted later, and the suite keeps the name so the read
-- surface can flag it as missing.
create table if not exists suites (
  name        text primary key,
  version     text not null,
  description jsonb not null,            -- {en: ..., zh?: ...}, same shape as packs
  category    text not null,
  author      text not null,             -- display author (submission form)
  author_id   uuid not null references auth.users(id),
  members     jsonb not null default '[]',
  enabled     boolean not null default true,
  added       timestamptz not null default now(),
  updated     timestamptz not null default now()
);

-- ---------------------------------------------------------------------------
-- Submissions learn about suites: kind distinguishes pack uploads (with an
-- archive + verifier report) from suite form submissions (no archive; the
-- whole form lives in suite_meta). Existing rows keep working unchanged.
alter table submissions add column if not exists kind text not null default 'pack';
do $$ begin
  if not exists (select from pg_constraint where conname = 'submissions_kind_shape' and conrelid = 'submissions'::regclass) then
    alter table submissions add constraint submissions_kind_shape
      check (kind in ('pack', 'suite'));
  end if;
end $$;
alter table submissions add column if not exists suite_meta jsonb;
