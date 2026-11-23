// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
#pragma once
#include <STEPControl_Writer.hxx>
#include <array>
#include <memory>
#include <mutex>
#include <vector>
#include "rust/cxx.h"

#include <BRepBuilderAPI_Sewing.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Wire.hxx>
#include <TopTools_IndexedMapOfShape.hxx>

namespace rol {

struct MeshData;
struct FaceInfo;
struct EdgeInfo;
struct Props;
struct OpResult;

/// Immutable shape plus index maps of its faces and edges. Indices are the stable
/// identities the rest of the application uses.
class Shape {
public:
    explicit Shape(const TopoDS_Shape& s);
    TopoDS_Shape s;
    TopTools_IndexedMapOfShape faces;
    TopTools_IndexedMapOfShape edges;
};

std::shared_ptr<Shape> make_box(double x0, double y0, double z0, double x1, double y1, double z1);
std::shared_ptr<Shape> make_cylinder(double radius, double height, std::array<double, 3> origin, std::array<double, 3> axis);
std::shared_ptr<Shape> make_cone(double r1, double r2, double height, std::array<double, 3> origin, std::array<double, 3> axis);
std::shared_ptr<Shape> make_planar_face(std::array<double, 9> plane, rust::Slice<const double> profile);
std::shared_ptr<Shape> prism(const Shape& shape, std::array<double, 3> v);

std::shared_ptr<Shape> revolve(const Shape& shape, std::array<double, 3> point, std::array<double, 3> dir, double angle);
OpResult transform_shape(const Shape& shape, std::array<double, 3> pivot, std::array<double, 3> axis, double angle, std::array<double, 3> t);
std::shared_ptr<Shape> compound_append(const Shape& acc, const Shape& s, bool first);
OpResult mirror_shape(const Shape& shape, std::array<double, 3> point, std::array<double, 3> normal);
uint32_t solid_count(const Shape& shape);
bool point_in_solid(const Shape& shape, std::array<double, 3> p);
bool point_outside(const Shape& sh, std::array<double, 3> p);
std::shared_ptr<Shape> solid_at(const Shape& shape, uint32_t index);
uint32_t face_count(const Shape& shape);
uint32_t edge_count(const Shape& shape);
FaceInfo face_info(const Shape& shape, uint32_t index);
EdgeInfo edge_info(const Shape& shape, uint32_t index);
std::shared_ptr<Shape> face_shape(const Shape& shape, uint32_t index);
void face_outline(const Shape& shape, uint32_t index, double deflection, rust::Vec<float>& points, rust::Vec<uint32_t>& offsets);
rust::Vec<uint32_t> face_edges(const Shape& shape, uint32_t index);

OpResult boolean_op(const Shape& a, const Shape& b, uint32_t op, bool unify);
OpResult unify_same_domain(const Shape& shape);
OpResult fillet_edges(const Shape& shape, rust::Slice<const uint32_t> edges, double radius);
OpResult defeature_faces(const Shape& shape, rust::Slice<const uint32_t> faces);
rust::Vec<double> blend_scan(const Shape& shape);
rust::Vec<uint32_t> blend_chain(const Shape& shape, rust::Slice<const uint32_t> edges, bool chamfer);
OpResult fillet_edges_radii(const Shape& shape, rust::Slice<const uint32_t> edges, rust::Slice<const double> radii);
OpResult chamfer_edges_asym(const Shape& shape, rust::Slice<const uint32_t> edges, rust::Slice<const uint32_t> faces, rust::Slice<const double> d1, rust::Slice<const double> d2);
OpResult chamfer_edges(const Shape& shape, rust::Slice<const uint32_t> edges, double distance);
class Sewer {
public:
    explicit Sewer(double tol) : sew(tol) {}
    BRepBuilderAPI_Sewing sew;
};
std::unique_ptr<Sewer> new_sewer(double tolerance);
class Loft {
public:
    explicit Loft(bool r) : ruled(r) {}
    bool ruled;
    size_t holes = 0;
    std::vector<TopoDS_Wire> outer;
    std::vector<std::vector<TopoDS_Wire>> inner;
};
std::shared_ptr<Shape> edge_face(const Shape& shape, uint32_t index);
rust::Vec<double> edge_ends(const Shape& shape, uint32_t index);
/// Middle point of the edge, then centre and axis when it is a circle (zeros otherwise): 9 numbers.
rust::Vec<double> edge_geom(const Shape& shape, uint32_t index);
bool has_self_intersections(const Shape& shape);
rust::Vec<double> shape_distance(const Shape& a, std::array<uint32_t, 2> ka, std::array<double, 3> pa, const Shape& b, std::array<uint32_t, 2> kb, std::array<double, 3> pb);
void set_cancel(bool on);
double volume_of(const Shape& shape);
rust::String shape_diag(const Shape& shape);
double solid_distance(const Shape& a, const Shape& b);
rust::Vec<double> bounds_of(const Shape& shape);
rust::Vec<double> face_scales(const Shape& shape);
rust::String take_stages();
void set_threads(int32_t n);
rust::Vec<uint64_t> shared_keys(const Shape& shape);
bool cancel_requested();
rust::Vec<double> take_thread_stats();
rust::String take_thread_worst();
std::shared_ptr<Shape> make_path(rust::Slice<const double> data);
std::shared_ptr<Shape> edges_wire(const Shape& shape, rust::Slice<const uint32_t> edges);
rust::Vec<double> path_info(const Shape& path);
rust::Vec<double> path_points(const Shape& path, double deflection);
std::shared_ptr<Shape> sweep_face(const Shape& face, const Shape& path, bool fixed, double ax, double ay, double az, bool rounded);
std::unique_ptr<Loft> new_loft(bool ruled);
void loft_add(Loft& l, const Shape& face);
std::shared_ptr<Shape> loft_finish(Loft& l);
class StepWriter {
public:
    STEPControl_Writer w;
    int count = 0;
};
std::unique_ptr<StepWriter> new_step_writer();
void step_writer_add(StepWriter& w, const Shape& shape);
void step_writer_add_named(StepWriter& w, const Shape& shape, rust::Str name);
rust::Vec<rust::String> read_step_names(rust::Slice<const uint8_t> data);
rust::Vec<uint8_t> step_writer_finish(StepWriter& w);
void sewer_add(Sewer& sewer, const Shape& shape);
std::shared_ptr<Shape> sewer_finish(Sewer& sewer);

MeshData mesh_shape(const Shape& shape, double linear, double angular);
MeshData mesh_shape_regions(const Shape& shape, double linear, double angular, double min_size, rust::Slice<const uint8_t> fine);
bool check_shape(const Shape& shape);
Props props(const Shape& shape);
rust::Vec<double> section_plane(const Shape& shape, std::array<double, 3> origin, std::array<double, 3> normal, double tolerance);

rust::Vec<uint8_t> write_step(const Shape& shape);
std::shared_ptr<Shape> read_step(rust::Slice<const uint8_t> data);
rust::Vec<uint8_t> write_brep(const Shape& shape);
std::shared_ptr<Shape> read_brep(rust::Slice<const uint8_t> data);

}  // namespace rol
