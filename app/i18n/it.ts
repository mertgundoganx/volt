import dialogs from './it/dialogs'
import request from './it/request'
import shell from './it/shell'
import store from './it/store'

/**
 * Italian. Keys are the English sentences as the components write them, kept
 * in one file per area of the app. A sentence used in two areas has to mean
 * the same thing in both: the later spread wins, so keep shared keys neutral.
 */
const table: Record<string, string> = { ...dialogs, ...store, ...shell, ...request }

export default table
