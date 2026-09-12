/**
 * Custom Node.js module loader hook that provides import.meta.env
 * for Vite-based code running outside of Vite.
 *
 * Do NOT `--import` this file: Node moved loader hooks onto `module.register`, and importing
 * a bare hooks module now fails SILENTLY — the shim never runs and the first Vite-flavoured
 * module dies on `import.meta.env.BASE_URL`. Go through the registrar:
 *
 *   npx tsx --import ./scripts/env-register.mjs scripts/export-to-godot.ts
 */

const ENV_SHIM = `
  if (!import.meta.env) {
    import.meta.env = {
      BASE_URL: '/',
      DEV: false,
      PROD: true,
      MODE: 'production',
      SSR: true,
    };
  }
`;

export async function load(
  url: string,
  context: { format?: string },
  nextLoad: Function
) {
  const result = await nextLoad(url, context);

  // For TypeScript/JavaScript source files, prepend the env shim
  if (
    result.format === 'module' &&
    typeof result.source === 'string' &&
    url.includes('/src/')
  ) {
    result.source = ENV_SHIM + result.source;
  }

  return result;
}
