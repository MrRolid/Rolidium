// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Camera, RenderError};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use rolidium_kernel::Mesh;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

pub const ID_FACE_BITS: u32 = 20;
pub const ID_NONE: u32 = 0;

/// Packs (body slot, face index) into a non-zero pick ID.
pub fn encode_pick(slot: u32, face: u32) -> u32 {
    ((slot + 1) << ID_FACE_BITS) | (face & ((1 << ID_FACE_BITS) - 1))
}

pub fn decode_pick(id: u32) -> Option<(u32, u32)> {
    if id == ID_NONE {
        None
    } else {
        Some(((id >> ID_FACE_BITS) - 1, id & ((1 << ID_FACE_BITS) - 1)))
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    hover: u32,
    selected_count: u32,
    pad: [u32; 2],
    selected: [u32; 8],
    body_marks: [u32; 16],
    grid_step: [f32; 4],
    grid_o: [f32; 4],
    grid_u: [f32; 4],
    grid_v: [f32; 4],
    viewport: [f32; 4],
}

pub struct BodyDraw<'a> {
    pub id: u64,
    pub mesh: &'a Mesh,
    pub color: [f32; 3],
}

/// Translucent preview geometry (triangle list and line list).
#[derive(Debug, Clone, Default)]
pub struct PreviewDraw {
    pub triangles: Vec<Vec3>,
    pub lines: Vec<Vec3>,
    /// Colour (with alpha) of the triangles.
    pub color: [f32; 4],
    /// Colour of the lines.
    pub line_color: [f32; 4],
    /// Segment pairs drawn as thick lines (about 3 px) in `thick_color`.
    pub thick: Vec<Vec3>,
    pub thick_color: [f32; 4],
}

/// The plane the grid is drawn on (sketch plane); `None` in a frame means the world XY plane.
#[derive(Debug, Clone, Copy)]
pub struct GridPlane {
    pub origin: Vec3,
    pub u: Vec3,
    pub v: Vec3,
}

pub struct Frame {
    pub camera: Camera,
    /// Pick ID of the hovered face or `ID_NONE`.
    pub hover: u32,
    /// Pick IDs of selected faces (at most 8 are highlighted).
    pub selected: Vec<u32>,
    /// Whole bodies to tint: (body slot, kind) with kind 1 selected, 2 target, 3 tool, 4 hovered (at most 16).
    pub body_marks: Vec<(u32, u32)>,
    pub size: [u32; 2],
    pub grid: Option<GridPlane>,
    /// Redraw the face-ID buffer. The app sets it when the camera, scene or viewport changed.
    pub render_ids: bool,
    /// Minor and major grid spacing (mm); `None` picks them from the camera distance.
    pub grid_levels: Option<(f32, f32)>,
    /// Radius of the model (mm), 0 when unknown: the grid is drawn at least this far.
    pub scene_radius: f32,
}

struct GpuBody {
    id: u64,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    ebuf: wgpu::Buffer,
    edge_count: u32,
    /// Smooth edges (seams between faces that continue each other): drawn faintly.
    sbuf: wgpu::Buffer,
    soft_count: u32,
    bind: wgpu::BindGroup,
}

struct GpuPreview {
    tri_buf: wgpu::Buffer,
    tri_count: u32,
    line_buf: wgpu::Buffer,
    line_count: u32,
    tri_bind: wgpu::BindGroup,
    line_bind: wgpu::BindGroup,
    thick_buf: wgpu::Buffer,
    thick_count: u32,
    thick_bind: wgpu::BindGroup,
}

/// Offscreen render target sized to the viewport.
pub struct RenderTarget {
    size: [u32; 2],
    msaa_color: wgpu::TextureView,
    depth: wgpu::TextureView,
    pub color: wgpu::Texture,
    color_view: wgpu::TextureView,
    id_tex: wgpu::Texture,
    id_view: wgpu::TextureView,
    id_depth: wgpu::TextureView,
    blit_bind: wgpu::BindGroup,
}

enum PickState {
    Idle,
    /// Copy recorded into the encoder; mapping not yet requested.
    Copied,
    Mapping(Arc<AtomicU32>),
}

