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
      tarball_path, tarball_sha256, org, status active|disabled,
      added, updated)                      -- "最新版"投影,读面由此渲染

pack_versions(name+version PK, name→packs on delete cascade, description,
      content_hash, tarball_path, tarball_sha256, has_function_rules,
      consumes/languages/requires_topic/artifact_prefix, submission_id,
      published_at)                        -- 每次通过发布追加一行,全历史可下载

submissions(id PK, pack_name, version, uploader→auth.users,
            status pending|approved|rejected, report jsonb, archive_path,
            category/author/license/homepage, review_note,
            reviewed_by, reviewed_at, created_at, UNIQUE(pack_name, version))

profiles(id→auth.users PK on delete cascade, is_admin, display_name,
         org, UNIQUE(org) where org is not null)
```

表结构即合同:审核通过的表单字段在 approve 时搬进 packs 并**同时追加
pack_versions**;`status` 只在插入时取默认值 `active`、绝不进 update 集——
这正是"下架期间发布新版本保持下架"的实现。删除 pack 时版本历史经外键级联
消失,但 submissions 历史保留,(name, version) 由此永久保留。发布者组织在
approve 时从投稿者当前 profile 盖章进 packs。

## API 面

| 端点 | 说明 |
|---|---|
| `POST /api/submissions` | 投稿(multipart:archive + 表单字段);要求账号已设组织(403) |
| `GET /api/submissions/{id}` | 作者/管理员查投稿状态与报告 |
| `GET /api/account` | 当前账号:email/org/显示名/管理员标 |
| `PUT /api/account/org` | 设置/修改发布者组织(单事务级联改名下全部 pack,返回 packsAffected) |
| `GET /api/my/packs`、`/api/my/packs/{name}` | 属主视角:自己的全部 pack(含下架) |
| `PATCH /api/my/packs/{name}` | 属主改元数据(category/license/homepage/author;homepage 显式 null 清空) |
| `POST /api/my/packs/{name}/disable`、`…/enable` | 下架 / 恢复(下架期间下载 410) |
| `DELETE /api/my/packs/{name}` | 硬删(目录 + 版本历史 + Storage 对象;submissions 保留) |
| `GET /api/my/submissions` | 自己的投稿历史;`DELETE /api/my/submissions/{id}` 撤回待审投稿 |
| `GET /api/packs`、`/api/packs/{name}` | 目录列表 / 详情(含体检单与版本历史);下架 pack 对外不可见 |
| `GET /api/index` | format:2 机器目录(org + versions[];顶层字段仍指最新版,format:1 读者兼容) |
| `GET /tarballs/{name}-{version}.tgz` | 下载(按 pack_versions.tarball_path 精确匹配;server 代理;下架 410) |
| `GET /api/admin/queue`、`POST /api/admin/review/{id}` | 审核队列 / 通过·拒绝 |
| `GET /config.json` | 浏览器引导:Supabase 公网地址 + publishable key |

索引 `format: 2` 是版本化契约:相对路径(经服务端路由)+ org + versions[];
顶层 per-pack 字段保持指向最新版,未钉版的安装行为与 format:1 完全一致。
密码修改不走服务端:浏览器持会话令牌直调 GoTrue `PUT /auth/v1/user`
(自托管 AUTOCONFIRM 无邮件依赖),符合"浏览器直接承担认证面"的既有边界。
