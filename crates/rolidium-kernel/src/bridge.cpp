// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
// Open CASCADE bridge. Every exported function takes a global lock (OCCT meshing mutates
// shared TShapes) and converts OCCT/STL exceptions into std::runtime_error, which cxx
// turns into a Rust `Err`. No exception may cross the FFI boundary.

#include "bridge.h"
#include "rolidium-kernel/src/occt.rs.h"

#include <cmath>
#include <cstdlib>
#include <sstream>
#include <string>

#include <BOPAlgo_ArgumentAnalyzer.hxx>
#include <IMeshTools_Parameters.hxx>
#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressRange.hxx>
#include <atomic>
#include <chrono>
#include <string>
#include <vector>
#include <BRepAdaptor_CompCurve.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepBuilderAPI_TransitionMode.hxx>
#include <BRepTools.hxx>
#include <BRepLProp_SLProps.hxx>
#include <GCPnts_QuasiUniformDeflection.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <Geom2d_Curve.hxx>
#include <TopTools_IndexedDataMapOfShapeListOfShape.hxx>
#include <BRepAlgoAPI_BooleanOperation.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Defeaturing.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib_ToolTriangulatedShape.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <TopoDS_Wire.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <gp_Ax1.hxx>
#include <gp_Vec.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Bnd_Box.hxx>
#include <BinTools.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Circle.hxx>
#include <GeomAbs_CurveType.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPControl_Reader.hxx>
#include <StepRepr_RepresentationItem.hxx>
#include <TCollection_HAsciiString.hxx>
#include <TransferBRep.hxx>
#include <TransferBRep_ShapeMapper.hxx>
#include <Transfer_FinderProcess.hxx>
#include <XSControl_TransferReader.hxx>
#include <XSControl_TransferWriter.hxx>
#include <XSControl_WorkSession.hxx>
#include <STEPControl_Writer.hxx>
#include <ShapeFix_Solid.hxx>
#include <ShapeFix_Face.hxx>
#include <Message.hxx>
#include <Message_Messenger.hxx>
#include <Message_Printer.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <OSD_ThreadPool.hxx>
#include <atomic>
#include <TopExp_Explorer.hxx>
#include <TopTools_ListOfShape.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

namespace rol {

namespace {

std::recursive_mutex g_lock;

[[noreturn]] void fail(const std::string& what) { throw std::runtime_error(what); }

template <bool Lock, typename F>
auto guarded_impl(const char* name, F&& f) -> decltype(f()) {
    std::unique_lock<std::recursive_mutex> lock(g_lock, std::defer_lock);
    if (Lock) lock.lock();
    static const bool quiet = [] {
        // Open CASCADE prints diagnostics to stdout; we report errors through Results instead.
        Message::DefaultMessenger()->RemovePrinters(STANDARD_TYPE(Message_Printer));
        return true;
    }();
    (void)quiet;
    try {
        return f();
    } catch (const Standard_Failure& e) {
        const char* m = e.GetMessageString();
        fail(std::string(name) + ": Open CASCADE error: " + (m && *m ? m : "unspecified failure"));
    } catch (const std::exception& e) {
        fail(std::string(name) + ": " + e.what());
    } catch (...) {
        fail(std::string(name) + ": unknown native error");
    }
}

// Every native call runs one at a time, except the ones that only read their own shape (or write only what belongs to it) and are
// made to run for many bodies at once: those use `guarded_free`. The caller never works on two shapes that share data at the same moment.
template <typename F>
auto guarded(const char* name, F&& f) -> decltype(f()) {
    return guarded_impl<true>(name, std::forward<F>(f));
}
template <typename F>
auto guarded_free(const char* name, F&& f) -> decltype(f()) {
    return guarded_impl<false>(name, std::forward<F>(f));
}


// ---- cancelling a long operation ----
// A flag that the application sets from another thread; the long Open CASCADE algorithms (booleans, rounds, lofts, sweeps,
// meshing, STEP export) poll it through a progress indicator and stop with a "cancelled" error.
static std::atomic<bool> g_cancel{false};

// ---- timing of the stages of an operation (for the `times` table of the command line tool) ----
static thread_local std::vector<std::pair<std::string, double>> g_stages;

// Number of threads Open CASCADE may use inside one operation (0: its default, one per core). 1 turns the parallel modes off.
static std::atomic<int> g_threads{0};
static bool run_parallel() { return g_threads.load() != 1; }

struct StageTimer {
    const char* name;
    std::chrono::steady_clock::time_point t0 = std::chrono::steady_clock::now();
    explicit StageTimer(const char* n) : name(n) {}
    ~StageTimer() {
        double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
        g_stages.emplace_back(name, ms);
    }
};


class CancelIndicator : public Message_ProgressIndicator {
public:
    bool UserBreak() override { return g_cancel.load(); }
    void Show(const Message_ProgressScope&, const bool) override {}
};

static Handle(CancelIndicator) g_indicator = new CancelIndicator();

static Message_ProgressRange progress() { return g_indicator->Start(); }

static void check_cancelled() {
    if (g_cancel.load()) fail("cancelled");
}

/// a minus b, cancellable.
static TopoDS_Shape cut_cancellable(const TopoDS_Shape& a, const TopoDS_Shape& b, const char* what) {
    BRepAlgoAPI_Cut cut;
    TopTools_ListOfShape args, tools;
    args.Append(a);
    tools.Append(b);
    cut.SetArguments(args);
    cut.SetTools(tools);
    cut.SetRunParallel(run_parallel());
    {
        StageTimer st("boolean cut");
        cut.Build(progress());
    }
    check_cancelled();
    if (!cut.IsDone() || cut.HasErrors()) fail(what);
    return cut.Shape();
}

std::shared_ptr<Shape> wrap(const TopoDS_Shape& s) {
    if (s.IsNull()) fail("operation produced a null shape");
    return std::make_shared<Shape>(s);
}

const TopoDS_Face& face_at(const Shape& sh, uint32_t i) {
    if (i >= (uint32_t)sh.faces.Extent()) fail("face index out of range");
    return TopoDS::Face(sh.faces.FindKey((int)i + 1));
}
const TopoDS_Edge& edge_at(const Shape& sh, uint32_t i) {
    if (i >= (uint32_t)sh.edges.Extent()) fail("edge index out of range");
    return TopoDS::Edge(sh.edges.FindKey((int)i + 1));
}

/// Picks the first solid out of a result, or the shape itself.
TopoDS_Shape first_solid(const TopoDS_Shape& s) {
    if (s.ShapeType() == TopAbs_SOLID) return s;
    TopExp_Explorer ex(s, TopAbs_SOLID);
    if (!ex.More()) return s;
    TopoDS_Shape first = ex.Current();
    ex.Next();
    if (ex.More()) {
        // Several solids: keep the compound, the caller decides what to do.
        return s;
    }
    return first;
}

/// Collects (arg, src_face, new_face) triples from an OCCT history.
void collect_history(const Handle(BRepTools_History)& hist, const Shape& src, uint32_t arg,
                     const Shape& result, rust::Vec<uint32_t>& out) {
    for (int i = 1; i <= src.faces.Extent(); ++i) {
        const TopoDS_Shape& f = src.faces.FindKey(i);
        bool any = false;
        auto add = [&](const TopTools_ListOfShape& l) {
            for (const TopoDS_Shape& g : l) {
                int idx = result.faces.FindIndex(g);
                if (idx > 0) {
                    out.push_back(arg);
                    out.push_back((uint32_t)(i - 1));
                    out.push_back((uint32_t)(idx - 1));
                    any = true;
                }
            }
        };
        if (!hist.IsNull()) {
            if (hist->IsRemoved(f)) continue;
            add(hist->Modified(f));
            add(hist->Generated(f));
        }
        if (!any) {
            int idx = result.faces.FindIndex(f);
            if (idx > 0) {
                out.push_back(arg);
                out.push_back((uint32_t)(i - 1));
                out.push_back((uint32_t)(idx - 1));
            }
        }
    }
}

OpResult make_result(const TopoDS_Shape& s, const Handle(BRepTools_History)& hist, const Shape* a, const Shape* b) {
    OpResult r;
    r.shape = wrap(s);
    if (a) collect_history(hist, *a, 0, *r.shape, r.history);
    if (b) collect_history(hist, *b, 1, *r.shape, r.history);
    return r;
}

}  // namespace

Shape::Shape(const TopoDS_Shape& sh) : s(sh) {
    TopExp::MapShapes(s, TopAbs_FACE, faces);
    TopExp::MapShapes(s, TopAbs_EDGE, edges);
}

std::shared_ptr<Shape> make_box(double x0, double y0, double z0, double x1, double y1, double z1) {
    return guarded("make_box", [&] {
        if (!(x1 > x0 && y1 > y0 && z1 > z0)) fail("box must have positive size");
        BRepPrimAPI_MakeBox mk(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1));
        mk.Build();
        if (!mk.IsDone()) fail("box construction failed");
        return wrap(mk.Shape());
    });
}

