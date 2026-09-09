use std::collections::HashSet;

use peniko::Color;
#[allow(unused_imports)]
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer as VelloRenderer, RendererOptions as VelloOptions,
    Scene,
};
use web_sys::HtmlCanvasElement;
use wgpu::{
    Backends, BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, ColorTargetState,
    ColorWrites, CommandEncoderDescriptor, Device, DeviceDescriptor, Extent3d, FragmentState,
    Instance, InstanceDescriptor, LoadOp, MultisampleState, Operations, PipelineCompilationOptions,
    PowerPreference, PrimitiveState, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, ShaderModuleDescriptor,
    ShaderSource, StoreOp, Surface, SurfaceConfiguration, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, VertexState,
};

#[allow(dead_code)]
const BLIT_SHADER: &str = r"
@group(0) @binding(0)
var source: texture_2d<f32>;

@group(0) @binding(1)
var source_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(source, source_sampler, input.uv);
}
";

const MAX_TRACKED_IMAGE_BLOBS: usize = 4_096;

pub struct GpuSurfaceRenderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    surface_config: SurfaceConfiguration,
    vello_renderer: VelloRenderer,
    vello_target: Texture,
    vello_target_view: wgpu::TextureView,
    dest_target: Texture,
    dest_target_view: wgpu::TextureView,
    blit_pipeline: RenderPipeline,
    blit_bind_group: BindGroup,
    curl_pipeline: rebook_vello_backend::Curl3dPipeline,
    curl_depth: rebook_vello_backend::Curl3dDepthTarget,
    clear_color: Color,
    uploaded_image_ids: HashSet<u64>,
}

