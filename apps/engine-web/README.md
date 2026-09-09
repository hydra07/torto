# Engine web harness

Install `wasm-pack` once, then build the Rust facade before starting Vite:

```bash
cd apps/engine-web
bun install
bun run build:wasm
bun dev -- --host 0.0.0.0
```

`build:wasm` produces the browser binding for `crates/engine-wasm`. The EPUB
harness forwards selected bytes to `WebReader::open_bytes`, renders retained
pages through WebGPU, supports engine-owned previous/next and TOC navigation,
and restores the durable locator saved for the selected book.