std::shared_ptr<Shape> make_cylinder(double radius, double height, std::array<double, 3> o, std::array<double, 3> a) {
    return guarded("make_cylinder", [&] {
        if (!(radius > 0 && height > 0)) fail("cylinder must have positive size");
        gp_Dir dir(a[0], a[1], a[2]);
        BRepPrimAPI_MakeCylinder mk(gp_Ax2(gp_Pnt(o[0], o[1], o[2]), dir), radius, height);
        mk.Build();
        if (!mk.IsDone()) fail("cylinder construction failed");
        return wrap(mk.Shape());
    });
}

std::shared_ptr<Shape> make_planar_face(std::array<double, 9> pl, rust::Slice<const double> prof) {
    return guarded("make_planar_face", [&] {
        gp_Pnt o(pl[0], pl[1], pl[2]);
        gp_Dir n(pl[3], pl[4], pl[5]);
        gp_Dir xd(pl[6], pl[7], pl[8]);
        gp_Ax3 ax(o, n, xd);
        gp_Dir yd = ax.YDirection();
        auto P = [&](double u, double v) {
            return gp_Pnt(o.X() + u * xd.X() + v * yd.X(), o.Y() + u * xd.Y() + v * yd.Y(),
                          o.Z() + u * xd.Z() + v * yd.Z());
        };
        std::vector<TopoDS_Wire> wires;
        size_t i = 0;
        const size_t n_all = prof.size();
        while (i < n_all) {
            // Loop header: marker 0, then segment count.
            if (prof[i] != 0.0 || i + 1 >= n_all) fail("malformed profile");
            size_t count = (size_t)prof[i + 1];
            i += 2;
            BRepBuilderAPI_MakeWire mw;
            for (size_t s = 0; s < count; ++s) {
                if (i >= n_all) fail("malformed profile");
                int kind = (int)prof[i];
                TopoDS_Edge e;
                if (kind == 1) {
                    if (i + 5 > n_all) fail("malformed profile");
                    gp_Pnt a = P(prof[i + 1], prof[i + 2]), b = P(prof[i + 3], prof[i + 4]);
                    if (a.Distance(b) < 1e-9) fail("zero-length line in profile");
                    e = BRepBuilderAPI_MakeEdge(a, b);
                    i += 5;
                } else if (kind == 2) {
                    if (i + 7 > n_all) fail("malformed profile");
                    gp_Pnt a = P(prof[i + 1], prof[i + 2]), m = P(prof[i + 3], prof[i + 4]), b = P(prof[i + 5], prof[i + 6]);
                    GC_MakeArcOfCircle arc(a, m, b);
                    if (!arc.IsDone()) fail("degenerate arc in profile");
                    e = BRepBuilderAPI_MakeEdge(arc.Value());
                    i += 7;
                } else if (kind == 3) {
                    if (i + 4 > n_all) fail("malformed profile");
                    if (!(prof[i + 3] > 0)) fail("circle radius must be positive");
                    gp_Circ c(gp_Ax2(P(prof[i + 1], prof[i + 2]), n), prof[i + 3]);
                    e = BRepBuilderAPI_MakeEdge(c);
                    i += 4;
                } else {
                    fail("unknown profile segment kind");
                }
                mw.Add(e);
                if (!mw.IsDone()) fail("profile segments do not connect");
            }
            if (!mw.IsDone()) fail("empty profile loop");
            if (!mw.Wire().Closed()) fail("profile loop is not closed");
            wires.push_back(mw.Wire());
        }
        if (wires.empty()) fail("empty profile");
        BRepBuilderAPI_MakeFace mf(gp_Pln(ax), wires[0]);
        if (!mf.IsDone()) fail("cannot build a face from the outer loop");
        for (size_t k = 1; k < wires.size(); ++k) {
            TopoDS_Wire w = wires[k];
            mf.Add(w);
        }
        TopoDS_Face f = mf.Face();
        ShapeFix_Face fixer(f);
        fixer.FixOrientation();
        fixer.Perform();
        f = fixer.Face();
        // Inner loops must run opposite to the outer one: let OCCT fix orientation.
        return wrap(f);
    });
}

std::shared_ptr<Shape> prism(const Shape& shape, std::array<double, 3> v) {
    return guarded("prism", [&] {
        gp_Vec vec(v[0], v[1], v[2]);
        if (vec.Magnitude() < 1e-9) fail("extrusion length is zero");
        BRepPrimAPI_MakePrism mk(shape.s, vec);
        mk.Build();
        if (!mk.IsDone()) fail("extrusion failed");
        return wrap(mk.Shape());
    });
}

std::shared_ptr<Shape> revolve(const Shape& shape, std::array<double, 3> p, std::array<double, 3> d, double angle) {
    return guarded("revolve", [&] {
        gp_Vec dir(d[0], d[1], d[2]);
        if (dir.Magnitude() < 1e-9) fail("the axis has no direction");
        BRepPrimAPI_MakeRevol mk(shape.s, gp_Ax1(gp_Pnt(p[0], p[1], p[2]), gp_Dir(dir)), angle);
        mk.Build();
        if (!mk.IsDone()) fail("revolve failed");
        return wrap(mk.Shape());
    });
}

/// Rigid motion: rotation by `angle` about the axis through `pivot` with direction `axis`, then translation.
OpResult transform_shape(const Shape& sh, std::array<double, 3> pivot, std::array<double, 3> axis, double angle,
                         std::array<double, 3> t) {
    return guarded("transform", [&] {
        gp_Trsf rot;
        gp_Vec ad(axis[0], axis[1], axis[2]);
        if (std::fabs(angle) > 1e-12 && ad.Magnitude() > 1e-9) {
            rot.SetRotation(gp_Ax1(gp_Pnt(pivot[0], pivot[1], pivot[2]), gp_Dir(ad)), angle);
        }
        gp_Trsf tr;
        tr.SetTranslation(gp_Vec(t[0], t[1], t[2]));
        gp_Trsf all = tr * rot;
        BRepBuilderAPI_Transform mk(sh.s, all, true);
        if (!mk.IsDone()) fail("the move failed");
        OpResult r;
        r.shape = wrap(mk.Shape());
        for (int i = 1; i <= sh.faces.Extent(); ++i) {
            const TopoDS_Shape& f = sh.faces.FindKey(i);
            for (const TopoDS_Shape& g : mk.Modified(f)) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); }
            }
        }
        return r;
    });
}

/// Mirror image about the plane through `point` with the normal `normal` (a new shape; the faces of the result keep the order of the faces of the original).
std::shared_ptr<Shape> compound_append(const Shape& acc, const Shape& s, bool first) {
    return guarded("compound", [&] {
        BRep_Builder bb;
        TopoDS_Compound comp;
        bb.MakeCompound(comp);
        if (!first) {
            for (TopExp_Explorer ex(acc.s, TopAbs_SOLID); ex.More(); ex.Next()) bb.Add(comp, ex.Current());
        }
        for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next()) bb.Add(comp, ex.Current());
        return wrap(comp);
    });
}

OpResult mirror_shape(const Shape& sh, std::array<double, 3> point, std::array<double, 3> normal) {
    return guarded("mirror", [&] {
        gp_Vec nv(normal[0], normal[1], normal[2]);
        if (nv.Magnitude() < 1e-9) fail("the mirror plane has no normal");
        gp_Trsf tr;
        tr.SetMirror(gp_Ax2(gp_Pnt(point[0], point[1], point[2]), gp_Dir(nv)));
        BRepBuilderAPI_Transform mk(sh.s, tr, true);
        if (!mk.IsDone()) fail("the mirroring failed");
        OpResult r;
        r.shape = wrap(mk.Shape());
        for (int i = 1; i <= sh.faces.Extent(); ++i) {
            const TopoDS_Shape& f = sh.faces.FindKey(i);
            for (const TopoDS_Shape& g : mk.Modified(f)) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); }
            }
        }
        return r;
    });
}

uint32_t solid_count(const Shape& s) {
    uint32_t n = 0;
    for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next()) ++n;
    return n;
}
bool point_in_solid(const Shape& sh, std::array<double, 3> p) {
    return guarded("point_in_solid", [&] {
        BRepClass3d_SolidClassifier cl(sh.s, gp_Pnt(p[0], p[1], p[2]), 1e-6);
        return cl.State() == TopAbs_IN;
    });
}

bool point_outside(const Shape& sh, std::array<double, 3> p) {
    return guarded("point_outside", [&] {
        BRepClass3d_SolidClassifier cl(sh.s, gp_Pnt(p[0], p[1], p[2]), 1e-6);
        return cl.State() == TopAbs_OUT;
    });
}

