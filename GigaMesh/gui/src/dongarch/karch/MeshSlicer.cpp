/* DongArch3D / K-ArchProfile MeshData plane slicer (plan 5.5) (GPLv3+) */

#include "MeshSlicer.h"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <deque>
#include <thread>
#include <unordered_map>
#include <utility>
#include <vector>

namespace DongArch {
namespace KArch {
namespace {

struct Seg {
  Point3 a;
  Point3 b;
  bool used = false;
};

double dist2(const Point3 &a, const Point3 &b) {
  const double dx = a.x - b.x;
  const double dy = a.y - b.y;
  const double dz = a.z - b.z;
  return dx * dx + dy * dy + dz * dz;
}

double bboxDiag(const MeshData &mesh) {
  if (mesh.vertices.empty()) {
    return 1.0;
  }
  if (mesh.bboxMax.x > mesh.bboxMin.x || mesh.bboxMax.y > mesh.bboxMin.y ||
      mesh.bboxMax.z > mesh.bboxMin.z) {
    return std::sqrt(dist2(mesh.bboxMin, mesh.bboxMax));
  }
  Point3 mn = mesh.vertices[0];
  Point3 mx = mesh.vertices[0];
  for (const Point3 &p : mesh.vertices) {
    mn.x = (p.x < mn.x) ? p.x : mn.x;
    mn.y = (p.y < mn.y) ? p.y : mn.y;
    mn.z = (p.z < mn.z) ? p.z : mn.z;
    mx.x = (p.x > mx.x) ? p.x : mx.x;
    mx.y = (p.y > mx.y) ? p.y : mx.y;
    mx.z = (p.z > mx.z) ? p.z : mx.z;
  }
  return std::sqrt(dist2(mn, mx));
}

Point3 lerp(const Point3 &a, const Point3 &b, double t) {
  return Point3{a.x + t * (b.x - a.x), a.y + t * (b.y - a.y),
                a.z + t * (b.z - a.z)};
}

struct BVHItem {
  float cx, cy, cz;
  int faceIdx;
};

} // namespace

void MeshBVH::build(const MeshData &mesh) {
  clear();
  const int numFaces = static_cast<int>(mesh.faces.size());
  const int numVerts = static_cast<int>(mesh.vertices.size());
  if (numFaces <= 0 || numVerts <= 0) {
    return;
  }

  const unsigned int nThreads =
      (numFaces > 100000) ? std::max(1u, std::thread::hardware_concurrency())
                          : 1u;
  std::vector<std::vector<BVHItem>> threadItems(nThreads);
  std::vector<std::thread> workers;
  workers.reserve(nThreads);
  for (unsigned int t = 0; t < nThreads; ++t) {
    workers.emplace_back([&, t]() {
      const int f0 =
          static_cast<int>(t * static_cast<unsigned int>(numFaces) / nThreads);
      const int f1 = static_cast<int>(
          (t + 1) * static_cast<unsigned int>(numFaces) / nThreads);
      auto &tItems = threadItems[t];
      tItems.reserve(static_cast<std::size_t>(f1 - f0));
      for (int i = f0; i < f1; ++i) {
        const auto &f = mesh.faces[static_cast<std::size_t>(i)];
        if (f[0] < 0 || f[1] < 0 || f[2] < 0 || f[0] >= numVerts ||
            f[1] >= numVerts || f[2] >= numVerts) {
          continue;
        }
        const Point3 &v0 = mesh.vertices[static_cast<std::size_t>(f[0])];
        const Point3 &v1 = mesh.vertices[static_cast<std::size_t>(f[1])];
        const Point3 &v2 = mesh.vertices[static_cast<std::size_t>(f[2])];

        const float cx = static_cast<float>((v0.x + v1.x + v2.x) * (1.0 / 3.0));
        const float cy = static_cast<float>((v0.y + v1.y + v2.y) * (1.0 / 3.0));
        const float cz = static_cast<float>((v0.z + v1.z + v2.z) * (1.0 / 3.0));
        tItems.push_back(BVHItem{cx, cy, cz, i});
      }
    });
  }
  for (auto &w : workers) {
    w.join();
  }
  std::vector<BVHItem> items;
  items.reserve(static_cast<std::size_t>(numFaces));
  for (auto &tItems : threadItems) {
    items.insert(items.end(), tItems.begin(), tItems.end());
  }

  if (items.empty()) {
    return;
  }

  faceIndices_.resize(items.size());
  nodes_.reserve(items.size() * 2);

  auto buildRecursive = [&](auto &self, int start, int end) -> int {
    const int count = end - start;
    const int nodeIdx = static_cast<int>(nodes_.size());
    nodes_.emplace_back();

    if (count <= 128) {
      nodes_[static_cast<std::size_t>(nodeIdx)].left = -1;
      nodes_[static_cast<std::size_t>(nodeIdx)].right = -1;
      nodes_[static_cast<std::size_t>(nodeIdx)].firstFace = start;
      nodes_[static_cast<std::size_t>(nodeIdx)].faceCount = count;
      Point3 bMin{1e30, 1e30, 1e30};
      Point3 bMax{-1e30, -1e30, -1e30};
      for (int i = start; i < end; ++i) {
        const int fi = items[static_cast<std::size_t>(i)].faceIdx;
        faceIndices_[static_cast<std::size_t>(i)] = fi;
        const auto &f = mesh.faces[static_cast<std::size_t>(fi)];
        for (int k = 0; k < 3; ++k) {
          const Point3 &v = mesh.vertices[static_cast<std::size_t>(f[k])];
          bMin.x = std::min(bMin.x, v.x);
          bMin.y = std::min(bMin.y, v.y);
          bMin.z = std::min(bMin.z, v.z);
          bMax.x = std::max(bMax.x, v.x);
          bMax.y = std::max(bMax.y, v.y);
          bMax.z = std::max(bMax.z, v.z);
        }
      }
      nodes_[static_cast<std::size_t>(nodeIdx)].bboxMin = bMin;
      nodes_[static_cast<std::size_t>(nodeIdx)].bboxMax = bMax;
      return nodeIdx;
    }

    float cMinX = items[static_cast<std::size_t>(start)].cx;
    float cMaxX = cMinX;
    float cMinY = items[static_cast<std::size_t>(start)].cy;
    float cMaxY = cMinY;
    float cMinZ = items[static_cast<std::size_t>(start)].cz;
    float cMaxZ = cMinZ;

    for (int i = start + 1; i < end; ++i) {
      const auto &it = items[static_cast<std::size_t>(i)];
      cMinX = std::min(cMinX, it.cx);
      cMaxX = std::max(cMaxX, it.cx);
      cMinY = std::min(cMinY, it.cy);
      cMaxY = std::max(cMaxY, it.cy);
      cMinZ = std::min(cMinZ, it.cz);
      cMaxZ = std::max(cMaxZ, it.cz);
    }

    const float extX = cMaxX - cMinX;
    const float extY = cMaxY - cMinY;
    const float extZ = cMaxZ - cMinZ;
    int axis = 0;
    if (extY > extX && extY > extZ) {
      axis = 1;
    } else if (extZ > extX && extZ > extY) {
      axis = 2;
    }

    const int mid = start + count / 2;
    auto comp = [axis](const BVHItem &a, const BVHItem &b) {
      if (axis == 0)
        return a.cx < b.cx;
      if (axis == 1)
        return a.cy < b.cy;
      return a.cz < b.cz;
    };
    std::nth_element(items.begin() + start, items.begin() + mid,
                     items.begin() + end, comp);

    const int l = self(self, start, mid);
    const int r = self(self, mid, end);
    nodes_[static_cast<std::size_t>(nodeIdx)].left = l;
    nodes_[static_cast<std::size_t>(nodeIdx)].right = r;
    nodes_[static_cast<std::size_t>(nodeIdx)].firstFace = 0;
    nodes_[static_cast<std::size_t>(nodeIdx)].faceCount = 0;
    nodes_[static_cast<std::size_t>(nodeIdx)].bboxMin =
        Point3{std::min(nodes_[static_cast<std::size_t>(l)].bboxMin.x,
                        nodes_[static_cast<std::size_t>(r)].bboxMin.x),
               std::min(nodes_[static_cast<std::size_t>(l)].bboxMin.y,
                        nodes_[static_cast<std::size_t>(r)].bboxMin.y),
               std::min(nodes_[static_cast<std::size_t>(l)].bboxMin.z,
                        nodes_[static_cast<std::size_t>(r)].bboxMin.z)};
    nodes_[static_cast<std::size_t>(nodeIdx)].bboxMax =
        Point3{std::max(nodes_[static_cast<std::size_t>(l)].bboxMax.x,
                        nodes_[static_cast<std::size_t>(r)].bboxMax.x),
               std::max(nodes_[static_cast<std::size_t>(l)].bboxMax.y,
                        nodes_[static_cast<std::size_t>(r)].bboxMax.y),
               std::max(nodes_[static_cast<std::size_t>(l)].bboxMax.z,
                        nodes_[static_cast<std::size_t>(r)].bboxMax.z)};
    return nodeIdx;
  };

  buildRecursive(buildRecursive, 0, static_cast<int>(items.size()));
}

void MeshBVH::queryPlane(const Plane &pl, double onEps,
                         std::vector<int> &candidates) const {
  if (nodes_.empty()) {
    return;
  }
  std::vector<int> stack;
  stack.reserve(128);
  stack.push_back(0);

  while (!stack.empty()) {
    const int nodeIdx = stack.back();
    stack.pop_back();
    const auto &node = nodes_[static_cast<std::size_t>(nodeIdx)];

    Point3 pMin, pMax;
    pMin.x = (pl.n.x >= 0.0) ? node.bboxMin.x : node.bboxMax.x;
    pMax.x = (pl.n.x >= 0.0) ? node.bboxMax.x : node.bboxMin.x;
    pMin.y = (pl.n.y >= 0.0) ? node.bboxMin.y : node.bboxMax.y;
    pMax.y = (pl.n.y >= 0.0) ? node.bboxMax.y : node.bboxMin.y;
    pMin.z = (pl.n.z >= 0.0) ? node.bboxMin.z : node.bboxMax.z;
    pMax.z = (pl.n.z >= 0.0) ? node.bboxMax.z : node.bboxMin.z;

    const double dMin = SignedDistance(pl, pMin);
    const double dMax = SignedDistance(pl, pMax);

    if (dMin > onEps || dMax < -onEps) {
      continue;
    }

    if (node.left == -1) {
      for (int i = 0; i < node.faceCount; ++i) {
        candidates.push_back(
            faceIndices_[static_cast<std::size_t>(node.firstFace + i)]);
      }
    } else {
      stack.push_back(node.left);
      stack.push_back(node.right);
    }
  }
}

double SignedDistance(const Plane &pl, const Point3 &p) {
  return pl.n.x * p.x + pl.n.y * p.y + pl.n.z * p.z + pl.d;
}

std::vector<Polyline3> Slice(const MeshData &mesh, const Plane &pl,
                             const MeshBVH *bvh) {
  const double diag = bboxDiag(mesh);
  const double weld = 1e-6 * (diag > 0.0 ? diag : 1.0);
  const double weld2 = weld * weld;
  const double onEps = 1e-12 * (diag > 0.0 ? diag : 1.0);

  std::vector<Seg> segs;
  const int nv = static_cast<int>(mesh.vertices.size());
  if (nv == 0 || mesh.faces.empty()) {
    return {};
  }

  auto processFace = [&](const std::array<int, 3> &f) {
    if (f[0] < 0 || f[1] < 0 || f[2] < 0 || f[0] >= nv || f[1] >= nv ||
        f[2] >= nv) {
      return;
    }
    const Point3 &v0 = mesh.vertices[static_cast<std::size_t>(f[0])];
    const Point3 &v1 = mesh.vertices[static_cast<std::size_t>(f[1])];
    const Point3 &v2 = mesh.vertices[static_cast<std::size_t>(f[2])];
    const double s0 = SignedDistance(pl, v0);
    const double s1 = SignedDistance(pl, v1);
    const double s2 = SignedDistance(pl, v2);

    // Feature 2: Early triangle reject (99.8% pruned)
    if ((s0 > onEps && s1 > onEps && s2 > onEps) ||
        (s0 < -onEps && s1 < -onEps && s2 < -onEps)) {
      return;
    }

    Point3 hits[4];
    int hitCount = 0;

    auto addHitStack = [&](const Point3 &p) {
      for (int k = 0; k < hitCount; ++k) {
        if (dist2(hits[k], p) <= weld2) {
          return;
        }
      }
      if (hitCount < 4) {
        hits[hitCount++] = p;
      }
    };

    auto edgeHitsStack = [&](const Point3 &a, const Point3 &b, double sa,
                             double sb) {
      if (std::fabs(sa - sb) < 1e-12) {
        return;
      }
      const bool aOn = std::fabs(sa) <= onEps;
      const bool bOn = std::fabs(sb) <= onEps;
      if (aOn && bOn) {
        return;
      }
      if (aOn) {
        addHitStack(a);
        return;
      }
      if (bOn) {
        addHitStack(b);
        return;
      }
      if (sa * sb < 0.0) {
        const double t = -sa / (sb - sa);
        addHitStack(lerp(a, b, t));
      }
    };

    edgeHitsStack(v0, v1, s0, s1);
    edgeHitsStack(v1, v2, s1, s2);
    edgeHitsStack(v2, v0, s2, s0);

    if (hitCount == 2 && dist2(hits[0], hits[1]) > weld2) {
      segs.push_back(Seg{hits[0], hits[1], false});
    }
  };

  if (bvh != nullptr && !bvh->empty()) {
    std::vector<int> candidates;
    candidates.reserve(4096);
    bvh->queryPlane(pl, onEps, candidates);
    for (int fIdx : candidates) {
      processFace(mesh.faces[static_cast<std::size_t>(fIdx)]);
    }
  } else {
    const std::size_t totalFaces = mesh.faces.size();
    if (totalFaces > 50000) {
      const unsigned int nThreads =
          std::max(1u, std::thread::hardware_concurrency());
      std::vector<std::vector<Seg>> threadSegs(nThreads);
      std::vector<std::thread> workers;
      workers.reserve(nThreads);
      for (unsigned int t = 0; t < nThreads; ++t) {
        workers.emplace_back([&, t]() {
          const std::size_t f0 = t * totalFaces / nThreads;
          const std::size_t f1 = (t + 1) * totalFaces / nThreads;
          auto &localSegs = threadSegs[t];
          localSegs.reserve(256);
          for (std::size_t fi = f0; fi < f1; ++fi) {
            const auto &f = mesh.faces[fi];
            if (f[0] < 0 || f[1] < 0 || f[2] < 0 || f[0] >= nv || f[1] >= nv ||
                f[2] >= nv) {
              continue;
            }
            const Point3 &v0 = mesh.vertices[static_cast<std::size_t>(f[0])];
            const Point3 &v1 = mesh.vertices[static_cast<std::size_t>(f[1])];
            const Point3 &v2 = mesh.vertices[static_cast<std::size_t>(f[2])];
            const double s0 = SignedDistance(pl, v0);
            const double s1 = SignedDistance(pl, v1);
            const double s2 = SignedDistance(pl, v2);
            if ((s0 > onEps && s1 > onEps && s2 > onEps) ||
                (s0 < -onEps && s1 < -onEps && s2 < -onEps)) {
              continue;
            }
            Point3 hits[4];
            int hitCount = 0;
            auto addHit = [&](const Point3 &p) {
              for (int k = 0; k < hitCount; ++k) {
                if (dist2(hits[k], p) <= weld2)
                  return;
              }
              if (hitCount < 4) {
                hits[hitCount++] = p;
              }
            };
            auto edgeHits = [&](const Point3 &a, const Point3 &b, double sa,
                                double sb) {
              if (std::fabs(sa - sb) < 1e-12)
                return;
              const bool aOn = std::fabs(sa) <= onEps;
              const bool bOn = std::fabs(sb) <= onEps;
              if (aOn && bOn)
                return;
              if (aOn) {
                addHit(a);
                return;
              }
              if (bOn) {
                addHit(b);
                return;
              }
              if (sa * sb < 0.0) {
                const double tVal = -sa / (sb - sa);
                addHit(lerp(a, b, tVal));
              }
            };
            edgeHits(v0, v1, s0, s1);
            edgeHits(v1, v2, s1, s2);
            edgeHits(v2, v0, s2, s0);
            if (hitCount == 2 && dist2(hits[0], hits[1]) > weld2) {
              localSegs.push_back(Seg{hits[0], hits[1], false});
            }
          }
        });
      }
      for (auto &w : workers) {
        w.join();
      }
      for (auto &ts : threadSegs) {
        segs.insert(segs.end(), ts.begin(), ts.end());
      }
    } else {
      segs.reserve(std::min<std::size_t>(mesh.faces.size() / 100 + 64, 32768));
      for (const auto &f : mesh.faces) {
        processFace(f);
      }
    }
  }

  if (segs.empty()) {
    return {};
  }

  // Feature 3: Spatial Hash Grid Segment Chaining
  const double cellSize = std::max(weld * 2.0, 1e-9);
  const double invCell = 1.0 / cellSize;

  auto hashCoord = [](int64_t x, int64_t y, int64_t z) -> uint64_t {
    uint64_t h = static_cast<uint64_t>(x) * 73856093ULL ^
                 static_cast<uint64_t>(y) * 19349663ULL ^
                 static_cast<uint64_t>(z) * 83492791ULL;
    h ^= (h >> 33);
    h *= 0xff51afd7ed558ccdULL;
    h ^= (h >> 33);
    return h;
  };

  struct EndpointRef {
    int segIdx;
    bool isB;
  };

  std::unordered_map<uint64_t, std::vector<EndpointRef>> grid;
  grid.reserve(segs.size() * 2);

  for (int i = 0; i < static_cast<int>(segs.size()); ++i) {
    const int64_t ax = static_cast<int64_t>(
        std::floor(segs[static_cast<std::size_t>(i)].a.x * invCell));
    const int64_t ay = static_cast<int64_t>(
        std::floor(segs[static_cast<std::size_t>(i)].a.y * invCell));
    const int64_t az = static_cast<int64_t>(
        std::floor(segs[static_cast<std::size_t>(i)].a.z * invCell));
    grid[hashCoord(ax, ay, az)].push_back(EndpointRef{i, false});

    const int64_t bx = static_cast<int64_t>(
        std::floor(segs[static_cast<std::size_t>(i)].b.x * invCell));
    const int64_t by = static_cast<int64_t>(
        std::floor(segs[static_cast<std::size_t>(i)].b.y * invCell));
    const int64_t bz = static_cast<int64_t>(
        std::floor(segs[static_cast<std::size_t>(i)].b.z * invCell));
    grid[hashCoord(bx, by, bz)].push_back(EndpointRef{i, true});
  }

  auto findJoinSpatial = [&](const Point3 &p, bool *atB) -> int {
    const int64_t cx = static_cast<int64_t>(std::floor(p.x * invCell));
    const int64_t cy = static_cast<int64_t>(std::floor(p.y * invCell));
    const int64_t cz = static_cast<int64_t>(std::floor(p.z * invCell));
    for (int64_t dz = -1; dz <= 1; ++dz) {
      for (int64_t dy = -1; dy <= 1; ++dy) {
        for (int64_t dx = -1; dx <= 1; ++dx) {
          const uint64_t key = hashCoord(cx + dx, cy + dy, cz + dz);
          auto it = grid.find(key);
          if (it != grid.end()) {
            for (const auto &ref : it->second) {
              if (segs[static_cast<std::size_t>(ref.segIdx)].used) {
                continue;
              }
              const Point3 &pt =
                  ref.isB ? segs[static_cast<std::size_t>(ref.segIdx)].b
                          : segs[static_cast<std::size_t>(ref.segIdx)].a;
              if (dist2(pt, p) <= weld2) {
                *atB = ref.isB;
                return ref.segIdx;
              }
            }
          }
        }
      }
    }
    return -1;
  };

  std::vector<Polyline3> out;
  for (std::size_t i = 0; i < segs.size(); ++i) {
    if (segs[i].used) {
      continue;
    }
    segs[i].used = true;
    std::deque<Point3> dq;
    dq.push_back(segs[i].a);
    dq.push_back(segs[i].b);
    bool grew = true;
    while (grew) {
      grew = false;
      bool atB = false;
      int j = findJoinSpatial(dq.back(), &atB);
      if (j >= 0) {
        segs[static_cast<std::size_t>(j)].used = true;
        dq.push_back(atB ? segs[static_cast<std::size_t>(j)].a
                         : segs[static_cast<std::size_t>(j)].b);
        grew = true;
        continue;
      }
      j = findJoinSpatial(dq.front(), &atB);
      if (j >= 0) {
        segs[static_cast<std::size_t>(j)].used = true;
        dq.push_front(atB ? segs[static_cast<std::size_t>(j)].a
                          : segs[static_cast<std::size_t>(j)].b);
        grew = true;
      }
    }
    if (dq.size() >= 2) {
      Polyline3 line;
      line.pts.assign(dq.begin(), dq.end());
      out.push_back(std::move(line));
    }
  }
  return out;
}

std::vector<std::vector<Polyline3>>
SliceHorizontalBatch(const MeshData &mesh,
                     const std::vector<double> &zHeights) {
  std::vector<std::vector<Polyline3>> all;
  all.reserve(zHeights.size());
  MeshBVH bvh;
  if (mesh.faces.size() > 5000) {
    bvh.build(mesh);
  }
  const MeshBVH *bvhPtr = bvh.empty() ? nullptr : &bvh;
  for (double z : zHeights) {
    all.push_back(Slice(mesh, Plane{{0.0, 0.0, 1.0}, -z}, bvhPtr));
  }
  return all;
}

} // namespace KArch
} // namespace DongArch
