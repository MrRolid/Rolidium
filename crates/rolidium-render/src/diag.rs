// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Graphics diagnostics: which backends and adapters exist, which one is chosen, and a test scene
//! (grid, axes, a sketch region, a body) rendered off screen with a check that every part is really in the picture.

use crate::{BodyDraw, Camera, Frame, PreviewDraw, RenderError, Renderer, StandardView};
use glam::Vec3;
use rolidium_kernel::{DVec3, Solid};
use rolidium_sketch::{DVec2, Entity, PlaneDef, Sketch};

/// The backend the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendChoice {
    #[default]
    Auto,
    Dx12,
    Vulkan,
    Gl,
}

impl BackendChoice {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Some(Self::Auto),
            "dx12" | "d3d12" => Some(Self::Dx12),
            "vulkan" | "vk" => Some(Self::Vulkan),
            "gl" | "opengl" | "gles" => Some(Self::Gl),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dx12 => "dx12",
            Self::Vulkan => "vulkan",
            Self::Gl => "gl",
        }
    }

    /// The wgpu backends that may be used. `Auto` leaves out OpenGL where a better one exists.
    pub fn backends(self) -> wgpu::Backends {
        match self {
            Self::Auto => wgpu::Backends::PRIMARY | wgpu::Backends::GL,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Gl => wgpu::Backends::GL,
        }
    }
}

/// One step of the automatic backend search: a backend of its own (one instance per step, so that the drivers of the other
/// backends are never loaded) and whether only a real graphics card is acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub backend: BackendChoice,
    pub hardware_only: bool,
}

/// The order in which `auto` tries the backends. On Windows: DirectX 12 with a real graphics card, then Vulkan with a real card,
/// then OpenGL, and only then software adapters (DirectX 12, Vulkan). Vulkan is never touched while DirectX 12 works, because some
/// Vulkan drivers of integrated graphics crash inside the driver when they are merely loaded. Elsewhere there is one step that
/// leaves the choice to wgpu.
pub fn auto_order(windows: bool) -> Vec<Step> {
    let s = |backend, hardware_only| Step {
        backend,
        hardware_only,
    };
    if windows {
        vec![
            s(BackendChoice::Dx12, true),
            s(BackendChoice::Vulkan, true),
            s(BackendChoice::Gl, true),
            s(BackendChoice::Dx12, false),
            s(BackendChoice::Vulkan, false),
            s(BackendChoice::Gl, false),
        ]
    } else {
        vec![s(BackendChoice::Auto, false)]
    }
}

/// The best acceptable adapter of a list (index), or `None`. Hardware-only steps reject software adapters.
pub fn best_adapter_index(infos: &[wgpu::AdapterInfo], hardware_only: bool) -> Option<usize> {
    infos
        .iter()
        .enumerate()
        .filter(|(_, i)| !(hardware_only && is_software(i)))
        .max_by_key(|(n, i)| (adapter_rank(i), std::cmp::Reverse(*n)))
        .map(|(n, _)| n)
}

/// Higher is better: a real graphics card beats an integrated one, which beats a virtual one, and a software
/// renderer comes last. Within a type the order is Vulkan, DX12, Metal, GL.
pub fn adapter_rank(info: &wgpu::AdapterInfo) -> u32 {
    let kind = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 50,
        wgpu::DeviceType::IntegratedGpu => 40,
        wgpu::DeviceType::VirtualGpu => 30,
        wgpu::DeviceType::Other => 20,
        wgpu::DeviceType::Cpu => 0,
    };
    let backend = match info.backend {
        wgpu::Backend::Vulkan => 4,
        wgpu::Backend::Dx12 => 3,
        wgpu::Backend::Metal => 3,
        wgpu::Backend::Gl => 1,
        _ => 0,
    };
    // the "Microsoft Basic Render Driver" is a software adapter even when it reports itself otherwise
    let software_name = info.name.contains("Basic Render") || info.name.to_lowercase().contains("llvmpipe");
    if software_name {
        return backend;
    }
    kind + backend
}

pub fn is_software(info: &wgpu::AdapterInfo) -> bool {
    info.device_type == wgpu::DeviceType::Cpu
        || info.name.contains("Basic Render")
        || info.name.to_lowercase().contains("llvmpipe")
        || info.name.to_lowercase().contains("swiftshader")
}

