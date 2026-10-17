/* DongArch3D / K-ArchProfile rim detection, Boundary mode (GPLv3+) */

#ifndef DONGARCH_KARCH_RIM_DETECTOR_H
#define DONGARCH_KARCH_RIM_DETECTOR_H

#include <array>
#include <string>
#include <vector>

namespace DongArch {
namespace KArch {

struct Point3 {
    double x = 0.0;
    double y = 0.0;
    double z = 0.0;
};

struct MeshData {
    std::vector<Point3> vertices;
    std::vector<std::array<int, 3>> faces;
    Point3 bboxMin{};
    Point3 bboxMax{};
    double meanEdgeLength = 0.0;
    bool hasBoundary = false;
};

enum class RimError {
    Ok = 0,
    RimNotFound,
    RimTooSmall
};

enum class RimMode {
    Auto = 0,
    Boundary,
    Curvature,
    Manual
};

struct RimResult {
    std::vector<Point3> rimPoints;
    double meanZ = 0.0;
    double length = 0.0;
    int loopCount = 0;
    double planarity = 0.0;
    RimMode usedMode = RimMode::Auto;
    RimError error = RimError::RimNotFound;
    std::string message;
    std::string warning;
};

// Boundary edges (one adjacent face) -> closed loops.
// Drop loops shorter than 30% of the longest. Pick highest mean Z; loops within 0.5 mm of
// the highest mean Z are a tie: largest enclosed area (outer lip). Fewer than 20 vertices -> RimTooSmall.
RimResult DetectRimBoundary(const MeshData& mesh);

// Auto: Boundary, then Curvature + "RIM_MODE_FALLBACK".
// Curvature: top 15% Z, cotan Laplace, high-curvature outer ring.
// Manual: plane through 3 points, |dist|<tol (default 1 mm) and top 30% Z.
RimResult DetectRim(const MeshData& mesh, RimMode mode = RimMode::Auto,
                    const std::vector<Point3>& manualPts = {},
                    double manualTolMm = 1.0);

} // namespace KArch
} // namespace DongArch

#endif