std::shared_ptr<Shape> solid_at(const Shape& s, uint32_t index) {
    return guarded("solid_at", [&] {
        uint32_t i = 0;
        for (TopExp_Explorer ex(s.s, TopAbs_SOLID); ex.More(); ex.Next(), ++i) {
            if (i == index) return wrap(ex.Current());
        }
        fail("solid index out of range");
    });
}

uint32_t face_count(const Shape& s) { return (uint32_t)s.faces.Extent(); }
uint32_t edge_count(const Shape& s) { return (uint32_t)s.edges.Extent(); }

FaceInfo face_info(const Shape& sh, uint32_t index) {
    return guarded("face_info", [&] {
        const TopoDS_Face& f = face_at(sh, index);
        BRepAdaptor_Surface sf(f);
        FaceInfo fi{};
        GProp_GProps gp;
        BRepGProp::SurfaceProperties(f, gp);
        fi.area = gp.Mass();
        int inner = 0;
        TopoDS_Wire outer = BRepTools::OuterWire(f);
        for (TopExp_Explorer ex(f, TopAbs_WIRE); ex.More(); ex.Next())
            if (!ex.Current().IsSame(outer)) ++inner;
        fi.inner_wires = (uint32_t)inner;
        bool rev = f.Orientation() == TopAbs_REVERSED;
        switch (sf.GetType()) {
            case GeomAbs_Plane: {
                fi.kind = 0;
                gp_Pln p = sf.Plane();
                // The natural normal of the surface is X x Y of its placement; for a plane with a left-handed placement
                // (some blends make them) that is the opposite of Axis().Direction().
                gp_Dir n = p.Position().XDirection().Crossed(p.Position().YDirection());
                if (rev) n.Reverse();
                fi.nx = n.X(); fi.ny = n.Y(); fi.nz = n.Z();
                gp_Pnt o = p.Location();
                fi.ox = o.X(); fi.oy = o.Y(); fi.oz = o.Z();
                break;
            }
            case GeomAbs_Cylinder: {
                fi.kind = 1;
                gp_Cylinder c = sf.Cylinder();
                gp_Dir d = c.Axis().Direction();
                fi.nx = d.X(); fi.ny = d.Y(); fi.nz = d.Z();
                gp_Pnt o = c.Location();
                fi.ox = o.X(); fi.oy = o.Y(); fi.oz = o.Z();
                fi.radius = c.Radius();
                break;
            }
            case GeomAbs_Cone: {
                fi.kind = 2;
                gp_Cone c = sf.Cone();
                gp_Dir d = c.Axis().Direction();
                fi.nx = d.X(); fi.ny = d.Y(); fi.nz = d.Z();
                gp_Pnt o = c.Location();
                fi.ox = o.X(); fi.oy = o.Y(); fi.oz = o.Z();
                fi.radius = c.RefRadius();
                break;
            }
            case GeomAbs_Sphere: {
                fi.kind = 3;
                gp_Sphere s = sf.Sphere();
                gp_Pnt o = s.Location();
                fi.ox = o.X(); fi.oy = o.Y(); fi.oz = o.Z();
                fi.radius = s.Radius();
                break;
            }
            case GeomAbs_Torus: {
                fi.kind = 4;
                gp_Torus t = sf.Torus();
                gp_Dir d = t.Axis().Direction();
                fi.nx = d.X(); fi.ny = d.Y(); fi.nz = d.Z();
                gp_Pnt o = t.Location();
                fi.ox = o.X(); fi.oy = o.Y(); fi.oz = o.Z();
                fi.radius = t.MajorRadius();
                break;
            }
            default: fi.kind = 5;
        }
        return fi;
    });
}

EdgeInfo edge_info(const Shape& sh, uint32_t index) {
    return guarded("edge_info", [&] {
        const TopoDS_Edge& e = edge_at(sh, index);
        BRepAdaptor_Curve c(e);
        EdgeInfo ei{};
        gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
        ei.sx = a.X(); ei.sy = a.Y(); ei.sz = a.Z();
        ei.ex = b.X(); ei.ey = b.Y(); ei.ez = b.Z();
        GProp_GProps gp;
        BRepGProp::LinearProperties(e, gp);
        ei.length = gp.Mass();
        switch (c.GetType()) {
            case GeomAbs_Line: ei.kind = 0; break;
            case GeomAbs_Circle: ei.kind = 1; ei.radius = c.Circle().Radius(); break;
            default: ei.kind = 2;
        }
        return ei;
    });
}

rust::Vec<double> edge_geom(const Shape& sh, uint32_t index) {
    return guarded("edge_geom", [&] {
        const TopoDS_Edge& e = edge_at(sh, index);
        BRepAdaptor_Curve c(e);
        rust::Vec<double> out;
        gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
        out.push_back(m.X()); out.push_back(m.Y()); out.push_back(m.Z());
        if (c.GetType() == GeomAbs_Circle) {
            gp_Circ ci = c.Circle();
            gp_Pnt o = ci.Location();
            gp_Dir n = ci.Axis().Direction();
            out.push_back(o.X()); out.push_back(o.Y()); out.push_back(o.Z());
            out.push_back(n.X()); out.push_back(n.Y()); out.push_back(n.Z());
        } else {
            for (int i = 0; i < 6; ++i) out.push_back(0.0);
        }
        return out;
    });
}

// Start point, start tangent, end point, end tangent of an edge (tangents are unit vectors in the direction of the curve parameter).
rust::Vec<double> edge_ends(const Shape& sh, uint32_t index) {
    return guarded("edge_ends", [&] {
        const TopoDS_Edge& e = edge_at(sh, index);
        BRepAdaptor_Curve c(e);
        rust::Vec<double> out;
        for (double t : {c.FirstParameter(), c.LastParameter()}) {
            gp_Pnt p; gp_Vec d;
            c.D1(t, p, d);
            double len = d.Magnitude();
            if (len > 1e-12) d.Divide(len);
            out.push_back(p.X()); out.push_back(p.Y()); out.push_back(p.Z());
            out.push_back(d.X()); out.push_back(d.Y()); out.push_back(d.Z());
        }
        return out;
    });
}

// True when the faces of the shape cut through each other (a damaged result of a round or chamfer, for example).
bool has_self_intersections(const Shape& sh) {
    try {
        std::lock_guard<std::recursive_mutex> lock(g_lock);
        BOPAlgo_ArgumentAnalyzer an;
        an.SetShape1(sh.s);
        an.SetRunParallel(run_parallel());
        an.SelfInterMode() = true;
        an.SmallEdgeMode() = false;
        an.RebuildFaceMode() = false;
        an.TangentMode() = false;
        an.MergeVertexMode() = false;
        an.MergeEdgeMode() = false;
        an.ContinuityMode() = false;
        an.CurveOnSurfaceMode() = false;
        an.Perform();
        return an.HasFaulty();
    } catch (...) {
        return false;
    }
}

std::shared_ptr<Shape> face_shape(const Shape& sh, uint32_t index) {
    return guarded("face_shape", [&] { return wrap(face_at(sh, index)); });
}

void face_outline(const Shape& sh, uint32_t index, double deflection, rust::Vec<float>& pts, rust::Vec<uint32_t>& offs) {
    guarded("face_outline", [&] {
        const TopoDS_Face& f = face_at(sh, index);
        uint32_t n = 0;
        for (TopExp_Explorer ex(f, TopAbs_EDGE); ex.More(); ex.Next()) {
            BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
            GCPnts_TangentialDeflection tg(c, 0.3, deflection);
            offs.push_back(n);
            for (int i = 1; i <= tg.NbPoints(); ++i) {
                gp_Pnt p = tg.Value(i);
                pts.push_back((float)p.X()); pts.push_back((float)p.Y()); pts.push_back((float)p.Z());
                ++n;
            }
        }
        offs.push_back(n);
        return 0;
    });
}

rust::Vec<uint32_t> face_edges(const Shape& sh, uint32_t index) {
    return guarded("face_edges", [&] {
        const TopoDS_Face& f = face_at(sh, index);
        rust::Vec<uint32_t> out;
        TopTools_IndexedMapOfShape m;
        TopExp::MapShapes(f, TopAbs_EDGE, m);
        for (int i = 1; i <= m.Extent(); ++i) {
            int idx = sh.edges.FindIndex(m.FindKey(i));
            if (idx > 0) out.push_back((uint32_t)(idx - 1));
        }
        return out;
    });
}

