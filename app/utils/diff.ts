/**
 * A line diff for two response bodies: what changed since the last send.
 *
 * Longest common subsequence over lines, which is the plain thing and reads
 * the way people expect (`-` gone, `+` new). It is quadratic, so it is capped:
 * past `DIFF_LIMIT` lines on either side the answer is "too large" rather than
 * a frozen window. Runs of unchanged lines longer than `CONTEXT * 2 + 1` fold
 * into one row saying how many were skipped.
 */

export type DiffKind = 'same' | 'gone' | 'new' | 'skip'
export interface DiffRow {
  kind: DiffKind
  text: string
  /** Line numbers in the old and new text, when the row is a real line. */
  before?: number
  after?: number
  /** For a `skip` row: how many unchanged lines it stands for. */
  count?: number
}

export const DIFF_LIMIT = 2500
const CONTEXT = 3

export function diffLines(before: string, after: string): { rows: DiffRow[]; changed: number; tooLarge: boolean } {
  const a = before.split('\n')
  const b = after.split('\n')
  if (a.length > DIFF_LIMIT || b.length > DIFF_LIMIT) return { rows: [], changed: 0, tooLarge: true }

  // lcs[i][j] = length of the common subsequence of a[i..] and b[j..].
  const n = a.length
  const m = b.length
  const width = m + 1
  const lcs = new Uint16Array((n + 1) * width)
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i * width + j] = a[i] === b[j] ? lcs[(i + 1) * width + j + 1]! + 1 : Math.max(lcs[(i + 1) * width + j]!, lcs[i * width + j + 1]!)
    }
  }

  const raw: DiffRow[] = []
  let i = 0
  let j = 0
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      raw.push({ kind: 'same', text: a[i]!, before: i + 1, after: j + 1 })
      i++
      j++
    } else if (lcs[(i + 1) * width + j]! >= lcs[i * width + j + 1]!) {
      raw.push({ kind: 'gone', text: a[i]!, before: i + 1 })
      i++
    } else {
      raw.push({ kind: 'new', text: b[j]!, after: j + 1 })
      j++
    }
  }
  while (i < n) raw.push({ kind: 'gone', text: a[i]!, before: ++i })
  while (j < m) raw.push({ kind: 'new', text: b[j]!, after: ++j })

  const changed = raw.filter((row) => row.kind !== 'same').length
  if (!changed) return { rows: [], changed: 0, tooLarge: false }

  // Fold long unchanged runs, keeping a few lines of context on each side.
  const rows: DiffRow[] = []
  let run: DiffRow[] = []
  const flush = (atEnd: boolean, atStart: boolean) => {
    const keepHead = atStart ? 0 : CONTEXT
    const keepTail = atEnd ? 0 : CONTEXT
    if (run.length > keepHead + keepTail + 1) {
      rows.push(...run.slice(0, keepHead))
      rows.push({ kind: 'skip', text: '', count: run.length - keepHead - keepTail })
      if (keepTail) rows.push(...run.slice(-keepTail))
    } else {
      rows.push(...run)
    }
    run = []
  }
  let started = false
  for (const row of raw) {
    if (row.kind === 'same') {
      run.push(row)
    } else {
      if (run.length) flush(false, !started)
      rows.push(row)
      started = true
    }
  }
  if (run.length) flush(true, !started)
  return { rows, changed, tooLarge: false }
}
