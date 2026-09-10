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

## Platform ownership

The platform owns UI, popup behavior, local library management, settings persistence, permissions, content URI/file resolution, clipboard, external links, navigation history/Back policy, bookmark and annotation persistence, and OS lifecycle translation. It persists engine DTOs such as `ReaderStyle`, `LocatorV1`, and source-backed overlay ranges without reproducing pagination, locator recovery, hit testing, or reader behavior.

WebAssembly and future JNI/C bindings are ABI adapters over this API. They may serialize DTOs at the language boundary, but must not implement a second reader state machine.
