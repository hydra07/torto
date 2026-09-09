use std::sync::Arc;

use peniko::Color;
use vello::{
    AaConfig, AaSupport, RenderParams, Renderer as VelloRenderer, RendererOptions as VelloOptions,
    Scene,
};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, ColorTargetState, ColorWrites,
    CommandEncoderDescriptor, CurrentSurfaceTexture, Device, DeviceDescriptor, Extent3d,
    FragmentState, Instance, LoadOp, MultisampleState, Operations, PipelineLayoutDescriptor,
    PowerPreference, PrimitiveState, Queue, RenderPassColorAttachment, RenderPassDescriptor,
    RenderPipeline, RenderPipelineDescriptor, RequestAdapterOptions, ShaderModuleDescriptor,
    ShaderSource, StoreOp, Surface, SurfaceConfiguration, Texture, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor, VertexState,
};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use rebook_vello_backend::{Curl3dConfig, Curl3dGesture, Curl3dPipeline};

pub struct SurfaceRenderer {
    _window: Arc<Window>,
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
    curl_pipeline: Curl3dPipeline,
    clear_color: Color,
}

const BLIT_SHADER: &str = r#"
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
"#;

impl SurfaceRenderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = Instance::default();
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|e| format!("Failed to create wgpu surface: {e}"))?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("Failed to request GPU adapter: {e}"))?;

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("rebook-window-device"),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("Failed to request GPU device: {e}"))?;

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
        let curl_pipeline = Curl3dPipeline::new(&device, format);

        Ok(Self {
            _window: window,
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
            clear_color: Color::WHITE,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if self.surface_config.width == size.width && self.surface_config.height == size.height {
            return;
        }
        self.surface_config.width = size.width;
        self.surface_config.height = size.height;
        self.surface.configure(&self.device, &self.surface_config);
        (self.vello_target, self.vello_target_view) =
            create_vello_target(&self.device, size.width, size.height);
        (self.dest_target, self.dest_target_view) =
            create_vello_target(&self.device, size.width, size.height);
        self.blit_bind_group =
            create_blit_bind_group(&self.device, &self.blit_pipeline, &self.vello_target_view);
    }

    pub fn mark_image_dirty(&mut self, image: &peniko::ImageData) {
        self.vello_renderer.mark_override_image_dirty(image);
    }

    pub fn render_curl_3d(
        &mut self,
        current_scene: &Scene,
        dest_scene: &Scene,
        gesture: Curl3dGesture,
        config: Curl3dConfig,
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
            CurrentSurfaceTexture::Success(frame) => frame,
            CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.surface_config);
                frame
            }
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".to_string());
            }
        };

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("rebook-curl-3d-encoder"),
            });

        // 5. Render 3D curl pass: (1) destination page background + fold shadow, (2) 3D deformed mesh
        self.curl_pipeline.render(
            &mut encoder,
            &view,
            &bind_group,
            Some(wgpu::Color::WHITE),
        );

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }

    pub fn render_frame(&mut self, scene: &Scene) -> Result<(), String> {
        // Vello's compute renderer requires an Rgba8Unorm storage texture. A
        // presentation surface is commonly Bgra8UnormSrgb and cannot be passed
        // directly to `render_to_texture`, so render before acquiring the
        // swapchain image and blit the result in a conventional render pass.
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
            CurrentSurfaceTexture::Success(frame) => frame,
            CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.surface_config);
                frame
            }
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".to_string());
            }
        };

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("rebook-window-blit-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("rebook-window-blit-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: Operations {
                        load: LoadOp::Clear(wgpu::Color::BLACK),
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
}

fn create_vello_target(device: &Device, width: u32, height: u32) -> (Texture, wgpu::TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("rebook-window-vello-target"),
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

fn create_blit_resources(
    device: &Device,
    surface_format: TextureFormat,
    source_view: &wgpu::TextureView,
) -> (RenderPipeline, BindGroup) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("rebook-window-blit-shader"),
        source: ShaderSource::Wgsl(BLIT_SHADER.into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rebook-window-blit-bind-group-layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("rebook-window-blit-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("rebook-window-blit-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let bind_group = create_blit_bind_group_with_layout(device, &bind_group_layout, source_view);
    (pipeline, bind_group)
}

fn create_blit_bind_group(
    device: &Device,
    pipeline: &RenderPipeline,
    source_view: &wgpu::TextureView,
) -> BindGroup {
    create_blit_bind_group_with_layout(device, &pipeline.get_bind_group_layout(0), source_view)
}

fn create_blit_bind_group_with_layout(
    device: &Device,
    layout: &wgpu::BindGroupLayout,
    source_view: &wgpu::TextureView,
) -> BindGroup {
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("rebook-window-blit-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("rebook-window-blit-bind-group"),
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(source_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    })
}
