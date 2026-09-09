//! Physically-plausible 3D page curl for wgpu/WebGPU.
//!
//! This module owns the complete interaction -> geometry -> GPU-uniform path.
//! The important design choices are:
//! - `glam::Vec2` for CPU-side gesture/fold math.
//! - all distances and dot products are evaluated in metric page space, not raw UV space;
//! - the grabbed material point is solved against a finite-radius cylinder, including the
//!   partial-bend case (small drags do not instantly become a 180-degree fold);
//! - curvature is allowed to relax away from the grabbed row to avoid a perfectly rigid tube;
//! - surface normals are reconstructed from finite differences of the actual deformed surface;
//! - fragment shading uses a rough paper model rather than a glossy Blinn-Phong highlight;
//! - destination-page shadowing combines a tight contact shadow with a soft directional cast shadow;
//! - an optional depth-tested pipeline fixes self-overlap ordering when the page folds over itself.
//!
//! Recommended interactive path:
//!
//! ```text
//! pointer down -> Curl3dGesture::begin(...)
//! pointer move -> Curl3dGesture::update(...)
//!               -> Curl3dPipeline::update_gesture(...)
//! render       -> Curl3dPipeline::render_with_depth(...)
//! ```

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoder, CompareFunction,
    DepthBiasState, DepthStencilState, Device, Extent3d, FragmentState, IndexFormat,
    MultisampleState, PipelineLayoutDescriptor, PrimitiveState, PrimitiveTopology, Queue,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StencilState, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureSampleType, TextureUsages, TextureView,
    TextureViewDescriptor, TextureViewDimension, VertexAttribute, VertexBufferLayout, VertexFormat,
    VertexState, VertexStepMode,
};

const PI: f32 = std::f32::consts::PI;
const GEOMETRY_EPSILON: f32 = 1.0e-5;

/// Dense enough that curved text edges no longer visibly facet on a normal reader viewport,
/// while still being cheap for WebGPU. Depth testing matters more than pushing this much higher.
const DEFAULT_MESH_COLS: u32 = 96;
const DEFAULT_MESH_ROWS: u32 = 128;

/// The depth-tested curl pipeline expects this exact format.
pub const CURL_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Curl3dVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
}

impl Curl3dVertex {
    const ATTRIBUTES: [VertexAttribute; 3] = [
        VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: VertexFormat::Float32x3,
        },
        VertexAttribute {
            offset: std::mem::size_of::<[f32; 3]>() as u64,
            shader_location: 1,
            format: VertexFormat::Float32x2,
        },
        VertexAttribute {
            offset: (std::mem::size_of::<[f32; 3]>() + std::mem::size_of::<[f32; 2]>()) as u64,
            shader_location: 2,
            format: VertexFormat::Float32x3,
        },
    ];

    #[inline]
    pub fn desc() -> VertexBufferLayout<'static> {
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Which leaf is being turned.
///
/// Mesh convention:
/// - `Next`: free edge x=1, attached/spine edge x=0.
/// - `Previous`: free edge x=0, attached/spine edge x=1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurlDirection {
    #[default]
    Next,
    Previous,
}

impl CurlDirection {
    #[inline]
    pub fn as_uniform(self) -> f32 {
        match self {
            Self::Next => 0.0,
            Self::Previous => 1.0,
        }
    }

    /// Unit vector from spine toward the free edge in page metric space.
    #[inline]
    pub fn outward_normal(self) -> Vec2 {
        match self {
            Self::Next => Vec2::X,
            Self::Previous => Vec2::NEG_X,
        }
    }

    #[inline]
    pub fn free_edge_x(self) -> f32 {
        match self {
            Self::Next => 1.0,
            Self::Previous => 0.0,
        }
    }

    #[inline]
    pub fn spine_distance_uv(self, uv: Vec2) -> f32 {
        match self {
            Self::Next => uv.x,
            Self::Previous => 1.0 - uv.x,
        }
    }
}

/// How pointer-down chooses the controlled material point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurlGrabMode {
    /// Exact touched material point. This is the intended mode for "grab from anywhere".
    #[default]
    TouchPoint,

    /// Project pointer-down to the free vertical edge while preserving Y.
    OuterEdge,
}

/// Runtime tuning. Keep geometry tuning here rather than introducing shader magic numbers.
#[derive(Clone, Copy, Debug)]
pub struct Curl3dConfig {
    /// Page height / page width. Width is one metric unit.
    pub aspect_ratio: f32,

    /// Preferred curl radius, in page-width units.
    pub base_radius: f32,
    pub min_radius: f32,
    pub max_radius: f32,

    /// Minimum component of the fold normal that must point toward the free edge.
    pub min_outward_normal: f32,

    /// Width of the softly pinned spine strip, in UV units.
    pub spine_pin_width: f32,

    /// 0 = orthographic, 1 = configured perspective.
    pub perspective_strength: f32,

    /// Destination-page contact/cast shadow strength.
    pub shadow_intensity: f32,

    /// Controls the width of the soft cast shadow lobe.
    pub shadow_softness: f32,

    /// Front ink visible through the back of the paper.
    pub paper_translucency: f32,

    /// 0..1 roughness used by the paper BRDF. High is intentionally matte.
    pub paper_roughness: f32,

    /// Overall micro-specular energy. Paper should stay low.
    pub specular_strength: f32,

    /// Relaxes curvature away from the grabbed row. 0 is a perfect cylinder.
    /// A small positive value gives a cylinder/cone-like developable approximation.
    pub cone_strength: f32,

    /// Subtle procedural paper grain. Keep tiny to avoid visible shader noise.
    pub fiber_strength: f32,

    /// Direction FROM the surface TOWARD the light. xyz is normalized in shader; w is intensity.
    pub light_dir: [f32; 4],

    /// Drag distance that corresponds to progress=1.
    pub progress_distance: f32,

    /// Ignore sub-pixel/tiny pointer movements before activating geometry.
    pub drag_dead_zone: f32,
}

impl Default for Curl3dConfig {
    fn default() -> Self {
        Self {
            aspect_ratio: 1.414,
            base_radius: 0.125,
            min_radius: 0.018,
            max_radius: 0.24,
            min_outward_normal: 0.07,
            spine_pin_width: 0.050,
            perspective_strength: 0.62,
            shadow_intensity: 0.72,
            shadow_softness: 0.82,
            paper_translucency: 0.13,
            paper_roughness: 0.84,
            specular_strength: 0.055,
            cone_strength: 0.24,
            fiber_strength: 0.006,
            light_dir: [-0.34, -0.28, 0.90, 1.0],
            progress_distance: 0.92,
            drag_dead_zone: 0.0035,
        }
    }
}