const MAP_PENDING: u32 = u32::MAX;
const MAP_FAILED: u32 = u32::MAX - 1;

pub struct Renderer {
    mesh_pipe: wgpu::RenderPipeline,
    id_pipe: wgpu::RenderPipeline,
    line_pipe: wgpu::RenderPipeline,
    prev_tri_pipe: wgpu::RenderPipeline,
    thick_pipe: wgpu::RenderPipeline,
    ids_dirty: bool,
    grid_pipe: wgpu::RenderPipeline,
    blit_pipe: wgpu::RenderPipeline,
    body_layout: wgpu::BindGroupLayout,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals_buf: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    edge_bind: wgpu::BindGroup,
    soft_bind: wgpu::BindGroup,
    zaxis_bind: wgpu::BindGroup,
    zaxis_buf: wgpu::Buffer,
    bodies: Vec<GpuBody>,
    preview: Option<GpuPreview>,
    layers: Vec<GpuPreview>,
    target: Option<RenderTarget>,
    pick_buf: wgpu::Buffer,
    pick_state: PickState,
    pick_result: u32,
}

fn uniform_bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, color: [f32; 4]) -> wgpu::BindGroup {
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("color uniform"),
        contents: bytemuck::cast_slice(&color),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("color bind"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buf.as_entire_binding(),
        }],
    })
}

