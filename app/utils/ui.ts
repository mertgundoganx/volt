import type { IconName } from './icons'

export interface TabItem {
  key: string
  label: string
  /** Shown in mono after the label: a count, or a short state like `json`. */
  meta?: string | number | null
  /** A small mark for "this has content" when a count would be noise. */
  dot?: boolean
}

export interface SegmentOption<V extends string = string> {
  value: V
  label: string
  icon?: IconName
}

export interface MenuItem {
  key: string
  label: string
  icon?: IconName
  hint?: string
  disabled?: boolean
  /** Draw a separator above this item. */
  divided?: boolean
}

export type Tone = 'ok' | 'warn' | 'bad' | 'off'

/** One reading of an HTTP status, shared by the response readout and history. */
export function statusTone(status: number | null | undefined, failed = false): Tone {
  if (failed) return 'bad'
  if (!status) return 'off'
  if (status >= 500) return 'bad'
  if (status >= 400) return 'warn'
  if (status >= 200 && status < 400) return 'ok'
  return 'off'
}
