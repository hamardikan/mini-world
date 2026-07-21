import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  const hostPort = env.VITE_HOST_PORT || '7878';
  const apiBase = env.VITE_API_BASE || '/v1';
  const proxyPath = apiBase.startsWith('/') ? apiBase : '/v1';

  const proxy = {
    [proxyPath]: {
      target: `http://127.0.0.1:${hostPort}`,
      changeOrigin: false,
    },
  };

  return {
    plugins: [react()],
    server: { proxy },
    preview: { proxy },
  };
});
