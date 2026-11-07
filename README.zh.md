# dsh-docwright-market

[docwright](https://github.com/sharework-ai/dsh-docwright)(@sharework-ai/dsh-docwright)
pack 的投稿式市场:作者从浏览器上传 `.tgz`,机器校验直接跑 docwright 自己的
`pack:check`(golden 全绿才进审核队列),管理员人工审核通过后上架,消费端用
零依赖脚本装进 `packsDir`。投稿不需要会 git/GitHub,注册只要一个邮箱。

English: [README.md](README.md)。功能/技术/测试的方案文档见 [docs/](docs/)。

## 快速开始

```sh
# 安装 pack(消费端)
node install.mjs task-card                # 装进 ./.docwright/packs/,重启宿主生效

# 投稿(作者)
打开 https://dm.shrwk.com/submit.html     # 上传 .tgz,实时看校验报告

# 审核(管理员)
打开 https://dm.shrwk.com/admin.html
```

## 方案文档

| 文档 | 内容 |
|---|---|
| [docs/features.md](docs/features.md) | 功能与业务规则:角色、投稿流水线、机器强制规则、人工审核的安全边界、账号权限 |
| [docs/architecture.md](docs/architecture.md) | 技术方案:组件与硬边界、关键设计决策(Rust+sidecar 分层、地址拆分、两级门、hostPath)、数据模型、API 面 |
| [docs/deployment.md](docs/deployment.md) | 部署与运维:k3s 部署步骤、入口域名与 DNS 三件套、服务端配置表、Supabase 须知与 operator 现状、日常运维速查 |
| [docs/testing.md](docs/testing.md) | 测试方案:三层体系(单元+覆盖门 / wiremock 集成 / e2e 场景)、各层钉什么、跑法、新增用例约定 |

## 自建实例

前提:node ≥ 18(verifier + 安装器)、podman + devcontainer 工具链仓库的
`rust-dev` 镜像、一个 Supabase 实例(`commons/supabase` 资产默认本机 k3s)。

```sh
# 1. Supabase(一次): ~/projects/commons/supabase/setup.sh install
# 2. Schema: setup.sh apply-migration supabase/migrations/0001_init.sql
# 3. 构建 + 部署到本机 k3s(细节见 docs/deployment.md):
deploy/build.sh && deploy/deploy.sh --tag "$(date +%H%M)"
# 4. 把自己提成审核员(Studio SQL 或 psql):
#    insert into profiles (id, is_admin, display_name)
#    values ('<你的auth.users id>', true, '<名字>')
#    on conflict (id) do update set is_admin = true;
```

集群部署前的轻量开发模式:`scripts/e2e/run.sh --local`,或 `scripts/dev-run.sh`。

## 测试

```sh
scripts/test.sh          # 单元 + 覆盖率门(两层)
scripts/e2e/run.sh       # 场景套件,打已部署的集群
```

npm devDependencies(vitest)仅开发期使用——运行时保持零依赖(verifier、
安装器、web 页面不拉任何包)。

## 路线图(有意延后)

公网加固(当前仅 frpc 路由)、同 API 的投稿 CLI、zip 上传、RLS 直读、
functions.js 静态 API 扫描、tarball 长期版本归档。pack 创作本身属于
docwright:领域中性的链 pack 进 docwright 仓库,市场只收部署侧 pack。
