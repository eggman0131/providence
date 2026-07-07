//! The on-screen `wgpu`/`winit` workbench renderer (ADR 0020 §2; issue #8).
//!
//! GPU code — not gated (I9). Realises [`RendererPort`] as a real window: it
//! owns the `winit` event loop (accepting `winit`'s control inversion, ADR 0020
//! §2) and draws the presented terrain as a lit 3D surface. An
//! [`OrbitController`] turns raw mouse-drag and scroll events into a live
//! orbit/pan/zoom view (issue #8 Phase 2). The camera is adapter-local view
//! state and never crosses the boundary (ADR 0020 §3), so moving the view can
//! never change a height.

use std::sync::Arc;

use providence_config::RenderParams;
use providence_ports::{RendererPort, TerrainFrame};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::camera::OrbitController;
use crate::context::{self, Gpu};
use crate::error::RendererError;
use crate::gpu::{self, TerrainScene};
#[cfg(feature = "debug-hud")]
use crate::hud::{Hud, Readout, ScreenDescriptor};
use crate::mesh::{Mesh, build_mesh};
#[cfg(feature = "debug-hud")]
use crate::pick::GridSnapshot;

/// The window title bar text for the workbench.
const WINDOW_TITLE: &str = "Providence — Terrain Workbench";

/// Trackpad/high-resolution wheels report scroll in pixels; mouse wheels report
/// it in lines. This normalises a pixel delta to roughly line units so one
/// `render.camera.zoom_speed` reads sensibly for both devices — input-device
/// plumbing, not a design tunable (the sensitivity lives in config).
const PIXEL_SCROLL_TO_LINES: f32 = 0.02;

/// A [`RendererPort`] that opens a window and draws the terrain in 3D.
///
/// `present` builds the drawable mesh from the snapshot; [`run`](WindowRenderer::run)
/// then launches the event loop and blocks until the window closes. Because the
/// terrain is static in issue #8, one presented frame is drawn every redraw.
pub struct WindowRenderer {
    params: RenderParams,
    mesh: Mesh,
    /// The presented grid, kept so the HUD can pick the reticle vertex each
    /// frame (issue #8 Phase 3). Only needed by the overlay.
    #[cfg(feature = "debug-hud")]
    grid: GridSnapshot,
}

impl WindowRenderer {
    /// A window renderer using the given presentation config.
    #[must_use]
    pub fn new(params: RenderParams) -> Self {
        Self {
            params,
            mesh: Mesh::default(),
            #[cfg(feature = "debug-hud")]
            grid: GridSnapshot::default(),
        }
    }

    /// Launch the `winit` event loop and block until the window is closed. Must
    /// be called on the main thread (a `winit` requirement).
    pub fn run(self) -> Result<(), RendererError> {
        let event_loop =
            EventLoop::new().map_err(|error| RendererError::EventLoop(error.to_string()))?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut app = WorkbenchApp {
            params: self.params,
            mesh: self.mesh,
            state: None,
            #[cfg(feature = "debug-hud")]
            grid: self.grid,
        };
        event_loop
            .run_app(&mut app)
            .map_err(|error| RendererError::EventLoop(error.to_string()))
    }
}

impl RendererPort for WindowRenderer {
    fn present(&mut self, frame: TerrainFrame<'_>) {
        self.mesh = build_mesh(
            &frame,
            self.params.mesh.vertical_scale,
            &self.params.palette,
        );
        #[cfg(feature = "debug-hud")]
        {
            self.grid = GridSnapshot::from_frame(&frame);
        }
    }
}

/// The `winit` application: the config and mesh to draw, plus the live GPU
/// state once the window exists.
struct WorkbenchApp {
    params: RenderParams,
    mesh: Mesh,
    state: Option<WindowState>,
    /// The presented grid, handed to the window state once the window exists so
    /// the HUD can pick against it (issue #8 Phase 3).
    #[cfg(feature = "debug-hud")]
    grid: GridSnapshot,
}

impl ApplicationHandler for WorkbenchApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // already initialised; ignore repeat resume
        }
        let attributes = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(
                self.params.window.width,
                self.params.window.height,
            ));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("workbench: could not create window: {error}");
                event_loop.exit();
                return;
            }
        };
        match WindowState::new(window, &self.params, &self.mesh) {
            Ok(state) => {
                #[cfg(feature = "debug-hud")]
                let mut state = state;
                #[cfg(feature = "debug-hud")]
                state.set_grid(self.grid.clone());
                self.state = Some(state);
            }
            Err(error) => {
                eprintln!("workbench: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::RedrawRequested => state.render(),
            WindowEvent::MouseInput {
                state: element_state,
                button,
                ..
            } => state.mouse_button(button, element_state),
            WindowEvent::CursorMoved { position, .. } => state.cursor_moved(position.x, position.y),
            WindowEvent::MouseWheel { delta, .. } => state.mouse_wheel(delta),
            _ => {}
        }
    }
}

/// Which drag gesture is active and where the cursor last was, so a
/// [`WindowEvent::CursorMoved`] can be turned into an orbit or pan delta.
#[derive(Default)]
struct DragState {
    /// The last cursor position seen, in physical pixels.
    last_cursor: Option<(f64, f64)>,
    /// Left button held → orbit on drag.
    orbiting: bool,
    /// Right button held → pan on drag.
    panning: bool,
}