impl Curl3dConfig {
    #[inline]
    pub fn page_size(self) -> Vec2 {
        Vec2::new(1.0, self.aspect_ratio.max(0.1))
    }

    #[inline]
    fn sanitized(self) -> Self {
        let mut out = self;
        out.aspect_ratio = out.aspect_ratio.max(0.1);
        out.min_radius = out.min_radius.max(1.0e-4);
        out.max_radius = out.max_radius.max(out.min_radius);
        out.base_radius = out.base_radius.clamp(out.min_radius, out.max_radius);
        out.min_outward_normal = out.min_outward_normal.clamp(0.0, 0.95);
        out.spine_pin_width = out.spine_pin_width.clamp(0.0, 0.35);
        out.perspective_strength = out.perspective_strength.clamp(0.0, 1.0);
        out.shadow_intensity = out.shadow_intensity.max(0.0);
        out.shadow_softness = out.shadow_softness.clamp(0.15, 2.0);
        out.paper_translucency = out.paper_translucency.clamp(0.0, 1.0);
        out.paper_roughness = out.paper_roughness.clamp(0.10, 1.0);
        out.specular_strength = out.specular_strength.clamp(0.0, 0.35);
        out.cone_strength = out.cone_strength.clamp(0.0, 0.65);
        out.fiber_strength = out.fiber_strength.clamp(0.0, 0.025);
        out.progress_distance = out.progress_distance.max(0.05);
        out.drag_dead_zone = out.drag_dead_zone.clamp(0.0, 0.05);

        let light = Vec2::new(out.light_dir[0], out.light_dir[1]);
        if !light.is_finite() || !out.light_dir[2].is_finite() || out.light_dir[2] <= 0.02 {
            out.light_dir = [-0.34, -0.28, 0.90, out.light_dir[3].max(0.0)];
        }
        out.light_dir[3] = out.light_dir[3].max(0.0);
        out
    }
}

/// The bend state of the grabbed material point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CurlBendRegime {
    #[default]
    Flat,
    /// Grab point itself is still on the cylindrical arc, angle < PI.
    Arc,
    /// Grab point has passed the 180-degree arc and is on the flat returned sheet.
    Returned,
}

/// Complete CPU-side solution for one gesture sample.
#[derive(Clone, Copy, Debug)]
pub struct Curl3dHandle {
    pub active: bool,
    pub direction: CurlDirection,
    pub page_size: Vec2,

    pub grab_uv: Vec2,
    pub pointer_uv: Vec2,
    pub visual_drag_uv: Vec2,

    pub grab_metric: Vec2,
    pub visual_drag_metric: Vec2,

    /// Zero-angle tangent line origin, in page metric space.
    pub fold_mid: Vec2,
    pub fold_normal: Vec2,
    pub axis_dir: Vec2,

    pub radius: f32,
    pub progress: f32,
    pub spine_pin_width: f32,

    /// Angle of the controlled point while it lies on the arc; PI once returned.
    pub grab_bend_angle: f32,
    pub bend_regime: CurlBendRegime,
}

impl Curl3dHandle {
    pub fn inactive(direction: CurlDirection, config: Curl3dConfig) -> Self {
        let config = config.sanitized();
        let page_size = config.page_size();
        let outward = direction.outward_normal();
        let axis_dir = perpendicular_ccw(outward);
        let grab_uv = Vec2::new(direction.free_edge_x(), 0.5);
        let grab_metric = uv_to_metric(grab_uv, page_size);

        Self {
            active: false,
            direction,
            page_size,
            grab_uv,
            pointer_uv: grab_uv,
            visual_drag_uv: grab_uv,
            grab_metric,
            visual_drag_metric: grab_metric,
            fold_mid: grab_metric,
            fold_normal: outward,
            axis_dir,
            radius: config.base_radius,
            progress: 0.0,
            spine_pin_width: config.spine_pin_width,
            grab_bend_angle: 0.0,
            bend_regime: CurlBendRegime::Flat,
        }
    }

    /// Solve a finite-radius page curl from a pointer gesture.
    ///
    /// The previous implementation forced the grabbed point immediately onto the returned side of a
    /// 180-degree cylinder. That mathematically required radius <= drag/PI and made tiny drags form
    /// an unnaturally tight crease. This solver handles the partial-arc case exactly:
    /// ```text
    /// drag / r = theta - sin(theta),  theta in [0, PI]
    /// ```
    /// Once drag exceeds PI*r, it switches continuously to the flat returned-sheet continuation.
    pub fn solve(
        direction: CurlDirection,
        grab_mode: CurlGrabMode,
        drag_start_uv: Vec2,
        drag_current_uv: Vec2,
        config: Curl3dConfig,
    ) -> Self {
        let config = config.sanitized();
        let page_size = config.page_size();

        let start_uv = clamp_page_uv(drag_start_uv);
        let pointer_uv = clamp_pointer_uv(drag_current_uv);

        let grab_uv = match grab_mode {
            CurlGrabMode::TouchPoint => {
                keep_grab_off_spine(start_uv, direction, config.spine_pin_width)
            }
            CurlGrabMode::OuterEdge => Vec2::new(direction.free_edge_x(), start_uv.y),
        };

        let grab_metric = uv_to_metric(grab_uv, page_size);
        let raw_pointer_metric = uv_to_metric(pointer_uv, page_size);
        let raw_pull = grab_metric - raw_pointer_metric;
        let pull_len = raw_pull.length();

        if !pull_len.is_finite() || pull_len <= config.drag_dead_zone.max(GEOMETRY_EPSILON) {
            return Self::inactive(direction, config);
        }

        let outward = direction.outward_normal();
        let raw_normal = raw_pull / pull_len;
        let fold_normal = stabilize_fold_normal(raw_normal, outward, config.min_outward_normal);
        let axis_dir = perpendicular_ccw(fold_normal);

        // A pure cylinder preserves coordinate along its axis. Only gestures that would point the
        // cylinder into the attached side are constrained; normal arbitrary diagonal drags follow
        // the actual pointer direction.
        let visual_drag_metric = grab_metric - fold_normal * pull_len;
        let visual_drag_uv = metric_to_uv(visual_drag_metric, page_size);

        // Keep radius stable through the start of the gesture. Small drags now use a partial arc,
        // so there is no reason to collapse the radius to pull_len / PI.
        let progress = (pull_len / config.progress_distance).clamp(0.0, 1.0);
        let radius_growth = smootherstep(0.0, 0.45, progress);
        let radius = (config.base_radius * (0.94 + 0.06 * radius_growth))
            .clamp(config.min_radius, config.max_radius);

        let (grab_distance_from_fold, grab_bend_angle, bend_regime) =
            solve_grab_bend(pull_len, radius);

        let fold_mid = grab_metric - fold_normal * grab_distance_from_fold;

        Self {
            active: true,
            direction,
            page_size,
            grab_uv,
            pointer_uv,
            visual_drag_uv,
            grab_metric,
            visual_drag_metric,
            fold_mid,
            fold_normal,
            axis_dir,
            radius,
            progress,
            spine_pin_width: config.spine_pin_width,
            grab_bend_angle,
            bend_regime,
        }
    }

