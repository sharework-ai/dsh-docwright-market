-- category -> tags: a pack or suite may belong to several domains, so the
-- single-choice text column becomes a text array. Idempotence note: this is
-- a one-shot rename/type change (migrations run once per database).
ALTER TABLE packs RENAME COLUMN category TO tags;
ALTER TABLE packs ALTER COLUMN tags TYPE text[] USING ARRAY[tags];
ALTER TABLE packs ALTER COLUMN tags SET DEFAULT '{}'::text[];

ALTER TABLE suites RENAME COLUMN category TO tags;
ALTER TABLE suites ALTER COLUMN tags TYPE text[] USING ARRAY[tags];
ALTER TABLE suites ALTER COLUMN tags SET DEFAULT '{}'::text[];

ALTER TABLE submissions RENAME COLUMN category TO tags;
ALTER TABLE submissions ALTER COLUMN tags TYPE text[] USING CASE WHEN tags IS NULL THEN ARRAY[]::text[] ELSE ARRAY[tags] END;
