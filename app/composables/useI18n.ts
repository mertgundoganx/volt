import de from '~/i18n/de'
import es from '~/i18n/es'
import it from '~/i18n/it'
import nl from '~/i18n/nl'
import pt from '~/i18n/pt'
import ru from '~/i18n/ru'
import tr from '~/i18n/tr'

/**
 * The interface in more than one language, with the English sentence itself
 * as the key: `t('Save request')`. A component stays readable, a string that
 * has no translation yet shows in English rather than as a key, and there is
 * no table of identifiers to keep in step with the text.
 *
 * Values go in by name — `t('{n} requests', { n })` — because word order is
 * the first thing that differs between languages.
 *
 * One English word is sometimes two words elsewhere: "Call" is a tab and a
 * key to press. The second use says which it is after a bar — `t('Call|verb')`
 * — and the bar and what follows are never shown.
 *
 * What comes from outside the interface is not translated: a server's reply,
 * a file name, an error message from the operating system.
 */
export type Language = 'en' | 'de' | 'es' | 'it' | 'nl' | 'pt' | 'ru' | 'tr'

/** Each under its own name, in the order of those names. */
export const LANGUAGES: { value: Language; label: string }[] = [
  { value: 'de', label: 'Deutsch' },
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
  { value: 'it', label: 'Italiano' },
  { value: 'nl', label: 'Nederlands' },
  { value: 'pt', label: 'Português' },
  { value: 'ru', label: 'Русский' },
  { value: 'tr', label: 'Türkçe' },
]

const TABLES: Record<Language, Record<string, string> | null> = { en: null, de, es, it, nl, pt, ru, tr }

export const uiLanguage = ref<Language>('en')

/** The language the operating system asks for, when volt has it. */
export function systemLanguage(): Language {
  const wanted = (typeof navigator === 'undefined' ? 'en' : navigator.language).toLowerCase().split('-')[0]
  return LANGUAGES.some((one) => one.value === wanted) ? (wanted as Language) : 'en'
}

export function t(text: string, values?: Record<string, string | number>): string {
  const found = TABLES[uiLanguage.value]?.[text]
  // No translation: the English, without the note after the bar.
  const bar = found === undefined ? text.indexOf('|') : -1
  const said = found ?? (bar === -1 ? text : text.slice(0, bar))
  if (!values) return said
  return said.replace(/\{(\w+)\}/g, (whole, name: string) => (name in values ? String(values[name]) : whole))
}