    /// CPU reference for the GPU XY deformation. Useful for debug handles and tests.
    pub fn deform_uv(self, uv: Vec2, config: Curl3dConfig) -> Vec2 {
        if !self.active {
            return uv;
        }

        let config = config.sanitized();
        let p_metric = uv_to_metric(uv, self.page_size);
        let rel = p_metric - self.fold_mid;
        let d = rel.dot(self.fold_normal);
        if d <= 0.0 {
            return uv;
        }

        let a = rel.dot(self.axis_dir);
        let grab_a = (self.grab_metric - self.fold_mid).dot(self.axis_dir);
        let local_radius = local_radius_cpu(
            self.radius,
            a,
            grab_a,
            self.page_size.y,
            self.progress,
            config.cone_strength,
        );

        let half_turn = PI * local_radius;
        let curled_metric = if d < half_turn {
            let theta = d / local_radius;
            self.fold_mid + self.axis_dir * a + self.fold_normal * (local_radius * theta.sin())
        } else {
            let extra = d - half_turn;
            self.fold_mid + self.axis_dir * a - self.fold_normal * extra
        };

        let pin = smootherstep(
            0.0,
            self.spine_pin_width.max(GEOMETRY_EPSILON),
            self.direction.spine_distance_uv(uv),
        );

        metric_to_uv(p_metric.lerp(curled_metric, pin), self.page_size)
    }

    #[inline]
    pub fn uniforms(self, config: Curl3dConfig) -> Curl3dUniforms {
        Curl3dUniforms::from_handle(self, config)
    }
}

/// Minimal persistent pointer state.
#[derive(Clone, Copy, Debug)]
pub struct Curl3dGesture {
    pub active: bool,
    pub direction: CurlDirection,
    pub grab_mode: CurlGrabMode,
    pub drag_start_uv: Vec2,
    pub drag_current_uv: Vec2,
}

impl Default for Curl3dGesture {
    fn default() -> Self {
        Self {
            active: false,
            direction: CurlDirection::Next,
            grab_mode: CurlGrabMode::TouchPoint,
            drag_start_uv: Vec2::new(1.0, 0.5),
            drag_current_uv: Vec2::new(1.0, 0.5),
        }
    }
}

impl Curl3dGesture {
    #[inline]
    pub fn begin(&mut self, direction: CurlDirection, grab_mode: CurlGrabMode, pointer_uv: Vec2) {
        let pointer_uv = clamp_page_uv(pointer_uv);
        self.active = true;
        self.direction = direction;
        self.grab_mode = grab_mode;
        self.drag_start_uv = pointer_uv;
        self.drag_current_uv = pointer_uv;
    }

    #[inline]
    pub fn update(&mut self, pointer_uv: Vec2) {
        if self.active {
            self.drag_current_uv = clamp_pointer_uv(pointer_uv);
        }
    }

    #[inline]
    pub fn cancel(&mut self) {
        self.active = false;
    }

    #[inline]
    pub fn handle(self, config: Curl3dConfig) -> Curl3dHandle {
        if !self.active {
            Curl3dHandle::inactive(self.direction, config)
        } else {
            Curl3dHandle::solve(
                self.direction,
                self.grab_mode,
                self.drag_start_uv,
                self.drag_current_uv,
                config,
            )
        }
    }

    #[inline]
    pub fn uniforms(self, config: Curl3dConfig) -> Curl3dUniforms {
        self.handle(config).uniforms(config)
    }

    /// Fallback/migration helper for automatic progress-driven turns.
    /// Interactive gestures should use begin/update and settle an effective pointer position.
    pub fn from_progress(
        direction: CurlDirection,
        progress: f32,
        y: f32,
        config: Curl3dConfig,
    ) -> Self {
        let config = config.sanitized();
        let progress = progress.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        let start = Vec2::new(direction.free_edge_x(), y);

        let toward_spine = -direction.outward_normal();
        // A very small arc avoids a robotic perfectly-horizontal auto-turn without overriding the
        // physical solver. The amplitude is metric, then converted back to page UV.
        let vertical_arc = (progress * PI).sin() * 0.026;
        let delta_metric =
            toward_spine * (progress * config.progress_distance) + Vec2::Y * vertical_arc;
        let page_size = config.page_size();
        let current = metric_to_uv(uv_to_metric(start, page_size) + delta_metric, page_size);

        Self {
            active: progress > 0.0001,
            direction,
            grab_mode: CurlGrabMode::OuterEdge,
            drag_start_uv: start,
            drag_current_uv: current,
        }
    }
}

/// Explicit POD GPU block. `glam` values intentionally stay outside this struct.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Curl3dUniforms {
    // 0..32
    pub page_size: [f32; 2],
    pub fold_mid: [f32; 2],
    pub fold_normal: [f32; 2],
    pub axis_dir: [f32; 2],

    // 32..48
    pub grab_metric: [f32; 2],
    pub visual_drag_metric: [f32; 2],

    // 48..64
    pub radius: f32,
    pub progress: f32,
    pub active: f32,
    pub direction: f32,

    // 64..80
    pub spine_pin_width: f32,
    pub perspective_strength: f32,
    pub shadow_intensity: f32,
    pub paper_translucency: f32,

    // 80..96
    pub paper_roughness: f32,
    pub specular_strength: f32,
    pub cone_strength: f32,
    pub shadow_softness: f32,

    // 96..112
    pub light_dir: [f32; 4],

    // 112..128
    // x = fiber_strength, yzw reserved.
    pub paper_params: [f32; 4],
}

impl Default for Curl3dUniforms {
    fn default() -> Self {
        let config = Curl3dConfig::default();
        Curl3dHandle::inactive(CurlDirection::Next, config).uniforms(config)
    }
}

