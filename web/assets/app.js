// docwright pack market — shared browser helpers (vanilla JS, no build step).
// Exposes window.MB: config/supabase bootstrap, i18n, authed fetch helpers.
window.MB = (() => {
  let config = null
  let supabase = null

  const I18N = {
    en: {
      browse: 'Market',
      packs: 'Packs',
      submit: 'Submit a pack',
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
      category: 'Category',
      license: 'License (SPDX)',
      homepage: 'Homepage (optional)',
      send: 'Submit',
      verifying: 'Verifying…',
      accepted: 'Accepted into the review queue.',
      rejectedMachine: 'Verification failed — fix the report below and resubmit.',
      status: 'Status',
      refresh: 'Refresh status',
      tabPack: 'Pack — upload archive',
      tabSuite: 'Suite — form',
      suiteSubmitHint: 'A suite is pure metadata: NO archive to upload — just the member list. Every member must already be a published pack. The submission enters the same review queue as packs.',
      suiteQueued: 'Suite submission accepted — it enters the review queue.',
      trackInPacks: 'Track it on the Packs page',
            queue: 'Pending review queue',
      tzTitle: 'Date & time zone',
      tzHint: 'Applies to every date shown on the market pages.',
      tzLabel: 'Time zone',
            tzAuto: 'Time zone: browser default',
      'tz_Asia/Shanghai': 'China Standard Time (zh-CN)',
      'tz_Europe/London': 'UK time (en-GB)',
      'tz_America/New_York': 'US Eastern time (en-US)',
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
      orgEditHint: 'Edit org on the account page',
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
      myPacks: 'My packs',
      mySubmissions: 'My submissions',
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
      mySuites: 'My suites',
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
    },
    zh: {
      browse: '市场',
      packs: '我的 Packs',
      submit: '投稿 pack',
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
      category: '分类',
      license: '许可证(SPDX)',
      homepage: '主页(可选)',
      send: '提交',
      verifying: '校验中…',
      accepted: '已进入审核队列。',
      rejectedMachine: '机器校验未通过——按下方面板修复后重新提交。',
      status: '状态',
      refresh: '刷新状态',
      tabPack: 'Pack — 上传压缩包',
      tabSuite: 'Suite — 表单',
      suiteSubmitHint: '套件是纯元数据：不需要上传压缩包，只需成员名单。每个成员必须已是已上架的 pack。投稿进入与 pack 相同的审核队列。',
      suiteQueued: '套件投稿已受理——进入审核队列。',
      trackInPacks: '到 Packs 页跟踪进度',
            queue: '待审核队列',
      tzTitle: '日期与时区',
      tzHint: '作用于市场各页面展示的所有日期时间。',
      tzLabel: '时区',
            tzAuto: '时区：跟随浏览器',
      'tz_Asia/Shanghai': '中国标准时间（中文格式）',
      'tz_Europe/London': '英国时间（英文格式）',
      'tz_America/New_York': '美国东部时间（英文格式）',
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
      orgEditHint: '到账号页编辑组织',
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
      myPacks: '我的 pack',
      mySubmissions: '我的投稿',
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
      mySuites: '我的套件',
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
    },
  }

  function lang() {
    return localStorage.getItem('dw-market-lang') || 'zh'
  }
  function setLang(l) {
    localStorage.setItem('dw-market-lang', l)
    applyI18n()
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
    const tz = TIME_ZONES.find(t => t.id === timeZoneId())
    const locale = tz.localeTag ?? (lang() === 'zh' ? 'zh-CN' : navigator.language || 'en')
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
      <a class="brand" href="/">docwright pack market</a>
      <nav>
        <a href="/" data-i18n="browse"></a>
        <a href="/packs.html" data-i18n="packs" class="hidden" id="nav-packs"></a>
        <a href="/admin.html" data-i18n="admin" class="hidden" id="nav-admin"></a>
        <span class="spacer"></span>
        <span id="auth-slot"></span>
        <button id="lang-btn" class="lang-btn" title="switch language"></button>
      </nav>`
    document.body.prepend(el)
    el.querySelector('#lang-btn').addEventListener('click', () => setLang(lang() === 'zh' ? 'en' : 'zh'))
    applyI18n()
    refreshAuthSlot()
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
    slot.innerHTML = `<a href="/account.html" class="auth-email">${esc(s.user.email)}</a> <a href="#" id="logout-a" data-i18n="logout"></a>`
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

  return { loadConfig, session, api, t, lang, setLang, applyI18n, header, localizedDesc, esc, fmtDateTime, timeZoneId, setTimeZone, renderTimeZonePicker, get supabase() { return supabase } }
})()
