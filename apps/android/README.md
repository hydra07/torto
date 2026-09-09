# Android host

This crate is the native, platform-neutral state boundary for the future Android app.

The Android UI layer should:

1. Resolve Storage Access Framework content URIs to bytes and a display name; format detection remains engine-owned.
2. Keep `AndroidReaderHost`/`EngineRuntime` alive independently of the current native window/surface.
3. Translate `MotionEvent` coordinates to logical pixels and `PointerEvent` values.
4. Send lifecycle and memory-pressure events to the host.
5. Own the wgpu/Vello surface adapter and release transient GPU resources when requested.

JNI, Gradle, Compose/View UI, permissions and packaging intentionally remain outside Rust core.
