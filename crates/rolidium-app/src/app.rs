// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use crate::exportdialog::{DialogAction, ExportDialog};
use crate::graphics;
use crate::i18n::{self, parse_num, tr, trf};
use crate::icons::{self, Help};
use crate::logging::{self, log};
use crate::settings::Settings;
use crate::sketchmode::{SkTool, SketchMode, Snap, SnapLabel};
use crate::theme;
use crate::viewport::{Resources, ViewportCb};
use crate::worker::{Done, MeshEntry, StlParams, Work, Worker};
use eframe::egui::{self, Color32, Key, PointerButton, Pos2, Rect, Sense, Stroke, Vec2};
use eframe::egui_wgpu;
use glam::Vec3;
use rolidium_doc::rolidium_sketch::{DVec2, Entity, EntityId, PlaneDef, Region, SnapKind};
use rolidium_doc::{BodyId, Command, Document, ItemId, PullMode, SketchId, SketchTarget};
use rolidium_kernel::{DVec3, FaceId, FaceKind};
use rolidium_render::{
    decode_pick, encode_pick, face_boundary, fill_layer, grid_levels, pull_preview, region_geometry,
    region_pull_preview, sketch_lines, Camera, Frame, GridPlane, PreviewDraw, RegionGeom, StandardView,
    ID_NONE,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

mod bench;
mod busy;
mod combinetool;
mod curvetools;
mod dock;
mod edgeedit;
mod edgepull;
mod editbench;
mod feedback;
mod files;
mod filltool;
mod guide_ui;
mod jointool;
mod linktool;
mod lofttool;
mod measuretool;
mod movetool;
mod panel;
mod pick;
mod pickcache;
mod planeat;
mod planemove;
mod planepick;
mod probe;
mod properties;
mod remesh;
mod revolve;
mod selftest;
mod snap;
mod sources;
mod toolbar;
mod updates;
mod viewanim;
pub use bench::Bench;
use edgepull::EdgePrev;
use files::Pending;
use panel::DimPanel;
use pick::Obj;
use selftest::SelfTest;
use snap::{Candidate, SnapTarget, UpTo};

/// A signed distance for display, e.g. `+5` or `-2.5`.
fn fmt_signed(v: f64) -> String {
    let v = if v.abs() < 5e-4 { 0.0 } else { v };
    let s = fmt_num(v);
    if v > 0.0 {
        format!("+{s}")
    } else {
        s
    }
}

/// Tangent-chain results per (revision, body, selected edges, chamfer).
type ChainCache = std::collections::HashMap<(u64, BodyId, Vec<u32>, bool), Vec<u32>>;

/// What the right-click menu in the viewport applies to.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CtxTarget {
    Entity(SketchId, EntityId),
    Region(SketchId, usize),
    Body(BodyId),
    Edge(BodyId, u32),
}

/// Name of a base plane (XY, XZ, YZ) or a general one.
fn plane_name(p: &PlaneDef) -> &'static str {
    let n = p.normal;
    if n.z.abs() > 0.999 {
        "XY"
    } else if n.y.abs() > 0.999 {
        "XZ"
    } else if n.x.abs() > 0.999 {
        "YZ"
    } else {
        "?"
    }
}

