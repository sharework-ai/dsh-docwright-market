# 测试方案

三层测试体系,各层钉住不同类别的行为;外加消费端冒烟。所有入口都收在
`scripts/test.sh`(单测/覆盖率)与 `scripts/e2e/run.sh`(场景套件)。

## 层次总览

| 层 | 工具 | 覆盖对象 | 现状 |
|---|---|---|---|
| 单元 + 覆盖率门(node) | vitest + v8 coverage | verifier/index.mjs(校验面):HTTP 端点、形状规则、解包安全、内容哈希 | 23 用例,92.3% 行(门:行/函数/语句 85%、分支 75%) |
| 安装器(node) | vitest(spawn CLI + 本地假 market) | install.mjs:退出码 0/2/3、落盘文件、全部完整性拒绝 | 7 用例 |
| 单元(Rust) | cargo test(rust-dev 容器) | semver(含 pre-release)、bearer 解析、sha256 向量 | 3 用例 |
| 集成(Rust) | sqlx::test + wiremock | 市场规则全集、发布 upsert、审核迁移、读侧 handler、认证拒绝路径 | 15 用例,Rust 合计 ~80% 行 |
| 端到端 | scripts/e2e/run.sh | 打已部署集群的 5 个场景 | 全绿 |

## 各层钉什么

### verifier 单元(vitest)

端点行为(200/400 语义、大小上限、穿越拒绝)、市场形状规则(命名、撞内置名、
目录名一致)、内容哈希稳定性。假 docwright fixture 让 HTTP 层可控;
真实 docwright 往返由 e2e 兜底。

### 安装器(vitest)

本地假 market(index + tarball server)spawn CLI,钉住:成功安装落盘、
sha256/内容哈希/名字不匹配的拒绝(退出码 2)、重装冲突(退出码 3)与
--force、--list。**注意:测试内用异步 spawn——spawnSync 会阻塞事件循环,
冻结进程内假 server 造成死锁**(踩过)。

### Rust(sqlx::test + wiremock)

- `sqlx::test` 对每个用例建独立 scratch 库并应用真实 migrations
  (`supabase/migrations/`,0000 为 auth schema stub——真库上 no-op)。
- wiremock 顶替外部世界:一个 server 扮 Supabase(Auth /user 身份回退 +
  Storage 上传/下载/ensure),另一个扮 verifier。
- 覆盖:市场规则全集(队列去重/版本唯一/所有权/递增/内容变更)、发布
  upsert(首发 + 更新保留 added)、审核迁移、读侧 handler 直调、401 认证
  拒绝(真实路由,双验签失败路径)。
- 未覆盖部分是启动装配与真实 Storage 往返——由 e2e 兜底。

### e2e 场景套件(scripts/e2e/)

场景文件化(`scenarios/*.sh`),每场景目录重置 + 独立账号,顺序无关、
可 `--only` 单跑;失败不中断其他场景,运行器聚合 PASS/FAIL 并以非零
退出码收尾。默认打已部署集群(`https://dm.shrwk.com`),`--local` 降级
宿主进程。

| 场景 | 钉住 |
|---|---|
| happy-chain | 投稿 → 校验绿 → 审核 → 发布 → 目录/索引/详情一致 |
| status-polling | 作者可见状态流转(pending → approved)与报告 |
| rules | R1 401 未认证 / R2 400 非法分类 / R3 403 非管理员队列 / R4 403 冒名更新(verifier 绿的升版包,拒绝落在所有权规则上)/ R5+R6 409 版本占用与重复投稿 |
| builtin-name-collision | 撞内置名在形状门被拒,零落库 |
| consumer-install | install.mjs 从真实市场装出 → 装出副本再过 docwright pack:check → 重装冲突与 --force |

## 跑法

```sh
scripts/test.sh                 # verifier 覆盖率门 + Rust 全部单测/集成
scripts/test.sh --cov           # 追加 Rust 覆盖率摘要(llvm-cov)
scripts/e2e/run.sh              # 场景套件(默认集群模式)
scripts/e2e/run.sh --local      # 宿主进程模式(集群部署前)
```

Rust 集成测试需要可达的 Postgres:test.sh 自动 port-forward 本机 supabase
PG 并以 `--network host` 传给测试容器。

## 新增场景/用例的约定

- 单测:行为命名(test 名 = 断言的行为),不动共享 fixture;
  Rust 测试放 `src/api/tests.rs`(纯逻辑)或 `wiremock_tests.rs`(带 mock)。
- e2e 场景:新文件进 `scenarios/`,文件尾 `scenario <name> <fn>` 注册;
  场景内不得依赖其他场景的残留状态(需要已上架 pack 就自己发布)。
- 覆盖率门只升不降;e2e 新增负路径时优先"拒绝落在哪条规则上"的精确断言,
  而非只断状态码。
