# CoH Sidekick

A character build planner for *City of Heroes*, built as a gift to the CoH community.

CoH Sidekick is designed to stay free and community-maintained for as long as the game endures.

The planner is built in Rust with Dioxus UI framework. The project ships a desktop app for Windows, macOS and Linux (Dioxus over a `wry` webview) and a web app (WASM). It includes four loadable datasets, one per server fork: Homecoming,
Rebirth, Thunderspy and Brainstorm.

---

## The Sidekick Suite

The Sidekick planner is the main feature of a small suite of community tools for the CoH data ecosystem:

- **CoH Sidekick** — the character build planner (this app). Rust and Dioxus, in [crates/app/](crates/app/).
- **Pigg Wrangler** — a viewer, extractor, and Python library for the game's `.pigg` archive files. Lives under [tools/pigg-wrangler/](tools/pigg-wrangler/), with a PyInstaller build configuration in [tools/pigg-wrangler-dist/](tools/pigg-wrangler-dist/).
- **Bin Crawler** — a parser for the game's binary `.bin` data files (Cryptic Parse6 / Parse7 formats), plus an HTTP API for consumers. Lives under [tools/bin-crawler/](tools/bin-crawler/). Depends on Pigg Wrangler for archive access, and is the first step of the planner's data pipeline.

Pigg Wrangler and Bin Crawler exist so the community has maintained, open tooling to keep
extracting and inspecting game data as the game evolves.

---

## Two halves

**The data pipeline** runs offline, on a machine that has *City of Heroes* installed. It reads the
game client's own binary files and produces one compressed dataset bundle per fork.

**The planner** is what ships. It reads that bundle and nothing else: no game install, no network
call, no server-side calculation.

---

## Architecture

The repository layout, the data pipeline, the dataset contract, the crates and how the app works
are written up in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## Running the app

The app is built and launched with `dx`, the Dioxus command-line tool
(`cargo install dioxus-cli`). Run these from `crates/app/`.

While developing — builds, launches, and reloads when the code changes:

```bash
dx serve --platform web       # web app, at the address dx prints
dx serve --platform desktop   # desktop app, in its own window
```

A build you can run on its own (the output paths are from the repository root):

```bash
dx build --platform linux    # desktop → target/dx/Sidekick/debug/linux/app/Sidekick
dx build --platform web      # web     → target/dx/Sidekick/debug/web/public (serve the folder)
```

Use `--platform macos` or `--platform windows` on those systems, and add `--release` for an
optimised build. Don't launch the binary `cargo build` leaves in `target/debug/`: it is not
bundled with the stylesheet and images, so it opens unstyled.

---

## Commands

Rebuild the data:

```bash
npm run regen                        # everything, all four datasets, from exported_powers/
npm run regen -- --dataset rebirth   # one dataset
npm run emit:contract                # contract only, from existing pipeline/ output
```

Lint and format:

```bash
npm run lint:rust        # clippy at -D warnings, all four crates
npm run lint:js          # eslint
npm run lint:py          # ruff
npm run fmt:rust         # rustfmt
```

Gates (each exits non-zero on failure):

```bash
npm run audit:census                 # is every effect family present in the data
npm run audit:atom-coverage          # how much of that data the test corpus reaches
npm run audit:export-integrity       # the parser's output against its own digest
npm run audit:def-fidelity           # decoded definitions against the raw bins
npm run audit:dataset-roster         # sites keyed by dataset name that a new fork would skip
npm run audit:cloud-target-arms      # no cfg(target_arch) in cloud client code
npm run audit:grid-viewport          # drives a real OS window through CDP
npm run baseline:check               # totals against baseline/totals-<fork>.json
```

Tests live in `crates/coh_math/tests/`: replay harnesses that grade `recalculate` against the
records in `fixtures/` (updated per game patch; see CLAUDE.md) — whole-build totals, per-power projections, set bonuses, procs and
movement.

CI (`.github/workflows/`). `ci.yml` holds seven jobs: `pipeline` (the regen-and-diff),
`rust` (the suite, under nextest), `desktop-build`, `web`, `beta-engine-staleness`,
`engine-rebuild-reproduces` and `mutants-diff`. `rc-bundle.yml` bundles release candidates for
Linux, macOS and Windows. `advisories.yml` watches dependencies. `playwright.yml` runs no browser
tests — there is no suite; the browser gate that does run is
`scripts/audit-grid-viewport-fit.cjs`, in `ci.yml`'s `web` job.

---

## License

CoH Sidekick is free software, licensed under the **GNU Affero General Public License v3.0 or later** (AGPL-3.0-or-later). See [LICENSE](LICENSE) for the full text.

In short:

- You are free to use, study, modify, and redistribute this software.
- Any modified version you distribute — **including versions you run as a hosted web service** — must also be released under AGPL-3.0 with full source code available to its users.
- There is no warranty. Use at your own risk.

This license was chosen deliberately to keep CoH Sidekick free and open: it prevents anyone from taking the code, closing the source, and selling it back to the community as a proprietary product.

Copyright (C) 2026 Wednesdaywoe.

## Trademarks

"CoH Sidekick" is the project's name and brand. The AGPL license covers the code; it does **not** grant permission to use the CoH Sidekick name or logo for forks or derivative works. See [TRADEMARKS.md](TRADEMARKS.md) for details.

## Game IP Notice

City of Heroes and all related assets are the property of their respective rights holders. CoH Sidekick is an unofficial, fan-made tool and is not affiliated with or endorsed by NCsoft, Paragon Studios, Homecoming, Rebirth, Thunderspy, or Brainstorm. Game data referenced by this tool is used solely to support community play.

## Contributing

Contributions are welcome. By submitting a pull request, you agree that your contribution will be licensed under AGPL-3.0-or-later on the same terms as the rest of the project. Please include a `Signed-off-by` line in your commits (Developer Certificate of Origin).

## A Word About Agentic Development

Development and continued maintenance of the planner is aided by LLM tools. This is the best way to avoid the otherwise inevitable future when the developer loses interest in the game, or is unable to devote time to maintaining the app. As it stands now, anyone could clone the project and take over development with an AI coding assistant. That is a better outcome than having another planner fall into disrepair, or be inaccessible to a significant portion of the player base who are on macOS or Linux. The reality is that the CoH playerbase includes capable programmers and developers, but in the game's multi-decade history, only one full-featured planner has ever existed. That's because making a CoH planner is really, really hard.