impl Curl3dUniforms {
    pub fn from_handle(handle: Curl3dHandle, config: Curl3dConfig) -> Self {
        let config = config.sanitized();
        Self {
            page_size: handle.page_size.to_array(),
            fold_mid: handle.fold_mid.to_array(),
            fold_normal: handle.fold_normal.to_array(),
            axis_dir: handle.axis_dir.to_array(),
            grab_metric: handle.grab_metric.to_array(),
            visual_drag_metric: handle.visual_drag_metric.to_array(),
            radius: handle.radius,
            progress: handle.progress,
            active: if handle.active { 1.0 } else { 0.0 },
            direction: handle.direction.as_uniform(),
            spine_pin_width: handle.spine_pin_width,
            perspective_strength: config.perspective_strength,
            shadow_intensity: config.shadow_intensity,
            paper_translucency: config.paper_translucency,
            paper_roughness: config.paper_roughness,
            specular_strength: config.specular_strength,
            cone_strength: config.cone_strength,
            shadow_softness: config.shadow_softness,
            light_dir: config.light_dir,
            paper_params: [config.fiber_strength, 0.0, 0.0, 0.0],
        }
    }
}

/// Owned depth attachment for correct self-occlusion of the folded mesh.
pub struct Curl3dDepthTarget {
    pub texture: Texture,
    pub view: TextureView,
    pub width: u32,
    pub height: u32,
}

impl Curl3dDepthTarget {
    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        create_depth_target(device, width, height)
    }

    /// Recreate only when the render target size changes. Returns true when recreated.
    pub fn ensure_size(&mut self, device: &Device, width: u32, height: u32) -> bool {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return false;
        }
        *self = create_depth_target(device, width, height);
        true
    }
}

