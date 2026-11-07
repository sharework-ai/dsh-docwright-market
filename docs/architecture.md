# 技术方案与设计决策

## 组件与边界

```
浏览器(浏览/投稿/审核页,JWT)
   → 市场主服务(Rust,axum)──POST /verify──▶ 校验 sidecar(node)
        │  sqlx              │ Storage REST             │ spawn docwright pack:check
        ▼                    ▼                          ▼
   Supabase Postgres    Storage 桶(uploads/tarballs)  docwright 自己的 loader
```

三条硬边界:

1. **Rust 主服务只做产品流程**——HTTP、JWT 验签、目录读写、审核流、tarball
   托管、静态页。它不理解 pack 的任何语义。
2. **校验能力只有一个面**:docwright 的 `pack:check`("the loader IS the
   linter")。sidecar 只是进程包装(解包、形状检查、spawn),绝不重写规则;
   报告里的 `checkGreen` 是 docwright 的裁决。
3. **安装器只复制字节**,完整性靠 index 的 sha256 + 内容哈希双重比对。

## 关键设计决策

### Rust + node sidecar 双进程

校验必须是 docwright 生态(node/tsx)的代码;产品服务选了 Rust(axum,学习
目标)。二者以 HTTP(`/verify`,body 为原始 tgz)为界,部署上互不牵连——
verifier 换部署形态不影响 API。

### Supabase 承担三块基础设施

- **Postgres**:三张表(packs/submissions/profiles),迁移 SQL 入仓
  (`supabase/migrations/`),服务以 service_role 级凭据直连(sqlx);
  RLS 有意未启用(anon key 不下发)。
- **Auth(GoTrue)**:邮箱注册(AUTOCONFIRM,自托管无 SMTP)、JWT 签发;
  服务端验签优先 JWKS,不可用时降级 GoTrue `/auth/v1/user`(适配任意
  自托管构建)。作者身份即账号,更新/冒名拦截天然成立,无需自制编辑令牌。
- **Storage**:`uploads`(投稿原始归档,审计凭据)+ `tarballs`(公开读,
  安装下载)。**tarball 下载由 Rust 服务代理**而非 302——Storage 端点是
  集群内地址,外部客户端无法跟随重定向。

### 地址拆分:服务端集群内 / 浏览器公网

`SUPABASE_API_URL`(集群内 svc)与 `PUBLIC_SUPABASE_URL`(公网域名,
经 `/config.json` 下发浏览器)是两个配置。tarball 走服务端代理同理。
**规则:任何集群内名字绝不出现在面向浏览器的响应里。**

### 校验的"机器绿 + 人工审"两级门

机器门(`pack:check` 全绿)先挡住全部可自动判定的坏投稿,人工只看
机器无法判定的部分:描述与行为一致性、prompt 质量、`functions.js` 代码。
审核队列里只有绿包,审核成本被压到"读代码"这一件事上。

### verifier 的 hostPath 部署

单节点 k3s 上,verifier 以 hostPath 挂载宿主的 docwright + deepseek-harness
checkout(而非在镜像内构建 harness):模块解析与宿主 `pack:check` 环境 1:1,
不会出现"镜像内缺一个深层依赖"的失效模式。代价是部署依赖宿主目录存在——
这是单节点部署的明确语义,写入部署文档。

### 完整性与原子性

- 投稿:原始 tgz 先入 Storage `uploads` 桶留档,再进队列;审核通过发布的
  tarball 就是从该归档取出的字节(审核对象与发布对象逐字节一致)。
- 安装:下载后校验 tarballSha256,解包后复核 manifest.name 与目录内容哈希,
  临时目录落盘后 rename 原子就位。

## 数据模型

```
packs(name PK, version, description jsonb, category, author, author_id,
      license, homepage, consumes/languages jsonb, requires_topic,
      artifact_prefix, has_function_rules, content_hash,
      tarball_path, tarball_sha256, added, updated)

submissions(id PK, pack_name, version, uploader→auth.users,
            status pending|approved|rejected, report jsonb, archive_path,
            category/author/license/homepage, review_note,
            reviewed_by, reviewed_at, created_at, UNIQUE(pack_name, version))

profiles(id→auth.users PK, is_admin, display_name)
```

表结构即合同:审核通过的表单字段在 approve 时搬进 packs;验证报告
(checkOutput/manifest/contentHash)随 submission 留档,详情页的"体检单"
即由此渲染。

## API 面

| 端点 | 说明 |
|---|---|
| `POST /api/submissions` | 投稿(multipart:archive + 表单字段),返回 id 与校验报告 |
| `GET /api/submissions/{id}` | 作者/管理员查投稿状态与报告 |
| `GET /api/packs`、`/api/packs/{name}` | 目录列表 / 详情(含体检单) |
| `GET /api/index` | format:1 机器目录(install.mjs 与未来宿主消费) |
| `GET /tarballs/{name}-{version}.tgz` | 下载(仅目录内已知路径;server 代理) |
| `GET /api/admin/queue`、`POST /api/admin/review/{id}` | 审核队列 / 通过·拒绝 |
| `GET /config.json` | 浏览器引导:Supabase 公网地址 + publishable key |

索引 `format: 1` 是版本化契约:tarball 为相对路径(经服务端路由),
公网部署后历史数据不需要迁移。
