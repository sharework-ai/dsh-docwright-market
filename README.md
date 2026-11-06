# dsh-docwright-market

A submission-based marketplace for [docwright](https://github.com/sharework-ai/dsh-docwright)
packs: authors upload a `.tgz` from the browser, machine verification runs
docwright's own `pack:check` (golden-green or nothing enters the queue), a human
approves, and consumers install into their `packsDir` with a zero-dependency
script. No git/GitHub knowledge required to submit; no accounts beyond an email
signup.

中文文档:[README.zh.md](README.zh.md)

## How it fits together

```
browser (submit/admin/browse pages)
   │  JWT (Supabase Auth)
   ▼
market service (Rust, axum)  ──POST /verify──▶  verifier sidecar (node)
   │  sqlx                │ REST                 │ spawn docwright pack:check
   ▼                      ▼                      ▼ (the loader IS the linter)
Postgres            Storage (uploads/         unpack → shape checks → golden
(packs/submissions/  tarballs buckets)         validation via the real loader
 profiles)
```

- **Rust service** (`server-rs/`): product flow only — auth, catalog, review
  queue, tarball hosting, static pages. Never touches pack semantics.
- **Verifier sidecar** (`verifier/`): the single validation surface. Unpacks the
  archive (traversal-safe), checks market-level shape rules (kebab-case names,
  no builtin-name collisions, dir == manifest.name), then runs docwright's own
  `pack:check` with the built-in packs as upstream roots. `checkGreen` in the
  report is docwright's verdict, not a reimplementation.
- **Supabase** (self-hosted, see `~/projects/commons/supabase`): Auth (email
  signup), Postgres (catalog + review queue), Storage (raw uploads kept as an
  audit trail; public tarball downloads).

## Security model

1. Machine gate first: only `pack:check`-green packs reach the review queue.
2. Human review is the only gate for `rules/functions.js` — executable code
   that runs on every user's machine when docwright loads the pack. The admin
   page shows the file tree and health sheet; the site and index flag packs
   containing function rules (`hasFunctionRules`).
3. Installing only copies bytes (integrity-checked against the index sha256);
   the installer never executes pack content.
4. Updates must come from the original author account, strictly increase the
   version, and change the content hash. Submissions enter the queue only once
   (same-name pending blocks re-submission).

## Install a pack (consumer)

```sh
node install.mjs task-card                     # latest, into ./.docwright/packs/
node install.mjs task-card --into /path/to/packs
node install.mjs --list                        # what's in the market
```

Then restart the dsh host and check `doc_packs`. Packs that consume other
market packs print a note to install those too; built-in upstreams resolve
automatically.

## Submit a pack (author)

1. Build your pack following docwright's bundled authoring guide
   (`guides/pack-authoring.md` in the docwright package) and make it green
   locally: `pnpm run pack:check -- <your-pack-dir> --with packs` (run inside
   the docwright checkout).
2. Archive it: `tar czf your-pack.tgz <pack-dir>` (the directory with
   `manifest.json` at the top, either as the archive root or its single child).
3. Open `/submit.html` on the market, sign up (instant on self-hosted hosts),
   fill the form (author / category / license / optional homepage), upload.
4. Watch the live verification report. Green → the review queue; red → fix and
   resubmit, nothing is stored.
5. After an admin approves, the pack is live in the catalog and index.

Rules the machine enforces: `manifest.name` must be kebab-case and not collide
with docwright's built-in packs; the directory name must equal the pack name;
content changes must bump `manifest.version` (strictly increasing); one
submission per pack in the queue at a time.

## Run your own instance

Prereqs: node ≥ 18 (verifier + installer), podman + the `rust-dev` image from
[devcontainer](https://github.com/sharework-ai) toolchain repo, a Supabase
instance (the `commons/supabase` assets assume the local k3s).

```sh
# 1. Supabase (once): ~/projects/commons/supabase/setup.sh install
# 2. Schema: setup.sh apply-migration supabase/migrations/0001_init.sql
# 3. Build the service:
podman run --rm --userns=keep-id -v "$PWD":/workspace -w /workspace/server-rs \
  -v ~/devcache/cargo/registry:/home/app/.cargo/registry \
  localhost/rust-dev:1.2 cargo build --release
# 4. Everything up (pg port-forward + verifier + service):
scripts/dev-run.sh
# 5. Promote yourself to reviewer, from the Studio SQL editor or psql:
#    insert into profiles (id, is_admin, display_name)
#    values ('<your-auth-users-id>', true, '<name>')
#    on conflict (id) do update set is_admin = true;
```

### Environment (server-rs, all required fail loud at boot)

| var | meaning |
|---|---|
| `PORT` | listen port (default 8398, loopback) |
| `SUPABASE_API_URL` | envoy gateway, e.g. `http://supabase.lan` |
| `SUPABASE_SECRET_KEY` | server-side credential — use the legacy service_role JWT (`supabase-jwt/service-key`); Storage rejects `sb_secret_` keys |
| `SUPABASE_PUBLISHABLE_KEY` | exposed to the browser via `/config.json` |
| `DATABASE_URL` | Postgres DSN (cluster-internal or port-forwarded) |
| `VERIFIER_URL` | sidecar base (default `http://127.0.0.1:8399`) |
| `WEB_DIR` / `DATA_DIR` | static pages / staging work dir |

Verifier env: `DOCWRIGHT_ROOT` (required; docwright checkout with `packs/` and
a working `pack:check`), `VERIFIER_PORT`, `VERIFIER_TMP`, `VERIFIER_MAX_BYTES`.

## Verification

```sh
scripts/test.sh          # both layers:
                         #   verifier: vitest + v8 coverage gates over
                         #             verifier/index.mjs (85% lines/funcs/
                         #             stmts, 75% branches — HTTP endpoints,
                         #             shape logic, archive safety)
                         #   server-rs: cargo test in the rust-dev container
                         #             (semver, bearer parsing, sha256)
scripts/test.sh --cov    # + Rust coverage summary (cargo-llvm-cov; needs
                         #             rust-dev:1.3+)
scripts/e2e.sh           # full chain: submit → verify → review → publish →
                         # install → docwright pack:check on the installed
                         # copy (integration; no unit-test substitute)
```

npm devDependencies (vitest) are dev-only — runtime stays zero-dependency
(verifier, installer, and web pages pull no packages).

## Roadmap (deferred on purpose)

Public deployment (t5s k3s + a real domain), a submit CLI over the same API,
zip uploads, RLS-backed direct reads, static API scans for `functions.js`,
long-term tarball versioning. Pack authoring lives in docwright itself —
domain-neutral chain packs belong in the docwright repo, not the market.
