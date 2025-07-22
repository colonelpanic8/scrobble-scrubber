import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    fs: {
      allow: ['..']
    },
    proxy: {
      '/api/lastfm': {
        target: 'https://www.last.fm',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api\/lastfm/, ''),
        configure: (proxy, options) => {
          proxy.on('proxyReq', (proxyReq, req, res) => {
            // Add browser-like headers to make requests look authentic
            proxyReq.setHeader('User-Agent', 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36');
            proxyReq.setHeader('Accept', 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7');
            proxyReq.setHeader('Accept-Language', 'en-US,en;q=0.9');
            proxyReq.setHeader('Accept-Encoding', 'gzip, deflate, br');
            proxyReq.setHeader('DNT', '1');
            proxyReq.setHeader('Connection', 'keep-alive');
            proxyReq.setHeader('Upgrade-Insecure-Requests', '1');
          });
        }
      }
    }
  },
  optimizeDeps: {
    exclude: ['scrobble-scrubber-js']
  }
});