// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! `rolidium --bench-viewport`: 10 s of camera rotation over a one-million-triangle scene.

use super::*;

#[derive(Default)]
pub struct Bench {
    pub first_frame_ms: f64,
    frames: Vec<f32>,
    skipped: u32,
    started: Option<Instant>,
    size: [f32; 2],
}

impl App {
    pub(super) fn bench_tick(&mut self, ctx: &egui::Context) {
        let Some(mut b) = self.bench.take() else { return };
        ctx.request_repaint();
        let ppp = ctx.pixels_per_point();
        let r = ctx.content_rect();
        b.size = [r.width() * ppp, r.height() * ppp];
        if !self.first_frame_done {
            self.bench = Some(b);
            return;
        }
        if b.skipped < 30 {
            b.skipped += 1; // warm-up
            self.bench = Some(b);
            return;
        }
        let t0 = *b.started.get_or_insert_with(Instant::now);
        b.frames.push(self.frame_ms);
        self.camera.orbit(3.0, 0.0);
        if t0.elapsed().as_secs_f32() < 10.0 {
            self.bench = Some(b);
            return;
        }
        let mut f = b.frames.clone();
        f.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let n = f.len().max(1);
        let avg_ms = f.iter().sum::<f32>() / n as f32;
        let worst = &f[..(n / 100).max(1)];
        let worst_ms = worst.iter().sum::<f32>() / worst.len() as f32;
        let tris: usize = self.meshes.iter().map(|m| m.mesh.triangle_count()).sum();
        let text = format!(
            "Rolidium viewport benchmark\nadapter: {}\nbackend: {}\nresolution (whole window): {:.0}x{:.0}\ntriangles: {}\nframes: {}\naverage fps: {:.1}\nworst 1% frames: {:.1} fps ({:.1} ms)\ntime to first frame: {:.0} ms\n",
            self.adapter, self.backend, b.size[0], b.size[1], tris, n, 1000.0 / avg_ms, 1000.0 / worst_ms, worst_ms, b.first_frame_ms
        );
        print!("{text}");
        let _ = std::fs::write("rolidium-bench.txt", &text);
        crate::startup::finish();
        std::process::exit(0);
    }
}
