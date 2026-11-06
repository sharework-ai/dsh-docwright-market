#!/usr/bin/env node
// docwright pack market — consumer installer (zero-dependency, node >= 18).
//
// Downloads verified pack tarballs from the market into a docwright packsDir.
// ONLY COPIES — never executes anything from a pack. rules/functions.js, if a
// pack has one, is executed by docwright itself at load time on this machine;
// the site flags such packs and you should review that code before installing.
//
// Usage:
//   node install.mjs <pack>[@version]...     install named packs (latest if no @version)
//   node install.mjs --all                   install every pack in the index
//   node install.mjs --list                  list the index and exit
//   node install.mjs --into <dir>            target packsDir (default ./.docwright/packs)
//   node install.mjs --market <url>          market base (env MARKET_URL, default http://localhost:8398)
//   node install.mjs --mirror <url>          use another market base (same API shape)
//   node install.mjs --index <url-or-path>   index override (file path or URL)
//   node install.mjs --force                 overwrite an already-installed pack
//
// Exit codes: 0 ok, 1 usage, 2 network/integrity failure, 3 name conflict.

import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'

const BUILTIN_PACKS = new Set([
  'cosmic-context', 'cosmic-context-sync', 'cosmic-plan', 'cosmic-req', 'cosmic-func', 'cosmic-tech',
])

function usage(code = 1) {
  console.log(`usage: node install.mjs <pack>[@version]... | --all | --list
  --into <dir>       target packsDir (default ./.docwright/packs)
  --market <url>     market base URL (env MARKET_URL, default http://localhost:8398)
  --mirror <url>     same as --market, for mirrors
  --index <loc>      index override (URL or local file path)
  --force            overwrite already-installed packs`)
  process.exit(code)
}

const args = process.argv.slice(2)
const wanted = []
let into = path.resolve('.docwright', 'packs')
let market = process.env.MARKET_URL ?? 'http://localhost:8398'
let indexOverride = null
let force = false
let listOnly = false
let all = false

for (let i = 0; i < args.length; i++) {
  const a = args[i]
  if (a === '--into') into = path.resolve(args[++i] ?? usage())
  else if (a === '--market' || a === '--mirror') market = String(args[++i] ?? usage()).replace(/\/+$/, '')
  else if (a === '--index') indexOverride = args[++i] ?? usage()
  else if (a === '--force') force = true
  else if (a === '--list') listOnly = true
  else if (a === '--all') all = true
  else if (a === '-h' || a === '--help') usage(0)
  else wanted.push(a)
}
if (!listOnly && !all && wanted.length === 0) usage(1)

const indexUrl = indexOverride ?? `${market}/api/index`
const refFor = rel => rel.startsWith('http') ? rel : `${market}${rel}`

async function fetchIndex() {
  if (indexUrl.startsWith('http')) {
    const res = await fetch(indexUrl)
    if (!res.ok) throw Object.assign(new Error(`index fetch ${indexUrl} -> ${res.status}`), { code: 2 })
    return res.json()
  }
  return JSON.parse(await fs.readFile(indexUrl, 'utf8'))
}

async function sha256File(file) {
  const hash = createHash('sha256')
  hash.update(await fs.readFile(file))
  return `sha256:${hash.digest('hex')}`
}