/// All adapters of the given backends, best first.
pub fn enumerate(backends: wgpu::Backends) -> Vec<wgpu::Adapter> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let mut v = pollster::block_on(instance.enumerate_adapters(backends));
    v.sort_by_key(|a| std::cmp::Reverse(adapter_rank(&a.get_info())));
    v
}

pub fn backend_name(b: wgpu::Backend) -> &'static str {
    match b {
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Gl => "gl",
        wgpu::Backend::Metal => "metal",
        _ => "other",
    }
}

/// One adapter described as text (no personal data).
pub fn describe_adapter(a: &wgpu::Adapter) -> String {
    let i = a.get_info();
    let l = a.limits();
    format!(
        "name: {}\nbackend: {}\ntype: {:?}{}\nvendor id: {:#06x}   device id: {:#06x}\ndriver: {} {}\nfeatures: {:?}\nlimits: max texture 2D {}, max bind groups {}, max uniform buffer {} B, max storage buffer {} B, max vertex buffers {}, max color attachments {}\n",
        i.name,
        backend_name(i.backend),
        i.device_type,
        if is_software(&i) { " (software)" } else { "" },
        i.vendor,
        i.device,
        i.driver,
        i.driver_info,
        a.features(),
        l.max_texture_dimension_2d,
        l.max_bind_groups,
        l.max_uniform_buffer_binding_size,
        l.max_storage_buffer_binding_size,
        l.max_vertex_buffers,
        l.max_color_attachments,
    )
}

/// What a rendered test picture contains.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneCheck {
    pub grid_pixels: u32,
    pub x_axis_pixels: u32,
    pub y_axis_pixels: u32,
    pub sketch_pixels: u32,
    pub body_pixels: u32,
}

impl SceneCheck {
    pub fn ok(&self) -> bool {
        self.grid_pixels >= 200
            && self.x_axis_pixels >= 20
            && self.y_axis_pixels >= 20
            && self.sketch_pixels >= 20
            && self.body_pixels >= 500
    }

    pub fn describe(&self) -> String {
        format!(
            "grid {} px, X axis {} px, Y axis {} px, sketch {} px, body {} px => {}",
            self.grid_pixels,
            self.x_axis_pixels,
            self.y_axis_pixels,
            self.sketch_pixels,
            self.body_pixels,
            if self.ok() {
                "all present"
            } else {
                "SOMETHING IS MISSING"
            }
        )
    }
}

/// Counts the characteristic colours of the test scene in an RGBA picture.
pub fn check_picture(rgba: &[u8]) -> SceneCheck {
    let mut c = SceneCheck::default();
    for p in rgba.as_chunks::<4>().0 {
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        // red X axis, green Y axis, yellow sketch outline, grey-blue body, faint grid lines
        if r > 150 && g < 110 && b < 110 {
            c.x_axis_pixels += 1;
        } else if g > 140 && r < 140 && b < 140 {
            c.y_axis_pixels += 1;
        } else if r > 200 && g > 170 && b < 140 {
            c.sketch_pixels += 1;
        } else if r > 90 && g > 100 && b > 110 && (r - b).abs() < 60 {
            c.body_pixels += 1;
        } else if (r + g + b) / 3 > 52 && (r - b).abs() < 25 {
            c.grid_pixels += 1;
        }
    }
    c
}

/// Renders the test scene with the given device: grid, axes, a box, and a sketch with one circular region
/// on the base plane. The box stands beside the sketch so that both are visible from above and from the side.
pub fn render_test_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: [u32; 2],
) -> Result<(Vec<u8>, SceneCheck), RenderError> {
    let mut r = Renderer::new(device, wgpu::TextureFormat::Rgba8Unorm);
    let solid = Solid::cuboid(DVec3::new(10.0, 10.0, 0.0), DVec3::new(50.0, 40.0, 20.0))
        .map_err(|e| RenderError::Device(e.to_string()))?;
    let mesh = solid
        .display_mesh()
        .map_err(|e| RenderError::Device(e.to_string()))?;
    r.set_bodies(
        device,
        &[BodyDraw {
            id: 1,
            mesh: &mesh,
            color: [0.62, 0.68, 0.76],
            stamp: 0,
        }],
    );
    let plane = PlaneDef::base();
    let mut sk = Sketch::new();
    sk.add(Entity::Circle {
        center: DVec2::new(-30.0, 25.0),
        radius: 15.0,
    })
    .map_err(|e| RenderError::Device(format!("{e:?}")))?;
    let mut fills = Vec::new();
    for reg in sk.regions() {
        fills.push(crate::region_geometry(&plane, &reg));
    }
    let refs: Vec<&crate::RegionGeom> = fills.iter().collect();
    r.set_layers(
        device,
        &[
            crate::fill_layer(&refs, [0.35, 0.65, 1.0, 0.22]),
            PreviewDraw {
                lines: crate::sketch_lines(&plane, &sk),
                line_color: [1.0, 0.85, 0.35, 1.0],
                thick: crate::sketch_lines(&plane, &sk),
                thick_color: [1.0, 0.85, 0.35, 1.0],
                ..Default::default()
            },
        ],
    );
    let mut cam = Camera::default();
    cam.set_view(StandardView::Isometric);
    cam.fit(Vec3::new(-50.0, 0.0, 0.0), Vec3::new(60.0, 50.0, 20.0));
    let frame = Frame {
        camera: cam,
        hover: 0,
        selected: vec![],
        body_marks: Vec::new(),
        size,
        grid: None,
        render_ids: true,
        grid_levels: None,
        scene_radius: 0.0,
        xray: false,
        clip: None,
    };
    let rgba = r.render_to_rgba(device, queue, &frame)?;
    let check = check_picture(&rgba);
    Ok((rgba, check))
}

