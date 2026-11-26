-- Category taxonomy v2: workflow roles collapse into domain categories.
-- estimation -> business; dev/pm/test/docs -> software. Idempotent.
UPDATE packs SET category = 'business' WHERE category = 'estimation';
UPDATE packs SET category = 'software' WHERE category IN ('dev', 'pm', 'test', 'docs');
UPDATE suites SET category = 'business' WHERE category = 'estimation';
UPDATE suites SET category = 'software' WHERE category IN ('dev', 'pm', 'test', 'docs');
UPDATE submissions SET category = 'business' WHERE category = 'estimation';
UPDATE submissions SET category = 'software' WHERE category IN ('dev', 'pm', 'test', 'docs');
