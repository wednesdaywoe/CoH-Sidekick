// The JavaScript half of the lint gate, added 2026-09-26. The Rust half landed the day before
// and said in its own commit that it did not cover this side: 73
// `.cjs` files and 33,940 lines of converters, audit gates and emitters, checked by nothing.
//
// `js.configs.recommended` and NOTHING on top of it. Every rule here is one that catches a
// mistake — an undefined name, a variable computed and never read, an invisible character in
// source. There is no formatting rule and no style preset, deliberately: nobody has complained
// about how these files are laid out, and reformatting 33,940 working lines to satisfy a tool
// is churn charged against a diff a person has to review. The Rust side took formatting too,
// because `rustfmt` is that language's single answer and the repo already ran it by hand.
// JavaScript has no such single answer here, so this gate is correctness only.

import js from "@eslint/js";

export default [
  {
    ignores: [
      "node_modules/**",
      "target/**",
      // Data and build output, none of it hand-written.
      "exported_powers/**",
      "contract/**",
      "pipeline/**",
      "baseline/**",
      "fixtures/**",
      "crates/**",
      // `scripts/attic/` is the known-dead directory and item 19 measured why: twelve scripts,
      // of which the README documents seven, and every `.cjs` in it was `git mv`'d in without
      // repointing its relative `require`s, so nine of them cannot resolve and none of them
      // runs. Linting code that cannot execute produces findings nobody can act on — 13 of the
      // 63 on the first pass came from here. If the attic is ever revived, lint it then; the
      // gate is for code the regen actually calls.
      "scripts/attic/**",
    ],
  },
  {
    files: ["scripts/**/*.cjs"],
    languageOptions: {
      ecmaVersion: 2024,
      // `.cjs` regardless of the `"type": "module"` in package.json — the extension wins, and
      // every one of these files is `require`-based.
      sourceType: "commonjs",
      // Spelled out rather than pulled from the `globals` package, which is one more dependency
      // for a list this short. Node's own, plus the web APIs Node has had built in since 18.
      globals: {
        require: "readonly",
        module: "writable",
        exports: "writable",
        __dirname: "readonly",
        __filename: "readonly",
        process: "readonly",
        console: "readonly",
        Buffer: "readonly",
        URL: "readonly",
        URLSearchParams: "readonly",
        TextDecoder: "readonly",
        TextEncoder: "readonly",
        structuredClone: "readonly",
        AbortController: "readonly",
        fetch: "readonly",
        globalThis: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        setInterval: "readonly",
        clearInterval: "readonly",
        setImmediate: "readonly",
        queueMicrotask: "readonly",
      },
    },
    rules: {
      ...js.configs.recommended.rules,
      // `_` and `_jsonErr` are this codebase's spelling for "this binding exists because the
      // position exists, and is not read" — a discarded destructuring slot or an unwanted catch
      // parameter. Eight of the first pass's 38 unused-variable findings were that convention
      // being reported as a defect. Naming the convention here keeps the rule pointed at the
      // real cases: a name that was meant to be used and is not.
      "no-unused-vars": [
        "error",
        {
          argsIgnorePattern: "^_",
          varsIgnorePattern: "^_",
          caughtErrorsIgnorePattern: "^_",
        },
      ],
    },
  },
  {
    // `audit-grid-viewport-fit.cjs` drives a real browser through Playwright and passes
    // functions to `page.evaluate`, which runs them in the PAGE. `document` and `window` are
    // defined there and nowhere in this file's own scope, so all eleven `no-undef` findings on
    // the first pass were the rule being right about the file and wrong about the runtime.
    files: ["scripts/audit-grid-viewport-fit.cjs"],
    languageOptions: {
      globals: { document: "readonly", window: "readonly" },
    },
  },
];
