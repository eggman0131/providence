//! The on-screen `wgpu`/`winit` workbench renderer (ADR 0020 §2; issue #8).
//!
//! GPU code — not gated (I9). Realises [`RendererPort`] as a real window: it
//! owns the `winit` event loop (accepting `winit`'s control inversion, ADR 0020
//! §2) and draws the presented terrain as a lit 3D surface. Phase 1 holds a
//! **fixed** camera resolved from config; orbit/pan/zoom arrives in Phase 2.
//! The camera is adapter-local view state and never crosses the boundary
//! (ADR 0020 §3), so moving the view can never change a height.

use std::sync::Arc;

use providence_config::RenderParams;
use providence_ports::{RendererPort, TerrainFrame};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::context::{self, Gpu};
use crate::error::RendererError;
use crate::gpu::{self, TerrainScene};
use crate::mesh::{Mesh, build_mesh};

/// The window title bar text for the workbench.
const WINDOW_TITLE: &str = "Providence — Terrain Workbench";

/// A [`RendererPort`] that opens a window and draws the terrain in 3D.
///
/// `present` builds the drawable mesh from the snapshot; [`run`](WindowRenderer::run)
/// then launches the event loop and blocks until the window closes. Because the
/// terrain is static in issue #8, one presented frame is drawn every redraw.
pub struct WindowRenderer {
    params: RenderParams,
    mesh: Mesh,
}

impl WindowRenderer {
    /// A window renderer using the given presentation config.
    #[must_use]
    pub fn new(params: RenderParams) -> Self {
        Self {
            params,
            mesh: Mesh::default(),
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
    }
}

/// The `winit` application: the config and mesh to draw, plus the live GPU
/// state once the window exists.
struct WorkbenchApp {
    params: RenderParams,
    mesh: Mesh,
    state: Option<WindowState>,
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
            Ok(state) => self.state = Some(state),
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
            _ => {}
        }
    }
}

/// Live GPU state for the open window: surface, device/queue, the prepared
/// scene, and the depth buffer sized to the surface.
struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scene: TerrainScene,
    depth_view: wgpu::TextureView,
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

        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            scene,
            depth_view,
        })
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
        self.queue.submit(Some(encoder.finish()));
        surface_texture.present();
    }
}
