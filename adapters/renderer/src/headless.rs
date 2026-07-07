//! Headless render-to-PNG capture (ADR 0007, ADR 0020 §2; issue #8 Phase 1).
//!
//! GPU code — not gated (I9). Realises [`RendererPort`] without a window: it
//! renders the presented terrain to an off-screen texture, reads the pixels
//! back, and writes a PNG. This is the **agents-only visual self-check** and
//! the basis for perceptual golden-image comparison mandated by ADR 0007 — the
//! way `/verify` can *see* the workbench without a display.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use providence_config::RenderParams;
use providence_ports::{RendererPort, TerrainFrame};

use crate::camera::Camera;
use crate::context::{self, Gpu};
use crate::error::RendererError;
use crate::gpu::{self, TerrainScene};
#[cfg(feature = "debug-hud")]
use crate::hud::{Hud, Readout, ScreenDescriptor};
use crate::mesh::{Mesh, build_mesh};
#[cfg(feature = "debug-hud")]
use crate::pick::GridSnapshot;

/// The captured image is 8-bit RGBA; the render target is sRGB so linear shader
/// colours land encoded correctly for a PNG.
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Bytes per captured pixel (RGBA8).
const BYTES_PER_PIXEL: u32 = 4;

/// A [`RendererPort`] that draws the presented terrain into a PNG file instead
/// of a window (ADR 0020 §2). Construct with the presentation config, `present`
/// a frame, then [`capture`](HeadlessRenderer::capture) it to a path.
pub struct HeadlessRenderer {
    params: RenderParams,
    mesh: Option<Mesh>,
    view: Option<Camera>,
    /// The presented grid, kept so the HUD can pick the reticle vertex for the
    /// capture (issue #8 Phase 3). Only needed by the overlay.
    #[cfg(feature = "debug-hud")]
    grid: Option<GridSnapshot>,
}

impl HeadlessRenderer {
    /// A headless renderer using the given presentation config. The capture
    /// resolution comes from `render.window.{width,height}`.
    #[must_use]
    pub fn new(params: RenderParams) -> Self {
        Self {
            params,
            mesh: None,
            view: None,
            #[cfg(feature = "debug-hud")]
            grid: None,
        }
    }

    /// Override the camera pose for the capture (issue #8 Phase 2). Adapter-local
    /// view state (ADR 0020 §3): the composition root uses it to render the
    /// static workbench from a chosen orbit for the multi-angle visual
    /// self-check. Without it, the capture uses the configured initial pose.
    pub fn set_view(&mut self, camera: Camera) {
        self.view = Some(camera);
    }

    /// Render the most recently presented frame to a PNG at `path`. Errors if
    /// no frame has been presented yet, or if any GPU/read-back step fails.
    pub fn capture(&self, path: &Path) -> Result<(), RendererError> {
        let mesh = self
            .mesh
            .as_ref()
            .ok_or_else(|| RendererError::Capture("no frame presented to capture".into()))?;
        let width = self.params.window.width;
        let height = self.params.window.height;

        let instance = wgpu::Instance::default();
        let Gpu { device, queue, .. } = context::request_gpu(&instance, None)?;

        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CAPTURE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = gpu::depth_view(&device, width, height);

        let mut scene = TerrainScene::new(&device, CAPTURE_FORMAT, &self.params, mesh);
        if let Some(view) = self.view {
            scene.set_camera(view);
        }
        scene.update(&queue, width, height);

        // The read-only HUD overlay for the capture (issue #8 Phase 3): built
        // against the capture format, drawn over the terrain below.
        #[cfg(feature = "debug-hud")]
        let mut hud = self
            .params
            .hud
            .enabled
            .then(|| Hud::new(&device, CAPTURE_FORMAT, self.params.hud.clone()));

        // A texture-to-buffer copy needs each row padded to 256 bytes.
        let unpadded_bytes_per_row = width * BYTES_PER_PIXEL;
        let padded_bytes_per_row = unpadded_bytes_per_row
            .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture-readback"),
            size: u64::from(padded_bytes_per_row) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        scene.draw(&mut encoder, &color_view, &depth_view);

        // Record the HUD over the terrain, before the texture is copied back so
        // the readback (and PNG) includes it (issue #8 Phase 3). Its
        // texture-upload command buffers are submitted ahead of this encoder.
        #[cfg(feature = "debug-hud")]
        let hud_buffers = self.record_hud(hud.as_mut(), &device, &queue, &mut encoder, &color_view);

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        #[cfg(feature = "debug-hud")]
        queue.submit(
            hud_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        #[cfg(not(feature = "debug-hud"))]
        queue.submit(Some(encoder.finish()));

        let pixels = read_back(
            &device,
            &readback,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            height,
        )?;
        write_png(path, width, height, &pixels)
    }

    /// Record the HUD overlay into `encoder` and return the command buffers to
    /// submit before it. Builds the reticle/pose readout from the capture's
    /// camera (the view override, or the configured pose) and presented grid
    /// (issue #8 Phase 3); a no-op when the overlay is off or nothing was
    /// presented. Headless captures at 1.0 pixels-per-point (physical pixels).
    #[cfg(feature = "debug-hud")]
    fn record_hud(
        &self,
        hud: Option<&mut Hud>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
    ) -> Vec<wgpu::CommandBuffer> {
        let (Some(hud), Some(grid)) = (hud, self.grid.as_ref()) else {
            return Vec::new();
        };
        let camera = self
            .view
            .unwrap_or_else(|| Camera::from_params(&self.params.camera));
        let width = self.params.window.width;
        let height = self.params.window.height;
        let aspect = width as f32 / height.max(1) as f32;
        let readout = Readout::new(&camera, aspect, grid, self.params.mesh.vertical_scale);
        let screen = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: 1.0,
        };
        hud.record(device, queue, encoder, color_view, &screen, &readout)
    }
}

impl RendererPort for HeadlessRenderer {
    fn present(&mut self, frame: TerrainFrame<'_>) {
        self.mesh = Some(build_mesh(
            &frame,
            self.params.mesh.vertical_scale,
            &self.params.palette,
        ));
        #[cfg(feature = "debug-hud")]
        {
            self.grid = Some(GridSnapshot::from_frame(&frame));
        }
    }
}

/// Map the read-back buffer, wait for the GPU, and copy out the tightly-packed
/// (unpadded) RGBA rows.
fn read_back(
    device: &wgpu::Device,
    readback: &wgpu::Buffer,
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    height: u32,
) -> Result<Vec<u8>, RendererError> {
    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| RendererError::Capture(format!("device poll failed: {error}")))?;
    receiver
        .recv()
        .map_err(|_| RendererError::Capture("read-back channel closed".into()))?
        .map_err(|error| RendererError::Capture(format!("buffer map failed: {error}")))?;

    let mapped = slice.get_mapped_range();
    let unpadded = unpadded_bytes_per_row as usize;
    let padded = padded_bytes_per_row as usize;
    let mut pixels = Vec::with_capacity(unpadded * height as usize);
    for row in 0..height as usize {
        let start = row * padded;
        pixels.extend_from_slice(&mapped[start..start + unpadded]);
    }
    drop(mapped);
    readback.unmap();
    Ok(pixels)
}

/// Write tightly-packed RGBA8 `pixels` (`width * height * 4` bytes) as a PNG.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), RendererError> {
    let file = File::create(path)
        .map_err(|error| RendererError::Capture(format!("create {}: {error}", path.display())))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| RendererError::Capture(format!("png header: {error}")))?;
    writer
        .write_image_data(pixels)
        .map_err(|error| RendererError::Capture(format!("png data: {error}")))?;
    Ok(())
}
