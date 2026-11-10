# 部署与运维

市场跑在本机 k3s 的 `market` 命名空间;依赖同一 k3s 上的 Supabase 栈
(部署资产:`~/projects/commons/supabase`,helm + operator 0.8.3)。
单节点部署语义:verifier 以 hostPath 挂宿主的 docwright/harness checkout,
模块解析与宿主环境 1:1。

## 入口与域名

| 域名 | 内容 | 路由 |
|---|---|---|
| `https://dm.shrwk.com` | 市场全站(API + 页面 + tarball) | traefik IngressRoute → svc/market:8398 |
| `https://dm-dev.shrwk.com` | **dev 市场**(独立 schema + 桶,资源轻量) | traefik IngressRoute → svc/market.market-dev:8398 |
| `https://sb.shrwk.com` | Supabase Auth/Storage(浏览器面) | traefik IngressRoute → svc/supabase-envoy:8000 |

两条路由共用同一张 `shrwk-tls` 通配证书与同一 traefik websecure 入口;
局域网走 hosts(`192.168.2.1 dm.shrwk.com` / `sb.shrwk.com`),公网走
aliyun DNS → t2 frps(SNI 分流,按 frpc 声明的域名自动路由,**t2 侧零配置**)
→ frpc(`dm-web-macus` / `dm-dev-web-macus` / `sb-web-macus`
指向 192.168.2.1:443)。**新增子域名时:aliyun 加一条 A 记录指向
119.91.111.213(*.shrwk.com 通配 A 已覆盖)+ macus frpc conf.d 加一个
https proxy + traefik 路由,三件套。**

## dev 市场(dm-dev.shrwk.com)

第二套轻量市场(2026-11-10 上线):ns `market-dev`,数据库同库**独立
schema `market_dev`**(登录角色 `market_dev` 的 role-level search_path,
服务端 sqlx 无限定名查询零改动落对地方;DML 授权 + default privileges
见 deploy-dev.sh),存储**独立桶** `uploads-dev`/`tarballs-dev`(环境变量
`UPLOADS_BUCKET`/`TARBALLS_BUCKET`,生产不设则用默认 uploads/tarballs),
Auth **共享** GoTrue(sb.shrwk.com 同一登录);profiles 按 schema 隔离,
deploy-dev.sh 每次从 public.profiles 同步管理员。资源:server limit
100Mi + verifier limit 256Mi(仅 pack:check 突发),稳态合计 <100Mi。

```sh
deploy/build.sh --tag dev-$(date +%m%d-%H%M)   # 镜像与生产同一份(dev 只差 env)
deploy/deploy-dev.sh --tag <tag>               # ns/secret/schema/迁移(幂等)+ apply + rollout
kubectl -n market-dev logs deploy/market-server --tail=50
```

迁移必须**幂等**(0002 的 ADD CONSTRAINT 已按名守护;重跑 deploy-dev.sh
安全)。0000_auth_stub 仅限裸 Postgres 测试库,真实 supabase 上跳过
(auth schema 属 supabase_admin,postgres 角色无权建表)。

## 部署 / 升级

```sh
# 1. schema 迁移先行(新 server + 旧库才是不行的组合;旧 server + 新库无碍)
~/projects/commons/supabase/setup.sh apply-migration supabase/migrations/0002_org_versions.sql
# 2. 镜像 + 部署
deploy/build.sh                 # 组装 context + podman build 镜像 + 导入 k3s
deploy/deploy.sh --tag <tag>    # ns/secret(幂等)+ apply + rollout 等待
```

- server 镜像:rust-dev 多阶段编译,base 承载二进制与 web 页。
- verifier 镜像:**无构建**——hostPath 挂宿主的 docwright + deepseek-harness
  checkout(与 pack:check 实际运行环境 1:1),镜像只提供 node。
- server 二进制/web 变更 → 重跑 build.sh;verifier 脚本变更(hostPath)→
  `kubectl -n market rollout restart deploy/market-verifier` 即生效。
- 迁移必须**纯 SQL**(sqlx::test 在无 storage schema 的 scratch 库上跑全
  目录迁移);桶供给在 provision/storage-buckets.sql 与服务端 ensure_buckets。
