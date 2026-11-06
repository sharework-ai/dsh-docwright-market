#!/usr/bin/env node
// docwright pack market — verification sidecar (zero-dependency, node >= 18).
//
// The market's Rust service owns product flow; this process owns pack
// semantics exclusively (single validation surface: docwright's own loader,
// "the loader IS the linter" — nothing here re-implements docwright rules).
//
// Endpoints:
//   POST /verify   body = raw .tgz bytes (application/octet-stream)
//                  -> 200 {ok, report}  (ok = shape+check green; report always present)
//                  -> 400 {ok:false, error}  (request itself unusable)
//   GET  /health   -> {ok, docwrightRoot}
//
// Config (env, fail loud at startup):
//   VERIFIER_PORT       default 8399
//   DOCWRIGHT_ROOT      required; docwright checkout with `pack:check` script
//                       and packs/ present (its ../deepseek-harness must be built)
//   VERIFIER_TMP        work dir root, default os.tmpdir()/docwright-market-verifier
//   VERIFIER_MAX_BYTES  upload cap, default 50 MiB
//
// This sidecar NEVER executes pack content beyond what docwright's own
// verification executes (function rules run inside pack:check, on this box,
// under the operator's review — never at install time on user machines).

import http from 'node:http'
import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { createWriteStream } from 'node:fs'
import fs from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const PORT = Number(process.env.VERIFIER_PORT ?? 8399)
const DOCWRIGHT_ROOT = process.env.DOCWRIGHT_ROOT ?? ''
const TMP_ROOT = process.env.VERIFIER_TMP ?? path.join(os.tmpdir(), 'docwright-market-verifier')
const MAX_BYTES = Number(process.env.VERIFIER_MAX_BYTES ?? 50 * 1024 * 1024)
const CHECK_TIMEOUT_MS = Number(process.env.VERIFIER_CHECK_TIMEOUT_MS ?? 5 * 60 * 1000)

const NAME_RE = /^[a-z0-9][a-z0-9-]*$/

async function failFast() {
  if (!DOCWRIGHT_ROOT) {
    console.error('DOCWRIGHT_ROOT is required (docwright checkout with packs/)')
    process.exit(1)
  }
  const packsDir = path.join(DOCWRIGHT_ROOT, 'packs')
  const script = path.join(DOCWRIGHT_ROOT, 'package.json')
  try {
    const [packsStat, pkgRaw] = await Promise.all([fs.stat(packsDir), fs.readFile(script, 'utf8')])
    if (!packsStat.isDirectory()) throw new Error('packs/ is not a directory')
    if (!JSON.parse(pkgRaw).scripts?.['pack:check']) throw new Error('no pack:check script')
  } catch (error) {
    console.error(`DOCWRIGHT_ROOT unusable at ${DOCWRIGHT_ROOT}: ${error.message}`)
    process.exit(1)
  }
  await fs.mkdir(TMP_ROOT, { recursive: true })
}

// ---- archive handling -------------------------------------------------------

/** List a tgz without extracting; rejects absolute paths and traversal. */
export async function listTarEntries(tgzPath) {
  const out = await run('tar', ['-tzf', tgzPath])
  const entries = out.split('\n').filter(line => line.trim() !== '').map(e => e.replace(/\/+$/, ''))
  for (const entry of entries) {
    const normalized = path.normalize(entry)
    if (entry.startsWith('/') || normalized.startsWith('..') || normalized.includes('../')) {
      throw Object.assign(new Error(`archive contains unsafe path: ${entry}`), { unsafe: true })
    }
  }
  return entries
}

async function extractTar(tgzPath, destDir) {
  await run('tar', ['-xzf', tgzPath, '-C', destDir])
}

async function run(cmd, args, options = {}) {
  const { code, stdout, stderr } = await runRaw(cmd, args, options)
  if (code !== 0) {
    const tail = (stderr || stdout).trim().split('\n').slice(-6).join('\n')
    throw new Error(`${cmd} exited ${code}: ${tail}`)
  }
  return stdout
}

