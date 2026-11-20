// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! egui <-> renderer glue: the renderer lives in egui_wgpu's callback resources; each frame a
//! paint callback renders the scene offscreen (prepare) and blits it into the egui pass (paint).

use crate::worker::MeshEntry;
use eframe::egui_wgpu::{self, CallbackTrait};
use rolidium_render::wgpu;
use rolidium_render::{BodyDraw, Frame, PreviewDraw, Renderer};
use std::sync::Arc;

pub struct Resources {
    pub renderer: Renderer,
}

pub struct ViewportCb {
    pub frame: Frame,
    pub pick_at: Option<[u32; 2]>,
    /// `Some` when the body meshes changed since the last frame.
    pub new_bodies: Option<Arc<Vec<MeshEntry>>>,
    /// `Some(x)` when the preview changed (`x == None` clears it).
    pub preview: Option<Option<PreviewDraw>>,
    /// `Some` when the sketch overlay layers changed.
    pub layers: Option<Vec<PreviewDraw>>,
}

impl CallbackTrait for ViewportCb {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(res) = resources.get_mut::<Resources>() {
            let r = &mut res.renderer;
            if let Some(b) = &self.new_bodies {
                let draws: Vec<BodyDraw> = b
                    .iter()
                    .map(|e| BodyDraw {
                        id: e.id,
                        mesh: &e.mesh,
                        color: e.color,
                        stamp: e.stamp,
                    })
                    .collect();
                let t = std::time::Instant::now();
                let made = r.set_bodies(device, &draws);
                crate::logging::log(&format!(
                    "scene on the graphics card: {} bodies, buffers made for {made}, {:.1} ms",
                    draws.len(),
                    t.elapsed().as_secs_f64() * 1e3
                ));
            }
            if let Some(l) = &self.layers {
                r.set_layers(device, l);
            }
            if let Some(p) = &self.preview {
                r.set_preview(device, p.as_ref());
            }
            r.render(device, queue, encoder, &self.frame, self.pick_at);
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: eframe::egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(res) = resources.get::<Resources>() {
            res.renderer.blit(pass);
        }
    }
}