- 组织保留名是代码常量(`ORG_RESERVED`),**零新增环境变量**——podman save →
  ctr import 丢镜像 ENV,新增配置一律走 deploy.sh secret + k8s.yaml 显式注入。

## 线上数据清理(一次性,e2e 残留)

```sh
scripts/ops/cleanup-e2e-residue.sh             # dry run:列出将删的账号与数据
scripts/ops/cleanup-e2e-residue.sh --apply     # 真删(默认保护 peng_long@hisuntech.com)
```

选择集 = email 匹配 `@example.invalid` / `@x.invalid` 的 auth.users(e2e 残留
账号多为 admin 标,故 is_admin **不**作为保护条件);删除顺序:Storage 对象 →
pack_versions → packs → submissions → profiles → GoTrue admin delete(children
先于 auth.users:`packs.author_id` 与 `submissions.uploader` 无级联)。清理后
由真实账号在 `/account.html` 设组织,再经 `/submit.html` 重新投稿 task-card。

## 服务端配置(全环境变量,缺失即启动失败)

| 变量 | 值(集群部署) |
|---|---|
| `SUPABASE_API_URL` | `http://supabase-envoy.supabase.svc:8000`(集群内) |
| `PUBLIC_SUPABASE_URL` | `https://sb.shrwk.com`(浏览器面,经 /config.json 下发) |
| `SUPABASE_SECRET_KEY` | supabase-jwt/service-key(**旧 JWT 形态**,Storage 不认 sb_secret_) |
| `SUPABASE_PUBLISHABLE_KEY` | 经 /config.json 下发浏览器 |
| `DATABASE_URL` | `postgres://postgres:…@supabase-postgres.supabase.svc:5432/postgres` |
| `VERIFIER_URL` | `http://verifier.market.svc:8399` |
| `LISTEN_HOST` / `PORT` | `0.0.0.0` / `8398`(loopback 默认,宿主直跑时) |
| `WEB_DIR` | 静态页目录 |

verifier 环境变量:`DOCWRIGHT_ROOT`(必填)、`VERIFIER_PORT`、
`VERIFIER_HOST`(容器内 0.0.0.0)、`VERIFIER_TMP`、`VERIFIER_MAX_BYTES`。

注意:podman save → ctr import 的链路**会丢镜像层 ENV**,凡容器必需的
变量一律在 k8s.yaml 显式注入(现状即如此),不要依赖镜像 ENV。

## Supabase 侧须知

- auth 由 `Auth CR` + operator 管理,但 **0.8.3 operator 的 reconcile 在
  首装后失效**(改 CR env、删 deployment 均不重建;helm upgrade 不更新
  deployment env)。`supabase-auth` deployment 目前为**手工维护**
  (gotrue v2.197.0,DATABASE_URL 走 postgres 超级用户 + search_path=auth,
  GOTRUE_API_PORT=9999 对齐 svc targetPort,labels 三件套对齐 svc selector)。
  改 auth 配置 = 直接改该 deployment;升级 operator 前先备份其 spec。
- 账号注册即确认(AUTOCONFIRM,无 SMTP);管理员 = profiles.is_admin=true。
  密码修改 = 浏览器侧 `PUT /auth/v1/user`(supabase-js updateUser),无需改
  auth 配置。
- e2e 的目录重置是**残留范围删除**(按测试邮箱域),绝不能 truncate——线上
  目录属于真实账号;e2e 结束时 teardown 删除本次创建的账号与其数据。

## 日常运维速查

```sh
kubectl -n market get pods                        # 服务状态
curl -sk https://dm.shrwk.com/api/index           # 存活 + 目录
curl -sk https://dm.shrwk.com/config.json         # 浏览器引导配置
~/projects/commons/supabase/setup.sh status       # supabase 栈状态
kubectl -n market logs deploy/market-server --tail=50
```

数据备份:postgres pg_dump(supabase 栈 README 有命令);Storage 两桶为
投稿归档与 tarball,可从 packs 表 + uploads 桶重建 tarballs 桶内容。
