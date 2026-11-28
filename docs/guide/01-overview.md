# 产品概览

## 产品定位

dsh-docwright-market 是 [docwright](https://github.com/sharework-ai/dsh-docwright) 场景包（pack）的投稿式市场，部署于 https://dm.shrwk.com。作者通过浏览器上传 pack 的 `.tgz` 归档完成投稿，系统先做机器校验，再由管理员人工审核，审核通过后上架。消费者用零依赖脚本将 pack 安装到本地的 docwright packsDir，可以安装最新版本，也可以指定任意历史版本。

投稿不要求作者具备 git 或 GitHub 使用经验，注册账号只需要一个邮箱。

## 三种角色

| 角色 | 入口 | 能做的事情 |
|---|---|---|
| 作者 | `/submit.html`、`/account.html` | 注册登录、设置发布者组织、上传 pack、查看校验报告、管理自己的 pack、撤回待审投稿 |
| 审核员 | `/admin.html` | 查看待审队列，阅读校验报告与源码，通过或拒绝投稿 |
| 消费者 | 站点目录页、`install.mjs` | 浏览与搜索目录，安装最新版或指定历史版本 |

## 核心流程

整个市场围绕一条流水线运作：

1. 作者设置发布者组织后上传 `.tgz`。
2. 系统执行机器校验，全部检查通过才进入审核队列。
3. 管理员人工审核，通过即上架并写入版本历史。
4. 消费者通过安装脚本获取 pack。

完整过程见 [03-submission-flow.md](03-submission-flow.md)。

## 与 docwright 插件的关系

docwright 是生成与校验文档的 dsh 插件；市场是它的 pack 分发渠道。docwright 插件内置 doc_packs / doc_market_search / doc_market_install 等工具，从市场的 API 读取目录、检索 pack，并通过 `install.mjs`（或同构的安装逻辑）把 pack 落到配置的 packsDir。pack 的创作在 docwright 仓库内完成，市场只负责投稿、审核与分发。

## 详档

角色、业务规则与安全边界的完整描述见 [features.md](../features.md)。
