/* DongArch3D / K-ArchProfile rim detection, Boundary mode (GPLv3+) */

#include "RimDetector.h"
#include "AxisPca.h"

#include <algorithm>
#include <cmath>
#include <map>
#include <set>
#include <utility>

namespace DongArch {
namespace KArch {
namespace {

using Edge = std::pair<int, int>;

Edge makeEdge(int a, int b) {
    return (a < b) ? Edge{a, b} : Edge{b, a};
}

double dist(const Point3& a, const Point3& b) {
    const double dx = a.x - b.x;
    const double dy = a.y - b.y;
    const double dz = a.z - b.z;
    return std::sqrt(dx * dx + dy * dy + dz * dz);
}

struct Loop {
    std::vector<int> indices;
    double meanZ = 0.0;
    double length = 0.0;
    double area = 0.0; // enclosed area (Newell vector / 2): pose-independent
};

Loop finishLoop(const MeshData& mesh, const std::vector<int>& idx) {
    Loop loop;
    loop.indices = idx;
    if (idx.empty()) {
        return loop;
    }
    for (int i : idx) {
        loop.meanZ += mesh.vertices[static_cast<std::size_t>(i)].z;
    }
    loop.meanZ /= static_cast<double>(idx.size());
    double nx = 0.0;
    double ny = 0.0;
    double nz = 0.0;
    for (std::size_t i = 0; i < idx.size(); ++i) {
        const Point3& a = mesh.vertices[static_cast<std::size_t>(idx[i])];
        const Point3& b = mesh.vertices[static_cast<std::size_t>(idx[(i + 1) % idx.size()])];
        loop.length += dist(a, b);
        nx += (a.y - b.y) * (a.z + b.z);
        ny += (a.z - b.z) * (a.x + b.x);
        nz += (a.x - b.x) * (a.y + b.y);
    }
    loop.area = 0.5 * std::sqrt(nx * nx + ny * ny + nz * nz);
    return loop;
}

} // namespace

RimResult DetectRimBoundary(const MeshData& mesh) {
    RimResult result;
    result.error = RimError::RimNotFound;
    result.message = "boundary loop not found";

    if (mesh.vertices.empty() || mesh.faces.empty()) {
        return result;
    }

    std::map<Edge, int> edgeCount;
    for (const auto& f : mesh.faces) {
        if (f[0] < 0 || f[1] < 0 || f[2] < 0) {
            continue;
        }
        const int n = static_cast<int>(mesh.vertices.size());
        if (f[0] >= n || f[1] >= n || f[2] >= n) {
            continue;
        }
        ++edgeCount[makeEdge(f[0], f[1])];
        ++edgeCount[makeEdge(f[1], f[2])];
        ++edgeCount[makeEdge(f[2], f[0])];
    }

    std::map<int, std::vector<int>> adj;
    std::set<Edge> unused;
    for (const auto& [e, c] : edgeCount) {
        if (c != 1) {
            continue;
        }
        adj[e.first].push_back(e.second);
        adj[e.second].push_back(e.first);
        unused.insert(e);
    }

    auto peelAntennas = [&unused]() {
        bool peeled = true;
        while (peeled) {
            peeled = false;
            std::map<int, int> deg;
            for (const Edge& e : unused) {
                ++deg[e.first];
                ++deg[e.second];
            }
            std::vector<Edge> drop;
            for (const Edge& e : unused) {
                if (deg[e.first] == 1 || deg[e.second] == 1) {
                    drop.push_back(e);
                }
            }
            for (const Edge& e : drop) {
                unused.erase(e);
                peeled = true;
            }
        }
    };

    std::vector<Loop> loops;
    peelAntennas();
    while (!unused.empty()) {
        const Edge start = *unused.begin();
        std::set<Edge> trail;
        trail.insert(start);
        std::vector<int> idx;
        idx.push_back(start.first);
        idx.push_back(start.second);
        int prev = start.first;
        int cur = start.second;
        bool closed = false;
        while (true) {
            const auto found = adj.find(cur);
            if (found == adj.end()) {
                break;
            }
            int next = -1;
            int closeNb = -1;
            int stepNb = -1;
            for (int nb : found->second) {
                if (nb == prev) {
                    continue;
                }
                const Edge e = makeEdge(cur, nb);
                if (unused.count(e) == 0 || trail.count(e) != 0) {
                    continue;
                }
                if (nb == idx.front()) {
                    closeNb = nb;
                } else if (stepNb < 0) {
                    stepNb = nb;
                }
            }
            next = (closeNb >= 0) ? closeNb : stepNb;
            if (next < 0) {
                break;
            }
            trail.insert(makeEdge(cur, next));
            if (next == idx.front()) {
                closed = true;
                break;
            }
            idx.push_back(next);
            prev = cur;
            cur = next;
        }
        if (closed && idx.size() >= 3) {
            for (const Edge& e : trail) {
                unused.erase(e);
            }
            loops.push_back(finishLoop(mesh, idx));
        } else {
            unused.erase(start);
            peelAntennas();
        }
    }

    result.loopCount = static_cast<int>(loops.size());
    if (loops.empty()) {
        return result;
    }

    double maxLen = 0.0;
    for (const Loop& L : loops) {
        if (L.length > maxLen) {
            maxLen = L.length;
        }
    }

    std::vector<Loop> kept;
    kept.reserve(loops.size());
    for (const Loop& L : loops) {
        if (L.length + 1e-12 >= 0.30 * maxLen) {
            kept.push_back(L);
        }
    }
    if (kept.empty()) {
        return result;
    }

    // Loops whose mean Z is within kRimTieMm of the highest are the same height: take the one that
    // encloses the largest area (outer lip). Scan noise moves the inner and outer lip loops' mean Z
    // by ~0.003 mm, so an exact (1e-9) tie picked the inner lip at random (rim = inner diameter,
    // JOB-009). Area, not length: a jagged inner boundary can be longer than the outer one.
    constexpr double kRimTieMm = 0.5;
    double zTop = kept.front().meanZ;
    for (const Loop& L : kept) {
        zTop = std::max(zTop, L.meanZ);
    }
    std::size_t best = kept.size();
    for (std::size_t i = 0; i < kept.size(); ++i) {
        if (kept[i].meanZ >= zTop - kRimTieMm &&
            (best == kept.size() || kept[i].area > kept[best].area)) {
            best = i;
        }
    }

    const Loop& rim = kept[best];
    result.meanZ = rim.meanZ;
    result.length = rim.length;
    result.rimPoints.reserve(rim.indices.size());
    for (int i : rim.indices) {
        result.rimPoints.push_back(mesh.vertices[static_cast<std::size_t>(i)]);
    }

    if (result.rimPoints.size() < 20) {
        result.error = RimError::RimTooSmall;
        result.message = "rim has fewer than 20 vertices";
        return result;
    }

    result.error = RimError::Ok;
    result.message.clear();
    result.usedMode = RimMode::Boundary;
    return result;
}

static Point3 sub(const Point3& a, const Point3& b) {
    return Point3{a.x - b.x, a.y - b.y, a.z - b.z};
}

static double nrm(const Point3& p) {
    return std::sqrt(p.x * p.x + p.y * p.y + p.z * p.z);
}

static Point3 crs(const Point3& a, const Point3& b) {
    return Point3{a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x};
}

static double dt(const Point3& a, const Point3& b) {
    return a.x * b.x + a.y * b.y + a.z * b.z;
}

static double cotCorner(const Point3& u, const Point3& v) {
    const double s = nrm(crs(u, v));
    if (s < 1e-15) {
        return 0.0;
    }
    return dt(u, v) / s;
}

static void finishRimStats(RimResult& result, const MeshData& mesh) {
    if (result.rimPoints.empty()) {
        return;
    }
    result.meanZ = 0.0;
    result.length = 0.0;
    for (const Point3& p : result.rimPoints) {
        result.meanZ += p.z;
    }
    result.meanZ /= static_cast<double>(result.rimPoints.size());
    for (std::size_t i = 0; i < result.rimPoints.size(); ++i) {
        result.length += dist(result.rimPoints[i],
                              result.rimPoints[(i + 1) % result.rimPoints.size()]);
    }
    const auto pca = ComputePcaAlignment(result.rimPoints, MeshCentroid(mesh));
    result.planarity = pca.planarity;
}

static RimResult DetectRimCurvature(const MeshData& mesh) {
    RimResult result;
    result.usedMode = RimMode::Curvature;
    result.warning = "RIM_MODE_FALLBACK";
    result.error = RimError::RimNotFound;
    result.message = "curvature rim not found";
    if (mesh.vertices.size() < 20 || mesh.faces.empty()) {
        return result;
    }

    double zMin = mesh.vertices[0].z;
    double zMax = zMin;
    for (const Point3& p : mesh.vertices) {
        zMin = std::min(zMin, p.z);
        zMax = std::max(zMax, p.z);
    }
    const double zCut = zMax - 0.15 * std::max(zMax - zMin, 1e-12);

    const int n = static_cast<int>(mesh.vertices.size());
    std::vector<double> Hx(static_cast<std::size_t>(n), 0.0);
    std::vector<double> Hy(static_cast<std::size_t>(n), 0.0);
    std::vector<double> Hz(static_cast<std::size_t>(n), 0.0);
    std::vector<double> area(static_cast<std::size_t>(n), 0.0);

    auto addEdge = [&](int i, int j, double w) {
        const Point3 d = sub(mesh.vertices[static_cast<std::size_t>(j)],
                             mesh.vertices[static_cast<std::size_t>(i)]);
        Hx[static_cast<std::size_t>(i)] += w * d.x;
        Hy[static_cast<std::size_t>(i)] += w * d.y;
        Hz[static_cast<std::size_t>(i)] += w * d.z;
    };

    for (const auto& f : mesh.faces) {
        if (f[0] < 0 || f[1] < 0 || f[2] < 0 || f[0] >= n || f[1] >= n || f[2] >= n) {
            continue;
        }
        const Point3& pa = mesh.vertices[static_cast<std::size_t>(f[0])];
        const Point3& pb = mesh.vertices[static_cast<std::size_t>(f[1])];
        const Point3& pc = mesh.vertices[static_cast<std::size_t>(f[2])];
        const double c0 = cotCorner(sub(pb, pa), sub(pc, pa));
        const double c1 = cotCorner(sub(pa, pb), sub(pc, pb));
        const double c2 = cotCorner(sub(pa, pc), sub(pb, pc));
        addEdge(f[1], f[2], c0);
        addEdge(f[2], f[1], c0);
        addEdge(f[0], f[2], c1);
        addEdge(f[2], f[0], c1);
        addEdge(f[0], f[1], c2);
        addEdge(f[1], f[0], c2);
        const double a = 0.5 * nrm(crs(sub(pb, pa), sub(pc, pa)));
        area[static_cast<std::size_t>(f[0])] += a / 3.0;
        area[static_cast<std::size_t>(f[1])] += a / 3.0;
        area[static_cast<std::size_t>(f[2])] += a / 3.0;
    }

    std::vector<int> band;
    std::vector<double> mag;
    for (int i = 0; i < n; ++i) {
        if (mesh.vertices[static_cast<std::size_t>(i)].z < zCut) {
            continue;
        }
        const double A = std::max(area[static_cast<std::size_t>(i)], 1e-18);
        const double h = std::sqrt(Hx[static_cast<std::size_t>(i)] * Hx[static_cast<std::size_t>(i)] +
                                   Hy[static_cast<std::size_t>(i)] * Hy[static_cast<std::size_t>(i)] +
                                   Hz[static_cast<std::size_t>(i)] * Hz[static_cast<std::size_t>(i)]) /
                         (2.0 * A);
        band.push_back(i);
        mag.push_back(h);
    }
    if (band.size() < 20) {
        return result;
    }

    std::vector<double> sorted = mag;
    std::sort(sorted.begin(), sorted.end());
    const double thr = sorted[static_cast<std::size_t>(0.75 * static_cast<double>(sorted.size() - 1))];

    std::vector<int> cand;
    for (std::size_t k = 0; k < band.size(); ++k) {
        if (mag[k] + 1e-15 >= thr) {
            cand.push_back(band[k]);
        }
    }
    if (cand.size() < 20) {
        cand = band;
    }

    std::vector<double> radii;
    radii.reserve(cand.size());
    for (int i : cand) {
        const Point3& p = mesh.vertices[static_cast<std::size_t>(i)];
        radii.push_back(std::hypot(p.x, p.y));
    }
    std::vector<double> sortedR = radii;
    std::sort(sortedR.begin(), sortedR.end());
    double rThr = 0.98 * sortedR.back();
    double bestGap = 0.0;
    std::size_t gapAt = 0;
    for (std::size_t i = 1; i < sortedR.size(); ++i) {
        const double g = sortedR[i] - sortedR[i - 1];
        if (g > bestGap) {
            bestGap = g;
            gapAt = i;
        }
    }
    if (bestGap > 0.25 * std::max(sortedR.back(), 1e-12)) {
        rThr = sortedR[gapAt];
    }
    std::vector<int> outer;
    for (std::size_t k = 0; k < cand.size(); ++k) {
        if (radii[k] + 1e-9 >= rThr) {
            outer.push_back(cand[k]);
        }
    }
    if (outer.size() < 20) {
        result.error = RimError::RimTooSmall;
        result.message = "rim has fewer than 20 vertices";
        return result;
    }

    std::sort(outer.begin(), outer.end(), [&](int a, int b) {
        const Point3& pa = mesh.vertices[static_cast<std::size_t>(a)];
        const Point3& pb = mesh.vertices[static_cast<std::size_t>(b)];
        return std::atan2(pa.y, pa.x) < std::atan2(pb.y, pb.x);
    });
    outer.erase(std::unique(outer.begin(), outer.end()), outer.end());

    result.rimPoints.reserve(outer.size());
    for (int i : outer) {
        result.rimPoints.push_back(mesh.vertices[static_cast<std::size_t>(i)]);
    }
    if (result.rimPoints.size() < 20) {
        result.error = RimError::RimTooSmall;
        result.message = "rim has fewer than 20 vertices";
        return result;
    }
    finishRimStats(result, mesh);
    result.error = RimError::Ok;
    result.message.clear();
    return result;
}

static RimResult DetectRimManual(const MeshData& mesh, const std::vector<Point3>& pts,
                                 double tolMm) {
    RimResult result;
    result.usedMode = RimMode::Manual;
    result.error = RimError::RimNotFound;
    result.message = "manual rim not found";
    if (pts.size() < 3 || mesh.vertices.size() < 20) {
        return result;
    }
    const Point3 n = crs(sub(pts[1], pts[0]), sub(pts[2], pts[0]));
    const double nl = nrm(n);
    if (nl < 1e-12) {
        result.message = "manual points are collinear";
        return result;
    }
    const double inv = 1.0 / nl;
    const double d = -(n.x * pts[0].x + n.y * pts[0].y + n.z * pts[0].z) * inv;
    const Point3 un{n.x * inv, n.y * inv, n.z * inv};

    double zMin = mesh.vertices[0].z;
    double zMax = zMin;
    for (const Point3& p : mesh.vertices) {
        zMin = std::min(zMin, p.z);
        zMax = std::max(zMax, p.z);
    }
    const double zCut = zMin + 0.70 * std::max(zMax - zMin, 1e-12);

    std::vector<Point3> hit;
    for (const Point3& p : mesh.vertices) {
        if (p.z < zCut) {
            continue;
        }
        const double distP = std::fabs(un.x * p.x + un.y * p.y + un.z * p.z + d);
        if (distP <= tolMm) {
            hit.push_back(p);
        }
    }
    if (hit.size() < 20) {
        result.error = RimError::RimTooSmall;
        result.message = "rim has fewer than 20 vertices";
        result.rimPoints = std::move(hit);
        return result;
    }

    double maxR = 0.0;
    for (const Point3& p : hit) {
        maxR = std::max(maxR, std::hypot(p.x, p.y));
    }
    for (const Point3& p : hit) {
        if (std::hypot(p.x, p.y) + 1e-9 >= 0.90 * maxR) {
            result.rimPoints.push_back(p);
        }
    }
    if (result.rimPoints.size() < 20) {
        result.error = RimError::RimTooSmall;
        result.message = "rim has fewer than 20 vertices";
        return result;
    }
    finishRimStats(result, mesh);
    result.error = RimError::Ok;
    result.message.clear();
    return result;
}

RimResult DetectRim(const MeshData& mesh, RimMode mode, const std::vector<Point3>& manualPts,
                    double manualTolMm) {
    if (mode == RimMode::Boundary) {
        return DetectRimBoundary(mesh);
    }
    if (mode == RimMode::Curvature) {
        return DetectRimCurvature(mesh);
    }
    if (mode == RimMode::Manual) {
        return DetectRimManual(mesh, manualPts, manualTolMm);
    }
    RimResult b = DetectRimBoundary(mesh);
    if (b.error == RimError::Ok) {
        return b;
    }
    return DetectRimCurvature(mesh);
}

} // namespace KArch
} // namespace DongArch
