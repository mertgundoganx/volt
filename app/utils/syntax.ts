/**
 * A small JSON highlighter for the response viewer. It works on text that is
 * already pretty-printed and returns lines of typed tokens, which components
 * render as spans; nothing here produces HTML, so response bodies are never
 * injected into the page.
 */

export type TokenKind = 'key' | 'str' | 'num' | 'lit' | 'punc' | 'text'
export interface Token {
  text: string
  kind: TokenKind
  /** Set by `markMatches`: this token is search hit number n. */
  hit?: number
}
export type Line = Token[]

/** Above this many characters, highlighting costs more than it helps. */
export const HIGHLIGHT_LIMIT = 300_000

const JSON_TOKEN = /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b|([{}[\],:])/g

export function plainLines(text: string): Line[] {
  return text.split('\n').map((line) => [{ text: line, kind: 'text' as const }])
}

export function jsonLines(text: string): Line[] {
  if (text.length > HIGHLIGHT_LIMIT) return plainLines(text)

  const lines: Line[] = [[]]
  const push = (value: string, kind: TokenKind) => {
    // Only whitespace between tokens holds newlines in pretty-printed JSON,
    // but splitting every piece keeps odd input from breaking the layout.
    const parts = value.split('\n')
    parts.forEach((part, i) => {
      if (i > 0) lines.push([])
      if (part) lines[lines.length - 1]!.push({ text: part, kind })
    })
  }

  let last = 0
  for (const match of text.matchAll(JSON_TOKEN)) {
    const start = match.index ?? 0
    if (start > last) push(text.slice(last, start), 'text')
    const [whole, str, colon, num, lit, punc] = match
    if (str !== undefined) {
      push(str, colon ? 'key' : 'str')
      if (colon) push(colon, 'punc')
    } else if (num !== undefined) {
      push(num, 'num')
    } else if (lit !== undefined) {
      push(lit, 'lit')
    } else if (punc !== undefined) {
      push(punc, 'punc')
    }
    last = start + whole.length
  }
  if (last < text.length) push(text.slice(last), 'text')
  return lines
}

/** Pretty-print when the text is JSON; otherwise hand it back untouched. */
export function prettyJson(text: string): { text: string; isJson: boolean } {
  const trimmed = text.trimStart()
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) return { text, isJson: false }
  try {
    return { text: JSON.stringify(JSON.parse(text), null, 2), isJson: true }
  } catch {
    return { text, isJson: false }
  }
}

/**
 * Find in the body. Matching is literal and case-insensitive: a response is
 * data, so a regex would be a trap rather than a feature.
 */
export function countMatches(text: string, query: string): number {
  if (!query) return 0
  const haystack = text.toLowerCase()
  const needle = query.toLowerCase()
  let count = 0
  for (let at = haystack.indexOf(needle); at !== -1; at = haystack.indexOf(needle, at + needle.length)) count += 1
  return count
}

/**
 * Split already-highlighted lines so that every occurrence of `query` is a
 * token of its own, numbered from 0 across the whole body. Still tokens, never
 * HTML — the rule the highlighter exists to keep.
 */
export function markMatches(lines: Line[], query: string): Line[] {
  if (!query) return lines
  const needle = query.toLowerCase()
  let hit = 0

  return lines.map((line) =>
    line.flatMap((token) => {
      const lower = token.text.toLowerCase()
      if (!lower.includes(needle)) return [token]

      const parts: Token[] = []
      let from = 0
      for (let at = lower.indexOf(needle); at !== -1; at = lower.indexOf(needle, from)) {
        if (at > from) parts.push({ text: token.text.slice(from, at), kind: token.kind })
        parts.push({ text: token.text.slice(at, at + needle.length), kind: token.kind, hit: hit++ })
        from = at + needle.length
      }
      if (from < token.text.length) parts.push({ text: token.text.slice(from), kind: token.kind })
      return parts
    }),
  )
}
