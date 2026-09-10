# Engine v1 API Draft

This document defines the minimum platform-facing call sequences for the standalone ebook engine. It describes the current `rebook-engine::EngineRuntime` boundary and does not make the inherited desktop application the reference architecture.

## Adapter rule

A new platform adapter owns bytes, fonts, persistence, UI, input arbitration, lifecycle translation, and the render surface. The engine owns publication opening, reading IR, layout, pagination, semantic navigation, selection, locator recovery, prepared frames, cache policy, and engine work scheduling.

New adapters should use `EngineRuntime` and should not implement a second reader state machine in the platform layer.

## Operation groups

| Group | Primary operations | Owner |
| --- | --- | --- |
| Open | `EngineRuntime::open_bytes`, `EngineRuntime::open` | Engine parses and initializes the first reader; platform supplies bytes/configuration. |
| Inspect | `is_open`, `viewport`, `toc`, `snapshot`, `locator`, `reader` DTO queries | Engine returns publication and reading state; platform presents it. |
| Configure | `resize`, `set_style`, `set_highlights`, `set_focus` | Engine invalidates the required layout/cache state; platform owns settings persistence. |
| Command | `navigate`, `go_to_toc_item`, `go_to_href`, `go_to_source`, `restore_locator`, selection commands | Engine resolves semantic commands and returns typed outcomes. |
| Work | `tick`, `animation_step` | Platform scheduler grants time; engine advances bounded cooperative work and transitions. |
| Frame | `frame` | Engine returns backend-neutral prepared frame data; backend/platform paints it. |
| Lifecycle | `lifecycle`, `memory_pressure` | Platform translates events; engine cancels transient work and returns a `PlatformDirective`. |
| Close | `close` | Engine drops reader-owned work/resources; platform drops surfaces and product state. |

## Use cases

### Open at the beginning

```text
create EngineRuntime(config, viewport)
open_bytes(bytes, file_name)
loop while work is pending:
    tick(budget)
    animation_step(timestamp)
    frame()
```

`open` installs style, locator, highlight, and focus inputs before the first platform-visible reader state. The platform may request a frame after opening; it must not parse or paginate the publication itself.

### Resume at a durable locator

```text
create EngineRuntime(config, viewport)
open(OpenReaderRequest { bytes, file_name, locator, reader_config, overlays })
read snapshot() and locator()
```

Locator restoration is source/progression based and can fail with a typed engine error. The current API does not yet expose structured recovery quality such as exact, quote, or progression fallback; that remains a planned API change under P1-020.

### Navigate by user intent

```text
navigate(direction)
while result is Pending:
    tick(budget)
    navigate(direction)
frame()
```

For direct semantic navigation, use `go_to_toc_item`, `go_to_href`, or `go_to_source`. These commands return `NavigationResult` through the runtime and preserve the distinction between a moved destination, a boundary, and an error. The platform owns history/back/peek policy by storing locators or snapshots; the engine does not store a history stack.

### Render a frame

```text
frame() -> PreparedReaderFrame
backend consumes current_spread, destination_spread, overlays, and transition data
backend requests another frame while requires_next_frame is true
```

`PreparedReaderFrame` is backend-neutral. It does not contain a window, surface, GPU device, texture handle, database entity, or product annotation.

### Select source-backed content

```text
begin_selection(x, y)
update_selection(x, y)
end_selection() -> selected text
selection() -> source ranges and rectangles
```

Coordinates are logical viewport coordinates. The engine returns source-backed ranges and geometry that can be regenerated after reflow. Selection spans logical pages and spread pages within one authored spine section; cross-section joining is platform policy.

### Lifecycle and memory pressure

```text
lifecycle(Suspended | SurfaceLost)
memory_pressure(level)
release or recreate platform render resources
lifecycle(Resumed | SurfaceRestored)
frame()
```

The engine cancels transient pointer, selection, and pending navigation work where required. The platform owns surface destruction/recreation and must not treat a stale prepared frame as valid after an invalidating event.

## Non-goals for v1

The runtime does not own library state, file pickers, permissions, databases, bookmarks, highlights as persistence entities, notes, sync, networking, OCR providers, translation providers, TTS, accessibility platform APIs, or OS-specific rendering surfaces.

## Open contract items

- Structured locator recovery quality and approximate-restore status.
- A stable deep-hit DTO containing text/link/image/reference/block context.
- Invalidation reason diagnostics rather than only generation changes.
- Native/WASM parity fixtures for snapshots, cancellation, and selection.
- Capability reporting for enabled formats and optional backend behavior.
