/**
 * What went wrong, in the user's words. The raw message from reqwest or the
 * OS is kept and shown underneath: it is what goes into a bug report, and a
 * translation that hides it would be worse than none.
 */

export interface Explained {
  /** One or two sentences: what happened and what to do about it. */
  text: string
  /** The message as it came, when it says more than `text` does. */
  raw: string | null
  /** The failure was the server's certificate, so a retry without verifying makes sense. */
  certificate: boolean
}

function hostOf(url: string | null | undefined): string | null {
  if (!url) return null
  try {
    return new URL(url).host || null
  } catch {
    // An unresolved `{{baseUrl}}` does not parse; say nothing rather than nonsense.
    return null
  }
}

export function explain(raw: string, url?: string | null, timeoutMs?: number): Explained {
  const host = hostOf(url)
  const at = host ? ` ${host}` : ''
  const lower = raw.toLowerCase()
  const keep = (text: string): Explained => ({ text, raw, certificate: false })

  if (lower === 'request failed: cancelled' || lower === 'cancelled') {
    return { text: 'Cancelled.', raw: null, certificate: false }
  }
  if (/certificate|self.signed|unknownissuer|invalid peer|tls handshake|hostname mismatch|notvalidforname/.test(lower)) {
    return {
      text: `${host ?? 'The server'} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.`,
      raw,
      certificate: true,
    }
  }
  if (/connection refused|os error 10061|econnrefused/.test(lower)) {
    return keep(`Could not connect to${at}: nothing is listening there. Check the URL and that the server is running.`)
  }
  if (/dns error|failed to lookup|no such host|name or service not known|os error 11001|getaddrinfo/.test(lower)) {
    return keep(`Could not find${at}. Check the URL for a typo, and that you are online.`)
  }
  if (/timed out|timeout|os error 10060/.test(lower)) {
    const wait = timeoutMs ? ` within ${Math.round(timeoutMs / 1000)} s` : ''
    return keep(`No response from${at}${wait}. The server may be slow or unreachable; the timeout is in Settings, and per request under Options.`)
  }
  if (/connection reset|os error 10054|broken pipe|connection closed|unexpected eof/.test(lower)) {
    return keep(`${host ?? 'The server'} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.`)
  }
  if (/network unreachable|os error 10051|no route to host/.test(lower)) {
    return keep(`No route to${at}. Check your network connection or proxy.`)
  }
  if (/proxy/.test(lower)) {
    return keep(`The proxy refused or could not reach${at}. Check the proxy in Settings.`)
  }
  if (/relative url without a base|invalid url|empty host|url parse/.test(lower)) {
    return keep('That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.')
  }
  return { text: raw, raw: null, certificate: false }
}
