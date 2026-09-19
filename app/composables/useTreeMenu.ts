import type { Node } from '~/types'

export interface TreeMenuTarget {
  node: Node
  x: number
  y: number
}

// The app is SPA-only (`ssr: false`), so module-level state is safe here: there
// is no server request that could leak it between users.
const target = shallowRef<TreeMenuTarget | null>(null)
const renamingId = ref<string | null>(null)
/** Where an inline "new folder" input is open; `parent: null` is the root. */
const creatingFolder = shallowRef<{ parent: string | null } | null>(null)

/**
 * One context menu shared by the whole tree. `TreeNode` is recursive, so the
 * menu itself is rendered once by `Sidebar` rather than per row.
 */
export function useTreeMenu() {
  function openAt(event: MouseEvent, node: Node) {
    event.preventDefault()
    renamingId.value = null
    target.value = { node, x: event.clientX, y: event.clientY }
  }

  /** From a "more" button: anchor under the button, which also works from the keyboard. */
  function openBelow(anchor: HTMLElement, node: Node) {
    const box = anchor.getBoundingClientRect()
    renamingId.value = null
    target.value = { node, x: box.left, y: box.bottom + 4 }
  }

  function close() {
    target.value = null
  }

  function startRenaming(id: string) {
    target.value = null
    renamingId.value = id
  }

  function stopRenaming() {
    renamingId.value = null
  }

  function startCreatingFolder(parent: string | null) {
    target.value = null
    renamingId.value = null
    creatingFolder.value = { parent }
  }

  function stopCreatingFolder() {
    creatingFolder.value = null
  }

  return {
    target,
    renamingId,
    creatingFolder,
    openAt,
    openBelow,
    close,
    startRenaming,
    stopRenaming,
    startCreatingFolder,
    stopCreatingFolder,
  }
}

/** The folder a node currently sits in, or null for the collection root. */
export function parentOf(id: string): string | null {
  const cut = id.lastIndexOf('/')
  return cut === -1 ? null : id.slice(0, cut)
}

export type DropPosition = 'before' | 'after' | 'inside'

const dragId = ref<string | null>(null)
const dropTarget = shallowRef<{ id: string; position: DropPosition } | null>(null)

/**
 * Dropping a folder on itself or on anything inside it would detach the
 * subtree. The Rust side refuses it too; this keeps the cursor honest.
 */
export function canDrop(from: string, onto: string): boolean {
  return from !== onto && !onto.startsWith(`${from}/`)
}

/** Which third of the row the cursor is in. Requests have no inside. */
export function dropPositionFor(event: DragEvent, isFolder: boolean): DropPosition {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const ratio = (event.clientY - box.top) / box.height

  if (!isFolder) return ratio < 0.5 ? 'before' : 'after'
  if (ratio < 0.25) return 'before'
  if (ratio > 0.75) return 'after'
  return 'inside'
}

export function useTreeDrag() {
  function start(event: DragEvent, id: string) {
    dragId.value = id
    dropTarget.value = null
    event.dataTransfer?.setData('text/plain', id)
    if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
  }

  function over(event: DragEvent, id: string, isFolder: boolean) {
    const from = dragId.value
    // Text from outside — a curl command from a browser or an editor — may
    // land on a folder and become a request there.
    if (!from && event.dataTransfer?.types.includes('text/plain')) {
      event.preventDefault()
      event.dataTransfer.dropEffect = 'copy'
      dropTarget.value = { id, position: isFolder ? 'inside' : 'after' }
      return
    }
    if (!from || !canDrop(from, id)) {
      if (event.dataTransfer) event.dataTransfer.dropEffect = 'none'
      return
    }
    event.preventDefault()
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
    dropTarget.value = { id, position: dropPositionFor(event, isFolder) }
  }

  function end() {
    dragId.value = null
    dropTarget.value = null
  }

  /** The class the row should carry while it is the current drop target. */
  function markerFor(id: string): string {
    const target = dropTarget.value
    return target?.id === id ? `drop-${target.position}` : ''
  }

  return { dragId, dropTarget, start, over, end, markerFor }
}
