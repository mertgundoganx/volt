// Nuxt runs as a pure SPA here: Tauri serves the generated files from disk and
// there is no Node server in the bundle.
export default defineNuxtConfig({
  ssr: false,
  compatibilityDate: '2025-01-01',
  devtools: { enabled: false },
  modules: ['@pinia/nuxt'],
  // Fonts are bundled, not fetched: the app runs offline and its CSP only
  // allows same-origin resources. Plex Sans carries its weight axis; Plex Mono
  // ships as static cuts, and three are all the UI uses.
  css: [
    '@fontsource-variable/ibm-plex-sans/wght.css',
    '@fontsource/ibm-plex-mono/400.css',
    '@fontsource/ibm-plex-mono/500.css',
    '@fontsource/ibm-plex-mono/600.css',
    '~/assets/tokens.css',
    '~/assets/base.css',
  ],
  app: {
    head: {
      title: 'volt',
      meta: [{ name: 'viewport', content: 'width=device-width, initial-scale=1' }],
    },
  },
  // Tauri points its window at this exact port, so wandering to 3001 when 3000
  // is taken would open a blank window rather than the app.
  devServer: { port: 3000 },
  vite: {
    // Tauri watches the terminal output; clearing it hides the compile errors.
    clearScreen: false,
    server: { strictPort: true },
  },
})
