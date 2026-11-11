#!/usr/bin/env node
// Submit pack tarballs to the docwright pack market from the command line.
//
// The submission and review flow needs a logged-in account (this script is
// the same REST the web UI drives): credentials arrive via environment
// variables ONLY — nothing is read from or written to disk.
//
//   MARKET_URL=https://dm.shrwk.com \
//   MARKET_EMAIL=you@example.com MARKET_PASSWORD=... \
//   node scripts/submit-pack.mjs packs-out/cosmic-plan.tgz \
//     --author peng_long --category estimation --license MIT [--approve]
//
// Steps: password login via the shared Supabase GoTrue (config.json names
// its URL and publishable key), multipart POST /api/submissions (the server
// runs the verifier inline), optional POST /api/admin/review/{id} approve
// (admin account), then a catalog smoke read.
// With no file argument, --pack <name> fetches the pack's archive from the
// market's own catalog and resubmits it (republish lane).

import { readFile } from 'node:fs/promises'
import process from 'node:process'

const usage = `usage: node scripts/submit-pack.mjs <archive.tgz> [--author NAME] [--category CAT]
                             [--license SPDX] [--homepage URL] [--approve]
       env: MARKET_URL, MARKET_EMAIL, MARKET_PASSWORD`

const argv = process.argv.slice(2)
const file = argv.find(a => !a.startsWith('--'))
const flag = name => {
  const i = argv.indexOf(name)
  return i === -1 ? undefined : argv[i + 1]
}
const author = flag('--author')
const category = flag('--category') ?? 'other'
const license = flag('--license') ?? 'MIT'
const homepage = flag('--homepage')
const approve = argv.includes('--approve')

const MARKET_URL = (process.env.MARKET_URL ?? 'https://dm.shrwk.com').replace(/\/$/, '')
const EMAIL = process.env.MARKET_EMAIL
const PASSWORD = process.env.MARKET_PASSWORD
if (!EMAIL || !PASSWORD) {
  console.error('MARKET_EMAIL and MARKET_PASSWORD must be set (env only, never stored)')
  console.error(usage)
  process.exit(1)
}
if (!file) {
  console.error(usage)
  process.exit(1)
}

const die = message => {
  console.error(`submit-pack: ${message}`)
  process.exit(1)
}

// 1. Config names the shared Supabase (browser face) and its publishable key.
const config = await fetch(`${MARKET_URL}/config.json`).then(r => {
  if (!r.ok) die(`config.json -> HTTP ${r.status}`)
  return r.json()
})

// 2. Password login (GoTrue REST — the same call supabase-js makes).
const login = await fetch(`${config.supabaseUrl}/auth/v1/token?grant_type=password`, {
  method: 'POST',
  headers: { apikey: config.supabasePublishableKey, 'content-type': 'application/json' },
  body: JSON.stringify({ email: EMAIL, password: PASSWORD }),
}).then(async r => ({ status: r.status, body: await r.json().catch(() => ({})) }))
if (login.status !== 200) die(`login failed (HTTP ${login.status}): ${JSON.stringify(login.body).slice(0, 300)}`)
const token = login.body.access_token
console.log(`logged in as ${EMAIL}`)

// 3. Submit the archive (the server runs the verifier inline).
const archive = await readFile(file)
const form = new FormData()
form.append('archive', new Blob([archive], { type: 'application/gzip' }), file.split('/').pop())
form.append('author', author ?? EMAIL.split('@')[0])
form.append('category', category)
form.append('license', license)
if (homepage !== undefined) form.append('homepage', homepage)

const submitted = await fetch(`${MARKET_URL}/api/submissions`, {
  method: 'POST',
  headers: { authorization: `Bearer ${token}` },
  body: form,
}).then(async r => ({ status: r.status, body: await r.json().catch(() => ({})) }))

if (submitted.body.ok !== true) {
  console.error(`submission rejected (HTTP ${submitted.status}):`)
  console.error(JSON.stringify(submitted.body, null, 2))
  process.exit(1)
}
const id = submitted.body.id
const report = submitted.body.report ?? {}
console.log(`submitted ${report.packName}@${report.version} -> submission ${id} (verifier green)`)

// 4. Approve (admin) — the review flow the web UI drives.
if (approve) {
  const reviewed = await fetch(`${MARKET_URL}/api/admin/review/${id}`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ action: 'approve', note: 'auto-approved via submit-pack.mjs' }),
  }).then(async r => ({ status: r.status, body: await r.json().catch(() => ({})) }))
  if (reviewed.body.ok !== true) {
    console.error(`review failed (HTTP ${reviewed.status}): ${JSON.stringify(reviewed.body).slice(0, 400)}`)
    process.exit(1)
  }
  console.log(`approved: ${reviewed.body.pack}`)
}

// 5. Catalog smoke: the pack must now be listed with the submitted version.
const detail = await fetch(`${MARKET_URL}/api/packs/${report.packName}`).then(r => r.json())
const versions = (detail.versions ?? []).map(v => v.version)
console.log(`catalog: ${detail.pack?.name}@${detail.pack?.version} versions=[${versions.join(', ')}]`)
if (detail.pack?.version !== report.version) {
  console.error('catalog does not serve the submitted version yet')
  process.exit(1)
}
console.log('done.')
