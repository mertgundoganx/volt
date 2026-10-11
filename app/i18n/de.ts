import dialogs from './de/dialogs'
import request from './de/request'
import shell from './de/shell'
import store from './de/store'

/**
 * German. Keys are the English sentences as the components write them, kept
 * in one file per area of the app. A sentence used in two areas has to mean
 * the same thing in both: the later spread wins, so keep shared keys neutral.
 */
const table: Record<string, string> = { ...dialogs, ...store, ...shell, ...request }

export default table
