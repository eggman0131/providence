//! The shared `wgpu` draw path (ADR 0020 §2; issue #8 Phase 1).
//!
//! GPU code — **not** run in the gate (no GPU there, I9); it is exercised only
//! through the headless-PNG capture used by `/verify` and by the on-screen
//! workbench. The pure geometry, camera, light, and colour it draws are tested
//! separately in their own modules. This module builds the render pipeline and
//! vertex/uniform buffers once, then records a Lambert-shaded pass into any
//! colour+depth target — shared verbatim by the windowed and headless adapters.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use providence_config::RenderParams;

use crate::camera::Camera;
use crate::light;
use crate::mesh::Mesh;

/// Depth buffer format for hidden-surface removal, shared by both targets.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The vertex buffer layout: three `vec3<f32>` attributes (position, normal,
/// colour) matching [`GpuVertex`] and the shader's `vs_main` locations.
const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];

/// The flat-shaded terrain shader: transform by the view/projection matrix,
/// then Lambert diffuse (single directional light) plus ambient fill. Colours
/// are linear RGB; an sRGB render target does the encode on write.
const SHADER: &str = r"
struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    shading: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(u.light_dir.xyz);
    let diffuse = max(dot(n, l), 0.0) * u.shading.y;
    let intensity = min(u.shading.x + diffuse, 1.0);
    return vec4<f32>(in.color * intensity, 1.0);
}
";

/// A GPU vertex, matching [`crate::mesh::Vertex`] with a `#[repr(C)]`,
/// `bytemuck`-castable layout for upload. Position, normal, colour — 9 floats.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
}

/// The per-frame uniform block mirrored by `Uniforms` in the shader. `vec4`
/// padding keeps the std140 layout the GPU expects.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    shading: [f32; 4],
}

/// The prepared terrain draw: pipeline, buffers, and the presentation state
/// needed to update the camera each frame. Built once from a [`Mesh`] and the
/// [`RenderParams`]; drawn into whatever colour/depth views the target provides.
pub struct TerrainScene {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    camera: Camera,
    background: wgpu::Color,
    ambient: f32,
    diffuse: f32,
    light_dir: [f32; 3],
}

impl TerrainScene {
    /// Build the pipeline and upload the mesh. `color_format` is the render
    /// target's texture format (the surface's for a window, `Rgba8UnormSrgb`
    /// for a capture).
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        params: &RenderParams,
        mesh: &Mesh,
    ) -> Self {
        let vertices: Vec<GpuVertex> = mesh
            .vertices
            .iter()
            .map(|v| GpuVertex {
                position: v.position,
                normal: v.normal,
                color: v.color,
            })
            .collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-bind-group-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-bind-group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline = build_pipeline(device, color_format, &bind_group_layout);

        let background = params.background.rgb;
        let light_dir = light::direction(
            params.lighting.azimuth_degrees,
            params.lighting.elevation_degrees,
        );
        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            vertex_buffer,
            vertex_count,
            camera: Camera::from_params(&params.camera),
            background: wgpu::Color {
                r: f64::from(background[0]),
                g: f64::from(background[1]),
                b: f64::from(background[2]),
                a: 1.0,
            },
            ambient: params.lighting.ambient,
            diffuse: params.lighting.diffuse,
            light_dir,
        }
    }

    /// Replace the view camera (issue #8 Phase 2). The window sets this from
    /// its [`OrbitController`](crate::camera::OrbitController) each time a drag
    /// or scroll moves the view; the next [`update`](Self::update) uploads the
    /// new view/projection matrix.
    pub fn set_camera(&mut self, camera: Camera) {
        self.camera = camera;
    }

    /// Recompute and upload the uniforms for a viewport of the given pixel
    /// size. Called on resize and before each draw so the projection tracks the
    /// surface's aspect ratio (and, in Phase 2, the live camera).
    pub fn update(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let aspect = aspect_ratio(width, height);
        let uniforms = Uniforms {
            view_proj: self.camera.view_projection(aspect),
            light_dir: [self.light_dir[0], self.light_dir[1], self.light_dir[2], 0.0],
            shading: [self.ambient, self.diffuse, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Record the terrain pass into `color_view` (cleared to the background)
    /// with hidden-surface removal against `depth_view`.
    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("terrain-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(self.background),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

/// Build the render pipeline: compile the shader, lay out the single uniform
/// bind group, and configure the flat-shaded, depth-tested triangle pass.
fn build_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("terrain-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("terrain-pipeline-layout"),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("terrain-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GpuVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRIBUTES,
            }],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Create the shared depth texture view for a target of the given size.
pub fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain-depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Aspect ratio `width / height`, guarding a zero-height viewport.
fn aspect_ratio(width: u32, height: u32) -> f32 {
    if height == 0 {
        1.0
    } else {
        width as f32 / height as f32
    }
}
