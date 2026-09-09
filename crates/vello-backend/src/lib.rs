//! Reusable Vello compositor for retained reader frames.
//!
//! This crate owns backend scene compilation and caching without depending on
//! a window, DOM, WebAssembly, or a platform surface.

mod compositor;
pub mod curl_3d;
mod scene_cache;
mod vello_scene;

pub use compositor::{ReaderCompositor, StaticSpreadLayers};
pub use curl_3d::{
    CURL_DEPTH_FORMAT, Curl3dConfig, Curl3dDepthTarget, Curl3dGesture, Curl3dHandle,
    Curl3dPipeline, Curl3dUniforms, Curl3dVertex, CurlBendRegime, CurlDirection, CurlGrabMode,
};
pub use scene_cache::{DEFAULT_SCENE_CACHE_CAPACITY, SpreadSceneCache};

/// Images referenced by the current prepared frame, including a transition
/// destination when present. Surface adapters use this to synchronize backend
/// image resources without understanding reader spread structure.
pub fn frame_images(
    frame: &rebook_engine::PreparedReaderFrame,
) -> impl Iterator<Item = &peniko::ImageData> {
    let current_primary = frame.current_spread.primary.image_data();
    let current_secondary = frame
        .current_spread
        .secondary
        .iter()
        .flat_map(|page| page.image_data());
    let destination_primary = frame
        .destination_spread
        .iter()
        .flat_map(|spread| spread.primary.image_data());
    let destination_secondary = frame
        .destination_spread
        .iter()
        .flat_map(|spread| spread.secondary.iter())
        .flat_map(|page| page.image_data());

    current_primary
        .chain(current_secondary)
        .chain(destination_primary)
        .chain(destination_secondary)
}
