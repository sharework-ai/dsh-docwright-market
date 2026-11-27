-- Category taxonomy v2: workflow roles collapse into domain categories.
-- estimation -> business; dev/pm/test/docs -> software.
-- Idempotence guard: 0006 renames the column; on an already-renamed database
-- these updates are no-ops.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema = current_schema() AND table_name = 'packs' AND column_name = 'category'
  ) THEN
    UPDATE packs SET category = 'business' WHERE category = 'estimation';
    UPDATE packs SET category = 'software' WHERE category IN ('dev', 'pm', 'test', 'docs');
    UPDATE suites SET category = 'business' WHERE category = 'estimation';
    UPDATE suites SET category = 'software' WHERE category IN ('dev', 'pm', 'test', 'docs');
    UPDATE submissions SET category = 'business' WHERE category = 'estimation';
    UPDATE submissions SET category = 'software' WHERE category IN ('dev', 'pm', 'test', 'docs');
  END IF;
END $$;