OpResult boolean_op(const Shape& a, const Shape& b, uint32_t op, bool unify) {
    return guarded("boolean", [&] {
        std::unique_ptr<BRepAlgoAPI_BooleanOperation> alg;
        if (op == 0) alg.reset(new BRepAlgoAPI_Fuse());
        else if (op == 1) alg.reset(new BRepAlgoAPI_Cut());
        else if (op == 2) alg.reset(new BRepAlgoAPI_Common());
        else fail("unknown boolean operation");
        TopTools_ListOfShape args, tools;
        args.Append(a.s);
        tools.Append(b.s);
        alg->SetArguments(args);
        alg->SetTools(tools);
        alg->SetRunParallel(run_parallel());
        {
            StageTimer st("boolean");
            alg->Build(progress());
        }
        check_cancelled();
        if (!alg->IsDone() || alg->HasErrors()) fail("boolean operation failed");
        TopoDS_Shape res = alg->Shape();
        if (res.IsNull()) fail("boolean produced an empty result");
        Handle(BRepTools_History) hist = alg->History();
        if (unify) {
            ShapeUpgrade_UnifySameDomain usd(res, true, true, false);
            usd.SetLinearTolerance(1e-6);
            usd.SetAngularTolerance(1e-6);
            {
                StageTimer st("unify same domain");
                usd.Build();
            }
            res = usd.Shape();
            if (!hist.IsNull()) hist->Merge(usd.History());
            else hist = usd.History();
        }
        return make_result(first_solid(res), hist, &a, &b);
    });
}

OpResult unify_same_domain(const Shape& sh) {
    return guarded("unify_same_domain", [&] {
        ShapeUpgrade_UnifySameDomain usd(sh.s, true, true, false);
        usd.Build();
        return make_result(usd.Shape(), usd.History(), &sh, nullptr);
    });
}

OpResult fillet_edges(const Shape& sh, rust::Slice<const uint32_t> edges, double radius) {
    return guarded("fillet", [&] {
        if (!(radius > 0)) fail("fillet radius must be positive");
        BRepFilletAPI_MakeFillet mk(sh.s);
        for (uint32_t e : edges) mk.Add(radius, edge_at(sh, e));
        mk.Build(progress());
        check_cancelled();
        if (!mk.IsDone()) fail("fillet failed (radius too large for the selected edges?)");
        TopoDS_Shape res = mk.Shape();
        Handle(BRepTools_History) none;
        // Fillet history: modified faces via MakeFillet::Modified.
        OpResult r;
        r.shape = wrap(res);
        for (int i = 1; i <= sh.faces.Extent(); ++i) {
            const TopoDS_Shape& f = sh.faces.FindKey(i);
            bool any = false;
            if (mk.IsDeleted(f)) continue;
            for (const TopoDS_Shape& g : mk.Modified(f)) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); any = true; }
            }
            if (!any) {
                int idx = r.shape->faces.FindIndex(f);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); }
            }
        }
        return r;
    });
}

OpResult fillet_edges_radii(const Shape& sh, rust::Slice<const uint32_t> edges, rust::Slice<const double> radii) {
    return guarded("fillet", [&] {
        if (edges.size() != radii.size() || edges.size() == 0) fail("fillet needs one radius per edge");
        BRepFilletAPI_MakeFillet mk(sh.s);
        for (size_t i = 0; i < edges.size(); ++i) {
            if (!(radii[i] > 0)) fail("fillet radius must be positive");
            if (mk.Contour(edge_at(sh, edges[i])) == 0) mk.Add(radii[i], edge_at(sh, edges[i]));
        }
        for (size_t i = 0; i < edges.size(); ++i) {
            const TopoDS_Edge& e = edge_at(sh, edges[i]);
            mk.SetRadius(radii[i], mk.Contour(e), e);
        }
        mk.Build(progress());
        check_cancelled();
        if (!mk.IsDone()) fail("fillet failed (radius too large for the selected edges?)");
        OpResult r;
        r.shape = wrap(mk.Shape());
        for (int i = 1; i <= sh.faces.Extent(); ++i) {
            const TopoDS_Shape& f = sh.faces.FindKey(i);
            if (mk.IsDeleted(f)) continue;
            bool any = false;
            for (const TopoDS_Shape& g : mk.Modified(f)) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); any = true; }
            }
            if (!any) {
                int idx = r.shape->faces.FindIndex(f);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); }
            }
        }

        for (size_t i = 0; i < edges.size(); ++i) {
            for (const TopoDS_Shape& g : mk.Generated(edge_at(sh, edges[i]))) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(2); r.history.push_back(edges[i]); r.history.push_back(idx - 1); }
            }
        }
        return r;
    });
}

OpResult chamfer_edges_asym(const Shape& sh, rust::Slice<const uint32_t> edges, rust::Slice<const uint32_t> faces,
                            double d1, double d2) {
    return guarded("chamfer", [&] {
        if (!(d1 > 0) || !(d2 > 0)) fail("chamfer distances must be positive");
        if (edges.size() != faces.size() || edges.size() == 0) fail("chamfer needs a reference face per edge");
        BRepFilletAPI_MakeChamfer mk(sh.s);
        for (size_t i = 0; i < edges.size(); ++i) mk.Add(d1, d2, edge_at(sh, edges[i]), face_at(sh, faces[i]));
        mk.Build(progress());
        check_cancelled();
        if (!mk.IsDone()) fail("chamfer failed (distance too large for the selected edges?)");
        OpResult r;
        r.shape = wrap(mk.Shape());
        for (int i = 1; i <= sh.faces.Extent(); ++i) {
            const TopoDS_Shape& f = sh.faces.FindKey(i);
            if (mk.IsDeleted(f)) continue;
            bool any = false;
            for (const TopoDS_Shape& g : mk.Modified(f)) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); any = true; }
            }
            if (!any) {
                int idx = r.shape->faces.FindIndex(f);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); }
            }
        }
        for (uint32_t e : edges) {
            for (const TopoDS_Shape& g : mk.Generated(edge_at(sh, e))) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(2); r.history.push_back(e); r.history.push_back(idx - 1); }
            }
        }
        return r;
    });
}

OpResult chamfer_edges(const Shape& sh, rust::Slice<const uint32_t> edges, double distance) {
    return guarded("chamfer", [&] {
        if (!(distance > 0)) fail("chamfer distance must be positive");
        BRepFilletAPI_MakeChamfer mk(sh.s);
        for (uint32_t e : edges) mk.Add(distance, edge_at(sh, e));
        mk.Build(progress());
        check_cancelled();
        if (!mk.IsDone()) fail("chamfer failed (distance too large for the selected edges?)");
        OpResult r;
        r.shape = wrap(mk.Shape());
        for (int i = 1; i <= sh.faces.Extent(); ++i) {
            const TopoDS_Shape& f = sh.faces.FindKey(i);
            if (mk.IsDeleted(f)) continue;
            bool any = false;
            for (const TopoDS_Shape& g : mk.Modified(f)) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); any = true; }
            }
            if (!any) {
                int idx = r.shape->faces.FindIndex(f);
                if (idx > 0) { r.history.push_back(0); r.history.push_back(i - 1); r.history.push_back(idx - 1); }
            }
        }
        for (uint32_t e : edges) {
            for (const TopoDS_Shape& g : mk.Generated(edge_at(sh, e))) {
                int idx = r.shape->faces.FindIndex(g);
                if (idx > 0) { r.history.push_back(2); r.history.push_back(e); r.history.push_back(idx - 1); }
            }
        }
        return r;
    });
}

OpResult defeature_faces(const Shape& sh, rust::Slice<const uint32_t> faces) {
    return guarded("defeature", [&] {
        BRepAlgoAPI_Defeaturing df;
        df.SetShape(sh.s);
        for (uint32_t f : faces) df.AddFaceToRemove(face_at(sh, f));
        df.SetToFillHistory(true);
        df.Build();
        if (!df.IsDone() || df.HasErrors()) fail("the faces cannot be removed (the neighbouring faces cannot be extended to close the gap)");
        TopoDS_Shape res = df.Shape();
        if (res.IsNull()) fail("removing the faces produced an empty result");
        return make_result(first_solid(res), df.History(), &sh, nullptr);
    });
}

rust::Vec<uint32_t> blend_chain(const Shape& sh, rust::Slice<const uint32_t> edges, bool chamfer) {
    return guarded("blend_chain", [&] {
        rust::Vec<uint32_t> out;
        auto collect = [&](auto& mk) {
            for (uint32_t e : edges) mk.Add(1.0, edge_at(sh, e));
            for (int c = 1; c <= mk.NbContours(); ++c) {
                for (int j = 1; j <= mk.NbEdges(c); ++j) {
                    int idx = sh.edges.FindIndex(mk.Edge(c, j));
                    if (idx > 0) {
                        uint32_t v = (uint32_t)(idx - 1);
                        bool dup = false;
                        for (uint32_t o : out) if (o == v) dup = true;
                        if (!dup) out.push_back(v);
                    }
                }
            }
        };
        if (chamfer) { BRepFilletAPI_MakeChamfer mk(sh.s); collect(mk); }
        else { BRepFilletAPI_MakeFillet mk(sh.s); collect(mk); }
        return out;
    });
}

