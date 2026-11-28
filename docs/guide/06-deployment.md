# 部署与运维摘要

系统部署在本机 k3s 集群的 `market` 命名空间，依赖同一集群上的 Supabase 栈。另有第二套轻量的 dev 实例（`market-dev` 命名空间，独立 schema 与桶，共享 Supabase 登录），部署于 https://dm-dev.shrwk.com。

## 对外入口

| 域名 | 用途 |
|---|---|
| `https://dm.shrwk.com` | 生产市场（API、页面、tarball 下载） |
| `https://dm-dev.shrwk.com` | dev 市场 |
| `https://sb.shrwk.com` | Supabase 认证与存储的浏览器侧入口 |

三个域名共用一张通配证书与同一 traefik 入口。新增子域名需要三处配置：aliyun DNS 一条 A 记录（`*.shrwk.com` 通配已覆盖）、macus 的 frpc 配置加一个 https 代理、traefik 加一条路由。

## 日常操作速查

```sh
kubectl -n market get pods                        # 服务状态
curl -sk https://dm.shrwk.com/api/index           # 存活与目录
curl -sk https://dm.shrwk.com/config.json         # 浏览器引导配置
kubectl -n market logs deploy/market-server --tail=50

deploy/build.sh                                   # 构建镜像并导入 k3s
deploy/deploy.sh --tag <tag>                      # 部署生产
deploy/deploy-dev.sh --tag <tag>                  # 部署 dev
```

服务端配置全部来自环境变量，缺失即启动失败；由于镜像导入过程会丢失镜像层 ENV，容器必需的变量一律在 k8s 清单中显式注入。数据库迁移必须先于新版本服务端执行，且迁移 SQL 必须幂等。

## Supabase 注意事项

- 账号注册即确认（AUTOCONFIRM），不依赖 SMTP。
- 首任管理员通过 SQL 将 `profiles.is_admin` 置 true。
- 对象存储含两个桶：投稿归档与公开 tarball。备份以 postgres pg_dump 为主，tarball 桶可从数据库与归档桶重建。

## 完整文档

部署步骤、dev 实例、服务端配置表与数据清理的完整说明见 [deployment.md](../deployment.md)。
