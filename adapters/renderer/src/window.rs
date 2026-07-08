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
use std::time::Instant;

use providence_config::{InputParams, MaterialParams, PointerButton, RenderParams};
use providence_ports::{RendererPort, SimDriver, TerrainFrame};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::anim::{MeshTween, ripple_delays};
use crate::camera::OrbitController;
use crate::context::{self, Gpu};
use crate::error::RendererError;
use crate::gpu::{self, TerrainScene};
#[cfg(feature = "debug-hud")]
use crate::hud::{Hud, Readout, ScreenDescriptor};
use crate::input::{is_shaping_click, shape_action};
use crate::mesh::{Mesh, build_mesh, vertex_position};
use crate::pick::{GridSnapshot, cursor_ndc, pick_vertex, screen_ray};
use crate::water::WaterPlane;

/// The window title bar text for the workbench.
const WINDOW_TITLE: &str = "Providence — Terrain Workbench";

/// Trackpad/high-resolution wheels report scroll in pixels; mouse wheels report
/// it in lines. This normalises a pixel delta to roughly line units so one
/// `render.camera.zoom_speed` reads sensibly for both devices — input-device
/// plumbing, not a design tunable (the sensitivity lives in config).
const PIXEL_SCROLL_TO_LINES: f32 = 0.02;