function runRaw(cmd, args, { timeoutMs = CHECK_TIMEOUT_MS, cwd } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] })
    let stdout = ''
    let stderr = ''
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      reject(new Error(`${cmd} timed out after ${timeoutMs}ms`))
    }, timeoutMs)
    child.stdout.on('data', d => { stdout += d })
    child.stderr.on('data', d => { stderr += d })
    child.on('error', error => { clearTimeout(timer); reject(error) })
    child.on('close', code => { clearTimeout(timer); resolve({ code, stdout, stderr }) })
  })
}

// ---- shape checks (market policy; docwright invariants stay in the loader) --

/**
 * Locate the pack directory inside an extracted tree:
 *   - tree itself is a pack (manifest.json at its root), or
 *   - the single top-level directory is a pack, or
 *   - nothing usable (typo'd archive, stray files) -> null.
 */
export async function findPackDir(extractDir) {
  const dirents = await fs.readdir(extractDir, { withFileTypes: true })
  if (await isPackDir(extractDir)) return extractDir
  const dirs = dirents.filter(d => d.isDirectory())
  if (dirs.length === 1) {
    const candidate = path.join(extractDir, dirs[0].name)
    if (await isPackDir(candidate)) return candidate
  }
  return null
}

async function isPackDir(dir) {
  try {
    await fs.access(path.join(dir, 'manifest.json'))
    return true
  } catch {
    return false
  }
}

/** Built-in pack names, read live from the docwright checkout. */
export async function builtinPackNames() {
  const dirs = await fs.readdir(path.join(DOCWRIGHT_ROOT, 'packs'), { withFileTypes: true })
  return new Set(dirs.filter(d => d.isDirectory()).map(d => d.name))
}

/**
 * Market-level manifest checks that must run before/independently of
 * pack:check (the loader would silently allow some of these by design).
 */
export async function checkShape(packDir, { builtins }) {
  const problems = []
  let manifest = null
  try {
    manifest = JSON.parse(await fs.readFile(path.join(packDir, 'manifest.json'), 'utf8'))
  } catch (error) {
    problems.push(`manifest.json is not readable JSON: ${error.message}`)
    return { problems, manifest: null }
  }
  if (typeof manifest.name !== 'string' || manifest.name.trim() === '') {
    problems.push('manifest.name must be a non-empty string')
  } else {
    if (!NAME_RE.test(manifest.name)) {
      problems.push(`manifest.name "${manifest.name}" must match ${NAME_RE} (kebab-case)`)
    }
    if (builtins.has(manifest.name)) {
      problems.push(`name "${manifest.name}" collides with a docwright built-in pack — same-name packs override builtins by design, the market refuses them; pick another name`)
    }
    if (path.basename(packDir) !== manifest.name) {
      problems.push(`directory name "${path.basename(packDir)}" must equal manifest.name "${manifest.name}"`)
    }
  }
  if (typeof manifest.version !== 'string' || manifest.version.trim() === '') {
    problems.push('manifest.version must be a non-empty string')
  }
  return { problems, manifest }
}

/**
 * sha256 over sorted (relative path, bytes) — stable content identity for
 * update detection and install-time verification.
 */
export async function contentHashOf(dir) {
  const files = []
  async function walk(current) {
    const dirents = await fs.readdir(current, { withFileTypes: true })
    dirents.sort((a, b) => a.name.localeCompare(b.name))
    for (const dirent of dirents) {
      const full = path.join(current, dirent.name)
      if (dirent.isDirectory()) await walk(full)
      else files.push(full)
    }
  }
  await walk(dir)
  const hash = createHash('sha256')
  for (const file of files.sort()) {
    const rel = path.relative(dir, file).split(path.sep).join('/')
    hash.update(rel)
    hash.update('\0')
    hash.update(await fs.readFile(file))
  }
  return `sha256:${hash.digest('hex')}`
}

// ---- the real gate: docwright's own pack:check ------------------------------

async function runPackCheck(checkRoot) {
  const result = await runRaw(
    'pnpm',
    ['run', 'pack:check', '--', checkRoot, '--with', 'packs'],
    { cwd: DOCWRIGHT_ROOT },
  )
  return { code: result.code, output: (result.stdout + result.stderr).trim() }
}

// ---- request handling -------------------------------------------------------

