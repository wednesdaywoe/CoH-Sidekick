// Preload shim: provides import.meta.env for Node.js
// tsx loads this before the main script via --import flag
(globalThis as any).__importMetaEnvShim = {
  BASE_URL: '/',
  DEV: false,
  PROD: true,
  MODE: 'production',
  SSR: true,
};
