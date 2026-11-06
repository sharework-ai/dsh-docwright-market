// HTTP-layer tests for POST /verify and GET /health. The real docwright
// round-trip (pnpm pack:check) is covered end-to-end by scripts/e2e.sh; here
// the fixture docwright is a stub, so green packs are expected to fail the
// check stage — what these tests pin is endpoint behavior and error mapping.
import { test, afterEach } from 'vitest'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import { mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

const fixtureRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-http-docwright-'))
await fs.mkdir(path.join(fixtureRoot, 'packs', 'cosmic-func'), { recursive: true })
await fs.writeFile(
  path.join(fixtureRoot, 'package.json'),
  JSON.stringify({ scripts: { 'pack:check': 'exit 1' } }),
)
process.env.DOCWRIGHT_ROOT = fixtureRoot
process.env.VERIFIER_PORT = '0'
// Small upload cap so the too-large case needs no 50 MiB payload.
process.env.VERIFIER_MAX_BYTES = '1024'
process.env.VERIFIER_TMP = path.join(fixtureRoot, 'tmp')

const { start } = await import('../index.mjs')

let server
afterEach(async () => {
  if (server) { server.close(); server = undefined }
})

async function baseUrl() {
  server = await start(0)
  return `http://127.0.0.1:${server.address().port}`
}

function makeTgz() {
  const base = `/tmp/verifier-http-${process.pid}-${Math.random().toString(36).slice(2)}`
  mkdirSync(base, { recursive: true })
  const src = path.join(base, 'task-card')
  mkdirSync(src, { recursive: true })
  writeFileSync(path.join(src, 'manifest.json'), JSON.stringify({ name: 'task-card', version: '0.1.0' }))
  const tgz = path.join(base, 'a.tgz')
  execFileSync('tar', ['czf', tgz, '-C', base, 'task-card'])
  const bytes = readFileSync(tgz)
  rmSync(base, { recursive: true, force: true })
  return bytes
}

test('GET /health reports the configured docwright root', async () => {
  const base = await baseUrl()
  const res = await fetch(`${base}/health`)
  assert.equal(res.status, 200)
  const body = await res.json()
  assert.equal(body.ok, true)
  assert.equal(body.docwrightRoot, fixtureRoot)
})

test('unknown routes return a JSON 404', async () => {
  const base = await baseUrl()
  const res = await fetch(`${base}/nope`)
  assert.equal(res.status, 404)
  assert.equal((await res.json()).ok, false)
})

test('POST /verify with an empty body is a 400 with a hint', async () => {
  const base = await baseUrl()
  const res = await fetch(`${base}/verify`, { method: 'POST' })
  assert.equal(res.status, 400)
  const body = await res.json()
  assert.equal(body.ok, false)
  assert.match(body.error, /empty body/)
})

test('POST /verify rejects bodies over the size cap', async () => {
  const base = await baseUrl()
  const res = await fetch(`${base}/verify`, {
    method: 'POST',
    body: Buffer.alloc(2048, 1),
  })
  assert.equal(res.status, 400)
  assert.match((await res.json()).error, /exceeds/)
})

test('POST /verify maps non-tarball bytes to a 400', async () => {
  const base = await baseUrl()
  const res = await fetch(`${base}/verify`, { method: 'POST', body: Buffer.from('not gzip at all') })
  assert.equal(res.status, 400)
  assert.match((await res.json()).error, /not a readable gzip tarball/)
})

test('POST /verify runs the pipeline for a green-shaped pack (check fails on the stub docwright)', async () => {
  const base = await baseUrl()
  const res = await fetch(`${base}/verify`, { method: 'POST', body: makeTgz() })
  assert.equal(res.status, 200)
  const { ok, report } = await res.json()
  // Shape and identity pass; the golden round fails because the fixture
  // docwright has no real pack:check — that integration lives in e2e.
  assert.equal(report.shapeOk, true)
  assert.equal(report.packName, 'task-card')
  assert.equal(report.version, '0.1.0')
  assert.equal(report.checkGreen, false)
  assert.equal(ok, false)
  assert.ok(report.checkOutput.length > 0, 'the failing check must surface its output')
})