std::shared_ptr<Shape> edge_face(const Shape& sh, uint32_t index) {
    return guarded("edge_face", [&] {
        const TopoDS_Edge& e = edge_at(sh, index);
        BRepBuilderAPI_MakeWire mw(e);
        if (!mw.IsDone()) fail("the edge does not form a closed outline");
        if (!BRep_Tool::IsClosed(mw.Wire())) fail("the edge is not a closed loop");
        BRepBuilderAPI_MakeFace mf(mw.Wire(), true);
        if (!mf.IsDone()) fail("the edge loop is not flat");
        return wrap(mf.Face());
    });
}

std::unique_ptr<Loft> new_loft(bool ruled) { return std::make_unique<Loft>(ruled); }

void loft_add(Loft& l, const Shape& face) {
    guarded("loft_add", [&] {
        TopoDS_Face f;
        if (face.s.ShapeType() == TopAbs_FACE) f = TopoDS::Face(face.s);
        else {
            TopExp_Explorer ex(face.s, TopAbs_FACE);
            if (!ex.More()) fail("a profile is not a face");
            f = TopoDS::Face(ex.Current());
        }
        TopoDS_Wire outer = BRepTools::OuterWire(f);
        std::vector<TopoDS_Wire> inner;
        for (TopExp_Explorer ex(f, TopAbs_WIRE); ex.More(); ex.Next()) {
            TopoDS_Wire w = TopoDS::Wire(ex.Current());
            if (!w.IsSame(outer)) inner.push_back(w);
        }
        if (l.outer.empty()) {
            l.holes = inner.size();
        } else if (inner.size() != l.holes) {
            fail("all profiles must have the same number of holes");
        }
        l.outer.push_back(outer);
        l.inner.push_back(inner);
        return 0;
    });
}

std::shared_ptr<Shape> loft_finish(Loft& l) {
    return guarded("loft", [&] {
        if (l.outer.size() < 2) fail("a loft needs at least two profiles");
        auto build = [&](const std::vector<TopoDS_Wire>& wires) {
            BRepOffsetAPI_ThruSections ts(true, l.ruled, 1e-6);
            ts.CheckCompatibility(true);
            for (const auto& w : wires) ts.AddWire(w);
            ts.Build(progress());
            check_cancelled();
            if (!ts.IsDone()) fail("the profiles cannot be joined (they may be too different or cross each other)");
            return ts.Shape();
        };
        TopoDS_Shape result = build(l.outer);
        for (size_t k = 0; k < l.holes; ++k) {
            std::vector<TopoDS_Wire> ws;
            for (const auto& v : l.inner) ws.push_back(v[k]);
            TopoDS_Shape hole = build(ws);
            result = cut_cancellable(result, hole, "the inner contours could not be subtracted");
        }
        return wrap(first_solid(result));
    });
}


// ---- paths (3D curves) and sweeps ----

// Records: 1 line (a, b: 6 numbers) | 2 arc (a, mid, b: 9) | 3 helix (origin 3, axis 3, x direction 3, radius, pitch, turns,
// hand 1 or -1, start angle in radians, cone half angle in radians: 17 numbers).
std::shared_ptr<Shape> make_path(rust::Slice<const double> d) {
    return guarded("make_path", [&] {
        BRepBuilderAPI_MakeWire mw;
        size_t i = 0;
        auto P = [&](size_t k) { return gp_Pnt(d[k], d[k + 1], d[k + 2]); };
        while (i < d.size()) {
            int kind = (int)d[i++];
            if (kind == 1) {
                gp_Pnt a = P(i), b = P(i + 3);
                i += 6;
                if (a.Distance(b) < 1e-9) fail("a line segment of the path has no length");
                BRepBuilderAPI_MakeEdge me(a, b);
                if (!me.IsDone()) fail("a line segment of the path cannot be built");
                mw.Add(me.Edge());
            } else if (kind == 2) {
                gp_Pnt a = P(i), m = P(i + 3), b = P(i + 6);
                i += 9;
                GC_MakeArcOfCircle arc(a, m, b);
                if (!arc.IsDone()) fail("an arc of the path cannot be built (its points are on one line)");
                BRepBuilderAPI_MakeEdge me(arc.Value());
                if (!me.IsDone()) fail("an arc of the path cannot be built");
                mw.Add(me.Edge());
            } else if (kind == 3) {
                gp_Pnt o = P(i);
                gp_Dir z(d[i + 3], d[i + 4], d[i + 5]), xd(d[i + 6], d[i + 7], d[i + 8]);
                double radius = d[i + 9], pitch = d[i + 10], turns = d[i + 11], hand = d[i + 12], start = d[i + 13], cone = d[i + 14];
                i += 15;
                if (radius <= 1e-9 || std::fabs(pitch) < 1e-9 || turns <= 1e-9) fail("the helix needs a positive diameter, pitch and height");
                // keep the x direction perpendicular to the axis
                gp_Dir x = z.Crossed(xd.Crossed(z));
                gp_Ax3 ax(o, z, x);
                const double two_pi = 2.0 * M_PI;
                Handle(Geom_Surface) surf;
                double dv = pitch;
                if (std::fabs(cone) > 1e-9) {
                    // a conical surface: v runs along the generatrix, the axial advance is v * cos(cone)
                    // a positive angle opens along the axis, a negative one narrows along it
                    surf = new Geom_ConicalSurface(ax, cone, radius);
                    dv = pitch / std::cos(cone);
                } else {
                    surf = new Geom_CylindricalSurface(ax, radius);
                }
                gp_Dir2d dir(hand * two_pi, dv);
                Handle(Geom2d_Line) l2 = new Geom2d_Line(gp_Pnt2d(start, 0.0), dir);
                double len = turns * std::sqrt(two_pi * two_pi + dv * dv);
                // Pieces of at most half a turn. One long edge makes the sweep very slow (a single huge approximation), and a
                // face that wraps a whole turn around a cylinder makes OCCT miss or botch its intersections with that cylinder
                // (the swept spring was lost or came out inside out when combined with the cylinder it winds on).
                int pieces = std::max(1, (int)std::ceil(turns * 2.0 - 1e-9));
                for (int k = 0; k < pieces; ++k) {
                    double t0 = len * k / pieces, t1 = len * (k + 1) / pieces;
                    BRepBuilderAPI_MakeEdge me(l2, surf, t0, t1);
                    if (!me.IsDone()) fail("the helix cannot be built");
                    TopoDS_Edge e = me.Edge();
                    BRepLib::BuildCurves3d(e);
                    mw.Add(e);
                }
            } else {
                fail("unknown path record");
            }
        }
        if (!mw.IsDone()) fail("the segments of the path do not join into one chain");
        return wrap(mw.Wire());
    });
}

std::shared_ptr<Shape> edges_wire(const Shape& sh, rust::Slice<const uint32_t> edges) {
    return guarded("edges_wire", [&] {
        TopTools_ListOfShape list;
        for (uint32_t e : edges) list.Append(edge_at(sh, e));
        BRepBuilderAPI_MakeWire mw;
        mw.Add(list);
        if (!mw.IsDone()) fail("the selected edges do not form one connected chain");
        return wrap(mw.Wire());
    });
}

// start point, start tangent, end point, end tangent, length, closed (1/0)
rust::Vec<double> path_info(const Shape& path) {
    return guarded("path_info", [&] {
        BRepAdaptor_CompCurve cc(TopoDS::Wire(path.s));
        rust::Vec<double> out;
        for (double t : {cc.FirstParameter(), cc.LastParameter()}) {
            gp_Pnt p; gp_Vec v;
            cc.D1(t, p, v);
            double m = v.Magnitude();
            if (m > 1e-12) v.Divide(m);
            for (double x : {p.X(), p.Y(), p.Z(), v.X(), v.Y(), v.Z()}) out.push_back(x);
        }
        GProp_GProps gp;
        BRepGProp::LinearProperties(path.s, gp);
        out.push_back(gp.Mass());
        out.push_back(path.s.Closed() ? 1.0 : 0.0);
        return out;
    });
}

// points along the path, x y z each, with at most `deflection` mm between the polyline and the curve
rust::Vec<double> path_points(const Shape& path, double deflection) {
    return guarded("path_points", [&] {
        BRepAdaptor_CompCurve cc(TopoDS::Wire(path.s));
        rust::Vec<double> out;
        GCPnts_QuasiUniformDeflection qd(cc, deflection);
        if (!qd.IsDone()) fail("the path cannot be drawn");
        for (int i = 1; i <= qd.NbPoints(); ++i) {
            gp_Pnt p = qd.Value(i);
            out.push_back(p.X()); out.push_back(p.Y()); out.push_back(p.Z());
        }
        return out;
    });
}

