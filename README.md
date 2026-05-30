# Spec6

Barebones Rust + React chat shell with streamed inference.

## Single Binary Build

Production builds embed the frontend into the Rust binary. A release build will default to `APP_ENV=production`, serve the bundled frontend itself, and still read secrets/runtime settings from the root `.env`.

Build it with Bun + Cargo:

- `bun run build:app` builds the embedded production binary at `target/release/spec6`
- `bun run package:linux-amd64` creates `dist/spec6-linux-amd64.tar.gz`

The Linux release archive contains:

- `spec6`
- `.env.example`
- `README.md`

## GitHub Releases

`.github/workflows/release.yml` publishes a new Linux x86_64 release asset on every push to `main` or `master`, versioned as `v<Cargo.toml version>.<run number>`.

## Inference

Pick one provider in `.env`:

- `INFERENCE_PROVIDER=gemini` with `GEMINI_API_KEY`
- `INFERENCE_PROVIDER=vultr` with `VULTR_INFERENCE_API_KEY`
- `INFERENCE_PROVIDER=aimlapi` with `AIMLAPI_API_KEY`

Set `INFERENCE_MODEL` to the provider-specific model id. The chat route streams responses over SSE all the way to the browser.
