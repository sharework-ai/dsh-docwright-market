// Unit tests for the verifier's market-level shape logic. The real docwright
// pack:check round is exercised by scripts/e2e.sh, not here.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

// A minimal fake docwright checkout so index.mjs's failFast passes and the
// server binds an ephemeral port (VERIFIER_PORT=0) without side effects.
const fixtureRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-test-docwright-'))
await fs.mkdir(path.join(fixtureRoot, 'packs', 'cosmic-func'), { recursive: true })
await fs.writeFile(
  path.join(fixtureRoot, 'package.json'),
  JSON.stringify({ scripts: { 'pack:check': 'tsx scripts/pack-check.ts' } }),
)

process.env.DOCWRIGHT_ROOT = fixtureRoot
process.env.VERIFIER_PORT = '0'
process.env.VERIFIER_TMP = path.join(fixtureRoot, 'tmp')

const {
  listTarEntries,
  findPackDir,
  checkShape,
  contentHashOf,
  builtinPackNames,
} = await import('../index.mjs')

async function makeTgz(files, dirName = 'pack-root') {
  const base = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-tgz-'))
  const src = path.join(base, dirName)
  await fs.mkdir(src, { recursive: true })
  for (const [rel, content] of Object.entries(files)) {
    const full = path.join(src, rel)
    await fs.mkdir(path.dirname(full), { recursive: true })
    await fs.writeFile(full, content)
  }
  const tgz = path.join(base, 'archive.tgz')
  execFileSync('tar', ['czf', tgz, '-C', base, dirName])
  return { base, tgz, src }
}

test('listTarEntries lists a normal tgz', async () => {
  const { base, tgz } = await makeTgz({ 'manifest.json': '{}' })
  const entries = await listTarEntries(tgz)
  assert.deepEqual(entries, ['pack-root', 'pack-root/manifest.json'])
  await fs.rm(base, { recursive: true, force: true })
})

test('listTarEntries rejects traversal entries', async () => {
  const base = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-evil-'))
  const tgz = path.join(base, 'evil.tgz')
  const src = path.join(base, 'stage')
  await fs.mkdir(src, { recursive: true })
  await fs.writeFile(path.join(src, 'payload.txt'), 'x')
  // Build an archive whose member escapes via an absolute path.
  execFileSync('tar', ['czf', tgz, '-C', src, '--transform', 's|payload.txt|/tmp/evil.txt|', 'payload.txt'])
  await assert.rejects(() => listTarEntries(tgz), error => error.unsafe === true)
  await fs.rm(base, { recursive: true, force: true })
})

test('listTarEntries rejects a non-tarball body', async () => {
  const base = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-junk-'))
  const tgz = path.join(base, 'junk.tgz')
  await fs.writeFile(tgz, 'definitely not gzip')
  await assert.rejects(() => listTarEntries(tgz))
  await fs.rm(base, { recursive: true, force: true })
})

test('findPackDir: archive root is the pack', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-find-'))
  await fs.writeFile(path.join(dir, 'manifest.json'), '{}')
  assert.equal(await findPackDir(dir), dir)
  await fs.rm(dir, { recursive: true, force: true })
})

test('findPackDir: single top-level pack dir', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-find-'))
  const pack = path.join(dir, 'task-card')
  await fs.mkdir(pack)
  await fs.writeFile(path.join(pack, 'manifest.json'), '{}')
  assert.equal(await findPackDir(dir), pack)
  await fs.rm(dir, { recursive: true, force: true })
})

test('findPackDir: stray tree yields null', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-find-'))
  await fs.mkdir(path.join(dir, 'docs'))
  await fs.writeFile(path.join(dir, 'readme.txt'), 'hi')
  assert.equal(await findPackDir(dir), null)
  await fs.rm(dir, { recursive: true, force: true })
})

test('checkShape flags builtin collision and dir mismatch', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-shape-'))
  const pack = path.join(dir, 'wrong-dir')
  await fs.mkdir(pack)
  await fs.writeFile(
    path.join(pack, 'manifest.json'),
    JSON.stringify({ name: 'cosmic-func', version: '1.0.0' }),
  )
  const builtins = await builtinPackNames()
  assert.ok(builtins.has('cosmic-func'), 'fixture docwright must expose builtin names')
  const { problems } = await checkShape(pack, { builtins })
  assert.ok(problems.some(p => p.includes('collides with a docwright built-in')))
  assert.ok(problems.some(p => p.includes('must equal manifest.name')))
  await fs.rm(dir, { recursive: true, force: true })
})

test('checkShape flags non-kebab names and missing version', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-shape-'))
  const pack = path.join(dir, 'pack-root')
  await fs.mkdir(pack)
  await fs.writeFile(path.join(pack, 'manifest.json'), JSON.stringify({ name: 'My Pack!' }))
  const { problems } = await checkShape(pack, { builtins: new Set() })
  assert.ok(problems.some(p => p.includes('kebab-case')))
  assert.ok(problems.some(p => p.includes('version')))
  await fs.rm(dir, { recursive: true, force: true })
})

test('checkShape accepts a well-formed pack', async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-shape-'))
  const pack = path.join(dir, 'task-card')
  await fs.mkdir(pack)
  await fs.writeFile(
    path.join(pack, 'manifest.json'),
    JSON.stringify({ name: 'task-card', version: '0.1.0' }),
  )
  const { problems, manifest } = await checkShape(pack, { builtins: new Set(['cosmic-func']) })
  assert.deepEqual(problems, [])
  assert.equal(manifest.name, 'task-card')
  await fs.rm(dir, { recursive: true, force: true })
})

test('contentHashOf is stable across file order and sensitive to content', async () => {
  const dirA = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-hash-a-'))
  const dirB = await fs.mkdtemp(path.join(os.tmpdir(), 'verifier-hash-b-'))
  for (const root of [dirA, dirB]) {
    await fs.writeFile(path.join(root, 'a.txt'), 'same bytes')
    await fs.mkdir(path.join(root, 'rules'))
    await fs.writeFile(path.join(root, 'rules', 'r.json'), '{}')
  }
  const hashA = await contentHashOf(dirA)
  const hashB = await contentHashOf(dirB)
  assert.equal(hashA, hashB, 'identical content sets must hash equal regardless of creation order')
  await fs.writeFile(path.join(dirA, 'a.txt'), 'changed')
  assert.notEqual(await contentHashOf(dirA), hashA)
  await fs.rm(dirA, { recursive: true, force: true })
  await fs.rm(dirB, { recursive: true, force: true })
})
