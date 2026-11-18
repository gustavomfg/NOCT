import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

// The release workflow of the Rust shell is a draft that no step of this repository runs. These checks keep its
// security properties from changing in an edit: the trigger, the pins, who can write and who can read the signing
// secrets, and which job publishes. They read the file as text, like the other workflow checks of the repository.
// Windows checks a text file out with CRLF line ends, and these checks match line ends, so each file is read with LF.
const workflow = fs.readFileSync(path.join(process.cwd(), '.github/workflows/shell-release.yml'), 'utf8').replace(/\r\n/g, '\n')

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

/** The tag patterns of the Electron workflows, as written in their `tags:` lists. Negated patterns are left out. */
function electronTagPatterns(): string[] {
  return ['stable-release.yml', 'package-validation.yml'].flatMap((file) => {
    const text = fs.readFileSync(path.join(process.cwd(), '.github/workflows', file), 'utf8').replace(/\r\n/g, '\n')
    return [...text.matchAll(/^\s+- '([^']+)'$/gm)].map((match) => match[1]).filter((pattern) => !pattern.startsWith('!'))
  })
}

/** Whether a tag pattern of a workflow, a glob with `*`, matches a tag name. */
function matchesGlob(pattern: string, name: string): boolean {
  return new RegExp(`^${pattern.replace(/\*/g, '.*')}$`).test(name)
}

describe('shell release workflow', () => {
  it('starts only on a rust-v* tag push, and no Electron workflow runs on that tag', () => {
    const triggers = workflow.slice(workflow.indexOf('\non:\n') + 1, workflow.indexOf('\npermissions:'))

    expect(triggers).toBe("on:\n  push:\n    tags:\n      - 'rust-v*'\n")
    // A shell tag must match no Electron tag pattern, or both workflows would run and two releases could claim one tag.
    for (const pattern of electronTagPatterns()) {
      expect(matchesGlob(pattern, 'rust-v1.0.5'), `Electron tag pattern ${pattern}`).toBe(false)
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

  it('serves the updater from the updater-feed channel that the configuration names, and no Electron tag matches it', () => {
    const config = JSON.parse(fs.readFileSync(path.join(process.cwd(), 'crates/noct-app/tauri.conf.json'), 'utf8'))
    const feed = 'updater-feed'
    expect(config.plugins.updater.endpoints).toEqual([
      `https://github.com/gustavomfg/NOCT/releases/download/${feed}/latest.json`,
    ])
    expect(workflow).toContain(`FEED_TAG: ${feed}`)
    for (const pattern of electronTagPatterns()) {
      expect(matchesGlob(pattern, feed), `Electron tag pattern ${pattern}`).toBe(false)
    }
    expect(matchesGlob('rust-v*', feed)).toBe(false)
  })

  it('publishes every shell release as a pre-release that is never the latest, and never with a latest mark', () => {
    const publish = job.get('publish') ?? ''
    // Two creations at most: the release of the tag, and the feed the first time it is made. Each command runs to the
    // blank line that ends its step.
    const commands = publish.split('gh release create').slice(1).map((part) => part.split(/\n\s*\n/)[0] ?? '')
    expect(commands).toHaveLength(2)
    for (const command of commands) {
      expect(command).toContain('--prerelease')
      expect(command).toContain('--latest=false')
    }
    expect(workflow).not.toMatch(/--latest(?!=false)/)
    expect(workflow).not.toMatch(/gh release edit/)
    expect(workflow).not.toMatch(/make_latest|make-latest/)
  })

  it('checks at run time that the shell release is a pre-release and that the latest release is another one', () => {
    const publish = job.get('publish') ?? ''
    expect(publish).toContain('--json isPrerelease')
    expect(publish).toContain('repos/$GITHUB_REPOSITORY/releases/latest')
  })

  it('attaches the signed artifacts to the shell release, but the manifest only to the feed', () => {
    const publish = job.get('publish') ?? ''
    expect(publish).toContain("find release-output -type f ! -name latest.json")
    expect(publish).toContain('gh release upload "$FEED_TAG" release-output/latest.json --repo "$GITHUB_REPOSITORY" --clobber')
    expect(publish).not.toMatch(/gh release create "\$RELEASE_TAG" release-output\/\*/)
  })

  it('never moves the feed back to an older version, and checks that the feed serves the signed manifest', () => {
    const publish = job.get('publish') ?? ''
    expect(publish).toContain('refusing to move it back')
    expect(publish).toContain('--pattern latest.json --dir "$RUNNER_TEMP/feed" --clobber')
    expect(publish).toContain('cmp "$RUNNER_TEMP/published/latest.json" release-output/latest.json')
  })

  it('runs the publication one at a time, because the feed is shared by every shell release', () => {
    expect(job.get('publish')).toContain('group: shell-updater-feed')
    expect(job.get('publish')).toMatch(/group: shell-updater-feed\n {6}cancel-in-progress: false/)
  })
})