fn create_depth_target(device: &Device, width: u32, height: u32) -> Curl3dDepthTarget {
    let width = width.max(1);
    let height = height.max(1);
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("curl-3d-depth-texture"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: CURL_DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    Curl3dDepthTarget {
        texture,
        view,
        width,
        height,
    }
}

#[inline]
fn clamp_page_uv(p: Vec2) -> Vec2 {
    Vec2::new(p.x.clamp(0.0, 1.0), p.y.clamp(0.0, 1.0))
}

#[inline]
fn clamp_pointer_uv(p: Vec2) -> Vec2 {
    // Pointer capture remains continuous outside the page. This is only a numerical safety bound.
    Vec2::new(p.x.clamp(-0.55, 1.55), p.y.clamp(-0.55, 1.55))
}

#[inline]
fn uv_to_metric(uv: Vec2, page_size: Vec2) -> Vec2 {
    uv * page_size
}

#[inline]
fn metric_to_uv(metric: Vec2, page_size: Vec2) -> Vec2 {
    metric / page_size
}

#[inline]
fn perpendicular_ccw(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

#[inline]
fn keep_grab_off_spine(uv: Vec2, direction: CurlDirection, spine_pin_width: f32) -> Vec2 {
    let margin = (spine_pin_width * 1.10).clamp(0.0, 0.30);
    match direction {
        CurlDirection::Next => Vec2::new(uv.x.max(margin), uv.y),
        CurlDirection::Previous => Vec2::new(uv.x.min(1.0 - margin), uv.y),
    }
}

/// Preserve arbitrary diagonal drag direction unless it would turn the attached side into the free
/// side. This is a stability constraint, not a forced center/edge curl.
#[inline]
fn stabilize_fold_normal(raw: Vec2, outward: Vec2, min_outward_dot: f32) -> Vec2 {
    let raw = raw.normalize_or_zero();
    if raw == Vec2::ZERO {
        return outward;
    }

    let min_dot = min_outward_dot.clamp(0.0, 0.95);
    let outward_dot = raw.dot(outward);
    if outward_dot >= min_dot {
        return raw;
    }

    let tangent = raw - outward * outward_dot;
    let tangent_dir = tangent.normalize_or_zero();
    if tangent_dir == Vec2::ZERO {
        return outward;
    }

    let tangent_weight = (1.0 - min_dot * min_dot).sqrt();
    (outward * min_dot + tangent_dir * tangent_weight).normalize_or_zero()
}

/// Solve the grabbed point against a finite-radius cylinder.
///
/// For a point on the arc:
///     pull = r * (theta - sin(theta))
/// which is monotonic over [0, PI]. A bounded binary solve is cheap, deterministic and much more
/// stable near theta=0 than raw Newton iteration because f'(theta) approaches zero there.
fn solve_grab_bend(pull_len: f32, radius: f32) -> (f32, f32, CurlBendRegime) {
    let radius = radius.max(GEOMETRY_EPSILON);
    let pull_len = pull_len.max(0.0);
    let half_turn_pull = PI * radius;

    if pull_len <= GEOMETRY_EPSILON {
        return (0.0, 0.0, CurlBendRegime::Flat);
    }

    if pull_len <= half_turn_pull {
        let target = pull_len / radius;
        let theta = solve_theta_minus_sin(target);
        return (radius * theta, theta, CurlBendRegime::Arc);
    }

    // Once the point passes the 180-degree arc, original distance from fold is
    // d = PI*r + extra, while mapped position lies at -extra. Therefore
    // pull = PI*r + 2*extra and d = (pull + PI*r)/2.
    let d = (pull_len + half_turn_pull) * 0.5;
    (d, PI, CurlBendRegime::Returned)
}

fn solve_theta_minus_sin(target: f32) -> f32 {
    let target = target.clamp(0.0, PI);
    if target <= 1.0e-8 {
        return 0.0;
    }

    let mut lo = 0.0_f32;
    let mut hi = PI;
    // 18 iterations is overkill for f32 but still negligible: one solve per pointer frame.
    for _ in 0..18 {
        let mid = (lo + hi) * 0.5;
        let value = mid - mid.sin();
        if value < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

#[inline]
fn local_radius_cpu(
    base_radius: f32,
    axis_coord: f32,
    grab_axis_coord: f32,
    page_height: f32,
    progress: f32,
    cone_strength: f32,
) -> f32 {
    let span = (page_height * 0.58).max(0.20);
    let axis_distance = ((axis_coord - grab_axis_coord).abs() / span).clamp(0.0, 1.0);
    let profile = smootherstep(0.0, 1.0, axis_distance);
    let gate = smootherstep(0.025, 0.28, progress);
    base_radius * (1.0 + cone_strength * profile * gate)
}

#[inline]
fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() <= f32::EPSILON {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

const CURL_3D_SHADER: &str = r#"
struct Uniforms {
    page_size: vec2<f32>,
    fold_mid: vec2<f32>,
    fold_normal: vec2<f32>,
    axis_dir: vec2<f32>,

    grab_metric: vec2<f32>,
    visual_drag_metric: vec2<f32>,

    radius: f32,
    progress: f32,
    is_active: f32,
    direction: f32,

    spine_pin_width: f32,
    perspective_strength: f32,
    shadow_intensity: f32,
    paper_translucency: f32,

    paper_roughness: f32,
    specular_strength: f32,
    cone_strength: f32,
    shadow_softness: f32,

    light_dir: vec4<f32>,
    paper_params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var current_texture: texture_2d<f32>;
@group(0) @binding(2) var dest_texture: texture_2d<f32>;
@group(0) @binding(3) var page_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) world_normal: vec3<f32>,
    @location(3) curl_factor: f32,
    @location(4) bend_angle: f32,
    @location(5) height01: f32,
};

struct BackgroundOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct PositionResult {
    metric: vec2<f32>,
    z: f32,
    curl_factor: f32,
    bend_angle: f32,
    height01: f32,
};

const PI: f32 = 3.141592653589793;

fn saturate(x: f32) -> f32 {
    return clamp(x, 0.0, 1.0);
}

fn smootherstep01(x: f32) -> f32 {
    let t = saturate(x);
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn smootherstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let denom = max(abs(edge1 - edge0), 0.000001);
    return smootherstep01((x - edge0) / denom);
}

fn safe_normalize3(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let len2 = dot(v, v);
    if (len2 <= 0.00000001) {
        return fallback;
    }
    return v * inverseSqrt(len2);
}

fn page_metric(uv: vec2<f32>) -> vec2<f32> {
    return uv * uniforms.page_size;
}

fn spine_pin(uv: vec2<f32>) -> f32 {
    let spine_distance = select(uv.x, 1.0 - uv.x, uniforms.direction > 0.5);
    let width = max(uniforms.spine_pin_width, 0.00001);
    return smootherstep(0.0, width, spine_distance);
}

fn local_radius(metric: vec2<f32>) -> f32 {
    let rel = metric - uniforms.fold_mid;
    let a = dot(rel, uniforms.axis_dir);
    let grab_a = dot(uniforms.grab_metric - uniforms.fold_mid, uniforms.axis_dir);
    let span = max(uniforms.page_size.y * 0.58, 0.20);
    let axis_distance = saturate(abs(a - grab_a) / span);
    let profile = smootherstep01(axis_distance);
    let gate = smootherstep(0.025, 0.28, uniforms.progress);
    return max(uniforms.radius * (1.0 + uniforms.cone_strength * profile * gate), 0.0001);
}

fn deform_position(uv: vec2<f32>) -> PositionResult {
    let original_metric = page_metric(uv);

    var out: PositionResult;
    out.metric = original_metric;
    out.z = 0.0;
    out.curl_factor = 0.0;
    out.bend_angle = 0.0;
    out.height01 = 0.0;

    if (uniforms.is_active <= 0.5) {
        return out;
    }

    let rel = original_metric - uniforms.fold_mid;
    let d = dot(rel, uniforms.fold_normal);
    if (d <= 0.0) {
        return out;
    }

    let a = dot(rel, uniforms.axis_dir);
    let r = local_radius(original_metric);
    let half_turn = PI * r;

    var curled_metric = original_metric;
    var z = 0.0;
    var bend_angle = 0.0;

    if (d < half_turn) {
        let theta = d / r;
        let sin_theta = sin(theta);
        let cos_theta = cos(theta);

        curled_metric =
            uniforms.fold_mid
            + uniforms.axis_dir * a
            + uniforms.fold_normal * (r * sin_theta);
        z = r * (1.0 - cos_theta);
        bend_angle = theta;
    } else {
        let extra = d - half_turn;
        curled_metric =
            uniforms.fold_mid
            + uniforms.axis_dir * a
            - uniforms.fold_normal * extra;
        z = 2.0 * r;
        bend_angle = PI;
    }

    // Quintic pinning avoids a visible derivative kink at the attached edge.
    let pin = spine_pin(uv);
    out.metric = mix(original_metric, curled_metric, pin);
    out.z = z * pin;
    out.curl_factor = saturate(d / max(half_turn, 0.0001)) * pin;
    out.bend_angle = bend_angle * pin;
    out.height01 = saturate(z / max(2.0 * r, 0.0001)) * pin;
    return out;
}

/// Reconstruct the actual deformed-surface normal, including axial radius relaxation and spine pin.
/// This costs extra vertex ALU but removes the "perfect plastic tube" lighting artifact that appears
/// when an analytic cylinder normal is used for a surface that is no longer a perfect cylinder.
fn reconstructed_normal(uv: vec2<f32>) -> vec3<f32> {
    let du = 1.0 / 96.0;
    let dv = 1.0 / 128.0;

    let uv_l = vec2<f32>(max(uv.x - du, 0.0), uv.y);
    let uv_r = vec2<f32>(min(uv.x + du, 1.0), uv.y);
    let uv_u = vec2<f32>(uv.x, max(uv.y - dv, 0.0));
    let uv_d = vec2<f32>(uv.x, min(uv.y + dv, 1.0));

    let pl = deform_position(uv_l);
    let pr = deform_position(uv_r);
    let pu = deform_position(uv_u);
    let pd = deform_position(uv_d);

    let tx = vec3<f32>(pr.metric - pl.metric, pr.z - pl.z);
    let ty = vec3<f32>(pd.metric - pu.metric, pd.z - pu.z);
    return safe_normalize3(cross(tx, ty), vec3<f32>(0.0, 0.0, 1.0));
}

fn metric_to_clip(metric: vec2<f32>, z: f32) -> vec4<f32> {
    let uv = metric / uniforms.page_size;

    // Mild camera perspective; geometry does the visual work, not a fish-eye projection.
    let camera_z = 2.65;
    let perspective_full = camera_z / max(camera_z - z, 0.20);
    let perspective = mix(1.0, perspective_full, uniforms.perspective_strength);

    let x_ndc = (uv.x - 0.5) * 2.0 * perspective;
    let y_ndc = (0.5 - uv.y) * 2.0 * perspective;

    // In WebGPU depth 0 is near and 1 is far. Lifted paper therefore moves toward smaller depth.
    let depth = clamp(0.55 - z * 0.20, 0.02, 0.98);
    return vec4<f32>(x_ndc, y_ndc, depth, 1.0);
}

@vertex
fn vs_background(@builtin(vertex_index) vertex_index: u32) -> BackgroundOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );

    let clip = positions[vertex_index];
    var out: BackgroundOutput;
    out.clip_position = vec4<f32>(clip, 0.80, 1.0);
    out.uv = vec2<f32>((clip.x + 1.0) * 0.5, (1.0 - clip.y) * 0.5);
    return out;
}

@fragment
fn fs_background(in: BackgroundOutput) -> @location(0) vec4<f32> {
    var color = textureSample(dest_texture, page_sampler, in.uv);

    if (uniforms.is_active > 0.5) {
        let p = page_metric(in.uv);
        let d = dot(p - uniforms.fold_mid, uniforms.fold_normal);
        let r = local_radius(p);
        let progress_gain = smootherstep(0.015, 0.22, uniforms.progress);

        let light = safe_normalize3(uniforms.light_dir.xyz, vec3<f32>(-0.3, -0.2, 0.9));
        let light_z = max(light.z, 0.12);

        // Tight contact shadow at the fold root.
        let contact_width = max(r * (0.10 + 0.10 * uniforms.shadow_softness), 0.0025);
        let contact = exp(-pow(abs(d) / contact_width, 1.35))
            * smootherstep(-r * 0.12, r * 0.60, d);

        // Soft cast shadow displaced by the light direction and approximate cylinder height.
        let projected_shift = -dot(light.xy / light_z, uniforms.fold_normal) * (r * 1.35);
        let cast_center = r * 0.72 + projected_shift;
        let cast_width = max(r * (0.42 + 0.48 * uniforms.shadow_softness), 0.008);
        let cast_x = (d - cast_center) / cast_width;
        let cast_val = exp(-0.5 * cast_x * cast_x);

        // Only the vacated side should visibly receive this shadow. Still-covered parts are also
        // overdrawn by the current page, but this bias keeps the shadow physically localized.
        let revealed = smootherstep(-r * 0.18, r * 2.6, d);
        let shadow = saturate(
            (contact * 0.34 + cast_val * 0.40)
            * revealed
            * progress_gain
            * uniforms.shadow_intensity
        );

        color = vec4<f32>(color.rgb * (1.0 - shadow * 0.62), color.a);
    }

    return color;
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let deformed = deform_position(in.uv);

    var out: VertexOutput;
    out.uv = in.uv;
    out.clip_position = metric_to_clip(deformed.metric, deformed.z);
    out.world_pos = vec3<f32>(deformed.metric, deformed.z);
    out.world_normal = reconstructed_normal(in.uv);
    out.curl_factor = deformed.curl_factor;
    out.bend_angle = deformed.bend_angle;
    out.height01 = deformed.height01;
    return out;
}

fn pow5(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2 * x;
}

fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let nh2 = n_dot_h * n_dot_h;
    let denom = nh2 * (a2 - 1.0) + 1.0;
    return a2 / max(PI * denom * denom, 0.00001);
}

fn geometry_schlick_ggx(n_dot_x: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;
    return n_dot_x / max(n_dot_x * (1.0 - k) + k, 0.00001);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness)
         * geometry_schlick_ggx(n_dot_l, roughness);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow5(1.0 - cos_theta);
}

