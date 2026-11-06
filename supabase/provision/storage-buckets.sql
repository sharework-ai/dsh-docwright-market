-- 0002_storage.sql — market storage buckets.
-- uploads: private, raw submission archive (audit trail).
-- tarballs: public read, install-time downloads.
-- The service re-runs an idempotent ensure at boot; this migration covers
-- provisioning from SQL alone (e.g. fresh instance without a service boot).
insert into storage.buckets (id, name, public)
values
  ('uploads', 'uploads', false),
  ('tarballs', 'tarballs', true)
on conflict (id) do nothing;
