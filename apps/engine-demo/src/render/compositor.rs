use std::sync::Arc;

use kurbo::Affine;
use peniko::Color;
use rebook_reader::ReaderSpread;
use vello::Scene;

use super::scene::{OverlaySet, SpreadSceneKey, StaticSpreadLayers};
use super::vello::VelloScene;

const TEXT_SELECTION_COLOR: Color = Color::from_rgba8(68, 137, 103, 72);
const ANNOTATION_MARK_COLOR: Color = Color::from_rgba8(96, 165, 250, 72);

pub struct ReaderCompositor;

impl ReaderCompositor {
    pub fn build_static_layers(spread: &ReaderSpread, key: SpreadSceneKey) -> StaticSpreadLayers {
        let mut underlay = Scene::new();
        let mut content = Scene::new();
        let mut images = Vec::new();

        // Underlay: background + images
        {
            images.extend(spread.primary.image_data().cloned());
            let mut underlay_bridge = VelloScene::new(&mut underlay);
            spread.primary.paint_background(&mut underlay_bridge);
            spread
                .primary
                .paint_images_at(&mut underlay_bridge, spread.primary_offset_x);
            if let Some(secondary) = &spread.secondary {
                images.extend(secondary.image_data().cloned());
                secondary.paint_images_at(&mut underlay_bridge, spread.secondary_offset_x);
            }
        }

        // Content: non-image content (text, rules, etc.)
        {
            let mut content_bridge = VelloScene::new(&mut content);
            spread
                .primary
                .paint_non_image_content_at(&mut content_bridge, spread.primary_offset_x);
            if let Some(secondary) = &spread.secondary {
                secondary
                    .paint_non_image_content_at(&mut content_bridge, spread.secondary_offset_x);
            }
        }

        StaticSpreadLayers {
            underlay: Arc::new(underlay),
            content: Arc::new(content),
            images: images.into(),
            key,
        }
    }

    pub fn compose_spread_scene(
        layers: &StaticSpreadLayers,
        spread: &ReaderSpread,
        overlays: &OverlaySet,
        transform: Option<Affine>,
    ) -> Scene {
        let mut scene = Scene::new();

        // 1. Static Underlay
        scene.append(&layers.underlay, transform);

        // 2. Dynamic Overlays
        if !overlays.highlights.is_empty()
            || !overlays.selection.is_empty()
            || !overlays.focus.is_empty()
        {
            let mut overlay_scene = Scene::new();
            {
                let mut bridge = VelloScene::new(&mut overlay_scene);
                Self::paint_overlays(
                    &spread.primary,
                    &mut bridge,
                    overlays,
                    spread.primary_offset_x,
                );
                if let Some(secondary) = &spread.secondary {
                    Self::paint_overlays(
                        secondary,
                        &mut bridge,
                        overlays,
                        spread.secondary_offset_x,
                    );
                }
            }
            scene.append(&overlay_scene, transform);
        }

        // 3. Static Foreground Content
        scene.append(&layers.content, transform);

        scene
    }

    pub fn compose_curl(
        source_layers: &StaticSpreadLayers,
        dest_layers: &StaticSpreadLayers,
        source_spread: &ReaderSpread,
        dest_spread: &ReaderSpread,
        direction: rebook_reader::PageDirection,
        progress: f32,
        width: f64,
        height: f64,
    ) -> Scene {
        let source = Self::compose_spread_scene(
            source_layers,
            source_spread,
            &OverlaySet::default(),
            None,
        );
        let destination = Self::compose_spread_scene(
            dest_layers,
            dest_spread,
            &OverlaySet::default(),
            None,
        );

        let progress = f64::from(progress.clamp(0.0, 1.0));
        let sin_prog = (progress * std::f64::consts::PI).sin();
        let edge = match direction {
            rebook_reader::PageDirection::Next => width * (1.0 - progress),
            rebook_reader::PageDirection::Previous => width * progress,
        };
        let bulge = sin_prog * width.min(1000.0) * 0.065;
        let source_clip = curl_clip_path(direction, edge, bulge, width, height);

        let mut scene = Scene::new();
        // 1. Destination spread (underneath)
        scene.append(&destination, None);

        // 2. Multi-layer drop shadow cast by the curl onto the destination spread
        let shadow_extent = (36.0 + 80.0 * sin_prog).max(12.0);
        let shadow_steps = 10;
        for i in 0..shadow_steps {
            let t = f64::from(i) / f64::from(shadow_steps);
            let alpha = ((1.0 - t).powi(2) * 48.0 * sin_prog) as u8;
            let step_w = shadow_extent / f64::from(shadow_steps);
            let offset = f64::from(i) * step_w;
            let rect = match direction {
                rebook_reader::PageDirection::Next => kurbo::Rect::new(
                    (edge - offset - step_w).max(0.0),
                    0.0,
                    (edge - offset).max(0.0),
                    height,
                ),
                rebook_reader::PageDirection::Previous => kurbo::Rect::new(
                    (edge + offset).min(width),
                    0.0,
                    (edge + offset + step_w).min(width),
                    height,
                ),
            };
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                Color::from_rgba8(12, 16, 24, alpha),
                None,
                &rect,
            );
        }