fn paper_lighting(
    base: vec3<f32>,
    n: vec3<f32>,
    v: vec3<f32>,
    l: vec3<f32>
) -> vec3<f32> {
    let roughness = clamp(uniforms.paper_roughness, 0.10, 1.0);
    let n_dot_l = saturate(dot(n, l));
    let n_dot_v = saturate(dot(n, v));
    let h = safe_normalize3(l + v, n);
    let n_dot_h = saturate(dot(n, h));
    let l_dot_h = saturate(dot(l, h));

    // Disney/Burley diffuse gives soft grazing behavior without a plastic-looking hard Lambert ridge.
    let fd90 = 0.5 + 2.0 * l_dot_h * l_dot_h * roughness;
    let light_scatter = 1.0 + (fd90 - 1.0) * pow5(1.0 - n_dot_l);
    let view_scatter = 1.0 + (fd90 - 1.0) * pow5(1.0 - n_dot_v);
    let burley = light_scatter * view_scatter;

    // High-roughness, very low-energy GGX specular. Real paper has a broad weak sheen, not the
    // narrow white Blinn-Phong stripe the old shader produced.
    let d = distribution_ggx(n_dot_h, roughness);
    let g = geometry_smith(n_dot_v, n_dot_l, roughness);
    let f0 = vec3<f32>(0.028);
    let f = fresnel_schlick(l_dot_h, f0);
    let spec = (d * g) * f / max(4.0 * n_dot_v * n_dot_l, 0.001);

    let energy = max(uniforms.light_dir.w, 0.0);
    let diffuse_light = 0.72 + 0.28 * n_dot_l * burley * energy;
    let specular = spec * n_dot_l * uniforms.specular_strength * energy;
    return base * diffuse_light + specular;
}