impl Renderer {
    /// `blit_format` is the format of the pass that [`Renderer::blit`] will draw into.
    pub fn new(device: &wgpu::Device, blit_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rolidium scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rolidium blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });

        let uniform_entry = |vis| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: vis,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let body_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("per-draw color"),
            entries: &[uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&body_layout)],
            immediate_size: 0,
        });
        let blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit layout"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });

        let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32];
        let mesh_vb = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attrs,
        };
        let line_attrs = wgpu::vertex_attr_array![0 => Float32x3];
        let line_vb = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &line_attrs,
        };

        let depth = |write: bool, bias: i32| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: bias,
                slope_scale: if bias != 0 { 1.0 } else { 0.0 },
                clamp: 0.0,
            },
        };
        let ms = wgpu::MultisampleState {
            count: SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let alpha = Some(wgpu::BlendState::ALPHA_BLENDING);
        let color_target = |blend| {
            Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })
        };
        let prim = |topology| wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..Default::default()
        };

        let mesh_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_mesh"),
                compilation_options: Default::default(),
                buffers: &[Some(mesh_vb.clone())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_mesh"),
                compilation_options: Default::default(),
                targets: &[color_target(None)],
            }),
            primitive: prim(wgpu::PrimitiveTopology::TriangleList),
            depth_stencil: Some(depth(true, 2)),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });
        let id_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ids"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_mesh"),
                compilation_options: Default::default(),
                buffers: &[Some(mesh_vb)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_id"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ID_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: prim(wgpu::PrimitiveTopology::TriangleList),
            depth_stencil: Some(depth(true, 0)),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let line_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[Some(line_vb.clone())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[color_target(alpha)],
            }),
            primitive: prim(wgpu::PrimitiveTopology::LineList),
            depth_stencil: Some(depth(false, 0)),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });
        let prev_tri_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("preview triangles"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_prev"),
                compilation_options: Default::default(),
                buffers: &[Some(line_vb)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[color_target(alpha)],
            }),
            primitive: prim(wgpu::PrimitiveTopology::TriangleList),
            depth_stencil: Some(depth(false, 0)),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });
        let thick_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let thick_vb = wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &thick_attrs,
        };
        let thick_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("thick lines"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_thick"),
                compilation_options: Default::default(),
                buffers: &[Some(thick_vb)],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[color_target(alpha)],
            }),
            primitive: prim(wgpu::PrimitiveTopology::TriangleList),
            depth_stencil: Some(depth(false, 0)),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });
        let grid_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_grid"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_grid"),
                compilation_options: Default::default(),
                targets: &[color_target(alpha)],
            }),
            primitive: prim(wgpu::PrimitiveTopology::TriangleList),
            depth_stencil: Some(depth(false, 0)),
            multisample: ms,
            multiview_mask: None,
            cache: None,
        });
        let blit_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&blit_pl),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: blit_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: prim(wgpu::PrimitiveTopology::TriangleList),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bind"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });
        let edge_bind = uniform_bind(device, &body_layout, [0.08, 0.09, 0.11, 1.0]);
        let soft_bind = uniform_bind(device, &body_layout, [0.08, 0.09, 0.11, 0.22]);
        let zaxis_bind = uniform_bind(device, &body_layout, [0.30, 0.45, 0.95, 1.0]);
        let zaxis_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("z axis"),
            size: 24,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let pick_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pick readback"),
            size: 256,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Renderer {
            mesh_pipe,
            id_pipe,
            line_pipe,
            prev_tri_pipe,
            thick_pipe,
            ids_dirty: true,
            grid_pipe,
            blit_pipe,
            body_layout,
            blit_layout,
            sampler,
            globals_buf,
            globals_bind,
            edge_bind,
            soft_bind,
            zaxis_bind,
            zaxis_buf,
            bodies: Vec::new(),
            preview: None,
            layers: Vec::new(),
            target: None,
            pick_buf,
            pick_state: PickState::Idle,
            pick_result: ID_NONE,
        }
    }

    /// Replaces all bodies. The pick slot of a body is its index in `bodies`.
    pub fn set_bodies(&mut self, device: &wgpu::Device, bodies: &[BodyDraw]) {
        self.bodies.clear();
        self.pick_result = ID_NONE;
        self.ids_dirty = true;
        for b in bodies {
            let m = b.mesh;
            if m.indices.is_empty() {
                continue;
            }
            let slot = self.bodies.len();
            let verts: Vec<Vertex> = (0..m.positions.len())
                .map(|i| Vertex {
                    pos: m.positions[i].to_array(),
                    normal: m.normals[i].to_array(),
                    id: 0,
                })
                .collect();
            // Face IDs are per triangle but vertices are not shared between faces, so the
            // ID can live on the vertex.
            let mut verts = verts;
            for (t, tri) in m.indices.chunks_exact(3).enumerate() {
                let id = encode_pick(slot as u32, m.tri_face[t]);
                for &i in tri {
                    verts[i as usize].id = id;
                }
            }
            let mut lines: Vec<[f32; 3]> = Vec::new();
            let mut soft: Vec<[f32; 3]> = Vec::new();
            for e in &m.edges {
                let target = if e.smooth { &mut soft } else { &mut lines };
                for w in e.points.windows(2) {
                    target.push(w[0].to_array());
                    target.push(w[1].to_array());
                }
            }
            let soft_empty = soft.is_empty();
            if soft_empty {
                soft.push([0.0; 3]);
            }
            let bind = uniform_bind(
                device,
                &self.body_layout,
                [b.color[0], b.color[1], b.color[2], 1.0],
            );
            self.bodies.push(GpuBody {
                id: b.id,
                vbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("body vertices"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                ibuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("body indices"),
                    contents: bytemuck::cast_slice(&m.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                index_count: m.indices.len() as u32,
                ebuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("body edges"),
                    contents: bytemuck::cast_slice(&lines),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                edge_count: lines.len() as u32,
                sbuf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("body smooth edges"),
                    contents: bytemuck::cast_slice(&soft),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                soft_count: if soft_empty { 0 } else { soft.len() as u32 },
                bind,
            });
        }
        // Slots refer to the position in `bodies`; keep a dense mapping.
    }

    /// Body ID of a pick slot, in the order bodies were given to `set_bodies`
    /// (bodies with empty meshes are skipped, so slots are dense).
    pub fn slot_body(&self, slot: u32) -> Option<u64> {
        self.bodies.get(slot as usize).map(|b| b.id)
    }

    fn make_layer(&self, device: &wgpu::Device, p: &PreviewDraw) -> GpuPreview {
        let tri: Vec<[f32; 3]> = p.triangles.iter().map(|v| v.to_array()).collect();
        let lin: Vec<[f32; 3]> = p.lines.iter().map(|v| v.to_array()).collect();
        let mk = |data: &[[f32; 3]]| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay"),
                contents: if data.is_empty() {
                    &[0u8; 12]
                } else {
                    bytemuck::cast_slice(data)
                },
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        GpuPreview {
            tri_buf: mk(&tri),
            tri_count: tri.len() as u32,
            line_buf: mk(&lin),
            line_count: lin.len() as u32,
            tri_bind: uniform_bind(device, &self.body_layout, p.color),
            line_bind: uniform_bind(device, &self.body_layout, p.line_color),
            thick_buf: {
                let t: Vec<[f32; 3]> = p.thick.iter().map(|v| v.to_array()).collect();
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("thick lines"),
                    contents: if t.is_empty() {
                        &[0u8; 24]
                    } else {
                        bytemuck::cast_slice(&t)
                    },
                    usage: wgpu::BufferUsages::VERTEX,
                })
            },
            thick_count: (p.thick.len() / 2) as u32,
            thick_bind: uniform_bind(device, &self.body_layout, p.thick_color),
        }
    }

    /// Sets the Pull preview (drawn last).
    pub fn set_preview(&mut self, device: &wgpu::Device, preview: Option<&PreviewDraw>) {
        self.preview = preview.map(|p| self.make_layer(device, p));
    }

    /// Sets persistent overlay layers (sketch curves, region fills), drawn before the preview.
    pub fn set_layers(&mut self, device: &wgpu::Device, layers: &[PreviewDraw]) {
        self.layers = layers.iter().map(|p| self.make_layer(device, p)).collect();
    }

    /// Returns true if the target was (re)created.
    fn ensure_target(&mut self, device: &wgpu::Device, size: [u32; 2]) -> bool {
        let size = [size[0].max(1), size[1].max(1)];
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return false;
        }
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let tex = |label, format, samples, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let msaa = tex(
            "msaa color",
            COLOR_FORMAT,
            SAMPLES,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let depth = tex(
            "msaa depth",
            DEPTH_FORMAT,
            SAMPLES,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let color = tex(
            "resolved color",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let id_tex = tex(
            "ids",
            ID_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let id_depth = tex(
            "id depth",
            DEPTH_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let color_view = view(&color);
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit bind"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.target = Some(RenderTarget {
            size,
            msaa_color: view(&msaa),
            depth: view(&depth),
            color,
            color_view,
            id_view: view(&id_tex),
            id_tex,
            id_depth: view(&id_depth),
            blit_bind,
        });
        true
    }

    /// Draws the scene and the ID buffer into the offscreen target.
    /// `pick_at` is a pixel position (in target pixels) whose face ID should be read back.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Frame,
        pick_at: Option<[u32; 2]>,
    ) {
        self.advance_pick(device);
        let created = self.ensure_target(device, frame.size);
        let ids_needed = frame.render_ids || created || std::mem::take(&mut self.ids_dirty);
        let aspect = frame.size[0].max(1) as f32 / frame.size[1].max(1) as f32;
        let vp = frame.camera.view_proj(aspect);
        let grid = frame.grid.unwrap_or(GridPlane {
            origin: Vec3::ZERO,
            u: Vec3::X,
            v: Vec3::Y,
        });
        let mut sel = [0u32; 8];
        let n = frame.selected.len().min(8);
        sel[..n].copy_from_slice(&frame.selected[..n]);
        let mut marks = [0u32; 16];
        for (m, (slot, kind)) in marks.iter_mut().zip(frame.body_marks.iter()) {
            *m = (slot + 1) | (kind << 24);
        }
        let g = Globals {
            view_proj: vp.to_cols_array_2d(),
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            eye: frame.camera.eye().extend(1.0).to_array(),
            hover: frame.hover,
            selected_count: n as u32,
            pad: [0; 2],
            selected: sel,
            body_marks: marks,
            grid_step: {
                let (minor, major) = frame.grid_levels.unwrap_or_else(|| {
                    let s = crate::camera::grid_step(frame.camera.distance);
                    (s, s * 10.0)
                });
                // The grid is visible far beyond the model: the reach follows the camera distance and the model size.
                let reach = (frame.camera.distance * 40.0).max(frame.scene_radius * 30.0);
                [minor, reach, major, 0.0]
            },
            grid_o: grid.origin.extend(0.0).to_array(),
            grid_u: grid.u.extend(0.0).to_array(),
            grid_v: grid.v.extend(0.0).to_array(),
            viewport: [frame.size[0] as f32, frame.size[1] as f32, 3.0, 0.0],
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&g));
        let zlen = frame.camera.distance * 0.35;
        queue.write_buffer(
            &self.zaxis_buf,
            0,
            bytemuck::cast_slice(&[[0.0f32, 0.0, 0.0], [0.0, 0.0, zlen]]),
        );

        let t = self.target.as_ref().expect("target");

        // Colour pass
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa_color,
                    depth_slice: None,
                    resolve_target: Some(&t.color_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.16,
                            g: 0.17,
                            b: 0.20,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);
            pass.set_pipeline(&self.mesh_pipe);
            for b in &self.bodies {
                pass.set_bind_group(1, &b.bind, &[]);
                pass.set_vertex_buffer(0, b.vbuf.slice(..));
                pass.set_index_buffer(b.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..b.index_count, 0, 0..1);
            }
            pass.set_pipeline(&self.line_pipe);
            pass.set_bind_group(1, &self.edge_bind, &[]);
            for b in &self.bodies {
                if b.edge_count > 0 {
                    pass.set_vertex_buffer(0, b.ebuf.slice(..));
                    pass.draw(0..b.edge_count, 0..1);
                }
            }
            pass.set_bind_group(1, &self.soft_bind, &[]);
            for b in &self.bodies {
                if b.soft_count > 0 {
                    pass.set_vertex_buffer(0, b.sbuf.slice(..));
                    pass.draw(0..b.soft_count, 0..1);
                }
            }
            pass.set_bind_group(1, &self.zaxis_bind, &[]);
            pass.set_vertex_buffer(0, self.zaxis_buf.slice(..));
            pass.draw(0..2, 0..1);
            // Grid last, depth-tested, so bodies hide it.
            pass.set_pipeline(&self.grid_pipe);
            pass.set_bind_group(1, &self.edge_bind, &[]);
            pass.draw(0..3, 0..1);
            for p in self.layers.iter().chain(self.preview.iter()) {
                if p.thick_count > 0 {
                    pass.set_pipeline(&self.thick_pipe);
                    pass.set_bind_group(1, &p.thick_bind, &[]);
                    pass.set_vertex_buffer(0, p.thick_buf.slice(..));
                    pass.draw(0..6, 0..p.thick_count);
                }
                if p.tri_count > 0 {
                    pass.set_pipeline(&self.prev_tri_pipe);
                    pass.set_bind_group(1, &p.tri_bind, &[]);
                    pass.set_vertex_buffer(0, p.tri_buf.slice(..));
                    pass.draw(0..p.tri_count, 0..1);
                }
                if p.line_count > 0 {
                    pass.set_pipeline(&self.line_pipe);
                    pass.set_bind_group(1, &p.line_bind, &[]);
                    pass.set_vertex_buffer(0, p.line_buf.slice(..));
                    pass.draw(0..p.line_count, 0..1);
                }
            }
        }

        // ID pass (only when the camera, scene or viewport changed)
        if ids_needed {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ids"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.id_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.id_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.id_pipe);
            pass.set_bind_group(0, &self.globals_bind, &[]);
            for b in &self.bodies {
                pass.set_bind_group(1, &b.bind, &[]);
                pass.set_vertex_buffer(0, b.vbuf.slice(..));
                pass.set_index_buffer(b.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..b.index_count, 0, 0..1);
            }
        }

        // Pick readback (one pixel). Only one read-back may be in flight.
        if let (Some([x, y]), PickState::Idle) = (pick_at, &self.pick_state) {
            if x < t.size[0] && y < t.size[1] {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &t.id_tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x, y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &self.pick_buf,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(1),
                        },
                    },
                    wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                );
                self.pick_state = PickState::Copied;
            }
        }
    }

    /// Moves the pick read-back state machine forward. Call once per frame.
    fn advance_pick(&mut self, device: &wgpu::Device) {
        match &self.pick_state {
            PickState::Copied => {
                // The copy was submitted with the previous frame; request the mapping now.
                let flag = Arc::new(AtomicU32::new(MAP_PENDING));
                let f2 = flag.clone();
                self.pick_buf.slice(..4).map_async(wgpu::MapMode::Read, move |r| {
                    f2.store(if r.is_ok() { 0 } else { MAP_FAILED }, Ordering::SeqCst);
                });
                self.pick_state = PickState::Mapping(flag);
                let _ = device.poll(wgpu::PollType::Poll);
            }
            PickState::Mapping(flag) => {
                let _ = device.poll(wgpu::PollType::Poll);
                let v = flag.load(Ordering::SeqCst);
                if v == 0 {
                    {
                        if let Ok(data) = self.pick_buf.slice(..4).get_mapped_range() {
                            self.pick_result = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                        }
                    }
                    self.pick_buf.unmap();
                    self.pick_state = PickState::Idle;
                } else if v == MAP_FAILED {
                    self.pick_state = PickState::Idle;
                }
            }
            PickState::Idle => {}
        }
    }

    /// Latest completed pick (face ID under the requested pixel, or `ID_NONE`).
    pub fn pick_result(&self) -> u32 {
        self.pick_result
    }
    /// True while a read-back is in flight (the app should keep repainting).
    pub fn pick_busy(&self) -> bool {
        !matches!(self.pick_state, PickState::Idle)
    }

    /// Draws the offscreen colour into a host render pass (set its viewport first).
    pub fn blit(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let Some(t) = &self.target {
            pass.set_pipeline(&self.blit_pipe);
            pass.set_bind_group(0, &t.blit_bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Renders one frame and waits until the GPU has finished it (for benchmarks; no read-back).
    pub fn render_and_wait(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame,
    ) -> Result<(), RenderError> {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("bench") });
        self.render(device, queue, &mut enc, frame, None);
        queue.submit([enc.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map(|_| ())
            .map_err(|e| RenderError::Device(format!("{e:?}")))
    }

    /// Renders one frame headlessly and returns RGBA8 pixels (`size.x * size.y * 4`).
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame,
    ) -> Result<Vec<u8>, RenderError> {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("offscreen"),
        });
        self.render(device, queue, &mut enc, frame, None);
        let t = self.target.as_ref().expect("target");
        let [w, h] = t.size;
        let row = (w * 4).next_multiple_of(256);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Device(format!("{e:?}")))?;
        rx.recv()
            .map_err(|e| RenderError::Device(e.to_string()))?
            .map_err(|e| RenderError::Device(e.to_string()))?;
        let data = buf
            .slice(..)
            .get_mapped_range()
            .map_err(|e| RenderError::Device(e.to_string()))?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            out.extend_from_slice(&data[(y * row) as usize..(y * row + w * 4) as usize]);
        }
        drop(data);
        buf.unmap();
        Ok(out)
    }

    /// Reads back the whole ID buffer headlessly (for tests): one `u32` per pixel.
    pub fn read_ids(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &Frame,
    ) -> Result<Vec<u32>, RenderError> {
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("ids") });
        self.render(device, queue, &mut enc, frame, None);
        let t = self.target.as_ref().expect("target");
        let [w, h] = t.size;
        let row = (w * 4).next_multiple_of(256);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("id readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.id_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Device(format!("{e:?}")))?;
        rx.recv()
            .map_err(|e| RenderError::Device(e.to_string()))?
            .map_err(|e| RenderError::Device(e.to_string()))?;
        let data = buf
            .slice(..)
            .get_mapped_range()
            .map_err(|e| RenderError::Device(e.to_string()))?;
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let o = (y * row + x * 4) as usize;
                out.push(u32::from_le_bytes([
                    data[o],
                    data[o + 1],
                    data[o + 2],
                    data[o + 3],
                ]));
            }
        }
        drop(data);
        buf.unmap();
        Ok(out)
    }
}

/// Encodes RGBA8 pixels as PNG.
pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc
            .write_header()
            .map_err(|e| RenderError::Image(e.to_string()))?;
        wr.write_image_data(rgba)
            .map_err(|e| RenderError::Image(e.to_string()))?;
    }
    Ok(out)
}

#[allow(dead_code)]
fn _unused(_: Mat4) {}
