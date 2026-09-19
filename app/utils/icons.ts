/**
 * The icon set. Drawn for this app on a 16px grid: 1.5px strokes, round caps
 * and joins, no fills except where marked `class="fill"`. Keep new icons on
 * the same grid and weight, and prefer a word over an icon when a control's
 * meaning would otherwise need a tooltip to guess.
 *
 * These strings are constants rendered with v-html; never build one from input.
 */
export const ICONS = {
  'chevron-down': '<path d="M4 6l4 4 4-4"/>',
  // The mark: a bolt, filled, two strokes of a pen.
  bolt: '<path class="fill" d="M9.4 1.25L3.25 9.1h4.2l-1.1 5.65 6.4-8.2H8.5z"/>',
  // A request list: a short method stub before each line.
  requests: '<path d="M2.5 4.25h2.25M7 4.25h6.5M2.5 8h2.25M7 8h6.5M2.5 11.75h2.25M7 11.75h6.5"/>',
  globe: '<circle cx="8" cy="8" r="6.25"/><path d="M1.75 8h12.5M8 1.75c2.1 2 2.1 10.5 0 12.5M8 1.75c-2.1 2-2.1 10.5 0 12.5"/>',
  keyboard: '<rect x="1.75" y="4.25" width="12.5" height="7.5" rx="1.5"/><path d="M4.25 6.75h.5M6.75 6.75h.5M9.25 6.75h.5M11.75 6.75h.5M5 9.25h6"/>',
  send: '<path d="M2.5 8h10M8.5 4l4 4-4 4"/>',
  'chevron-up': '<path d="M4 10l4-4 4 4"/>',
  'chevron-right': '<path d="M6 4l4 4-4 4"/>',
  folder: '<path d="M1.75 4.25a1 1 0 0 1 1-1h3.1l1.6 1.6h5.8a1 1 0 0 1 1 1v6.4a1 1 0 0 1-1 1H2.75a1 1 0 0 1-1-1z"/>',
  'folder-plus': '<path d="M1.75 4.25a1 1 0 0 1 1-1h3.1l1.6 1.6h5.8a1 1 0 0 1 1 1v6.4a1 1 0 0 1-1 1H2.75a1 1 0 0 1-1-1z"/><path d="M8 7.25v3.5M6.25 9h3.5"/>',
  'folder-open': '<path d="M1.75 12V4.25a1 1 0 0 1 1-1h3.1l1.6 1.6h4.3a1 1 0 0 1 1 1v1"/><path d="M1.75 12l1.6-4.35a1 1 0 0 1 .94-.65h9.46a.75.75 0 0 1 .7 1l-1.4 3.8a1 1 0 0 1-.94.65H2.75A1 1 0 0 1 1.75 12z"/>',
  file: '<path d="M4 1.75h5l3 3v8.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V2.75a1 1 0 0 1 1-1z"/><path d="M9 1.75v3h3"/>',
  'file-plus': '<path d="M4 1.75h5l3 3v8.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V2.75a1 1 0 0 1 1-1z"/><path d="M9 1.75v3h3M7.5 7.25v4M5.5 9.25h4"/>',
  cookie:
    '<circle cx="8" cy="8" r="6.25"/><circle class="fill" cx="6" cy="6.5" r="0.85"/><circle class="fill" cx="9.9" cy="7.6" r="0.85"/><circle class="fill" cx="7" cy="10.2" r="0.85"/>',
  'file-down':
    '<path d="M4 1.75h5l3 3v8.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V2.75a1 1 0 0 1 1-1z"/><path d="M9 1.75v3h3"/><path d="M7.5 7.25v3.5M5.9 9.35l1.6 1.6 1.6-1.6"/>',
  collection: '<rect x="2.25" y="2.25" width="11.5" height="11.5" rx="2"/><path d="M8 5.25v5.5M5.25 8h5.5"/>',
  import: '<path d="M8 2.25v7.5M5 6.75l3 3 3-3"/><path d="M2.25 10.25v2a1.5 1.5 0 0 0 1.5 1.5h8.5a1.5 1.5 0 0 0 1.5-1.5v-2"/>',
  history: '<path d="M2.6 8.3A5.4 5.4 0 1 0 4.2 4.2"/><path d="M2.4 2.6v2.7h2.7M8 5.25V8l2 1.3"/>',
  x: '<path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/>',
  plus: '<path d="M8 3.5v9M3.5 8h9"/>',
  more: '<circle class="fill" cx="3.5" cy="8" r="1.15"/><circle class="fill" cx="8" cy="8" r="1.15"/><circle class="fill" cx="12.5" cy="8" r="1.15"/>',
  copy: '<rect x="5.5" y="5.5" width="8.25" height="8.25" rx="1.5"/><path d="M10.5 3.5v-.25A1.25 1.25 0 0 0 9.25 2h-6A1.25 1.25 0 0 0 2 3.25v6a1.25 1.25 0 0 0 1.25 1.25h.25"/>',
  check: '<path d="M3.25 8.5l3 3 6.5-7"/>',
  sliders: '<path d="M2.5 5h5.25M11.75 5h1.75M2.5 11h1.75M8.25 11h5.25"/><circle cx="9.75" cy="5" r="2"/><circle cx="6.25" cy="11" r="2"/>',
  pencil: '<path d="M10.4 2.9a1.5 1.5 0 0 1 2.1 0l.6.6a1.5 1.5 0 0 1 0 2.1L6 12.7l-3.25.55.55-3.25z"/><path d="M9.25 4.1l2.65 2.65"/>',
  trash: '<path d="M2.75 4.25h10.5M6.25 4.25V3a.75.75 0 0 1 .75-.75h2a.75.75 0 0 1 .75.75v1.25"/><path d="M4 4.25l.62 8.6a1 1 0 0 0 1 .9h4.76a1 1 0 0 0 1-.9l.62-8.6M6.75 7v4M9.25 7v4"/>',
  move: '<path d="M3.25 2.75v5.5a2 2 0 0 0 2 2h7.5"/><path d="M10 7.25l2.75 3-2.75 3"/>',
  lock: '<rect x="3" y="7" width="10" height="6.75" rx="1.5"/><path d="M5.25 7V5.25a2.75 2.75 0 0 1 5.5 0V7"/>',
  unlock: '<rect x="3" y="7" width="10" height="6.75" rx="1.5"/><path d="M5.25 7V5.25a2.75 2.75 0 0 1 5.3-1"/>',
  eye: '<path d="M1.75 8S4 3.75 8 3.75 14.25 8 14.25 8 12 12.25 8 12.25 1.75 8 1.75 8z"/><circle cx="8" cy="8" r="2"/>',
  'eye-off': '<path d="M2.25 2.25l11.5 11.5M6.5 3.95A6 6 0 0 1 8 3.75C12 3.75 14.25 8 14.25 8a11.4 11.4 0 0 1-1.9 2.4M4.25 5.1C2.65 6.3 1.75 8 1.75 8S4 12.25 8 12.25a6.2 6.2 0 0 0 2.9-.7M6.6 6.6a2 2 0 0 0 2.8 2.8"/>',
  warning: '<path d="M7.13 2.75a1 1 0 0 1 1.74 0l5.4 9.5a1 1 0 0 1-.87 1.5H2.6a1 1 0 0 1-.87-1.5z"/><path d="M8 6.25v3M8 11.35v.15"/>',
  info: '<circle cx="8" cy="8" r="5.75"/><path d="M8 7.25v4M8 4.85v.15"/>',
  'arrow-up-right': '<path d="M5 11l6-6M6.25 5H11v4.75"/>',
  'arrow-right': '<path d="M3 8h10M9.5 4.5L13 8l-3.5 3.5"/>',
  sun: '<circle cx="8" cy="8" r="2.75"/><path d="M8 1.75v1.25M8 13v1.25M1.75 8H3M13 8h1.25M3.58 3.58l.88.88M11.54 11.54l.88.88M3.58 12.42l.88-.88M11.54 4.46l.88-.88"/>',
  moon: '<path d="M13.25 9.6A5.5 5.5 0 1 1 6.4 2.75a4.4 4.4 0 0 0 6.85 6.85z"/>',
  monitor: '<rect x="1.75" y="2.75" width="12.5" height="8.5" rx="1.5"/><path d="M5.5 13.75h5M8 11.25v2.5"/>',
  braces: '<path d="M5.5 2.75H5a1.5 1.5 0 0 0-1.5 1.5v2A1.75 1.75 0 0 1 1.75 8 1.75 1.75 0 0 1 3.5 9.75v2A1.5 1.5 0 0 0 5 13.25h.5M10.5 2.75h.5a1.5 1.5 0 0 1 1.5 1.5v2A1.75 1.75 0 0 0 14.25 8a1.75 1.75 0 0 0-1.75 1.75v2a1.5 1.5 0 0 1-1.5 1.5h-.5"/>',
  wrap: '<path d="M2.5 4h11M2.5 8h8.75a2.25 2.25 0 0 1 0 4.5H9M10.5 11l-1.5 1.5 1.5 1.5M2.5 12.5h3.5"/>',
  search: '<circle cx="7" cy="7" r="4.5"/><path d="M10.4 10.4l3.35 3.35"/>',
  key: '<circle cx="5.25" cy="10.75" r="3"/><path d="M7.4 8.6l5.85-5.85M11.25 4.75l1.75 1.75M9.5 6.5l1.25 1.25"/>',
  grip: '<path d="M5 6.5h6M5 9.5h6"/>',
  // Two readings side by side: a plus over a minus, the shape of a change.
  diff: '<path d="M3 5.25h6.5M6.25 2v6.5M3 12h6.5"/><path d="M12.75 4v8"/>',
} as const

export type IconName = keyof typeof ICONS
