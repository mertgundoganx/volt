/** Mirrors `curl::is_curl`: is this pasted text a curl command? */
export function isCurl(text: string): boolean {
  const first = text.trimStart().replace(/^\$\s+/, '').split(/\s+/)[0] ?? ''
  const program = (first.split(/[\\/]/).pop() ?? '').toLowerCase()
  return program === 'curl' || program === 'curl.exe'
}

/** A file name base from a request name: `/v2/users` → `v2-users`. */
export function fileBase(name: string, fallback = 'request'): string {
  const base = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return base || fallback
}
