-- compatibleFrom: the pack manifest's backward-compat floor (docs in the
-- docwright repo: docs/designs/pack-market-contract.md). Extracted from the
-- submitted manifest at publish time like consumes/requiresTopic; NULL means
-- "no floor declared — always compatible". history keeps per-version rows so
-- an installed consumer can look up the floor of the exact version it runs.
alter table packs add column if not exists compatible_from text;
alter table pack_versions add column if not exists compatible_from text;
