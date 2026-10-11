// Every language says everything. Three checks, none of which needs the app:
//
//   1. each language has the four files, with exactly the same keys;
//   2. a translation keeps its key's {placeholders} and is not empty;
//   3. every t('…') written in app/ has an entry — so a new string cannot ship
//      in English to seven languages without anybody noticing.
//
// Run: node tests/i18n.mjs   (exits 1 on any problem)
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const app = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'app')
const PARTS = ['dialogs', 'request', 'shell', 'store']
const languages = readdirSync(join(app, 'i18n')).filter((name) => statSync(join(app, 'i18n', name)).isDirectory())

/** A dictionary file is one object literal; read it as that. */
function load(language, part) {
  const source = readFileSync(join(app, 'i18n', language, `${part}.ts`), 'utf8')
  const body = source.slice(source.indexOf('export default') + 'export default'.length).replace(/\}\s*satisfies[^\n]*\s*$/, '}')
  return new Function(`return (${body})`)()
}
const holes = (text) => (text.match(/\{\w+\}/g) ?? []).sort().join(',')

let problems = 0
const say = (line) => { problems += 1; console.log(`  ${line}`) }

const tables = {}
for (const language of languages) {
  tables[language] = {}
  for (const part of PARTS) {
    try {
      tables[language][part] = load(language, part)
    } catch (error) {
      say(`${language}/${part}.ts cannot be read: ${error.message}`)
      tables[language][part] = {}
    }
  }
}

// 1 and 2: against the first language, which is as good a reference as any.
const reference = languages[0]
for (const language of languages) {
  for (const part of PARTS) {
    const base = tables[reference][part]
    const mine = tables[language][part]
    for (const key of Object.keys(base)) if (!(key in mine)) say(`${language}/${part}: missing ${JSON.stringify(key)}`)
    for (const [key, value] of Object.entries(mine)) {
      if (!(key in base)) say(`${language}/${part}: ${JSON.stringify(key)} is in no other language`)
      if (typeof value !== 'string' || !value.length) say(`${language}/${part}: ${JSON.stringify(key)} is empty`)
      else if (holes(value) !== holes(key)) say(`${language}/${part}: placeholders differ in ${JSON.stringify(key)}`)
    }
  }
}

// 3: the keys the code asks for. Only literal first arguments can be checked;
// a key built at run time has to be listed by hand where it is built.
const known = new Set(PARTS.flatMap((part) => Object.keys(tables[reference][part])))
const sources = []
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) {
      if (name !== 'i18n' && name !== 'node_modules') walk(path)
    } else if (/\.(vue|ts)$/.test(name)) sources.push(path)
  }
}
walk(app)
const CALL = /(?<![\w.$])t\(\s*(?:'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\$]|\\.)*)`)\s*[,)]/g
let used = 0
for (const file of sources) {
  const text = readFileSync(file, 'utf8')
  for (const match of text.matchAll(CALL)) {
    const raw = match[1] ?? match[2] ?? match[3]
    const key = raw.replace(/\\(['"`\\])/g, '$1').replace(/\\n/g, '\n')
    used += 1
    if (!known.has(key)) say(`${file.slice(app.length + 1)}: no translation for ${JSON.stringify(key)}`)
  }
}

console.log(`${languages.length} languages (${languages.join(', ')}), ${known.size} sentences, ${used} uses in the code`)
console.log(problems ? `${problems} PROBLEMS` : 'ALL PASSED')
process.exit(problems ? 1 : 0)
