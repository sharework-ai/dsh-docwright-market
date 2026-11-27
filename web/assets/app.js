// docwright pack market — shared browser helpers (vanilla JS, no build step).
// Exposes window.MB: config/supabase bootstrap, i18n, authed fetch helpers.
window.MB = (() => {
  let config = null
  let supabase = null

  const I18N = {
    en: {
      browse: 'Market',
      packs: 'Suites & Packs',
      submit: 'Submit',
      admin: 'Review',
      login: 'Sign in',
      logout: 'Sign out',
      email: 'Email',
      password: 'Password',
      signUp: 'Sign up',
      signIn: 'Sign in',
      searchHint: 'Search packs…',
      all: 'All',
      noPacks: 'No packs yet — be the first to submit one!',
      install: 'Install',
      download: 'Download tarball',
      healthSheet: 'Verification health sheet',
      functionsWarn:
        'This pack contains executable function rules (rules/functions.js). They run on every user machine when docwright loads the pack — review the code before installing.',
      submitTitle: 'Submit a pack',
      submitHint:
        'Upload a .tgz of your pack directory. Machine verification runs immediately; green packs enter the human review queue.',
      archive: 'Pack archive (.tgz)',
      author: 'Author (display name)',
      category: 'Tags',
      license: 'License (SPDX)',
      homepage: 'Homepage (optional)',
      send: 'Submit',
      verifying: 'Verifying…',
      accepted: 'Accepted into the review queue.',
      rejectedMachine: 'Verification failed — fix the report below and resubmit.',
      status: 'Status',
      refresh: 'Refresh status',
      tabPack: 'Pack',
      tabSuite: 'Suite',
      suiteSubmitHint: 'A suite is pure metadata: NO archive to upload — just the member list. Every member must already be a published pack. The submission enters the same review queue as packs.',
      suiteQueued: 'Suite submission accepted — it enters the review queue.',
      trackInPacks: 'Track it on the Packs page',
            queue: 'Pending review queue',
      tzTitle: 'Date & time zone',
      tzHint: 'Applies to every date shown on the market pages.',
      tzLabel: 'Time zone',
            tzAuto: 'Time zone: browser default',
      'tz_Asia/Shanghai': 'China Standard Time',
      'tz_Europe/London': 'UK time',
      'tz_America/New_York': 'US Eastern time',
      history: 'Review history',
      emptyHistory: 'No decided reviews yet.',
      filter_all: 'All',
      filter_approved: 'Approved',
      filter_rejected: 'Rejected',
      th_time: 'Time',
      th_target: 'Target',
      th_reviewer: 'Reviewer',
      approved: 'Approved',
      rejected: 'Rejected',
      approve: 'Approve',
      reject: 'Reject',
      note: 'Note (optional)',
      emptyQueue: 'Queue is empty.',
      version: 'Version',
      consumes: 'Consumes',
      languages: 'Languages',
      requiresTopic: 'Requires topic',
      artifactPrefix: 'Artifact prefix',
      functionRules: 'Function rules',
      added: 'Added',
      updated: 'Updated',
      packNotFound: 'Pack not found.',
      account: 'Account',
      accountTitle: 'Your account',
      orgGate: 'Set your publisher org before submitting — it takes one minute on the account page.',
      orgEditHint: 'Edit org',
      signInToPublish: 'Sign in to publish your own packs.',
      accountHint: 'Password and publisher org settings. Your packs and submissions live on the Packs page.',
      passwordTitle: 'Change password',
      currentPassword: 'Current password',
      newPassword: 'New password (min 6)',
      changePassword: 'Change password',
      passwordChanged: 'Password changed. Please sign in again with the new password.',
      orgTitle: 'Publisher org',
      orgHint:
        'A publisher org labels every pack you publish. It is required before submitting; changing it relabels all your packs at once.',
      orgCurrent: 'Current org',
      orgNone: 'No org yet — set one before submitting.',
      orgNew: 'New org (lowercase kebab-case)',
      orgSet: 'Set org',
      orgChanged: (n) => `Org saved — ${n} pack(s) relabelled.`,
      orgConfirm: (n) => `All ${n} of your pack(s) will be relabelled. Change now?`,
      orgTaken: 'That org is already taken.',
      orgInvalid: 'Org must be lowercase kebab-case (letters, digits, dashes), up to 39 chars, not reserved.',
      myPacks: 'Packs',
      mySubmissions: 'Submissions',
      noPacksYet: 'No packs published yet.',
      noSubmissions: 'No submissions yet.',
      withdraw: 'Withdraw',
      disable: 'Disable',
      enable: 'Enable',
      delete: 'Delete',
      edit: 'Edit',
      save: 'Save',
      cancel: 'Cancel',
      editMeta: 'Edit metadata',
      deleteConfirmPrefix: 'Type the pack name to confirm deletion of',
      deleteReservedNote: 'The name + every version number stay reserved forever; a re-registered pack must bump the version.',
      versionsLabel: 'Versions',
      publishedAt: 'Published',
      disabledBadge: 'disabled',
      suites: 'Suites',
      mySuites: 'Suites',
      noSuitesYet: 'No suites published yet.',
      suitesIntro: 'A suite is a named group of packs — installing it brings every member at once.',
      suiteName: 'Suite name (lowercase kebab-case)',
      suiteDescEn: 'Description (EN)',
      suiteDescZh: 'Description (中文, optional)',
      suiteVersion: 'Suite version',
      suiteMembers: 'Member packs (multi-select)',
      createSuite: 'Submit suite for review',
      suiteCreated: 'Submitted — the suite enters the review queue.',
      suiteUpdated: 'Edit submitted — it takes effect after approval.',
      suiteDeleteQueued: 'Deletion submitted — it takes effect after approval.',
      suiteWithdrawn: 'Pending submission withdrawn.',
      editSuite: 'Edit (goes through review)',
      kindSuite: 'suite',
      kindPack: 'pack',
      suiteFormContent: 'Suite form',
      members: 'Members',
      missingMember: 'missing',
      belongsTo: 'Suites',
      installSuite: 'Install',
      noSuites: 'No suites yet.',
      suiteNotFound: 'Suite not found.',
      membersLabel: 'Member packs',
      loading: 'Loading…',
      heroTitleHtml: 'Generate consistent documents —<br>start by installing a <span class="hero-grad">pack</span>.',
      heroSubHtml: 'The scenario-pack market for docwright: generator prompts, JSON schemas, checking rules and templates — machine-verified (<code class="font-mono text-primary dark:text-accent-strong">pack:check</code> green) + human-reviewed, installed straight into your <code class="font-mono text-primary dark:text-accent-strong">packsDir</code>.',
      footerHtml: 'No pack fits? Just say: <strong>“please create a dsh-docwright pack or suite for me — here is what I need…”</strong> — <a href="https://github.com/sharework-ai/dsh-docwright-market">source</a>',
      yes: 'yes',
      no: 'no',
      cliSummary: 'CLI (outside a DSH session)',
      cliDetailsHtml: 'Copies the pack into <code class="font-mono">./.docwright/packs/</code> (or pass <code class="font-mono">--into</code>), then restart the dsh host and check with <code class="font-mono">doc_packs</code>. Pin a version with <code class="font-mono">node install.mjs {name}@0.1.0</code>.',
      installDshHtml: 'In a DSH session with the docwright plugin, just ask: <strong>“install {name}”</strong>. The model finds this pack and installs it after your confirmation — it is usable in the same session, no restart.',
      installSuiteDshHtml: 'In a DSH session with the docwright plugin, just ask: <strong>“install the {name} suite”</strong>. The model installs every member pack at once, after your confirmation.',
      installHereBtn: 'Install into this machine',
      cardInstall: 'Install',
      cardInstalledOk: 'Installed',
      cardInstallFail: 'Failed',
      searchBtn: 'Search',
      installNeedsDsh: 'Browse in DeepSeek Harness to install',
      downloadZip: 'Download ZIP',
      bundleSuite: 'Suite',
      bundlePack: 'Pack',
      status_new: 'new',
      status_update: 'update',
      status_unchanged: 'unchanged',
      status_tooLow: 'version lower than the market — blocked',
      status_missing: 'missing',
      submitAll: 'Parse and update',
      submitZipHint: 'Upload the ZIP the download buttons produce: a root manifest.json makes it a suite (members in their own folders); otherwise every top-level folder with a manifest.json is a pack. Versions must not be lower than the published ones.',
      versionRule: 'Suite and pack versions must not be lower than the published ones.',
      noBundle: 'No pack or suite found in this ZIP (expected manifest.json at the root or one per top-level folder).',
      memberPacks: 'Member packs',
      submittingItem: 'Submitting {name}…',
      suiteBadge: 'Suite',
      assetsTitle: 'Gallery',
      zipping: 'Zipping…',
      zipFail: 'ZIP failed',
      suiteCreate: 'New suite',
      confirmStep: 'Step 2 — confirm the submission',
      lockedFromArchive: 'Read from the archive — not editable.',
      cat_all: 'All',
      cat_general: 'General',
      cat_business: 'Business',
      cat_it: 'IT',
      cat_finance: 'Finance',
      cat_education: 'Education',
      cat_entertainment: 'Entertainment',
      cat_media: 'Media',
      cat_healthcare: 'Healthcare',
      cat_legal: 'Legal',
      cat_engineering: 'Engineering',
      cat_other: 'Other',
      standaloneHint: 'Standalone packs only — packs that belong to a suite are managed on their suite.',
      suiteMaint: 'Suite maintenance',
      suiteTitleEn: 'Title (EN)',
      suiteTitleZh: 'Title (中文, optional)',
      suiteMaintIntro: 'A suite is a named group of published packs. Pick members, edit metadata, submit for review.',
      suiteMembersPick: 'Member packs',
      suitePickHint: 'Type to filter, click a row to add or remove.',
      suiteMembersChosen: 'Selected',
      suitePickEmpty: 'No packs match this filter.',
      step1Archive: 'Step 1 — upload the pack archive',
      recognizedHtml: 'Recognized <b>{name}</b>@{version}',
      recognizedDesc: '{desc}',
      recognizedUnknown: 'Could not read manifest.json from this archive — you can still submit; the server verifies it.',
      membersMustBePublished: 'Members must already be published packs — upload those first on the Pack tab.',
      goSuiteMaint: 'Maintain suites',
      installing: 'Installing… do not close this page.',
      installDone: 'Installed — usable in the current DSH session, no restart.',
      installFailed: 'Install failed',
      installConfirm: 'Install "{name}" into this machine\'s packsDir now?',
      backPacks: '← Packs',
      authFill: 'Fill in email and password first.',
      authMin6: 'Password needs at least 6 characters.',
      pwFillRequired: 'Current password + new password (min 6) required.',
      signupInstantHtml: 'Sign-up confirms instantly on this host (no SMTP).',
      adminNoteHtml: 'Reviewing requires an admin profile (<code class="font-mono">profiles.is_admin</code>).',
    },
    zh: {
      browse: '市场',
      packs: '套件与 Packs',
      submit: '投稿',
      admin: '审核',
      login: '登录',
      logout: '退出',
      email: '邮箱',
      password: '密码',
      signUp: '注册',
      signIn: '登录',
      searchHint: '搜索 pack…',
      all: '全部',
      noPacks: '还没有 pack——来做第一个投稿人!',
      install: '安装',
      download: '下载 tarball',
      healthSheet: '校验体检单',
      functionsWarn:
        '该 pack 含可执行函数规则(rules/functions.js),会在每个用户的 docwright 装载 pack 时执行——安装前请审查代码。',
      submitTitle: '投稿 pack',
      submitHint: '上传 pack 目录的 .tgz 压缩包;机器校验立即执行,全绿后进入人工审核队列。',
      archive: 'Pack 压缩包(.tgz)',
      author: '作者(显示名)',
      category: '标签',
      license: '许可证(SPDX)',
      homepage: '主页(可选)',
      send: '提交',
      verifying: '校验中…',
      accepted: '已进入审核队列。',
      rejectedMachine: '机器校验未通过——按下方面板修复后重新提交。',
      status: '状态',
      refresh: '刷新状态',
      tabPack: 'Pack',
      tabSuite: '套件',
      suiteSubmitHint: '套件是纯元数据：不需要上传压缩包，只需成员名单。每个成员必须已是已上架的 pack。投稿进入与 pack 相同的审核队列。',
      suiteQueued: '套件投稿已受理——进入审核队列。',
      trackInPacks: '到 Packs 页跟踪进度',
            queue: '待审核队列',
      tzTitle: '日期与时区',
      tzHint: '作用于市场各页面展示的所有日期时间。',
      tzLabel: '时区',
            tzAuto: '时区：跟随浏览器',
      'tz_Asia/Shanghai': '中国标准时间',
      'tz_Europe/London': '英国时间',
      'tz_America/New_York': '美国东部时间',
      history: '审核历史',
      emptyHistory: '还没有审核记录。',
      filter_all: '全部',
      filter_approved: '已通过',
      filter_rejected: '已拒绝',
      th_time: '时间',
      th_target: '对象',
      th_reviewer: '审核人',
      approved: '已通过',
      rejected: '已拒绝',
      approve: '通过',
      reject: '拒绝',
      note: '备注(可选)',
      emptyQueue: '队列为空。',
      version: '版本',
      consumes: '消费上游',
      languages: '语言',
      requiresTopic: '需要话题',
      artifactPrefix: '产物前缀',
      functionRules: '函数规则',
      added: '收录时间',
      updated: '更新时间',
      packNotFound: '找不到该 pack。',
      account: '账号',
      accountTitle: '我的账号',
      orgGate: '投稿前请先设置发布者组织——去账号页一分钟搞定。',
      orgEditHint: '编辑组织',
      signInToPublish: '登录后可以发布和管理自己的 pack。',
      accountHint: '密码与发布者组织设置;你的 pack 与投稿管理在 Packs 页面。',
      passwordTitle: '修改密码',
      currentPassword: '当前密码',
      newPassword: '新密码(至少 6 位)',
      changePassword: '修改密码',
      passwordChanged: '密码已修改,请用新密码重新登录。',
      orgTitle: '发布者组织',
      orgHint: '组织标识你发布的所有 pack;投稿前必须设置,修改组织会一并更新你的全部 pack。',
      orgCurrent: '当前组织',
      orgNone: '尚未设置组织——投稿前请先设置。',
      orgNew: '新组织(小写 kebab-case)',
      orgSet: '保存组织',
      orgChanged: (n) => `组织已保存——已同步更新 ${n} 个 pack。`,
      orgConfirm: (n) => `你的全部 ${n} 个 pack 都将一并更新组织标识,确定修改?`,
      orgTaken: '该组织名已被占用。',
      orgInvalid: '组织只能是小写字母/数字/连字符,最长 39 字符,且不能用保留名。',
      myPacks: 'Packs',
      mySubmissions: '投稿记录',
      noPacksYet: '还没有已发布的 pack。',
      noSubmissions: '还没有投稿记录。',
      withdraw: '撤回',
      disable: '下架',
      enable: '恢复上架',
      delete: '删除',
      edit: '编辑',
      save: '保存',
      cancel: '取消',
      editMeta: '编辑元数据',
      deleteConfirmPrefix: '输入 pack 名以确认删除',
      deleteReservedNote: '名字与所有版本号将永久保留;同名重注册必须提升版本号。',
      versionsLabel: '版本历史',
      publishedAt: '发布时间',
      disabledBadge: '已下架',
      suites: '套件',
      mySuites: '套件',
      noSuitesYet: '还没有已发布的套件。',
      suitesIntro: '套件是一组 pack 的命名集合——安装套件会一次装上全部成员。',
      suiteName: '套件名(小写 kebab-case)',
      suiteDescEn: '英文描述(必填)',
      suiteDescZh: '中文描述(可选)',
      suiteVersion: '套件版本',
      suiteMembers: '成员 pack(多选)',
      createSuite: '提交套件进审核',
      suiteCreated: '已提交——套件进入审核队列。',
      suiteUpdated: '修改已提交——审核通过后生效。',
      suiteDeleteQueued: '删除已提交——审核通过后生效。',
      suiteWithdrawn: '待审提交已撤回。',
      editSuite: '修改(进审核队列)',
      kindSuite: '套件',
      kindPack: 'pack',
      suiteFormContent: '套件表单',
      members: '成员',
      missingMember: '缺失',
      belongsTo: '所属套件',
      installSuite: '安装',
      noSuites: '还没有套件。',
      suiteNotFound: '找不到该套件。',
      membersLabel: '成员 pack',
      loading: '加载中…',
      heroTitleHtml: '生成规范一致的文档，<br>从安装一个 <span class="hero-grad">pack</span> 开始。',
      heroSubHtml: 'docwright 的场景 pack 市场：生成 prompt、JSON Schema、校验规则与模板；机器校验（<code class="font-mono text-primary dark:text-accent-strong">pack:check</code> 全绿）+ 人工审核，装进你的 <code class="font-mono text-primary dark:text-accent-strong">packsDir</code> 即用。',
      footerHtml: '没有找到合适的 pack？您可以试着说：<strong>“请帮我创建一个 dsh-docwright 的 pack 或 suite，具体要求如下……”</strong>——<a href="https://github.com/sharework-ai/dsh-docwright-market">源码</a>',
      yes: '是',
      no: '否',
      cliSummary: 'CLI（在 DSH 会话之外）',
      cliDetailsHtml: '拷入 <code class="font-mono">./.docwright/packs/</code>（可用 <code class="font-mono">--into</code> 指定），重启宿主后用 <code class="font-mono">doc_packs</code> 确认可见；<code class="font-mono">node install.mjs {name}@0.1.0</code> 可安装指定历史版本。',
      installDshHtml: '在装有 docwright 插件的 DSH 会话中直接说<strong>“帮我安装 {name}”</strong>即可：模型会找到本包并在你确认后安装，装完当前会话立即可用，无需重启。',
      installSuiteDshHtml: '在装有 docwright 插件的 DSH 会话中直接说<strong>“安装 {name} 套件”</strong>即可：模型会按成员清单一次装上全部 pack，你确认后才执行。',
      installHereBtn: '一键安装到本机',
      cardInstall: '安装',
      cardInstalledOk: '已安装',
      cardInstallFail: '失败',
      searchBtn: '搜索',
      installNeedsDsh: '请在 DeepSeek Harness 中浏览并安装',
      downloadZip: '下载 ZIP',
      bundleSuite: '套件',
      bundlePack: 'Pack',
      status_new: '新包',
      status_update: '更新',
      status_unchanged: '版本相同',
      status_tooLow: '版本低于市场已发布——阻断',
      status_missing: '缺失',
      submitAll: '解析并更新',
      submitZipHint: '上传下载按钮产出的 ZIP：根目录有 manifest.json 即套件（成员在各自目录中）；否则每个含 manifest.json 的顶层目录是一个 pack。版本不得低于市场当前版本。',
      versionRule: '套件与 pack 的版本不得低于市场当前版本。',
      noBundle: 'ZIP 中没有找到 pack 或套件（应在根目录或每个顶层目录下有 manifest.json）。',
      memberPacks: '成员 pack',
      submittingItem: '正在提交 {name}…',
      suiteBadge: '套件成员',
      assetsTitle: '图集',
      zipping: '打包中…',
      zipFail: '打包失败',
      suiteCreate: '新建套件',
      confirmStep: '第 2 步：确认投稿内容',
      lockedFromArchive: '以下信息读取自压缩包，不可修改。',
      cat_all: '全部',
      cat_general: '通用',
      cat_business: '商务',
      cat_it: '信息技术',
      cat_finance: '财经',
      cat_education: '教育',
      cat_entertainment: '娱乐',
      cat_media: '多媒体',
      cat_healthcare: '医疗',
      cat_legal: '法律',
      cat_engineering: '工程',
      cat_other: '其他',
      standaloneHint: '仅列独立的 pack——属于套件的 pack 请在对应套件下管理。',
      suiteMaint: '套件维护',
      suiteTitleEn: '标题（英文）',
      suiteTitleZh: '标题（中文，可选）',
      suiteMaintIntro: '套件是一组已发布 pack 的命名集合。选择成员、编辑元数据，提交进审核队列。',
      suiteMembersPick: '成员 pack',
      suitePickHint: '输入过滤，点击一行添加或移除。',
      suiteMembersChosen: '已选',
      suitePickEmpty: '没有匹配的 pack。',
      step1Archive: '第 1 步：上传 pack 压缩包',
      recognizedHtml: '已识别 <b>{name}</b>@{version}',
      recognizedDesc: '{desc}',
      recognizedUnknown: '未能从压缩包读取 manifest.json——仍可继续提交，服务端会校验。',
      membersMustBePublished: '成员必须已是已发布的 pack——请先在 Pack 标签上传。',
      goSuiteMaint: '套件维护',
      installing: '正在安装…请勿关闭页面。',
      installDone: '安装完成——当前 DSH 会话立即可用，无需重启。',
      installFailed: '安装失败',
      installConfirm: '现在把“{name}”安装进本机的 packsDir？',
      backPacks: '← 返回 Packs',
      authFill: '请先填写邮箱和密码。',
      authMin6: '密码至少 6 位。',
      pwFillRequired: '请填写当前密码，新密码至少 6 位。',
      signupInstantHtml: '本机自托管无邮件服务，注册即时生效。',
      adminNoteHtml: '审核页需要管理员账号（<code class="font-mono">profiles.is_admin</code>）。',
    },
  }

  function lang() {
    return localStorage.getItem('dw-market-lang') || 'zh'
  }
  function setLang(l) {
    localStorage.setItem('dw-market-lang', l)
    // Dynamically rendered templates (pack cards, queues, detail bodies)
    // pick up MB.t() at render time — a reload re-renders them all, the
    // same approach the timezone picker already uses.
    location.reload()
  }

  // ---- date & time formatting --------------------------------------------
  // A few typical timezones with their pairing locales; "auto" follows the
  // browser. The choice persists in localStorage and re-renders the page.
  const TIME_ZONES = [
    { id: 'auto', localeTag: null, timeZone: null },
    { id: 'Asia/Shanghai', localeTag: 'zh-CN', timeZone: 'Asia/Shanghai' },
    { id: 'Europe/London', localeTag: 'en-GB', timeZone: 'Europe/London' },
    { id: 'America/New_York', localeTag: 'en-US', timeZone: 'America/New_York' },
  ]
  // Entry query strings: ?lang=zh|en&tz=<zone id|auto> — applied once on
  // arrival (persisted like a manual choice) and stripped from the address
  // bar, so a refresh or in-site navigation does not force them again.
  ;(() => {
    const params = new URLSearchParams(location.search)
    const langParam = params.get('lang')
    if (langParam === 'zh' || langParam === 'en') localStorage.setItem('dw-market-lang', langParam)
    const tzParam = params.get('tz')
    if (tzParam && TIME_ZONES.some(tz => tz.id === tzParam)) localStorage.setItem('dw-market-tz', tzParam)
    if (params.has('lang') || params.has('tz')) {
      params.delete('lang')
      params.delete('tz')
      const rest = params.toString()
      history.replaceState(null, '', location.pathname + (rest ? `?${rest}` : '') + location.hash)
    }
  })()

  function timeZoneId() {
    const saved = localStorage.getItem('dw-market-tz')
    return TIME_ZONES.some(tz => tz.id === saved) ? saved : 'auto'
  }
  function setTimeZone(id) {
    if (!TIME_ZONES.some(tz => tz.id === id)) return
    localStorage.setItem('dw-market-tz', id)
    // Pages render dates inline at fetch time; a reload is the cheapest way
    // to re-render everything with the new zone.
    location.reload()
  }
  function fmtDateTime(iso) {
    if (!iso) return '—'
    // Format follows the INTERFACE LANGUAGE; only the zone comes from the
    // timezone picker. (The old pairing bound each zone to a locale, which
    // froze Chinese date shapes onto the English UI.)
    const tz = TIME_ZONES.find(t => t.id === timeZoneId())
    const locale = lang() === 'zh' ? 'zh-CN' : 'en-US'
    const options = { dateStyle: 'medium', timeStyle: 'short' }
    if (tz.timeZone) options.timeZone = tz.timeZone
    const date = new Date(iso)
    if (Number.isNaN(date.getTime())) return iso
    return new Intl.DateTimeFormat(locale, options).format(date)
  }
  function t(key) {
    return I18N[lang()][key] ?? I18N.en[key] ?? key
  }
  function applyI18n() {
    document.querySelectorAll('[data-i18n]').forEach(el => {
      el.textContent = t(el.dataset.i18n)
    })
    // data-i18n-html carries rich copy (links, code spans, <br>); "{name}"
    // placeholders are filled from data-i18n-args ("name:cosmic-plan,...").
    document.querySelectorAll('[data-i18n-html]').forEach(el => {
      let html = t(el.dataset.i18nHtml)
      const args = el.dataset.i18nArgs
      if (args) for (const pair of args.split(',')) {
        const [k, v] = pair.split(':')
        html = html.replaceAll(`{${k}}`, esc(v ?? ''))
      }
      el.innerHTML = html
    })
    document.querySelectorAll('[data-i18n-ph]').forEach(el => {
      el.placeholder = t(el.dataset.i18nPh)
    })
    const btn = document.getElementById('lang-btn')
    if (btn) btn.textContent = lang() === 'zh' ? 'EN' : '中文'
    const tzSel = document.getElementById('tz-select')
    if (tzSel) tzSel.value = timeZoneId()
  }

  async function loadConfig() {
    if (config) return config
    config = await fetch('/config.json').then(r => r.json())
    if (window.supabase) {
      supabase = window.supabase.createClient(config.supabaseUrl, config.supabasePublishableKey)
    }
    return config
  }

  async function session() {
    await loadConfig()
    if (!supabase) return null
    const { data } = await supabase.auth.getSession()
    return data.session
  }

  async function api(path, { method = 'GET', body, auth = false } = {}) {
    const headers = {}
    if (auth) {
      const s = await session()
      if (!s) throw new Error('请先登录 / sign in first')
      headers.Authorization = `Bearer ${s.access_token}`
    }
    if (body && !(body instanceof FormData)) headers['Content-Type'] = 'application/json'
    const res = await fetch(path, { method, headers, body: body ? (body instanceof FormData ? body : JSON.stringify(body)) : undefined })
    const json = await res.json().catch(() => ({}))
    if (!res.ok) throw new Error(json.error || `${res.status} ${res.statusText}`)
    return json
  }

  function header() {
    // Pages call MB.header from several fallback paths — keep it idempotent:
    // replace any existing header instead of prepending a second one.
    document.querySelector('header.site')?.remove()
    const el = document.createElement('header')
    el.className = 'site'
    el.innerHTML = `
      <div class="inner">
        <a class="brand" href="/">dsh-docwright market</a>
        <nav>
          <a href="/" data-i18n="browse"></a>
          <a href="/packs.html" data-i18n="packs" class="hidden" id="nav-packs"></a>
          <a href="/admin.html" data-i18n="admin" class="hidden" id="nav-admin"></a>
          <span id="auth-slot"></span>
          <button id="lang-btn" class="lang-btn" title="switch language"></button>
        </nav>
      </div>`
    document.body.prepend(el)
    el.querySelector('#lang-btn').addEventListener('click', () => setLang(lang() === 'zh' ? 'en' : 'zh'))
    applyI18n()
    refreshAuthSlot()
  }

  /** Mask an account string for display: keep the first and last 2
   * characters, at most 4 '*' in between; strings shorter than 4 characters
   * collapse to x*x / x* shapes (first and last letters survive). */
  function maskEmail(str) {
    const v = String(str ?? '')
    const n = v.length
    if (n < 2) return 'x*'
    if (n === 2) return 'x*'
    if (n < 4) return 'x' + '*'.repeat(n - 2) + 'x'
    return v.slice(0, 2) + '****' + v.slice(-2)
  }

  async function refreshAuthSlot() {
    const slot = document.getElementById('auth-slot')
    if (!slot) return
    const s = await session().catch(() => null)
    if (!s) {
      slot.innerHTML = `<a href="/account.html" data-i18n="login"></a>`
      applyI18n()
      return
    }
    // The email itself is the account entry point.
    slot.innerHTML = `<a href="/account.html" class="auth-email" title="${esc(s.user.email)}">${esc(maskEmail(s.user.email))}</a> <a href="#" id="logout-a" data-i18n="logout"></a>`
    slot.querySelector('#logout-a').addEventListener('click', async e => {
      e.preventDefault()
      await supabase.auth.signOut()
      location.reload()
    })
    applyI18n()
    // Personal nav surfaces only for the signed-in: Packs for everyone,
    // Review for admins only.
    document.getElementById('nav-packs')?.classList.remove('hidden')
    const acc = await api('/api/account', { auth: true }).catch(() => null)
    if (acc?.isAdmin) document.getElementById('nav-admin')?.classList.remove('hidden')
  }

  /** Display title: the localized `title` when present, else the code name. */
  function displayTitle(item) {
    const t = item?.title
    if (t && typeof t === 'object') return t[lang()] || t.en || t.zh || item?.name || ''
    if (typeof t === 'string' && t) return t
    return item?.name || ''
  }

  function localizedDesc(description) {
    if (!description) return ''
    if (typeof description === 'string') return description
    return description[lang()] || description.en || ''
  }

  function esc(text) {
    const div = document.createElement('div')
    div.textContent = String(text ?? '')
    return div.innerHTML
  }

  /** Populate a <select> with the timezone choices and wire the change
   * handler (the picker lives on the account page, not the header). */
  function renderTimeZonePicker(select) {
    select.innerHTML = TIME_ZONES.map(tz =>
      `<option value="${tz.id}">${tz.id === 'auto' ? t('tzAuto') : t('tz_' + tz.id)}</option>`
    ).join('')
    select.value = timeZoneId()
    select.addEventListener('change', () => setTimeZone(select.value))
  }

  // ---- in-DSH one-click install bridge ------------------------------------
  // When the site is embedded by the docwright plugin's market tab
  // (window.parent !== window), the install button posts an install request
  // to the parent tab; the tab — same-origin with the plugin's HTTP routes —
  // performs the POST against /api/docwright/{pack,suite}-install and replies
  // here. The tab verifies OUR origin before acting; replies are matched to
  // pending requests by their protocol shape and correlation id, and the
  // payload carries no secrets, so accepting them from any parent is safe.
  let installSeq = 0
  const pendingInstalls = new Map()
  // Optional chaining keeps non-browser shims (unit-test sandboxes without a
  // real window) loading; the bridge simply stays dormant there.
  window.addEventListener?.('message', event => {
    const data = event.data
    if (typeof data !== 'object' || data === null || data.source !== 'docwright-market') return
    const pending = pendingInstalls.get(data.id)
    if (!pending) return
    if (data.type === 'install-accepted') {
      pending.onAccepted?.()
      return
    }
    if (data.type === 'install-result') {
      pendingInstalls.delete(data.id)
      pending.onResult(data)
    }
  })

  /** True when the site is framed (i.e. embedded by the docwright market tab). */
  function inDsh() {
    try { return window.parent !== window } catch { return true }
  }

  /** Ask the embedding tab to install one pack or suite. */
  function requestInstall(kind, name, { onAccepted, onResult } = {}) {
    const id = `dw-install-${Date.now()}-${installSeq++}`
    pendingInstalls.set(id, { onAccepted, onResult })
    window.parent?.postMessage?.({ source: 'docwright-market', type: 'install-request', id, kind, name }, '*')
  }

  /** Mount the one-click install button + status line into `host` (a plain
   * element inside the page's install section). Tailwind-styled like the
   * page's other primary buttons; a no-op when the site is NOT framed, so
   * standalone visitors keep the session/CLI instructions only. */
  function mountInstallButton(kind, name, host) {
    if (!host) return
    if (!inDsh()) {
      const hint = document.createElement('p')
      hint.className = 'notice info !inline-block max-w-xl'
      hint.textContent = t('installNeedsDsh')
      host.prepend(hint)
      return
    }
    const wrap = document.createElement('div')
    wrap.className = 'mt-6 mb-4'
    wrap.innerHTML = `
      <button class="primary" type="button">${esc(t('installHereBtn'))}</button>
      <p class="mt-2 text-[13px] text-slate-500 dark:text-slate-400" data-slot="status" hidden></p>`
    host.prepend(wrap)
    const button = wrap.querySelector('button')
    const status = wrap.querySelector('[data-slot="status"]')
    const show = text => { status.textContent = text; status.hidden = false }
    button.addEventListener('click', () => {
      if (!window.confirm(t('installConfirm').replaceAll('{name}', name))) return
      button.disabled = true
      show(t('installing'))
      requestInstall(kind, name, {
        onResult(result) {
          button.disabled = false
          if (result.ok) {
            const landed = Array.isArray(result.installed) ? result.installed.length : 0
            show(`${t('installDone')} (${landed})`)
          } else {
            show(`${t('installFailed')}: ${result.message || 'unknown error'}`)
          }
        },
      })
    })
  }

  /** Compact corner install button for catalog cards: absolute top-right
   * (the card template must carry the `relative` class), shows a pending
   * spinner face and the outcome inline. No-op when NOT framed by DSH. */
  function mountCardInstallButton(kind, name, card) {
    if (!card) return
    // Outside a DSH frame there is no install lane — render nothing rather
    // than a hint on every card (the hint lives on the detail pages).
    if (!inDsh()) return
    const btn = document.createElement('button')
    btn.type = 'button'
    btn.title = t('installHereBtn')
    btn.className = 'absolute right-3 top-3 cursor-pointer rounded-full border border-slate-200 bg-white px-3 py-0.5 text-xs font-medium text-slate-500 transition hover:border-primary hover:text-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-400 dark:hover:border-accent dark:hover:text-accent-strong'
    btn.textContent = t('cardInstall')
    card.appendChild(btn)
    btn.addEventListener('click', e => {
      e.stopPropagation()
      btn.disabled = true
      btn.textContent = '…'
      requestInstall(kind, name, {
        onResult(result) {
          btn.disabled = false
          if (result.ok) {
            btn.textContent = `✓ ${t('cardInstalledOk')}`
            btn.classList.add('!border-primary', '!text-primary', 'dark:!border-accent', 'dark:!text-accent-strong')
          } else {
            btn.textContent = `✗ ${t('cardInstallFail')}`
            btn.title = result.message || ''
          }
        },
      })
    })
  }

  // ---- client-side ZIP download (pack files / suite members) -------------
  // The server stays zero-dependency: tarballs are public tgz files, so the
  // browser gunzips (DecompressionStream), walks the ustar entries with the
  // same minimal parser the submit page uses, and writes a STORE-method ZIP
  // (the entries are already compressed — re-deflating would buy nothing).

  const CRC_TABLE = (() => {
    const table = new Uint32Array(256)
    for (let n = 0; n < 256; n++) {
      let c = n
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
      table[n] = c >>> 0
    }
    return table
  })()

  function crc32(bytes) {
    let c = 0xffffffff
    for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8)
    return (c ^ 0xffffffff) >>> 0
  }

  /** Minimal ustar walk: returns [{ path, bytes }] for regular files. */
  async function tarEntries(gzipStream) {
    const reader = gzipStream.getReader()
    const chunks = []
    let total = 0
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      chunks.push(value)
      total += value.length
      if (total > 512 * 1024 * 1024) break // sanity cap
    }
    const bytes = new Uint8Array(total)
    let off = 0
    for (const c of chunks) { bytes.set(c, off); off += c.length }
    const dec = new TextDecoder()
    const entries = []
    for (let pos = 0; pos + 512 <= bytes.length;) {
      const h = bytes.subarray(pos, pos + 512)
      if (h.every(b => b === 0)) break
      let name = ''
      for (let i = 0; i < 100 && h[i] !== 0; i++) name += String.fromCharCode(h[i])
      // ustar prefix field (345..500) when present (155..255 is checksum /
      // typeflag / linkname — reading it there corrupts every path)
      let prefix = ''
      for (let i = 345; i < 500 && h[i] !== 0; i++) prefix += String.fromCharCode(h[i])
      const sizeField = dec.decode(h.subarray(124, 136)).replace(/[\0 ]/g, '')
      const size = parseInt(sizeField || '0', 8) || 0
      const typeFlag = String.fromCharCode(h[156] || 48)
      const dataStart = pos + 512
      const clean = (prefix ? prefix + '/' : '') + name.replace(/^\.\//, '')
      if ((typeFlag === '0' || typeFlag === '\0') && clean && !clean.endsWith('/')) {
        entries.push({ path: clean, bytes: bytes.slice(dataStart, dataStart + size) })
      }
      pos = dataStart + Math.ceil(size / 512) * 512
    }
    return entries
  }

  /** STORE-method ZIP writer: fixed DOS timestamp, UTF-8 names, no dirs. */
  function makeZip(entries) {
    const enc = new TextEncoder()
    const now = new Date()
    const dosTime = (now.getHours() << 11) | (now.getMinutes() << 5) | (now.getSeconds() >> 1)
    const dosDate = ((now.getFullYear() - 1980) << 9) | ((now.getMonth() + 1) << 5) | now.getDate()
    const chunks = []
    const central = []
    let offset = 0
    const view = (size) => new DataView(new ArrayBuffer(size))
    for (const e of entries) {
      const nameB = enc.encode(e.path)
      const crc = crc32(e.bytes)
      const lh = view(30)
      lh.setUint32(0, 0x04034b50, true)
      lh.setUint16(4, 20, true)        // version needed
      lh.setUint16(6, 0x0800, true)    // UTF-8 names
      lh.setUint16(8, 0, true)         // stored
      lh.setUint16(10, dosTime, true)
      lh.setUint16(12, dosDate, true)
      lh.setUint32(14, crc, true)
      lh.setUint32(18, e.bytes.length, true)
      lh.setUint32(22, e.bytes.length, true)
      lh.setUint16(26, nameB.length, true)
      lh.setUint16(28, 0, true)
      chunks.push(new Uint8Array(lh.buffer), nameB, e.bytes)
      central.push({ nameB, crc, size: e.bytes.length, offset })
      offset += 30 + nameB.length + e.bytes.length
    }
    const centralStart = offset
    for (const c of central) {
      const ch = view(46)
      ch.setUint32(0, 0x02014b50, true)
      ch.setUint16(4, 20, true)
      ch.setUint16(6, 20, true)
      ch.setUint16(8, 0x0800, true)
      ch.setUint16(10, 0, true)
      ch.setUint16(12, dosTime, true)
      ch.setUint16(14, dosDate, true)
      ch.setUint32(16, c.crc, true)
      ch.setUint32(20, c.size, true)
      ch.setUint32(24, c.size, true)
      ch.setUint16(28, c.nameB.length, true)
      ch.setUint32(42, c.offset, true)
      chunks.push(new Uint8Array(ch.buffer), c.nameB)
      offset += 46 + c.nameB.length
    }
    const eocd = view(22)
    eocd.setUint32(0, 0x06054b50, true)
    eocd.setUint16(8, central.length, true)
    eocd.setUint16(10, central.length, true)
    eocd.setUint32(12, offset - centralStart, true)
    eocd.setUint32(16, centralStart, true)
    chunks.push(new Uint8Array(eocd.buffer))
    return new Blob(chunks, { type: 'application/zip' })
  }

  /** Fetch one pack's tgz and return its file entries. */
  async function packEntries(name, version) {
    const res = await fetch(`/tarballs/${encodeURIComponent(name)}-${encodeURIComponent(version)}.tgz`)
    if (!res.ok) throw new Error(`tarball ${name}@${version}: HTTP ${res.status}`)
    return tarEntries(res.body.pipeThrough(new DecompressionStream('gzip')))
  }

  /** Rewrite a tarball's manifest.json so the DOWNLOADED pack reflects the
   * market's metadata contract (author/tags/license/homepage/title/suite/
   * assets) — published tarballs predate these fields when the author's
   * manifest does not carry them. */
  function enrichManifestEntry(entry, meta) {
    if (!entry || entry.path.split('/').pop() !== 'manifest.json') return entry
    let manifest
    try { manifest = JSON.parse(new TextDecoder().decode(entry.bytes)) } catch { return entry }
    manifest.author ??= meta.author
    manifest.tags ??= meta.tags
    manifest.license ??= meta.license
    manifest.homepage ??= meta.homepage
    manifest.title ??= meta.title
    manifest.suite ??= meta.suite
    manifest.assets ??= meta.assets
    delete manifest.category // legacy single-choice field
    return { path: entry.path, bytes: new TextEncoder().encode(JSON.stringify(manifest, null, 2)) }
  }

  /** Build and download the ZIP for a pack (its files) or a suite (each
   * member's own top-level folder as published). `button` gets pending
   * outcome text when passed. */
  async function downloadZip(kind, name, button, version) {
    const label = () => { if (button) { button.dataset.orig = button.dataset.orig || button.textContent; button.textContent = t('zipping'); button.disabled = true } }
    const done = (ok, msg) => {
      if (!button) return
      button.disabled = false
      button.textContent = ok ? button.dataset.orig : `${t('zipFail')}`
      if (!ok) button.title = msg || ''
    }
    label()
    try {
      let entries = []
      let zipName
      if (kind === 'pack') {
        const { pack } = await api(`/api/packs/${encodeURIComponent(name)}`)
        const effective = version ?? pack.version
        entries = (await packEntries(pack.name, effective)).map(e => enrichManifestEntry(e, pack))
        zipName = `${pack.name}-${effective}.zip`
      } else {
        const { suite, members } = await api(`/api/suites/${encodeURIComponent(name)}`)
        // The suite's own manifest — same file layout convention as packs.
        entries.push({
          path: 'manifest.json',
          bytes: new TextEncoder().encode(JSON.stringify({
            name: suite.name,
            org: suite.org ?? null,
            version: suite.version,
            title: suite.title ?? null,
            description: suite.description ?? null,
            tags: suite.tags ?? [],
            members: suite.members ?? [],
            kind: 'suite',
          }, null, 2)),
        })
        for (const m of members.filter(m => !m.missing)) {
          // The published tarball already carries the pack's own top-level
          // folder (tar -C packDir .) — adding another prefix nested it twice.
          const meta = await api(`/api/packs/${encodeURIComponent(m.name)}`).catch(() => ({}))
          for (const e of await packEntries(m.name, m.version)) {
            entries.push(enrichManifestEntry(e, meta.pack || {}))
          }
        }
        zipName = `${suite.name}-${suite.version}.zip`
      }
      if (!entries.length) throw new Error('no entries')
      const url = URL.createObjectURL(makeZip(entries))
      const a = document.createElement('a')
      a.href = url
      a.download = zipName
      a.click()
      URL.revokeObjectURL(url)
      done(true)
    } catch (e) {
      done(false, e.message)
    }
  }

  // ---- ZIP reader + tar.gz builder (the submission round-trip) -----------
  // Submissions upload ZIPs in exactly the format the download buttons
  // produce: a root manifest.json for a suite (members in their own folders)
  // or one folder per pack. The reader walks the central directory and
  // inflates DEFLATE entries via DecompressionStream('deflate-raw'); STORE
  // entries pass through. Pack submissions still land as .tgz on the server,
  // so a reader-side pack is re-tarred and gzipped here.

  async function zipEntries(file) {
    const buf = new Uint8Array(await file.arrayBuffer())
    const dv = new DataView(buf.buffer)
    let eocd = -1
    for (let i = buf.length - 22; i >= 0 && i > buf.length - 22 - 65536; i--) {
      if (dv.getUint32(i, true) === 0x06054b50) { eocd = i; break }
    }
    if (eocd < 0) throw new Error('not a zip file')
    const count = dv.getUint16(eocd + 10, true)
    let off = dv.getUint32(eocd + 16, true)
    const dec = new TextDecoder()
    const entries = []
    for (let i = 0; i < count; i++) {
      if (dv.getUint32(off, true) !== 0x02014b50) throw new Error('corrupt zip central directory')
      const method = dv.getUint16(off + 10, true)
      const size = dv.getUint32(off + 24, true)
      const nameLen = dv.getUint16(off + 28, true)
      const extraLen = dv.getUint16(off + 30, true)
      const commentLen = dv.getUint16(off + 32, true)
      const localOff = dv.getUint32(off + 42, true)
      const name = dec.decode(buf.subarray(off + 46, off + 46 + nameLen))
      const lNameLen = dv.getUint16(localOff + 26, true)
      const lExtraLen = dv.getUint16(localOff + 28, true)
      const dataStart = localOff + 30 + lNameLen + lExtraLen
      let data = buf.subarray(dataStart, dataStart + size)
      if (method === 8) {
        const reader = new Blob([data]).stream().pipeThrough(new DecompressionStream('deflate-raw')).getReader()
        const chunks = []
        let total = 0
        for (;;) {
          const { done, value } = await reader.read()
          if (done) break
          chunks.push(value)
          total += value.length
        }
        const out = new Uint8Array(total)
        let o = 0
        for (const c of chunks) { out.set(c, o); o += c.length }
        data = out
      } else if (method !== 0) {
        throw new Error(`unsupported zip compression method ${method}`)
      }
      if (!name.endsWith('/')) entries.push({ path: name.replace(/^\.\//, ''), bytes: data.slice() })
      off += 46 + nameLen + extraLen + commentLen
    }
    return entries
  }

  /** Numeric x.y.z compare: negative when a < b. */
  function versionCompare(a, b) {
    const pa = String(a).split('.')
    const pb = String(b).split('.')
    for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
      const na = parseInt(pa[i] ?? '0', 10) || 0
      const nb = parseInt(pb[i] ?? '0', 10) || 0
      if (na !== nb) return na - nb
    }
    return 0
  }

  /** Classify a ZIP's entries: a root manifest.json with `members` is a
   * suite bundle; otherwise every top-level folder with its own
   * manifest.json is a pack bundle. */
  function detectBundle(entries) {
    const root = entries.find(e => e.path === 'manifest.json')
    if (root) {
      const manifest = JSON.parse(new TextDecoder().decode(root.bytes))
      if (Array.isArray(manifest.members)) {
        return { kind: 'suite', manifest, entries }
      }
    }
    const dirs = new Map()
    for (const e of entries) {
      const top = e.path.includes('/') ? e.path.split('/')[0] : ''
      if (!top) continue
      if (!dirs.has(top)) dirs.set(top, [])
      dirs.get(top).push(e)
    }
    const packs = []
    for (const [dir, files] of dirs) {
      const manifestEntry = files.find(e => e.path === `${dir}/manifest.json`)
      if (!manifestEntry) continue
      packs.push({ dir, files, manifest: JSON.parse(new TextDecoder().decode(manifestEntry.bytes)) })
    }
    if (packs.length > 0) return { kind: 'packs', packs, entries }
    if (root) return { kind: 'pack', manifest: JSON.parse(new TextDecoder().decode(root.bytes)), entries }
    return null
  }

  /** Entries → ustar → gzip Blob (the server's submission archive shape). */
  async function makeTarGz(entries) {
    const enc = new TextEncoder()
    const parts = []
    for (const e of entries) {
      let name = e.path.replace(/^\.\//, '')
      const content = e.bytes
      const header = new Uint8Array(512)
      let nameField = name
      let prefixField = ''
      if (name.length > 100) {
        const slash = name.lastIndexOf('/', 99)
        if (slash < 0) throw new Error(`tar path too long: ${name}`)
        prefixField = name.slice(0, slash)
        nameField = name.slice(slash + 1)
      }
      header.set(enc.encode(nameField), 0)
      header.set(enc.encode(prefixField), 345)
      header.set(enc.encode('000644 \0'), 100) // mode
      header.set(enc.encode(String(content.length.toString(8)).padStart(11, '0') + ' '), 124)
      // checksum: spaces placeholder, then octal over the raw header
      header.set(enc.encode('        '), 148)
      let sum = 0
      for (const b of header) sum += b
      header.set(enc.encode(sum.toString(8).padStart(6, '0') + '\0 '), 148)
      const pad = (512 - content.length % 512) % 512
      const body = new Uint8Array(content.length + pad)
      body.set(content)
      parts.push(header, body)
    }
    parts.push(new Uint8Array(1024))
    const tar = new Blob(parts)
    return new Response(tar.stream().pipeThrough(new CompressionStream('gzip'))).blob()
  }

  /** Submit-side status of one code@version against the published catalog. */
  async function versionStatus(code, version) {
    const current = await api(`/api/packs/${encodeURIComponent(code)}`).then(p => p.pack?.version).catch(() => null)
    const cmp = current === null || current === undefined ? null : versionCompare(version, current)
    return { current, state: cmp === null ? 'new' : cmp > 0 ? 'update' : cmp === 0 ? 'unchanged' : 'tooLow' }
  }

  return { loadConfig, session, api, t, lang, setLang, applyI18n, header, localizedDesc, displayTitle, esc, fmtDateTime, timeZoneId, setTimeZone, renderTimeZonePicker, inDsh, requestInstall, mountInstallButton, mountCardInstallButton, maskEmail, downloadZip, tarEntries, makeZip, zipEntries, makeTarGz, detectBundle, versionCompare, versionStatus, get supabase() { return supabase } }
})()
