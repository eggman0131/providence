//! Shared `wgpu` device acquisition (issue #8 Phase 1).
//!
//! GPU code — not gated (I9). Both adapters (windowed and headless) need a
//! device and queue; only the details of the render target differ. This module
//! requests an adapter and device once, blocking on `wgpu`'s async setup with
//! `pollster` at the edge (no async runtime reaches the rest of the crate).

use crate::error::RendererError;

/// A ready GPU device: the logical [`wgpu::Device`], its command [`wgpu::Queue`],
/// and the [`wgpu::Adapter`] they came from (kept for surface configuration).
pub struct Gpu {
    /// The logical device commands are recorded against.
    pub device: wgpu::Device,
    /// The queue submitted command buffers run on.
    pub queue: wgpu::Queue,
    /// The physical adapter, needed to query surface capabilities.
    pub adapter: wgpu::Adapter,
}

/// Request an adapter and device. Pass the window surface as `compatible_surface`
/// so the chosen adapter can present to it; pass `None` for headless capture.
pub fn request_gpu(
    instance: &wgpu::Instance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
) -> Result<Gpu, RendererError> {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface,
        force_fallback_adapter: false,
    }))
    .map_err(|_| RendererError::NoAdapter)?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("workbench-device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .map_err(|error| RendererError::Device(error.to_string()))?;

    Ok(Gpu {
        device,
        queue,
        adapter,
    })
}
