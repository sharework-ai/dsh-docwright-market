# 安装 pack

## 前提

安装脚本 `install.mjs` 只依赖 node（版本 18 及以上），不安装任何 npm 包。安装目标目录是 docwright 插件配置的 packsDir，默认为本目录下的 `.docwright/packs/`。

## 用法

```sh
node install.mjs task-card                # 安装最新版本
node install.mjs task-card@0.1.0          # 安装指定历史版本
node install.mjs --list                   # 列出市场目录
node install.mjs --all                    # 安装目录中的全部 pack
node install.mjs --into <dir>             # 指定目标 packsDir
node install.mjs --market <url>           # 指定市场地址（默认 http://localhost:8398）
node install.mjs --force                  # 覆盖已安装的同名 pack
```

环境变量 `MARKET_URL` 可以代替 `--market`。已装好的 pack 在 docwright 宿主重启后生效。

## 安装过程

脚本从市场拉取安装索引（`/api/index`），按名字找到 pack 与版本，下载 tarball 并做两级完整性检查：先比对索引中的 `tarballSha256`，再复核解包后的内容哈希与 pack 名。检查通过后先写入临时目录，再原子替换到目标位置。全程只复制文件，不执行 pack 中的任何代码。

若 pack 含 `rules/functions.js`，该文件由 docwright 在装载 pack 时于用户本机执行。此类 pack 在站点目录与安装索引中都带 `hasFunctionRules` 标记，安装前请自行审阅其源码；人工审核是它进入市场的唯一关口。

## 与 docwright 插件的衔接

docwright 插件的 doc_market_search / doc_market_install 等工具调用与 `install.mjs` 相同的市场 API，把 pack 安装进配置的 packsDir；手动安装与插件安装结果一致，可以混用。

## 版本选择

- 不带版本号时安装最新已发布版本。
- `name@version` 安装任意历史版本；全部历史版本的 tarball 在市场长期保留。
- pack 被属主下架后，下载请求返回 410，安装失败。

## 详档

索引格式与完整性机制的说明见 [architecture.md](../architecture.md)。
