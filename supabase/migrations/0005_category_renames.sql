-- Category renames: software -> it (信息技术), home -> media (多媒体),
-- construction -> engineering (工程). Idempotent.
UPDATE packs SET category = 'it' WHERE category = 'software';
UPDATE packs SET category = 'media' WHERE category = 'home';
UPDATE packs SET category = 'engineering' WHERE category = 'construction';
UPDATE suites SET category = 'it' WHERE category = 'software';
UPDATE suites SET category = 'media' WHERE category = 'home';
UPDATE suites SET category = 'engineering' WHERE category = 'construction';
UPDATE submissions SET category = 'it' WHERE category = 'software';
UPDATE submissions SET category = 'media' WHERE category = 'home';
UPDATE submissions SET category = 'engineering' WHERE category = 'construction';