/// A number for display: up to three decimals, no trailing zeros.
fn fmt_num(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Pull,
    Move,
    Combine,
    Loft,
    Sweep,
    Helix,
    Measure,
    Fill,
    Join,
    Pattern,
    Mirror,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PullKind {
    Face { body: BodyId, face: u32 },
    Region { sketch: SketchId, idx: usize },
    Edges { body: BodyId },
}

struct PullState {
    kind: PullKind,
    normal: Vec3,
    origin: DVec3,
    geom: Option<RegionGeom>,
    start: Pos2,
    /// Screen direction (unit) of the normal and pixels per millimetre along it.
    axis_dir: Vec2,
    px_per_mm: f32,
    d_mouse: f32,
    typed: String,
    dragging: bool,
    /// Edge indices for `PullKind::Edges`.
    edges: Vec<u32>,
    /// Planes and vertices the distance can snap to, and the one it is attached to now.
    cands: Vec<Candidate>,
    snap: Option<SnapTarget>,
}

/// "Up to": waiting for a click on the target the face should be pulled to.
struct UpToState {
    kind: PullKind,
    origin: DVec3,
    normal: Vec3,
}

impl PullState {
    fn distance(&self) -> f32 {
        parse_num(&self.typed)
            .map(|v| v as f32)
            .unwrap_or(if self.typed.is_empty() { self.d_mouse } else { 0.0 })
    }
}

struct Status {
    text: String,
    error: bool,
}

struct PendingPull {
    body: BodyId,
    origin: DVec3,
    normal: Vec3,
    distance: f64,
}

#[derive(PartialEq)]
enum Busy {
    No,
    Command,
    Meshes,
    Export,
    /// Opening, saving or reading a file.
    File,
}

/// Cached regions of one sketch with their 3D geometry.
/// What the cache of `edges_at_body_joint` was made for: the revision, the body and its edges.
type JointKey = (u64, BodyId, Vec<u32>);

struct SketchRegions {
    id: SketchId,
    plane: PlaneDef,
    regions: Vec<Region>,
    geoms: Vec<RegionGeom>,
}

/// A diagnostics zip being written on another thread: the result appears in the slot when it is done.
/// What the status bar shows about the graphics: (the name of the card, the label "software drawing"). In the presentation mode
/// (`ROLIDIUM_PRESENTATION=1`, a help for pictures of the window) both are left out; nothing else changes.
pub(crate) fn status_labels(presentation: bool, software: bool) -> (bool, bool) {
    (!presentation, software && !presentation)
}

/// Whether the environment asks for the presentation mode.
pub(crate) fn presentation_env() -> bool {
    std::env::var("ROLIDIUM_PRESENTATION").is_ok_and(|v| v == "1")
}

type DiagnosticsJob = std::sync::Arc<std::sync::Mutex<Option<Result<std::path::PathBuf, String>>>>;

pub struct App {
    doc: Document,
    worker: Worker,
    busy: Busy,
    pending: Option<PendingPull>,
    /// A command that edits a body made by a link, waiting for the answer: (command, link, link name, body name).
    derived_ask: Option<(Command, rolidium_doc::LinkId, String, String)>,
    /// The command to run once the link is removed.
    after_unlink: Option<Command>,
    meshes: Arc<Vec<MeshEntry>>,
    scene_radius_cache: (usize, f32),
    meshes_changed: bool,
    camera: Camera,
    tool: Tool,
    selection: Vec<(BodyId, u32)>,
    region_sel: Option<(SketchId, usize)>,
    hover_id: u32,
    hover_region: Option<(SketchId, usize)>,
    pull: Option<PullState>,
    preview: Option<PreviewDraw>,
    preview_changed: bool,
    sk: Option<SketchMode>,
    regions: Vec<SketchRegions>,
    regions_rev: u64,
    layers_changed: bool,
    /// How many vertices of curves the last uploaded layers hold (for the self-test: what is drawn must match the document).
    drawn_curve_vertices: usize,
    status: Status,
    adapter: String,
    adapter_software: bool,
    adapter_description: String,
    diagnostics_saved: Option<std::path::PathBuf>,
    /// What the status bar showed in the last frame: (the name of the graphics card, the label of software drawing). For the self test.
    status_shows: (bool, bool),
    /// The self test sets this instead of the environment variable.
    presentation_override: Option<bool>,
    diagnostics_job: Option<DiagnosticsJob>,
    diagnostics_error: Option<String>,
    started: Instant,
    first_frame_done: bool,
    exit_after_first_frame: bool,
    /// `--measure-start` with ROLIDIUM_MEASURE_VIEW set: frame times while the view turns (frames, durations, last frame).
    measure_view: Option<(usize, Vec<f64>, Instant)>,
    measure_started: Option<Instant>,
    first_frame_ms_value: f64,
    last_pointer: Option<Pos2>,
    render_state: egui_wgpu::RenderState,
    fitted: bool,
    selftest: Option<SelfTest>,
    preview_ms: f32,
    show_debug: bool,
    last_op_ms: Option<f32>,
    frame_ms: f32,
    /// Smoothed work of the window per frame in ms: the whole `ui`, the viewport, and the part of it that finds what is under the pointer.
    work_ms: [f32; 3],
    last_ui_ms: std::rc::Rc<std::cell::Cell<f32>>,
    last_work_log: Instant,
    fps_smooth: f32,
    /// Why the view asked for the next frame: the pointer moved, a pick read-back is running (for the diagnostic panel).
    repaint_why: (bool, bool, bool, bool),
    /// When the next frame may start (the software ceiling of the frame rate).
    frame_deadline: Option<Instant>,
    /// A pick read-back is due (the pointer or the picture changed since the last one).
    pick_dirty: bool,
    /// The number of read-backs the renderer had started at the last frame.
    pick_serial: u64,
    /// The size of the diagnostic panel in the last frame (to put it into the corner).
    diag_size: Vec2,
    last_frame: Instant,
    backend: String,
    bench: Option<Bench>,
    edit_bench: Option<editbench::EditBench>,
    viewport_rect: Rect,
    hover_obj: Option<Obj>,
    /// A hole or a pin that is being dragged on its face (Select tool), with its live distances to the edges.
    hole_drag: Option<edgeedit::HoleDrag>,
    /// The probe for the scripted window tests (`ROLIDIUM_PROBE`).
    probe: Option<probe::Probe>,
    entity_sel: Option<(SketchId, EntityId)>,
    edge_sel: Vec<(BodyId, u32)>,
    body_sel: Option<BodyId>,
    /// The body whose row the list on the right was last scrolled to.
    tree_seen: Option<BodyId>,
    update: updates::UpdateState,
    feedback: feedback::Feedback,
    /// The graphics adapter of the window (for the summary of the diagnostics).
    gpu: crate::diagnose::GpuFacts,
    /// For the self test: the row of the body picked last: (body, marked in the list, drawn inside the visible part of the list) and how far the list is scrolled.
    tree_probe: (Option<BodyId>, bool, bool),
    tree_scroll_y: f32,
    settings: Settings,
    export: ExportDialog,
    help: Help,
    dim: DimPanel,
    edge_chamfer: bool,
    edge_size_text: String,
    edge_prev: EdgePrev,
    body_override: Option<Arc<Vec<MeshEntry>>>,
    stats_ticket: u64,
    last_cam: Camera,
    last_size: [u32; 2],
    pull_mode: PullMode,
    up_to: Option<UpToState>,
    settings_open: bool,
    grid_text: String,
    last_error_tone: Instant,
    /// After editing a round or chamfer its face is selected again, so it can be edited again.
    reselect_blend: Option<BodyId>,
    sketch_sel: Option<SketchId>,
    move_sel: Vec<BodyId>,
    region_multi: Vec<(SketchId, usize)>,
    sel_order: Vec<rolidium_doc::LoftRef>,
    plane_move: Option<planemove::PlaneMove>,
    /// The tool Sketch plane (key N).
    plane_at: Option<planeat::PlaneAt>,
    docks: dock::Docks,
    loft: lofttool::LoftUi,
    line3: curvetools::Line3,
    helix: curvetools::HelixUi,
    sweep: curvetools::SweepUi,
    curve_cache: curvetools::CurveCache,
    curve_sel: Option<rolidium_doc::CurveId>,
    /// The fields of the helix in the Selection panel: the curve, the texts, and the numbers they were made from.
    curve_edit: (Option<rolidium_doc::CurveId>, Vec<String>, [f64; 5]),
    /// Which of height, pitch and turns the Selection panel calculates for a helix of a face (0 height, 1 pitch, 2 turns)
    curve_solve: u8,
    measure: measuretool::MeasureUi,
    view_anim: Option<viewanim::ViewAnim>,
    view_before_sketch: Option<rolidium_render::Camera>,
    /// Text of the density field while it is being typed, per body.
    density_text: std::collections::HashMap<BodyId, String>,
    busy_since: Option<Instant>,
    cancelling: bool,
    move_curves: Vec<rolidium_doc::CurveId>,
    about_open: bool,
    donate_idx: usize,
    about_scroll: bool,
    /// Height of the left column above the File block, and the height available to the column (for the size of the File icons).
    left_rest_h: f32,
    /// Where the File block was drawn this frame.
    left_file_rect: Rect,
    left_avail_h: f32,
    /// For the layout test: the column needed its scroll bar, where the Settings/About row was drawn, where the status bar starts.
    left_scrolls: bool,
    left_bottom_rect: Rect,
    status_top: f32,
    chamfer_angle_text: String,
    chamfer_flip: bool,
    /// Change all rounds of the same size that touch the selected one.
    blend_chain: bool,
    /// A new diameter of an end of a cone keeps the angle of the cone (the other end follows) instead of the diameter of the other end.
    /// The self-test of the frame budget lets the display meshes for the zoom be made (they are off in the other steps)
    selftest_remesh: bool,
    cap_keep_angle: bool,
    /// For the self-test: the state of the document before a heavy calculation.
    state_before_heavy: Option<Arc<rolidium_doc::State>>,
    /// The Move tool moves the end face of a loft: the body and the sketch of its profile (the sketch moves).
    move_end: Option<(BodyId, SketchId)>,
    combine: combinetool::CombineUi,
    link_ui: linktool::LinkUi,
    link_sel: Option<rolidium_doc::LinkId>,
    hover_body: Option<BodyId>,
    /// Where the last cycling click was, and which candidate it took.
    pick_cycle: Option<(Pos2, usize)>,
    revolve: Option<revolve::RevolveState>,
    plane_pick: bool,
    prop_cache: Option<(String, Vec<properties::Prop>)>,
    plane_hover: Option<planepick::PlaneHover>,
    /// A tangent plane chosen on a curved face: body, point, normal (taken by `enter_sketch_on`).
    tangent_plane: Option<(BodyId, DVec3, DVec3)>,
    keep_move_sel: Vec<BodyId>,
    move_ui: movetool::MoveUi,
    ctx_target: Option<CtxTarget>,
    rename: Option<(ItemId, String)>,
    chain_cache: std::cell::RefCell<ChainCache>,
    /// Whether the selected edges lie where another body touches (revision, body, edges) -> yes/no.
    /// Fill tool: sketch lines chosen so far, and whether a Fill command is running (its result starts the Pull tool).
    fill_entities: Vec<(SketchId, EntityId)>,
    fill_pending: bool,
    join: jointool::JoinUi,
    remesh: remesh::RemeshState,
    guide: crate::guide::Guide,
    /// Where the Pull button of the left column was drawn (the guide points at it).
    pull_btn_rect: Rect,
    joint_cache: std::cell::RefCell<Option<(JointKey, bool)>>,
    pick_cache: std::cell::RefCell<pickcache::PickCache>,
    file_path: Option<PathBuf>,
    file_dialog: Option<crate::filedialog::FileDialog>,
    egui_ctx: egui::Context,
    confirm: Option<Pending>,
    after_save: Option<Pending>,
    allow_close: bool,
    quit_now: bool,
    last_title: String,
    autosave_state: Option<Arc<rolidium_doc::State>>,
    autosave_inflight: bool,
    last_autosave: Instant,
    recovery: Option<files::Recovery>,
    /// Set while an autosave is being opened: the path of the document it came from.
    recovering: Option<Option<PathBuf>>,
    /// When the last restore of an autosave began (until the program has run on for a while after it).
    restore_started: Option<Instant>,
    /// After a start with another graphics backend than usual: which backends failed and which one runs now (shown once).
    startup_notice: Option<(Vec<String>, String)>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        started: Instant,
        selftest_dir: Option<PathBuf>,
        exit_after_first_frame: bool,
        bench_viewport: bool,
        open_path: Option<PathBuf>,
        demo: bool,
    ) -> Self {
        let rs = cc
            .wgpu_render_state
            .clone()
            .expect("the wgpu renderer is required");
        let renderer = rolidium_render::Renderer::new(&rs.device, rs.target_format);
        rs.renderer
            .write()
            .callback_resources
            .insert(Resources { renderer });
        let info = rs.adapter.get_info();
        let adapter = format!("{} ({:?})", info.name, info.device_type);
        logging::log(&format!(
            "adapter: {adapter}, backend {:?}, driver {} {}",
            info.backend, info.driver, info.driver_info
        ));
        graphics::install_error_logging(&rs.device);
        let adapter_software = graphics::is_software_adapter(&info);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx
            .all_styles_mut(|st| st.interaction.tooltip_delay = 0.5);
        let settings = Settings::load();
        rolidium_kernel::par::configure(settings.threads as usize);
        let guide = crate::guide::Guide::new(settings.guide_mask, settings.guide_enabled);
        i18n::set_language(&settings.language);
        theme::apply(&cc.egui_ctx, &settings.theme, settings.brightness);
        install_fallback_fonts(&cc.egui_ctx);
        let mut settings = settings;
        let (donate_idx, new_choice) = crate::support::chosen_index(&settings);
        if new_choice {
            settings.donate_index = Some(donate_idx);
            if selftest_dir.is_none() {
                settings.save();
            }
        }
        let mut app = App {
            doc: Document::new(),
            worker: Worker::spawn(cc.egui_ctx.clone()),
            busy: Busy::No,
            pending: None,
            meshes: Arc::new(Vec::new()),
            scene_radius_cache: (0, 0.0),
            meshes_changed: false,
            camera: Camera::default(),
            tool: Tool::Select,
            selection: Vec::new(),
            region_sel: None,
            hover_id: ID_NONE,
            hover_region: None,
            pull: None,
            preview: None,
            preview_changed: false,
            sk: None,
            regions: Vec::new(),
            regions_rev: u64::MAX,
            layers_changed: true,
            drawn_curve_vertices: 0,
            status: Status {
                text: tr("Ready"),
                error: false,
            },
            adapter,
            adapter_software,
            adapter_description: rolidium_render::describe_adapter(&rs.adapter),
            diagnostics_saved: None,
            status_shows: (true, false),
            presentation_override: None,
            diagnostics_job: None,
            diagnostics_error: None,
            started,
            first_frame_done: false,
            exit_after_first_frame,
            measure_view: None,
            measure_started: None,
            first_frame_ms_value: 0.0,
            last_pointer: None,
            render_state: rs,
            fitted: false,
            selftest: selftest_dir.map(SelfTest::new),
            preview_ms: 0.0,
            show_debug: false,
            last_op_ms: None,
            frame_ms: 0.0,
            work_ms: [0.0; 3],
            last_ui_ms: Default::default(),
            last_work_log: Instant::now(),
            fps_smooth: 0.0,
            repaint_why: (false, false, false, false),
            frame_deadline: None,
            pick_dirty: true,
            pick_serial: 0,
            diag_size: Vec2::new(300.0, 90.0),
            last_frame: Instant::now(),
            backend: format!("{:?}", info.backend),
            bench: None,
            edit_bench: None,
            viewport_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)),
            hover_obj: None,
            hole_drag: None,
            probe: probe::Probe::from_env(),
            entity_sel: None,
            edge_sel: Vec::new(),
            body_sel: None,
            tree_seen: None,
            update: Default::default(),
            feedback: Default::default(),
            gpu: crate::diagnose::GpuFacts::from_info(&info),
            tree_probe: (None, false, false),
            tree_scroll_y: 0.0,
            export: ExportDialog::new(&settings),
            settings,
            help: Help::load(),
            dim: DimPanel::default(),
            edge_chamfer: false,
            edge_size_text: "2".into(),
            edge_prev: EdgePrev::default(),
            body_override: None,
            stats_ticket: 0,
            last_cam: Camera::default(),
            last_size: [0, 0],
            pull_mode: PullMode::Auto,
            up_to: None,
            settings_open: false,
            grid_text: String::new(),
            last_error_tone: Instant::now() - std::time::Duration::from_secs(10),
            reselect_blend: None,
            sketch_sel: None,
            move_sel: Vec::new(),
            region_multi: Vec::new(),
            sel_order: Vec::new(),
            plane_move: None,
            plane_at: None,
            docks: Default::default(),
            loft: Default::default(),
            line3: Default::default(),
            helix: Default::default(),
            sweep: Default::default(),
            curve_cache: Default::default(),
            curve_sel: None,
            curve_edit: (None, Vec::new(), [0.0; 5]),
            curve_solve: 1,
            measure: Default::default(),
            view_anim: None,
            view_before_sketch: None,
            density_text: Default::default(),
            busy_since: None,
            cancelling: false,
            move_curves: Vec::new(),
            about_open: false,
            donate_idx,
            about_scroll: false,
            left_rest_h: 560.0,
            left_file_rect: Rect::NOTHING,
            left_avail_h: 0.0,
            left_scrolls: false,
            left_bottom_rect: Rect::NOTHING,
            status_top: f32::MAX,
            chamfer_angle_text: "45".into(),
            chamfer_flip: false,
            blend_chain: false,
            selftest_remesh: false,
            cap_keep_angle: false,
            state_before_heavy: None,
            move_end: None,
            combine: Default::default(),
            link_ui: Default::default(),
            link_sel: None,
            derived_ask: None,
            after_unlink: None,
            hover_body: None,
            pick_cycle: None,
            revolve: None,
            plane_pick: false,
            prop_cache: None,
            plane_hover: None,
            tangent_plane: None,
            keep_move_sel: Vec::new(),
            move_ui: Default::default(),
            ctx_target: None,
            rename: None,
            chain_cache: Default::default(),
            fill_entities: Vec::new(),
            fill_pending: false,
            join: Default::default(),
            remesh: Default::default(),
            guide,
            pull_btn_rect: Rect::NOTHING,
            joint_cache: Default::default(),
            pick_cache: Default::default(),
            file_path: None,
            file_dialog: None,
            egui_ctx: cc.egui_ctx.clone(),
            confirm: None,
            after_save: None,
            allow_close: false,
            quit_now: false,
            last_title: String::new(),
            autosave_state: None,
            autosave_inflight: false,
            last_autosave: Instant::now(),
            recovery: None,
            recovering: None,
            restore_started: None,
            startup_notice: crate::startup::take_notice(),
        };
        app.grid_text = fmt_num(app.settings.grid_step);
        // a normal window (not a self test or a benchmark): the session marker tells whether the last run ended properly
        if app.selftest.is_none()
            && !BENCH_RUN.load(std::sync::atomic::Ordering::Relaxed)
            && !exit_after_first_frame
            && !bench_viewport
            && crate::session::begin()
        {
            app.feedback.open_after_crash();
        }
        if bench_viewport {
            app.bench = Some(Bench::default());
            app.set_meshes(vec![crate::bench::million_triangle_entry()]);
            return app;
        }
        if demo || app.selftest.is_some() {
            // The demo scene: a 40 x 30 x 20 mm box made by the kernel.
            app.submit(Command::AddBox {
                name: String::new(),
                min: DVec3::ZERO,
                max: DVec3::new(40.0, 30.0, 20.0),
            });
        }
        if app.selftest.is_none() && std::env::var_os("ROLIDIUM_NO_RECOVERY").is_none() {
            app.recovery = files::find_recovery();
        }
        if let Some(p) = open_path {
            app.perform(Pending::Open(p));
        } else if !demo && app.selftest.is_none() && app.recovery.is_none() {
            // a new empty document: straight into a sketch, the isometric view shown for a moment first
            app.start_first_steps(guide_ui::FIRST_PAUSE);
        }
        app
    }

    /// Shows a message in the status bar. An error message also plays the soft error tone (not more
    /// often than twice a second, and never during self-tests).
    fn set_status(&mut self, text: impl Into<String>, error: bool) {
        let text = tr(&text.into());
        if error {
            log(&format!("error shown to user: {text}"));
            if self.settings.sound
                && self.selftest.is_none()
                && self.last_error_tone.elapsed().as_millis() > 500
            {
                self.last_error_tone = Instant::now();
                crate::sound::play_error(self.settings.volume);
            }
        }
        self.status = Status { text, error };
    }

    /// An error-coloured message without the tone (used for live previews that may fail often).
    fn set_status_quiet(&mut self, text: impl Into<String>) {
        self.status = Status {
            text: tr(&text.into()),
            error: true,
        };
    }

    fn idle(&self) -> bool {
        self.busy == Busy::No
    }

    fn submit(&mut self, command: Command) {
        if !self.idle() {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        let job = self.doc.plan(command);
        self.cancel_remesh();
        if self.worker.send(Work::Command(job)) {
            self.busy = Busy::Command;
        } else {
            self.set_status("The geometry worker has stopped; please restart Rolidium", true);
        }
    }

    fn request_meshes(&mut self) {
        let bodies = self
            .doc
            .bodies()
            .iter()
            .filter(|b| b.visible)
            .map(|b| (b.id, b.name.clone(), b.solid.clone(), b.color))
            .collect();
        if self.worker.send(Work::Meshes { bodies }) {
            self.busy = Busy::Meshes;
        }
    }

    fn poll_worker(&mut self) {
        while let Some((done, ms)) = self.worker.poll() {
            if matches!(done, Done::Command(_)) {
                self.last_op_ms = Some(ms);
            }
            match done {
                Done::Saved(r) => match r {
                    Ok((path, state, auto)) => {
                        if !auto {
                            self.busy = Busy::No;
                        }
                        self.saved(path, state, auto);
                    }
                    Err(msg) => {
                        self.autosave_inflight = false;
                        self.busy = Busy::No;
                        self.after_save = None;
                        self.set_status(msg, true);
                    }
                },
                Done::Opened(r) => {
                    self.busy = Busy::No;
                    match r {
                        Ok((path, loaded)) => {
                            let recovered = self.recovering.take();
                            self.opened(path, *loaded, recovered);
                        }
                        Err(msg) => {
                            self.recovering = None;
                            self.set_status(msg, true);
                        }
                    }
                }
                Done::StepRead(r) => {
                    self.busy = Busy::No;
                    match r {
                        Ok((solids, skipped)) => self.step_read(solids, skipped),
                        Err(msg) => self.set_status(msg, true),
                    }
                }
                Done::StepExported(r) => {
                    self.busy = Busy::No;
                    match r {
                        Ok(path) => self.set_status(trf("Exported {}", &[path.display().to_string()]), false),
                        Err(msg) => {
                            if !self.was_cancelled(&msg) {
                                self.set_status(msg, true);
                            }
                        }
                    }
                }
                Done::Failed(msg) => {
                    self.busy = Busy::No;
                    self.autosave_inflight = false;
                    self.set_status(msg, true);
                }
                Done::Command(Ok((computed, meshes))) => {
                    let touched = computed.touched_body;
                    let filled = if std::mem::take(&mut self.fill_pending) {
                        computed.created_sketch
                    } else {
                        None
                    };
                    let notice = computed.notice.clone();
                    match self.doc.commit(computed) {
                        Ok(()) => {
                            self.set_meshes(meshes);
                            self.after_commit(touched);
                            if let Some(n) = notice {
                                self.set_status(n, true);
                            }
                        }
                        Err(e) => self.set_status(e.to_string(), true),
                    }
                    self.pending = None;
                    self.busy = Busy::No;
                    if let Some(sk) = filled {
                        self.fill_after(sk);
                    }
                    if let Some(next) = self.after_unlink.take() {
                        self.submit(next);
                    }
                }
                Done::Derived(b) => {
                    let (cmd, link, link_name, body_name) = *b;
                    self.fill_pending = false;
                    self.reselect_blend = None;
                    self.pending = None;
                    self.busy = Busy::No;
                    self.derived_ask = Some((cmd, link, link_name, body_name));
                }
                Done::Command(Err(msg)) => {
                    self.after_unlink = None;
                    self.fill_pending = false;
                    self.reselect_blend = None;
                    self.pending = None;
                    self.busy = Busy::No;
                    if !self.was_cancelled(&msg) {
                        self.set_status(msg, true);
                    }
                }
                Done::Meshes(Ok(meshes)) => {
                    self.set_meshes(meshes);
                    self.busy = Busy::No;
                }
                Done::Meshes(Err(msg)) => {
                    self.busy = Busy::No;
                    if !self.was_cancelled(&msg) {
                        self.set_status(msg, true);
                    }
                }
                Done::Export(Ok((path, tris))) => {
                    self.busy = Busy::No;
                    self.set_status(format!("Exported {} ({tris} triangles)", path.display()), false);
                }
                Done::Export(Err(msg)) => {
                    self.busy = Busy::No;
                    if !self.was_cancelled(&msg) {
                        self.set_status(msg, true);
                    }
                }
                Done::MeshStats(ticket, r) => {
                    if ticket == self.stats_ticket {
                        if let (Ok(n), Some(p)) = (r, self.export.stats_requested) {
                            self.export.stats = Some((p, n));
                        }
                    }
                }
                Done::EdgePreview(ticket, r) => self.edge_preview_done(ticket, r),
                Done::Remesh(ticket, r) => self.remesh_done(ticket, r),
            }
        }
    }

    fn set_meshes(&mut self, meshes: Vec<MeshEntry>) {
        self.meshes = Arc::new(meshes);
        self.meshes_changed = true;
        self.hover_id = ID_NONE;
        if !self.fitted && !self.meshes.is_empty() {
            self.fit_all();
            self.fitted = true;
        }
    }

    /// Radius of the sphere around all bodies (cached while the meshes stay the same).
    fn scene_radius(&mut self) -> f32 {
        let key = Arc::as_ptr(&self.meshes) as usize;
        if self.scene_radius_cache.0 != key {
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            scene_bounds(&self.meshes, &mut lo, &mut hi);
            let r = if lo.x <= hi.x {
                (hi - lo).length() * 0.5
            } else {
                0.0
            };
            self.scene_radius_cache = (key, r);
        }
        self.scene_radius_cache.1
    }

    fn fit_all(&mut self) {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        scene_bounds(&self.meshes, &mut lo, &mut hi);
        if lo.x <= hi.x {
            let mut to = self.camera;
            to.fit(lo, hi);
            self.animate_to(to);
        }
    }

    /// Re-selects the pulled face after a successful face pull so the user can keep pulling it.
    fn after_commit(&mut self, _touched: Option<BodyId>) {
        self.clear_selection();
        self.hover_region = None;
        if let Some(p) = self.pending.take() {
            let n = p.normal.as_dvec3();
            for m in self.meshes.iter().filter(|m| m.id == p.body) {
                let found = m.faces.iter().position(|f| {
                    f.kind == FaceKind::Plane
                        && f.normal.dot(n) > 0.999
                        && ((f.origin - p.origin).dot(n) - p.distance).abs() < 1e-3
                });
                if let Some(i) = found {
                    self.selection.push((p.body, i as u32));
                }
            }
            self.set_status(format!("Pull {:+.2} mm done", p.distance), false);
        } else {
            self.set_status("Done", false);
        }
        if self.tool == Tool::Move {
            let keep = std::mem::take(&mut self.keep_move_sel);
            self.move_sel = keep.into_iter().filter(|b| self.doc.body(*b).is_some()).collect();
        }
        if let Some(b) = self.reselect_blend.take() {
            let face = self
                .doc
                .body(b)
                .and_then(|bd| bd.features.last())
                .and_then(|f| f.faces.first().copied());
            if let Some(f) = face {
                self.selection = vec![(b, f.0)];
            }
        }
        // sketch mode: the active sketch may have been used up
        if let Some(sm) = &mut self.sk {
            if sm.sketch.is_some_and(|id| self.doc.sketch(id).is_none()) {
                sm.sketch = None;
            }
        }
    }

    fn undo(&mut self) {
        if !self.idle() {
            return;
        }
        if self.doc.undo() {
            self.after_history_jump("Undo");
        } else {
            self.set_status("Nothing to undo", false);
        }
    }

    fn redo(&mut self) {
        if !self.idle() {
            return;
        }
        if self.doc.redo() {
            self.after_history_jump("Redo");
        } else {
            self.set_status("Nothing to redo", false);
        }
    }

    fn after_history_jump(&mut self, what: &str) {
        self.clear_selection();
        self.clear_edge_preview();
        self.hover_region = None;
        self.pull = None;
        self.clear_preview();
        if let Some(sm) = &mut self.sk {
            sm.drawing = None;
            if sm.sketch.is_some_and(|id| self.doc.sketch(id).is_none()) {
                sm.sketch = None;
            }
        }
        // curves, sketches and bodies are drawn from the document as it is now, not as it was a frame ago
        self.curve_cache.defs.clear();
        self.layers_changed = true;
        self.request_meshes();
        self.set_status(what, false);
    }

    fn clear_preview(&mut self) {
        if self.preview.is_some() {
            self.preview = None;
            self.preview_changed = true;
        }
    }

    fn entry(&self, body: BodyId) -> Option<&MeshEntry> {
        self.meshes.iter().find(|m| m.id == body)
    }

    fn slot_of(&self, body: BodyId) -> Option<u32> {
        self.meshes.iter().position(|m| m.id == body).map(|i| i as u32)
    }

    fn selected_ids(&self) -> Vec<u32> {
        self.selection
            .iter()
            .filter_map(|(b, f)| self.slot_of(*b).map(|s| encode_pick(s, *f)))
            .collect()
    }

    /// Whole bodies to tint in the 3D view. A tool that works on a whole body shows the whole body, never one face of it.
    fn body_marks(&self) -> Vec<(u32, u32)> {
        marks_for(
            self.tool,
            &self.move_sel,
            self.move_ui.drag.is_some(),
            self.hovered_face().map(|(b, _)| b),
            self.combine.target,
            &self.combine.tools,
            self.body_sel,
            |b| self.slot_of(b),
        )
    }

    fn is_blend_face(&self, body: BodyId, face: u32) -> bool {
        self.doc
            .body(body)
            .is_some_and(|b| b.features.iter().any(|f| f.faces.contains(&FaceId(face))))
    }

    fn hovered_face(&self) -> Option<(BodyId, u32)> {
        let (slot, face) = decode_pick(self.hover_id)?;
        self.meshes.get(slot as usize).map(|m| (m.id, face))
    }

    // ---- sketch regions cache and overlay layers ----

    fn refresh_regions(&mut self) {
        if self.regions_rev == self.doc.revision() {
            return;
        }
        self.regions_rev = self.doc.revision();
        self.regions = self
            .doc
            .sketches()
            .iter()
            .filter(|s| s.visible)
            .map(|s| {
                let regions = s.sketch.regions();
                let geoms = regions.iter().map(|r| region_geometry(&s.plane, r)).collect();
                SketchRegions {
                    id: s.id,
                    plane: s.plane,
                    regions,
                    geoms,
                }
            })
            .collect();
        if self
            .region_sel
            .is_some_and(|(s, i)| !self.regions.iter().any(|r| r.id == s && i < r.regions.len()))
        {
            self.region_sel = None;
        }
        self.layers_changed = true;
    }

    fn geom_of(&self, sketch: SketchId, idx: usize) -> Option<&RegionGeom> {
        self.regions
            .iter()
            .find(|r| r.id == sketch)
            .and_then(|r| r.geoms.get(idx))
    }

    fn build_layers(&self) -> Vec<PreviewDraw> {
        let all: Vec<&RegionGeom> = self.regions.iter().flat_map(|r| r.geoms.iter()).collect();
        let mut layers = vec![fill_layer(&all, [0.35, 0.65, 1.0, 0.16])];
        if let Some((s, i)) = self.hover_region {
            if let Some(g) = self.geom_of(s, i) {
                layers.push(fill_layer(&[g], [1.0, 0.75, 0.30, 0.42]));
            }
        }
        if let Some((s, i)) = self.region_sel {
            if let Some(g) = self.geom_of(s, i) {
                layers.push(fill_layer(&[g], [0.25, 0.55, 1.0, 0.50]));
            }
        }
        let mut lines = Vec::new();
        for s in self.doc.sketches().iter().filter(|s| s.visible) {
            lines.extend(sketch_lines(&s.plane, &s.sketch));
        }
        layers.push(PreviewDraw {
            triangles: Vec::new(),
            lines,
            color: [0.0; 4],
            line_color: [1.0, 0.85, 0.35, 1.0],
            ..Default::default()
        });
        // thick highlights of hovered / selected edges and sketch shapes
        let pairs = |pts: &[Vec3]| -> Vec<Vec3> { pts.windows(2).flat_map(|w| [w[0], w[1]]).collect() };
        let obj_lines = |app: &App, o: Obj| -> Vec<Vec3> {
            match o {
                Obj::Edge(b, e) => app.edge_polyline(b, e).map(pairs).unwrap_or_default(),
                Obj::Curve(c) => pairs(app.curve_points(c)),
                Obj::Entity(s, id) => app
                    .doc
                    .sketch(s)
                    .and_then(|sk| sk.sketch.entity(id).map(|e| (sk.plane, *e)))
                    .map(|(pl, e)| {
                        e.polylines()
                            .iter()
                            .flat_map(|poly| {
                                let p3: Vec<Vec3> = poly.iter().map(|p| pl.to_3d(*p).as_vec3()).collect();
                                pairs(&p3)
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        };
        let mut sel_lines: Vec<Vec3> = Vec::new();
        for (b, e) in &self.edge_sel {
            sel_lines.extend(obj_lines(self, Obj::Edge(*b, *e)));
        }
        if let Some((s, e)) = self.entity_sel {
            sel_lines.extend(obj_lines(self, Obj::Entity(s, e)));
        }
        // Edges that would be rounded or chamfered too because they continue a selected one tangentially.
        let mut extra: Vec<Vec3> = Vec::new();
        let mut bodies: Vec<BodyId> = self.edge_sel.iter().map(|(b, _)| *b).collect();
        bodies.dedup();
        for b in bodies {
            for e in self.chain_extra(
                b,
                &self
                    .edge_sel
                    .iter()
                    .filter(|(bb, _)| *bb == b)
                    .map(|(_, e)| *e)
                    .collect::<Vec<_>>(),
            ) {
                extra.extend(obj_lines(self, Obj::Edge(b, e)));
            }
        }
        if let Some(Obj::Edge(b, e)) = self.hover_obj {
            for x in self.chain_extra(b, &[e]) {
                extra.extend(obj_lines(self, Obj::Edge(b, x)));
            }
        }
        if !extra.is_empty() {
            layers.push(PreviewDraw {
                thick: extra,
                thick_color: [0.85, 0.5, 1.0, 1.0],
                ..Default::default()
            });
        }
        if !sel_lines.is_empty() {
            layers.push(PreviewDraw {
                thick: sel_lines,
                thick_color: [0.25, 0.55, 1.0, 1.0],
                ..Default::default()
            });
        }
        if let Some(SnapTarget::Face { body, face }) = self.pull.as_ref().and_then(|p| p.snap.clone()) {
            if let Some(m) = self.entry(body) {
                let (segs, _) = face_boundary(&m.mesh, face);
                let lines: Vec<Vec3> = segs.iter().flat_map(|(a, b)| [*a, *b]).collect();
                if !lines.is_empty() {
                    layers.push(PreviewDraw {
                        thick: lines,
                        thick_color: [0.47, 1.0, 0.63, 1.0],
                        ..Default::default()
                    });
                }
            }
        }
        layers.extend(self.combine_layers());
        layers.extend(self.link_layers());
        layers.extend(self.hole_drag_layers());
        layers.extend(self.loft_layers());
        layers.extend(self.curve_layers());
        if let Some(l) = self.revolve_layer() {
            layers.push(l);
        }
        if self.tool == Tool::Move && (!self.move_sel.is_empty() || !self.move_curves.is_empty()) {
            let boxl = self.move_preview_lines();
            if self.move_ui.drag.is_some() {
                layers.push(PreviewDraw {
                    lines: boxl,
                    line_color: [1.0, 0.9, 0.4, 1.0],
                    ..Default::default()
                });
            } else {
                layers.push(PreviewDraw {
                    thick: boxl,
                    thick_color: [0.25, 0.55, 1.0, 1.0],
                    ..Default::default()
                });
            }
        }
        if let Some(o) = self.hover_obj {
            let l = obj_lines(self, o);
            if !l.is_empty() {
                layers.push(PreviewDraw {
                    thick: l,
                    thick_color: [1.0, 0.72, 0.25, 1.0],
                    ..Default::default()
                });
            }
        }
        layers
    }

    /// Nearest region under a ray; a body face that is clearly in front of it wins.
    fn pick_region(&self, o: Vec3, d: Vec3, face: Option<(BodyId, u32)>) -> Option<(SketchId, usize)> {
        let (o, d) = (o.as_dvec3(), d.as_dvec3());
        let mut best: Option<(f64, SketchId, usize)> = None;
        for sr in &self.regions {
            let Some((p, t)) = sr.plane.ray_hit(o, d) else {
                continue;
            };
            for (i, r) in sr.regions.iter().enumerate() {
                if r.contains(p) && best.is_none_or(|(bt, _, _)| t < bt) {
                    best = Some((t, sr.id, i));
                }
            }
        }
        let (t, s, i) = best?;
        if let Some((b, f)) = face {
            if let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)) {
                if meta.kind == FaceKind::Plane {
                    let den = d.dot(meta.normal);
                    if den.abs() > 1e-9 {
                        let tf = (meta.origin - o).dot(meta.normal) / den;
                        if tf > 0.0 && tf < t - 1e-3 {
                            return None;
                        }
                    }
                }
            }
        }
        Some((s, i))
    }

    // ---- Pull tool ----

    /// Maps mouse movement to a distance along the normal at `centre`.
    fn axis_mapping(&self, centre: Vec3, normal: Vec3, rect: Rect) -> (Vec2, f32) {
        let aspect = rect.width() / rect.height().max(1.0);
        let vp = self.camera.view_proj(aspect);
        let to_px = |p: Vec3| -> Option<Vec2> {
            let c = vp * p.extend(1.0);
            if c.w <= 1e-6 {
                return None;
            }
            Some(Vec2::new(
                (c.x / c.w * 0.5 + 0.5) * rect.width(),
                (1.0 - (c.y / c.w * 0.5 + 0.5)) * rect.height(),
            ))
        };
        let len = 10.0;
        let axis = match (to_px(centre), to_px(centre + normal * len)) {
            (Some(a), Some(b)) => (b - a) / len,
            _ => Vec2::ZERO,
        };
        let fallback = rect.height() / (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan());
        if axis.length() * len > 8.0 {
            (axis.normalized(), axis.length())
        } else {
            (Vec2::new(0.0, -1.0), fallback)
        }
    }

    fn begin_pull(&mut self, kind: PullKind, pointer: Pos2, rect: Rect) -> bool {
        let (normal, centre, origin, geom) = match kind {
            PullKind::Face { body, face } => {
                let Some(m) = self.entry(body) else { return false };
                let Some(meta) = m.faces.get(face as usize) else {
                    return false;
                };
                if meta.kind != FaceKind::Plane {
                    self.set_status("Pull works on planar faces in this version", true);
                    return false;
                }
                (
                    meta.normal.as_vec3().normalize(),
                    meta.centroid,
                    meta.origin,
                    None,
                )
            }
            PullKind::Edges { .. } => return false,
            PullKind::Region { sketch, idx } => {
                let Some(sr) = self.regions.iter().find(|r| r.id == sketch) else {
                    return false;
                };
                let Some(g) = sr.geoms.get(idx) else { return false };
                (
                    sr.plane.normal.as_vec3(),
                    g.centroid,
                    sr.plane.origin,
                    Some(g.clone()),
                )
            }
        };
        let (axis_dir, px_per_mm) = self.axis_mapping(centre, normal, rect);
        let cands = snap::candidates(&self.meshes, origin, normal.as_dvec3());
        self.pull = Some(PullState {
            kind,
            normal,
            origin,
            geom,
            start: pointer,
            axis_dir,
            px_per_mm,
            d_mouse: 0.0,
            typed: String::new(),
            dragging: false,
            edges: Vec::new(),
            cands,
            snap: None,
        });
        match kind {
            PullKind::Face { body, face } => {
                self.selection = vec![(body, face)];
                self.region_sel = None;
            }
            PullKind::Region { sketch, idx } => {
                self.region_sel = Some((sketch, idx));
                self.selection.clear();
            }
            PullKind::Edges { .. } => {}
        }
        self.entity_sel = None;
        self.edge_sel.clear();
        self.layers_changed = true;
        true
    }

    fn update_pull_preview(&mut self) {
        let Some(p) = &self.pull else { return };
        let t = Instant::now();
        let d = self.effective_distance(p.kind, p.distance());
        match (&p.kind, &p.geom) {
            (PullKind::Face { body, face }, _) => {
                if let Some(m) = self.entry(*body) {
                    self.preview = Some(pull_preview(&m.mesh, *face, p.normal, d));
                    self.preview_changed = true;
                }
            }
            (PullKind::Region { .. }, Some(g)) => {
                self.preview = Some(region_pull_preview(g, p.normal, d));
                self.preview_changed = true;
            }
            (PullKind::Edges { .. }, _) => self.request_edge_preview(),
            _ => {}
        }
        self.preview_ms = t.elapsed().as_secs_f32() * 1000.0;
    }

    /// The distance a pull really moves by, after the Add / Cut mode forced a direction.
    fn effective_distance(&self, kind: PullKind, d: f32) -> f32 {
        match kind {
            PullKind::Face { .. } => self.pull_mode.apply(d as f64) as f32,
            PullKind::Region { sketch, .. } => {
                if self.doc.sketch(sketch).is_some_and(|s| s.target.is_some()) {
                    self.pull_mode.apply(d as f64) as f32
                } else {
                    d
                }
            }
            PullKind::Edges { .. } => d,
        }
    }

    fn commit_pull(&mut self) {
        let Some(p) = self.pull.take() else { return };
        let d = p.distance() as f64;
        self.clear_preview();
        self.clear_edge_preview();
        if let PullKind::Edges { body } = p.kind {
            if d.abs() < 1e-3 {
                self.set_status("Pull cancelled (zero distance)", false);
                return;
            }
            if !self.idle() {
                self.set_status("Still working on the previous operation", true);
                return;
            }
            self.pending = None;
            let angle = parse_num(&self.chamfer_angle_text).unwrap_or(45.0);
            if self.edge_chamfer && ((angle - 45.0).abs() > 1e-9 || self.chamfer_flip) {
                self.submit(Command::ChamferAngled {
                    body,
                    edges: p.edges.clone(),
                    distance: d.abs(),
                    angle_deg: angle,
                    flip: self.chamfer_flip,
                });
            } else {
                self.submit(Command::FilletEdges {
                    body,
                    edges: p.edges.clone(),
                    size: d.abs(),
                    chamfer: self.edge_chamfer,
                });
            }
            self.set_status("Computing…", false);
            return;
        }
        self.submit_pull(p.kind, p.origin, p.normal, d, self.pull_mode);
    }

    /// Starts a pull of a face or region by `distance` (signed; see [`PullMode`]).
    fn submit_pull(&mut self, kind: PullKind, origin: DVec3, normal: Vec3, distance: f64, mode: PullMode) {
        if distance.abs() < 1e-3 {
            self.set_status("Pull cancelled (zero distance)", false);
            return;
        }
        if !self.idle() {
            self.set_status("Still working on the previous operation", true);
            return;
        }
        match kind {
            PullKind::Face { body, face } => {
                // a face of a body is pulled in Auto, Add or Cut (New body belongs to the regions of a sketch)
                let mode = if mode == PullMode::New {
                    PullMode::Auto
                } else {
                    mode
                };
                self.pending = Some(PendingPull {
                    body,
                    origin,
                    normal,
                    distance: mode.apply(distance),
                });
                self.submit(Command::PullFace {
                    body,
                    face: FaceId(face),
                    distance,
                    mode,
                });
            }
            PullKind::Region { sketch, idx } => {
                self.pending = None;
                self.submit(Command::PullRegion {
                    sketch,
                    region: idx,
                    distance,
                    mode,
                });
            }
            PullKind::Edges { .. } => return,
        }
        self.set_status("Computing…", false);
    }

    // ---- "Up to" ----

    /// Asks for a target (face, edge, vertex or point); the selected face or region is pulled to it.
    fn begin_up_to(&mut self) {
        if !self.idle() {
            return;
        }
        let state = if let Some((s, i)) = self.region_sel {
            self.regions.iter().find(|r| r.id == s).and_then(|sr| {
                (i < sr.regions.len()).then(|| UpToState {
                    kind: PullKind::Region { sketch: s, idx: i },
                    origin: sr.plane.origin,
                    normal: sr.plane.normal.as_vec3(),
                })
            })
        } else if let [(b, f)] = self.selection[..] {
            self.entry(b)
                .and_then(|m| m.faces.get(f as usize))
                .filter(|m| m.kind == FaceKind::Plane)
                .map(|m| UpToState {
                    kind: PullKind::Face { body: b, face: f },
                    origin: m.origin,
                    normal: m.normal.as_vec3().normalize(),
                })
        } else {
            None
        };
        match state {
            Some(st) => {
                self.up_to = Some(st);
                self.set_status(
                    "Up to: click a face, an edge, a vertex or a point on a face (Esc cancels)",
                    false,
                );
            }
            None => self.set_status("Select a flat face or a sketch region first", true),
        }
    }

    /// What the cursor is over for "up to": a vertex, else an edge, else a face.
    pub(super) fn up_to_target(&self, pointer: Pos2, rect: Rect) -> Option<UpTo> {
        // vertices (ends of body edges) within a few pixels
        let mut best: Option<(f32, Vec3)> = None;
        for m in self.meshes.iter() {
            for e in &m.mesh.edges {
                for p in [e.points.first(), e.points.last()].into_iter().flatten() {
                    if let Some(sp) = self.project(*p, rect) {
                        let d = (sp - pointer).length();
                        if d <= 10.0 && best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, *p));
                        }
                    }
                }
            }
        }
        if let Some((_, p)) = best {
            if !self.point_hidden(p) {
                return Some(UpTo::Point(p.as_dvec3()));
            }
        }
        if let Some(Obj::Edge(b, e)) = self.pick_object(pointer, rect, true, false) {
            let pl = self.edge_polyline(b, e)?;
            return Some(UpTo::Edge(pl.first()?.as_dvec3(), pl.last()?.as_dvec3()));
        }
        let (b, f) = self.hovered_face()?;
        let meta = self.entry(b)?.faces.get(f as usize)?;
        let normal = self.up_to.as_ref().map(|u| u.normal).or_else(|| {
            self.plane_move
                .as_ref()
                .and_then(|_| self.sk.as_ref().map(|s| s.base_plane.normal.as_vec3()))
        })?;
        if meta.kind == FaceKind::Plane && meta.normal.dot(normal.as_dvec3()).abs() > 0.9999 {
            return Some(UpTo::Plane(meta.origin));
        }
        let aspect = rect.width() / rect.height().max(1.0);
        let (o, d) = self.camera.ray(Self::ndc(pointer, rect), aspect);
        let hit = self
            .ray_face_point(b, f, o, d)
            .unwrap_or(meta.centroid.as_dvec3());
        Some(UpTo::Point(hit))
    }

    /// The pointer clicked while "up to" waits for its target.
    fn finish_up_to(&mut self, pointer: Pos2, rect: Rect) {
        let Some(target) = self.up_to_target(pointer, rect) else {
            self.set_status("Click on the model: a face, an edge or a vertex", false);
            return;
        };
        let Some(up) = self.up_to.take() else { return };
        let n = up.normal.as_dvec3();
        let raw = snap::up_to_distance(up.origin, n, target);
        // Edge and vertex points are single precision: use the exact plane of a face at that height.
        let planes: Vec<DVec3> = self
            .meshes
            .iter()
            .flat_map(|m| m.faces.iter())
            .filter(|f| f.kind == FaceKind::Plane && f.normal.dot(n).abs() > 0.9999)
            .map(|f| f.origin)
            .collect();
        let d = snap::align_to_planes(raw, up.origin, n, planes);
        if d.abs() < 1e-3 {
            self.set_status("The face is already there", false);
            return;
        }
        self.submit_pull(up.kind, up.origin, up.normal, d, PullMode::Auto);
    }

    fn cancel_pull(&mut self) {
        if self.up_to.take().is_some() {
            self.set_status("Cancelled", false);
        }
        if self.pull.take().is_some() {
            self.clear_preview();
            self.clear_edge_preview();
            self.set_status("Pull cancelled", false);
        }
    }

    /// Opens the export dialog.
    fn export_stl(&mut self) {
        if self.doc.bodies().is_empty() {
            self.set_status("Nothing to export", true);
            return;
        }
        self.export.stats = None;
        self.export.stats_requested = None;
        self.export.open = true;
    }

    fn visible_solids(&self) -> Vec<rolidium_kernel::Solid> {
        self.doc
            .bodies()
            .iter()
            .filter(|b| b.visible)
            .map(|b| b.solid.clone())
            .collect()
    }

    fn export_to(&mut self, path: PathBuf, params: StlParams) {
        if !self.idle() {
            return;
        }
        let solids = self.visible_solids();
        if self.worker.send(Work::ExportStl { solids, path, params }) {
            self.busy = Busy::Export;
            self.set_status("Exporting STL…", false);
        }
    }

    fn handle_export_dialog(&mut self, ctx: &egui::Context) {
        let busy = !self.idle();
        let recent = self.settings.recent_dirs.clone();
        match self.export.ui(ctx, busy, &recent) {
            DialogAction::None => {}
            DialogAction::NeedStats(p) => {
                self.stats_ticket += 1;
                let solids = self.visible_solids();
                let _ = self.worker.send(Work::MeshStats {
                    solids,
                    params: p,
                    ticket: self.stats_ticket,
                });
            }
            DialogAction::Export(path, params) => {
                self.settings = self.export.settings(&self.settings);
                let d = self.settings.export_dir.clone();
                self.settings.remember_dir(&d);
                self.settings.save();
                self.export.open = false;
                self.export_to(path, params);
            }
        }
        if !self.export.open {
            let s = self.export.settings(&self.settings);
            if s != self.settings {
                self.settings = s;
                self.settings.save();
            }
        }
    }

    // ---- sketch mode ----

    /// K: a sketch on the selected flat face, else a choice of the base plane.
    fn enter_sketch(&mut self) {
        if !self.idle() {
            return;
        }
        let flat = if let [(b, f)] = self.selection[..] {
            self.entry(b)
                .and_then(|m| m.faces.get(f as usize))
                .is_some_and(|m| m.kind == FaceKind::Plane)
        } else {
            false
        };
        if flat {
            self.enter_sketch_on(PlaneDef::base());
        } else {
            self.plane_pick = true;
            self.set_status("Choose a plane or a face for the sketch", false);
        }
    }

    fn enter_sketch_on(&mut self, base: PlaneDef) {
        self.plane_pick = false;
        self.cancel_pull();
        let mut plane = base;
        let mut body = None;
        let mut outline = Vec::new();
        let mut corners = Vec::new();
        if let [(b, f)] = self.selection[..] {
            if let Some(m) = self.entry(b) {
                if let Some(meta) = m.faces.get(f as usize) {
                    if meta.kind == FaceKind::Plane {
                        plane = PlaneDef::from_face(meta.origin, meta.normal);
                        body = Some(b);
                        let (segs, cs) = face_boundary(&m.mesh, f);
                        let p2 = |v: Vec3| plane.to_2d(v.as_dvec3());
                        outline = segs.iter().map(|(a, b)| (p2(*a), p2(*b))).collect();
                        corners = cs.iter().map(|v| p2(*v)).collect();
                    }
                }
            }
        }
        let existing = self
            .doc
            .sketches()
            .iter()
            .find(|s| s.target == body && s.plane.same_plane(&plane))
            .map(|s| s.id);
        let tangent = self.tangent_plane.take();
        if let Some((b, point, normal)) = tangent {
            // tangent to the curved face, with the origin at the clicked point
            plane = planepick::tangent_plane_def(point, normal);
            body = Some(b);
        }
        let existing = if tangent.is_some() { None } else { existing };
        let prev_sel = self.selection.clone();
        let mut sm = SketchMode::new(plane, body, existing);
        if let Some((_, point, _)) = tangent {
            sm.focus = point.as_vec3();
        }
        sm.outline = outline;
        if let [(b, f)] = prev_sel[..] {
            if let Some(solid) = self.doc.body(b).map(|x| &x.solid) {
                if let Ok(es) = solid.face_edges(FaceId(f)) {
                    sm.straight = es
                        .into_iter()
                        .filter_map(|e| solid.edge_info(e).ok())
                        .filter(|i| i.kind == rolidium_kernel::EdgeKind::Line)
                        .map(|i| (plane.to_2d(i.start), plane.to_2d(i.end)))
                        .collect();
                }
            }
        }
        if let [(b, f)] = prev_sel[..] {
            if let Some(meta) = self.entry(b).and_then(|m| m.faces.get(f as usize)) {
                if body.is_some() {
                    sm.focus = meta.centroid;
                }
            }
        }
        sm.corners = corners;
        self.sk = Some(sm);
        self.turn_to_sketch();
        self.selection.clear();
        self.region_sel = None;
        self.hover_region = None;
        self.layers_changed = true;
        self.set_status(
            if body.is_some() {
                tr("Sketch on the selected face")
            } else {
                trf("Sketch on the base plane ({})", &[plane_name(&plane).to_string()])
            },
            false,
        );
    }

    fn leave_sketch(&mut self) {
        if self.sk.take().is_some() {
            self.return_from_sketch();
            self.set_status("Left sketch mode", false);
        }
    }

    fn view_normal_to_sketch(&mut self) {
        let Some(sm) = &self.sk else { return };
        let mut to = viewanim::plane_view(&self.camera, sm.plane.normal.as_vec3());
        to.target = sm.focus;
        self.animate_to(to);
    }

    fn add_entity(&mut self, e: Entity) -> bool {
        let Some(sm) = &self.sk else { return false };
        let target = match sm.sketch {
            Some(id) if self.doc.sketch(id).is_some() => SketchTarget::Existing(id),
            _ => match &sm.anchor {
                Some(anchor) => SketchTarget::Anchored {
                    anchor: anchor.clone(),
                },
                None => SketchTarget::New {
                    plane: sm.plane,
                    body: sm.body,
                },
            },
        };
        match self.doc.apply(Command::AddEntity { target, entity: e }) {
            Ok(c) => {
                if let (Some(id), Some(sm)) = (c.created_sketch, self.sk.as_mut()) {
                    sm.sketch = Some(id);
                }
                self.layers_changed = true;
                true
            }
            Err(err) => {
                self.set_status(err.to_string(), true);
                false
            }
        }
    }

    /// Finishes the current drawing with the shape under the mouse and the typed values.
    fn finish_drawing(&mut self) {
        let Some(sm) = &self.sk else { return };
        let Some(mouse) = sm.snap.map(|s| s.pos) else {
            return;
        };
        let Some(e) = sm.effective(mouse) else { return };
        let tool = sm.tool;
        if tool == SkTool::Arc && sm.arc_end.is_none() {
            // the end point of the arc is placed; the next click gives the point on the arc
            if let Some(sm) = &mut self.sk {
                if mouse.distance(sm.drawing.as_ref().map_or(mouse, |d| d.start)) > 1e-6 {
                    sm.arc_end = Some(mouse);
                    for f in sm.drawing.iter_mut().flat_map(|d| d.fields.iter_mut()) {
                        f.text.clear();
                    }
                }
            }
            return;
        }
        if self.add_entity(e) {
            if let Some(sm) = &mut self.sk {
                sm.drawing = None;
                if let (SkTool::Line, Entity::Line { b, .. }) = (tool, e) {
                    sm.begin(b); // chain: the next line starts where this one ended
                }
                if let (
                    SkTool::Arc,
                    Entity::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    },
                ) = (tool, e)
                {
                    let end = rolidium_doc::rolidium_sketch::Segment::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    }
                    .end_point();
                    sm.begin(end); // chain: the next arc or line continues from here
                }
            }
        }
    }

    fn snap_points(&self) -> Vec<(DVec2, SnapKind)> {
        let Some(sm) = &self.sk else { return Vec::new() };
        let mut v = Vec::new();
        for s in self.doc.sketches() {
            if s.plane.same_plane(&sm.plane) && s.target == sm.body {
                for (_, e) in s.sketch.entities() {
                    v.extend(e.snap_points());
                }
            }
        }
        v
    }

    fn project(&self, p: Vec3, rect: Rect) -> Option<Pos2> {
        let aspect = rect.width() / rect.height().max(1.0);
        let c = self.camera.view_proj(aspect) * p.extend(1.0);
        if c.w <= 1e-6 {
            return None;
        }
        Some(Pos2::new(
            rect.min.x + (c.x / c.w * 0.5 + 0.5) * rect.width(),
            rect.min.y + (1.0 - (c.y / c.w * 0.5 + 0.5)) * rect.height(),
        ))
    }

    /// Minor and major grid spacing for the user's step at the current zoom.
    fn grid_levels(&self, rect: Rect) -> (f32, f32) {
        let px_per_mm = rect.height() / (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan());
        grid_levels(self.settings.grid_step as f32, px_per_mm)
    }

    fn mm_per_px(&self, plane: &PlaneDef, p: DVec2, rect: Rect) -> f64 {
        let p3 = plane.to_3d(p).as_vec3();
        let q3 = plane.to_3d(p + DVec2::X).as_vec3();
        if let (Some(a), Some(b)) = (self.project(p3, rect), self.project(q3, rect)) {
            let px = (a - b).length() as f64;
            if px > 1e-3 {
                return 1.0 / px;
            }
        }
        (2.0 * self.camera.distance * (self.camera.fov_y * 0.5).tan() / rect.height().max(1.0)) as f64
    }

    // ---- input ----

    fn handle_keys(&mut self, ctx: &egui::Context, rect: Rect) {
        // the file dialogs read the keyboard themselves
        if self.file_dialog.is_some() || self.export.open {
            return;
        }
        // While a text field has the keyboard, only the file shortcuts (Ctrl+N, O, S, Shift+S) still work: they do not
        // mean anything to a text field, and they must not depend on where the focus happens to be.
        if ctx.egui_wants_keyboard_input() {
            let events = ctx.input(|i| i.events.clone());
            for ev in events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = ev
                {
                    if modifiers.command && matches!(key, Key::N | Key::O | Key::S) {
                        self.key_pressed(key, modifiers, rect);
                    }
                }
            }
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => self.key_pressed(key, modifiers, rect),
                egui::Event::Text(t) => {
                    for ch in t.chars() {
                        self.typed_char(ch, rect);
                    }
                }
                _ => {}
            }
        }
    }

    fn key_pressed(&mut self, key: Key, modifiers: egui::Modifiers, rect: Rect) {
        if key == Key::Escape && self.can_cancel() && !self.idle() {
            return self.cancel_computation();
        }
        // History works everywhere.
        match key {
            Key::Z if modifiers.command && modifiers.shift => return self.redo(),
            Key::Z if modifiers.command => return self.undo(),
            Key::Y if modifiers.command => return self.redo(),
            Key::G if modifiers.command => return self.group_marked(),
            Key::N if modifiers.command => return self.request(Pending::New),
            Key::O if modifiers.command => return self.request(Pending::OpenDialog),
            Key::S if modifiers.command && modifiers.shift => return self.save_clicked(true),
            Key::S if modifiers.command => return self.save_clicked(false),
            _ => {}
        }
        if modifiers.command {
            return;
        }
        if self.tool == Tool::Pattern && self.link_ui.pick == Some(linktool::LinkPick::Point) {
            match key {
                Key::Enter if self.confirm_second_typed() => return,
                Key::Backspace if self.link_ui.second_edit.backspace() => {
                    self.layers_changed = true;
                    return;
                }
                Key::Escape if self.link_ui.second_edit.on => {
                    self.link_ui.second_edit.clear();
                    self.layers_changed = true;
                    return;
                }
                _ => {}
            }
        }
        if self.hole_drag.is_some() {
            match key {
                Key::Escape => self.cancel_hole_drag(),
                Key::Enter => self.commit_hole_drag(),
                Key::Backspace => {
                    if let Some(h) = &mut self.hole_drag {
                        h.edit.backspace();
                        h.update(h.cur, 0.0);
                    }
                    self.layers_changed = true;
                }
                _ => {}
            }
            return;
        }
        if key == Key::Delete {
            return self.delete_selected();
        }
        if key == Key::F2 {
            // F2 renames the selected body
            if let Some(b) = self.body_sel.and_then(|b| self.doc.body(b)) {
                self.rename = Some((ItemId::Body(b.id), b.name.clone()));
            } else if let Some(l) = self
                .link_sel
                .and_then(|id| self.doc.state().links.iter().find(|l| l.id == id))
            {
                self.rename = Some((ItemId::Link(l.id), l.name.clone()));
            } else if let Some(g) = self.doc.state().groups.iter().find(|g| {
                !g.bodies.is_empty()
                    && g.bodies.len() == self.move_sel.len()
                    && g.bodies.iter().all(|b| self.move_sel.contains(b))
            }) {
                self.rename = Some((ItemId::Group(g.id), g.name.clone()));
            }
            return;
        }
        // while the tool Sketch plane is active, Esc and Enter belong to it (see `planeat.rs`) and no other key works
        if self.plane_at.is_some() {
            return;
        }
        if self.sk.is_some() && self.line3_key(key) {
            return;
        }
        if self.sk.is_some() && self.plane_move.is_some() {
            match key {
                Key::Escape => self.cancel_plane_move(),
                Key::Enter => self.finish_plane_move(),
                Key::U => {
                    if let Some(pm) = &mut self.plane_move {
                        pm.waiting_target = true;
                    }
                    self.set_status("Up to: click a face, an edge or a vertex", false);
                }
                Key::Backspace => {
                    if let Some(pm) = &mut self.plane_move {
                        pm.typed.pop();
                    }
                }
                _ => {}
            }
            return;
        }
        if self.sk.is_some() {
            match key {
                Key::L => self.sk.as_mut().unwrap().set_tool(SkTool::Line),
                Key::R => self.sk.as_mut().unwrap().set_tool(SkTool::Rect),
                Key::C => self.sk.as_mut().unwrap().set_tool(SkTool::Circle),
                Key::A => self.sk.as_mut().unwrap().set_tool(SkTool::Arc),
                Key::X => self.sk.as_mut().unwrap().set_tool(SkTool::Axis),
                Key::V => self.view_normal_to_sketch(),
                Key::N => self.begin_plane_at(),
                // K finishes the sketch (as the main button does); D and P still work
                Key::K => self.leave_sketch(),
                Key::D | Key::S => {
                    if key == Key::S {
                        self.tool = Tool::Select;
                    }
                    self.leave_sketch();
                }
                Key::P => {
                    self.tool = Tool::Pull;
                    self.leave_sketch();
                }
                Key::Escape => {
                    if let Some(sm) = &mut self.sk {
                        sm.drawing = None;
                        sm.place = None;
                        sm.edge_edit.clear();
                    }
                    self.entity_sel = None;
                    self.layers_changed = true;
                }
                Key::Enter
                    if self
                        .sk
                        .as_ref()
                        .is_some_and(|sm| sm.edge_typing() && sm.edge_edit.any_typed()) =>
                {
                    // the circle is placed where the typed distances to the edges put its centre
                    if let Some(sm) = &mut self.sk {
                        if let Some(s) = sm.snap {
                            sm.begin(s.pos);
                        }
                    }
                }
                Key::Enter => {
                    let place = self.sk.as_ref().and_then(|sm| {
                        if sm.drawing.is_none() && sm.place.is_some() {
                            sm.snap.and_then(|s| sm.place_target(s.pos))
                        } else {
                            None
                        }
                    });
                    if let Some(t) = place {
                        if let Some(sm) = &mut self.sk {
                            sm.place = None;
                            sm.begin(t);
                        }
                    } else {
                        self.finish_drawing();
                    }
                }
                Key::Backspace => {
                    if let Some(sm) = &mut self.sk {
                        sm.backspace();
                    }
                }
                _ => {}
            }
            return;
        }
        match key {
            Key::Escape => {
                if self.revolve.is_some() {
                    self.cancel_revolve();
                } else if self.move_ui.drag.is_some() {
                    self.cancel_move();
                } else if self.pull.is_some() || self.up_to.is_some() {
                    self.cancel_pull();
                } else if self.measure_escape() || self.join_cancel() {
                } else {
                    self.clear_selection();
                }
            }
            Key::Enter => {
                if self.revolve.as_ref().is_some_and(|r| r.axis.is_some()) {
                    self.commit_revolve();
                } else if self.tool == Tool::Loft && self.loft.profiles.len() >= 2 {
                    self.apply_loft();
                } else if self.tool == Tool::Combine && self.combine.target.is_some() {
                    self.apply_combine();
                } else if self.move_ui.drag.is_some() {
                    self.commit_move();
                } else if self.pull.is_some() {
                    self.commit_pull();
                }
            }
            Key::Backspace => {
                if let Some(d) = self.move_ui.drag.as_mut() {
                    d.pop_typed();
                    self.layers_changed = true;
                } else if let Some(p) = &mut self.pull {
                    p.typed.pop();
                    self.update_pull_preview();
                }
            }
            Key::S => self.tool = Tool::Select,
            Key::P => self.tool = Tool::Pull,
            Key::M => self.tool = Tool::Move,
            Key::I => self.tool = Tool::Combine,
            Key::T => self.activate_link_tool(linktool::LinkMode::Linear),
            Key::B => self.activate_link_tool(linktool::LinkMode::Mirror),
            Key::L => self.activate_loft(),
            Key::W => self.activate_sweep(),
            Key::H => self.activate_helix(),
            Key::K => self.enter_sketch(),
            Key::N => self.begin_plane_at(),
            Key::U => self.begin_up_to(),
            Key::E => self.activate_measure(),
            Key::F => self.activate_fill(),
            Key::J => self.activate_join(),
            _ => {}
        }
        let _ = rect;
    }

    fn delete_selected(&mut self) {
        if !self.idle() {
            return;
        }
        let cmd = if let Some((sketch, entity)) = self.entity_sel {
            Command::DeleteEntity { sketch, entity }
        } else if let Some((sketch, region)) = self.region_sel {
            Command::DeleteRegionShapes { sketch, region }
        } else if let Some(sketch) = self.sketch_sel {
            Command::DeleteSketch { sketch }
        } else if let Some(curve) = self.curve_sel {
            Command::DeleteCurve { curve }
        } else if let Some(body) = self.body_sel {
            Command::DeleteBody { body }
        } else {
            return;
        };
        self.run_doc_command(cmd, "Deleted (Ctrl+Z brings it back)");
    }

    /// Applies a command at once (the quick ones: delete, hide, rename) and refreshes what depends on it.
    fn run_doc_command(&mut self, cmd: Command, ok_text: &str) {
        if !self.idle() {
            return;
        }
        let bodies_change = matches!(
            cmd,
            Command::DeleteBody { .. }
                | Command::SetVisible {
                    target: ItemId::Body(_) | ItemId::Link(_) | ItemId::Group(_),
                    ..
                }
                | Command::DeleteLink { .. }
        );
        // a command that is made here and not on the worker can still change bodies (a curve edited, the sweeps along it follow): their
        // meshes are made again whatever the command was
        let before: Vec<(BodyId, rolidium_kernel::Solid)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.id, b.solid.clone()))
            .collect();
        match self.doc.apply(cmd) {
            Ok(_) => {
                let after = self.doc.bodies();
                let bodies_change = bodies_change
                    || before.len() != after.len()
                    || before
                        .iter()
                        .zip(after)
                        .any(|((id, s), b)| *id != b.id || !s.is_same_solid(&b.solid));
                self.clear_selection();
                if let Some(sm) = &mut self.sk {
                    sm.drawing = None;
                    if sm.sketch.is_some_and(|id| self.doc.sketch(id).is_none()) {
                        sm.sketch = None;
                    }
                }
                self.layers_changed = true;
                if bodies_change {
                    self.request_meshes();
                }
                self.set_status(ok_text, false);
            }
            Err(e) => self.set_status(e.to_string(), true),
        }
    }

    /// The item under the pointer, for the right-click menu.
    fn context_target(&self, hovered_face: Option<(BodyId, u32)>) -> Option<CtxTarget> {
        match (self.hover_obj, self.hover_region, hovered_face) {
            (Some(Obj::Entity(s, e)), _, _) => Some(CtxTarget::Entity(s, e)),
            (Some(Obj::Edge(b, e)), _, _) => Some(CtxTarget::Edge(b, e)),
            (None, Some((s, i)), _) => Some(CtxTarget::Region(s, i)),
            (None, None, Some((b, _))) => Some(CtxTarget::Body(b)),
            _ => None,
        }
    }

    fn context_menu_ui(&mut self, ui: &mut egui::Ui) {
        let Some(t) = self.ctx_target else {
            ui.close();
            return;
        };
        let mut chosen: Option<(Command, &str)> = None;
        match t {
            CtxTarget::Entity(sketch, entity) => {
                if ui.button(tr("Delete")).clicked() {
                    chosen = Some((
                        Command::DeleteEntity { sketch, entity },
                        "Deleted (Ctrl+Z brings it back)",
                    ));
                }
            }
            CtxTarget::Region(sketch, region) => {
                if ui.button(tr("Delete")).clicked() {
                    chosen = Some((
                        Command::DeleteRegionShapes { sketch, region },
                        "Deleted (Ctrl+Z brings it back)",
                    ));
                }
                if ui.button(tr("Delete sketch")).clicked() {
                    chosen = Some((
                        Command::DeleteSketch { sketch },
                        "Deleted (Ctrl+Z brings it back)",
                    ));
                }
            }
            CtxTarget::Body(body) => {
                if ui.button(tr("Delete body")).clicked() {
                    chosen = Some((Command::DeleteBody { body }, "Deleted (Ctrl+Z brings it back)"));
                }
                if ui.button(tr("Hide")).clicked() {
                    chosen = Some((
                        Command::SetVisible {
                            target: ItemId::Body(body),
                            visible: false,
                        },
                        "Hidden",
                    ));
                }
            }
            CtxTarget::Edge(body, edge) => {
                for (chamfer, name) in [(false, "Round"), (true, "Chamfer")] {
                    if ui.button(tr(name)).clicked() {
                        self.clear_selection();
                        self.edge_sel = vec![(body, edge)];
                        self.edge_chamfer = chamfer;
                        self.tool = Tool::Pull;
                        self.layers_changed = true;
                        self.set_status("Type the size and press Enter, or drag the edge", false);
                        ui.close();
                    }
                }
            }
        }
        if let Some((cmd, text)) = chosen {
            ui.close();
            self.run_doc_command(cmd, text);
        }
    }

    /// About, with the "Support development" section (also what the Donate button opens).
    #[allow(deprecated)]
    /// Starts writing the diagnostics zip to the Documents folder (in the background).
    fn start_save_diagnostics(&mut self, ctx: &egui::Context) {
        let job = std::sync::Arc::new(std::sync::Mutex::new(None));
        let slot = job.clone();
        let (desc, backend, stext, summary) = (
            self.adapter_description.clone(),
            self.settings.backend.clone(),
            self.settings.diagnostic_text(),
            self.summary_for(ctx),
        );
        let c = ctx.clone();
        std::thread::spawn(move || {
            let path = crate::diagnose::default_path();
            let r = crate::diagnose::write_zip(&path, Some(&desc), &backend, &stext, &summary).map(|()| path);
            if let Ok(mut s) = slot.lock() {
                *s = Some(r);
            }
            c.request_repaint();
        });
        self.diagnostics_job = Some(job);
        self.diagnostics_error = None;
    }

    fn presentation(&self) -> bool {
        self.presentation_override.unwrap_or_else(presentation_env)
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        let mut restart_guide = false;
        let addr = crate::support::ADDRESSES[self.donate_idx.min(2)];
        let scroll = std::mem::take(&mut self.about_scroll);
        egui::Window::new(tr("About"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                ui.label(egui::RichText::new("Rolidium").strong().size(20.0));
                ui.label(tr("Direct modeling CAD (Alpha - developer version)"));
                ui.label(trf("Version {}", &[crate::version_string()]));
                ui.label(trf("Developed by {}", &["Martin Vorel".to_string()]));
                ui.label(trf("Published by {}", &["Rolid spol. s r.o. (connect@rolid.com)".to_string()]));
                ui.label(tr("Licence: AGPL-3.0-only. Files you create belong to you."));
                if ui.button(tr("Run the guide again")).clicked() {
                    restart_guide = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Web")).on_hover_text(rolidium_net::urls::WEB).clicked() {
                        crate::diagnose::open_path(std::path::Path::new(rolidium_net::urls::WEB));
                    }
                    if ui
                        .button(tr("Support forum"))
                        .on_hover_text(rolidium_net::urls::FORUM)
                        .clicked()
                    {
                        crate::diagnose::open_path(std::path::Path::new(rolidium_net::urls::FORUM));
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(tr("Feedback and bug reports")).clicked() {
                        crate::diagnose::open_path(std::path::Path::new(crate::ISSUES_URL));
                    }
                    if ui.button(tr("Third-party licences")).clicked() {
                        match crate::third_party_licenses_path() {
                            Some(p) => crate::diagnose::open_path(&p),
                            None => crate::diagnose::open_path(std::path::Path::new(crate::SOURCE_URL)),
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let busy = self.diagnostics_job.is_some();
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr("Save diagnostics")))
                        .on_hover_text(tr("Saves a zip with the graphics adapters, the log and test pictures, to send with a bug report. It contains no personal paths."))
                        .clicked()
                    {
                        self.start_save_diagnostics(ctx);
                    }
                    if busy {
                        ui.spinner();
                        if ui.button(tr("Cancel")).clicked() {
                            crate::diagnose::CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                });
                if let Some(job) = &self.diagnostics_job {
                    let done = job.lock().ok().and_then(|mut s| s.take());
                    if let Some(r) = done {
                        match r {
                            Ok(p) => self.diagnostics_saved = Some(p),
                            Err(e) if e == "cancelled" => {}
                            Err(e) => self.diagnostics_error = Some(e),
                        }
                        self.diagnostics_job = None;
                    }
                }
                if let Some(p) = self.diagnostics_saved.clone() {
                    ui.label(trf("Saved to {}", &[p.display().to_string()]));
                    if ui.button(tr("Open the folder")).clicked() {
                        if let Some(d) = p.parent() {
                            crate::diagnose::open_folder(d);
                        }
                    }
                }
                if let Some(e) = &self.diagnostics_error {
                    ui.colored_label(crate::theme::rgb(255, 120, 110), trf("Cannot save the diagnostics: {}", std::slice::from_ref(e)));
                }
                ui.separator();
                let head = ui.label(egui::RichText::new(tr("Support development")).strong());
                if scroll {
                    head.scroll_to_me(Some(egui::Align::TOP));
                }
                ui.label(tr("Rolidium is developed in spare time. If it is useful to you, you can support it with Bitcoin."));
                ui.add(egui::Label::new(egui::RichText::new(addr).monospace()).selectable(true));
                if let Some((w, modules)) = crate::support::qr_modules(addr) {
                    let px = 6.0;
                    let size = (w as f32 + 8.0) * px;
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(rect, 0.0, Color32::WHITE);
                    for y in 0..w {
                        for x in 0..w {
                            if modules[y * w + x] {
                                let p = rect.min + Vec2::new((x as f32 + 4.0) * px, (y as f32 + 4.0) * px);
                                painter.rect_filled(Rect::from_min_size(p, Vec2::splat(px)), 0.0, Color32::BLACK);
                            }
                        }
                    }
                }
                if ui.button(tr("Copy")).clicked() {
                    ctx.copy_text(addr.to_string());
                }
                ui.weak(tr("Nothing is sent anywhere; this is just an address."));
            });
        if !open {
            self.about_open = false;
        }
        if restart_guide {
            self.about_open = false;
            self.restart_guide();
        }
    }

    /// K without a flat face selected: which base plane to sketch on.
    #[allow(deprecated)]
    fn plane_window(&mut self, ctx: &egui::Context) {
        if !self.plane_pick {
            return;
        }
        let mut choice: Option<PlaneDef> = None;
        let mut cancel = false;
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            choice = Some(PlaneDef::base());
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        egui::Window::new(tr("Sketch plane"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(tr("Choose the plane to sketch on (Enter = XY)"));
                ui.horizontal(|ui| {
                    if ui.button("XY").clicked() {
                        choice = Some(PlaneDef::base());
                    }
                    if ui.button("XZ").clicked() {
                        choice = Some(PlaneDef::xz());
                    }
                    if ui.button("YZ").clicked() {
                        choice = Some(PlaneDef::yz());
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if let Some(p) = choice {
            self.enter_sketch_on(p);
        } else if cancel {
            self.plane_pick = false;
        }
    }

    #[allow(deprecated)]
    fn rename_window(&mut self, ctx: &egui::Context) {
        let Some((target, text)) = &mut self.rename else {
            return;
        };
        let target = *target;
        let mut done: Option<bool> = None;
        egui::Window::new(tr("Rename"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let r = ui.add(egui::TextEdit::singleline(text).desired_width(220.0));
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = Some(true);
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("OK")).clicked() {
                        done = Some(true);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        done = Some(false);
                    }
                });
            });
        match done {
            Some(true) => {
                let name = text.clone();
                self.rename = None;
                self.run_doc_command(Command::Rename { target, name }, "Renamed");
            }
            Some(false) => self.rename = None,
            None => {}
        }
    }

    fn typed_char(&mut self, ch: char, rect: Rect) {
        if let Some(h) = &mut self.hole_drag {
            h.edit.type_char(ch);
            h.update(h.cur, 0.0);
            self.layers_changed = true;
            return;
        }
        if self.tool == Tool::Pattern
            && self.link_ui.pick == Some(linktool::LinkPick::Point)
            && self.link_ui.second.is_some()
        {
            self.link_ui.second_edit.type_char(ch);
            self.layers_changed = true;
            return;
        }
        if self.line3_typed(ch) {
            return;
        }
        if let Some(pm) = &mut self.plane_move {
            let ch = if ch == ',' { '.' } else { ch };
            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && pm.typed.is_empty()) {
                pm.typed.push(ch);
                if let (Some(v), Some(sm)) = (parse_num(&pm.typed), self.sk.as_mut()) {
                    sm.set_offset(v);
                }
            }
            return;
        }
        if let Some(sm) = &mut self.sk {
            sm.type_char(ch);
            return;
        }
        if !(ch.is_ascii_digit() || ch == '.' || ch == ',' || ch == '-') {
            return;
        }
        let ch = if ch == ',' { '.' } else { ch };
        if let Some(rv) = self.revolve.as_mut() {
            if rv.axis.is_some() {
                rv.text.push(ch);
                self.update_revolve_preview();
            }
            return;
        }
        if self.tool == Tool::Move {
            if let Some(d) = self.move_ui.drag.as_mut() {
                d.push_typed(ch);
                self.layers_changed = true;
            }
            return;
        }
        if self.pull.is_none() && self.tool == Tool::Pull && self.idle() {
            // Typing a number with a face or region selected starts a numeric pull.
            if !self.edge_sel.is_empty() {
                let body = self.edge_sel[0].0;
                let edges: Vec<u32> = self
                    .edge_sel
                    .iter()
                    .filter(|(b, _)| *b == body)
                    .map(|(_, e)| *e)
                    .collect();
                self.begin_edge_pull(
                    body,
                    edges,
                    rect.center() - rect.min.to_vec2(),
                    Rect::from_min_size(Pos2::ZERO, rect.size()),
                );
            }
            let kind = if self.pull.is_some() {
                None
            } else if let Some((s, i)) = self.region_sel {
                Some(PullKind::Region { sketch: s, idx: i })
            } else {
                self.selection
                    .first()
                    .map(|&(body, face)| PullKind::Face { body, face })
            };
            if let Some(kind) = kind {
                self.begin_pull(
                    kind,
                    rect.center() - rect.min.to_vec2(),
                    Rect::from_min_size(Pos2::ZERO, rect.size()),
                );
            }
        }
        if let Some(p) = &mut self.pull {
            if ch == '-' && !p.typed.is_empty() {
                return;
            }
            p.typed.push(ch);
            self.update_pull_preview();
        }
    }

    fn ndc(p: Pos2, rect: Rect) -> glam::Vec2 {
        glam::Vec2::new(
            (p.x - rect.min.x) / rect.width() * 2.0 - 1.0,
            1.0 - (p.y - rect.min.y) / rect.height() * 2.0,
        )
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let t0 = Instant::now();
        self.viewport_inner(ui);
        let ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.work_ms[1] = self.work_ms[1] * 0.9 + ms * 0.1;
    }

    fn viewport_inner(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        self.viewport_rect = rect;
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let aspect = rect.width() / rect.height().max(1.0);
        self.refresh_regions();

        // Camera control
        let (shift, scroll) = ctx.input(|i| (i.modifiers.shift, i.smooth_scroll_delta.y));
        let pointer = resp.hover_pos().or_else(|| ctx.input(|i| i.pointer.latest_pos()));
        // the wheel goes through the planes proposed at a point while the tool Sketch plane points at one
        let wheel_taken = self
            .plane_at
            .as_ref()
            .is_some_and(|p| p.hover.is_some() && p.free.is_none());
        if resp.hovered() && scroll.abs() > 0.0 && !wheel_taken {
            if let Some(p) = pointer {
                self.camera
                    .zoom_at(Self::ndc(p, rect), aspect, (scroll * 0.0025).exp());
            }
        }
        let delta = resp.drag_delta();
        if resp.dragged_by(PointerButton::Middle) || resp.dragged_by(PointerButton::Secondary) {
            if shift {
                self.camera.pan(delta.x, delta.y, rect.height());
            } else {
                self.camera.orbit(delta.x, delta.y);
            }
        }

        let ray = pointer.map(|p| self.camera.ray(Self::ndc(p, rect), aspect));
        let mut hovered_face = if resp.hovered() { self.hovered_face() } else { None };
        let prev_hover_region = self.hover_region;
        let prev_obj = self.hover_obj;
        let t_pointer = Instant::now();
        self.hover_obj = match (resp.hovered(), pointer, self.pull.is_some()) {
            (true, Some(p), false) => {
                let entities = self.sk.as_ref().is_none_or(|s| s.tool == SkTool::None);
                let hit = self.pick_object_d(p, rect, self.sk.is_none(), entities);
                // A narrow round or chamfer face must stay clickable: unless the pointer is right on the
                // edge, the face under it wins.
                match (hit, hovered_face) {
                    (Some((Obj::Edge(..), d)), Some((b, f))) if d > 2.5 && self.is_blend_face(b, f) => None,
                    (h, _) => h.map(|(o, _)| o),
                }
            }
            (_, _, true) => self.hover_obj,
            _ => None,
        };
        // Tools that work with regions take the region under the pointer even right at its outline, where the outline
        // (a sketch shape) would otherwise win.
        let region_tool = match self.tool {
            Tool::Loft | Tool::Pull | Tool::Combine => true,
            Tool::Sweep => self.sweep.profile.is_none(),
            _ => false,
        };
        if region_tool
            && self.sk.is_none()
            && self.pull.is_none()
            && matches!(self.hover_obj, Some(Obj::Entity(..)))
        {
            if let Some((o, d)) = ray {
                if self.pick_region(o, d, hovered_face).is_some() {
                    self.hover_obj = None;
                }
            }
        }
        {
            let ms = t_pointer.elapsed().as_secs_f32() * 1000.0;
            self.work_ms[2] = self.work_ms[2] * 0.9 + ms * 0.1;
        }
        if prev_obj != self.hover_obj {
            self.layers_changed = true;
        }
        // choosing the sketch plane with the mouse also works before any sketch exists
        if self.plane_at.is_some() {
            self.hover_region = None;
            self.plane_at_input(&resp, pointer, rect, scroll);
        } else if self.sk.is_some() || self.plane_pick {
            self.hover_region = None;
            self.sketch_input(&resp, pointer, ray, rect);
        } else {
            self.hover_region = match (resp.hovered(), ray) {
                (true, Some((o, d))) if self.pull.is_none() && self.tool != Tool::Measure => {
                    self.pick_region(o, d, hovered_face)
                }
                _ => self.hover_region.filter(|_| self.pull.is_some()),
            };
            if self.hover_obj.is_some() {
                self.hover_region = None;
            }
            if self.hover_region.is_some() || self.hover_obj.is_some() {
                hovered_face = None;
            }
            self.tool_input(&resp, pointer, rect, hovered_face);
        }
        if prev_hover_region != self.hover_region {
            self.layers_changed = true;
        }
        // A short right click (no drag) opens the context menu; dragging with it still orbits.
        if resp.secondary_clicked() {
            let raw_face = if resp.hovered() { self.hovered_face() } else { None };
            self.ctx_target = self.context_target(raw_face);
        }
        resp.context_menu(|ui| self.context_menu_ui(ui));

        self.handle_keys(&ctx, rect);

        // Hand the frame to the renderer
        let size_px = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let pick_at = pointer.filter(|p| rect.contains(*p) && resp.hovered()).map(|p| {
            let l = (p - rect.min) * ppp;
            [l.x.max(0.0) as u32, l.y.max(0.0) as u32]
        });
        let (busy_pick, pick_serial) = {
            let r = self.render_state.renderer.read();
            match r.callback_resources.get::<Resources>() {
                Some(res) => {
                    self.hover_id = res.renderer.pick_result();
                    (res.renderer.pick_busy(), res.renderer.pick_serial())
                }
                None => (false, 0),
            }
        };
        // a read-back was started since the last frame: what was asked for is being done
        if pick_serial != self.pick_serial {
            self.pick_serial = pick_serial;
            self.pick_dirty = false;
        }
        // nothing moves and nothing is computed: no frame is drawn (a finished calculation and every input wake the window by themselves)
        let pointer_moved = self.last_pointer != pointer;
        self.repaint_why = (pointer_moved, busy_pick, false, false);
        if pointer_moved || busy_pick {
            ctx.request_repaint();
        }
        self.last_pointer = pointer;
        if pick_at.is_none() && !pointer.is_some_and(|p| rect.contains(p) && resp.hovered()) {
            self.hover_id = ID_NONE;
        }

        let grid = self.sk.as_ref().map(|sm| GridPlane {
            origin: sm.plane.origin.as_vec3(),
            u: sm.plane.x_dir.as_vec3(),
            v: sm.plane.y_dir().as_vec3(),
        });
        let layers = if std::mem::take(&mut self.layers_changed) {
            let l = self.build_layers();
            self.drawn_curve_vertices = l
                .iter()
                .filter(|d| d.thick_color == curvetools::CURVE_COLOR)
                .map(|d| d.thick.len())
                .sum();
            Some(l)
        } else {
            None
        };
        self.pump_edge_preview();
        let scene_sent = self.meshes_changed;
        let render_ids = scene_sent || self.camera != self.last_cam || size_px != self.last_size;
        self.repaint_why.2 = scene_sent;
        self.repaint_why.3 = self.camera != self.last_cam || size_px != self.last_size;
        // the pick read-back (what is under the pointer) is asked for when the pointer or the picture changed, not every frame: a
        // pick in flight keeps the window repainting, so a pick on every frame would draw frames for ever
        if pointer_moved || render_ids {
            self.pick_dirty = true;
        }
        let pick_at = if self.pick_dirty { pick_at } else { None };
        self.last_cam = self.camera;
        self.last_size = size_px;
        let scene_radius = self.scene_radius();
        let cb = ViewportCb {
            frame: Frame {
                camera: self.camera,
                hover: if self.sk.is_some()
                    || matches!(self.tool, Tool::Move | Tool::Combine)
                    || self.hover_region.is_some()
                    || self.hover_obj.is_some()
                    || self.pull.is_some()
                {
                    ID_NONE
                } else {
                    self.hover_id
                },
                selected: self.selected_ids(),
                body_marks: self.body_marks(),
                size: size_px,
                grid,
                render_ids,
                grid_levels: Some(self.grid_levels(rect)),
                scene_radius,
            },
            pick_at,
            new_bodies: if std::mem::take(&mut self.meshes_changed) {
                Some(self.body_override.clone().unwrap_or_else(|| self.meshes.clone()))
            } else {
                None
            },
            preview: if std::mem::take(&mut self.preview_changed) {
                Some(self.preview.clone())
            } else {
                None
            },
            layers,
        };
        ui.painter()
            .add(egui_wgpu::Callback::new_paint_callback(rect, cb));

        // Pull numeric label near the cursor
        if let (Some(p), Some(cur)) = (&self.pull, pointer) {
            let d = self.effective_distance(p.kind, p.distance());
            let mut text = if p.typed.is_empty() {
                format!("{} mm", fmt_signed(d as f64))
            } else {
                format!("{} mm", p.typed)
            };
            if self.pull_mode != PullMode::Auto && !matches!(p.kind, PullKind::Edges { .. }) {
                text.push_str(&format!(
                    " [{}]",
                    tr(match self.pull_mode {
                        PullMode::Add => "Add",
                        PullMode::New => "New body",
                        _ => "Cut",
                    })
                ));
            }
            let painter = ui.painter().with_clip_rect(rect);
            painter.text(
                cur + Vec2::new(16.0, -22.0),
                egui::Align2::LEFT_BOTTOM,
                text,
                egui::FontId::proportional(16.0),
                crate::theme::rgb(255, 230, 140),
            );
            // What the distance is attached to: a marker at the cursor and, for a vertex, a ring on it.
            if let Some(t) = &p.snap {
                let green = crate::theme::rgb(120, 255, 160);
                let label = match t {
                    SnapTarget::Face { .. } => tr("snap: face"),
                    SnapTarget::Vertex(_) => tr("snap: vertex"),
                    SnapTarget::Grid => tr("snap: grid"),
                };
                let mark = cur + Vec2::new(10.0, 10.0);
                match t {
                    SnapTarget::Face { .. } => drop(painter.rect_stroke(
                        Rect::from_center_size(mark, Vec2::splat(9.0)),
                        0.0,
                        Stroke::new(2.0, green),
                        egui::StrokeKind::Middle,
                    )),
                    SnapTarget::Vertex(_) => drop(painter.circle_stroke(mark, 5.0, Stroke::new(2.0, green))),
                    SnapTarget::Grid => {
                        painter.line_segment(
                            [mark + Vec2::new(-5.0, 0.0), mark + Vec2::new(5.0, 0.0)],
                            Stroke::new(2.0, green),
                        );
                        painter.line_segment(
                            [mark + Vec2::new(0.0, -5.0), mark + Vec2::new(0.0, 5.0)],
                            Stroke::new(2.0, green),
                        );
                    }
                }
                painter.text(
                    cur + Vec2::new(20.0, 16.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(12.0),
                    green,
                );
                if let SnapTarget::Vertex(v) = t {
                    if let Some(sp) = self.project(*v, rect) {
                        painter.circle_stroke(sp, 8.0, Stroke::new(2.5, green));
                    }
                }
            }
        }
        self.paint_sketch_overlay(ui, rect);
        self.hole_drag_paint(ui, rect);
        self.second_paint(ui, rect);
        self.empty_hint_paint(ui, rect);
        self.plane_move_paint(ui, rect);
        self.plane_at_paint(ui, rect);
        self.paint_curve_ends(ui, rect);
        self.move_paint(ui, rect);
        self.loft_paint(ui, rect);
        self.line3_paint(ui, rect);
        self.join_paint(ui, rect);
        self.measure_paint(ui, rect);
        self.paint_plane_labels(ui, rect);
    }

    fn tool_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        rect: Rect,
        hovered_face: Option<(BodyId, u32)>,
    ) {
        let ctx = resp.ctx.clone();
        let local = pointer.map(|p| p - rect.min);
        let local_rect = Rect::from_min_size(Pos2::ZERO, rect.size());
        let hovered_region = self.hover_region;
        let hovered_obj = self.hover_obj;
        let select = |app: &mut App, add: bool| {
            match (hovered_obj, hovered_region, hovered_face) {
                (Some(Obj::Entity(s, e)), _, _) => app.pick_entity(s, e),
                (Some(Obj::Curve(c)), _, _) => {
                    app.clear_selection();
                    app.curve_sel = Some(c);
                }
                (Some(Obj::Edge(b, e)), _, _) => {
                    app.selection.clear();
                    app.region_sel = None;
                    app.entity_sel = None;
                    app.body_sel = None;
                    if add {
                        if let Some(i) = app.edge_sel.iter().position(|x| *x == (b, e)) {
                            app.edge_sel.remove(i);
                        } else {
                            app.edge_sel.push((b, e));
                        }
                    } else {
                        app.edge_sel = vec![(b, e)];
                    }
                }
                (None, Some(r), _) => {
                    if add {
                        // Ctrl+click collects several regions (for Loft)
                        let keep_faces = app.selection.clone();
                        let keep_edges = app.edge_sel.clone();
                        let mut multi = std::mem::take(&mut app.region_multi);
                        if let Some(old) = app.region_sel {
                            if !multi.contains(&old) {
                                multi.push(old);
                            }
                        }
                        app.clear_selection();
                        if let Some(i) = multi.iter().position(|x| *x == r) {
                            multi.remove(i);
                        } else {
                            multi.push(r);
                        }
                        app.region_sel = multi.last().copied();
                        app.region_multi = multi;
                        app.selection = keep_faces;
                        app.edge_sel = keep_edges;
                    } else {
                        app.clear_selection();
                        app.region_sel = Some(r);
                        app.region_multi = vec![r];
                    }
                }
                (None, None, Some(h)) if add => {
                    app.region_sel = None;
                    app.entity_sel = None;
                    app.edge_sel.clear();
                    app.body_sel = None;
                    if let Some(i) = app.selection.iter().position(|s| *s == h) {
                        app.selection.remove(i);
                    } else {
                        app.selection.push(h);
                    }
                }
                (None, None, Some(h)) => {
                    app.clear_selection();
                    app.selection = vec![h];
                }
                (None, None, None) if !add => app.clear_selection(),
                _ => {}
            }
            app.sync_sel_order();
            app.layers_changed = true;
        };
        if self.revolve.is_some() {
            self.revolve_input(resp, pointer, rect);
            return;
        }
        if self.up_to.is_some() {
            if resp.clicked_by(PointerButton::Primary) {
                if let Some(p) = pointer {
                    self.finish_up_to(p, rect);
                }
            }
            return;
        }
        if !matches!(
            self.tool,
            Tool::Move | Tool::Combine | Tool::Pattern | Tool::Mirror
        ) && self.hover_body.take().is_some()
        {
            self.layers_changed = true;
        }
        match self.tool {
            Tool::Combine => self.combine_input(resp),
            Tool::Pattern | Tool::Mirror => self.link_input(resp),
            Tool::Loft => {
                let raw_face = self.hovered_face();
                self.loft_input(resp, raw_face);
            }
            Tool::Sweep => {
                let raw_face = self.hovered_face();
                self.sweep_input(resp, pointer, rect, raw_face);
            }
            Tool::Helix => self.helix_input(resp, pointer, rect),
            Tool::Fill => self.fill_input(resp, pointer, rect),
            Tool::Join => self.join_input(resp, pointer, rect),
            Tool::Measure => {
                let raw_face = self.hovered_face();
                self.measure_input(resp, pointer, rect, raw_face);
            }
            Tool::Move => {
                let raw_face = self.hovered_face();
                self.move_input(resp, pointer, rect, raw_face);
            }
            Tool::Select => {
                self.hole_drag_input(resp, pointer, rect, hovered_face);
                if resp.clicked_by(PointerButton::Primary) {
                    let add = ctx.input(|i| i.modifiers.command || i.modifiers.shift);
                    select(self, add);
                }
                // a double click takes the whole body: the Selection panel then shows the body
                if resp.double_clicked_by(PointerButton::Primary)
                    && hovered_obj.is_none()
                    && hovered_region.is_none()
                {
                    if let Some((b, _)) = hovered_face {
                        self.clear_selection();
                        self.body_sel = Some(b);
                        self.layers_changed = true;
                    }
                }
            }
            Tool::Pull => {
                if resp.drag_started_by(PointerButton::Primary) && self.pull.is_none() && self.idle() {
                    // Measure from where the button went down: the drag only starts after a few pixels of
                    // movement, and those must not be lost.
                    let start_local = ctx
                        .input(|i| i.pointer.press_origin())
                        .map(|p| p - rect.min)
                        .or(local);
                    if let (Some(Obj::Edge(b, e)), Some(l)) = (hovered_obj, start_local) {
                        let edges: Vec<u32> = if self.edge_sel.contains(&(b, e)) {
                            self.edge_sel
                                .iter()
                                .filter(|(bb, _)| *bb == b)
                                .map(|(_, x)| *x)
                                .collect()
                        } else {
                            vec![e]
                        };
                        if self.begin_edge_pull(b, edges, Pos2::new(l.x, l.y), local_rect) {
                            if let Some(s) = &mut self.pull {
                                s.dragging = true;
                            }
                        }
                        return;
                    }
                    let kind = match (hovered_region, hovered_face) {
                        (Some((s, i)), _) => Some(PullKind::Region { sketch: s, idx: i }),
                        (None, Some((b, f))) => Some(PullKind::Face { body: b, face: f }),
                        (None, None) => match (self.region_sel, self.selection.first()) {
                            (Some((s, i)), _) => Some(PullKind::Region { sketch: s, idx: i }),
                            (None, Some(&(b, f))) => Some(PullKind::Face { body: b, face: f }),
                            _ => None,
                        },
                    };
                    if let (Some(kind), Some(l)) = (kind, start_local) {
                        if self.begin_pull(kind, Pos2::new(l.x, l.y), local_rect) {
                            if let Some(s) = &mut self.pull {
                                s.dragging = true;
                            }
                        }
                    }
                }
                if resp.dragged_by(PointerButton::Primary) {
                    let alt = ctx.input(|i| i.modifiers.shift);
                    let (step, grid_on) = (self.settings.grid_step, self.settings.snap);
                    let mut snap_changed = false;
                    if let (Some(p), Some(l)) = (&mut self.pull, local) {
                        if p.dragging && p.typed.is_empty() {
                            let moved = Vec2::new(l.x, l.y) - Vec2::new(p.start.x, p.start.y);
                            if matches!(p.kind, PullKind::Edges { .. }) {
                                let mm = (moved.length() / p.px_per_mm.max(1e-3)) as f64;
                                let v = if grid_on && !alt && step > 0.0 {
                                    (mm / step).round() * step
                                } else {
                                    (mm * 100.0).round() / 100.0
                                };
                                p.d_mouse = v as f32;
                            } else {
                                let mm = (moved.dot(p.axis_dir) / p.px_per_mm.max(1e-3)) as f64;
                                let (d, t) =
                                    snap::snap_distance(mm, &p.cands, p.px_per_mm as f64, step, grid_on, alt);
                                p.d_mouse = d as f32;
                                snap_changed = p.snap != t;
                                p.snap = t;
                            }
                        }
                    }
                    if snap_changed {
                        self.layers_changed = true;
                    }
                    self.update_pull_preview();
                }
                if resp.drag_stopped_by(PointerButton::Primary) {
                    if let Some(p) = &mut self.pull {
                        p.dragging = false;
                        if p.typed.is_empty() {
                            self.commit_pull();
                        } else {
                            self.set_status("Press Enter to apply the typed distance, Esc to cancel", false);
                        }
                    }
                }
                if resp.clicked_by(PointerButton::Primary) && self.pull.is_none() {
                    let add = ctx.input(|i| i.modifiers.command || i.modifiers.shift);
                    select(self, add);
                }
            }
        }
    }

    fn sketch_input(
        &mut self,
        resp: &egui::Response,
        pointer: Option<Pos2>,
        ray: Option<(Vec3, Vec3)>,
        rect: Rect,
    ) {
        if self.plane_pick && self.sk.is_none() {
            self.plane_pick_input(resp, pointer, rect);
            return;
        }
        if self.plane_move.is_some() {
            self.plane_move_input(resp, pointer, rect);
            return;
        }
        if self.line3_active() {
            self.line3_input(resp, pointer, ray, rect);
            return;
        }
        let Some(sm) = &self.sk else { return };
        let plane = sm.plane;
        let hit = match (resp.hovered(), ray) {
            (true, Some((o, d))) => plane.ray_hit(o.as_dvec3(), d.as_dvec3()),
            _ => None,
        };
        let snap: Option<Snap> = hit.map(|(raw, _)| {
            let mpp = self.mm_per_px(&plane, raw, rect);
            let g = if self.settings.snap {
                // The user's step as long as it is at least 4 px on the screen, else the grid shown.
                let step = self.settings.grid_step;
                if step / mpp >= 4.0 {
                    step
                } else {
                    self.grid_levels(rect).0 as f64
                }
            } else {
                0.0
            };
            let points = self.snap_points();
            let base = self.sk.as_ref().unwrap().compute_snap(raw, mpp, g, &points);
            self.apply_aids(base, raw, mpp, pointer, rect, &points)
        });
        let snap = self.edge_dims_snap(snap, hit.map(|h| h.0), rect);
        if let Some(sm) = &mut self.sk {
            sm.snap = snap;
            if snap.is_none() {
                sm.guides.clear();
                sm.real3d = None;
            }
        }
        let _ = pointer;
        if resp.clicked_by(PointerButton::Primary) && self.sk.as_ref().is_some_and(|m| m.tool == SkTool::None)
        {
            match self.hover_obj {
                Some(Obj::Entity(s, e)) => self.pick_entity(s, e),
                _ => self.clear_selection(),
            }
            self.layers_changed = true;
        }
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(s) = snap {
                let drawing = self.sk.as_ref().is_some_and(|m| m.drawing.is_some());
                if drawing {
                    self.finish_drawing();
                } else if let Some(sm) = &mut self.sk {
                    if sm.tool == SkTool::None {
                        self.status = Status {
                            text: tr("Choose a tool first: L line, R rectangle, C circle"),
                            error: false,
                        };
                    } else {
                        sm.place = None;
                        sm.begin(s.pos);
                    }
                }
            }
        }
        if resp.clicked_by(PointerButton::Secondary) {
            if let Some(sm) = &mut self.sk {
                sm.drawing = None;
            }
        }
    }

    /// The straight shapes of the sketches in a plane (lines, axes, the sides of rectangles): what the live distances are measured to, besides the edges of the face.
    fn sketch_segments(&self, plane: &PlaneDef, body: Option<BodyId>) -> Vec<(DVec2, DVec2)> {
        let mut v = Vec::new();
        for s in self
            .doc
            .sketches()
            .iter()
            .filter(|s| s.visible && s.plane.same_plane(plane) && s.target == body)
        {
            for (_, e) in s.sketch.entities() {
                match *e {
                    Entity::Line { a, b } | Entity::Axis { a, b } => v.push((a, b)),
                    Entity::Rect { a, b } => {
                        let c = Entity::rect_corners(a, b);
                        v.extend((0..4).map(|i| (c[i], c[(i + 1) % 4])));
                    }
                    _ => {}
                }
            }
        }
        v
    }

    /// While a circle is placed: the live distances to the two nearest straight edges at the snapped point, the snaps to equal distance and to the middle
    /// of an edge, and the typed distances (which put the point where they say). Returns the snap that is used.
    fn edge_dims_snap(&mut self, snap: Option<Snap>, raw: Option<DVec2>, rect: Rect) -> Option<Snap> {
        let Some(sm) = &self.sk else { return snap };
        if sm.tool != SkTool::Circle {
            return snap;
        }
        let plane = sm.plane;
        // the circle is drawn: the centre is fixed, its distances are shown
        let centre = sm.drawing.as_ref().map(|d| d.start);
        let Some(at) = centre.or(snap.map(|s| s.pos)) else {
            if let Some(sm) = &mut self.sk {
                sm.edge_dims = None;
                sm.edge_snap = None;
            }
            return snap;
        };
        let mut segs = sm.straight.clone();
        segs.extend(self.sketch_segments(&plane, sm.body));
        let dims = rolidium_doc::rolidium_sketch::edgedims::nearest_edges(at, &segs);
        let mut out = snap;
        let mut edge_snap = None;
        let mut final_at = at;
        if centre.is_none() {
            if let (Some(d), Some(s), Some(raw)) = (&dims, snap, raw) {
                let mpp = self.mm_per_px(&plane, raw, rect);
                let free = matches!(s.label, SnapLabel::Free | SnapLabel::Grid);
                let sm = self.sk.as_ref().unwrap();
                let (ta, tb) = sm.edge_edit.values();
                let mut pos = s.pos;
                let mut label = s.label;
                if free {
                    let (p2, k) = d.snap(pos, 8.0 * mpp);
                    if let Some(k) = k {
                        pos = p2;
                        edge_snap = Some(k);
                        label = match k {
                            rolidium_doc::rolidium_sketch::edgedims::EdgeSnap::Equal => {
                                SnapLabel::EqualDistance
                            }
                            rolidium_doc::rolidium_sketch::edgedims::EdgeSnap::Middle(_) => {
                                SnapLabel::EdgeMiddle
                            }
                        };
                    }
                }
                if ta.is_some() || tb.is_some() {
                    pos = d.place(ta, tb);
                    edge_snap = None;
                }
                final_at = pos;
                out = Some(Snap {
                    pos,
                    label,
                    horizontal: false,
                    vertical: false,
                });
            }
        }
        let dims = if final_at != at {
            rolidium_doc::rolidium_sketch::edgedims::nearest_edges(final_at, &segs)
        } else {
            dims
        };
        if let Some(sm) = &mut self.sk {
            sm.edge_dims = dims;
            sm.edge_snap = edge_snap;
        }
        out
    }

    fn paint_sketch_overlay(&self, ui: &egui::Ui, rect: Rect) {
        let Some(sm) = &self.sk else { return };
        let painter = ui.painter().with_clip_rect(rect);
        let Some(snap) = sm.snap else { return };
        let plane = sm.plane;
        // the live distances of a circle to the two nearest edges
        if sm.tool == SkTool::Circle {
            if let Some(d) = &sm.edge_dims {
                let at = sm.drawing.as_ref().map_or(snap.pos, |d| d.start);
                self.paint_edge_dims(ui, rect, &plane, at, d, &sm.edge_edit, sm.edge_snap);
            }
        }
        // rubber band
        if let Some(e) = sm.effective(snap.pos) {
            for pl in e.polylines() {
                let pts: Vec<Pos2> = pl
                    .iter()
                    .filter_map(|p| self.project(plane.to_3d(*p).as_vec3(), rect))
                    .collect();
                if pts.len() > 1 {
                    painter.add(egui::Shape::line(
                        pts,
                        Stroke::new(2.0, crate::theme::rgb(255, 235, 120)),
                    ));
                }
            }
        }
        // an offset plane is drawn as a translucent sheet
        if sm.offset.abs() > 1e-9 {
            let half = (self.camera.distance * 0.45) as f64;
            let c = sm.focus.as_dvec3();
            let (u, v) = (sm.plane.x_dir, sm.plane.y_dir());
            let corner = |a: f64, b: f64| self.project((c + u * a * half + v * b * half).as_vec3(), rect);
            if let (Some(p0), Some(p1), Some(p2), Some(p3)) = (
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ) {
                painter.add(egui::Shape::convex_polygon(
                    vec![p0, p1, p2, p3],
                    Color32::from_rgba_unmultiplied(110, 170, 255, 28),
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(110, 170, 255, 140)),
                ));
            }
        }
        // helper points (mirrored copies), weakly
        for h in &sm.helpers {
            if let Some(hp) = self.project(plane.to_3d(*h).as_vec3(), rect) {
                painter.circle_stroke(
                    hp,
                    5.0,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(120, 255, 160, 110)),
                );
            }
        }
        // reference point of the distance fields
        if sm.drawing.is_none() && sm.tool != SkTool::None {
            if let Some(rp) = sm
                .ref_point
                .and_then(|r| self.project(plane.to_3d(r).as_vec3(), rect))
            {
                painter.circle_stroke(rp, 8.0, Stroke::new(2.0, crate::theme::rgb(255, 150, 60)));
                painter.text(
                    rp + Vec2::new(10.0, -16.0),
                    egui::Align2::LEFT_BOTTOM,
                    tr("reference"),
                    egui::FontId::proportional(12.0),
                    crate::theme::rgb(255, 150, 60),
                );
            }
        }
        // dotted guide lines
        for (a, b) in &sm.guides {
            if let (Some(pa), Some(pb)) = (
                self.project(plane.to_3d(*a).as_vec3(), rect),
                self.project(plane.to_3d(*b).as_vec3(), rect),
            ) {
                painter.extend(egui::Shape::dashed_line(
                    &[pa, pb],
                    Stroke::new(1.2, crate::theme::rgb(120, 220, 255)),
                    4.0,
                    4.0,
                ));
            }
        }
        // a 3D feature: thin line from the real point to its projection
        if let Some(real) = sm.real3d {
            if let (Some(pr), Some(pp)) = (
                self.project(real, rect),
                self.project(plane.to_3d(snap.pos).as_vec3(), rect),
            ) {
                painter.line_segment(
                    [pr, pp],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 255, 160, 170)),
                );
                painter.circle_filled(pr, 3.5, crate::theme::rgb(120, 255, 160));
            }
        }
        // snap marker
        let Some(c) = self.project(plane.to_3d(snap.pos).as_vec3(), rect) else {
            return;
        };
        let col = crate::theme::rgb(120, 255, 160);
        let stroke = Stroke::new(2.0, col);
        match snap.label {
            SnapLabel::Endpoint | SnapLabel::FaceVertex => drop(painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(11.0)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            )),
            SnapLabel::Midpoint => drop(painter.add(egui::Shape::closed_line(
                vec![
                    c + Vec2::new(0.0, -7.0),
                    c + Vec2::new(7.0, 6.0),
                    c + Vec2::new(-7.0, 6.0),
                ],
                stroke,
            ))),
            SnapLabel::Center => drop(painter.circle_stroke(c, 6.0, stroke)),
            SnapLabel::FaceEdge => drop(painter.add(egui::Shape::closed_line(
                vec![
                    c + Vec2::new(0.0, -7.0),
                    c + Vec2::new(7.0, 0.0),
                    c + Vec2::new(0.0, 7.0),
                    c + Vec2::new(-7.0, 0.0),
                ],
                stroke,
            ))),
            SnapLabel::Grid => {
                painter.line_segment([c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
                painter.line_segment([c + Vec2::new(0.0, -5.0), c + Vec2::new(0.0, 5.0)], stroke);
            }
            SnapLabel::Free => drop(painter.circle_filled(c, 2.5, col)),
            SnapLabel::Vertex3D | SnapLabel::Intersection => drop(painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(11.0)),
                0.0,
                stroke,
                egui::StrokeKind::Middle,
            )),
            SnapLabel::Mid3D => drop(painter.add(egui::Shape::closed_line(
                vec![
                    c + Vec2::new(0.0, -7.0),
                    c + Vec2::new(7.0, 6.0),
                    c + Vec2::new(-7.0, 6.0),
                ],
                stroke,
            ))),
            SnapLabel::Center3D | SnapLabel::Symmetric => drop(painter.circle_stroke(c, 6.0, stroke)),
            _ => drop(painter.add(egui::Shape::closed_line(
                vec![
                    c + Vec2::new(0.0, -7.0),
                    c + Vec2::new(7.0, 0.0),
                    c + Vec2::new(0.0, 7.0),
                    c + Vec2::new(-7.0, 0.0),
                ],
                stroke,
            ))),
        };
        let mut tag = tr(snap.label.text());
        if snap.horizontal {
            tag = if tag.is_empty() {
                tr("horizontal")
            } else {
                format!("{tag}, {}", tr("horizontal"))
            };
        }
        if snap.vertical {
            tag = if tag.is_empty() {
                tr("vertical")
            } else {
                format!("{tag}, {}", tr("vertical"))
            };
        }
        let font = egui::FontId::proportional(13.0);
        if !tag.is_empty() {
            painter.text(
                c + Vec2::new(14.0, 10.0),
                egui::Align2::LEFT_TOP,
                tag,
                font.clone(),
                col,
            );
        }
        // distances from the reference while placing a point
        if sm.drawing.is_none() && sm.tool != SkTool::None {
            if let Some(rp) = sm.ref_point {
                let target = sm.place_target(snap.pos).unwrap_or(snap.pos);
                let d = target - rp;
                let typed = sm.place.as_ref();
                for (i, (label, v)) in [("dx", d.x), ("dy", d.y)].iter().enumerate() {
                    let pos = c + Vec2::new(14.0, -58.0 + 21.0 * i as f32);
                    let r = Rect::from_min_size(pos, Vec2::new(150.0, 19.0));
                    painter.rect_filled(r, 3.0, Color32::from_black_alpha(205));
                    let active = typed.is_some_and(|p| p.active == i);
                    painter.rect_stroke(
                        r,
                        3.0,
                        Stroke::new(
                            1.5,
                            if active {
                                crate::theme::rgb(255, 215, 90)
                            } else {
                                Color32::from_gray(90)
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    painter.text(
                        r.left_center() + Vec2::new(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        *label,
                        font.clone(),
                        crate::theme::rgb(255, 150, 60),
                    );
                    let txt = match typed.map(|p| p.fields[i].as_str()) {
                        Some(t) if !t.is_empty() => t.to_string(),
                        _ => format!("{v:.2}"),
                    };
                    painter.text(
                        r.right_center() - Vec2::new(6.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        txt,
                        font.clone(),
                        Color32::from_gray(225),
                    );
                }
            }
        }
        // dimension fields
        let fields = sm.field_texts(snap.pos);
        let active = sm.drawing.as_ref().map(|d| d.active);
        for (i, (label, text, typed)) in fields.iter().enumerate() {
            let pos = c + Vec2::new(14.0, -58.0 + 21.0 * i as f32);
            let r = Rect::from_min_size(pos, Vec2::new(150.0, 19.0));
            painter.rect_filled(r, 3.0, Color32::from_black_alpha(205));
            let border = if Some(i) == active {
                crate::theme::rgb(255, 215, 90)
            } else {
                Color32::from_gray(90)
            };
            painter.rect_stroke(r, 3.0, Stroke::new(1.5, border), egui::StrokeKind::Inside);
            painter.text(
                r.left_center() + Vec2::new(6.0, 0.0),
                egui::Align2::LEFT_CENTER,
                tr(label),
                font.clone(),
                Color32::from_gray(170),
            );
            let vc = if *typed {
                crate::theme::rgb(255, 235, 120)
            } else {
                Color32::from_gray(225)
            };
            painter.text(
                r.right_center() - Vec2::new(6.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                text,
                font.clone(),
                vc,
            );
        }
    }

    /// Grid step, its presets and the snapping switch (right side of the status bar).
    fn grid_controls(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        if ui
            .checkbox(&mut self.settings.snap, tr("Snap"))
            .on_hover_text(tr("Snap sketching and drag distances to the grid step"))
            .changed()
        {
            changed = true;
        }
        egui::ComboBox::from_id_salt("grid_presets")
            .width(0.0)
            .selected_text("")
            .show_ui(ui, |ui| {
                for s in [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0] {
                    if ui
                        .selectable_label(
                            (self.settings.grid_step - s).abs() < 1e-9,
                            format!("{} mm", fmt_num(s)),
                        )
                        .clicked()
                    {
                        self.settings.grid_step = s;
                        self.grid_text = fmt_num(s);
                        changed = true;
                    }
                }
            });
        let r = ui.add(egui::TextEdit::singleline(&mut self.grid_text).desired_width(42.0));
        if r.lost_focus() {
            match parse_num(&self.grid_text).filter(|v| *v > 0.0) {
                Some(v) if (v - self.settings.grid_step).abs() > 1e-12 => {
                    self.settings.grid_step = v;
                    changed = true;
                }
                _ => {}
            }
            self.grid_text = fmt_num(self.settings.grid_step);
        } else if !r.has_focus() {
            self.grid_text = fmt_num(self.settings.grid_step);
        }
        ui.label(format!("{}:", tr("Grid (mm)")));
        if changed {
            self.settings.save();
            self.layers_changed = true;
        }
    }

    /// The one-time message after a start with a different graphics backend than the usual one.
    fn startup_notice_window(&mut self, ctx: &egui::Context) {
        let Some((failed, running)) = &self.startup_notice else {
            return;
        };
        let text = trf(
            "The graphics backend {} did not start last time, so Rolidium is using {} now. You can change this in Settings (Graphics backend). Choosing auto there makes Rolidium try all backends again, for example after a graphics driver update.",
            &[failed.join(", "), running.clone()],
        );
        let mut close = false;
        egui::Window::new(tr("Graphics"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(text);
                if ui.button(tr("OK")).clicked() {
                    close = true;
                }
            });
        if close {
            self.startup_notice = None;
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        egui::Window::new(tr("Settings"))
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr("Language"));
                    let langs = i18n::available();
                    let shown = if self.settings.language.is_empty() {
                        tr("System default")
                    } else {
                        langs
                            .iter()
                            .find(|(c, _)| *c == self.settings.language)
                            .map_or(self.settings.language.clone(), |(_, n)| n.clone())
                    };
                    egui::ComboBox::from_id_salt("language")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(self.settings.language.is_empty(), tr("System default"))
                                .clicked()
                            {
                                self.settings.language.clear();
                                changed = true;
                            }
                            for (code, name) in &langs {
                                if ui
                                    .selectable_label(self.settings.language == *code, name)
                                    .clicked()
                                {
                                    self.settings.language = code.clone();
                                    changed = true;
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Appearance"));
                    let before = self.settings.theme.clone();
                    egui::ComboBox::from_id_salt("theme")
                        .selected_text(tr(match self.settings.theme.as_str() {
                            "light" => "Light",
                            "system" => "Follow the system",
                            _ => "Dark",
                        }))
                        .show_ui(ui, |ui| {
                            for (code, name) in [
                                ("dark", "Dark"),
                                ("light", "Light"),
                                ("system", "Follow the system"),
                            ] {
                                ui.selectable_value(&mut self.settings.theme, code.to_string(), tr(name));
                            }
                        });
                    if self.settings.theme != before {
                        theme::apply(ctx, &self.settings.theme, self.settings.brightness);
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Brightness"));
                    let r = ui
                        .add(egui::Slider::new(&mut self.settings.brightness, theme::MIN..=theme::MAX).show_value(false))
                        .on_hover_text(tr("Moves the grey of the panels and of the model area; the text and the icons keep their contrast"));
                    if r.changed() {
                        theme::apply(ctx, &self.settings.theme, self.settings.brightness);
                        changed = true;
                    }
                    if ui
                        .add_enabled(self.settings.brightness != 0, egui::Button::new(tr("Default")))
                        .clicked()
                    {
                        self.settings.brightness = 0;
                        theme::apply(ctx, &self.settings.theme, 0);
                        changed = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Frame rate ceiling"));
                    let label = |c: &str| match c {
                        "monitor" => tr("By the monitor"),
                        "none" => tr("Unlimited"),
                        n => n.to_string(),
                    };
                    let before = self.settings.fps_cap.clone();
                    egui::ComboBox::from_id_salt("fps_cap")
                        .selected_text(label(&self.settings.fps_cap))
                        .show_ui(ui, |ui| {
                            for c in crate::settings::FPS_CAPS {
                                ui.selectable_value(&mut self.settings.fps_cap, c.to_string(), label(c));
                            }
                        });
                    if self.settings.fps_cap != before {
                        changed = true;
                    }
                });
                if crate::settings::synchronised(&self.settings.fps_cap)
                    != crate::settings::PRESENT.get().is_none_or(|p| p.starts_with("AutoVsync"))
                {
                    ui.label(tr("The change between synchronised and unsynchronised frames takes effect after a restart."));
                }
                if ui
                    .checkbox(
                        &mut self.settings.auto_turn_view,
                        tr("Turn the view square to the sketch automatically"),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(
                        &mut self.settings.new_doc_sketch,
                        tr("A new document starts with a sketch on the XY plane"),
                    )
                    .changed()
                {
                    changed = true;
                }
                if ui
                    .checkbox(
                        &mut self.settings.guide_enabled,
                        tr("Show the guide to the first steps"),
                    )
                    .changed()
                {
                    self.guide.enabled = self.settings.guide_enabled;
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.label(tr("Graphics backend"));
                    let before = self.settings.backend.clone();
                    egui::ComboBox::from_id_salt("backend")
                        .selected_text(self.settings.backend.clone())
                        .show_ui(ui, |ui| {
                            for code in ["auto", "dx12", "vulkan", "gl"] {
                                ui.selectable_value(&mut self.settings.backend, code.to_string(), code);
                            }
                        });
                    if self.settings.backend != before {
                        if self.settings.backend == "auto" {
                            // back to auto: the skipped backends are tried again (for example after a driver update)
                            self.settings.backend_blocked.clear();
                        }
                        changed = true;
                    }
                });
                if !self.settings.backend_blocked.is_empty() {
                    ui.label(
                        egui::RichText::new(trf(
                            "Skipped because they failed to start earlier: {}.",
                            &[crate::startup::blocked_names(&self.settings.backend_blocked)],
                        ))
                        .small(),
                    );
                    if ui.button(tr("Try all backends again")).clicked() {
                        self.settings.backend_blocked.clear();
                        changed = true;
                    }
                }
                ui.label(
                    egui::RichText::new(tr("The backend changes after the program is started again."))
                        .weak()
                        .small(),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr("Autosave every (minutes, 0 = off)"));
                    let mut m = self.settings.autosave_minutes;
                    if ui.add(egui::DragValue::new(&mut m).range(0..=240)).changed() {
                        self.settings.autosave_minutes = m;
                        changed = true;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(tr("Calculation threads (Auto = one per core less one)"));
                    let label = if self.settings.threads == 0 {
                        tr("Auto")
                    } else {
                        self.settings.threads.to_string()
                    };
                    egui::ComboBox::from_id_salt("threads")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for n in [0u32, 1, 2, 4, 8] {
                                let t = if n == 0 { tr("Auto") } else { n.to_string() };
                                if ui.selectable_value(&mut self.settings.threads, n, t).changed() {
                                    changed = true;
                                }
                            }
                        });
                });
                ui.label(
                    egui::RichText::new(trf(
                        "Threads in use: {}",
                        &[rolidium_kernel::par::threads().to_string()],
                    ))
                    .weak()
                    .small(),
                );
                if crate::settings::threads_pending(
                    self.settings.threads,
                    rolidium_kernel::par::threads(),
                    rolidium_kernel::par::env_choice().is_some(),
                ) {
                    ui.label(
                        egui::RichText::new(tr(
                            "The new number takes effect the next time Rolidium is started.",
                        ))
                        .color(ui.visuals().warn_fg_color)
                        .small(),
                    );
                }
                ui.label(
                    egui::RichText::new(tr(
                        "The environment variable ROLIDIUM_THREADS overrides this choice.",
                    ))
                    .weak()
                    .small(),
                );
                ui.separator();
                {
                    let mut yes = self.settings.check_updates == Some(true);
                    let offline = rolidium_net::offline();
                    if ui
                        .add_enabled(
                            !offline,
                            egui::Checkbox::new(&mut yes, tr("Check for a new version once a day")),
                        )
                        .changed()
                    {
                        self.settings.check_updates = Some(yes);
                        changed = true;
                    }
                    if offline {
                        ui.label(
                            egui::RichText::new(tr("The network is switched off (ROLIDIUM_OFFLINE)."))
                                .color(ui.visuals().warn_fg_color)
                                .small(),
                        );
                    }
                    ui.label(
                        egui::RichText::new(tr("The question that is sent (nothing else):"))
                            .weak()
                            .small(),
                    );
                    ui.label(
                        egui::RichText::new(rolidium_net::describe_version_request(
                            env!("ROLIDIUM_VERSION_LABEL"),
                            &rolidium_net::system_info(),
                        ))
                        .monospace()
                        .small(),
                    );
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.diagnostics_job.is_none(),
                                egui::Button::new(tr("Save diagnostics")),
                            )
                            .clicked()
                        {
                            self.start_save_diagnostics(ctx);
                        }
                    });
                    if ui
                        .link(tr("Report a bug or an idea…"))
                        .on_hover_text(tr(
                            "Opens the panel at the bottom of the right column. You see what is sent first, and nothing is sent without your confirmation.",
                        ))
                        .clicked()
                    {
                        self.feedback.expanded = true;
                    }
                    if ui.link(tr("Privacy notice")).clicked() {
                        crate::diagnose::open_path(std::path::Path::new(rolidium_net::urls::PRIVACY));
                    }
                }
                ui.separator();
                if ui
                    .checkbox(&mut self.settings.sound, tr("Sound on error"))
                    .changed()
                {
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(self.settings.sound, |ui| {
                        ui.label(tr("Volume"));
                        let mut pct = (self.settings.volume * 100.0).round();
                        if ui
                            .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix(" %"))
                            .changed()
                        {
                            self.settings.volume = pct / 100.0;
                            changed = true;
                        }
                        if ui.button(tr("Test")).clicked() {
                            crate::sound::play_error(self.settings.volume);
                        }
                    });
                });
            });
        if changed {
            i18n::set_language(&self.settings.language);
            self.settings.save();
            self.layers_changed = true;
        }
        if !open {
            self.settings_open = false;
        }
    }

    fn hint(&self) -> String {
        if self.plane_pick {
            return tr("Choose a plane or a face for the sketch");
        }
        if let Some(sm) = &self.sk {
            return match (&sm.drawing, sm.tool) {
                (Some(_), _) => tr(
                    "Click to finish, or type a size (Tab: next field, Enter: confirm, Esc: cancel shape).",
                ),
                (None, SkTool::None) => {
                    tr("Sketch: L line, R rectangle, C circle. V faces the sketch, D or P leaves.")
                }
                (None, _) => {
                    tr("Click to place the first point. Esc/right click cancels, D or P leaves the sketch.")
                }
            };
        }
        if self.up_to.is_some() {
            return tr("Up to: click a face, an edge, a vertex or a point on a face. Esc cancels.");
        }
        match (&self.pull, self.tool) {
            (Some(p), _) if !p.typed.is_empty() => {
                tr("Pull: type the distance (negative removes material). Enter applies, Esc cancels.")
            }
            (Some(_), _) => tr("Pull: drag along the normal (snaps to faces, vertices and the grid; hold Shift for free movement), or type a number."),
            (None, Tool::Select) => {
                tr("Select: click a face or a sketch region. K sketches on the selected flat face (or the base plane).")
            }
            (None, Tool::Loft) => tr("Loft: click the profiles (sketch regions or flat faces) in the order they should be joined, then Apply or Enter."),
            (None, Tool::Combine) => tr("Combine: click the target body, then the bodies to combine with it; choose Merge, Cut or Intersect and press Apply or Enter."),
            (None, Tool::Pattern | Tool::Mirror) => self.link_hint(),
            (None, Tool::Join) => tr("Join ends: select the lines (Shift adds more; nothing selected means the whole sketch), choose the largest gap, Enter confirms"),
            (None, Tool::Fill) => tr("Fill: click an edge of a hole or opening, or the lines of a shape that is not filled yet. Shift adds more."),
            (None, Tool::Measure) => tr("Measure: click two elements (vertex, edge, face, point of a sketch or curve). Esc clears."),
            (None, Tool::Sweep) => tr("Sweep: click the profile, then the path; choose the mode and press Apply or Enter."),
            (None, Tool::Helix) => tr("Helix: click the axis (a cylindrical face, a straight edge or line, or a point), then set the values and press Create."),
            (None, Tool::Move) => tr("Move: click a body (Ctrl+click adds more), then drag an arrow to move or a ring to rotate; type a number and Enter for an exact value."),
            (None, Tool::Pull) => {
                tr("Pull: drag a flat face or a sketch region (out adds, in removes). Or select one and type a distance. U pulls up to a target.")
            }
        }
    }
}

