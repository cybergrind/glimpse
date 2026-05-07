import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Glimpse',
  description: 'A Wayland shell for Niri, built from small focused services.',
  base: '/glimpse/',
  cleanUrls: true,
  srcExclude: ['packaging.md', 'superpowers/**'],
  themeConfig: {
    nav: [
      { text: 'Motivation', link: '/motivation' },
      { text: 'Installation', link: '/installation' },
      { text: 'Configuration', link: '/configuration' },
      { text: 'Applets', link: '/applets/' },
      { text: 'Theming', link: '/theming' }
    ],
    sidebar: [
      {
        text: 'Start Here',
        items: [
          { text: 'Introduction', link: '/' },
          { text: 'Motivation', link: '/motivation' },
          { text: 'Installation', link: '/installation' },
          { text: 'Theming', link: '/theming' }
        ]
      },
      {
        text: 'Configuration',
        items: [
          { text: 'Panels', link: '/configuration' }
        ]
      },
      {
        text: 'Applets',
        items: [
          { text: 'Applet Reference', link: '/applets/' },
          { text: 'Command Applet', link: '/custom-applets/command' },
          { text: 'Exec Applet', link: '/custom-applets/exec' },
          { text: 'Exec SDK', link: '/applets/exec-sdk' }
        ]
      },
      {
        text: 'Services',
        items: [
          { text: 'Idle', link: '/idle' },
          { text: 'Sunset', link: '/sunset' },
          { text: 'Lock', link: '/lock' },
          { text: 'Wallpaper', link: '/wallpaper' }
        ]
      }
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/alex-oleshkevich/glimpse' }
    ],
    search: {
      provider: 'local'
    }
  }
})
