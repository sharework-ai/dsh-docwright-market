---
# gstack: design-md-format=spec
name: docwright pack market
description: 终端原住民的 AI pack 市场——机器事实的冷静密度感，hero 一处克制的蓝色光晕渐变
colors:
  primary: "#155DFC"          # blue-600: all action & confirmation (light mode)
  on-primary: "#FFFFFF"
  primary-ink-dark: "#2B7FFF" # blue-500: action color in dark mode / hero glow accent
  surface: "#FFFFFF"          # cards, panels; dark: #0F172B
  background: "#F8FAFC"       # slate-50; dark: #020618 (slate-950)
  text: "#0F172B"             # slate-900; dark: #F1F5F9
  text-muted: "#62748E"       # slate-500; dark: #90A1B9
  line: "#E2E8F0"             # slate-200 borders; dark: #1D293D
  accent: "#2B7FFF"           # blue-500: hero glow gradient (blue→#51A2FF), never on buttons
  warning: "#FE9A37"          # amber-500: pending-human-review / executable-code badges only
  warning-ink: "#E1712B"      # amber-600: warning text on light surfaces
  error: "#DC2626"
typography:
  display:
    fontFamily: Cabinet Grotesk     # Fontshare, self-hosted woff2; CJK falls back to system stack
    fontWeight: 800
    fontSize: clamp(38px, 5.4vw, 62px)   # section h2: 24-26px
    letterSpacing: -0.02em
  body:
    fontFamily: General Sans        # Fontshare, self-hosted; fallback PingFang SC / MiSans / Microsoft YaHei
    fontSize: 1rem                  # 14-15px in dense UI
    lineHeight: 1.55
  label:
    fontFamily: General Sans
    fontSize: 0.75rem
    letterSpacing: 0.02em
  mono:
    fontFamily: JetBrains Mono      # Google Fonts, self-hosted; first-class citizen for names/versions/commands/reports
    fontFeature: tnum
rounded:
  sm: 8px      # inputs, inline code, notice blocks
  md: 9px      # buttons
  lg: 12px     # cards, panels
  full: 9999px # chips, badges
spacing:
  xs: 4px
  sm: 8px
  md: 16px
  lg: 24px
  xl: 32px
  2xl: 48px
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    rounded: "{rounded.md}"
  button-primary-hover:
    backgroundColor: "#1447E6"    # blue-700 light / #51A2FF dark
  input:
    borderColor: "{colors.line}"
    rounded: "{rounded.sm}"
    focusRing: "3px blue at 18% opacity, border blue-600"
  card:
    backgroundColor: "{colors.surface}"
    borderColor: "{colors.line}"
    rounded: "{rounded.lg}"
    hover: "border blue-line, translateY(-2px)"
  nav-link:
    textColor: "{colors.text-muted}"
    hover: "{colors.primary}"
  terminal-block:
    backgroundColor: "#0F172B"    # fixed dark code surface in both modes; dark: #01060F
    textColor: "#CAD5E2"          # slate-300; check marks may use blue-300 #8EC5FF
---

# docwright pack market

## Overview

**Creative North Star:** 终端原住民——整个市场做成一份正在运行的终端会话：界面装饰全部来自真实数据（版本号、sha256、check 时间戳），唯一的装饰性装置是 hero 一处克制的蓝色光晕渐变；"AI 前沿感"来自"每像素都是机器校验过的事实"，不来自光晕堆砌。
**Product context:** docwright pack 市场（投稿 → 机器校验 → 人工审核 → 安装），用户是中文为主的开发者/AI agent 用户，页面有市场列表、pack/suite 详情、投稿、作者控制台、审核队列、账号管理。
**Mode per surface:** 全站 Operate（列表/详情/表单/审核队列）；首页 hero 承担 Persuade。
**Reference sites:** 未做外部调研（用户选择凭设计经验）；概念参照 npm / VS Code marketplace 的密度，刻意背离其"安装命令藏在文档深处"的惯例。
**Key characteristics:** 冷静、密集、可信；满屏真实版本号与绿勾；一行可复制的安装命令；amber 只在"人未拍板"处出现。

## Colors

