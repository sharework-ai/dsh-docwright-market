-- Category renames: software -> it, home -> media, construction -> engineering.
-- Idempotence guard: 0006 renames the column; on an already-renamed database
-- these updates are no-ops.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema = current_schema() AND table_name = 'packs' AND column_name = 'category'
  ) THEN
    UPDATE packs SET category = 'it' WHERE category = 'software';
    UPDATE packs SET category = 'media' WHERE category = 'home';
    UPDATE packs SET category = 'engineering' WHERE category = 'construction';
    UPDATE suites SET category = 'it' WHERE category = 'software';
    UPDATE suites SET category = 'media' WHERE category = 'home';
    UPDATE suites SET category = 'engineering' WHERE category = 'construction';
    UPDATE submissions SET category = 'it' WHERE category = 'software';
    UPDATE submissions SET category = 'media' WHERE category = 'home';
    UPDATE submissions SET category = 'engineering' WHERE category = 'construction';
  END IF;
END $$;
