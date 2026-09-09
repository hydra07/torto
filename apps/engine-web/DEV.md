# Web engine development loop

The browser harness is the cross-platform engine test surface. Keep the loop
measurable and do not use the Vite development bundle for size benchmarks.

```bash
bun run build:wasm       # release WASM, not a debug artifact
bun run analyze:wasm     # report size and apply wasm-opt when installed
bun run build            # production JS/CSS bundle
bun run preview -- --host 0.0.0.0
```

Install Binaryen once for the final size pass (`wasm-opt -Oz`). Use Chrome or
Chromium with HTTPS/WebGPU enabled. The target is EPUB-first; format parsers
and native filesystem access must not enter the web hot path.
