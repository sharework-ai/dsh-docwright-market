# dsh-docwright-market

[docwright](https://github.com/sharework-ai/dsh-docwright)(@sharework-ai/dsh-docwright)
pack 的投稿式市场:作者从浏览器上传 `.tgz`,机器校验直接跑 docwright 自己的
`pack:check`(golden 全绿才进审核队列),管理员人工审核通过后上架,消费端用
零依赖脚本装进 `packsDir`。投稿不需要会 git/GitHub,注册只要一个邮箱。

English: [README.md](README.md)

## 架构

```
浏览器(浏览/投稿/审核三页)
   │  JWT(Supabase Auth)
   ▼
市场主服务(Rust,axum)──POST /verify──▶ 校验 sidecar(node)
   │  sqlx              │ REST                │ spawn docwright pack:check
   ▼                    ▼                     ▼(loader IS the linter)
Postgres          Storage(uploads/         解包(防穿越)→ 市场形状检查
(packs/submissions/ tarballs 桶)           → golden 全绿(真实 loader)
 profiles)
```

- **Rust 主服务**(`server-rs/`):只做产品流程——认证、目录、审核队列、
  tarball 托管、静态页。**不碰 pack 语义**。
- **校验 sidecar**(`verifier/`):唯一校验面。解包(防路径穿越)、市场级
  形状规则(kebab-case、不撞内置名、目录名==manifest.name),然后跑
  docwright 自己的 `pack:check`(以内置 packs 为上游根)。报告里的
  `checkGreen` 是 docwright 的裁决,不是重新实现。
- **Supabase**(自托管,部署资产见 `~/projects/commons/supabase`):
  Auth(邮箱注册)、Postgres(目录+审核队列)、Storage(原始上传留档 +
  tarball 公开下载)。

## 安全模型

1. 机器门在前:只有 `pack:check` 全绿的投稿才进人工队列。
2. `rules/functions.js` 是可执行代码,会在每个用户宿主装载 pack 时执行——
   人工审核是它唯一的门;审核页展示文件树与体检单,站点与 index 打披露标
   (`hasFunctionRules`)。
3. 安装只复制字节(对照 index 的 sha256 校验完整性),安装器永不执行 pack 内容。
4. 更新必须来自原作者账号、版本严格递增、内容哈希必须变化;同名待审期间
   拒绝重复投稿。

## 安装 pack(消费端)

```sh
node install.mjs task-card                     # 最新版,装进 ./.docwright/packs/
node install.mjs task-card --into /path/to/packs
node install.mjs --list                        # 看市场里有什么
```

装完重启 dsh 宿主,用 `doc_packs` 确认可见。pack 若消费其他市场 pack 会提示
一并安装;内置上游(cosmic-*)自动解析。

## 投稿(作者)

1. 按 docwright 随包的 authoring 指南(`guides/pack-authoring.md`)做好 pack,
   本地跑绿:在 docwright 仓库内 `pnpm run pack:check -- <你的pack目录> --with packs`。
2. 打包:`tar czf your-pack.tgz <pack目录>`(manifest.json 在该目录根部;
   压缩包根即该目录,或其唯一子目录均可)。
3. 打开市场 `/submit.html` 注册并登录,填表(作者/分类/许可证/可选主页),
   上传压缩包。
4. 看实时校验报告:绿 → 进审核队列;红 → 按报告修复后重投(不留任何记录)。
5. 管理员通过后,pack 即上架(目录 + index 即刻可见)。

机器强制规则:`manifest.name` 必须 kebab-case 且不与 docwright 内置 pack 同名
(同名会覆盖内置,市场拒绝);目录名必须等于 pack 名;内容变更必须升
`manifest.version`(严格递增);同名投稿在待审期间只能有一个。

## 自建实例

前提:node ≥ 18(verifier + 安装器)、podman + devcontainer 工具链仓库的
`rust-dev` 镜像、一个 Supabase 实例(`commons/supabase` 资产默认本机 k3s)。

```sh
# 1. Supabase(一次): ~/projects/commons/supabase/setup.sh install
# 2. Schema: setup.sh apply-migration supabase/migrations/0001_init.sql
# 3. 构建服务:
podman run --rm --userns=keep-id -v "$PWD":/workspace -w /workspace/server-rs \
  -v ~/devcache/cargo/registry:/home/app/.cargo/registry \
  localhost/rust-dev:1.2 cargo build --release
# 4. 一键起(pg port-forward + verifier + 服务):
scripts/dev-run.sh
# 5. 把自己提成审核员(Studio SQL 或 psql):
#    insert into profiles (id, is_admin, display_name)
#    values ('<你的auth.users id>', true, '<名字>')
#    on conflict (id) do update set is_admin = true;
```

### 环境变量(server-rs,缺失即启动失败)

| 变量 | 含义 |
|---|---|
| `PORT` | 监听端口(默认 8398,仅 loopback) |
| `SUPABASE_API_URL` | envoy 网关,如 `http://supabase.lan` |
| `SUPABASE_SECRET_KEY` | 服务端凭据——用旧版 service_role JWT(`supabase-jwt/service-key`);Storage 不认 `sb_secret_` key |
| `SUPABASE_PUBLISHABLE_KEY` | 经 `/config.json` 下发给浏览器 |
| `DATABASE_URL` | Postgres 连接串(集群内或 port-forward) |
| `VERIFIER_URL` | sidecar 地址(默认 `http://127.0.0.1:8399`) |
| `WEB_DIR` / `DATA_DIR` | 静态页 / 暂存工作目录 |

Verifier 环境变量:`DOCWRIGHT_ROOT`(必填,docwright checkout,需含 `packs/`
且 `pack:check` 可跑)、`VERIFIER_PORT`、`VERIFIER_TMP`、`VERIFIER_MAX_BYTES`。

## 验证

```sh
scripts/test.sh          # 两层一起跑:
                         #   verifier:vitest + v8 覆盖率门
                         #             (verifier/index.mjs 行/函数/语句 85%、
                         #             分支 75%——HTTP 端点、形状逻辑、解包安全)
                         #   server-rs:rust-dev 容器内 cargo test;DB 用例
                         #             (sqlx::test)在本机 supabase postgres 上
                         #             建独立 scratch 库并应用真实 migrations
scripts/test.sh --cov    # 追加 Rust 覆盖率摘要(cargo-llvm-cov,需
                         #             rust-dev:1.3+)
scripts/e2e/run.sh       # 场景套件(见下方 "E2E 场景" 一节)
```

单元层目前钉住的行为:

- **verifier**(约 92% 行):端点行为、市场形状规则、解包安全、内容哈希。
- **install.mjs**(7 个 CLI 用例):本地假 market 钉住退出码(0/2/3)、
  落盘文件与全部完整性拒绝(tarball sha256、内容哈希、manifest 名、
  未带 --force 的重装)。
- **server-rs**(约 80% 行):上述全部,外加 wiremock 全 handler 集成
  测试(mock Supabase 的 Auth /user 降级与 Storage 上传/下载/ensure,
  以及 verifier sidecar)——投稿快乐路径、机器拒绝不落库、approve 发布
  链端到端(mock 下载 → 目录 upsert → 审核打戳 → tarball 重定向)、
  投稿的属主/陌生人视角、仅管理员队列、拒绝附备注、verifier 之前的
  表单字段校验。剩余缺口是启动装配(config/main,由 e2e 启动覆盖)。

npm devDependencies(vitest)仅开发期使用——运行时保持零依赖(verifier、
安装器、web 页面不拉任何包)。

## 部署到本机 k3s(无需常驻脚本)

Rust 主服务为镜像;verifier sidecar 以 hostPath 挂载宿主的
docwright + deepseek-harness checkout(单节点 k3s 的本机部署语义:模块
解析与宿主 pack:check 环境 1:1,必然一致)。集群内直连 supabase 各服务,
彻底告别 port-forward:

```sh
deploy/build.sh               # 构建 + 导入 localhost/market-{server,verifier}:<tag>
deploy/deploy.sh --tag <tag>  # ns + secret + deployments + IngressRoute
```

部署后的入口(本机 hosts 条目 `192.168.2.1 dm.shrwk.com` 经 k3s traefik 走
通配符证书 TLS;frpc 的 `dm-web-macus` 代理把同一路由暴露到公网):

- 浏览/投稿:`https://dm.shrwk.com`
- API:`https://dm.shrwk.com/api/index`

局域网 hosts 路由与公网 frpc 路由终止在同一个 IngressRoute 上,不会漂移。

## E2E 场景

场景在 `scripts/e2e/scenarios/*.sh`,一个主题一个文件,彼此隔离(目录重置 +
独立账号)。运行器聚合结果:

```sh
scripts/e2e/run.sh                # 全部场景,打已部署的集群
scripts/e2e/run.sh --only happy-chain,rules
scripts/e2e/run.sh --list         # 场景名列表
scripts/e2e/run.sh --local        # 宿主进程模式(集群部署前)
```

现有场景:`happy-chain`(投稿 → 校验 → 审核 → 发布 → 目录/索引一致)、
`status-polling`(作者可见状态)、`rules`(401 / 400 / 403 队列 / 403 冒名 /
409 版本 / 409 重复投稿)、`builtin-name-collision`(形状门拒绝,零落库)、
`consumer-install`(从真实市场安装 → 装出副本过 pack:check → --force 重装)。

## 路线图(有意延后)

公网部署(t5s k3s + 域名)、同 API 的投稿 CLI、zip 上传、RLS 直读、
functions.js 静态 API 扫描、tarball 长期版本归档。pack 创作本身属于
docwright:领域中性的链 pack 进 docwright 仓库,市场只收部署侧 pack。
