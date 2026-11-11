// Web-asset unit tests: web/assets/app.js is a browser script, so these
// tests evaluate it inside a minimal DOM/localStorage sandbox and drive the
// pure helpers — date/time formatting with the locale+timezone pairing, the
// ?lang=&tz= entry-parameter application, and the idempotent header (the
// double-header regression).
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { fileURLToPath } from 'node:url'

const APP_JS = readFileSync(fileURLToPath(new URL('../web/assets/app.js', import.meta.url)), 'utf8')

/** Evaluate app.js under browser shims and return the exposed MB object plus
 * the sandbox handles a test wants to inspect (storage contents, history
 * rewrites, prepended headers). */
function loadAppBrowser({ search = '', storage = {} } = {}) {
  const store = { ...storage }
  const headerElements = []
  let removedHeaders = 0
  let replacedUrl = null
  const fakeElement = () => {
    const el = {
      _html: '',
      className: '',
      value: '',
      textContent: '',
      classList: { add() {}, remove() {}, toggle() {} },
      querySelector: () => fakeElement(),
      querySelectorAll: () => [],
      addEventListener() {},
      prepend() {},
      remove() { removedHeaders += 1 },
    }
    Object.defineProperty(el, 'innerHTML', {
      get() { return el._html },
      set(v) { el._html = v },
    })
    return el
  }
  const document = {
    querySelector(selector) {
      if (selector === 'header.site') {
        // Simulate an existing header only after the first prepend.
        return headerElements.length > 0 ? { remove() { removedHeaders += 1 } } : null
      }
      return null
    },
    querySelectorAll: () => [],
    getElementById: () => null,
    createElement: fakeElement,
    body: { prepend: el => headerElements.push(el) },
  }
  const sandbox = {
    localStorage: {
      getItem: k => (k in store ? store[k] : null),
      setItem: (k, v) => { store[k] = String(v) },
    },
    location: { search, pathname: '/', hash: '' },
    navigator: { language: 'en-US' },
    history: { replaceState: (_a, _b, url) => { replacedUrl = url } },
    document,
  }
  const fn = new Function('window', 'localStorage', 'location', 'navigator', 'history', 'document', APP_JS)
  fn(sandbox, sandbox.localStorage, sandbox.location, sandbox.navigator, sandbox.history, document)
  return { MB: sandbox.MB, store, headerElements, replacedUrl, removedCount: () => removedHeaders }
}

describe('MB.fmtDateTime (locale + timezone pairing)', () => {
  const iso = '2026-11-11T13:50:40Z'

  it('renders China Standard Time in the zh-CN format', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-tz': 'Asia/Shanghai' } })
    const text = MB.fmtDateTime(iso)
    expect(text).toContain('2026')
    expect(text).toContain('21:50')
  })

  it('renders UK time (en-GB, GMT in November) when London is selected', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-tz': 'Europe/London' } })
    const text = MB.fmtDateTime(iso)
    expect(text).toContain('13:50')
    expect(text).toMatch(/11 Nov|11\/11/)
  })

  it('renders US Eastern time (EST in November) when New York is selected', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-tz': 'America/New_York' } })
    const text = MB.fmtDateTime(iso)
    expect(text).toMatch(/8:50/)
    expect(text).toContain('AM')
  })

  it('auto mode follows the interface language for the locale and the browser zone for offset', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-lang': 'zh' } })
    const text = MB.fmtDateTime(iso)
    // zh locale names the date regardless of zone; the exact clock time
    // depends on the machine running the test, so only assert the year.
    expect(text).toContain('2026')
  })

  it('passes null/invalid input through harmlessly', () => {
    const { MB } = loadAppBrowser({})
    expect(MB.fmtDateTime(null)).toBe('—')
    expect(MB.fmtDateTime('')).toBe('—')
    expect(MB.fmtDateTime('not-a-date')).toBe('not-a-date')
  })
})

describe('entry query strings (?lang= &tz=)', () => {
  it('persists both values and strips them from the address bar', () => {
    const { store, replacedUrl } = loadAppBrowser({ search: '?lang=en&tz=Europe/London' })
    expect(store['dw-market-lang']).toBe('en')
    expect(store['dw-market-tz']).toBe('Europe/London')
    expect(replacedUrl).toBe('/')
  })

  it('keeps unrelated query parameters when stripping', () => {
    const { replacedUrl } = loadAppBrowser({ search: '?name=cosmic&lang=zh' })
    expect(replacedUrl).toBe('/?name=cosmic')
  })

  it('ignores unknown lang and tz values without touching storage', () => {
    const { store, replacedUrl } = loadAppBrowser({ search: '?lang=fr&tz=Mars/Olympus' })
    expect(store['dw-market-lang']).toBeUndefined()
    expect(store['dw-market-tz']).toBeUndefined()
    expect(replacedUrl).toBe('/')
  })

  it('accepts tz=auto explicitly', () => {
    const { store } = loadAppBrowser({ search: '?tz=auto' })
    expect(store['dw-market-tz']).toBe('auto')
  })

  it('does nothing when no relevant parameters are present', () => {
    const { replacedUrl } = loadAppBrowser({ search: '?name=cosmic' })
    expect(replacedUrl).toBeNull()
  })
})

describe('header idempotency (double-header regression)', () => {
  it('removes the previous header on every subsequent call', () => {
    const { MB, headerElements, removedCount } = loadAppBrowser({})
    MB.header()
    expect(headerElements).toHaveLength(1)
    expect(removedCount()).toBe(0) // first call: nothing to replace
    MB.header()
    MB.header()
    expect(headerElements).toHaveLength(3) // one element per call…
    expect(removedCount()).toBe(2) // …but each previous one was removed first
  })
})

describe('timezone picker (account page)', () => {
  it('renders the four choices and honors the persisted selection', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-tz': 'America/New_York' } })
    const options = []
    const select = {
      addEventListener() {},
      set innerHTML(html) {
        options.push(...[...html.matchAll(/value="([^"]+)"/g)].map(m => m[1]))
      },
    }
    MB.renderTimeZonePicker(select)
    expect(options).toEqual(['auto', 'Asia/Shanghai', 'Europe/London', 'America/New_York'])
    expect(MB.timeZoneId()).toBe('America/New_York')
  })

  it('falls back to auto for an unknown persisted value', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-tz': 'Mars/Olympus' } })
    expect(MB.timeZoneId()).toBe('auto')
  })
})
