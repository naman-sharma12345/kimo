// Narrow C ABI over vendored Manifold 3.5.3 (Apache-2.0, vendor/manifold-3.5.3/LICENSE; Sun fdlibm
// notice retained in vendor/manifold-3.5.3/include/manifold/math.h and vendor/notices/manifold-3.5.3/).
// One boolean op per call; buffers are malloc'd and freed by kimo_manifold_free. No exceptions cross
// the boundary; errors return codes plus a message in err.
#include <manifold/manifold.h>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <new>

using namespace manifold;

// ToString(Error) is behind MANIFOLD_DEBUG; name the public enum values directly instead.
static const char* err_name(Manifold::Error e) {
  switch (e) {
    case Manifold::Error::NoError: return "NoError";
    case Manifold::Error::NonFiniteVertex: return "NonFiniteVertex";
    case Manifold::Error::NotManifold: return "NotManifold";
    case Manifold::Error::VertexOutOfBounds: return "VertexOutOfBounds";
    case Manifold::Error::PropertiesWrongLength: return "PropertiesWrongLength";
    case Manifold::Error::MissingPositionProperties: return "MissingPositionProperties";
    case Manifold::Error::MergeVectorsDifferentLengths: return "MergeVectorsDifferentLengths";
    case Manifold::Error::MergeIndexOutOfBounds: return "MergeIndexOutOfBounds";
    case Manifold::Error::TransformWrongLength: return "TransformWrongLength";
    case Manifold::Error::RunIndexWrongLength: return "RunIndexWrongLength";
    case Manifold::Error::FaceIDWrongLength: return "FaceIDWrongLength";
    case Manifold::Error::InvalidConstruction: return "InvalidConstruction";
    default: return "Unknown";
  }
}

static Manifold to_manifold(const double* pos, size_t nv, const uint32_t* tri, size_t nt) {
  MeshGL64 m;
  m.numProp = 3;
  m.vertProperties.assign(pos, pos + nv * 3);
  m.triVerts.assign(tri, tri + nt * 3);
  return Manifold(m);
}

extern "C" int32_t kimo_manifold_boolean(
    const double* a_pos, size_t a_nv, const uint32_t* a_tri, size_t a_nt,
    const double* b_pos, size_t b_nv, const uint32_t* b_tri, size_t b_nt,
    int32_t op, double** out_pos, size_t* out_nv, uint32_t** out_tri,
    size_t* out_nt, double* out_volume, char* err, size_t err_len) {
  *out_pos = nullptr; *out_tri = nullptr; *out_nv = 0; *out_nt = 0; *out_volume = 0.0;
  if (!a_pos || !a_tri || !b_pos || !b_tri) return 2;
  Manifold a = to_manifold(a_pos, a_nv, a_tri, a_nt);
  if (a.Status() != Manifold::Error::NoError) {
    std::snprintf(err, err_len, "input A not manifold: %s", err_name(a.Status()));
    return 3;
  }
  Manifold b = to_manifold(b_pos, b_nv, b_tri, b_nt);
  if (b.Status() != Manifold::Error::NoError) {
    std::snprintf(err, err_len, "input B not manifold: %s", err_name(b.Status()));
    return 4;
  }
  Manifold r;
  switch (op) {
    case 0: r = a + b; break;
    case 1: r = a - b; break;
    case 2: r = a ^ b; break;
    default: return 2;
  }
  if (r.Status() != Manifold::Error::NoError) {
    std::snprintf(err, err_len, "boolean failed: %s", err_name(r.Status()));
    return 5;
  }
  if (r.IsEmpty()) return 0;  // clean empty result: null buffers, zero counts
  MeshGL64 mg = r.GetMeshGL64();
  size_t nv = mg.NumVert(), nt = mg.NumTri();
  double* p = static_cast<double*>(std::malloc(nv * 3 * sizeof(double)));
  uint32_t* t = static_cast<uint32_t*>(std::malloc(nt * 3 * sizeof(uint32_t)));
  if (!p || !t) { std::free(p); std::free(t); return 6; }
  for (size_t i = 0; i < nv * 3; i++) p[i] = mg.vertProperties[i];
  for (size_t i = 0; i < nt * 3; i++) {
    if (mg.triVerts[i] > 0xFFFFFFFFu) { std::free(p); std::free(t); return 6; }
    t[i] = static_cast<uint32_t>(mg.triVerts[i]);
  }
  *out_pos = p; *out_tri = t; *out_nv = nv; *out_nt = nt;
  *out_volume = r.Volume();
  return 0;
}

extern "C" void kimo_manifold_free(double* p, uint32_t* t) {
  std::free(p); std::free(t);
}
