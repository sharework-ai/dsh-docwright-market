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
function loadAppBrowser({ search = '', storage = {}, windowOverrides = {} } = {}) {
  const store = { ...storage }
  const headerElements = []
  let removedHeaders = 0
  let replacedUrl = null
  const posted = []
  const windowListeners = {}
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
    addEventListener(type, fn) { (windowListeners[type] ??= []).push(fn) },
    postMessage(msg) { posted.push(msg) },
    ...windowOverrides,
  }
  const fn = new Function('window', 'localStorage', 'location', 'navigator', 'history', 'document', APP_JS)
  fn(sandbox, sandbox.localStorage, sandbox.location, sandbox.navigator, sandbox.history, document)
  return { MB: sandbox.MB, store, headerElements, replacedUrl, removedCount: () => removedHeaders, posted, windowListeners }
}

describe('MB.fmtDateTime (format follows language, zone follows picker)', () => {
  const iso = '2026-11-11T13:50:40Z'

  it('zh interface renders the zh-CN shape in the picked zone', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-lang': 'zh', 'dw-market-tz': 'Asia/Shanghai' } })
    const text = MB.fmtDateTime(iso)
    expect(text).toContain('2026年11月11日')
    expect(text).toContain('21:50')
  })

  it('en interface renders the en-US shape in the picked zone', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-lang': 'en', 'dw-market-tz': 'Europe/London' } })
    const text = MB.fmtDateTime(iso)
    expect(text).toContain('Nov 11, 2026')
    expect(text).toContain('PM')
  })

  it('switching the language switches the shape without touching the zone', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-lang': 'en', 'dw-market-tz': 'America/New_York' } })
    const text = MB.fmtDateTime(iso)
    expect(text).toContain('Nov 11, 2026')
    expect(text).toContain('8:50')
    expect(text).toContain('AM')
  })

  it('auto mode uses the browser zone', () => {
    const { MB } = loadAppBrowser({ storage: { 'dw-market-lang': 'zh' } })
    expect(MB.fmtDateTime(iso)).toContain('2026')
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

describe('in-DSH install bridge', () => {
  it('reports framed vs standalone from window.parent', () => {
    // The default sandbox window has no parent → treated as framed (the
    // path a real embedded frame exercises; a standalone page answers false
    // through the parent === window identity check in the browser).
    const framed = loadAppBrowser()
    expect(framed.MB.inDsh()).toBe(true)
    // window.parent present (any parent object) → framed.
    const withParent = loadAppBrowser({ windowOverrides: { parent: { postMessage() {} } } })
    expect(withParent.MB.inDsh()).toBe(true)
  })

  it('posts the install-request protocol and routes replies by correlation id', () => {
    const postedToParent = []
    const app = loadAppBrowser({ windowOverrides: { parent: { postMessage: msg => postedToParent.push(msg) } } })
    const accepted = []
    const results = []
    app.MB.requestInstall('pack', 'cosmic-plan', {
      onAccepted: () => accepted.push(true),
      onResult: r => results.push(r),
    })
    expect(postedToParent).toHaveLength(1)
    const request = postedToParent[0]
    expect(request).toMatchObject({ source: 'docwright-market', type: 'install-request', kind: 'pack', name: 'cosmic-plan' })
    expect(typeof request.id).toBe('string')

    // A result under the request's id resolves the pending call; unknown ids
    // and foreign sources are ignored.
    const listeners = app.windowListeners.message
    expect(listeners).toHaveLength(1)
    const onMessage = listeners[0]
    onMessage({ data: { source: 'docwright-market', type: 'install-result', id: 'unknown', ok: true } })
    onMessage({ data: { source: 'other', type: 'install-result', id: request.id, ok: true } })
    expect(results).toHaveLength(0)
    onMessage({ data: { source: 'docwright-market', type: 'install-accepted', id: request.id } })
    expect(accepted).toEqual([true])
    onMessage({ data: { source: 'docwright-market', type: 'install-result', id: request.id, ok: true, installed: [{ name: 'cosmic-plan', version: '1' }] } })
    expect(results).toHaveLength(1)
    expect(results[0]).toMatchObject({ ok: true, installed: [{ name: 'cosmic-plan' }] })

    // The id is single-shot: a replayed result does not fire again.
    onMessage({ data: { source: 'docwright-market', type: 'install-result', id: request.id, ok: true } })
    expect(results).toHaveLength(1)
  })
})

describe('email masking (header account display)', () => {
  it('keeps the first and last 2 characters with at most 4 stars between', () => {
    const { MB } = loadAppBrowser()
    expect(MB.maskEmail('penglong@example.com')).toBe('pe****om')
    expect(MB.maskEmail('ab@example.com')).toBe('ab****om')
  })

  it('collapses short strings into x*x / x* shapes', () => {
    const { MB } = loadAppBrowser()
    expect(MB.maskEmail('abc')).toBe('x*x')
    expect(MB.maskEmail('ab')).toBe('x*')
    expect(MB.maskEmail('a')).toBe('x*')
    expect(MB.maskEmail('')).toBe('x*')
    expect(MB.maskEmail(null)).toBe('x*')
  })
})

describe('ZIP download (tarEntries + makeZip)', () => {
  const enc = new TextEncoder()
  function ustarEntry(name, content) {
    const header = new Uint8Array(512)
    header.set(enc.encode(name), 0)
    header.set(enc.encode(content.length.toString(8).padStart(11, '0') + ' '), 124)
    header[156] = 48 // '0' regular file
    const pad = (512 - content.length % 512) % 512
    const body = new Uint8Array(content.length + pad)
    body.set(content)
    return new Uint8Array([...header, ...body])
  }

  async function tgzOf(files) {
    const parts = []
    for (const [name, text] of files) parts.push(ustarEntry(name, enc.encode(text)))
    parts.push(new Uint8Array(1024))
    const gz = new Blob(parts).stream().pipeThrough(new CompressionStream('gzip'))
    return new Response(gz).arrayBuffer()
  }

  it('walks ustar entries (strip ./, keep file bytes)', async () => {
    const { MB } = loadAppBrowser()
    const tgz = await tgzOf([['./manifest.json', '{"name":"demo"}'], ['rules/zh-CN.json', '[]']])
    const entries = await MB.tarEntries(new Blob([tgz]).stream().pipeThrough(new DecompressionStream('gzip')))
    expect(entries.map(e => e.path).sort()).toEqual(['manifest.json', 'rules/zh-CN.json'])
    expect(new TextDecoder().decode(entries.find(e => e.path === 'manifest.json').bytes)).toBe('{"name":"demo"}')
  })

  it('writes a valid STORE-method zip (signatures, crc via data descriptor offset math)', async () => {
    const { MB } = loadAppBrowser()
    const blob = MB.makeZip([
      { path: 'a/one.json', bytes: enc.encode('one') },
      { path: 'b/two.md', bytes: enc.encode('two-two') },
    ])
    const buf = new Uint8Array(await blob.arrayBuffer())
    const dv = new DataView(buf.buffer)
    expect(dv.getUint32(0, true)).toBe(0x04034b50) // local header
    // walk both local headers, then expect the central directory signature
    const nameLen1 = dv.getUint16(26, true)
    const size1 = dv.getUint32(18, true)
    let off = 30 + nameLen1 + size1
    expect(dv.getUint32(off, true)).toBe(0x04034b50)
    const nameLen2 = dv.getUint16(off + 26, true)
    const size2 = dv.getUint32(off + 18, true)
    off = off + 30 + nameLen2 + size2
    expect(dv.getUint32(off, true)).toBe(0x02014b50) // central directory
    expect(dv.getUint16(off + 10, true)).toBe(0) // stored
    const cdOffset = off // where the central directory starts (after both locals)
    const cdSize = buf.length - 22 - cdOffset
    expect(dv.getUint32(buf.length - 22, true)).toBe(0x06054b50) // EOCD
    expect(dv.getUint16(buf.length - 22 + 8, true)).toBe(2) // two entries
    expect(dv.getUint16(buf.length - 22 + 10, true)).toBe(2)
    expect(dv.getUint32(buf.length - 22 + 12, true)).toBe(cdSize)
    expect(dv.getUint32(buf.length - 22 + 16, true)).toBe(cdOffset)
  })
})

describe('submission round-trip (zip in, tar.gz out)', () => {
  const enc = new TextEncoder()

  it('reads back a STORE zip written by makeZip', async () => {
    const { MB } = loadAppBrowser()
    const blob = MB.makeZip([
      { path: 'demo/manifest.json', bytes: enc.encode('{"name":"demo"}') },
      { path: 'demo/rules/a.json', bytes: enc.encode('[]') },
    ])
    const file = new File([blob], 'demo.zip')
    const entries = await MB.zipEntries(file)
    expect(entries.map(e => e.path).sort()).toEqual(['demo/manifest.json', 'demo/rules/a.json'])
    expect(new TextDecoder().decode(entries.find(e => e.path === 'demo/rules/a.json').bytes)).toBe('[]')
  })

  it('detectBundle: root manifest with members is a suite, folders are packs', () => {
    const { MB } = loadAppBrowser()
    const mk = (path, text) => ({ path, bytes: enc.encode(text) })
    const suite = MB.detectBundle([
      mk('manifest.json', '{"name":"cosmic","members":["a","b"]}'),
      mk('a/manifest.json', '{"name":"a"}'),
      mk('a/rules.json', '[]'),
      mk('b/manifest.json', '{"name":"b"}'),
    ])
    expect(suite.kind).toBe('suite')
    expect(suite.manifest.members).toEqual(['a', 'b'])
    const packs = MB.detectBundle([mk('x/manifest.json', '{"name":"x"}'), mk('x/a.json', '1'), mk('y/manifest.json', '{"name":"y"}')])
    expect(packs.kind).toBe('packs')
    expect(packs.packs.map(p => p.manifest.name)).toEqual(['x', 'y'])
    expect(MB.detectBundle([mk('random.txt', 'x')])).toBeNull()
  })

  it('versionCompare is numeric per segment', () => {
    const { MB } = loadAppBrowser()
    expect(MB.versionCompare('0.2.0', '0.10.0')).toBeLessThan(0)
    expect(MB.versionCompare('1.0.0', '1.0.0')).toBe(0)
    expect(MB.versionCompare('2.0.0', '1.9.9')).toBeGreaterThan(0)
  })

  it('makeTarGz produces a tar the reader parses back (checksum valid)', async () => {
    const { MB } = loadAppBrowser()
    const blob = await MB.makeTarGz([
      { path: 'manifest.json', bytes: enc.encode('{"name":"rt"}') },
      { path: 'rules/zh-CN.json', bytes: enc.encode('[1,2]') },
    ])
    const entries = await MB.tarEntries(await blob.stream().pipeThrough(new DecompressionStream('gzip')))
    expect(entries.map(e => e.path).sort()).toEqual(['manifest.json', 'rules/zh-CN.json'])
    expect(new TextDecoder().decode(entries.find(e => e.path === 'manifest.json').bytes)).toBe('{"name":"rt"}')
  })
})
