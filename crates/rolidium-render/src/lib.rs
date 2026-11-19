// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Viewport renderer for Rolidium (wgpu).
//!
//! The scene is drawn into an offscreen target (4x MSAA colour + depth) and a second pass
//! writes face IDs for picking. The same code serves the egui viewport and the headless
//! CLI (offscreen PNG).

mod camera;
mod diag;
mod gpu;
mod preview;
mod sketch_view;

pub use camera::{grid_levels, grid_step, Camera, StandardView};
pub use diag::{
    adapter_rank, auto_order, backend_name, best_adapter_index, check_picture, describe_adapter, enumerate,
    is_software, render_test_scene, run_adapter, AdapterRun, BackendChoice, SceneCheck, Step,
};
pub use gpu::{
    decode_pick, encode_pick, encode_png, BodyDraw, Frame, GridPlane, PreviewDraw, RenderTarget, Renderer,
    ID_FACE_BITS, ID_NONE,
};
pub use preview::{face_boundary, pull_preview};
pub use rolidium_sketch;
pub use sketch_view::{fill_layer, region_geometry, region_pull_preview, sketch_lines, RegionGeom};
pub use wgpu;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("No usable graphics adapter was found (tried hardware and software rendering)")]
    NoAdapter,
    #[error("Graphics device error: {0}")]
    Device(String),
    #[error("Image encoding error: {0}")]
    Image(String),
}

/// Requests an adapter: a real graphics card is preferred over a software renderer.
/// Returns the adapter plus whether it is a software (CPU) implementation.
pub fn request_adapter(instance: &wgpu::Instance) -> Result<(wgpu::Adapter, bool), RenderError> {
    request_adapter_for(instance, BackendChoice::Auto)
}

pub fn request_adapter_for(
    instance: &wgpu::Instance,
    choice: BackendChoice,
) -> Result<(wgpu::Adapter, bool), RenderError> {
    let mut all = pollster::block_on(instance.enumerate_adapters(choice.backends()));
    all.sort_by_key(|a| std::cmp::Reverse(adapter_rank(&a.get_info())));
    if let Some(a) = all.into_iter().next() {
        let software = is_software(&a.get_info());
        return Ok((a, software));
    }
    for force_fallback in [false, true] {
        let opts = wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: force_fallback,
            compatible_surface: None,
            apply_limit_buckets: false,
        };
        if let Ok(a) = pollster::block_on(instance.request_adapter(&opts)) {
            let software = is_software(&a.get_info());
            return Ok((a, software));
        }
    }
    Err(RenderError::NoAdapter)
}

/// Creates a headless device and queue (for the CLI and tests).
pub fn headless_device() -> Result<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo), RenderError> {
    // ROLIDIUM_BACKEND=auto|dx12|vulkan|gl picks the backend of the command-line tool
    let choice = std::env::var("ROLIDIUM_BACKEND")
        .ok()
        .and_then(|b| BackendChoice::parse(&b))
        .unwrap_or_default();
    headless_device_with(choice)
}

pub fn headless_device_with(
    choice: BackendChoice,
) -> Result<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo), RenderError> {
    // `auto` tries the backends one after another, each with an instance of its own (see `auto_order`)
    let steps = if choice == BackendChoice::Auto {
        auto_order(cfg!(windows))
    } else {
        vec![Step {
            backend: choice,
            hardware_only: false,
        }]
    };
    let mut last = RenderError::NoAdapter;
    for step in steps {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: step.backend.backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let found = if step.hardware_only {
            let mut all = pollster::block_on(instance.enumerate_adapters(step.backend.backends()));
            let infos: Vec<wgpu::AdapterInfo> = all.iter().map(|a| a.get_info()).collect();
            best_adapter_index(&infos, true).map(|n| all.swap_remove(n))
        } else {
            request_adapter_for(&instance, step.backend).ok().map(|(a, _)| a)
        };
        let Some(adapter) = found else { continue };
        let info = adapter.get_info();
        match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) {
            Ok((device, queue)) => return Ok((device, queue, info)),
            Err(e) => last = RenderError::Device(e.to_string()),
        }
    }
    Err(last)
}
