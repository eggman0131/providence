//! The read-only developer HUD overlay (ADR 0015; issue #8 Phase 3).
//!
//! Compiled **only** under the `debug-hud` cargo feature — absent from a default
//! release build (ADR 0015). It draws an `egui` panel over the terrain surface
//! this adapter already owns (no second GUI/render stack): the grid dimensions,
//! the live camera pose, and the vertex under the screen-centre reticle. It is a
//! **read-only** presentation surface (ADR 0015): the [`Readout`] it draws is a
//! derived snapshot built at the edge from the resolved camera and the terrain
//! grid; nothing here holds simulation state or can mutate a height (ADR 0020
//! §3). The richer app-assembled `DiagnosticsSnapshot` (sim/advisor/timings)
//! that ADR 0015 describes lands when there is a live sim to read; Phase 3 shows
//! the presentation state the workbench already has.
//!
//! The `egui`/`egui-wgpu` glue is GPU code and is **not** gated (I9); it is
//! exercised through the headless-PNG capture. The pure part — turning a
//! [`Readout`] into the panel's text lines ([`readout_lines`]) — is unit-tested
//! and runs in the gate's `--features debug-hud` pass (ADR 0020 enforcement).

use egui_wgpu::{Renderer, RendererOptions};

/// The overlay's target size + DPI, handed to [`Hud::record`]. Re-exported so
/// the renderer adapters can build it without naming `egui-wgpu` directly.
pub use egui_wgpu::ScreenDescriptor;

use providence_config::HudParams;

use crate::camera::Camera;

/// egui frames run per `record` so an auto-sized `Window` settles its layout
/// before it is drawn (the first pass measures, the second places). Structural
/// egui plumbing, not a tunable — like the mesh's buffer strides, it stays code.
const HUD_SETTLE_PASSES: usize = 2;
use crate::pick::{self, GridSnapshot, PickedVertex};
use providence_ports::TerrainFrame;

/// The derived, read-only data the overlay draws for one frame (ADR 0015): the
/// grid dimensions, the camera pose, and the reticle vertex. Built at the edge
/// from the resolved [`Camera`] and the terrain snapshot; carries no simulation
/// or camera *control* state.
#[derive(Clone, Debug, PartialEq)]
pub struct Readout {
    /// Grid columns.
    pub grid_width: u32,
    /// Grid rows.
    pub grid_height: u32,
    /// Orbit yaw, degrees.
    pub yaw_degrees: f32,
    /// Orbit pitch, degrees.
    pub pitch_degrees: f32,
    /// Orbit distance from the look-at target, world units.
    pub distance: f32,
    /// World-space eye position.
    pub eye: [f32; 3],
    /// The vertex under the screen-centre reticle, or `None` if the crosshair
    /// points at empty space.
    pub reticle: Option<PickedVertex>,
}

impl Readout {
    /// Build the readout from the resolved camera and the terrain grid: pick the
    /// reticle vertex ([`pick`]) and recover the orbit pose
    /// ([`Camera::orbit_pose`]). `aspect` is the viewport's `width / height`
    /// (so the reticle ray matches the projection); `vertical_scale` matches the
    /// drawn mesh.
    #[must_use]
    pub fn new(camera: &Camera, aspect: f32, grid: &GridSnapshot, vertical_scale: f32) -> Self {
        let frame: TerrainFrame<'_> = grid.frame();
        let reticle = pick::pick_vertex(&pick::reticle_ray(camera, aspect), &frame, vertical_scale);
        let (yaw_degrees, pitch_degrees, distance) = camera.orbit_pose();
        Self {
            grid_width: grid.width,
            grid_height: grid.height,
            yaw_degrees,
            pitch_degrees,
            distance,
            eye: camera.eye,
            reticle,
        }
    }
}

/// The overlay's visible text lines for a [`Readout`], honouring the panel
/// toggles. Pure and unit-tested — the panel just lays these out.
#[must_use]
pub fn readout_lines(readout: &Readout, params: &HudParams) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "Grid: {} × {} vertices",
        readout.grid_width, readout.grid_height
    ));
    if params.show_camera {
        lines.push(format!(
            "Camera: yaw {:.0}°  pitch {:.0}°  dist {:.1}",
            readout.yaw_degrees, readout.pitch_degrees, readout.distance
        ));
        lines.push(format!(
            "Eye: ({:.1}, {:.1}, {:.1})",
            readout.eye[0], readout.eye[1], readout.eye[2]
        ));
    }
    if params.show_reticle {
        lines.push(match readout.reticle {
            Some(v) => format!("Reticle: vertex ({}, {})  height {}", v.x, v.y, v.height),
            None => "Reticle: (no vertex under crosshair)".to_string(),
        });
    }
    lines
}

/// The `egui` overlay: an immediate-mode context plus the `egui-wgpu` renderer
/// that paints its triangles into the surface. Held by whichever renderer adapter
/// owns the surface (windowed or headless); created only when the HUD is enabled.
pub struct Hud {
    context: egui::Context,
    renderer: Renderer,
    params: HudParams,
}