/// The result of trying one adapter: its description, the picture and its check, or the error.
pub struct AdapterRun {
    pub label: String,
    pub description: String,
    pub result: Result<(Vec<u8>, SceneCheck), String>,
    /// Graphics errors (validation, pipeline creation, device loss) reported while this adapter was used.
    pub errors: Vec<String>,
}

/// Renders the test scene on one adapter (device creation errors are reported, not fatal).
pub fn run_adapter(a: &wgpu::Adapter, label: String, size: [u32; 2]) -> AdapterRun {
    let description = describe_adapter(a);
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = errors.clone();
    let result = (|| -> Result<(Vec<u8>, SceneCheck), String> {
        let (device, queue) = pollster::block_on(a.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("cannot create a device: {e}"))?;
        device.on_uncaptured_error(std::sync::Arc::new(move |e| {
            if let Ok(mut v) = sink.lock() {
                v.push(e.to_string());
            }
        }));
        render_test_scene(&device, &queue, size).map_err(|e| e.to_string())
    })();
    let errors = errors.lock().map(|v| v.clone()).unwrap_or_default();
    AdapterRun {
        label,
        description,
        result,
        errors,
    }
}

#[cfg(test)]
mod order_tests {
    use super::*;

    fn info(name: &str, kind: wgpu::DeviceType, backend: wgpu::Backend) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: name.into(),
            vendor: 0,
            device: 0,
            device_type: kind,
            device_pci_bus_id: String::new(),
            driver: String::new(),
            driver_info: String::new(),
            backend,
            subgroup_min_size: 0,
            subgroup_max_size: 0,
            transient_saves_memory: None,
            limit_bucket: None,
        }
    }

    #[test]
    fn auto_on_windows_goes_dx12_vulkan_gl_then_software_and_one_backend_per_step() {
        let o = auto_order(true);
        let names: Vec<(&str, bool)> = o.iter().map(|s| (s.backend.name(), s.hardware_only)).collect();
        assert_eq!(
            names,
            vec![
                ("dx12", true),
                ("vulkan", true),
                ("gl", true),
                ("dx12", false),
                ("vulkan", false),
                ("gl", false)
            ]
        );
        for s in &o {
            assert_eq!(
                s.backend.backends().iter().count(),
                1,
                "{s:?} must hold one backend only"
            );
        }
        // elsewhere nothing changes: one step that leaves the choice to wgpu
        assert_eq!(
            auto_order(false),
            vec![Step {
                backend: BackendChoice::Auto,
                hardware_only: false
            }]
        );
    }

    #[test]
    fn hardware_steps_refuse_software_adapters() {
        use wgpu::{Backend::Dx12, DeviceType::*};
        let list = vec![
            info("Microsoft Basic Render Driver", Cpu, Dx12),
            info("Intel UHD", IntegratedGpu, Dx12),
        ];
        assert_eq!(best_adapter_index(&list, true), Some(1));
        assert_eq!(best_adapter_index(&list, false), Some(1));
        let only_soft = vec![info("Microsoft Basic Render Driver", Cpu, Dx12)];
        assert_eq!(best_adapter_index(&only_soft, true), None);
        assert_eq!(best_adapter_index(&only_soft, false), Some(0));
        assert_eq!(best_adapter_index(&[], false), None);
    }
}
