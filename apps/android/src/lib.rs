//! Android application boundary.
//!
//! This crate intentionally contains no JNI or Activity implementation yet.
//! It is the stable native host that a thin Kotlin/JNI layer will own across
//! Android surface recreation.

use std::sync::Arc;

use rebook_engine::{
    AppLifecycleEvent, EngineConfig, EngineRuntime, MemoryPressure, OpenBookSummary,
    PlatformDirective, PointerEvent, PointerGestureResult, ViewportMetrics,
};

pub struct AndroidReaderHost {
    engine: EngineRuntime,
}

impl AndroidReaderHost {
    pub fn new(config: EngineConfig, viewport: ViewportMetrics) -> Self {
        Self {
            engine: EngineRuntime::new(config, viewport),
        }
    }

    /// Android's Storage Access Framework should resolve a content URI and
    /// pass its bytes plus display name here. Core code never assumes a path.
    pub fn open_book(
        &mut self,
        bytes: impl Into<Arc<[u8]>>,
        display_name: &str,
    ) -> Result<OpenBookSummary, rebook_engine::EngineError> {
        self.engine.open_bytes(bytes, display_name)
    }

    pub fn resize(&mut self, viewport: ViewportMetrics) -> Result<(), rebook_engine::EngineError> {
        self.engine.resize(viewport)
    }

    pub fn pointer(
        &mut self,
        event: PointerEvent,
    ) -> Result<PointerGestureResult, rebook_engine::EngineError> {
        self.engine.pointer(event)
    }

    pub fn lifecycle(&mut self, event: AppLifecycleEvent, timestamp_ms: f64) -> PlatformDirective {
        self.engine.lifecycle(event, timestamp_ms)
    }

    pub fn memory_pressure(&mut self, level: MemoryPressure) -> PlatformDirective {
        self.engine.memory_pressure(level)
    }

    pub fn engine(&self) -> &EngineRuntime {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut EngineRuntime {
        &mut self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_survives_surface_recreation_without_owning_a_surface() {
        let viewport = ViewportMetrics::from_logical_size(412, 915, 2.625);
        let mut host = AndroidReaderHost::new(EngineConfig::default(), viewport);
        assert_eq!(
            host.lifecycle(AppLifecycleEvent::SurfaceLost, 1.0),
            PlatformDirective::ReleaseTransientRenderResources
        );
        assert_eq!(
            host.lifecycle(AppLifecycleEvent::SurfaceRestored, 2.0),
            PlatformDirective::RequestFrame
        );
        assert_eq!(host.engine().viewport(), viewport);
    }
}
