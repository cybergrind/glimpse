import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Glimpse',
  description: 'A polished desktop shell toolkit for Niri.',
  base: '/glimpse/',
  cleanUrls: true,
  srcExclude: ['packaging.md', 'superpowers/**'],
  themeConfig: {
    nav: [
      { text: 'Motivation', link: '/motivation' },
      { text: 'Installation', link: '/installation' },
      { text: 'Configuration', link: '/configuration' },
      { text: 'Applets', link: '/applets/' },
      { text: 'Theming', link: '/theming' },
      { text: 'Wallpaper', link: '/wallpaper' },
      { text: 'Lock', link: '/lock' },
      { text: 'GitHub', link: 'https://github.com/alex-oleshkevich/glimpse' }
    ],
    sidebar: [
      {
        text: 'Start Here',
        items: [
          { text: 'Introduction', link: '/' },
          { text: 'Motivation', link: '/motivation' },
          { text: 'Installation', link: '/installation' },
          { text: 'Configuration', link: '/configuration' },
          { text: 'Theming', link: '/theming' }
        ]
      },
      {
        text: 'Configuration',
        items: [
          { text: 'Panels and Applets', link: '/configuration' },
          { text: 'Wallpaper', link: '/wallpaper' },
          { text: 'Lock Screen', link: '/lock' }
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
          { text: 'Sunset', link: '/sunset' }
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
