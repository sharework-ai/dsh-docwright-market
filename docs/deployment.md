# 部署与运维

市场跑在本机 k3s 的 `market` 命名空间;依赖同一 k3s 上的 Supabase 栈
(部署资产:`~/projects/commons/supabase`,helm + operator 0.8.3)。
单节点部署语义:verifier 以 hostPath 挂宿主的 docwright/harness checkout,
模块解析与宿主环境 1:1。

## 入口与域名

| 域名 | 内容 | 路由 |
|---|---|---|
| `https://dm.shrwk.com` | 市场全站(API + 页面 + tarball) | traefik IngressRoute → svc/market:8398 |
| `https://sb.shrwk.com` | Supabase Auth/Storage(浏览器面) | traefik IngressRoute → svc/supabase-envoy:8000 |

两条路由共用同一张 `shrwk-tls` 通配证书与同一 traefik websecure 入口;
局域网走 hosts(`192.168.2.1 dm.shrwk.com` / `sb.shrwk.com`),公网走
aliyun DNS → t2 frps(SNI 分流)→ frpc(`dm-web-macus` / `sb-web-macus`
指向 192.168.2.1:443)。**新增子域名时:aliyun 加一条 A 记录指向
119.91.111.213 + conf.d 加一个 https proxy + traefik 路由,三件套。**

## 部署 / 升级

```sh
deploy/build.sh                 # 组装 context + podman build 两镜像 + 导入 k3s
deploy/deploy.sh --tag <tag>    # ns/secret(幂等)+ apply + rollout 等待
```

- server 镜像:rust-dev 多阶段编译,base 承载二进制与 web 页。
- verifier 镜像:**无构建**——hostPath 挂宿主的 docwright + deepseek-harness
  checkout(与 pack:check 实际运行环境 1:1),镜像只提供 node。
- server 二进制/web 变更 → 重跑 build.sh;verifier 脚本变更(hostPath)→
  `kubectl -n market rollout restart deploy/market-verifier` 即生效。

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
| `WEB_DIR` / `DATA_DIR` | 静态页 / 暂存目录 |

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
- 目录重置(测试):`truncate table submissions, packs;`

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
