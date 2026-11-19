// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! How the window gets its graphics: backend choice, adapter selection (a real graphics card beats a software
//! renderer) and logging of every graphics error.

use crate::logging::{log, log_sync};
use eframe::egui_wgpu::{WgpuSetup, WgpuSetupCreateNew};
use rolidium_render::wgpu;
use rolidium_render::{adapter_rank, is_software, BackendChoice, Step};
use std::sync::Arc;

/// The backend to use: `--backend` wins over the environment variable `ROLIDIUM_BACKEND`, which wins over the setting.
pub fn chosen_backend(args: &[String], setting: &str) -> BackendChoice {
    let from_arg = args
        .iter()
        .position(|a| a == "--backend")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);
    let env = std::env::var("ROLIDIUM_BACKEND").ok();
    let text = from_arg.or(env.as_deref()).unwrap_or(setting);
    BackendChoice::parse(text).unwrap_or_else(|| {
        log(&format!("unknown backend '{text}', using auto"));
        BackendChoice::Auto
    })
}

/// Whether the user named a backend on the command line or in the environment (`--backend auto` then means "forget the skips").
pub fn backend_given(args: &[String]) -> bool {
    args.iter().any(|a| a == "--backend") || std::env::var("ROLIDIUM_BACKEND").is_ok()
}

/// wgpu setup for eframe for one step: exactly the backend of the step (an instance with one backend only, so no other driver is
/// loaded), and with `hardware_only` software adapters are refused.
pub fn wgpu_setup(step: Step) -> WgpuSetup {
    let hardware_only = step.hardware_only;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = step.backend.backends();
    log_sync(&format!(
        "graphics: handing over to the window library: instance for {}, then adapter, device and surface",
        step.backend.name()
    ));
    let default_descriptor = setup.device_descriptor.clone();
    setup.device_descriptor = Arc::new(move |adapter| {
        let i = adapter.get_info();
        log_sync(&format!(
            "graphics: creating the device on {} ({:?})",
            i.name, i.backend
        ));
        default_descriptor(adapter)
    });
    setup.native_adapter_selector = Some(Arc::new(move |adapters, surface| {
        log_sync("graphics: choosing the adapter for the window");
        let mut usable: Vec<&wgpu::Adapter> = adapters
            .iter()
            .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
            .filter(|a| !(hardware_only && is_software(&a.get_info())))
            .collect();
        usable.sort_by_key(|a| std::cmp::Reverse(adapter_rank(&a.get_info())));
        for a in adapters {
            let i = a.get_info();
            log(&format!(
                "graphics adapter found: {} ({:?}, {:?}){}",
                i.name,
                i.backend,
                i.device_type,
                if surface.is_none_or(|s| a.is_surface_supported(s)) {
                    ""
                } else {
                    " - cannot draw to this window"
                }
            ));
        }
        usable
            .first()
            .map(|a| (*a).clone())
            .ok_or_else(|| "no graphics adapter can draw to the window".to_string())
    }));
    WgpuSetup::CreateNew(setup)
}

/// Writes every graphics error (validation, pipeline creation, device loss) to the log.
pub fn install_error_logging(device: &wgpu::Device) {
    device.on_uncaptured_error(Arc::new(|e| log(&format!("graphics error: {e}"))));
    device.set_device_lost_callback(|reason, message| {
        log(&format!("graphics device lost ({reason:?}): {message}"))
    });
}

pub fn is_software_adapter(info: &wgpu::AdapterInfo) -> bool {
    is_software(info)
}
