/**
 * One keydown listener for the window, with each binding registered by the
 * component that owns the action. Keeping them in a registry is what lets the
 * "?" sheet list what actually exists instead of a hand-written table that
 * quietly goes out of date.
 */
export interface Binding {
  /** `mod+p`, `mod+shift+n`, `/`, `?`. `mod` is ⌘ on macOS, Ctrl elsewhere. */
  keys: string
  label: string
  run: () => void
}

const bindings = shallowRef<Binding[]>([])

/** Register for as long as the calling component is mounted. */
export function useShortcut(keys: string, label: string, run: () => void) {
  const binding: Binding = { keys, label, run }
  onMounted(() => {
    bindings.value = [...bindings.value, binding]
  })
  onUnmounted(() => {
    bindings.value = bindings.value.filter((b) => b !== binding)
  })
}

/** For the help sheet, in the order they were registered. */
export function useShortcutList() {
  return computed(() => bindings.value)
}

function isTyping(target: EventTarget | null) {
  const el = target as HTMLElement | null
  if (!el) return false
  return el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)
}

/** Called once, from the shell. */
export function installShortcuts() {
  const onKey = (event: KeyboardEvent) => {
    // A dialog owns the keyboard while it is open: it has its own Esc and its
    // own focus trap, and a shortcut firing behind it is never what was meant.
    if (document.querySelector('.ui-backdrop')) return

    const mod = event.ctrlKey || event.metaKey
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key.toLowerCase()
    const combo = mod ? `mod+${event.shiftKey ? 'shift+' : ''}${key}` : event.key

    // Plain keys would otherwise type themselves into whatever has focus.
    if (!mod && isTyping(event.target)) return

    const binding = bindings.value.find((b) => b.keys === combo)
    if (!binding) return
    event.preventDefault()
    binding.run()
  }

  onMounted(() => window.addEventListener('keydown', onKey))
  onUnmounted(() => window.removeEventListener('keydown', onKey))
}

/** `mod+shift+n` → `Ctrl ⇧ N`, for showing a binding to the user. */
export function shortcutLabel(keys: string) {
  return keys
    .split('+')
    .map((part) => {
      if (part === 'mod') return modKey
      if (part === 'shift') return '⇧'
      if (part === 'enter') return '↵'
      return part.length === 1 ? part.toUpperCase() : part
    })
    .join(' ')
}
