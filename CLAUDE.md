# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

docwright pack 市场:投稿式 pack 分发服务(浏览器上传 tgz → 机器校验 → 人工审核 → 发布 → 安装)。方案文档在 `docs/`(中文,按章节分篇);README 只是门面。代码/注释/commit 英文,方案文档中文。

## 常用命令

```sh
# 单测 + 覆盖率门(verifier vitest + Rust cargo test,含 sqlx::test 集成)
scripts/test.sh                    # 两层全跑
scripts/test.sh --cov              # 追加 Rust 覆盖率摘要(llvm-cov)
npx vitest run verifier/test/verifier.test.mjs   # 单个测试文件
cd server-rs && cargo test api::tests::rules        # 单个 Rust 用例(容器内)

# e2e 场景套件(默认打已部署集群 https://dm.shrwk.com)
scripts/e2e/run.sh                 # 全部场景
scripts/e2e/run.sh --only rules    # 子集
scripts/e2e/run.sh --local         # 宿主进程模式(集群部署前)

# 部署(本机 k3s)
deploy/build.sh                    # 组装 context + podman build 镜像 + ctr 导入
deploy/deploy.sh --tag <tag>       # ns/secret/apply + rollout 等待
```

**Rust 编译必须进容器**(本机无工具链,devcontainer 纪律):

```sh
podman run --rm --userns=keep-id -v "$PWD":/workspace -w /workspace/server-rs \
  -v ~/devcache/cargo/registry:/home/app/.cargo/registry \
  localhost/rust-dev:1.3 cargo build --release
```

前置:`~/projects/commons/supabase` 栈在跑;宿主 `~/projects/lab/deepseek-harness` 已构建(`build:lib:host`)。

## 架构:三条硬边界

1. **`server-rs/`(Rust,axum)只做产品流程**——HTTP、JWT 验签、目录读写、审核流、tarball 托管。它不理解 pack 的任何语义。
2. **`verifier/`(node sidecar)是唯一校验面**——解包(防穿越)、市场形状检查、spawn docwright 自己的 `pack:check`(runPackCheck 直接 `node --import tsx/esm`,**绕开 pnpm** 以免触发 packageManager 版本切换)。"loader IS the linter":绝不在此重写 docwright 规则。
3. **`install.mjs` 只复制字节**(sha256 + 内容哈希双重校验),永不执行 pack 内容。`rules/functions.js` 在用户宿主装载时执行——人工审核是它唯一的门,审核页/站点/index 都要披露。

## 部署形态(k3s market ns)

- **server**:镜像多阶段构建(rust-dev 编译 + base 承载二进制与 `web/`)。
- **verifier**:**hostPath 挂宿主** docwright + deepseek-harness(单节点语义,模块解析与宿主 pack:check 环境 1:1,必然一致);镜像只是 node22-dev。
- server 配置一律环境变量(缺即启动失败);**地址拆分**是硬约定:`SUPABASE_API_URL` 是集群内 svc(服务端用),`PUBLIC_SUPABASE_URL`(浏览器面,经 /config.json 下发)——**任何集群内名字绝不出现在面向浏览器的响应里**(tarball 下载因此由 server 代理而非 302)。

## 数据流与规则

投稿(要求账号已设发布者组织)→ verifier 校验(报告落库)→ 待审队列(只有绿包进)→ 管理员 approve(Storage 归档 → packs upsert + pack_versions 追加 + org 盖章)→ 消费端按 index(format:2,org + versions[],可钉版)安装。机器强制规则在 `server-rs/src/api/submissions.rs::check_submission_rules`:同名待审去重、(name,version) 唯一(**删除 pack 也不释放**——submissions 历史保留)、属主校验、版本严格递增、内容哈希必须变化。DB 四表 packs(最新版投影 + status/org)/pack_versions(版本历史)/submissions/profiles(org 全局唯一),migration 在 `supabase/migrations/`(0000 是 auth schema stub,真库上 no-op)。属主管理端点在 `api/owner.rs`(PATCH 元数据 / disable·enable / DELETE / my/submissions 撤回),组织端点在 `api/account.rs`;下架 pack 对公共读面不可见、tarball 410(`error.rs::gone`)。密码修改纯浏览器侧(GoTrue `PUT /auth/v1/user`),服务端不参与。

## 本仓库踩过的坑(复发时直接用)

- **Storage 不认 `sb_secret_` key**:服务端凭据用 `supabase-jwt/service-key`(旧 service_role JWT)。
- **podman save → k3s ctr import 丢镜像层 ENV**:容器必需变量一律在 k8s.yaml 显式注入,别依赖镜像 ENV。
- **不要把 harness/docwright 树打进构建 context**(前 verifier 镜像如此,已删):pnpm 的循环 symlink 遇 `cp -rL` 会指数级自拷贝,一夜吃掉 ~800G 磁盘并让构建假死;集群 verifier 用 hostPath 挂宿主目录,镜像只需 node(k8s.yaml 即事实)。
- **后台任务假死先查 `ps aux | grep cp`**:构建命令"超时转后台"或纯后台都可能仍在跑,`grep podman|cargo` 看不到 `cp -rL` 阶段;清理病态目录前必须先杀干净,否则 rm 与 cp 赛跑永不收敛。
- **测试里 spawnSync 会死锁进程内假 server**(子进程 fetch 等父进程事件循环)——install.test.mjs 用异步 spawn。
- **vitest 3.2.7 静默忽略 config 文件的 coverage 段**:覆盖率门走 package.json `test:coverage` 的 CLI 参数(见 vitest.config.mjs 注释)。
- **k3s 部署:podman 的镜像 store 与 k3s containerd 不互通**,build.sh 末尾 save + `k3s ctr images import`;kubelet 不认手工导入镜像的某些情况时用 `~/projects/commons/supabase/pre-pull.sh` 思路。