impl eframe::App for App {
    /// Tab belongs to the dimension fields in sketch mode, not to keyboard navigation of the buttons, so it is
    /// taken out of the input before egui sees it.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let second = self.tool == Tool::Pattern && self.link_ui.pick == Some(linktool::LinkPick::Point);
        if (self.sk.is_none() && self.hole_drag.is_none() && !second) || ctx.memory(|m| m.focused()).is_some()
        {
            return;
        }
        let mut pressed = false;
        raw.events.retain(|e| match e {
            egui::Event::Key {
                key: Key::Tab,
                pressed: p,
                modifiers,
                ..
            } if !modifiers.any() => {
                pressed |= *p;
                false
            }
            _ => true,
        });
        if pressed {
            if let Some(h) = &mut self.hole_drag {
                h.edit.tab();
                self.layers_changed = true;
            } else if second && self.sk.is_none() {
                self.link_ui.second_edit.tab();
                self.layers_changed = true;
            } else if let Some(sm) = &mut self.sk {
                sm.tab();
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // the time of the whole frame function is noted when it ends (also on an early return) and taken into account by the next frame
        struct Timer(Instant, std::rc::Rc<std::cell::Cell<f32>>);
        impl Drop for Timer {
            fn drop(&mut self) {
                self.1.set(self.0.elapsed().as_secs_f32() * 1000.0);
            }
        }
        let _timer = Timer(Instant::now(), self.last_ui_ms.clone());
        crate::prof::next_frame();
        crate::prof::take_kernel();
        self.work_ms[0] = self.work_ms[0] * 0.9 + self.last_ui_ms.get() * 0.1;
        if self.last_work_log.elapsed().as_secs() >= 2 && std::env::var_os("ROLIDIUM_FRAME_LOG").is_some() {
            self.last_work_log = Instant::now();
            log(&format!(
                "frame work: window {:.1} ms (viewport {:.1}, pointer {:.1}); frame {:.1} ms; parts: {}; dearest native call: {:?}",
                self.work_ms[0],
                self.work_ms[1],
                self.work_ms[2],
                self.frame_ms,
                crate::prof::all_text(),
                crate::prof::worst_kernel_call()
            ));
        }
        let ctx = ui.ctx().clone();
        crate::startup::frame_drawn();
        self.startup_notice_window(&ctx);
        // the ceiling of the frame rate: the next frame waits for its time (kept as a deadline, not as a pause after each frame, so that the
        // rate does not drift; the last millisecond is waited for in a loop, where the sleep of the system is not exact)
        if self.selftest.is_none() && self.bench.is_none() {
            if let Some(iv) = crate::settings::fps_interval(&self.settings.fps_cap) {
                let iv = if crate::settings::synchronised(&self.settings.fps_cap) {
                    iv.mul_f64(crate::settings::SYNC_MARGIN)
                } else {
                    iv
                };
                crate::settings::pace(&mut self.frame_deadline, iv);
            } else {
                self.frame_deadline = None;
            }
        }
        let now = Instant::now();
        self.frame_ms = (now - self.last_frame).as_secs_f32() * 1000.0;
        self.last_frame = now;
        // the rate is measured over frames that follow each other (a pause of the window is not a slow frame)
        if self.frame_ms > 0.0 && self.frame_ms < 250.0 {
            let fps = 1000.0 / self.frame_ms;
            self.fps_smooth = if self.fps_smooth == 0.0 {
                fps
            } else {
                self.fps_smooth * 0.95 + fps * 0.05
            };
        }
        if ctx.input(|i| i.key_pressed(Key::F12)) {
            self.show_debug = !self.show_debug;
        }
        // icons follow the theme, also when "follow the system" changes it while the program runs
        theme::set(!ctx.global_style().visuals.dark_mode, self.settings.brightness);
        icons::set_light(!ctx.global_style().visuals.dark_mode);
        prof!("refresh_curve_cache", self.refresh_curve_cache(&ctx));
        prof!("busy_tick", self.busy_tick());
        prof!("view_tick", self.view_tick(&ctx));
        prof!("bench_tick", self.bench_tick(&ctx));
        prof!("edit_bench_tick", self.edit_bench_tick(&ctx));
        prof!("poll_worker", self.poll_worker());
        // a result of the worker or an Undo of this frame changed the document: the curves are drawn from it, not from the frame before
        prof!("refresh_curve_cache", self.refresh_curve_cache(&ctx));
        prof!("selftest_tick", self.selftest_tick(&ctx));
        prof!("guide_tick", self.guide_tick());
        prof!("remesh_tick", self.remesh_tick(&ctx));

        egui::Panel::top("toolbar")
            .resizable(false)
            .exact_size(46.0)
            .show(ui, |ui| prof!("toolbar", self.top_toolbar(ui)));

        // declared before the side panels, so that they end above it and nothing of them is hidden under it
        let status = egui::Panel::bottom("status").resizable(false).show(ui, |ui| {
            let avail = ui.available_width();
            let right_w = 600.0_f32.min(avail * 0.6);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new((avail - right_w - 12.0).max(100.0), 20.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let c = if self.status.error {
                            crate::theme::rgb(255, 120, 110)
                        } else {
                            Color32::from_gray(200)
                        };
                        ui.add(egui::Label::new(egui::RichText::new(&self.status.text).color(c)).truncate());
                        if !self.idle() {
                            ui.spinner();
                        }
                        ui.separator();
                        ui.add(egui::Label::new(egui::RichText::new(self.hint()).weak()).truncate());
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(crate::version_string()).color(crate::theme::rgb(150, 200, 255)),
                    );
                    ui.separator();
                    let labels = status_labels(self.presentation(), self.adapter_software);
                    self.status_shows = labels;
                    if labels.0 {
                        ui.scope(|ui| {
                            ui.set_max_width(120.0);
                            ui.add(egui::Label::new(egui::RichText::new(&self.adapter).weak()).truncate());
                        });
                    }
                    if labels.1 {
                        ui.label(egui::RichText::new(tr("software drawing")).small().color(crate::theme::rgb(220, 160, 80)))
                            .on_hover_text(tr("The picture is drawn by the processor, not by a graphics card, so it can be slow and may look different. Try another graphics backend in Settings, or update the graphics driver."));
                    }
                    ui.separator();
                    self.grid_controls(ui);
                    ui.separator();
                    if icons::icon_button(ui, &self.help, "donate", true).clicked() {
                        self.about_open = true;
                        self.about_scroll = true;
                    }
                    if let Some(p) = &self.pull {
                        ui.separator();
                        ui.strong(trf(
                            "Distance: {} mm",
                            &[if p.typed.is_empty() {
                                fmt_signed(p.d_mouse as f64)
                            } else {
                                p.typed.clone()
                            }],
                        ));
                    }
                });
            });
        });
        self.status_top = status.response.rect.min.y;

        egui::Panel::left("tools")
            .resizable(false)
            .exact_size(196.0)
            .show(ui, |ui| {
                // the middle part (tools, edges, views) scrolls when the window is low; the File block and the Settings / About
                // buttons below it are always visible
                let column_h = ui.available_height();
                self.left_avail_h = column_h;
                let (size, per_row) = toolbar::file_block_layout(column_h, self.left_rest_h);
                let bottom_h = toolbar::file_block_height(size, per_row) + toolbar::LEFT_BOTTOM_H;
                let top = ui.cursor().min.y;
                let out = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((column_h - bottom_h).max(60.0))
                    .show(ui, |ui| prof!("left column", self.left_column(ui)));
                self.left_rest_h = out.content_size.y;
                self.left_scrolls = out.content_size.y > out.inner_rect.height() + 0.5;
                self.left_file_rect = Rect::NOTHING;
                ui.add_space(4.0);
                let fb = ui.scope(|ui| self.left_file_block(ui, size, per_row));
                self.left_file_rect = fb.response.rect;
                let _ = top;
                self.left_bottom(ui);
            });

        egui::Panel::right("tree")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                // the report panel sits at the bottom (40 % of the column is its area, it uses what it needs), the rest of the column above it
                let full = ui.available_rect_before_wrap();
                // the folded row at the top of the panel is drawn above what is reserved (about 24 points), so the open panel is 40 % of the column
                // (at least 226 points, so that the kind and the text always fit above the buttons even in a low window)
                let area_h = (full.height() * 0.4 - 24.0).max(226.0).min(full.height() * 0.7);
                let used_h = self.feedback.rect.height().clamp(20.0, area_h);
                let fb_rect = Rect::from_min_max(Pos2::new(full.min.x, full.max.y - area_h), full.max);
                let split = full.max.y - used_h - 6.0;
                let top_rect = Rect::from_min_max(full.min, Pos2::new(full.max.x, split));
                ui.painter().hline(
                    full.x_range(),
                    split + 2.0,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
                let before = self.feedback.rect.height();
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(fb_rect)
                        .layout(egui::Layout::bottom_up(egui::Align::Min)),
                    |ui| prof!("report panel", self.feedback_panel(ui, full.height())),
                );
                if (self.feedback.rect.height() - before).abs() > 0.5 {
                    ui.ctx().request_repaint();
                }
                ui.scope_builder(egui::UiBuilder::new().max_rect(top_rect), |ui| {
                    {
                        egui::ScrollArea::vertical()
                            .id_salt("right-column")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(tr("Document")).strong());
                                let mut tree_cmd: Option<(Command, &str)> = None;
                                egui::CollapsingHeader::new(tr("Bodies"))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        let mut clicked = None;
                                        let bodies: Vec<rolidium_doc::Body> = self.doc.bodies().to_vec();
                                        // the bodies picked in the model are marked in the list, and the list scrolls to the last one picked
                                        let marked: std::collections::HashSet<BodyId> = self
                                            .selection
                                            .iter()
                                            .map(|s| s.0)
                                            .chain(self.body_sel)
                                            .chain(self.move_sel.iter().copied())
                                            .collect();
                                        let last_picked = self
                                            .body_sel
                                            .or_else(|| self.selection.last().map(|s| s.0))
                                            .or_else(|| self.move_sel.last().copied());
                                        let scroll_to = if last_picked != self.tree_seen {
                                            last_picked
                                        } else {
                                            None
                                        };
                                        self.tree_seen = last_picked;
                                        // the list scrolls by itself (hundreds of bodies), and follows the selection
                                        let list_h = (ui.ctx().content_rect().height() * 0.45).max(160.0);
                                        let scroll_out = egui::ScrollArea::vertical()
                                            .id_salt("body-list")
                                            .max_height(list_h)
                                            .auto_shrink([true, true])
                                            .show(ui, |ui| {
                                                for b in &bodies {
                                                    ui.horizontal(|ui| {
                                                        if icons::eye_button(ui, b.visible).clicked() {
                                                            tree_cmd = Some((
                                                                Command::SetVisible {
                                                                    target: ItemId::Body(b.id),
                                                                    visible: !b.visible,
                                                                },
                                                                if b.visible { "Hidden" } else { "Shown" },
                                                            ));
                                                        }
                                                        let faces =
                                                            self.entry(b.id).map_or(0, |m| m.faces.len());
                                                        let derived = rolidium_doc::link_of_result(self.doc.state(), b.id).is_some();
                                                        let mut t = if b.visible {
                                                            trf(
                                                                "{}  ({} faces)",
                                                                &[b.name.clone(), faces.to_string()],
                                                            )
                                                        } else {
                                                            format!("{}  ({})", b.name, tr("hidden"))
                                                        };
                                                        if derived {
                                                            t = format!("⛓ {t}");
                                                        }
                                                        let r =
                                                            ui.selectable_label(marked.contains(&b.id), t);
                                                        // a body can be dragged onto a group
                                                        let r = r.interact(egui::Sense::click_and_drag());
                                                        if r.drag_started() {
                                                            r.dnd_set_drag_payload(linktool::Dnd::Body(b.id));
                                                        }
                                                        let r = match rolidium_doc::link_of_result(self.doc.state(), b.id) {
                                                            Some(l) => r.on_hover_text(trf(
                                                                "Made by the link '{}'. Click the link in the list of links to change it.",
                                                                std::slice::from_ref(&l.name),
                                                            )),
                                                            None => r,
                                                        };
                                                        if scroll_to == Some(b.id) {
                                                            r.scroll_to_me(Some(egui::Align::Center));
                                                        }
                                                        if Some(b.id) == last_picked {
                                                            self.tree_probe = (
                                                                Some(b.id),
                                                                marked.contains(&b.id),
                                                                ui.is_rect_visible(r.rect),
                                                            );
                                                        }
                                                        if r.clicked() {
                                                            clicked = Some(b.id);
                                                        }
                                                        if r.double_clicked() {
                                                            // a double click renames (Enter confirms, Esc cancels)
                                                            self.rename =
                                                                Some((ItemId::Body(b.id), b.name.clone()));
                                                        }
                                                        r.context_menu(|ui| {
                                                            if ui.button(tr("Delete")).clicked() {
                                                                tree_cmd = Some((
                                                                    Command::DeleteBody { body: b.id },
                                                                    "Deleted (Ctrl+Z brings it back)",
                                                                ));
                                                                ui.close();
                                                            }
                                                            if ui
                                                                .button(tr(if b.visible {
                                                                    "Hide"
                                                                } else {
                                                                    "Show"
                                                                }))
                                                                .clicked()
                                                            {
                                                                tree_cmd = Some((
                                                                    Command::SetVisible {
                                                                        target: ItemId::Body(b.id),
                                                                        visible: !b.visible,
                                                                    },
                                                                    if b.visible {
                                                                        "Hidden"
                                                                    } else {
                                                                        "Shown"
                                                                    },
                                                                ));
                                                                ui.close();
                                                            }
                                                            if ui.button(tr("Rename")).clicked() {
                                                                self.rename = Some((
                                                                    ItemId::Body(b.id),
                                                                    b.name.clone(),
                                                                ));
                                                                ui.close();
                                                            }
                                                        });
                                                    });
                                                }
                                            });
                                        self.tree_scroll_y = scroll_out.state.clamped_offset().y;
                                        if let Some(id) = clicked {
                                            let add = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                                            if self.tool == Tool::Move && add {
                                                self.move_toggle(id);
                                            } else {
                                                self.clear_selection();
                                                self.body_sel = Some(id);
                                                if self.tool == Tool::Move {
                                                    self.move_sel = vec![id];
                                                }
                                            }
                                            self.layers_changed = true;
                                        }
                                        if self.doc.bodies().is_empty() {
                                            ui.weak(tr("(none)"));
                                        }
                                    });
                                prof!("tree: links", self.links_and_groups_tree(ui, &mut tree_cmd));
                                egui::CollapsingHeader::new(tr("Sketches"))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        let mut clicked = None;
                                        let sketches: Vec<rolidium_doc::SketchDoc> =
                                            self.doc.sketches().to_vec();
                                        for s in &sketches {
                                            ui.horizontal(|ui| {
                                                if icons::eye_button(ui, s.visible).clicked() {
                                                    tree_cmd = Some((
                                                        Command::SetVisible {
                                                            target: ItemId::Sketch(s.id),
                                                            visible: !s.visible,
                                                        },
                                                        if s.visible { "Hidden" } else { "Shown" },
                                                    ));
                                                }
                                                let n = self
                                                    .regions
                                                    .iter()
                                                    .find(|r| r.id == s.id)
                                                    .map_or(0, |r| r.regions.len());
                                                let on = if s.target.is_some() {
                                                    tr("on a face")
                                                } else {
                                                    trf("on {}", &[plane_name(&s.plane).to_string()])
                                                };
                                                let t = trf(
                                                    "{}  ({} shapes, {} regions, {})",
                                                    &[
                                                        s.name.clone(),
                                                        s.sketch.entities().len().to_string(),
                                                        n.to_string(),
                                                        on,
                                                    ],
                                                );
                                                let active =
                                                    self.sk.as_ref().is_some_and(|m| m.sketch == Some(s.id));
                                                let text = if active {
                                                    egui::RichText::new(t).strong()
                                                } else {
                                                    egui::RichText::new(t)
                                                };
                                                let r =
                                                    ui.selectable_label(self.sketch_sel == Some(s.id), text);
                                                if r.clicked() {
                                                    clicked = Some(s.id);
                                                }
                                                r.context_menu(|ui| {
                                                    if ui.button(tr("Delete")).clicked() {
                                                        tree_cmd = Some((
                                                            Command::DeleteSketch { sketch: s.id },
                                                            "Deleted (Ctrl+Z brings it back)",
                                                        ));
                                                        ui.close();
                                                    }
                                                    if ui
                                                        .button(tr(if s.visible { "Hide" } else { "Show" }))
                                                        .clicked()
                                                    {
                                                        tree_cmd = Some((
                                                            Command::SetVisible {
                                                                target: ItemId::Sketch(s.id),
                                                                visible: !s.visible,
                                                            },
                                                            if s.visible { "Hidden" } else { "Shown" },
                                                        ));
                                                        ui.close();
                                                    }
                                                    if ui.button(tr("Rename")).clicked() {
                                                        self.rename =
                                                            Some((ItemId::Sketch(s.id), s.name.clone()));
                                                        ui.close();
                                                    }
                                                });
                                            });
                                        }
                                        if let Some(id) = clicked {
                                            self.clear_selection();
                                            self.sketch_sel = Some(id);
                                        }
                                        if self.doc.sketches().is_empty() {
                                            ui.weak(tr("(none)"));
                                        }
                                    });
                                egui::CollapsingHeader::new(tr("Curves"))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        let mut clicked = None;
                                        let curves: Vec<rolidium_doc::CurveDoc> = self.doc.curves().to_vec();
                                        for c in &curves {
                                            ui.horizontal(|ui| {
                                                if icons::eye_button(ui, c.visible).clicked() {
                                                    tree_cmd = Some((
                                                        Command::SetVisible {
                                                            target: ItemId::Curve(c.id),
                                                            visible: !c.visible,
                                                        },
                                                        if c.visible { "Hidden" } else { "Shown" },
                                                    ));
                                                }
                                                let kind = match c.def {
                                                    rolidium_doc::CurveDef::Polyline { .. } => tr("3D line"),
                                                    rolidium_doc::CurveDef::Helix(_) => tr("Helix"),
                                                };
                                                let t = if c.visible {
                                                    format!("{}  ({})", c.name, kind)
                                                } else {
                                                    format!("{}  ({})", c.name, tr("hidden"))
                                                };
                                                let r = ui.selectable_label(self.curve_sel == Some(c.id), t);
                                                if r.clicked() {
                                                    clicked = Some(c.id);
                                                }
                                                r.context_menu(|ui| {
                                                    if ui.button(tr("Delete")).clicked() {
                                                        tree_cmd = Some((
                                                            Command::DeleteCurve { curve: c.id },
                                                            "Deleted (Ctrl+Z brings it back)",
                                                        ));
                                                        ui.close();
                                                    }
                                                    if ui
                                                        .button(tr(if c.visible { "Hide" } else { "Show" }))
                                                        .clicked()
                                                    {
                                                        tree_cmd = Some((
                                                            Command::SetVisible {
                                                                target: ItemId::Curve(c.id),
                                                                visible: !c.visible,
                                                            },
                                                            if c.visible { "Hidden" } else { "Shown" },
                                                        ));
                                                        ui.close();
                                                    }
                                                    if ui.button(tr("Rename")).clicked() {
                                                        self.rename =
                                                            Some((ItemId::Curve(c.id), c.name.clone()));
                                                        ui.close();
                                                    }
                                                });
                                            });
                                        }
                                        if let Some(id) = clicked {
                                            self.clear_selection();
                                            self.curve_sel = Some(id);
                                            if self.tool == Tool::Move {
                                                self.move_curves = vec![id];
                                            }
                                            self.layers_changed = true;
                                        }
                                        if self.doc.curves().is_empty() {
                                            ui.weak(tr("(none)"));
                                        }
                                    });
                                if let Some((cmd, text)) = tree_cmd {
                                    self.run_doc_command(cmd, text);
                                }
                                ui.separator();
                                ui.label(egui::RichText::new(tr("Selection")).strong());
                                if self.selection.is_empty()
                                    && self.region_sel.is_none()
                                    && self.entity_sel.is_none()
                                    && self.edge_sel.is_empty()
                                    && self.body_sel.is_none()
                                    && self.curve_sel.is_none()
                                {
                                    ui.weak(tr("nothing selected"));
                                }
                                if let Some(b) = self.body_sel.and_then(|b| self.doc.body(b)) {
                                    ui.label(trf(
                                        "Body: {} (Delete removes it)",
                                        std::slice::from_ref(&b.name),
                                    ));
                                    ui.weak(tr("F2 or a double click on the name renames it"));
                                }
                                if let Some(b) = self.body_sel {
                                    self.link_sources_ui(ui, b, None);
                                }
                                if self.entity_sel.is_some() {
                                    ui.label(tr("Sketch shape (Delete removes it)"));
                                    self.selection_dims_ui(ui);
                                }
                                if let Some(c) = self.curve_sel.and_then(|c| self.doc.curve(c)) {
                                    ui.label(trf(
                                        "Curve: {} (Delete removes it)",
                                        std::slice::from_ref(&c.name),
                                    ));
                                    if let Some(l) = self.curve_length(c.id) {
                                        ui.label(trf("Length: {} mm", &[format!("{l:.2}")]));
                                    }
                                    let cid = c.id;
                                    self.curve_edit_ui(ui, cid);
                                }
                                if !self.edge_sel.is_empty() {
                                    ui.label(trf("{} edge(s)", &[self.edge_sel.len().to_string()]));
                                    let b = self.edge_sel[0].0;
                                    let sel: Vec<u32> = self
                                        .edge_sel
                                        .iter()
                                        .filter(|(bb, _)| *bb == b)
                                        .map(|(_, e)| *e)
                                        .collect();
                                    let n = self.chain_extra(b, &sel).len();
                                    if n > 0 {
                                        ui.colored_label(
                                    crate::theme::rgb(217, 128, 255),
                                    trf(
                                        "{} more edge(s) continue them tangentially and are treated too",
                                        &[n.to_string()],
                                    ),
                                );
                                    }
                                }
                                for (b, f) in &self.selection {
                                    if let Some(meta) = self.entry(*b).and_then(|m| m.faces.get(*f as usize))
                                    {
                                        ui.label(trf(
                                            "Face #{}: {}",
                                            &[f.to_string(), tr(&format!("{:?}", meta.kind))],
                                        ));
                                        // a hole, a pin or a round face of a body: the diameter for information (a hole or pin is changed in the panel next to it)
                                        if let Some(d) = meta.cyl_diameter.or(meta.disc.map(|(_, d)| d)) {
                                            ui.label(trf("Diameter: {} mm", &[fmt_num(d)]));
                                        }
                                        if meta.kind == FaceKind::Plane {
                                            ui.label(trf(
                                                "Normal: {}, {}, {}",
                                                &[
                                                    format!("{:.2}", meta.normal.x),
                                                    format!("{:.2}", meta.normal.y),
                                                    format!("{:.2}", meta.normal.z),
                                                ],
                                            ));
                                        }
                                        if let Some(ft) = self.doc.body(*b).and_then(|bd| {
                                            bd.features.iter().find(|ft| ft.faces.contains(&FaceId(*f)))
                                        }) {
                                            let name = tr(if ft.kind == rolidium_doc::BlendKind::Round {
                                                "Round"
                                            } else {
                                                "Chamfer"
                                            });
                                            ui.label(trf("{}: {} mm", &[name, fmt_num(ft.size)]));
                                        }
                                    }
                                }
                                let face_body = self.selection.first().map(|s| s.0);
                                if let Some(b) = face_body.filter(|b| self.made_by_profiles(*b)) {
                                    let kind = self.selection.first().and_then(|(sb, f)| self.entry(*sb).and_then(|m| m.faces.get(*f as usize)).map(|m| m.kind));
                                    self.link_sources_ui(ui, b, Some(kind.unwrap_or(FaceKind::Other)));
                                }
                                self.properties_ui(ui);
                                self.edge_distance_ui(ui);
                                if let Some((s, i)) = self.region_sel {
                                    if let Some(r) = self
                                        .regions
                                        .iter()
                                        .find(|r| r.id == s)
                                        .and_then(|r| r.regions.get(i))
                                    {
                                        ui.label(trf("Sketch region #{}", &[(i + 1).to_string()]));
                                        ui.label(trf("Area: {} mm²", &[format!("{:.2}", r.area())]));
                                        ui.label(trf("Holes: {}", &[r.holes.len().to_string()]));
                                    }
                                }
                            });
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                prof!("viewport total", self.viewport(ui));
            });

        let vr = self.viewport_rect;
        self.docks.area = Some(vr);
        prof!("show_dim_panel", self.show_dim_panel(&ctx, vr));
        prof!("move_panel", self.move_panel(&ctx, vr));
        prof!("combine_panel", self.combine_panel(&ctx, vr));
        prof!("link_panel", self.link_panel(&ctx, vr));
        prof!("sketch_pattern_panel", self.sketch_pattern_panel(&ctx, vr));
        prof!("busy_overlay", self.busy_overlay(&ctx, vr));
        prof!("measure_panel", self.measure_panel(&ctx, vr));
        prof!("loft_panel", self.loft_panel(&ctx, vr));
        prof!("helix_panel", self.helix_panel(&ctx, vr));
        prof!("sweep_panel", self.sweep_panel(&ctx, vr));
        prof!("plane_at_panel", self.plane_at_panel(&ctx, vr));
        prof!("plane_free_panel", self.plane_free_panel(&ctx, vr));
        prof!("line3_panel", self.line3_panel(&ctx, vr));
        prof!("join_panel", self.join_panel(&ctx, vr));
        prof!("revolve_panel", self.revolve_panel(&ctx, vr));
        prof!("plane_window", self.plane_window(&ctx));
        prof!("handle_export_dialog", self.handle_export_dialog(&ctx));
        prof!("settings_window", self.settings_window(&ctx));
        prof!("about_window", self.about_window(&ctx));
        prof!("updates_tick", self.updates_tick(&ctx));
        prof!("updates_ui", self.updates_ui(&ctx));
        prof!("feedback_preview", self.feedback_preview(&ctx));
        prof!("feedback_big", self.feedback_big(&ctx));
        prof!("guide_window", self.guide_window(&ctx));
        prof!("rename_window", self.rename_window(&ctx));
        prof!("derived_window", self.derived_window(&ctx));
        prof!("dialogs", self.dialogs(&ctx));
        self.autosave_tick();
        self.probe_tick(&ctx);

        if self.show_debug || self.bench.is_some() {
            let tris: usize = self.meshes.iter().map(|m| m.mesh.triangle_count()).sum();
            let text = format!(
                "{}\nbackend: {}\n{:.0} fps   frame {:.1} ms\ntriangles: {}\nlast kernel op: {}",
                self.adapter,
                self.backend,
                self.fps_smooth,
                self.frame_ms,
                tris,
                self.last_op_ms
                    .map_or("-".to_string(), |v| format!("{v:.1} ms (incl. meshing)"))
            );
            let text = format!(
                "{text}\nwork of the window per frame: {:.1} ms (viewport {:.1}, pointer {:.1})\nthe three dearest parts (ms): {}\nceiling: {}\nhand-over (as requested): {}\nframes drawn: {}",
                self.work_ms[0],
                self.work_ms[1],
                self.work_ms[2],
                crate::prof::top3_text(),
                match self.settings.fps_cap.as_str() {
                    "monitor" => "by the monitor".to_string(),
                    "none" => "none".to_string(),
                    n => format!("{n} fps"),
                },
                crate::settings::PRESENT.get().map_or("-", |s| s.as_str()),
                crate::startup::frame_count()
            );
            // who asked for the last frame (a frame without a reason is a bug: nothing should draw at rest)
            let causes: Vec<String> = ctx
                .repaint_causes()
                .iter()
                .take(3)
                .map(|c| {
                    format!(
                        "{}:{}",
                        c.file.rsplit(['/', '\\']).next().unwrap_or(c.file),
                        c.line
                    )
                })
                .collect();
            let text = format!(
                "{text}\nlast frame asked for by: {}\nview: pointer moved {}, pick running {}, scene sent {}, camera or size changed {}",
                if causes.is_empty() {
                    "-".into()
                } else {
                    causes.join(", ")
                },
                self.repaint_why.0,
                self.repaint_why.1,
                self.repaint_why.2,
                self.repaint_why.3
            );
            // inside the model area (never over the bars or the columns): default in the lower left corner, dragged by the mouse
            let vr = self.viewport_rect;
            let [ox, oy] = self.settings.diag_pos.unwrap_or([8.0, 8.0]);
            let size = self.diag_size;
            let max_x = (vr.width() - size.x).max(0.0);
            let max_y = (vr.height() - size.y).max(0.0);
            let pos = Pos2::new(
                vr.left() + ox.clamp(0.0, max_x),
                vr.bottom() - size.y - oy.clamp(0.0, max_y),
            );
            const W: f32 = 520.0;
            let mut close = false;
            let mut drag = Vec2::ZERO;
            let mut released = false;
            #[allow(deprecated)]
            let area = egui::Area::new(egui::Id::new("debug_overlay"))
                .fixed_pos(pos)
                .constrain_to(vr)
                .show(&ctx, |ui| {
                    egui::Frame::NONE
                        .fill(Color32::from_black_alpha(200))
                        .show(ui, |ui| {
                            ui.set_width(W);
                            // the bar: the move icon (drag here), the title and the cross that closes the panel (as F12 does)
                            let (bar, bar_resp) =
                                ui.allocate_exact_size(Vec2::new(W, 22.0), egui::Sense::click_and_drag());
                            let ink = crate::theme::rgb(140, 255, 140);
                            ui.painter().rect_filled(bar, 0.0, Color32::from_white_alpha(28));
                            let c = Pos2::new(bar.left() + 14.0, bar.center().y);
                            let st = Stroke::new(1.5, ink);
                            for d in [Vec2::X, Vec2::Y] {
                                ui.painter().line_segment([c - d * 7.0, c + d * 7.0], st);
                                for sgn in [-1.0_f32, 1.0] {
                                    let tip = c + d * 7.0 * sgn;
                                    let perp = Vec2::new(d.y, d.x);
                                    ui.painter()
                                        .line_segment([tip, tip - d * 3.0 * sgn + perp * 2.5], st);
                                    ui.painter()
                                        .line_segment([tip, tip - d * 3.0 * sgn - perp * 2.5], st);
                                }
                            }
                            ui.painter().text(
                                Pos2::new(bar.left() + 30.0, bar.center().y),
                                egui::Align2::LEFT_CENTER,
                                "Diagnostics (F12)",
                                egui::FontId::monospace(12.0),
                                ink,
                            );
                            let x_rect = Rect::from_center_size(
                                Pos2::new(bar.right() - 14.0, bar.center().y),
                                Vec2::splat(18.0),
                            );
                            let x_resp = ui.interact(
                                x_rect,
                                egui::Id::new("debug_overlay_close"),
                                egui::Sense::click(),
                            );
                            if x_resp.hovered() {
                                ui.painter()
                                    .rect_filled(x_rect, 3.0, Color32::from_white_alpha(40));
                            }
                            for (a, b) in [((-1.0, -1.0), (1.0, 1.0)), ((-1.0, 1.0), (1.0, -1.0))] {
                                ui.painter().line_segment(
                                    [
                                        x_rect.center() + Vec2::new(a.0, a.1) * 4.5,
                                        x_rect.center() + Vec2::new(b.0, b.1) * 4.5,
                                    ],
                                    st,
                                );
                            }
                            if x_resp.clicked() {
                                close = true;
                            }
                            if bar_resp.dragged() && !x_resp.hovered() {
                                drag = bar_resp.drag_delta();
                            }
                            released = bar_resp.drag_stopped();
                            if bar_resp.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                            }
                            // the lines: a fixed width (a long line is cut, the panel does not change its width with the text)
                            ui.add_space(3.0);
                            for line in text.lines() {
                                ui.horizontal(|ui| {
                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::Label::new(egui::RichText::new(line).monospace().color(ink))
                                            .truncate(),
                                    );
                                });
                            }
                            ui.add_space(4.0);
                        });
                });
            self.diag_size = area.response.rect.size();
            if close {
                self.show_debug = false;
            }
            if drag != Vec2::ZERO {
                let [x, y] = self.settings.diag_pos.unwrap_or([8.0, 8.0]);
                self.settings.diag_pos =
                    Some([(x + drag.x).clamp(0.0, max_x), (y - drag.y).clamp(0.0, max_y)]);
            }
            if released {
                self.settings.save();
            }
        }

        if !self.first_frame_done && !self.meshes.is_empty() {
            self.first_frame_done = true;
            let ms = self.started.elapsed().as_secs_f64() * 1000.0;
            log(&format!("first frame with content after {ms:.0} ms"));
            self.first_frame_ms_value = ms;
            if let Some(b) = &mut self.bench {
                b.first_frame_ms = ms;
            }
            if self.exit_after_first_frame {
                println!("time to first frame: {ms:.0} ms");
                if std::env::var_os("ROLIDIUM_MEASURE_VIEW").is_some()
                    || BENCH_RUN.load(std::sync::atomic::Ordering::Relaxed)
                {
                    // the frames while the view turns, then the small operations (see docs/DECISIONS.md D38, D39)
                    self.measure_view = Some((0, Vec::new(), Instant::now()));
                    self.exit_after_first_frame = false;
                } else {
                    crate::startup::finish();
                    std::process::exit(0);
                }
            }
        }
        if let Some((n, times, last)) = &mut self.measure_view {
            times.push(last.elapsed().as_secs_f64() * 1e3);
            *last = Instant::now();
            *n += 1;
            let bench = BENCH_RUN.load(std::sync::atomic::Ordering::Relaxed);
            let started = self.measure_started.get_or_insert_with(Instant::now);
            // `--bench` turns the view for 5 seconds; the old measuring (ROLIDIUM_MEASURE_VIEW) for 60 frames
            let done = if bench {
                started.elapsed().as_secs_f64() >= 5.0
            } else {
                *n >= 60
            };
            if done {
                let seconds = started.elapsed().as_secs_f64();
                let mut t = times.split_off(2.min(times.len().saturating_sub(1)));
                t.sort_by(|a, b| a.total_cmp(b));
                let text = format!(
                    "frame while turning: median {:.1} ms, worst {:.1} ms ({} frames in {:.1} s)",
                    t[t.len() / 2],
                    t[t.len() - 1],
                    t.len(),
                    seconds
                );
                println!("{text}");
                let adapter = self
                    .adapter_description
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches("name: ")
                    .to_string();
                println!("graphics adapter: {adapter}");
                println!("graphics interface (backend): {}", self.backend);
                log(&format!(
                    "bench: first frame with content after {:.0} ms; {text}; adapter {}; backend {}; {} bodies",
                    self.first_frame_ms_value,
                    adapter,
                    self.backend,
                    self.doc.bodies().len()
                ));
                if BENCH_RUN.load(std::sync::atomic::Ordering::Relaxed) && self.start_edit_bench() {
                    self.measure_view = None;
                    return;
                }
                crate::startup::finish();
                std::process::exit(0);
            }
            self.camera.orbit(6.0, 0.0);
            ctx.request_repaint();
        }
    }
}

