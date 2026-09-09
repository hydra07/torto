use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect};
use peniko::{BlendMode, Color, Fill, ImageData};
use rebook_engine::ReaderSpread;
use rebook_engine::frame::{OverlaySet, SpreadFrameKey};
use vello::Scene;

use crate::scene_cache::SpreadSceneCache;
use crate::vello_scene::VelloScene;

const TEXT_SELECTION_COLOR: Color = Color::from_rgba8(68, 137, 103, 72);
const ANNOTATION_MARK_COLOR: Color = Color::from_rgba8(96, 165, 250, 72);

pub struct StaticSpreadLayers {
    pub underlay: Arc<Scene>,
    pub content: Arc<Scene>,
    pub images: Arc<[ImageData]>,
    pub key: SpreadFrameKey,
}

pub struct ReaderCompositor;

impl ReaderCompositor {
    pub fn build_static_layers(spread: &ReaderSpread, key: SpreadFrameKey) -> StaticSpreadLayers {
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

    pub fn compose_current_scene(
        cache: &mut SpreadSceneCache,
        frame: &rebook_engine::PreparedReaderFrame,
    ) -> Scene {
        let layers = cache.get_or_build(&frame.key, &frame.current_spread);
        Self::compose_spread_scene(&layers, &frame.current_spread, &frame.overlays, None)
    }

    pub fn compose_destination_scene(
        cache: &mut SpreadSceneCache,
        frame: &rebook_engine::PreparedReaderFrame,
    ) -> Option<Scene> {
        let destination_spread = frame.destination_spread.as_ref()?;
        let dest_key = frame
            .destination_key
            .clone()
            .unwrap_or_else(|| frame.key.clone());
        let dest_layers = cache.get_or_build(&dest_key, destination_spread);
        Some(Self::compose_spread_scene(
            &dest_layers,
            destination_spread,
            &OverlaySet::default(),
            None,
        ))
    }

    pub fn compose_frame(
        cache: &mut SpreadSceneCache,
        frame: &rebook_engine::PreparedReaderFrame,
    ) -> Scene {
        match frame.transition {
            rebook_engine::FrameTransition::None => {
                let layers = cache.get_or_build(&frame.key, &frame.current_spread);
                Self::compose_spread_scene(&layers, &frame.current_spread, &frame.overlays, None)
            }
            rebook_engine::FrameTransition::Slide {
                primary_offset_x,
                destination_offset_x,
                ..
            } => {
                let mut scene = Scene::new();
                let primary_layers = cache.get_or_build(&frame.key, &frame.current_spread);
                let primary_transform = Some(Affine::translate((f64::from(primary_offset_x), 0.0)));
                let primary_scene = Self::compose_spread_scene(
                    &primary_layers,
                    &frame.current_spread,
                    &frame.overlays,
                    primary_transform,
                );
                scene.append(&primary_scene, None);

                if let Some(destination_spread) = &frame.destination_spread {
                    let dest_key = frame
                        .destination_key
                        .clone()
                        .unwrap_or_else(|| frame.key.clone());
                    let dest_layers = cache.get_or_build(&dest_key, destination_spread);
                    let dest_transform =
                        Some(Affine::translate((f64::from(destination_offset_x), 0.0)));
                    let dest_scene = Self::compose_spread_scene(
                        &dest_layers,
                        destination_spread,
                        &OverlaySet::default(),
                        dest_transform,
                    );
                    scene.append(&dest_scene, None);
                }

                scene
            }
            rebook_engine::FrameTransition::Curl {
                direction,
                progress,
                start_x_ratio: _,
                start_y_ratio: _,
                current_x_ratio: _,
                current_y_ratio: _,
            } => Self::compose_curl(cache, frame, direction, progress),
        }
    }

    fn compose_curl(
        cache: &mut SpreadSceneCache,
        frame: &rebook_engine::PreparedReaderFrame,
        direction: rebook_engine::PageDirection,
        progress: f32,
    ) -> Scene {
        let source_layers = cache.get_or_build(&frame.key, &frame.current_spread);
        let source = Self::compose_spread_scene(
            &source_layers,
            &frame.current_spread,
            &frame.overlays,
            None,
        );
        let Some(destination_spread) = &frame.destination_spread else {
            return source;
        };
        let destination_key = frame
            .destination_key
            .clone()
            .unwrap_or_else(|| frame.key.clone());
        let destination_layers = cache.get_or_build(&destination_key, destination_spread);
        let destination = Self::compose_spread_scene(
            &destination_layers,
            destination_spread,
            &OverlaySet::default(),
            None,
        );

        let width = f64::from(frame.viewport.width);
        let height = f64::from(frame.viewport.height);
        let progress = f64::from(progress.clamp(0.0, 1.0));
        let sin_prog = (progress * std::f64::consts::PI).sin();
        let edge = match direction {
            rebook_engine::PageDirection::Next => width * (1.0 - progress),
            rebook_engine::PageDirection::Previous => width * progress,
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
                rebook_engine::PageDirection::Next => Rect::new(
                    (edge - offset - step_w).max(0.0),
                    0.0,
                    (edge - offset).max(0.0),
                    height,
                ),
                rebook_engine::PageDirection::Previous => Rect::new(
                    (edge + offset).min(width),
                    0.0,
                    (edge + offset + step_w).min(width),
                    height,
                ),
            };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(12, 16, 24, alpha),
                None,
                &rect,
            );
        }

