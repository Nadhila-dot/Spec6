# Sentinel

Barebones Rust + React chat shell with streamed inference.

## Inference

Pick one provider in `.env`:

- `INFERENCE_PROVIDER=gemini` with `GEMINI_API_KEY`
- `INFERENCE_PROVIDER=vultr` with `VULTR_INFERENCE_API_KEY`

Set `INFERENCE_MODEL` to the provider-specific model id. The chat route streams responses over SSE all the way to the browser.