/// Set by `rolidium --bench file`: after the first frame the view turns for 5 seconds and the frame times are reported.
pub static BENCH_RUN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Japanese (and other CJK) characters come from Noto Sans JP (SIL Open Font License 1.1), used only for the characters
/// the built-in fonts do not have, so that file names in the file dialog are shown correctly in every language.
fn install_fallback_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "noto-sans-jp".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/NotoSansJP-Regular.otf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("noto-sans-jp".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// Which bodies are tinted as a whole, as `(slot, kind)`: kind 1 selected, 2 target of Combine, 3 tool of Combine, 4 under the pointer.
/// Move and Combine always take the whole body, never a single face.
#[allow(clippy::too_many_arguments)]
fn marks_for(
    tool: Tool,
    move_sel: &[BodyId],
    dragging: bool,
    hovered: Option<BodyId>,
    target: Option<BodyId>,
    tools: &[BodyId],
    body_sel: Option<BodyId>,
    slot_of: impl Fn(BodyId) -> Option<u32>,
) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut add = |b: BodyId, kind: u32| {
        if let Some(s) = slot_of(b) {
            out.push((s, kind));
        }
    };
    match tool {
        Tool::Move => {
            for b in move_sel {
                add(*b, 1);
            }
            if !dragging {
                if let Some(b) = hovered {
                    add(b, 4);
                }
            }
        }
        Tool::Combine => {
            if let Some(t) = target {
                add(t, 2);
            }
            for t in tools {
                add(*t, 3);
            }
            if let Some(b) = hovered {
                add(b, 4);
            }
        }
        _ => {
            if let Some(b) = body_sel {
                add(b, 1);
            }
        }
    }
    out.truncate(16);
    out
}

