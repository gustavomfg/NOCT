// Builds the latest.json that the Rust shell's updater reads from its endpoint. Standard library only.
//
// Each platform entry takes its signature from the `.sig` file that the Tauri CLI wrote beside the artifact. That file
// is the base64 of a minisign signature, and its trusted comment names the artifact and the version it was signed for.
// The builder refuses an artifact whose trusted comment names another file or another version: `requireSignedVersion`
// rejects such an update on the installed copy, so the mismatch is a release error to stop here. The builder does not
// check the cryptographic signature. The release workflow verifies that with minisign against the configured public key
// before it runs this script.
//
// Usage:
//   node scripts/shell-latest-json.mjs --version 1.0.5 --tag rust-v1.0.5 --repository owner/name \
//     --pub-date 2026-11-18T12:00:00Z --unsigned-dir unsigned --output release-output/latest.json

import fs from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const TARGET = /^(linux|windows|darwin)-(x86_64|aarch64|i686|armv7)$/
const VERSION = /^\d+\.\d+\.\d+$/
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/
const PUB_DATE = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/
const UNSIGNED_DIRECTORY = /^shell-unsigned-(.+)$/
const TRUSTED_COMMENT = 'trusted comment: '

/** The fields of a signature's trusted comment (`timestamp:…`, `file:…`, `version:…`), keyed by name. */
export function trustedCommentOf(signature) {
  if (typeof signature !== 'string' || !BASE64.test(signature)) throw new Error('a signature is not base64')
  const text = Buffer.from(signature, 'base64').toString('utf8')
  const line = text.split('\n').find((candidate) => candidate.startsWith(TRUSTED_COMMENT))
  if (line === undefined) throw new Error('a signature has no trusted comment')
  const fields = new Map()
  for (const field of line.slice(TRUSTED_COMMENT.length).split('\t')) {
    const separator = field.indexOf(':')
    if (separator > 0) fields.set(field.slice(0, separator), field.slice(separator + 1))
  }
  return fields
}

/** Builds the manifest from the signed artifacts. Throws on any mismatch, so a wrong release is never written. */
export function buildLatestManifest({ version, tag, repository, pubDate, assets }) {
  if (!VERSION.test(version)) throw new Error(`the version is not X.Y.Z: ${version}`)
  if (tag !== `rust-v${version}`) throw new Error(`the tag ${tag} does not name version ${version}`)
  if (!REPOSITORY.test(repository)) throw new Error(`the repository is not owner/name: ${repository}`)
  if (!PUB_DATE.test(pubDate)) throw new Error(`the publication date is not RFC 3339 UTC: ${pubDate}`)
  if (!Array.isArray(assets) || assets.length === 0) throw new Error('no signed artifact was given')

  const platforms = {}
  for (const { target, file, signature } of assets) {
    if (!TARGET.test(target)) throw new Error(`the platform key is not OS-ARCH: ${target}`)
    if (Object.hasOwn(platforms, target)) throw new Error(`two artifacts target ${target}`)
    const name = path.basename(file)
    const fields = trustedCommentOf(signature)
    if (fields.get('file') !== name) {
      throw new Error(`the signature of ${name} names the file ${fields.get('file')}`)
    }
    if (fields.get('version') !== version) {
      throw new Error(`the signature of ${name} names version ${fields.get('version')}, not ${version}`)
    }
    platforms[target] = {
      signature,
      url: `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(name)}`,
    }
  }

  return {
    version,
    pub_date: pubDate,
    platforms: Object.fromEntries(Object.entries(platforms).sort(([a], [b]) => a.localeCompare(b))),
  }
}

/**
 * Reads one artifact per platform from `unsignedDir/shell-unsigned-<platform>/`. Each directory holds exactly one
 * artifact and the `.sig` the signing step wrote beside it.
 */
export function collectAssets(unsignedDir) {
  const assets = []
  for (const entry of fs.readdirSync(unsignedDir, { withFileTypes: true })) {
    const match = UNSIGNED_DIRECTORY.exec(entry.name)
    if (!match || !entry.isDirectory()) continue
    const directory = path.join(unsignedDir, entry.name)
    const artifacts = fs.readdirSync(directory).filter((name) => !name.endsWith('.sig'))
    if (artifacts.length !== 1) {
      throw new Error(`${entry.name} must hold exactly one artifact, found ${artifacts.length}`)
    }
    const file = path.join(directory, artifacts[0])
    const signature = fs.readFileSync(`${file}.sig`, 'utf8').trim()
    assets.push({ target: match[1], file, signature })
  }
  if (assets.length === 0) throw new Error(`no shell-unsigned-<platform> directory in ${unsignedDir}`)
  return assets
}

function parseArguments(argv) {
  const options = new Map()
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index]
    const value = argv[index + 1]
    if (!name?.startsWith('--') || value === undefined) throw new Error(`expected --name value, got ${name}`)
    options.set(name.slice(2), value)
  }
  const required = ['version', 'tag', 'repository', 'pub-date', 'unsigned-dir', 'output']
  for (const name of required) {
    if (!options.has(name)) throw new Error(`missing --${name}`)
  }
  return Object.fromEntries(options)
}

function main(argv) {
  const options = parseArguments(argv)
  const manifest = buildLatestManifest({
    version: options.version,
    tag: options.tag,
    repository: options.repository,
    pubDate: options['pub-date'],
    assets: collectAssets(options['unsigned-dir']),
  })
  fs.mkdirSync(path.dirname(options.output), { recursive: true })
  fs.writeFileSync(options.output, `${JSON.stringify(manifest, null, 2)}\n`)
  process.stdout.write(`latest.json written for ${Object.keys(manifest.platforms).join(', ')}.\n`)
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1] ?? '')).href) {
  try {
    main(process.argv.slice(2))
  } catch (error) {
    process.stderr.write(`${error.message}\n`)
    process.exitCode = 1
  }
}