        // 3. Current page clipped by the curl curve
        scene.push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            kurbo::Affine::IDENTITY,
            &source_clip,
        );
        scene.append(&source, None);
        scene.pop_layer();

        // 4. Backside of the curling page (flap) with realistic 3D paper shading
        let flap_width = (28.0 + 120.0 * sin_prog).max(4.0);
        let (flap_x0, flap_x1) = match direction {
            rebook_reader::PageDirection::Next => (edge, (edge + flap_width).min(width)),
            rebook_reader::PageDirection::Previous => ((edge - flap_width).max(0.0), edge),
        };
        scene.fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            Color::from_rgba8(246, 243, 235, 230),
            None,
            &kurbo::Rect::new(flap_x0, 0.0, flap_x1, height),
        );

        // 5. 3D Cylindrical lighting: inner shadow + specular highlight ridge
        let flap_steps = 8;
        for i in 0..flap_steps {
            let t = f64::from(i) / f64::from(flap_steps);
            let step_w = (flap_x1 - flap_x0) / f64::from(flap_steps);
            let x0 = flap_x0 + f64::from(i) * step_w;
            let x1 = x0 + step_w;
            let rect = kurbo::Rect::new(x0, 0.0, x1, height);

            if t < 0.25 {
                let highlight_alpha = ((1.0 - (t / 0.25 - 0.5).abs() * 2.0).max(0.0) * 80.0 * sin_prog) as u8;
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    Color::from_rgba8(255, 255, 255, highlight_alpha),
                    None,
                    &rect,
                );
            } else {
                let shadow_t = (t - 0.25) / 0.75;
                let shadow_alpha = (shadow_t * 55.0 * sin_prog) as u8;
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    Color::from_rgba8(20, 24, 30, shadow_alpha),
                    None,
                    &rect,
                );
            }
        }

        // 6. Subtle spine depth shadow
        let spine_shadow_w = 20.0;
        let spine_alpha = (18.0 * sin_prog) as u8;
        if spine_alpha > 0 {
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                Color::from_rgba8(0, 0, 0, spine_alpha),
                None,
                &kurbo::Rect::new(0.0, 0.0, spine_shadow_w, height),
            );
        }

        scene
    }

    fn paint_overlays(
        page: &rebook_renderer::PageDisplayList,
        scene: &mut VelloScene<'_>,
        overlays: &OverlaySet,
        offset_x: f32,
    ) {
        if !overlays.highlights.is_empty() {
            page.paint_source_ranges(scene, &overlays.highlights, ANNOTATION_MARK_COLOR, offset_x);
        }
        if !overlays.selection.is_empty() {
            page.paint_source_ranges(scene, &overlays.selection, TEXT_SELECTION_COLOR, offset_x);
        }
        if !overlays.focus.is_empty() {
            page.paint_source_ranges(scene, &overlays.focus, TEXT_SELECTION_COLOR, offset_x);
        }
    }
}

fn curl_clip_path(
    direction: rebook_reader::PageDirection,
    edge: f64,
    _bulge: f64,
    width: f64,
    height: f64,
) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    match direction {
        rebook_reader::PageDirection::Next => {
            path.move_to((0.0, 0.0));
            path.line_to((edge, 0.0));
            path.line_to((edge, height));
            path.line_to((0.0, height));
        }
        rebook_reader::PageDirection::Previous => {
            path.move_to((width, 0.0));
            path.line_to((edge, 0.0));
            path.line_to((edge, height));
            path.line_to((width, height));
        }
    }
    path.close_path();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::scene::PageSceneKey;
    use rebook_layout::PageLayout;
    use rebook_renderer::DisplayListCompiler;

    #[test]
    fn transforms_compose_without_rebuilding_static_layers() {
        let compiler = DisplayListCompiler;
        let page_layout = PageLayout {
            viewport: rebook_layout::LayoutViewport {
                width: 800,
                height: 1000,
            },
            background: rebook_publication::Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            leading_gap: 0.0,
            items: Vec::new(),
        };
        let page_display_list = Arc::new(compiler.compile(&page_layout));
        let spread = ReaderSpread {
            primary: Arc::clone(&page_display_list),
            secondary: None,
            primary_offset_x: 0.0,
            secondary_offset_x: 0.0,
        };

        let key = SpreadSceneKey {
            primary: PageSceneKey {
                position: rebook_reader::ReaderPosition {
                    section_index: 0,
                    segment_index: 0,
                    page_index: 0,
                },
                layout_generation: 0,
            },
            secondary: None,
            width: 800,
            height: 1000,
        };
        let layers = ReaderCompositor::build_static_layers(&spread, key);

        let t1 = Some(Affine::translate((10.0, 0.0)));
        let t2 = Some(Affine::translate((20.0, 0.0)));

        let _scene1 =
            ReaderCompositor::compose_spread_scene(&layers, &spread, &OverlaySet::default(), t1);
        let _scene2 =
            ReaderCompositor::compose_spread_scene(&layers, &spread, &OverlaySet::default(), t2);

        assert_eq!(Arc::strong_count(&layers.underlay), 1);
        assert_eq!(Arc::strong_count(&layers.content), 1);
    }
}
