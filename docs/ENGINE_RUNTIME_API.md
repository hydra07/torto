# Engine Runtime API

`rebook-engine::EngineRuntime` is the product-facing reader API for new platform adapters. New platform code should not call format, layout, reader, or renderer crates directly. The inherited `apps/desktop` application is a compatibility/reference consumer and may continue using lower-level APIs while it remains outside the engine migration path.

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
- optional normalized pointer input helpers; platforms may instead translate touch, mouse, wheel, keyboard, or hardware input directly into semantic reader commands;
- selection start/update/end;
- style and persistence-neutral overlay updates;
- search, TOC, locator and reader snapshots;
- prepared render frames;
- resize, lifecycle and memory-pressure handling.

Coordinates passed to the engine are always logical pixels. Physical dimensions are used only by the platform render surface.

## State ownership and scheduling

The runtime has three deliberately different ownership layers:

- `ReaderSession` owns committed reading state: the source/repository, layout and display compilers, viewport/style, TOC indexes, section/segment/page position, cache/LRU, prefetch generation, and prepare/poll/commit/cancel navigation state.
- `EngineReader` owns reader-facing transient state: platform-neutral pointer intent, interactive transition progress, selection anchor/selection, overlay ranges/revision, and the facade-level pending navigation token. These values must not be persisted as the durable reading position.
- `EngineRuntime` owns the currently opened `EngineBook`/`EngineReader`, logical and physical viewport metrics, and lifecycle suspension state. It translates platform events but does not duplicate pagination or locator recovery.

A committed position is changed only by `ReaderSession` navigation or locator operations. Prepared navigation and interactive transition state may describe a destination without changing that position; cancellation drops those transient values. Resize/style changes invalidate layout generations and preserve meaning through source locators/progression rather than page numbers.

On native targets, `ReaderSession` owns a bounded prefetch worker and generation-tagged requests/results. The worker may parse/layout/compile adjacent segments, but stale generations are discarded and the session remains the only owner that installs results. On single-threaded WASM, the same policy is advanced cooperatively through `EngineRuntime::tick`; no second reader state machine or background thread is introduced.

The runtime object and its reader are intended to be driven by one platform owner/event loop. Platforms may serialize DTOs or schedule calls externally, but must not concurrently mutate one `EngineRuntime` from multiple owners.

## Platform ownership

The platform owns UI, popup behavior, local library management, settings persistence, permissions, content URI/file resolution, clipboard, external links, navigation history/Back policy, bookmark and annotation persistence, and OS lifecycle translation. It persists engine DTOs such as `ReaderStyle`, `LocatorV1`, and source-backed overlay ranges without reproducing pagination, locator recovery, hit testing, or reader behavior.

WebAssembly and future JNI/C bindings are ABI adapters over this API. They may serialize DTOs at the language boundary, but must not implement a second reader state machine.
