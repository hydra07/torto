# Engine Runtime API

`rebook-engine::EngineRuntime` is the single product-facing reader API. Platform adapters should not call format, layout, reader or renderer crates directly.

## Opening a reader

Create one `OpenReaderRequest` containing:

- file bytes and display name;
- logical and physical viewport metrics;
- complete `ReaderStyle`;
- optional durable `LocatorV1`;
- existing highlight ranges;
- current focus ranges.

Calling `EngineRuntime::open` applies this state before the first visible frame, avoiding repeated pagination and render invalidation during startup.

## Runtime operations

Platforms can call the runtime directly for:

- navigation and animation ticks;
- normalized pointer input;
- selection start/update/end;
- style, highlight and focus updates;
- search, TOC, locator and reader snapshots;
- prepared render frames;
- resize, lifecycle and memory-pressure handling.

Coordinates passed to the engine are always logical pixels. Physical dimensions are used only by the platform render surface.

## Platform ownership

The platform owns UI, popup behavior, local library management, settings persistence, permissions, content URI/file resolution, clipboard, external links and OS lifecycle translation. It persists engine DTOs such as `ReaderStyle`, `LocatorV1` and source-backed annotation ranges without reproducing engine behavior.

WebAssembly and future JNI/C bindings are ABI adapters over this API. They may serialize DTOs at the language boundary, but must not implement a second reader state machine.