fn hash12(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

fn paper_grain(uv: vec2<f32>) -> f32 {
    let grid = vec2<f32>(210.0, 310.0);
    let q = uv * grid;
    let footprint = max(length(dpdx(q)), length(dpdy(q)));
    let aa = 1.0 - smootherstep(0.65, 2.0, footprint);
    return (hash12(floor(q)) - 0.5) * uniforms.paper_params.x * aa;
}

fn page_edge_mask(uv: vec2<f32>) -> f32 {
    let d = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let width = max(fwidth(d) * 1.45, 0.00065);
    return 1.0 - smoothstep(0.0, width, d);
}

/// Approximate ink density seen through paper. Explicit LOD avoids derivative-uniformity issues in
/// the front/back branch and the 5 taps create a tiny diffusion blur rather than sharp mirrored ink.
fn backside_ink_density(uv: vec2<f32>) -> f32 {
    let dims_u = textureDimensions(current_texture);
    let texel = vec2<f32>(1.0 / f32(dims_u.x), 1.0 / f32(dims_u.y));

    let c0 = textureSampleLevel(current_texture, page_sampler, uv, 0.0).rgb;
    let c1 = textureSampleLevel(current_texture, page_sampler, uv + vec2<f32>( texel.x * 1.6, 0.0), 0.0).rgb;
    let c2 = textureSampleLevel(current_texture, page_sampler, uv + vec2<f32>(-texel.x * 1.6, 0.0), 0.0).rgb;
    let c3 = textureSampleLevel(current_texture, page_sampler, uv + vec2<f32>(0.0,  texel.y * 1.6), 0.0).rgb;
    let c4 = textureSampleLevel(current_texture, page_sampler, uv + vec2<f32>(0.0, -texel.y * 1.6), 0.0).rgb;
    let c = c0 * 0.36 + (c1 + c2 + c3 + c4) * 0.16;
    let luminance = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return saturate(1.0 - luminance);
}

@fragment
fn fs_main(
    in: VertexOutput,
    @builtin(front_facing) front_facing: bool
) -> @location(0) vec4<f32> {
    // Derivatives are evaluated before the front/back branch to keep derivative control flow valid.
    let grain = paper_grain(in.uv);
    let edge_mask = page_edge_mask(in.uv);
    let front_sample = textureSample(current_texture, page_sampler, in.uv);

    let light_dir = safe_normalize3(uniforms.light_dir.xyz, vec3<f32>(-0.3, -0.2, 0.9));
    let camera_pos = vec3<f32>(0.5, uniforms.page_size.y * 0.5, 2.65);
    let view_dir = safe_normalize3(camera_pos - in.world_pos, vec3<f32>(0.0, 0.0, 1.0));

    if (!front_facing) {
        let n = safe_normalize3(-in.world_normal, vec3<f32>(0.0, 0.0, -1.0));
        let paper_tint = vec3<f32>(0.974, 0.966, 0.946);
        let ink = backside_ink_density(in.uv);
        let bleed = clamp(uniforms.paper_translucency, 0.0, 1.0);

        // Bleed affects ink density, not the entire front image color. This reads much more like thin
        // paper and much less like a semi-transparent mirrored screenshot.
        var base = paper_tint * (1.0 - ink * bleed * 0.30);
        base = base * (1.0 + grain * 0.75);

        var lit = paper_lighting(base, n, view_dir, light_dir);

        // Deep fold cavity and underside are naturally darker.
        let cavity = smootherstep(PI * 0.50, PI, in.bend_angle) * 0.105
                   + in.height01 * 0.025;
        lit = lit * (1.0 - cavity);

        // Weak warm transmitted light when the key light is behind the visible backside.
        let transmission = pow(saturate(dot(-n, light_dir)), 1.5)
                         * bleed * 0.055 * max(uniforms.light_dir.w, 0.0);
        lit = lit + vec3<f32>(1.00, 0.94, 0.82) * transmission;

        // A tiny dark edge implies paper thickness without drawing a literal thick slab.
        lit = lit * (1.0 - edge_mask * 0.045);
        return vec4<f32>(lit, 1.0);
    }

    let n = safe_normalize3(in.world_normal, vec3<f32>(0.0, 0.0, 1.0));
    var base = front_sample.rgb * (1.0 + grain * 0.60);
    var lit = paper_lighting(base, n, view_dir, light_dir);

    // Very subtle fold-root energy loss; the cast shadow carries most of the shape cue.
    let fold_occlusion = smootherstep(PI * 0.28, PI * 0.52, in.bend_angle) * 0.018;
    lit = lit * (1.0 - fold_occlusion);
    lit = lit * (1.0 - edge_mask * 0.032);

    return vec4<f32>(lit, front_sample.a);
}
"#;

pub struct Curl3dPipeline {
    /// Backward-compatible page pipeline without depth testing.
    pub pipeline: RenderPipeline,

    /// Preferred page pipeline. Use with `render_with_depth`.
    pub depth_pipeline: RenderPipeline,

    /// Destination page + analytic soft shadows (non-depth render pass).
    pub background_pipeline: RenderPipeline,

    /// Destination page + analytic soft shadows (depth render pass).
    pub depth_background_pipeline: RenderPipeline,

    pub bind_group_layout: BindGroupLayout,
    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub uniform_buffer: Buffer,
    pub sampler: Sampler,
    pub index_count: u32,
}

impl Curl3dPipeline {
    pub fn new(device: &Device, target_format: TextureFormat) -> Self {
        let (vertices, indices) = generate_dense_grid_mesh(DEFAULT_MESH_COLS, DEFAULT_MESH_ROWS);

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("curl-3d-vertex-buffer"),
            size: (vertices.len() * std::mem::size_of::<Curl3dVertex>()) as u64,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        vertex_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&vertices));
        vertex_buffer.unmap();

        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("curl-3d-index-buffer"),
            size: (indices.len() * std::mem::size_of::<u32>()) as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        index_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&indices));
        index_buffer.unmap();

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("curl-3d-uniform-buffer"),
            size: std::mem::size_of::<Curl3dUniforms>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Anisotropy is especially useful for text on the oblique folded sheet. It only reaches its
        // full benefit when the page texture has mip levels, so the integration plan asks the host
        // renderer to verify mip generation as a separate concern.
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("curl-3d-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("curl-3d-bind-group-layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("curl-3d-wgsl-shader"),
            source: ShaderSource::Wgsl(CURL_3D_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("curl-3d-pipeline-layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let background_pipeline = create_background_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            None,
            "curl-3d-background-pipeline",
        );

        let depth_background_pipeline = create_background_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            Some(DepthStencilState {
                format: CURL_DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Always),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            "curl-3d-depth-background-pipeline",
        );

        let pipeline = create_page_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            None,
            "curl-3d-render-pipeline",
        );

        let depth_pipeline = create_page_pipeline(
            device,
            &pipeline_layout,
            &shader,
            target_format,
            Some(DepthStencilState {
                format: CURL_DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::Less),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            "curl-3d-depth-render-pipeline",
        );

        Self {
            pipeline,
            depth_pipeline,
            background_pipeline,
            depth_background_pipeline,
            bind_group_layout,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            sampler,
            index_count: indices.len() as u32,
        }
    }

    pub fn create_bind_group(
        &self,
        device: &Device,
        current_view: &TextureView,
        dest_view: &TextureView,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("curl-3d-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(current_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(dest_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    #[inline]
    pub fn update_uniforms(&self, queue: &Queue, uniforms: &Curl3dUniforms) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniforms));
    }

    /// Convenience path that deliberately keeps all fold math inside this module.
    #[inline]
    pub fn update_gesture(
        &self,
        queue: &Queue,
        gesture: Curl3dGesture,
        config: Curl3dConfig,
    ) -> Curl3dHandle {
        let handle = gesture.handle(config);
        self.update_uniforms(queue, &handle.uniforms(config));
        handle
    }

    /// Compatibility path. Works, but self-overlapping folded geometry can be ordered incorrectly.
    /// Prefer `render_with_depth` for production.
    pub fn render(
        &self,
        encoder: &mut CommandEncoder,
        target_view: &TextureView,
        bind_group: &BindGroup,
        clear_color: Option<wgpu::Color>,
    ) {
        let load_op = clear_color.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("curl-3d-render-pass-no-depth"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_bind_group(0, bind_group, &[]);
        pass.set_pipeline(&self.background_pipeline);
        pass.draw(0..3, 0..1);

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }

    /// Preferred production path. Depth testing fixes front/back ordering where the curled sheet
    /// overlaps itself; without it, index order can create visible "gợn" and physically impossible
    /// overdraw even when the geometry itself is correct.
    pub fn render_with_depth(
        &self,
        encoder: &mut CommandEncoder,
        target_view: &TextureView,
        depth_view: &TextureView,
        bind_group: &BindGroup,
        clear_color: Option<wgpu::Color>,
    ) {
        let load_op = clear_color.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("curl-3d-render-pass-depth"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_bind_group(0, bind_group, &[]);

        // Destination + soft analytic shadow does not write depth.
        pass.set_pipeline(&self.depth_background_pipeline);
        pass.draw(0..3, 0..1);

        // The actual sheet writes/tests depth so folded layers self-occlude correctly.
        pass.set_pipeline(&self.depth_pipeline);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

fn create_background_pipeline(
    device: &Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: TextureFormat,
    depth_stencil: Option<DepthStencilState>,
    label: &'static str,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pipeline_layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_background"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_background"),
            compilation_options: Default::default(),
            targets: &[Some(ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_page_pipeline(
    device: &Device,
    pipeline_layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    target_format: TextureFormat,
    depth_stencil: Option<DepthStencilState>,
    label: &'static str,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pipeline_layout),
        vertex: VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Curl3dVertex::desc()],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(ColorTargetState {
                format: target_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn generate_dense_grid_mesh(cols: u32, rows: u32) -> (Vec<Curl3dVertex>, Vec<u32>) {
    assert!(cols > 0 && rows > 0);

    let mut vertices = Vec::with_capacity(((cols + 1) * (rows + 1)) as usize);
    let mut indices = Vec::with_capacity((cols * rows * 6) as usize);

    for r in 0..=rows {
        let v = r as f32 / rows as f32;
        for c in 0..=cols {
            let u = c as f32 / cols as f32;
            vertices.push(Curl3dVertex {
                position: [u, v, 0.0],
                uv: [u, v],
                normal: [0.0, 0.0, 1.0],
            });
        }
    }

    let stride = cols + 1;
    for r in 0..rows {
        for c in 0..cols {
            let i0 = r * stride + c;
            let i1 = i0 + 1;
            let i2 = (r + 1) * stride + c;
            let i3 = i2 + 1;

            indices.push(i0);
            indices.push(i2);
            indices.push(i1);
            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, epsilon: f32) -> bool {
        (a - b).abs() <= epsilon
    }

    fn approx_vec2(a: Vec2, b: Vec2, epsilon: f32) -> bool {
        (a - b).length() <= epsilon
    }

    #[test]
    fn partial_arc_equation_is_solved() {
        let target = 0.42;
        let theta = solve_theta_minus_sin(target);
        assert!(approx(theta - theta.sin(), target, 2.0e-5));
        assert!(theta > 0.0 && theta < PI);
    }

    #[test]
    fn tiny_but_active_drag_stays_on_partial_arc() {
        let config = Curl3dConfig::default();
        let handle = Curl3dHandle::solve(
            CurlDirection::Next,
            CurlGrabMode::TouchPoint,
            Vec2::new(0.88, 0.55),
            Vec2::new(0.84, 0.55),
            config,
        );
        assert!(handle.active);
        assert_eq!(handle.bend_regime, CurlBendRegime::Arc);
        assert!(handle.grab_bend_angle < PI);
    }

    #[test]
    fn direct_grab_maps_to_visual_drag_in_partial_arc() {
        let config = Curl3dConfig::default();
        let grab = Vec2::new(0.88, 0.62);
        let drag = Vec2::new(0.73, 0.55);
        let handle = Curl3dHandle::solve(
            CurlDirection::Next,
            CurlGrabMode::TouchPoint,
            grab,
            drag,
            config,
        );

        let mapped = handle.deform_uv(handle.grab_uv, config);
        assert!(
            approx_vec2(mapped, handle.visual_drag_uv, 1.5e-3),
            "grab={:?} mapped={:?} visual={:?} regime={:?}",
            handle.grab_uv,
            mapped,
            handle.visual_drag_uv,
            handle.bend_regime
        );
    }

    #[test]
    fn direct_grab_maps_to_visual_drag_after_half_turn() {
        let config = Curl3dConfig::default();
        let grab = Vec2::new(0.95, 0.72);
        let drag = Vec2::new(0.20, 0.44);
        let handle = Curl3dHandle::solve(
            CurlDirection::Next,
            CurlGrabMode::TouchPoint,
            grab,
            drag,
            config,
        );

        assert_eq!(handle.bend_regime, CurlBendRegime::Returned);
        let mapped = handle.deform_uv(handle.grab_uv, config);
        assert!(approx_vec2(mapped, handle.visual_drag_uv, 2.0e-3));
    }

    #[test]
    fn previous_pull_keeps_normal_toward_left_free_edge() {
        let config = Curl3dConfig::default();
        let handle = Curl3dHandle::solve(
            CurlDirection::Previous,
            CurlGrabMode::TouchPoint,
            Vec2::new(0.20, 0.50),
            Vec2::new(0.75, 0.10),
            config,
        );

        assert!(handle.fold_normal.dot(Vec2::NEG_X) >= config.min_outward_normal - 1.0e-4);
    }

    #[test]
    fn metric_space_respects_aspect_ratio() {
        let config = Curl3dConfig {
            aspect_ratio: 2.0,
            ..Default::default()
        };
        let page = config.page_size();
        assert!(approx(
            uv_to_metric(Vec2::new(0.5, 0.5), page).y,
            1.0,
            1.0e-6
        ));
        assert!(approx(
            uv_to_metric(Vec2::new(0.5, 1.0), page).y,
            2.0,
            1.0e-6
        ));
    }

    #[test]
    fn curvature_relaxes_away_from_grab_row() {
        let config = Curl3dConfig::default();
        let base = config.base_radius;
        let at_grab = local_radius_cpu(
            base,
            0.5,
            0.5,
            config.aspect_ratio,
            0.8,
            config.cone_strength,
        );
        let away = local_radius_cpu(
            base,
            1.1,
            0.5,
            config.aspect_ratio,
            0.8,
            config.cone_strength,
        );
        assert!(away > at_grab);
    }

    #[test]
    fn uniform_layout_is_explicit_128_bytes() {
        assert_eq!(std::mem::size_of::<Curl3dUniforms>(), 128);
        assert_eq!(std::mem::align_of::<Curl3dUniforms>(), 16);
    }
}
