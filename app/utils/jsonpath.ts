/**
 * A path into a JSON response, for narrowing what the viewer shows:
 * `$.items[*].id`, `items[0].tags`, `$..email`.
 *
 * Wider than what a capture reads on purpose. A capture has to name one
 * value, so it takes one path; looking at a response is the opposite — "all
 * the ids" is the common question — so `[*]`, `.*` and `..name` fan out here.
 * There are no filter expressions: a predicate language inside a text field
 * is a script by another name.
 */

type Step = { kind: 'key'; name: string } | { kind: 'index'; at: number } | { kind: 'all' } | { kind: 'deep'; name: string }

export interface Filtered {
  /** Undefined when the path could not be read. */
  value?: unknown
  /** How many values the path matched; 1 for a plain path that found something. */
  count: number
  error?: string
}

function parse(path: string): Step[] | string {
  const steps: Step[] = []
  let rest = path.trim()
  if (rest.startsWith('$')) rest = rest.slice(1)
  while (rest.length) {
    let match: RegExpExecArray | null
    if ((match = /^\.\.([^.[\]*]+)/.exec(rest))) steps.push({ kind: 'deep', name: match[1]! })
    else if ((match = /^\.?\*/.exec(rest)) || (match = /^\[\*\]/.exec(rest))) steps.push({ kind: 'all' })
    else if ((match = /^\[(-?\d+)\]/.exec(rest))) steps.push({ kind: 'index', at: Number(match[1]) })
    else if ((match = /^\[(?:"([^"]*)"|'([^']*)')\]/.exec(rest))) steps.push({ kind: 'key', name: match[1] ?? match[2] ?? '' })
    // Any run of characters up to the next dot or bracket: a key may be
    // `şehir`, `first name` or `2024`, and the tree offers such paths to copy.
    else if ((match = /^\.?([^.[\]]+)/.exec(rest))) steps.push({ kind: 'key', name: match[1]! })
    else return t('Cannot read the path from “{rest}”', { rest: rest.slice(0, 16) })
    rest = rest.slice(match[0].length)
  }
  return steps
}

function everywhere(value: unknown, name: string, out: unknown[]) {
  if (Array.isArray(value)) {
    for (const item of value) everywhere(item, name, out)
  } else if (value && typeof value === 'object') {
    for (const [key, inner] of Object.entries(value)) {
      if (key === name) out.push(inner)
      everywhere(inner, name, out)
    }
  }
}

export function filterJson(root: unknown, path: string): Filtered {
  const steps = parse(path)
  if (typeof steps === 'string') return { count: 0, error: steps }

  let current: unknown[] = [root]
  // Once a step fans out, the answer is a list even when it holds one thing.
  let many = false
  for (const step of steps) {
    const next: unknown[] = []
    for (const value of current) {
      if (step.kind === 'deep') {
        everywhere(value, step.name, next)
      } else if (step.kind === 'all') {
        // One at a time: spreading a very long array into push overflows the stack.
        const items = Array.isArray(value) ? value : value && typeof value === 'object' ? Object.values(value) : []
        for (const item of items) next.push(item)
      } else if (step.kind === 'index') {
        if (Array.isArray(value)) {
          const item = value[step.at < 0 ? value.length + step.at : step.at]
          if (item !== undefined) next.push(item)
        }
      } else if (value && typeof value === 'object' && !Array.isArray(value) && Object.hasOwn(value, step.name)) {
        next.push((value as Record<string, unknown>)[step.name])
      }
    }
    if (step.kind === 'deep' || step.kind === 'all') many = true
    current = next
  }
  if (!current.length) return { count: 0 }
  return many ? { value: current, count: current.length } : { value: current[0], count: 1 }
}