// Sweeps a flat face along a path. Holes of the face are swept separately and subtracted. `fixed` keeps the profile at a
// constant orientation to the direction (ax, ay, az) (the axis of a helix); `rounded` rounds the corners of the path.
std::shared_ptr<Shape> sweep_face(const Shape& face, const Shape& path, bool fixed, double ax, double ay, double az, bool rounded) {
    return guarded("sweep", [&] {
        TopoDS_Face f = TopoDS::Face(face.s);
        TopoDS_Wire outer = BRepTools::OuterWire(f);
        auto pipe = [&](const TopoDS_Wire& profile) {
            BRepOffsetAPI_MakePipeShell mk(TopoDS::Wire(path.s));
            if (fixed) mk.SetMode(gp_Dir(ax, ay, az));
            mk.SetTransitionMode(rounded ? BRepBuilderAPI_RoundCorner : BRepBuilderAPI_RightCorner);
            mk.Add(profile, Standard_False, Standard_False);
            {
                StageTimer st("pipe shell");
                mk.Build(progress());
            }
            check_cancelled();
            if (!mk.IsDone()) fail("the profile cannot be swept along this path (the path may turn too sharply for the profile)");
            if (!mk.MakeSolid()) fail("the swept surface cannot be closed into a solid");
            return mk.Shape();
        };
        TopoDS_Shape result = pipe(outer);
        for (TopExp_Explorer ex(f, TopAbs_WIRE); ex.More(); ex.Next()) {
            TopoDS_Wire w = TopoDS::Wire(ex.Current());
            if (w.IsSame(outer)) continue;
            TopoDS_Shape hole = pipe(w);
            result = cut_cancellable(result, hole, "the hole of the profile could not be subtracted");
        }
        return wrap(first_solid(result));
    });
}

std::unique_ptr<Sewer> new_sewer(double tol) { return std::make_unique<Sewer>(tol); }

void sewer_add(Sewer& sw, const Shape& sh) {
    guarded("sewer_add", [&] { sw.sew.Add(sh.s); return 0; });
}

std::shared_ptr<Shape> sewer_finish(Sewer& sw) {
    return guarded("sew_faces", [&] {
        sw.sew.Perform();
        TopoDS_Shape sewn = sw.sew.SewedShape();
        if (sewn.IsNull()) fail("sewing produced nothing");
        TopoDS_Shell shell;
        if (sewn.ShapeType() == TopAbs_SHELL) shell = TopoDS::Shell(sewn);
        else {
            TopExp_Explorer ex(sewn, TopAbs_SHELL);
            if (!ex.More()) fail("faces do not form a shell");
            shell = TopoDS::Shell(ex.Current());
        }
        BRepBuilderAPI_MakeSolid ms(shell);
        if (!ms.IsDone()) fail("cannot make a solid from the shell");
        ShapeFix_Solid fix(ms.Solid());
        fix.Perform();
        return wrap(fix.Solid());
    });
}

MeshData mesh_shape(const Shape& sh, double linear, double angular) {
    return mesh_shape_regions(sh, linear, angular, linear, rust::Slice<const uint8_t>());
}

// Faces marked in `fine` (one byte per face) are triangulated with the deflection `linear`; the others are left out (the caller keeps the
// triangles it has of them), and so are the edges that border no fine face. An empty mask means all faces, with `linear`. (`coarse` is unused
// and kept for the callers.)
MeshData mesh_shape_regions(const Shape& sh, double linear, double angular, double coarse, rust::Slice<const uint8_t> fine) {
    (void)coarse;
    return guarded_free("mesh", [&] {
        // an indicator of its own: the shared one keeps the state of one run and meshing runs for many bodies at once
        Handle(CancelIndicator) ind = new CancelIndicator();
        BRepTools::Clean(sh.s);
        const bool partial = fine.size() > 0;
        auto is_fine = [&](int fi) { return !partial || (fi - 1 < (int)fine.size() && fine[fi - 1] != 0); };
        IMeshTools_Parameters mp;
        mp.Deflection = linear;
        mp.Angle = angular;
        // the boundary edges (the circles of a thin wire) get the fine angle; the interior of a face keeps the coarse one (rows along a long
        // swept face would otherwise be six times as many for no gain)
        mp.AngleInterior = std::max(angular, 0.5);
        mp.Relative = false;
        mp.InParallel = run_parallel();
        StageTimer st("mesh");
        if (!partial) {
            BRepMesh_IncrementalMesh im(sh.s, mp, ind->Start());
        } else {
            BRep_Builder bb;
            TopoDS_Compound comp;
            bb.MakeCompound(comp);
            for (int fi = 1; fi <= sh.faces.Extent(); ++fi) {
                if (is_fine(fi)) bb.Add(comp, sh.faces.FindKey(fi));
            }
            BRepMesh_IncrementalMesh im(comp, mp, ind->Start());
        }
        check_cancelled();
        MeshData m;
        for (int fi = 1; fi <= sh.faces.Extent(); ++fi) {
            m.face_fine.push_back(is_fine(fi) ? 1 : 0);
            if (!is_fine(fi)) continue;
            const TopoDS_Face& f = TopoDS::Face(sh.faces.FindKey(fi));
            TopLoc_Location loc;
            Handle(Poly_Triangulation) tri = BRep_Tool::Triangulation(f, loc);
            if (tri.IsNull()) continue;
            BRepLib_ToolTriangulatedShape::ComputeNormals(f, tri);
            const bool rev = f.Orientation() == TopAbs_REVERSED;
            const uint32_t base = (uint32_t)(m.positions.size() / 3);
            const gp_Trsf tr = loc.Transformation();
            for (int i = 1; i <= tri->NbNodes(); ++i) {
                gp_Pnt p = tri->Node(i).Transformed(tr);
                m.positions.push_back((float)p.X()); m.positions.push_back((float)p.Y()); m.positions.push_back((float)p.Z());
                gp_Dir n(0, 0, 1);
                if (tri->HasNormals()) n = tri->Normal(i);
                n.Transform(tr);
                if (rev) n.Reverse();
                m.normals.push_back((float)n.X()); m.normals.push_back((float)n.Y()); m.normals.push_back((float)n.Z());
            }
            for (int t = 1; t <= tri->NbTriangles(); ++t) {
                int a, b, c;
                tri->Triangle(t).Get(a, b, c);
                if (rev) std::swap(b, c);
                m.indices.push_back(base + a - 1); m.indices.push_back(base + b - 1); m.indices.push_back(base + c - 1);
                m.tri_face.push_back((uint32_t)(fi - 1));
            }
        }
        uint32_t npts = 0;
        TopTools_IndexedDataMapOfShapeListOfShape e2f;
        TopExp::MapShapesAndAncestors(sh.s, TopAbs_EDGE, TopAbs_FACE, e2f);
        auto face_normal = [](const TopoDS_Edge& e, const TopoDS_Face& f, gp_Dir& out) {
            try {
                double a, b;
                Handle(Geom2d_Curve) pc = BRep_Tool::CurveOnSurface(e, f, a, b);
                if (pc.IsNull()) return false;
                gp_Pnt2d uv = pc->Value(0.5 * (a + b));
                BRepAdaptor_Surface sf(f);
                gp_Pnt p; gp_Vec du, dv;
                sf.D1(uv.X(), uv.Y(), p, du, dv);
                gp_Vec n = du.Crossed(dv);
                if (n.Magnitude() < 1e-12) return false;
                if (f.Orientation() == TopAbs_REVERSED) n.Reverse();
                out = gp_Dir(n);
                return true;
            } catch (...) { return false; }
        };
        for (int ei = 1; ei <= sh.edges.Extent(); ++ei) {
            const TopoDS_Edge& e = TopoDS::Edge(sh.edges.FindKey(ei));
            if (BRep_Tool::Degenerated(e)) continue;
            if (partial) {
                bool any = false;
                int ie0 = e2f.FindIndex(e);
                if (ie0 > 0) {
                    for (const TopoDS_Shape& f : e2f.FindFromIndex(ie0)) {
                        int fi = sh.faces.FindIndex(f);
                        if (fi > 0 && is_fine(fi)) any = true;
                    }
                }
                if (!any) continue;
            }
            {
                uint8_t smooth = 0;
                int ie = e2f.FindIndex(e);
                if (ie > 0) {
                    std::vector<TopoDS_Face> fs;
                    for (const TopoDS_Shape& f : e2f.FindFromIndex(ie)) fs.push_back(TopoDS::Face(f));
                    if (fs.size() == 1) smooth = 1;  // the seam of a periodic face
                    else if (fs.size() == 2) {
                        gp_Dir n1, n2;
                        if (face_normal(e, fs[0], n1) && face_normal(e, fs[1], n2) && n1.Dot(n2) > std::cos(2.0 * M_PI / 180.0)) smooth = 1;
                    }
                }
                m.edge_smooth.push_back(smooth);
            }
            BRepAdaptor_Curve c(e);
            GCPnts_TangentialDeflection tg(c, angular, linear);
            m.edge_offsets.push_back(npts);
            m.edge_ids.push_back((uint32_t)(ei - 1));
            for (int i = 1; i <= tg.NbPoints(); ++i) {
                gp_Pnt p = tg.Value(i);
                m.edge_points.push_back((float)p.X()); m.edge_points.push_back((float)p.Y()); m.edge_points.push_back((float)p.Z());
                ++npts;
            }
        }
        m.edge_offsets.push_back(npts);
        return m;
    });
}

