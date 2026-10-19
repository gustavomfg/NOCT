import fs from 'node:fs'
import path from 'node:path'

const root = process.cwd()
const pkg = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8'))
const releaseFile = path.join(root, 'docs/releases', `v${pkg.version}.md`)
const checks = [
  ['README.md', `v${pkg.version}`],
  [`docs/releases/v${pkg.version}.md`, `v${pkg.version}`],
]

if (!fs.existsSync(releaseFile)) throw new Error(`Notas da versão ausentes: ${path.relative(root, releaseFile)}`)
for (const [file, expected] of checks) {
  const content = fs.readFileSync(path.join(root, file), 'utf8')
  if (!content.includes(expected)) throw new Error(`${file} diverge da fonte canônica; valor esperado: ${expected}`)
}
process.stdout.write(`Metadados consistentes: v${pkg.version}.\n`)