async function handleVerify(req, res) {
  const workDir = await fs.mkdtemp(path.join(TMP_ROOT, 'verify-'))
  try {
    const tgzPath = path.join(workDir, 'archive.tgz')
    const size = await saveBody(req, tgzPath)
    if (size === 0) return sendJson(res, 400, { ok: false, error: 'empty body — send the raw .tgz bytes' })

    const report = {
      packName: null,
      version: null,
      manifest: null,
      shapeOk: false,
      checkGreen: false,
      hasFunctionRules: false,
      contentHash: null,
      files: [],
      checkOutput: '',
      problems: [],
    }
    try {
      report.files = await listTarEntries(tgzPath)
    } catch (error) {
      if (error.unsafe) return sendJson(res, 400, { ok: false, error: error.message })
      return sendJson(res, 400, { ok: false, error: `not a readable gzip tarball: ${error.message}` })
    }

    const extractDir = path.join(workDir, 'extracted')
    await fs.mkdir(extractDir)
    await extractTar(tgzPath, extractDir)

    const found = await findPackDir(extractDir)
    if (found === null) {
      report.problems.push('no pack found: archive root must be a pack directory (manifest.json at its root) or contain exactly one')
      return sendJson(res, 200, { ok: false, report })
    }
    const builtins = await builtinPackNames()
    const shape = await checkShape(found, { builtins })
    report.problems.push(...shape.problems)
    report.shapeOk = shape.problems.length === 0
    if (shape.manifest) {
      report.packName = shape.manifest.name ?? null
      report.version = shape.manifest.version ?? null
      report.manifest = shape.manifest
      report.hasFunctionRules = await exists(path.join(found, 'rules', 'functions.js'))
      report.contentHash = await contentHashOf(found)
    }
    if (!report.shapeOk) return sendJson(res, 200, { ok: false, report })

    // Directory name must equal pack name for the loader+check round.
    const checkRoot = path.join(workDir, 'check-root')
    await fs.mkdir(path.join(checkRoot, report.packName), { recursive: true })
    await fs.cp(found, path.join(checkRoot, report.packName), { recursive: true })

    const check = await runPackCheck(checkRoot)
    report.checkOutput = check.output
    report.checkGreen = check.code === 0
    return sendJson(res, 200, { ok: report.checkGreen, report })
  } finally {
    await fs.rm(workDir, { recursive: true, force: true }).catch(() => {})
  }
}

function saveBody(req, dest) {
  return new Promise((resolve, reject) => {
    let size = 0
    const out = createWriteStream(dest)
    req.on('data', chunk => {
      size += chunk.length
      if (size > MAX_BYTES) {
        req.destroy()
        reject(Object.assign(new Error(`body exceeds ${MAX_BYTES} bytes`), { tooBig: true }))
        return
      }
      out.write(chunk)
    })
    req.on('end', () => { out.end(() => resolve(size)) })
    req.on('error', error => reject(error))
    out.on('error', error => reject(error))
  })
}

async function exists(p) {
  try { await fs.access(p); return true } catch { return false }
}

function sendJson(res, code, body) {
  const payload = JSON.stringify(body, null, 2)
  res.writeHead(code, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) })
  res.end(payload)
}

// ---- server -----------------------------------------------------------------

const server = http.createServer(async (req, res) => {
  try {
    if (req.method === 'GET' && req.url === '/health') {
      return sendJson(res, 200, { ok: true, docwrightRoot: DOCWRIGHT_ROOT })
    }
    if (req.method === 'POST' && req.url === '/verify') {
      try {
        return await handleVerify(req, res)
      } catch (error) {
        if (error.tooBig) return sendJson(res, 400, { ok: false, error: error.message })
        console.error('verify failed:', error)
        return sendJson(res, 400, { ok: false, error: error.message })
      }
    }
    return sendJson(res, 404, { ok: false, error: 'not found' })
  } catch (error) {
    console.error('request error:', error)
    if (!res.headersSent) sendJson(res, 500, { ok: false, error: 'internal error' })
  }
})

// Only boot (and fail-fast on config) when run as the entry module; the test
// suite imports this file for its pure helpers and must not bind a server.
const isMain = process.argv[1]
  && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
if (isMain) {
  await failFast()
  server.listen(PORT, '127.0.0.1', () => {
    console.log(`verifier listening on 127.0.0.1:${PORT} (docwright: ${DOCWRIGHT_ROOT})`)
  })
}