**Strategy:** Committed — slate 承担 90% 界面，blue 独占一切"动作与确认"（按钮、链接、选中 chip、check 通过徽标），amber 是流程色（"机器已过、人未拍板"），红只做错误。
**Light or dark:** 双模式，跟随 `prefers-color-scheme`；浅色为默认工作场景（办公室日光），暗色为终端用户偏好。暗色不是简单反转：面板用 slate-900 承载层级，动作色提亮到 blue-500 保证对比。
**光晕渐变规则（用户点名保留）：** 光晕/渐变只允许出现在首页 hero 一处——浅色模式为 blue-50/100 级别的径向光洗，暗色模式为受控透明度的 blue-500 径向光晕；渐变文字只用于 hero 标题中的强调词（blue-600→blue-400）。卡片、按钮、徽标、代码块一律纯色实底，禁止渐变，禁止零偏移发光边。
**Amber 稀缺原则：** amber 只出现在：待审核状态、functions.js 可执行代码警告、suite 待审成员。绝不作装饰。

## Typography

- Display：Cabinet Grotesk 800（Fontshare，自托管 woff2，UTF-8 latin 子集），字距收紧 -0.02em；中文标题回退系统 CJK 栈，用加重字重而非加大字号制造层级。
- Body/UI：General Sans 400/500/600（Fontshare，自托管）。
- Mono：JetBrains Mono 400/600（Google Fonts，自托管）——pack 名、版本号、安装命令、check 报告、分类 chip（`--category` 旗标样式）全用 mono；mono 是本站一等公民。
- 中文不加载 CJK webfont（体积与许可不划算），回退栈：PingFang SC / HarmonyOS Sans SC / MiSans / Microsoft YaHei；中英混排（中文句内嵌 mono 英文术语）本身是视觉签名，不要"修正"它。
- 加载策略：构建期下载 woff2 入 `web/assets/fonts/`，`font-display: swap`；运行时零外部请求。

## Layout

单列沉浸式：无侧栏。首页首屏 = 海报时刻：大标题（左对齐）+ shell 提示符搜索框（`dm ▍`，focus 蓝框外扩 3px）+ `--flag` 式分类 chips；搜索框与 chips 吸顶随滚动保留在页头下。内容区 max-width 1120px，卡片网格 `repeat(auto-fill, minmax(320px, 1fr))`。详情页双栏（≥860px）：左校验报告 + 描述，右元数据表 + 安装面板。

## Elevation & Depth

阴影极少且必须带偏移（如 `0 8px 24px -18px`）；层级主要靠边框（slate-200/800）与背景色差表达；hover 抬升 2px + 边框变蓝。禁止零偏移彩色光晕边。

## Shapes

圆角分层：输入/内联代码 8px，按钮 9px，卡片 12px，chip/徽标全圆。嵌套内圆角 = 外圆角 − 间距。

## Components

- **主按钮**：纯蓝实底白字，hover 加深（light blue-700 / dark blue-500→400）；禁渐变、禁光晕。
- **次按钮**：透明底 + slate 边框。
- **卡片**：白/slate-900 底 + 1px 边框，hover 边框变蓝 + 上移 2px；pack 卡必须含：mono 名称、蓝色 mono `@version`、两行描述、元数据 chips、一行可复制安装命令 `$ dm install <name>@<version>`。禁止卡片套卡片。
- **状态徽标**：✓ check 全绿 = blue tint；待审/suite/amber；functions.js 警告 = amber；错误 = red。
- **终端块 `pre.sheet`**：固定深色底（两种模式都是），check 行可用 blue-300；是详情页第一屏内容（校验报告前置）。
- **搜索框**：shell 提示符样式（蓝色 `dm` 前缀 + 光标动画），focus 蓝框 + 18% 光环 3px。
- **chips**：mono、`--flag` 样式，选中态纯蓝实底白字。
- 状态全覆盖：空态（"还没有 pack"）、加载、错误 notice（ok=blue tint / warn=amber / bad=red）。

## Do's and Don'ts

- Do：装饰来自真实数据（版本号大号 mono 蓝色、sha256 片段、check 时间戳）；每张可安装对象都带一行可复制命令；amber 留给流程语义；详情页先给机器体检报告。
- Don't：紫色/靛蓝渐变或 blue→purple 配色；hero 以外的光晕、发光边、聚光灯洗；渐变按钮；图标进彩色圆圈的三栏功能格；全部居中；卡片套卡片；标题上方加 kicker/pill；emoji 当设计元素；system-ui 当主字体。

## Motion

- **Approach:** minimal-functional
- **Easing:** enter(ease-out) exit(ease-out) move(ease-in-out)
- **Duration:** micro(50-100ms) short(150-250ms) medium(250-400ms)
- **The one authored moment:** 详情页 pack:check 摘要逐行浮现（150-250ms，ease-out，层级自上而下）。

## Decisions Log
| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-11-24 | 初始设计系统定稿 | /design-consultation：终端原住民 × 单列沉浸式 × blue/slate/amber；外部声音 = Claude 子代理（Codex 未安装）；字体经 Fontshare/Google 在线核验（2026-11-24） |
