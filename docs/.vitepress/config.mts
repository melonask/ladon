import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Ladon', description: 'Configuration-driven multi-chain address pools', base: '/ladon/',
  themeConfig: {
    nav: [{ text: 'Guide', link: '/getting-started/' }, { text: 'Operations', link: '/responses/operations' }],
    sidebar: [
      { text: 'Getting started', items: [{ text: 'Overview', link: '/getting-started/' }, { text: 'Configuration', link: '/getting-started/configuration' }, { text: 'Usage', link: '/getting-started/usage' }] },
      { text: 'Reference', items: [{ text: 'Operations', link: '/responses/operations' }] }
    ]
  }
})
