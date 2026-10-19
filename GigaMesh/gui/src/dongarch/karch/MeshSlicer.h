/* DongArch3D / K-ArchProfile MeshData plane slicer (plan 5.5) (GPLv3+) */

#ifndef DONGARCH_KARCH_MESH_SLICER_H
#define DONGARCH_KARCH_MESH_SLICER_H

#include "RimDetector.h"

#include <vector>

namespace DongArch {
namespace KArch {

struct Plane {
  Point3 n = {0, 0, 1};
  double d = 0.0; // n·x + d = 0
};

struct Polyline3 {
  std::vector<Point3> pts;
};

// Compact Linear BVH Node for accelerated spatial queries
struct MeshBVHNode {
  Point3 bboxMin{};
  Point3 bboxMax{};
  int left = -1; // -1 if leaf
  int right = -1;
  int firstFace = 0;
  int faceCount = 0;
};

class MeshBVH {
public:
  MeshBVH() = default;
  explicit MeshBVH(const MeshData &mesh) { build(mesh); }
  void build(const MeshData &mesh);
  void clear() {
    nodes_.clear();
    faceIndices_.clear();
  }
  bool empty() const { return nodes_.empty(); }
  void queryPlane(const Plane &pl, double onEps,
                  std::vector<int> &candidates) const;

  const std::vector<MeshBVHNode> &nodes() const { return nodes_; }
  const std::vector<int> &faceIndices() const { return faceIndices_; }

private:
  std::vector<MeshBVHNode> nodes_;
  std::vector<int> faceIndices_;
};

double SignedDistance(const Plane &pl, const Point3 &p);
std::vector<Polyline3> Slice(const MeshData &mesh, const Plane &pl,
                             const MeshBVH *bvh = nullptr);
std::vector<std::vector<Polyline3>>
SliceHorizontalBatch(const MeshData &mesh, const std::vector<double> &zHeights);

} // namespace KArch
} // namespace DongArch

#endif
