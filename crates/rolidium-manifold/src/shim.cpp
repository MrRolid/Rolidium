// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
// A small C interface to Manifold: a boolean operation of two triangle meshes (double coordinates) and the volume of a mesh.
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <vector>

#include "manifold/manifold.h"

using manifold::Manifold;
using manifold::MeshGL64;

static const char* status_text(Manifold::Error e) {
    switch (e) {
        case Manifold::Error::NoError: return "";
        case Manifold::Error::NonFiniteVertex: return "a vertex is not a finite number";
        case Manifold::Error::NotManifold: return "the mesh is not a closed surface (open edges, or an edge shared by more than two triangles); repair it first";
        case Manifold::Error::VertexOutOfBounds: return "a triangle refers to a vertex that does not exist";
        case Manifold::Error::PropertiesWrongLength: return "the mesh data have a wrong length";
        case Manifold::Error::MissingPositionProperties: return "the mesh has no positions";
        case Manifold::Error::MergeVectorsDifferentLengths: return "the merge vectors have different lengths";
        case Manifold::Error::MergeIndexOutOfBounds: return "a merge index is out of range";
        case Manifold::Error::TransformWrongLength: return "the transform has a wrong length";
        case Manifold::Error::RunIndexWrongLength: return "the run index has a wrong length";
        case Manifold::Error::FaceIDWrongLength: return "the face ids have a wrong length";
        case Manifold::Error::InvalidConstruction: return "the mesh could not be constructed";
        default: return "the mesh was refused";
    }
}

static Manifold make(const double* pos, size_t nv, const uint32_t* tri, size_t nt, const char** err) {
    MeshGL64 m;
    m.numProp = 3;
    m.vertProperties.assign(pos, pos + 3 * nv);
    m.triVerts.assign(tri, tri + 3 * nt);
    Manifold r(m);
    if (r.Status() != Manifold::Error::NoError) *err = status_text(r.Status());
    return r;
}

extern "C" {

// op: 0 union, 1 difference (a - b), 2 intersection. Returns 0 and the result in newly allocated arrays (free with rm_free), or 1 and a message.
int rm_boolean(const double* pa, size_t na, const uint32_t* ta, size_t nta, const double* pb, size_t nb, const uint32_t* tb, size_t ntb, int op,
               double** out_pos, size_t* out_np, uint32_t** out_tri, size_t* out_nt, const char** err) {
    try {
        *err = nullptr;
        Manifold a = make(pa, na, ta, nta, err);
        if (*err) return 1;
        Manifold b = make(pb, nb, tb, ntb, err);
        if (*err) return 1;
        manifold::OpType t = op == 0 ? manifold::OpType::Add : op == 1 ? manifold::OpType::Subtract : manifold::OpType::Intersect;
        Manifold r = a.Boolean(b, t);
        if (r.Status() != Manifold::Error::NoError) {
            *err = status_text(r.Status());
            return 1;
        }
        MeshGL64 g = r.GetMeshGL64();
        const size_t np = g.vertProperties.size() / g.numProp;
        *out_np = np;
        *out_nt = g.triVerts.size() / 3;
        *out_pos = static_cast<double*>(std::malloc(sizeof(double) * 3 * (np ? np : 1)));
        *out_tri = static_cast<uint32_t*>(std::malloc(sizeof(uint32_t) * 3 * (*out_nt ? *out_nt : 1)));
        for (size_t i = 0; i < np; ++i)
            for (int k = 0; k < 3; ++k) (*out_pos)[3 * i + k] = g.vertProperties[i * g.numProp + k];
        for (size_t i = 0; i < g.triVerts.size(); ++i) (*out_tri)[i] = static_cast<uint32_t>(g.triVerts[i]);
        return 0;
    } catch (const std::exception& e) {
        *err = "the boolean operation failed inside the library";
        return 1;
    } catch (...) {
        *err = "the boolean operation failed inside the library";
        return 1;
    }
}

// The volume of the mesh as Manifold sees it (0 and an error when it is not a closed surface).
int rm_volume(const double* pa, size_t na, const uint32_t* ta, size_t nta, double* volume, const char** err) {
    try {
        *err = nullptr;
        Manifold a = make(pa, na, ta, nta, err);
        if (*err) return 1;
        *volume = a.Volume();
        return 0;
    } catch (...) {
        *err = "the mesh was refused";
        return 1;
    }
}

void rm_free(void* p) { std::free(p); }

}