/// The box around all meshes, from the boxes of their faces (no walk over all vertices after every change).
fn scene_bounds(meshes: &[MeshEntry], lo: &mut Vec3, hi: &mut Vec3) {
    for m in meshes {
        for f in m.faces.iter() {
            if f.bounds.0.x <= f.bounds.1.x {
                *lo = lo.min(f.bounds.0);
                *hi = hi.max(f.bounds.1);
            }
        }
    }
}

#[cfg(test)]
mod marks_tests {
    use super::*;

    #[test]
    fn the_presentation_mode_leaves_out_the_card_and_the_software_label_only() {
        assert_eq!(status_labels(false, true), (true, true));
        assert_eq!(status_labels(false, false), (true, false));
        assert_eq!(status_labels(true, true), (false, false));
        assert_eq!(status_labels(true, false), (false, false));
    }

    fn slot(b: BodyId) -> Option<u32> {
        // body ids 10, 20, 30 live in slots 0, 1, 2; body 99 has no mesh
        [10, 20, 30].iter().position(|x| *x == b).map(|i| i as u32)
    }

    #[test]
    fn move_tints_the_selected_bodies_and_the_hovered_one_as_a_whole() {
        let m = marks_for(Tool::Move, &[10, 20], false, Some(30), None, &[], None, slot);
        assert_eq!(m, vec![(0, 1), (1, 1), (2, 4)]);
        // while dragging the hover tint is off
        let m = marks_for(Tool::Move, &[10], true, Some(30), None, &[], None, slot);
        assert_eq!(m, vec![(0, 1)]);
    }

    #[test]
    fn combine_tints_target_tools_and_hover() {
        let m = marks_for(Tool::Combine, &[], false, Some(30), Some(10), &[20], None, slot);
        assert_eq!(m, vec![(0, 2), (1, 3), (2, 4)]);
        // a body without a mesh is skipped
        let m = marks_for(Tool::Combine, &[], false, None, Some(99), &[20], None, slot);
        assert_eq!(m, vec![(1, 3)]);
    }

    #[test]
    fn other_tools_tint_only_the_selected_body() {
        let m = marks_for(
            Tool::Select,
            &[10],
            false,
            Some(30),
            Some(10),
            &[20],
            Some(20),
            slot,
        );
        assert_eq!(m, vec![(1, 1)]);
        let m = marks_for(Tool::Pull, &[], false, None, None, &[], None, slot);
        assert!(m.is_empty());
    }
}
