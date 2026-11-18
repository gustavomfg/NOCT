import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

// The Rust shell validation workflow is read as text, like the other workflow checks. Git on Windows writes CRLF,
// so line ends are normalized before the checks. These checks keep the caching of the build from weakening: every
// action is pinned by commit, the cache is scoped to the system, the toolchain and the locks, and the trunk binary
// is reused only when it reports the pinned version.
const workflow = fs
  .readFileSync(path.join(process.cwd(), '.github/workflows/rust-shell-validation.yml'), 'utf8')
  .replace(/\r\n/g, '\n')

describe('rust shell validation workflow', () => {
  it('pins every action to a full commit SHA with its version as a comment', () => {
    const uses = workflow.match(/^\s*(?:- )?uses: .+$/gm) ?? []
    expect(uses.length).toBeGreaterThan(0)
    for (const line of uses) {
      expect(line).toMatch(/uses: [\w.-]+\/[\w.-]+@[0-9a-f]{40} # v\d+(\.\d+\.\d+)?$/)
    }
  })

  it('caches with the official cache action at the commit of its verified release', () => {
    expect(workflow).toContain('uses: actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9 # v6.1.0')
    expect(workflow.match(/uses: actions\/cache@/g)).toHaveLength(1)
  })

  it('scopes the cache key to the system, the architecture, the toolchain and the locks', () => {
    const key = /key: (.+)$/m.exec(workflow)?.[1] ?? ''
    expect(key).toContain('${{ runner.os }}')
    expect(key).toContain('${{ runner.arch }}')
    expect(key).toContain("hashFiles('rust-toolchain.toml', 'Cargo.lock')")
  })

  it('caches the paths that the build reads and writes, and no secret', () => {
    const block = /path: \|\n((?: {12}.+\n)+)/.exec(workflow)?.[1] ?? ''
    for (const expected of ['~/.cargo/registry', '~/.cargo/git', '~/.cargo/bin/trunk', 'target']) {
      expect(block).toContain(expected)
    }
    expect(block).not.toMatch(/secrets|\.env|credentials/)
  })

  it('reuses the trunk binary only when it reports the pinned version', () => {
    const install = /- name: Install trunk at a pinned version[\s\S]*?(?=\n {6}- name:)/.exec(workflow)?.[0] ?? ''
    expect(install).toContain('*0.22.1')
    expect(install).toContain('cargo install --locked trunk@0.22.1')
    // The final check fails the step when the binary still reports another version.
    expect(install.trimEnd().endsWith('[[ "$(trunk --version)" == *0.22.1 ]]')).toBe(true)
  })
})
