// install.mjs behavior tests: spawn the CLI against a local fake market
// (index + tarball server) and pin exit codes, landed files, and integrity
// rejections. Integrity values are computed from the real archive bytes, so
// a passing run proves the whole verify-then-copy chain and doctored index
// values prove the chain actually rejects.
import { test, beforeAll, afterAll } from 'vitest'
import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { readFileSync } from 'node:fs'
import http from 'node:http'
import os from 'node:os'
import path from 'node:path'

const REPO = process.cwd() // vitest run always starts at the repo root
const INSTALL = path.join(REPO, 'install.mjs')

let tgzBytes
let packHash
let index
let server
let baseUrl
let targetsRoot

function sha256(bytes) {
  return `sha256:${createHash('sha256').update(bytes).digest('hex')}`
}

/** Directory hash identical to install.mjs's own algorithm (sorted path+bytes). */
async function dirHash(dir) {
  const files = []
  async function walk(current) {
    for (const dirent of (await fs_readdirSorted(current))) {
      const full = path.join(current, dirent.name)
      if (dirent.isDirectory()) await walk(full)
      else files.push(full)
    }
  }
  await walk(dir)
  const hash = createHash('sha256')
  for (const file of files.sort()) {
    hash.update(path.relative(dir, file).split(path.sep).join('/'))
    hash.update('\0')
    hash.update(await readFile(file))
  }
  return `sha256:${hash.digest('hex')}`
}

async function fs_readdirSorted(current) {
  const dirents = await import('node:fs').then(fs => fs.promises.readdir(current, { withFileTypes: true }))
  return dirents
}

beforeAll(async () => {
  // A real docwright pack archive: build one from files on disk.
  const src = await mkdtemp(path.join(os.tmpdir(), 'install-test-src-'))
  const packDir = path.join(src, 'fixture-pack')
  await mkdir(packDir)
  await writeFile(path.join(packDir, 'manifest.json'), JSON.stringify({ name: 'fixture-pack', version: '0.1.0' }))
  await writeFile(path.join(packDir, 'schema.json'), '{}')
  const tgzPath = path.join(src, 'pack.tgz')
  spawnSync('tar', ['czf', tgzPath, '-C', src, 'fixture-pack'])
  tgzBytes = await readFile(tgzPath)

  const unpack = await mkdtemp(path.join(os.tmpdir(), 'install-test-hash-'))
  spawnSync('tar', ['xzf', tgzPath, '-C', unpack])
  packHash = await dirHash(path.join(unpack, 'fixture-pack'))
  await rm(src, { recursive: true, force: true })
  await rm(unpack, { recursive: true, force: true })

  index = makeIndex()

  server = http.createServer((req, res) => {
    if (req.url === '/api/index') {
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end(JSON.stringify(index))
      return
    }
    if (req.url === '/tarballs/fixture-pack-0.1.0.tgz') {
      res.writeHead(200, { 'content-type': 'application/octet-stream' })
      res.end(tgzBytes)
      return
    }
    res.writeHead(404)
    res.end('nope')
  })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  baseUrl = `http://127.0.0.1:${server.address().port}`
  targetsRoot = await mkdtemp(path.join(os.tmpdir(), 'install-test-targets-'))
})

afterAll(async () => {
  server?.close()
  await rm(targetsRoot, { recursive: true, force: true })
})

function makeIndex() {
  return {
    format: 1,
    packs: [{
      name: 'fixture-pack',
      version: '0.1.0',
      description: { en: 'fixture', zh: '夹具' },
      category: 'dev',
      author: 'someone',
      license: 'MIT',
      consumes: ['cosmic-func'],
      languages: ['zh-CN'],
      hasFunctionRules: false,
      contentHash: packHash,
      tarball: '/tarballs/fixture-pack-0.1.0.tgz',
      tarballSha256: sha256(tgzBytes),
    }],
  }
}

function freshInto(tag) {
  return path.join(targetsRoot, `${tag}-${Math.random().toString(36).slice(2)}`)
}

// Async spawn is required: spawnSync blocks this process's event loop, which
// would freeze the in-process fake market server and deadlock the child's
// fetch (observed as an 8s-timeout SIGTERM with empty output).
function run(args) {
  return new Promise(resolve => {
    const child = spawn('node', [INSTALL, ...args, '--market', baseUrl], { encoding: 'utf8' })
    let stdout = ''
    let stderr = ''
    child.stdout.on('data', d => { stdout += d })
    child.stderr.on('data', d => { stderr += d })
    child.on('close', (status, signal) => resolve({ status: status ?? signal, stdout, stderr }))
  })
}

test('--list prints the catalog without touching the filesystem', async () => {
  const r = await run(['--list'])
  assert.equal(r.status, 0)
  assert.match(r.stdout, /fixture-pack@0\.1\.0/)
  assert.match(r.stdout, /1 pack\(s\)/)
})

test('install lands a verified pack into --into', async () => {
  const into = freshInto('ok')
  const r = await run(['fixture-pack', '--into', into])
  assert.equal(r.status, 0, r.stderr || r.stdout)
  const manifest = JSON.parse(readFileSync(path.join(into, 'fixture-pack', 'manifest.json'), 'utf8'))
  assert.equal(manifest.name, 'fixture-pack')
  assert.match(r.stdout, /installed fixture-pack@0\.1\.0/)
})

test('already-installed pack conflicts (exit 3) unless --force', async () => {
  const into = freshInto('dup')
  assert.equal((await run(['fixture-pack', '--into', into])).status, 0)
  const again = await run(["fixture-pack", "--into", into])
  assert.equal(again.status, 3)
  assert.match(again.stderr, /already exists/)
  assert.equal((await run(['fixture-pack', '--into', into, '--force'])).status, 0)
})

test('tarball sha256 mismatch fails loudly and lands nothing (exit 2)', async () => {
  index.packs[0].tarballSha256 = 'sha256:deadbeef'
  const into = freshInto('badsha')
  const r = await run(['fixture-pack', '--into', into])
  index.packs[0].tarballSha256 = sha256(tgzBytes)
  assert.equal(r.status, 2)
  assert.match(r.stderr, /integrity check failed/)
  try {
    await access(path.join(into, 'fixture-pack'))
    assert.fail('pack dir must not exist after a failed install')
  } catch {
    // expected
  }
})

test('content hash mismatch fails loudly (exit 2)', async () => {
  index.packs[0].contentHash = 'sha256:deadbeef'
  const into = freshInto('badhash')
  const r = await run(['fixture-pack', '--into', into])
  index.packs[0].contentHash = packHash
  assert.equal(r.status, 2)
  assert.match(r.stderr, /content hash mismatch/)
})

test('manifest name mismatch fails loudly (exit 2)', async () => {
  index.packs[0].name = 'renamed-pack'
  const into = freshInto('badname')
  const r = await run(['renamed-pack', '--into', into])
  index.packs[0].name = 'fixture-pack'
  assert.equal(r.status, 2)
  assert.match(r.stderr, /integrity mismatch/)
})

test('unknown pack is a clean failure (exit 2)', async () => {
  const r = await run(['ghost-pack', '--into', freshInto('ghost')])
  assert.equal(r.status, 2)
  assert.match(r.stderr, /not in the index/)
})