        // 3. Current page clipped by the curl curve
        scene.push_layer(
            Fill::NonZero,
            BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &source_clip,
        );
        scene.append(&source, None);
        scene.pop_layer();

        // 4. Backside of the curling page (flap) with realistic 3D paper shading
        let flap_width = (28.0 + 120.0 * sin_prog).max(4.0);
        let (flap_x0, flap_x1) = match direction {
            rebook_engine::PageDirection::Next => (edge, (edge + flap_width).min(width)),
            rebook_engine::PageDirection::Previous => ((edge - flap_width).max(0.0), edge),
        };
        // Paper base tone of curling flap
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(246, 243, 235, 230),
            None,
            &Rect::new(flap_x0, 0.0, flap_x1, height),
        );

        // 5. 3D Cylindrical lighting: inner shadow + specular highlight ridge
        let flap_steps = 8;
        for i in 0..flap_steps {
            let t = f64::from(i) / f64::from(flap_steps);
            let step_w = (flap_x1 - flap_x0) / f64::from(flap_steps);
            let x0 = flap_x0 + f64::from(i) * step_w;
            let x1 = x0 + step_w;
            let rect = Rect::new(x0, 0.0, x1, height);

            // Shading curve across the curl cylinder
            if t < 0.25 {
                // Highlight near the peak of the curl cylinder
                let highlight_alpha =
                    ((1.0 - (t / 0.25 - 0.5).abs() * 2.0).max(0.0) * 80.0 * sin_prog) as u8;
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgba8(255, 255, 255, highlight_alpha),
                    None,
                    &rect,
                );
            } else {
                // Soft shading into the underside fold
                let shadow_t = (t - 0.25) / 0.75;
                let shadow_alpha = (shadow_t * 55.0 * sin_prog) as u8;
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
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
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(0, 0, 0, spine_alpha),
                None,
                &Rect::new(0.0, 0.0, spine_shadow_w, height),
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
    direction: rebook_engine::PageDirection,
    edge: f64,
    _bulge: f64,
    width: f64,
    height: f64,
) -> BezPath {
    let mut path = BezPath::new();
    match direction {
        rebook_engine::PageDirection::Next => {
            path.move_to((0.0, 0.0));
            path.line_to((edge, 0.0));
            path.line_to((edge, height));
            path.line_to((0.0, height));
        }
        rebook_engine::PageDirection::Previous => {
            path.move_to((width, 0.0));
            path.line_to((edge, 0.0));
            path.line_to((edge, height));
            path.line_to((width, height));
        }
    }
    path.close_path();
    path
}
