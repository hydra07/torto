use rebook_layout::LayoutViewport;

/// Geometry shared by native, Android and browser shells.
/// Pagination uses logical pixels while GPU surfaces use physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportMetrics {
    pub layout: LayoutViewport,
    pub surface_width: u32,
    pub surface_height: u32,
    pub scale_factor: f32,
}

impl ViewportMetrics {
    pub fn new(
        logical_width: u32,
        logical_height: u32,
        surface_width: u32,
        surface_height: u32,
        scale_factor: f32,
    ) -> Self {
        Self {
            layout: LayoutViewport {
                width: logical_width.max(1),
                height: logical_height.max(1),
            },
            surface_width: surface_width.max(1),
            surface_height: surface_height.max(1),
            scale_factor: sanitize_scale_factor(scale_factor),
        }
    }

    pub fn from_logical_size(width: u32, height: u32, scale_factor: f32) -> Self {
        let scale_factor = sanitize_scale_factor(scale_factor);
        Self::new(
            width,
            height,
            physical_extent(width, scale_factor),
            physical_extent(height, scale_factor),
            scale_factor,
        )
    }

    pub fn logical_width(self) -> u32 {
        self.layout.width
    }

    pub fn logical_height(self) -> u32 {
        self.layout.height
    }
}

fn sanitize_scale_factor(scale_factor: f32) -> f32 {
    if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn physical_extent(logical: u32, scale_factor: f32) -> u32 {
    ((logical as f32 * scale_factor).round() as u32).max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycleEvent {
    Resumed,
    Suspended,
    SurfaceLost,
    SurfaceRestored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressure {
    Moderate,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_factor_does_not_change_layout_size() {
        let viewport = ViewportMetrics::from_logical_size(400, 800, 2.75);
        assert_eq!(
            viewport.layout,
            LayoutViewport {
                width: 400,
                height: 800
            }
        );
        assert_eq!(
            (viewport.surface_width, viewport.surface_height),
            (1100, 2200)
        );
    }

    #[test]
    fn invalid_scale_factor_is_sanitized() {
        let viewport = ViewportMetrics::from_logical_size(400, 800, f32::NAN);
        assert_eq!(viewport.scale_factor, 1.0);
        assert_eq!(
            (viewport.surface_width, viewport.surface_height),
            (400, 800)
        );
    }
}
