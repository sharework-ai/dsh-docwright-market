-- Display title (multilingual jsonb, falls back to name) and the suite
-- membership flag (suite code) for packs; suites get a title too. The
-- assets themselves stay inside the published tarballs — the market pages
-- extract them client-side, so no assets column is needed.
alter table packs add column if not exists title jsonb;
alter table packs add column if not exists suite text;
alter table packs add column if not exists assets jsonb;
alter table suites add column if not exists title jsonb;