/// Map a `winit` mouse button to the config-level [`PointerButton`] the shaping
/// bindings speak, or `None` for a button the bindings can't name (back/forward/
/// extra). Converting at the window edge keeps the input mapping ([`crate::input`])
/// `winit`-free and unit-tested in the gate (ADR 0022; I9).
fn pointer_button(button: MouseButton) -> Option<PointerButton> {
    match button {
        MouseButton::Left => Some(PointerButton::Left),
        MouseButton::Right => Some(PointerButton::Right),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

/// A [`RendererPort`] that opens a window and draws the terrain in 3D.
///
/// `present` seeds the initial drawable mesh from the snapshot;
/// [`run`](WindowRenderer::run) then launches the event loop, holding a
/// `&mut dyn SimDriver` so a shaping click submits a discrete `TerrainCommand`
/// and the changed land is pulled back and redrawn (ADR 0022 §4). Dragging still
/// orbits/pans/zooms the camera exactly as in issue #8.
pub struct WindowRenderer {
    params: RenderParams,
    mesh: Mesh,
    /// The presented grid, kept so a click can pick the vertex under the cursor
    /// (issue #9 Phase 2) and the HUD can pick the reticle vertex (issue #8
    /// Phase 3). Refreshed from the driver after every shaping command.
    grid: GridSnapshot,
    /// The living water surface (ADR 0023, Phase 2), built from the presented
    /// frame's waterline. `None` until the first `present`; constant thereafter
    /// (the datum and grid extent do not change under shaping).
    water: Option<WaterPlane>,
    /// The waterline datum from the presented frame, kept so a shaping rebuild
    /// can reconstruct a snapshot that carries it (the value itself is unread by
    /// terrain meshing; the water plane is fixed).
    waterline: i32,
}

impl WindowRenderer {
    /// A window renderer using the given presentation config.
    #[must_use]
    pub fn new(params: RenderParams) -> Self {
        Self {
            params,
            mesh: Mesh::default(),
            grid: GridSnapshot::default(),
            water: None,
            waterline: 0,
        }
    }

    /// Launch the `winit` event loop and block until the window is closed,
    /// driving the interactive shaping seam (ADR 0022 §4). `driver` is the
    /// application session the renderer submits commands to and pulls fresh
    /// snapshots from; `input` binds the shaping controls (`input.shape.*`). The
    /// renderer holds only the [`SimDriver`] trait — never the core (I2/I4).
    /// Must be called on the main thread (a `winit` requirement).
    pub fn run(self, driver: &mut dyn SimDriver, input: InputParams) -> Result<(), RendererError> {
        let event_loop =
            EventLoop::new().map_err(|error| RendererError::EventLoop(error.to_string()))?;
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut app = WorkbenchApp {
            params: self.params,
            mesh: self.mesh,
            grid: self.grid,
            water: self.water,
            waterline: self.waterline,
            input,
            driver,
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
            &self.params.material,
        );
        // The grid is now kept unconditionally: a shaping click picks against it
        // (issue #9 Phase 2), not only the debug HUD (issue #8 Phase 3).
        self.grid = GridSnapshot::from_frame(&frame);
        // Float the living water surface at the frame's waterline (ADR 0023,
        // Phase 2). The plane is constant for the session (the datum and grid
        // extent do not change under shaping), so it is built once here.
        self.waterline = frame.waterline();
        self.water = Some(WaterPlane::new(
            frame.width(),
            frame.height(),
            frame.waterline(),
            self.params.mesh.vertical_scale,
            self.params.water.surface_lift,
        ));
    }
}

/// The `winit` application: the config, initial mesh, and input bindings to
/// draw and drive with, the `SimDriver` the interactive seam submits to
/// (ADR 0022 §4), plus the live GPU state once the window exists.
struct WorkbenchApp<'driver> {
    params: RenderParams,
    mesh: Mesh,
    /// The presented grid, handed to the window state once the window exists so
    /// a click (and the HUD) can pick against it.
    grid: GridSnapshot,
    /// The living water surface (ADR 0023, Phase 2), handed to the window state
    /// so the scene floats it over the terrain. Seeded by the initial `present`.
    water: Option<WaterPlane>,
    /// The waterline datum (from the presented frame), so a shaping rebuild can
    /// reconstruct a snapshot carrying it.
    waterline: i32,
    /// The shaping controls (`input.shape.*`) — which button raises/lowers and
    /// the click-vs-drag threshold.
    input: InputParams,
    /// The application session the renderer shapes through: `submit` a command,
    /// then pull `heights`/`revision`. Only the trait is held — never the core.
    driver: &'driver mut dyn SimDriver,
    state: Option<WindowState>,
}

impl ApplicationHandler for WorkbenchApp<'_> {
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
        match WindowState::new(
            window,
            &self.params,
            &self.mesh,
            self.water.as_ref(),
            self.waterline,
            &self.input,
        ) {
            Ok(mut state) => {
                // The window picks against the presented grid on a click, so it
                // is seeded unconditionally now (issue #9 Phase 2), not only for
                // the HUD reticle (issue #8 Phase 3).
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
        // Borrow the window state and the driver as disjoint fields so a click
        // can drive both (the pick lives in the state, the submit on the driver).
        let WorkbenchApp { state, driver, .. } = self;
        let Some(state) = state.as_mut() else {
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
            } => state.mouse_button(button, element_state, &mut **driver),
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

/// One in-flight held-button gesture, tracked so release can tell a shaping
/// *click* from a camera *drag* (ADR 0022, the Director's control-scheme
/// ruling): which platform button began it, and how far the cursor has
/// travelled (physical pixels) since the press.
#[derive(Clone, Copy, Debug)]
struct PressGesture {
    /// The button pressed — resolved to a shaping action on release.
    button: MouseButton,
    /// Accumulated cursor path length since the press; compared to
    /// `input.shape.click_drag_threshold_px` to classify the gesture.
    motion_px: f32,
}

/// An in-flight shaping animation (ADR 0022 §5; issue #9/#10 Phase 3): the tween
/// from the old drawn surface to the post-command one, and when it started. The
/// only wall-clock in the whole path lives here, in the adapter — the elapsed
/// time decides *how far* the visual surface has settled, never anything the
/// core computes (I3).
struct Animation {
    /// The old→new surface tween; `at(fraction)` is the frame to draw.
    tween: MeshTween,
    /// When the animation began, for the elapsed-time fraction.
    start: Instant,
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
    /// The in-flight press gesture, if a button is down (issue #9 Phase 2).
    press: Option<PressGesture>,
    /// The CPU copy of the surface currently drawn — the `from` anchor a new
    /// shaping animation eases out of (issue #9/#10 Phase 3).
    drawn: Mesh,
    /// The in-flight shaping animation, if one is settling (issue #9/#10
    /// Phase 3); `None` when the surface is at rest.
    animation: Option<Animation>,
    /// `render.animation.duration_ms` — how long a shaping change settles; 0
    /// snaps instantly.
    animation_duration_ms: f32,
    /// `render.animation.ripple_ms_per_unit` — per-unit-distance start delay that
    /// makes the cascade ripple outward from the click (issue #9/#10 Phase 4).
    animation_ripple_ms_per_unit: f32,
    /// The shaping controls (`input.shape.*`): bindings + click-vs-drag slack.
    input: InputParams,
    /// The material table, so a shaping command can rebuild the mesh from the
    /// fresh snapshot with the same terrain-type colouring (issue #9 Phase 2,
    /// ADR 0023).
    material: MaterialParams,
    /// The presented grid: a click picks the vertex under the cursor from it
    /// (issue #9 Phase 2), and the HUD picks the reticle vertex (issue #8
    /// Phase 3). Refreshed from the driver after every shaping command.
    grid: GridSnapshot,
    /// The mesh's vertical scale, so picks and rebuilt geometry line up with
    /// what is drawn.
    vertical_scale: f32,
    /// The waterline datum (from the presented frame), so a shaping rebuild
    /// reconstructs a snapshot carrying it (ADR 0023, Phase 2). The water plane
    /// itself is fixed and lives in the scene.
    waterline: i32,
    /// Whether the water shimmer is animating (`render.water.ripple_*` both
    /// positive). When it is, each frame requests another redraw so the sea keeps
    /// moving; a still sea leaves the window event-driven as before.
    water_animates: bool,
    /// The wall-clock origin the water shimmer is timed from (ADR 0023, Phase 2).
    /// The only water clock in the whole path lives here, at the edge — nothing
    /// the core computes reads it (I3), exactly like the shaping animation.
    clock_start: Instant,
    /// The read-only debug/HUD overlay, when enabled (issue #8 Phase 3).
    #[cfg(feature = "debug-hud")]
    hud: Option<Hud>,
}

impl WindowState {
    fn new(
        window: Arc<Window>,
        params: &RenderParams,
        mesh: &Mesh,
        water: Option<&WaterPlane>,
        waterline: i32,
        input: &InputParams,
    ) -> Result<Self, RendererError> {
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

        let scene = TerrainScene::new(&device, config.format, params, mesh, water);
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
            press: None,
            drawn: mesh.clone(),
            animation: None,
            animation_duration_ms: params.animation.duration_ms,
            animation_ripple_ms_per_unit: params.animation.ripple_ms_per_unit,
            input: input.clone(),
            material: params.material.clone(),
            grid: GridSnapshot::default(),
            vertical_scale: params.mesh.vertical_scale,
            waterline,
            water_animates: params.water.ripple_amplitude > 0.0 && params.water.ripple_speed > 0.0,
            clock_start: Instant::now(),
            #[cfg(feature = "debug-hud")]
            hud,
        })
    }

    /// Hand the presented grid to the window state so a click can pick the
    /// vertex under the cursor (issue #9 Phase 2) and the HUD can pick the
    /// reticle vertex (issue #8 Phase 3).
    fn set_grid(&mut self, grid: GridSnapshot) {
        self.grid = grid;
    }

    /// Track a mouse button press/release (issue #8 Phase 2 camera + issue #9
    /// Phase 2 shaping). Left arms orbit, right arms pan (unchanged); every
    /// press also begins a [`PressGesture`], and a release that stayed a click
    /// shapes the picked vertex through `driver` (ADR 0022 §3).
    fn mouse_button(
        &mut self,
        button: MouseButton,
        state: ElementState,
        driver: &mut dyn SimDriver,
    ) {
        let pressed = state == ElementState::Pressed;
        match button {
            MouseButton::Left => self.drag.orbiting = pressed,
            MouseButton::Right => self.drag.panning = pressed,
            _ => {}
        }
        if pressed {
            // Any button may be a shaping click — a rebind can put raise/lower on
            // the middle button. Motion accrues in `cursor_moved`.
            self.press = Some(PressGesture {
                button,
                motion_px: 0.0,
            });
        } else if let Some(press) = self.press.take()
            && press.button == button
        {
            self.resolve_click(press, driver);
        }
    }

    /// A held button was released: if the gesture stayed a *click* (cursor moved
    /// no more than `input.shape.click_drag_threshold_px`) and the button is
    /// bound to a shaping action, pick the vertex under the cursor and submit the
    /// command through `driver`, then pull the fresh snapshot and rebuild the
    /// drawn mesh when the heights actually moved (ADR 0022 §3, the
    /// submit → pull → redraw path). A *drag* already moved the camera live and
    /// shapes nothing.
    fn resolve_click(&mut self, press: PressGesture, driver: &mut dyn SimDriver) {
        if !is_shaping_click(press.motion_px, &self.input.shape) {
            return; // a camera drag, already applied — not a shaping click
        }
        let Some(pointer) = pointer_button(press.button) else {
            return; // a button the bindings can't name
        };
        let Some(action) = shape_action(pointer, &self.input.shape) else {
            return; // bound to neither raise nor lower
        };
        let Some((cursor_x, cursor_y)) = self.drag.last_cursor else {
            return; // no cursor position yet — nothing to pick
        };

        // Resolve the vertex under the cursor through the same ray/pick maths the
        // reticle uses (issue #8 Phase 3), generalised to the live cursor. All
        // float/ray work stays here at the edge; only an integer command crosses.
        let size = (self.config.width, self.config.height);
        let ndc = cursor_ndc((cursor_x as f32, cursor_y as f32), size);
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let ray = screen_ray(&self.controller.camera(), aspect, ndc);
        let Some(picked) = pick_vertex(&ray, &self.grid.frame(), self.vertical_scale) else {
            return; // the cursor is off the terrain
        };

        // Input reaches the sim ONLY here, as a discrete TerrainCommand (ADR
        // 0022 §3). The renderer holds the SimDriver trait, never the core.
        let before = driver.revision();
        driver.submit(action.command(picked.x, picked.y));
        if driver.revision() == before {
            return; // a no-op: ceiling, out of bounds, or an immovable refusal
        }

        // Pull the fresh snapshot and rebuild the target surface. Picking tracks
        // the new *logical* heights immediately (the grid), while the *drawn*
        // surface eases toward the target over `render.animation.*`, rippling out
        // from the shaped vertex.
        let (width, height) = (driver.width(), driver.height());
        let heights = driver.heights().to_vec();
        let types = driver.types().to_vec();
        // The waterline is the session-constant datum, so it is cached rather
        // than re-pulled (the water plane is fixed; only the terrain is rebuilt).
        let frame = TerrainFrame::new(width, height, &heights, &types, self.waterline);
        let target = build_mesh(&frame, self.vertical_scale, &self.material);
        self.grid = GridSnapshot::from_frame(&frame);
        // The shaped vertex's world (x, z) is the ripple centre (the ripple lags
        // by distance from it). Reuse the mesh's own vertex placement so the
        // centre lines up with the drawn geometry.
        let origin = vertex_position(picked.x, picked.y, 0, width, height, self.vertical_scale);
        self.start_animation(target, [origin[0], origin[2]]);
    }

    /// Begin easing the drawn surface toward `target`, rippling outward from
    /// `center_xz` (ADR 0022 §5; issue #9/#10 Phase 3-4), or snap to it when
    /// animation is disabled (`render.animation.duration_ms == 0`). Either way a
    /// redraw is requested; an in-flight animation then drives its own redraws
    /// until it settles.
    fn start_animation(&mut self, target: Mesh, center_xz: [f32; 2]) {
        if self.animation_duration_ms <= 0.0 {
            self.scene.set_mesh(&self.device, &target);
            self.drawn = target;
            self.animation = None;
        } else {
            let from = self.drawn.clone();
            let delays = ripple_delays(
                &target.vertices,
                center_xz,
                self.animation_ripple_ms_per_unit,
            );
            self.animation = Some(Animation {
                tween: MeshTween::new(from, target, delays),
                start: Instant::now(),
            });
        }
        self.window.request_redraw();
    }

    /// Advance any in-flight shaping animation to the current wall-clock time,
    /// re-uploading the eased surface (ADR 0022 §5). Returns `true` while the
    /// animation is still settling (elapsed under the ripple's total span), so
    /// the caller keeps requesting redraws; on the final frame it snaps exactly
    /// onto the target and clears the state. The wall-clock is read here and
    /// nowhere near the core (I3).
    fn advance_animation(&mut self) -> bool {
        let (mesh, done) = {
            let Some(anim) = self.animation.as_ref() else {
                return false;
            };
            let elapsed_ms = anim.start.elapsed().as_secs_f32() * 1000.0;
            let done = elapsed_ms >= anim.tween.total_ms(self.animation_duration_ms);
            let mesh = if done {
                anim.tween.target().clone()
            } else {
                anim.tween.at(elapsed_ms, self.animation_duration_ms)
            };
            (mesh, done)
        };
        self.scene.set_mesh(&self.device, &mesh);
        self.drawn = mesh;
        if done {
            self.animation = None;
        }
        !done
    }

    /// Turn cursor motion into an orbit or pan while a button is held, and accrue
    /// it into the active press so a release can tell a click from a drag, then
    /// redraw. The very first move after a press seeds `last_cursor` and produces
    /// no jump.
    fn cursor_moved(&mut self, x: f64, y: f64) {
        if let Some((last_x, last_y)) = self.drag.last_cursor {
            let dx = (x - last_x) as f32;
            let dy = (y - last_y) as f32;
            if let Some(press) = self.press.as_mut() {
                press.motion_px += (dx * dx + dy * dy).sqrt();
            }
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

        // Advance any in-flight shaping animation to now and re-upload the eased
        // surface (ADR 0022 §5; issue #9/#10 Phase 3). Keep the redraw chain
        // alive until it settles; requesting eagerly means a skipped frame below
        // (a lost surface) still gets another shot.
        if self.advance_animation() {
            self.window.request_redraw();
        }

        // Advance the water shimmer to the current wall-clock (ADR 0023, Phase 2)
        // and, while the sea animates, keep requesting redraws so it keeps moving
        // — the only water clock is read here, at the edge (I3). A still sea
        // (`ripple_*` = 0) leaves the window event-driven as before.
        self.scene
            .set_time(self.clock_start.elapsed().as_secs_f32());
        if self.water_animates {
            self.window.request_redraw();
        }

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