// "name:ms;name:ms" of the stages since the last call (this thread), then clears the list.
rust::String take_stages() {
    std::string s;
    for (auto& p : g_stages) s += p.first + ":" + std::to_string(p.second) + ";";
    g_stages.clear();
    return rust::String(s);
}

void set_cancel(bool on) { g_cancel.store(on); }
bool cancel_requested() { return g_cancel.load(); }

static TopoDS_Shape measure_part(const Shape& sh, uint32_t kind, uint32_t index, const std::array<double, 3>& p) {
    if (kind == 0) return BRepBuilderAPI_MakeVertex(gp_Pnt(p[0], p[1], p[2]));
    if (kind == 1) return edge_at(sh, index);
    return face_at(sh, index);
}

// Shortest distance between two parts (a point, an edge or a face of a shape): distance, the nearest point on a, on b.
rust::Vec<double> shape_distance(const Shape& a, std::array<uint32_t, 2> ka, std::array<double, 3> pa, const Shape& b, std::array<uint32_t, 2> kb, std::array<double, 3> pb) {
    return guarded("shape_distance", [&] {
        TopoDS_Shape sa = measure_part(a, ka[0], ka[1], pa), sb = measure_part(b, kb[0], kb[1], pb);
        BRepExtrema_DistShapeShape d(sa, sb);
        if (!d.IsDone() || d.NbSolution() < 1) fail("the distance could not be measured");
        rust::Vec<double> out;
        out.push_back(d.Value());
        gp_Pnt p1 = d.PointOnShape1(1), p2 = d.PointOnShape2(1);
        for (double v : {p1.X(), p1.Y(), p1.Z(), p2.X(), p2.Y(), p2.Z()}) out.push_back(v);
        return out;
    });
}

bool check_shape(const Shape& sh) {
    try {
        std::lock_guard<std::recursive_mutex> lock(g_lock);
        StageTimer st("validity check");
        BRepCheck_Analyzer an(sh.s, /*GeomControls=*/false, /*isParallel=*/true);
        return an.IsValid();
    } catch (...) {
        return false;
    }
}

// The smallest radius of curvature of every face (the order of the faces of the shape), sampled on a grid of the face: how fine the mesh of
// the face must be to stay close to it (a thread of a 1 mm wire along a helix of 20 mm has 0.5, a plane has none: 1e9).
rust::Vec<double> face_scales(const Shape& sh) {
    return guarded_free("face_scales", [&] {
        rust::Vec<double> out;
        for (int fi = 1; fi <= sh.faces.Extent(); ++fi) {
            const TopoDS_Face& f = TopoDS::Face(sh.faces.FindKey(fi));
            double scale = 1e9;
            try {
                BRepAdaptor_Surface sf(f);
                if (sf.GetType() != GeomAbs_Plane) {
                    double u0, u1, v0, v1;
                    BRepTools::UVBounds(f, u0, u1, v0, v1);
                    if (u1 - u0 > 1e6) u1 = u0 + 1e6;
                    if (v1 - v0 > 1e6) v1 = v0 + 1e6;
                    BRepLProp_SLProps lp(sf, 2, 1e-7);
                    // an analytic round face has one radius (or a few): one sample, the others a small grid
                    const GeomAbs_SurfaceType ty = sf.GetType();
                    const int N = (ty == GeomAbs_Cylinder || ty == GeomAbs_Sphere) ? 0 : 4;
                    for (int i = 0; i <= N; ++i) {
                        for (int j = 0; j <= N; ++j) {
                            const double fu = N ? double(i) / N : 0.5, fv = N ? double(j) / N : 0.5;
                            lp.SetParameters(u0 + (u1 - u0) * fu, v0 + (v1 - v0) * fv);
                            if (!lp.IsCurvatureDefined()) continue;
                            double k = std::max(std::abs(lp.MaxCurvature()), std::abs(lp.MinCurvature()));
                            if (k > 1e-9) scale = std::min(scale, 1.0 / k);
                        }
                    }
                }
            } catch (...) {}
            out.push_back(scale);
        }
        return out;
    });
}

// Bounding box only: min x y z, max x y z (the same box props() gives).
rust::Vec<double> bounds_of(const Shape& sh) {
    return guarded_free("bounds", [&] {
        rust::Vec<double> out;
        Bnd_Box bb;
        BRepBndLib::AddOptimal(sh.s, bb, false, false);
        if (bb.IsVoid()) fail("the shape has no extent");
        double x0, y0, z0, x1, y1, z1;
        bb.Get(x0, y0, z0, x1, y1, z1);
        for (double v : {x0, y0, z0, x1, y1, z1}) out.push_back(v);
        return out;
    });
}

// Smallest distance between two solids: 0 when they touch, overlap or one lies inside the other.
double solid_distance(const Shape& a, const Shape& b) {
    return guarded("solid_distance", [&] {
        BRepExtrema_DistShapeShape d(a.s, b.s);
        d.Perform(progress());
        check_cancelled();
        if (!d.IsDone()) fail("the distance of the bodies cannot be found");
        double v = d.Value();
        if (v < 1e-7) return 0.0;
        TopExp_Explorer ea(a.s, TopAbs_VERTEX), eb(b.s, TopAbs_VERTEX);
        if (eb.More() && BRepClass3d_SolidClassifier(a.s, BRep_Tool::Pnt(TopoDS::Vertex(eb.Current())), 1e-6).State() == TopAbs_IN) return 0.0;
        if (ea.More() && BRepClass3d_SolidClassifier(b.s, BRep_Tool::Pnt(TopoDS::Vertex(ea.Current())), 1e-6).State() == TopAbs_IN) return 0.0;
        return v;
    });
}

rust::String shape_diag(const Shape& sh) {
    return guarded("shape_diag", [&] {
        double te = 0, tv = 0, tf = 0;
        for (int i = 1; i <= sh.edges.Extent(); ++i) te = std::max(te, BRep_Tool::Tolerance(TopoDS::Edge(sh.edges.FindKey(i))));
        for (TopExp_Explorer ex(sh.s, TopAbs_VERTEX); ex.More(); ex.Next()) tv = std::max(tv, BRep_Tool::Tolerance(TopoDS::Vertex(ex.Current())));
        for (int i = 1; i <= sh.faces.Extent(); ++i) tf = std::max(tf, BRep_Tool::Tolerance(TopoDS::Face(sh.faces.FindKey(i))));
        int nsol = 0, nsh = 0;
        for (TopExp_Explorer ex(sh.s, TopAbs_SOLID); ex.More(); ex.Next()) ++nsol;
        for (TopExp_Explorer ex(sh.s, TopAbs_SHELL); ex.More(); ex.Next()) ++nsh;
        std::ostringstream o;
        o << "solids " << nsol << " shells " << nsh << " type " << (int)sh.s.ShapeType() << " orient " << (int)sh.s.Orientation()
          << " tol edge " << te << " vertex " << tv << " face " << tf;
        return rust::String(o.str());
    });
}

double volume_of(const Shape& sh) {
    return guarded("volume", [&] {
        GProp_GProps v;
        BRepGProp::VolumeProperties(sh.s, v);
        return v.Mass();
    });
}

Props props(const Shape& sh) {
    return guarded("props", [&] {
        Props p{};
        GProp_GProps v, s;
        BRepGProp::VolumeProperties(sh.s, v);
        BRepGProp::SurfaceProperties(sh.s, s);
        p.volume = v.Mass();
        p.area = s.Mass();
        gp_Pnt c = v.CentreOfMass();
        p.centroid = {c.X(), c.Y(), c.Z()};
        Bnd_Box bb;
        BRepBndLib::AddOptimal(sh.s, bb, false, false);
        if (!bb.IsVoid()) {
            double x0, y0, z0, x1, y1, z1;
            bb.Get(x0, y0, z0, x1, y1, z1);
            p.min = {x0, y0, z0};
            p.max = {x1, y1, z1};
        }
        return p;
    });
}

