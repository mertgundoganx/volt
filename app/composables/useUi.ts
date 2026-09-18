/** Shortcut labels follow the platform: ⌘ on macOS, Ctrl elsewhere. */
export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.userAgent)
export const modKey = isMac ? '⌘' : 'Ctrl'

/**
 * A number kept in localStorage, for layout the user adjusts by hand (pane
 * sizes). Storage can be unavailable; the value then simply does not persist.
 */
export function usePersistentNumber(key: string, fallback: number, min: number, max: number) {
  const clamp = (n: number) => Math.min(max, Math.max(min, n))

  let initial = fallback
  try {
    const stored = Number(localStorage.getItem(key))
    if (Number.isFinite(stored) && stored !== 0) initial = clamp(stored)
  } catch {
    // Private storage or blocked: use the default.
  }

  const value = ref(initial)
  watch(value, (next) => {
    const clamped = clamp(next)
    if (clamped !== next) {
      value.value = clamped
      return
    }
    try {
      localStorage.setItem(key, String(clamped))
    } catch {
      // Not persisting is fine.
    }
  })

  return { value, reset: () => (value.value = fallback) }
}

export function humanSize(bytes: number): { value: string; unit: string } {
  if (bytes < 1024) return { value: String(bytes), unit: 'B' }
  if (bytes < 1024 * 1024) return { value: (bytes / 1024).toFixed(1), unit: 'KB' }
  return { value: (bytes / 1024 / 1024).toFixed(2), unit: 'MB' }
}
