export default {
  title: 'OpenAI Proxy',
  tagline: 'KnowMe console and OpenAI-compatible proxy',
  url: 'https://gqadonis.github.io',
  baseUrl: '/openai-proxy/',
  organizationName: 'GQAdonis',
  projectName: 'openai-proxy',
  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',
  presets: [
    ['classic', {
      docs: {
        path: '../docs',
        routeBasePath: '/',
        sidebarPath: './sidebars.mjs',
        editUrl: ({docPath}) => `https://github.com/GQAdonis/openai-proxy/edit/main/docs/${docPath}`,
      },
      blog: false,
    }],
  ],
  themeConfig: {
    navbar: {
      title: 'OpenAI Proxy',
      items: [
        {to: '/console', label: 'Console', position: 'left'},
        {to: '/opencode-setup', label: 'OpenCode', position: 'left'},
        {href: 'https://github.com/GQAdonis/openai-proxy', label: 'GitHub', position: 'right'},
      ],
    },
  },
};
