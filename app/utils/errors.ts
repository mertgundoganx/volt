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
  // A URL still holding `{{baseUrl}}` has no host to name; say "the server".
  const lower = raw.toLowerCase()
  const keep = (text: string): Explained => ({ text, raw, certificate: false })

  if (lower === 'request failed: cancelled' || lower === 'cancelled') {
    return { text: t('Cancelled.'), raw: null, certificate: false }
  }
  if (/certificate|self.signed|unknownissuer|invalid peer|tls handshake|hostname mismatch|notvalidforname/.test(lower)) {
    return {
      text: host
        ? t('{host} sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.', { host })
        : t('The server sent a certificate volt does not trust — self-signed, expired, or for another name. For a host you control, send without verifying it.'),
      raw,
      certificate: true,
    }
  }
  if (/connection refused|os error 10061|econnrefused/.test(lower)) {
    return keep(
      host
        ? t('Could not connect to {host}: nothing is listening there. Check the URL and that the server is running.', { host })
        : t('Could not connect to the server: nothing is listening there. Check the URL and that the server is running.'),
    )
  }
  if (/dns error|failed to lookup|no such host|name or service not known|os error 11001|getaddrinfo/.test(lower)) {
    return keep(
      host
        ? t('Could not find {host}. Check the URL for a typo, and that you are online.', { host })
        : t('Could not find the server. Check the URL for a typo, and that you are online.'),
    )
  }
  if (/timed out|timeout|os error 10060/.test(lower)) {
    if (timeoutMs) {
      const seconds = Math.round(timeoutMs / 1000)
      return keep(
        host
          ? t('No response from {host} within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.', { host, seconds })
          : t('No response from the server within {seconds} s. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.', { seconds }),
      )
    }
    return keep(
      host
        ? t('No response from {host}. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.', { host })
        : t('No response from the server. The server may be slow or unreachable; the timeout is in Settings, and per request in its Settings tab.'),
    )
  }
  if (/connection reset|os error 10054|broken pipe|connection closed|unexpected eof/.test(lower)) {
    return keep(
      host
        ? t('{host} closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.', { host })
        : t('The server closed the connection before answering. Try again; if it keeps happening the server is rejecting the request early.'),
    )
  }
  if (/network unreachable|os error 10051|no route to host/.test(lower)) {
    return keep(
      host
        ? t('No route to {host}. Check your network connection or proxy.', { host })
        : t('No route to the server. Check your network connection or proxy.'),
    )
  }
  if (/proxy/.test(lower)) {
    return keep(
      host
        ? t('The proxy refused or could not reach {host}. Check the proxy in Settings.', { host })
        : t('The proxy refused or could not reach the server. Check the proxy in Settings.'),
    )
  }
  if (/relative url without a base|invalid url|empty host|url parse/.test(lower)) {
    return keep(t('That is not a URL volt can send to. It needs a scheme and a host, like https://api.example.com/path.'))
  }
  return { text: raw, raw: null, certificate: false }
}
