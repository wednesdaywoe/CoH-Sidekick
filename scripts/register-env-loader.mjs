/**
 * Registers the env-loader hook so that import.meta.env is available
 * for Vite-authored code running under tsx/Node.
 *
 * Usage: tsx --import ./scripts/register-env-loader.mjs scripts/your-script.ts
 *
 * `env-register.mjs` beside this one does the same job against `env-hooks.mjs`, and is what the
 * script headers name.
 */
import { register } from 'node:module';

// import.meta.url is already a file:// URL; pass it directly as the parent URL.
register('./env-loader.ts', import.meta.url);
