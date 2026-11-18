import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

// The release workflow of the Rust shell is a draft that no step of this repository runs. These checks keep its
// security properties from changing in an edit: the trigger, the pins, who can write and who can read the signing
// secrets, and which job publishes. They read the file as text, like the other workflow checks of the repository.
const workflow = fs.readFileSync(path.join(process.cwd(), '.github/workflows/shell-release.yml'), 'utf8')

/** The text of each job under `jobs:`, keyed by the job name. */
function jobs(text: string): Map<string, string> {
  const marker = '\njobs:\n'
  const body = text.slice(text.indexOf(marker) + marker.length)
  const blocks = new Map<string, string>()
  let current = ''
  let lines: string[] = []
  for (const line of body.split('\n')) {
    const header = /^ {2}([a-z-]+):$/.exec(line)
    if (header) {
      if (current) blocks.set(current, lines.join('\n'))
      current = header[1]
      lines = []
    } else {
      lines.push(line)
    }
  }
  if (current) blocks.set(current, lines.join('\n'))
  return blocks
}

const job = jobs(workflow)

describe('shell release workflow', () => {
  it('starts only on a rust-v* tag push, and no Electron workflow runs on that tag', () => {
    const triggers = workflow.slice(workflow.indexOf('\non:\n') + 1, workflow.indexOf('\npermissions:'))

    expect(triggers).toBe("on:\n  push:\n    tags:\n      - 'rust-v*'\n")
    // The Electron release and validation workflows match their tags by glob. A shell tag must match none of them,
    // or both workflows would run, and two releases could claim one tag.
    for (const file of ['stable-release.yml', 'package-validation.yml']) {
      const text = fs.readFileSync(path.join(process.cwd(), '.github/workflows', file), 'utf8')
      const patterns = [...text.matchAll(/^\s+- '([^']+)'$/gm)].map((match) => match[1]).filter((p) => !p.startsWith('!'))
      for (const pattern of patterns) {
        const glob = new RegExp(`^${pattern.replace(/\*/g, '.*')}$`)
        expect(glob.test('rust-v1.0.5'), `${file} tag pattern ${pattern}`).toBe(false)
      }
    }
  })

  it('reads only contents at the top, and grants writes per job', () => {
    expect(workflow).toMatch(/\npermissions:\n {2}contents: read\n/)
    for (const [name, block] of job) {
      if (name === 'publish') continue
      expect(block, `${name} must not write`).not.toMatch(/: write/)
    }
  })

  it('pins every action to a full commit SHA with its version as a comment', () => {
    const uses = workflow.match(/^\s*(?:- )?uses: .+$/gm) ?? []
    expect(uses.length).toBeGreaterThan(0)
    for (const line of uses) {
      expect(line).toMatch(/uses: [\w.-]+\/[\w.-]+@[0-9a-f]{40} # v\d+(\.\d+\.\d+)?$/)
    }
  })

  it('reads the signing secrets only in the sign job, which runs in the release environment', () => {
    const secretNames = ['TAURI_SIGNING_PRIVATE_KEY', 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD']
    for (const [name, block] of job) {
      const reads = secretNames.filter((secret) => block.includes(`secrets.${secret}`))
      if (name === 'sign') {
        expect(reads).toEqual(secretNames)
      } else {
        expect(reads, `${name} must not read a signing secret`).toEqual([])
      }
    }
    const environments = [...job].filter(([, block]) => /^ {4}environment: release$/m.test(block))
    expect(environments.map(([name]) => name)).toEqual(['sign'])
  })

  it('builds without a signing key, and signs only in the sign job', () => {
    expect(job.get('build')).toContain('--no-sign')
    expect(job.get('build')).toContain('createUpdaterArtifacts')
    expect(job.get('sign')).toContain('cargo tauri signer sign --app-version')
    expect(job.get('build')).not.toContain('signer sign')
    expect(job.get('publish')).not.toContain('signer sign')
  })

  it('pins the build tools to the versions the repository uses', () => {
    expect(workflow).toContain('cargo install --locked trunk@0.22.1')
    expect(workflow).toContain('cargo install --locked tauri-cli@2.12.4')
    expect(workflow).not.toMatch(/tauri-cli@(?!2\.12\.4\b)/)
  })

  it('refuses a tag that is not a stable rust-v version, or a version that differs from the configuration', () => {
    const gate = job.get('gate') ?? ''
    expect(gate).toContain('^rust-v[0-9]+\\.[0-9]+\\.[0-9]+$')
    expect(gate).toContain('"rust-v$version"')
    expect(gate).toContain('crates/noct-app/tauri.conf.json')
    expect(gate).toContain('version = \\"$version\\"')
  })

  it('requires successful shell CI for the tagged commit, not for main, before anything is built', () => {
    const gate = job.get('gate') ?? ''
    expect(gate).toContain('rust-shell-validation.yml/runs')
    expect(gate).toContain('head_sha="$RELEASE_SHA"')
    expect(gate).toContain('run.head_sha === process.env.RELEASE_SHA')
    expect(gate).not.toContain('main')
    expect(job.get('build')).toContain('needs: gate')
  })

  it('does not depend on main anywhere: the tag is the only thing that identifies a release', () => {
    for (const [name, block] of job) {
      expect(block, `${name} must not read main`).not.toMatch(/refs\/heads\/main|head_branch|branch=main/)
    }
  })

  it('verifies every signature with minisign against the configured key before the manifest is written', () => {
    const sign = job.get('sign') ?? ''
    expect(sign.indexOf('minisign -Vm')).toBeGreaterThan(sign.indexOf('cargo tauri signer sign'))
    expect(sign.indexOf('scripts/shell-latest-json.mjs')).toBeGreaterThan(sign.indexOf('minisign -Vm'))
    expect(sign).toContain('plugins.updater.pubkey')
  })

  it('attests the release assets and publishes only from the exact tag, after the signing job', () => {
    const publish = job.get('publish') ?? ''
    expect(job.get('publish')).toContain('needs: sign')
    // The attestation action is the one the dry run of build-provenance-dry-run.yml verified, at the same pin.
    expect(publish).toContain('actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8 # v4.2.2')
    expect(publish.indexOf('attest-build-provenance')).toBeLessThan(publish.indexOf('gh release create'))
    expect(publish).toContain('--verify-tag')
    expect(publish).toContain('refs/tags/$RELEASE_TAG')
    // An annotated tag names its commit on the peeled line, so the tag is compared through it.
    expect(publish).toContain('refs/tags/$RELEASE_TAG^{}')
    for (const [name, block] of job) {
      if (name !== 'publish') expect(block, `${name} must not publish`).not.toContain('gh release create')
    }
  })
})
