# dsh-docwright-market

A submission-based marketplace for [docwright](https://github.com/sharework-ai/dsh-docwright)
packs: authors set a publisher org, upload a `.tgz` from the browser, machine
verification runs docwright's own `pack:check` (golden-green or nothing enters
the queue), a human approves (which appends the version history), and consumers
install the latest — or any pinned historical version — into their `packsDir`
with a zero-dependency script. Authors manage their own packs (metadata edits,
disable/enable, hard delete, submission withdraw) from the account page. No
git/GitHub knowledge required to submit; no accounts beyond an email signup.

中文文档:[README.zh.md](README.zh.md);设计与方案文档见 [docs/](docs/)(中文)。

## Quick start

```sh
# install a pack (consumer): latest, or pin an exact historical version
node install.mjs task-card                # into ./.docwright/packs/, then restart the dsh host
node install.mjs task-card@0.1.0          # pinned

# publish a pack (author): set your org once, then upload
open https://dm.shrwk.com/account.html    # password, org, your packs, your submissions
open https://dm.shrwk.com/submit.html     # upload a .tgz, watch the live report

# review submissions (admin)
open https://dm.shrwk.com/admin.html
```

## Documentation

| Document | Content |
|---|---|
| [docs/features.md](docs/features.md) | 功能与业务规则:角色、投稿流水线、机器强制规则、人工审核的安全边界、账号权限 |
| [docs/architecture.md](docs/architecture.md) | 技术方案:组件与硬边界、关键设计决策(Rust+sidecar 分层、地址拆分、两级门、hostPath)、数据模型、API 面 |
| [docs/deployment.md](docs/deployment.md) | 部署与运维:k3s 部署步骤、入口域名与 DNS 三件套、服务端配置表、Supabase 须知与 operator 现状、日常运维速查 |
| [docs/testing.md](docs/testing.md) | 测试方案:三层体系(单元+覆盖门 / wiremock 集成 / e2e 场景)、各层钉什么、跑法、新增用例约定 |

## Run your own instance

Prereqs: node ≥ 18 (verifier + installer), podman + the `rust-dev` image from
the devcontainer toolchain repo, a Supabase instance (the `commons/supabase`
assets assume the local k3s).

```sh
# 1. Supabase (once): ~/projects/commons/supabase/setup.sh install
# 2. Schema: setup.sh apply-migration supabase/migrations/0001_init.sql
# 3. Build + deploy to the local k3s (see docs/deployment.md for details):
deploy/build.sh && deploy/deploy.sh --tag "$(date +%H%M)"
# 4. Promote yourself to reviewer (Studio SQL or psql):
#    insert into profiles (id, is_admin, display_name)
#    values ('<your-auth-users-id>', true, '<name>')
#    on conflict (id) do update set is_admin = true;
```

A lighter host-process mode exists for development before a cluster deploy:
`scripts/e2e/run.sh --local`, or `scripts/dev-run.sh`.

## Tests

```sh
scripts/test.sh          # unit + coverage gates (both layers)
scripts/e2e/run.sh       # scenario suite against the deployed cluster
```

npm devDependencies (vitest) are dev-only — runtime stays zero-dependency.

## Roadmap (deferred on purpose)

Public hardening beyond the current frpc route, a submit CLI over the same
API, zip uploads, RLS-backed direct reads, static API scans for
`functions.js`. Pack authoring lives in docwright itself — domain-neutral
chain packs belong in the docwright repo, not the market.
