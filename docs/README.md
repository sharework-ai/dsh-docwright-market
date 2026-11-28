# 文档导航

本目录是 dsh-docwright-market 的文档。`guide/` 是分章的系统化文档，面向想了解和使用市场的读者；目录根下的四篇详档是各领域的完整事实源，guide 与详档之间以链接衔接。

## 指南（guide/）

| 章节 | 内容 |
|---|---|
| [01 产品概览](guide/01-overview.md) | 市场的定位、三种角色、核心流程、与 docwright 插件的关系 |
| [02 系统组成](guide/02-architecture.md) | 服务端、校验器、页面与 Supabase 的职责划分，两级审核门与地址拆分约定 |
| [03 投稿与发布流程](guide/03-submission-flow.md) | 从上传 tarball 到可安装的完整过程，机器强制规则清单，套件投稿 |
| [04 账号与权限](guide/04-account-and-permissions.md) | 注册登录、发布者组织、管理员、作者对自己 pack 的管理 |
| [05 安装 pack](guide/05-install.md) | `install.mjs` 的用法、安装过程与完整性检查、与 docwright 插件的衔接 |
| [06 部署与运维摘要](guide/06-deployment.md) | 入口域名、日常操作速查、Supabase 注意事项 |
| [07 测试体系摘要](guide/07-testing.md) | 单元、集成、端到端三层测试与端到端场景清单 |

## 详档

| 文档 | 内容 |
|---|---|
| [features.md](features.md) | 功能与业务规则的完整清单：角色、投稿规则、人工审核的安全边界、账号权限 |
| [architecture.md](architecture.md) | 技术方案：组件边界、关键设计决策、数据模型、完整 API 面 |
| [deployment.md](deployment.md) | 部署与运维：k3s 部署步骤、DNS 配置、服务端配置表、dev 实例、数据清理 |
| [testing.md](testing.md) | 测试方案：三层体系的各层细节、跑法、新增用例约定 |
