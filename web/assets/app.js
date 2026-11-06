// docwright pack market — shared browser helpers (vanilla JS, no build step).
// Exposes window.MB: config/supabase bootstrap, i18n, authed fetch helpers.
window.MB = (() => {
  let config = null
  let supabase = null

  const I18N = {
    en: {
      browse: 'Browse',
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
      queue: 'Pending review queue',
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
    },
    zh: {
      browse: '浏览',
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
      queue: '待审核队列',
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
    },
  }

  function lang() {
    return localStorage.getItem('dw-market-lang') || 'zh'
  }
  function setLang(l) {
    localStorage.setItem('dw-market-lang', l)
    applyI18n()
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

  function header(active) {
    const el = document.createElement('header')
    el.className = 'site'
    el.innerHTML = `
      <a class="brand" href="/">docwright pack market</a>
      <nav>
        <a href="/" data-i18n="browse"></a>
        <a href="/submit.html" data-i18n="submit"></a>
        <a href="/admin.html" data-i18n="admin"></a>
        <span class="spacer"></span>
        <span id="auth-slot"></span>
        <button id="lang-btn" class="lang-btn" title="switch language"></button>
      </nav>`
    document.body.prepend(el)
    el.querySelector('#lang-btn').addEventListener('click', () => setLang(lang() === 'zh' ? 'en' : 'zh'))
    applyI18n()
    refreshAuthSlot(active)
  }

  async function refreshAuthSlot(active) {
    const slot = document.getElementById('auth-slot')
    if (!slot) return
    const s = await session().catch(() => null)
    if (s) {
      slot.innerHTML = `<span class="muted">${s.user.email}</span> <a href="#" id="logout-a" data-i18n="logout"></a>`
      slot.querySelector('#logout-a').addEventListener('click', async e => {
        e.preventDefault()
        await supabase.auth.signOut()
        location.reload()
      })
      applyI18n()
    } else {
      slot.innerHTML = `<a href="${active === 'submit' ? '#' : '/submit.html'}" data-i18n="login"></a>`
      applyI18n()
    }
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

  return { loadConfig, session, api, t, lang, setLang, applyI18n, header, localizedDesc, esc, get supabase() { return supabase } }
})()