rust::Vec<double> section_plane(const Shape& sh, std::array<double, 3> o, std::array<double, 3> n, double tol) {
    return guarded("section", [&] {
        gp_Pln pl(gp_Pnt(o[0], o[1], o[2]), gp_Dir(n[0], n[1], n[2]));
        BRepAlgoAPI_Section sec(sh.s, pl, false);
        sec.Build();
        if (!sec.IsDone() || sec.HasErrors()) fail("section failed");
        rust::Vec<double> out;
        for (TopExp_Explorer ex(sec.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
            const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
            BRepAdaptor_Curve c(e);
            double u0 = c.FirstParameter(), u1 = c.LastParameter();
            auto push3 = [&](gp_Pnt p) { out.push_back(p.X()); out.push_back(p.Y()); out.push_back(p.Z()); };
            if (c.GetType() == GeomAbs_Line) {
                out.push_back(1); push3(c.Value(u0)); push3(c.Value(u1));
            } else if (c.GetType() == GeomAbs_Circle) {
                bool full = std::abs((u1 - u0) - 2 * M_PI) < 1e-7;
                if (full) {
                    gp_Circ ci = c.Circle();
                    out.push_back(3); push3(ci.Location());
                    gp_Dir d = ci.Axis().Direction();
                    out.push_back(d.X()); out.push_back(d.Y()); out.push_back(d.Z());
                    out.push_back(ci.Radius());
                } else {
                    out.push_back(2); push3(c.Value(u0)); push3(c.Value(0.5 * (u0 + u1))); push3(c.Value(u1));
                }
            } else {
                // Other curves: polyline approximation (arc fitting arrives with the laser phase).
                GCPnts_TangentialDeflection tg(c, 0.1, tol);
                for (int i = 1; i < tg.NbPoints(); ++i) {
                    out.push_back(1); push3(tg.Value(i)); push3(tg.Value(i + 1));
                }
            }
        }
        return out;
    });
}

rust::Vec<uint8_t> write_step(const Shape& sh) {
    return guarded("write_step", [&] {
        STEPControl_Writer w;
        if (w.Transfer(sh.s, STEPControl_AsIs, true, progress()) != IFSelect_RetDone) { check_cancelled(); fail("STEP transfer failed"); }
        std::ostringstream os;
        if (w.WriteStream(os) != IFSelect_RetDone) fail("STEP write failed");
        std::string s = os.str();
        rust::Vec<uint8_t> out;
        out.reserve(s.size());
        for (char ch : s) out.push_back((uint8_t)ch);
        return out;
    });
}

std::shared_ptr<Shape> read_step(rust::Slice<const uint8_t> data) {
    return guarded("read_step", [&] {
        std::istringstream is(std::string((const char*)data.data(), data.size()));
        STEPControl_Reader r;
        if (r.ReadStream("memory.step", is) != IFSelect_RetDone) fail("not a readable STEP file");
        if (r.TransferRoots() <= 0) fail("STEP file contains no transferable shapes");
        return wrap(r.OneShape());
    });
}

std::unique_ptr<StepWriter> new_step_writer() { return std::make_unique<StepWriter>(); }

void step_writer_add(StepWriter& w, const Shape& sh) {
    guarded("step_writer_add", [&] {
        if (w.w.Transfer(sh.s, STEPControl_AsIs, true, progress()) != IFSelect_RetDone) { check_cancelled(); fail("STEP transfer failed"); }
        w.count++;
        return 0;
    });
}

// Like step_writer_add, and the name is written as the name of the solid (MANIFOLD_SOLID_BREP).
void step_writer_add_named(StepWriter& w, const Shape& sh, rust::Str name) {
    step_writer_add(w, sh);
    try {
        Handle(Transfer_FinderProcess) fp = w.w.WS()->TransferWriter()->FinderProcess();
        // the shape itself and the solids inside it (a body that is wrapped in a compound is read back as its solid)
        auto set_name = [&](const TopoDS_Shape& t) {
            Handle(TransferBRep_ShapeMapper) m = TransferBRep::ShapeMapper(fp, t);
            Handle(Standard_Transient) ent;
            if (!m.IsNull() && fp->FindTypedTransient(m, STANDARD_TYPE(StepRepr_RepresentationItem), ent)) {
                Handle(StepRepr_RepresentationItem) item = Handle(StepRepr_RepresentationItem)::DownCast(ent);
                if (!item.IsNull()) item->SetName(new TCollection_HAsciiString(std::string(name).c_str()));
            }
        };
        set_name(sh.s);
        if (sh.s.ShapeType() != TopAbs_SOLID) {
            for (TopExp_Explorer ex(sh.s, TopAbs_SOLID); ex.More(); ex.Next()) set_name(ex.Current());
        }
    } catch (...) {}
}

// The names of the solids of a STEP file, in the order of solid_at ("" where the file has none).
rust::Vec<rust::String> read_step_names(rust::Slice<const uint8_t> data) {
    return guarded("read_step_names", [&] {
        std::istringstream is(std::string((const char*)data.data(), data.size()));
        STEPControl_Reader r;
        if (r.ReadStream("memory.step", is) != IFSelect_RetDone) fail("not a readable STEP file");
        if (r.TransferRoots() <= 0) fail("STEP file contains no transferable shapes");
        TopoDS_Shape all = r.OneShape();
        rust::Vec<rust::String> out;
        for (TopExp_Explorer ex(all, TopAbs_SOLID); ex.More(); ex.Next()) {
            std::string nm;
            try {
                Handle(Standard_Transient) e = r.WS()->TransferReader()->EntityFromShapeResult(ex.Current(), 1);
                Handle(StepRepr_RepresentationItem) item = Handle(StepRepr_RepresentationItem)::DownCast(e);
                if (!item.IsNull() && !item->Name().IsNull()) nm = item->Name()->ToCString();
            } catch (...) {}
            out.push_back(rust::String(nm));
        }
        return out;
    });
}

rust::Vec<uint8_t> step_writer_finish(StepWriter& w) {
    return guarded("step_writer_finish", [&] {
        if (w.count == 0) fail("nothing to write");
        std::ostringstream os;
        if (w.w.WriteStream(os) != IFSelect_RetDone) fail("STEP write failed");
        std::string s = os.str();
        rust::Vec<uint8_t> out;
        out.reserve(s.size());
        for (char ch : s) out.push_back((uint8_t)ch);
        return out;
    });
}

rust::Vec<uint8_t> write_brep(const Shape& sh) {
    return guarded_free("write_brep", [&] {
        std::ostringstream os;
        BinTools::Write(sh.s, os, false, false, BinTools_FormatVersion_CURRENT);  // no triangulation: the file does not carry display meshes, and the writer does not touch what a mesher may be changing
        std::string s = os.str();
        rust::Vec<uint8_t> out;
        out.reserve(s.size());
        for (char ch : s) out.push_back((uint8_t)ch);
        return out;
    });
}

std::shared_ptr<Shape> read_brep(rust::Slice<const uint8_t> data) {
    return guarded_free("read_brep", [&] {
        std::istringstream is(std::string((const char*)data.data(), data.size()));
        TopoDS_Shape s;
        BinTools::Read(s, is);
        return wrap(s);
    });
}

}  // namespace rol

namespace rol {

// How many threads one parallel job of Open CASCADE may take from its pool. The pool itself is never resized (that is refused while it works),
// only the number a single job locks is set, which is a plain number and safe at any time.
void set_threads(int32_t n) {
    g_threads.store(n);
    if (n > 0) OSD_ThreadPool::DefaultPool()->SetNbDefaultThreadsToLaunch(n);
}

// The identities of the parts (faces, edges, vertices) of the shape: two shapes that have one in common share data.
rust::Vec<uint64_t> shared_keys(const Shape& sh) {
    return guarded_free("keys", [&] {
        rust::Vec<uint64_t> out;
        for (TopAbs_ShapeEnum t : {TopAbs_FACE, TopAbs_EDGE, TopAbs_VERTEX}) {
            TopTools_IndexedMapOfShape m;
            TopExp::MapShapes(sh.s, t, m);
            for (int i = 1; i <= m.Extent(); ++i) {
                const TopoDS_Shape& p = m.FindKey(i);
                out.push_back((uint64_t)(uintptr_t)p.TShape().get());
                // the geometry too: shapes that have the same surface or curve are not meshed at the same moment either
                TopLoc_Location loc;
                if (t == TopAbs_FACE) {
                    Handle(Geom_Surface) g = BRep_Tool::Surface(TopoDS::Face(p), loc);
                    if (!g.IsNull()) out.push_back((uint64_t)(uintptr_t)g.get());
                } else if (t == TopAbs_EDGE) {
                    double a, b;
                    Handle(Geom_Curve) g = BRep_Tool::Curve(TopoDS::Edge(p), loc, a, b);
                    if (!g.IsNull()) out.push_back((uint64_t)(uintptr_t)g.get());
                }
            }
        }
        return out;
    });
}

}  // namespace rol