impl GpuSurfaceRenderer {
    pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);

        let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = Backends::BROWSER_WEBGPU;
        let instance = Instance::new(instance_descriptor);

        // Do not force a discrete/high-performance adapter in browsers. Linux
        // Chrome commonly exposes only an integrated adapter, and requesting
        // high performance can otherwise turn a usable WebGPU setup into
        // `No suitable graphics adapter found`.
        let adapter = match instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::None,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
        {
            Ok(adapter) => adapter,
            Err(primary_error) => instance
                .request_adapter(&RequestAdapterOptions {
                    power_preference: PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: true,
                })
                .await
                .map_err(|fallback_error| {
                    format!(
                        "No usable WebGPU adapter. Hardware request: {primary_error}; fallback request: {fallback_error}. Enable WebGPU/Vulkan in the browser or use a WebGPU-capable browser."
                    )
                })?,
        };

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("rebook-wasm-device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("Failed to request GPU device: {e}"))?;

        // Acquire the canvas WebGPU context only after both adapter and device
        // exist. Any earlier failure leaves the canvas available to Canvas2D.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (canvas, width, height, device, queue);
            return Err("WebGPU canvas is only supported on wasm32".to_string());
        }

        #[cfg(target_arch = "wasm32")]
        {
            let surface = instance
                .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
                .map_err(|e| format!("Failed to create WebGPU surface: {e}"))?;
            let capabilities = surface.get_capabilities(&adapter);
            let format = capabilities
                .formats
                .iter()
                .copied()
                .find(TextureFormat::is_srgb)
                .unwrap_or(capabilities.formats[0]);
            let mut surface_config = surface
                .get_default_config(&adapter, width, height)
                .ok_or_else(|| "Surface is not supported by GPU adapter".to_string())?;
            surface_config.format = format;
            surface_config.usage = TextureUsages::RENDER_ATTACHMENT;
            surface_config.view_formats = vec![format];
            surface_config.desired_maximum_frame_latency = 1;
            surface.configure(&device, &surface_config);

            let vello_renderer = VelloRenderer::new(
                &device,
                VelloOptions {
                    antialiasing_support: AaSupport::area_only(),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("Failed to create Vello renderer: {e}"))?;

            let (vello_target, vello_target_view) = create_vello_target(&device, width, height);
            let (dest_target, dest_target_view) = create_vello_target(&device, width, height);
            let (blit_pipeline, blit_bind_group) =
                create_blit_resources(&device, format, &vello_target_view);
            let curl_pipeline = rebook_vello_backend::Curl3dPipeline::new(&device, format);
            let curl_depth = rebook_vello_backend::Curl3dDepthTarget::new(&device, width, height);

            Ok(Self {
                surface,
                device,
                queue,
                surface_config,
                vello_renderer,
                vello_target,
                vello_target_view,
                dest_target,
                dest_target_view,
                blit_pipeline,
                blit_bind_group,
                curl_pipeline,
                curl_depth,
                clear_color: Color::WHITE,
                uploaded_image_ids: HashSet::new(),
            })
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.surface_config.width == width && self.surface_config.height == height {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        let (vello_target, vello_target_view) = create_vello_target(&self.device, width, height);
        let (dest_target, dest_target_view) = create_vello_target(&self.device, width, height);
        self.vello_target = vello_target;
        self.vello_target_view = vello_target_view;
        self.dest_target = dest_target;
        self.dest_target_view = dest_target_view;
        self.curl_depth.ensure_size(&self.device, width, height);
        self.blit_bind_group =
            create_blit_bind_group(&self.device, &self.blit_pipeline, &self.vello_target_view);
    }

    /// Makes an image visible to Vello once per backing blob. Calling
    /// `mark_override_image_dirty` on every animation frame forces redundant
    /// atlas work and is especially expensive in a single-threaded browser.
    pub fn ensure_image_uploaded(&mut self, image: &peniko::ImageData) {
        if self.uploaded_image_ids.len() >= MAX_TRACKED_IMAGE_BLOBS
            && !self.uploaded_image_ids.contains(&image.data.id())
        {
            self.uploaded_image_ids.clear();
        }
        if self.uploaded_image_ids.insert(image.data.id()) {
            self.vello_renderer.mark_override_image_dirty(image);
        }
    }

    pub fn render_frame(&mut self, scene: &Scene) -> Result<(), String> {
        self.vello_renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &self.vello_target_view,
                &RenderParams {
                    base_color: self.clear_color,
                    width: self.surface_config.width,
                    height: self.surface_config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| format!("Vello render_to_texture failed: {e}"))?;

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.surface_config);
                frame
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".to_string());
            }
        };

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("rebook-wasm-blit-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("rebook-wasm-blit-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::WHITE),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &self.blit_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    pub fn render_curl_3d(
        &mut self,
        current_scene: &Scene,
        dest_scene: &Scene,
        gesture: rebook_vello_backend::Curl3dGesture,
        config: rebook_vello_backend::Curl3dConfig,
    ) -> Result<(), String> {
        // 1. Render deformed current page to vello_target
        self.vello_renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                current_scene,
                &self.vello_target_view,
                &RenderParams {
                    base_color: self.clear_color,
                    width: self.surface_config.width,
                    height: self.surface_config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| format!("Vello render_to_texture (current) failed: {e}"))?;

        // 2. Render revealed destination page to dest_target
        self.vello_renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                dest_scene,
                &self.dest_target_view,
                &RenderParams {
                    base_color: self.clear_color,
                    width: self.surface_config.width,
                    height: self.surface_config.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| format!("Vello render_to_texture (dest) failed: {e}"))?;

        // 3. Solve gesture and upload uniforms directly through Curl3D pipeline
        let bind_group = self.curl_pipeline.create_bind_group(
            &self.device,
            &self.vello_target_view,
            &self.dest_target_view,
        );
        self.curl_pipeline.update_gesture(&self.queue, gesture, config);

        // 4. Acquire surface swapchain frame
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.surface_config);
                frame
            }
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".to_string());
            }
        };

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("rebook-wasm-curl-3d-encoder"),
            });

        // 5. Render 3D curl pass: background destination + fold shadow, then deformed mesh with depth
        self.curl_pipeline.render_with_depth(
            &mut encoder,
            &view,
            &self.curl_depth.view,
            &bind_group,
            Some(wgpu::Color::WHITE),
        );

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}

fn create_vello_target(device: &Device, width: u32, height: u32) -> (Texture, wgpu::TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("rebook-wasm-vello-target"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::STORAGE_BINDING | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    (texture, view)
}

#[allow(dead_code)]
fn create_blit_resources(
    device: &Device,
    format: TextureFormat,
    vello_target_view: &wgpu::TextureView,
) -> (RenderPipeline, BindGroup) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("rebook-wasm-blit-shader"),
        source: ShaderSource::Wgsl(BLIT_SHADER.into()),
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("rebook-wasm-blit-pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });

    let bind_group = create_blit_bind_group(device, &pipeline, vello_target_view);
    (pipeline, bind_group)
}

fn create_blit_bind_group(
    device: &Device,
    pipeline: &RenderPipeline,
    vello_target_view: &wgpu::TextureView,
) -> BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("rebook-wasm-blit-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    device.create_bind_group(&BindGroupDescriptor {
        label: Some("rebook-wasm-blit-bind-group"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(vello_target_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    })
}
