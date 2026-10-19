/* DongArch3D / K-ArchProfile T1 mesh I/O (GPLv3+) */

#include "MeshLoader.h"
#include "SyntheticMeshGen.h"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <cstdint>
#include <fstream>
#include <iomanip>
#include <locale>
#include <map>
#include <sstream>
#include <utility>

namespace DongArch {
namespace KArch {

static std::string lowerExt(const std::string &path) {
  const auto dot = path.find_last_of('.');
  if (dot == std::string::npos) {
    return {};
  }
  std::string e = path.substr(dot);
  for (char &c : e) {
    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  return e;
}

static double dist3(const Point3 &a, const Point3 &b) {
  return std::hypot(a.x - b.x, std::hypot(a.y - b.y, a.z - b.z));
}

static void computeBbox(const MeshData &mesh, Point3 &mn, Point3 &mx) {
  if (mesh.vertices.empty()) {
    mn = Point3{};
    mx = Point3{};
    return;
  }
  mn = mx = mesh.vertices.front();
  for (const Point3 &p : mesh.vertices) {
    mn.x = std::min(mn.x, p.x);
    mn.y = std::min(mn.y, p.y);
    mn.z = std::min(mn.z, p.z);
    mx.x = std::max(mx.x, p.x);
    mx.y = std::max(mx.y, p.y);
    mx.z = std::max(mx.z, p.z);
  }
}

static void weldAndClean(MeshData &mesh) {
  Point3 mn{};
  Point3 mx{};
  computeBbox(mesh, mn, mx);
  const double diag = dist3(mn, mx);
  const double eps = (diag > 0.0) ? (1e-6 * diag) : 0.0;

  if (eps > 0.0 && !mesh.vertices.empty()) {
    const double inv = 1.0 / eps;
    struct Key {
      long long x = 0;
      long long y = 0;
      long long z = 0;
      bool operator<(const Key &o) const {
        if (x != o.x) {
          return x < o.x;
        }
        if (y != o.y) {
          return y < o.y;
        }
        return z < o.z;
      }
    };
    std::map<Key, int> first;
    std::vector<int> remap(mesh.vertices.size(), -1);
    std::vector<Point3> kept;
    kept.reserve(mesh.vertices.size());
    for (std::size_t i = 0; i < mesh.vertices.size(); ++i) {
      const Point3 &p = mesh.vertices[i];
      const Key k{std::llround(p.x * inv), std::llround(p.y * inv),
                  std::llround(p.z * inv)};
      const auto it = first.find(k);
      if (it == first.end()) {
        first.emplace(k, static_cast<int>(kept.size()));
        remap[i] = static_cast<int>(kept.size());
        kept.push_back(p);
      } else {
        remap[i] = it->second;
      }
    }
    std::vector<std::array<int, 3>> faces;
    faces.reserve(mesh.faces.size());
    for (const auto &f : mesh.faces) {
      const int a = remap[static_cast<std::size_t>(f[0])];
      const int b = remap[static_cast<std::size_t>(f[1])];
      const int c = remap[static_cast<std::size_t>(f[2])];
      if (a < 0 || b < 0 || c < 0 || a == b || b == c || c == a) {
        continue;
      }
      const Point3 &pa = kept[static_cast<std::size_t>(a)];
      const Point3 &pb = kept[static_cast<std::size_t>(b)];
      const Point3 &pc = kept[static_cast<std::size_t>(c)];
      const double ux = pb.x - pa.x;
      const double uy = pb.y - pa.y;
      const double uz = pb.z - pa.z;
      const double vx = pc.x - pa.x;
      const double vy = pc.y - pa.y;
      const double vz = pc.z - pa.z;
      const double cx = uy * vz - uz * vy;
      const double cy = uz * vx - ux * vz;
      const double cz = ux * vy - uy * vx;
      if (std::hypot(cx, std::hypot(cy, cz)) < 1e-18) {
        continue;
      }
      faces.push_back({a, b, c});
    }
    mesh.vertices = std::move(kept);
    mesh.faces = std::move(faces);
  } else {
    std::vector<std::array<int, 3>> faces;
    faces.reserve(mesh.faces.size());
    const int n = static_cast<int>(mesh.vertices.size());
    for (const auto &f : mesh.faces) {
      if (f[0] < 0 || f[0] >= n || f[1] < 0 || f[1] >= n || f[2] < 0 ||
          f[2] >= n || f[0] == f[1] || f[1] == f[2] || f[2] == f[0]) {
        continue;
      }
      faces.push_back(f);
    }
    mesh.faces = std::move(faces);
  }
}

void FillMeshStats(MeshData &mesh) {
  computeBbox(mesh, mesh.bboxMin, mesh.bboxMax);
  mesh.meanEdgeLength = 0.0;
  mesh.hasBoundary = false;
  if (mesh.faces.empty() || mesh.vertices.empty()) {
    return;
  }

  const std::size_t numFaces = mesh.faces.size();
  std::vector<uint64_t> edges;
  edges.reserve(numFaces * 3);
  for (const auto &f : mesh.faces) {
    const int e[3][2] = {{f[0], f[1]}, {f[1], f[2]}, {f[2], f[0]}};
    for (const auto &ab : e) {
      const uint32_t u = static_cast<uint32_t>((ab[0] < ab[1]) ? ab[0] : ab[1]);
      const uint32_t v = static_cast<uint32_t>((ab[0] < ab[1]) ? ab[1] : ab[0]);
      edges.push_back((static_cast<uint64_t>(u) << 32) |
                      static_cast<uint64_t>(v));
    }
  }
  std::sort(edges.begin(), edges.end());

  double edgeSum = 0.0;
  int edgeN = 0;
  const std::size_t nEdges = edges.size();
  std::size_t i = 0;
  while (i < nEdges) {
    const uint64_t key = edges[i];
    std::size_t j = i + 1;
    while (j < nEdges && edges[j] == key) {
      ++j;
    }
    const std::size_t count = j - i;
    if (count == 1) {
      mesh.hasBoundary = true;
    }
    const uint32_t u = static_cast<uint32_t>(key >> 32);
    const uint32_t v = static_cast<uint32_t>(key & 0xFFFFFFFFULL);
    if (static_cast<std::size_t>(u) < mesh.vertices.size() &&
        static_cast<std::size_t>(v) < mesh.vertices.size()) {
      edgeSum += dist3(mesh.vertices[u], mesh.vertices[v]);
      ++edgeN;
    }
    i = j;
  }
  if (edgeN > 0) {
    mesh.meanEdgeLength = edgeSum / static_cast<double>(edgeN);
  }
}

static LoadResult finish(MeshData mesh, double unitScale) {
  LoadResult r;
  if (mesh.vertices.empty() || mesh.faces.empty()) {
    r.error = LoadError::EmptyMesh;
    r.message = "empty mesh";
    return r;
  }
  if (unitScale != 1.0) {
    for (Point3 &p : mesh.vertices) {
      p.x *= unitScale;
      p.y *= unitScale;
      p.z *= unitScale;
    }
  }
  weldAndClean(mesh);
  if (mesh.vertices.empty() || mesh.faces.empty()) {
    r.error = LoadError::EmptyMesh;
    r.message = "empty after clean";
    return r;
  }
  FillMeshStats(mesh);
  r.mesh = std::move(mesh);
  r.error = LoadError::Ok;
  r.message = "ok";
  return r;
}

static bool parseObj(const std::string &path, MeshData &dest) {
  dest.vertices.clear();
  dest.faces.clear();
  std::ifstream in(path, std::ios::in | std::ios::binary);
  if (!in) {
    return false;
  }
  in.imbue(std::locale::classic());
  std::string line;
  while (std::getline(in, line)) {
    if (!line.empty() && line.back() == '\r') {
      line.pop_back();
    }
    if (line.empty() || line[0] == '#') {
      continue;
    }
    std::istringstream ls(line);
    ls.imbue(std::locale::classic());
    std::string tok;
    ls >> tok;
    if (tok == "v") {
      Point3 p{};
      if (!(ls >> p.x >> p.y >> p.z)) {
        return false;
      }
      dest.vertices.push_back(p);
    } else if (tok == "f") {
      std::vector<int> idx;
      std::string corner;
      while (ls >> corner) {
        const auto slash = corner.find('/');
        const std::string vs =
            (slash == std::string::npos) ? corner : corner.substr(0, slash);
        int v = 0;
        try {
          v = std::stoi(vs);
        } catch (...) {
          return false;
        }
        if (v < 0) {
          v = static_cast<int>(dest.vertices.size()) + v + 1;
        }
        if (v <= 0) {
          return false;
        }
        idx.push_back(v - 1);
      }
      if (idx.size() < 3) {
        return false;
      }
      for (std::size_t i = 1; i + 1 < idx.size(); ++i) {
        dest.faces.push_back({idx[0], idx[i], idx[i + 1]});
      }
    }
  }
  return true;
}

LoadResult LoadMesh(const std::string &path, double unitScale) {
  LoadResult r;
  std::ifstream probe(path, std::ios::binary);
  if (!probe) {
    r.error = LoadError::FileNotFound;
    r.message = "file not found";
    return r;
  }
  probe.close();

  const std::string ext = lowerExt(path);
  MeshData mesh;
  if (ext == ".ply") {
    if (!Testing::ReadPlyAscii(path, mesh)) {
      r.error = LoadError::ParseFailed;
      r.message = "ply parse failed";
      return r;
    }
  } else if (ext == ".obj") {
    if (!parseObj(path, mesh)) {
      r.error = LoadError::ParseFailed;
      r.message = "obj parse failed";
      return r;
    }
  } else {
    r.error = LoadError::UnsupportedFormat;
    r.message = "unsupported format";
    return r;
  }
  return finish(std::move(mesh), unitScale);
}

bool WriteObjAscii(const MeshData &mesh, const std::string &path) {
  std::ofstream out(path, std::ios::out | std::ios::binary);
  if (!out) {
    return false;
  }
  out.imbue(std::locale::classic());
  out << "# karch\n";
  out << std::setprecision(17);
  for (const Point3 &v : mesh.vertices) {
    out << "v " << v.x << ' ' << v.y << ' ' << v.z << '\n';
  }
  for (const auto &f : mesh.faces) {
    out << "f " << (f[0] + 1) << ' ' << (f[1] + 1) << ' ' << (f[2] + 1) << '\n';
  }
  return static_cast<bool>(out);
}

} // namespace KArch
} // namespace DongArch