/** Deterministic directory hash: sorted (relative path, bytes). */
async function dirHash(dir) {
  const files = []
  async function walk(current) {
    for (const dirent of (await fs.readdir(current, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
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
    hash.update(await fs.readFile(file))
  }
  return `sha256:${hash.digest('hex')}`
}

function die(message, code = 2) {
  console.error(`install: ${message}`)
  process.exit(code)
}

const index = await fetchIndex().catch(e => die(`cannot read index: ${e.message}`))
if (index.format !== 1) die(`unsupported index format: ${index.format}`)

if (listOnly) {
  for (const p of index.packs) {
    console.log(`${p.name}@${p.version} [${p.category}] ${p.hasFunctionRules ? '(functions.js) ' : ''}${p.description?.en ?? ''}`)
  }
  console.log(`\n${index.packs.length} pack(s); market: ${market}`)
  process.exit(0)
}

const targets = all ? index.packs.map(p => p.name) : wanted
const installed = []

for (const request of targets) {
  const [name, version] = request.split('@')
  const entry = index.packs.find(p => p.name === name)
  if (!entry) die(`pack "${name}" not in the index (try --list)`)
  if (version && entry.version !== version) die(`pack "${name}" is at ${entry.version}, requested ${version} — only the current version is served`)

  const dest = path.join(into, name)
  if (!force) {
    try {
      await fs.access(dest)
      die(`"${name}" already exists at ${dest} — pass --force to overwrite`, 3)
    } catch { /* not installed yet: good */ }
  }

  const url = refFor(entry.tarball)
  const tmpRoot = await fs.mkdtemp(path.join(os.tmpdir(), `dw-install-${name}-`))
  try {
    const tgz = path.join(tmpRoot, 'pack.tgz')
    const res = await fetch(url)
    if (!res.ok) die(`tarball ${url} -> ${res.status}`)
    const bytes = Buffer.from(await res.arrayBuffer())
    const gotSha = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
    if (gotSha !== entry.tarballSha256) {
      die(`integrity check failed for ${name}: index says ${entry.tarballSha256}, got ${gotSha}`)
    }
    await fs.writeFile(tgz, bytes)

    const extractDir = path.join(tmpRoot, 'x')
    await fs.mkdir(extractDir)
    const tar = spawnSync('tar', ['-xzf', tgz, '-C', extractDir], { stdio: 'pipe' })
    if (tar.status !== 0) die(`tar extraction failed: ${tar.stderr}`)

    // Locate the pack dir (root or single top-level) and verify its identity
    // against the index before anything is copied into the packsDir.
    const dirents = await fs.readdir(extractDir, { withFileTypes: true })
    let packDir = extractDir
    const hasManifest = async d => { try { await fs.access(path.join(d, 'manifest.json')); return true } catch { return false } }
    if (!(await hasManifest(packDir))) {
      const dirs = dirents.filter(d => d.isDirectory())
      if (dirs.length !== 1) die(`archive of "${name}" has no single pack directory`)
      packDir = path.join(extractDir, dirs[0].name)
      if (!(await hasManifest(packDir))) die(`archive of "${name}" contains no manifest.json`)
    }
    const manifest = JSON.parse(await fs.readFile(path.join(packDir, 'manifest.json'), 'utf8'))
    if (manifest.name !== name) die(`integrity mismatch: index says "${name}", archive manifest says "${manifest.name}"`)
    const hash = await dirHash(packDir)
    if (hash !== entry.contentHash) {
      die(`content hash mismatch for "${name}": index says ${entry.contentHash}, got ${hash}`)
    }

    // Atomic-ish landing: copy next to the destination, then rename over.
    await fs.mkdir(path.dirname(dest), { recursive: true })
    const staged = path.join(path.dirname(dest), `.${name}.staging`)
    await fs.rm(staged, { recursive: true, force: true })
    await fs.cp(packDir, staged, { recursive: true })
    await fs.rm(dest, { recursive: true, force: true })
    await fs.rename(staged, dest)
    installed.push(entry)

    console.log(`installed ${name}@${entry.version} -> ${dest}`)
  } finally {
    await fs.rm(tmpRoot, { recursive: true, force: true }).catch(() => {})
  }
}

// Follow-ups: missing upstreams, builtin overrides, restart reminder.
const localNames = new Set(installed.map(p => p.name))
for (const p of installed) {
  for (const up of p.consumes ?? []) {
    if (BUILTIN_PACKS.has(up) || localNames.has(up)) continue
    if (!index.packs.some(x => x.name === up)) {
      console.warn(`warning: ${p.name} consumes "${up}" which is neither built-in nor in this market`)
      continue
    }
    console.warn(`note: ${p.name} consumes "${up}" — install it too: node install.mjs ${up}`)
  }
  if (BUILTIN_PACKS.has(p.name)) {
    console.warn(`warning: "${p.name}" overrides the docwright built-in pack of the same name`)
  }
}
console.log(`\nDone. Packs load when the docwright plugin applies — restart the dsh host, then check with doc_packs.
(装完后重启宿主,用 doc_packs 确认可见。)`)