/// Live GPU state for the open window: surface, device/queue, the prepared
/// scene, the depth buffer sized to the surface, and the interactive camera
/// controller with its in-flight drag gesture (issue #8 Phase 2).
struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scene: TerrainScene,
    depth_view: wgpu::TextureView,
    controller: OrbitController,
    drag: DragState,
    /// The read-only debug/HUD overlay, when enabled (issue #8 Phase 3).
    #[cfg(feature = "debug-hud")]
    hud: Option<Hud>,
    /// The presented grid the HUD picks the reticle vertex from.
    #[cfg(feature = "debug-hud")]
    grid: GridSnapshot,
    /// The mesh's vertical scale, so HUD picks line up with what is drawn.
    #[cfg(feature = "debug-hud")]
    vertical_scale: f32,
}

impl WindowState {
    fn new(window: Arc<Window>, params: &RenderParams, mesh: &Mesh) -> Result<Self, RendererError> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|error| RendererError::Surface(error.to_string()))?;
        let Gpu {
            device,
            queue,
            adapter,
        } = context::request_gpu(&instance, Some(&surface))?;

        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| {
                RendererError::Surface("surface is unsupported by the adapter".into())
            })?;
        // Prefer an sRGB surface so linear shader colours display correctly.
        let capabilities = surface.get_capabilities(&adapter);
        config.format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(config.format);
        surface.configure(&device, &config);

        let scene = TerrainScene::new(&device, config.format, params, mesh);
        scene.update(&queue, width, height);
        let depth_view = gpu::depth_view(&device, width, height);

        // Build the overlay against the surface format when enabled (Phase 3).
        #[cfg(feature = "debug-hud")]
        let hud = params
            .hud
            .enabled
            .then(|| Hud::new(&device, config.format, params.hud.clone()));

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            scene,
            depth_view,
            controller: OrbitController::from_params(&params.camera),
            drag: DragState::default(),
            #[cfg(feature = "debug-hud")]
            hud,
            #[cfg(feature = "debug-hud")]
            grid: GridSnapshot::default(),
            #[cfg(feature = "debug-hud")]
            vertical_scale: params.mesh.vertical_scale,
        })
    }

    /// Hand the presented grid to the window state so the HUD can pick the
    /// reticle vertex each frame (issue #8 Phase 3).
    #[cfg(feature = "debug-hud")]
    fn set_grid(&mut self, grid: GridSnapshot) {
        self.grid = grid;
    }

    /// Track a mouse button press/release: left arms orbit, right arms pan.
    fn mouse_button(&mut self, button: MouseButton, state: ElementState) {
        let pressed = state == ElementState::Pressed;
        match button {
            MouseButton::Left => self.drag.orbiting = pressed,
            MouseButton::Right => self.drag.panning = pressed,
            _ => {}
        }
    }

    /// Turn cursor motion into an orbit or pan while a button is held, then
    /// redraw. The very first move after a press seeds `last_cursor` and
    /// produces no jump.
    fn cursor_moved(&mut self, x: f64, y: f64) {
        if let Some((last_x, last_y)) = self.drag.last_cursor {
            let dx = (x - last_x) as f32;
            let dy = (y - last_y) as f32;
            if self.drag.orbiting {
                self.controller.orbit(dx, dy);
                self.window.request_redraw();
            } else if self.drag.panning {
                self.controller.pan(dx, dy);
                self.window.request_redraw();
            }
        }
        self.drag.last_cursor = Some((x, y));
    }

    /// Zoom on scroll (wheel lines or trackpad pixels), then redraw.
    fn mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let amount = match delta {
            MouseScrollDelta::LineDelta(_, lines) => lines,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * PIXEL_SCROLL_TO_LINES,
        };
        self.controller.zoom(amount);
        self.window.request_redraw();
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = gpu::depth_view(&self.device, width, height);
        self.scene.update(&self.queue, width, height);
        self.window.request_redraw();
    }

    fn render(&mut self) {
        use wgpu::CurrentSurfaceTexture;

        // Refresh the uniforms from the live controller so any orbit/pan/zoom
        // since the last frame shows (issue #8 Phase 2). Cheap: one small
        // buffer write.
        self.scene.set_camera(self.controller.camera());
        self.scene
            .update(&self.queue, self.config.width, self.config.height);

        let surface_texture = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture)
            | CurrentSurfaceTexture::Suboptimal(texture) => texture,
            // A lost/outdated surface is transient — reconfigure and skip the
            // frame; the OS will request another redraw.
            CurrentSurfaceTexture::Lost | CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            // Timeout / Occluded / Validation: skip this frame.
            _ => return,
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.scene.draw(&mut encoder, &view, &self.depth_view);

        // Draw the read-only HUD over the terrain (issue #8 Phase 3). Its
        // texture-upload command buffers must be submitted before the encoder
        // that reads them, so chain them ahead of the terrain pass.
        #[cfg(feature = "debug-hud")]
        {
            let hud_buffers = self.record_hud(&mut encoder, &view);
            self.queue.submit(
                hud_buffers
                    .into_iter()
                    .chain(std::iter::once(encoder.finish())),
            );
        }
        #[cfg(not(feature = "debug-hud"))]
        self.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }

    /// Record the HUD overlay into `encoder` and return the command buffers to
    /// submit before it. Builds the reticle/pose readout from the live camera
    /// and presented grid (issue #8 Phase 3); a no-op when the overlay is off or
    /// no frame has been presented.
    #[cfg(feature = "debug-hud")]
    fn record_hud(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(hud) = self.hud.as_mut() else {
            return Vec::new();
        };
        if self.grid.width == 0 || self.grid.height == 0 {
            return Vec::new();
        }
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let readout = Readout::new(
            &self.controller.camera(),
            aspect,
            &self.grid,
            self.vertical_scale,
        );
        let screen = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };
        hud.record(&self.device, &self.queue, encoder, view, &screen, &readout)
    }
}
