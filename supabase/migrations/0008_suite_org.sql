-- Suites get a publisher org (from the author's profile at publish time) so
-- a suite's qualified identity is org/suite-name, matching packs. Backfill
-- from the profile's CURRENT org (suites predate the column).
alter table suites add column if not exists org text;
update suites set org = (select org from profiles p where p.id = suites.author_id)
where org is null;