impl Hud {
    /// Build the overlay for a target of the given colour format. `color_format`
    /// is the surface's (windowed) or the capture texture's; the HUD draws with
    /// no depth buffer, over the already-drawn terrain.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        params: HudParams,
    ) -> Self {
        // No depth buffer: the overlay draws flat, over the terrain already
        // rendered. `RendererOptions` defaults are msaa off, no depth.
        let renderer = Renderer::new(device, color_format, RendererOptions::default());
        Self {
            context: egui::Context::default(),
            renderer,
            params,
        }
    }

    /// Record the overlay into `encoder`, drawing over `color_view` (loaded, not
    /// cleared, so it lands on top of the terrain). Returns any command buffers
    /// `egui-wgpu` needs run **before** the encoder — the caller submits them
    /// first. `screen` carries the target's pixel size and scale (window DPI, or
    /// 1.0 headless).
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        screen: &ScreenDescriptor,
        readout: &Readout,
    ) -> Vec<wgpu::CommandBuffer> {
        let pixels_per_point = screen.pixels_per_point;
        self.context.set_pixels_per_point(pixels_per_point);
        let screen_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(
                screen.size_in_pixels[0] as f32 / pixels_per_point,
                screen.size_in_pixels[1] as f32 / pixels_per_point,
            ),
        );
        let raw_input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };

        // An auto-sized `Window` positions/sizes itself from the previous
        // frame's memory, so a single pass renders it collapsed. Run twice —
        // uploading each pass's new textures (the font atlas lands on the first)
        // — and tessellate the settled second pass. The windowed adapter settles
        // over successive redraws; this makes the one-shot headless capture agree.
        let params = self.params.clone();
        let mut primitives = Vec::new();
        let mut to_free = Vec::new();
        for pass in 0..HUD_SETTLE_PASSES {
            let output = self
                .context
                .run_ui(raw_input(), |ui| draw_panel(ui, readout, &params));
            for (id, delta) in &output.textures_delta.set {
                self.renderer.update_texture(device, queue, *id, delta);
            }
            if pass + 1 == HUD_SETTLE_PASSES {
                primitives = self
                    .context
                    .tessellate(output.shapes, output.pixels_per_point);
                to_free = output.textures_delta.free;
            }
        }

        let command_buffers =
            self.renderer
                .update_buffers(device, queue, encoder, &primitives, screen);

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("hud-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.renderer
                .render(&mut pass.forget_lifetime(), &primitives, screen);
        }

        for id in &to_free {
            self.renderer.free_texture(id);
        }
        command_buffers
    }
}

/// Draw the overlay: a fixed, non-interactive panel of [`readout_lines`], plus a
/// crosshair at the screen centre so the reticle the readout reports is visible.
/// Runs against the root [`egui::Ui`] `run_ui` hands us (egui 0.35).
fn draw_panel(ui: &mut egui::Ui, readout: &Readout, params: &HudParams) {
    // An opaque dark panel with bright text so the readout stays legible over
    // the lit terrain (egui's default translucent window washes out against it).
    let frame = egui::Frame::window(ui.style())
        .fill(egui::Color32::from_rgba_unmultiplied(16, 18, 24, 236));
    let ctx = ui.ctx();
    let title = egui::RichText::new("Terrain Workbench")
        .strong()
        .color(egui::Color32::WHITE);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .movable(false)
        .frame(frame)
        .show(ctx, |ui| {
            for line in readout_lines(readout, params) {
                ui.label(
                    egui::RichText::new(line)
                        .size(15.0)
                        .color(egui::Color32::from_rgb(232, 236, 240)),
                );
            }
        });

    if params.show_reticle {
        draw_reticle(ctx, ui.max_rect().center());
    }
}

/// Draw a small crosshair at `center` (the screen centre) — the reticle the
/// readout identifies a vertex under.
fn draw_reticle(ctx: &egui::Context, center: egui::Pos2) {
    let arm = 8.0;
    let stroke = egui::Stroke::new(1.5, egui::Color32::from_white_alpha(200));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("workbench-reticle"),
    ));
    painter.line_segment(
        [center - egui::vec2(arm, 0.0), center + egui::vec2(arm, 0.0)],
        stroke,
    );
    painter.line_segment(
        [center - egui::vec2(0.0, arm), center + egui::vec2(0.0, arm)],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::{Readout, readout_lines};
    use crate::pick::PickedVertex;
    use providence_config::HudParams;

    fn readout() -> Readout {
        Readout {
            grid_width: 32,
            grid_height: 32,
            yaw_degrees: 45.0,
            pitch_degrees: 30.0,
            distance: 24.0,
            eye: [1.0, 2.0, 3.0],
            reticle: Some(PickedVertex {
                x: 16,
                y: 16,
                height: 9,
            }),
        }
    }

    fn all_on() -> HudParams {
        HudParams {
            enabled: true,
            show_camera: true,
            show_reticle: true,
        }
    }

    #[test]
    fn all_sections_on_show_grid_camera_and_reticle() {
        let lines = readout_lines(&readout(), &all_on());
        assert_eq!(lines.len(), 4, "grid + 2 camera + reticle");
        assert!(lines[0].contains("32 × 32"), "grid dimensions");
        assert!(lines.iter().any(|l| l.contains("yaw 45")), "camera pose");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("(16, 16)") && l.contains("height 9")),
            "reticle vertex + height",
        );
    }

    #[test]
    fn toggling_camera_off_drops_its_lines() {
        let params = HudParams {
            show_camera: false,
            ..all_on()
        };
        let lines = readout_lines(&readout(), &params);
        assert!(
            !lines.iter().any(|l| l.contains("Camera")),
            "no camera section when toggled off",
        );
        assert!(lines.iter().any(|l| l.contains("Reticle")), "reticle stays");
    }

    #[test]
    fn an_empty_reticle_reads_as_no_vertex() {
        let mut readout = readout();
        readout.reticle = None;
        let lines = readout_lines(&readout, &all_on());
        assert!(
            lines
                .iter()
                .any(|l| l.contains("no vertex under crosshair")),
            "an empty reticle says so plainly",
        );
    }
}
