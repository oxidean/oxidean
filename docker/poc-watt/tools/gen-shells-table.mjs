// Regenerate services/web/src/shells-table.js from crates/oxidean-web/src/shells.rs.
// Run from the repo root:  node docker/poc-watt/tools/gen-shells-table.mjs
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '../../..')
const src = readFileSync(join(ROOT, 'crates/oxidean-web/src/shells.rs'), 'utf8')

// Grab the table body between `static SHELLS: &[Shell] = &[` and the closing `];`
const start = src.indexOf('static SHELLS')
const open = src.indexOf('&[', src.indexOf('=', start))
const end = src.indexOf('];', open)
const body = src.slice(open + 2, end)

// Split into `Shell { ... }` blocks (strip // comments first). Blocks end at a
// `},` on its own line — `{param}` placeholders inside title strings would
// truncate a naive brace match.
const stripped = body.replace(/\/\/[^\n]*/g, '')
const blocks = [...stripped.matchAll(/Shell\s*\{([\s\S]*?)\n\s*\},/g)].map(m => m[1])

function parseSegs(pattern) {
  const inner = pattern.trim().replace(/^&\[/, '').replace(/\]$/, '')
  return [...inner.matchAll(/([LPS])\("([^"]+)"\)/g)].map(([, kind, name]) => ({
    kind,
    name,
  }))
}

function field(block, name) {
  const re = new RegExp(name + ':\\s*([^\\n]+),')
  const m = block.match(re)
  return m && m[1].trim()
}

const rows = []
for (const b of blocks) {
  const pattern = parseSegs(field(b, 'pattern'))
  const file = JSON.parse(field(b, 'file'))
  const titleRaw = field(b, 'title')
  const titleM = titleRaw.match(/Some\(("(?:[^"\\]|\\.)*")\)/)
  const title = titleM ? JSON.parse(titleM[1]) : null
  const protectedF = field(b, 'protected') === 'true'
  const redirRaw = field(b, 'redirect')
  const redirM = redirRaw.match(/Some\(("(?:[^"\\]|\\.)*")\)/)
  const redirect = redirM ? JSON.parse(redirM[1]) : null
  rows.push({
    pattern: pattern.map(s => (s.kind === 'L' ? { lit: s.name } : s.kind === 'P' ? { param: s.name } : { splat: s.name })),
    file,
    title,
    protected: protectedF,
    redirect,
  })
}

const out = `// GENERATED FILE — extracted from crates/oxidean-web/src/shells.rs by
// docker/poc-watt/tools/gen-shells-table.mjs. Regenerate after route changes
// before reviving the PoC.
export const SHELLS = ${JSON.stringify(rows, null, 2)}
`
writeFileSync(join(ROOT, 'docker/poc-watt/services/web/src/shells-table.js'), out)
console.log(`wrote ${rows.length} shell entries`)
