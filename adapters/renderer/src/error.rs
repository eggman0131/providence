//! Errors from the GPU-backed workbench adapters (issue #8 Phase 1).
//!
//! The pure modules cannot fail; only the `wgpu`/`winit` paths can (no adapter,
//! device creation, surface/window setup, or a headless read-back). Those
//! surface here as one small typed error the composition root can print.

use std::fmt;

/// A failure setting up or driving the GPU workbench renderer.
#[derive(Debug)]
pub enum RendererError {
    /// No GPU adapter satisfied the workbench's requirements.
    NoAdapter,
    /// The GPU logical device could not be created.
    Device(String),
    /// The window surface could not be created or configured.
    Surface(String),
    /// The `winit` event loop could not be created or run.
    EventLoop(String),
    /// A headless capture could not be rendered, read back, or encoded.
    Capture(String),
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RendererError::NoAdapter => write!(f, "no compatible GPU adapter was found"),
            RendererError::Device(detail) => write!(f, "GPU device error: {detail}"),
            RendererError::Surface(detail) => write!(f, "window surface error: {detail}"),
            RendererError::EventLoop(detail) => write!(f, "event loop error: {detail}"),
            RendererError::Capture(detail) => write!(f, "headless capture error: {detail}"),
        }
    }
}

impl std::error::Error for RendererError {}
