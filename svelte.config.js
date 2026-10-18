import adapter from '@sveltejs/adapter-static'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    // Tauri 内嵌静态资源：SPA fallback 到 index.html
    adapter: adapter({
      fallback: 'index.html'
    })
  }
}

export default config
