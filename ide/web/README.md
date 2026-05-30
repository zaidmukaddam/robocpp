# RoboC++ Studio Web

Browser-hosted IDE prototype for RoboC++.

## Development

```sh
npm install
npm run dev
```

The app starts with a checked-in WASM stub so UI development works before the
Rust WASM package is generated. The language-service bridge falls back to sample
analysis data when the generated package is missing.

The project explorer uses [`@pierre/trees`](https://trees.software/) for the
sidebar file tree (search, keyboard navigation, rename, delete, context menus).

## WASM Language Service

Install tooling once:

```sh
brew install wasm-pack
rustup target add wasm32-unknown-unknown
```

If you use Homebrew Rust (`/opt/homebrew/bin/rustc`), `wasm-pack` needs the
rustup toolchain for the `wasm32-unknown-unknown` target. The build script below
handles that automatically when `rustup` is available.

Generate the Rust language-service package with:

```sh
npm run wasm:build
```

The generated package is written to `src/wasm/iec_language_service_wasm/` and
replaces the stub module used by the development fallback.

## Verification

```sh
npm run build
npm run check:bundle
npm test
npm run test:e2e
```

## Target Bridge

See [docs/TARGET_BRIDGE.md](./docs/TARGET_BRIDGE.md) for simulator and hardware
bridge setup, Modbus mapping notes, and common failure modes.

Use `npm run dev:with-target` to start the bridge and dev server together.

## Production Build

```sh
npm run build:check
npm run preview
```

Serve the `dist/` folder from any static host. WASM and editor chunks are split
into separate assets; run `npm run check:bundle` to enforce size budgets in CI.
