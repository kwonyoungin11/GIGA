/* Standalone T2 check: open cylinder shell rim = top outer loop. */

#include "RimDetector.h"
#include "SyntheticMeshGen.h"

#include <cmath>
#include <cstdio>
#include <numbers>

using DongArch::KArch::DetectRim;
using DongArch::KArch::DetectRimBoundary;
using DongArch::KArch::RimMode;
using DongArch::KArch::MeshData;
using DongArch::KArch::Point3;
using DongArch::KArch::RimError;

static MeshData makeOpenCylinderShell(double rOuter, double height, double thickness,
                                      int radial, int heightSeg) {
    MeshData m;
    const double rInner = rOuter - thickness;
    const int rings = heightSeg + 1;
    m.vertices.reserve(static_cast<std::size_t>(2 * rings * radial));

    auto addRing = [&](double r, double z) {
        for (int j = 0; j < radial; ++j) {
            const double t = 2.0 * std::numbers::pi * static_cast<double>(j) /
                             static_cast<double>(radial);
            m.vertices.push_back(Point3{r * std::cos(t), r * std::sin(t), z});
        }
    };

    for (int i = 0; i < rings; ++i) {
        const double z = height * static_cast<double>(i) / static_cast<double>(heightSeg);
        addRing(rOuter, z);
    }
    const int innerBase = rings * radial;
    for (int i = 0; i < rings; ++i) {
        const double z = height * static_cast<double>(i) / static_cast<double>(heightSeg);
        addRing(rInner, z);
    }

    auto quad = [&](int a, int b, int c, int d) {
        m.faces.push_back({a, b, c});
        m.faces.push_back({a, c, d});
    };

    for (int i = 0; i < heightSeg; ++i) {
        for (int j = 0; j < radial; ++j) {
            const int jn = (j + 1) % radial;
            const int o0 = i * radial + j;
            const int o1 = i * radial + jn;
            const int o2 = (i + 1) * radial + jn;
            const int o3 = (i + 1) * radial + j;
            quad(o0, o1, o2, o3);
            const int i0 = innerBase + i * radial + j;
            const int i1 = innerBase + i * radial + jn;
            const int i2 = innerBase + (i + 1) * radial + jn;
            const int i3 = innerBase + (i + 1) * radial + j;
            quad(i0, i3, i2, i1);
        }
    }

    // Bottom cap only (z=0): no top cap -> two boundary loops at z=height.
    for (int j = 0; j < radial; ++j) {
        const int jn = (j + 1) % radial;
        quad(j, innerBase + j, innerBase + jn, jn);
    }
    return m;
}

static void addTopCap(MeshData& m, int radial, int heightSeg) {
    const int rings = heightSeg + 1;
    const int innerBase = rings * radial;
    const int oTop = heightSeg * radial;
    const int iTop = innerBase + heightSeg * radial;
    for (int j = 0; j < radial; ++j) {
        const int jn = (j + 1) % radial;
        m.faces.push_back({oTop + j, oTop + jn, iTop + jn});
        m.faces.push_back({oTop + j, iTop + jn, iTop + j});
    }
}

int main() {
    const MeshData shell = makeOpenCylinderShell(50.0, 100.0, 5.0, 180, 20);
    const auto rim = DetectRimBoundary(shell);
    if (rim.error != RimError::Ok) {
        std::fprintf(stderr, "FAIL: error=%d %s loops=%d n=%zu\n",
                     static_cast<int>(rim.error), rim.message.c_str(),
                     rim.loopCount, rim.rimPoints.size());
        return 1;
    }
    if (rim.loopCount != 2) {
        std::fprintf(stderr, "FAIL: expected 2 top loops, got %d\n", rim.loopCount);
        return 1;
    }
    if (rim.rimPoints.size() != 180) {
        std::fprintf(stderr, "FAIL: rim verts %zu expected 180\n", rim.rimPoints.size());
        return 1;
    }
    if (std::fabs(rim.meanZ - 100.0) > 1e-9) {
        std::fprintf(stderr, "FAIL: meanZ=%.12f expected 100\n", rim.meanZ);
        return 1;
    }

    double rMean = 0.0;
    for (const auto& p : rim.rimPoints) {
        rMean += std::sqrt(p.x * p.x + p.y * p.y);
    }
    rMean /= static_cast<double>(rim.rimPoints.size());
    const double rErr = std::fabs(rMean - 50.0);
    std::printf("loops=%d n=%zu meanZ=%.3f length=%.6f rMean=%.12f rErr=%.3e\n",
                rim.loopCount, rim.rimPoints.size(), rim.meanZ, rim.length, rMean, rErr);
    if (rErr >= 1e-9) {
        std::fprintf(stderr, "FAIL: picked inner or wrong ring\n");
        return 1;
    }

    MeshData tiny = makeOpenCylinderShell(50.0, 100.0, 5.0, 12, 4);
    const auto small = DetectRimBoundary(tiny);
    if (small.error != RimError::RimTooSmall) {
        std::fprintf(stderr, "FAIL: expected RimTooSmall, got %d\n",
                     static_cast<int>(small.error));
        return 1;
    }

    MeshData empty;
    const auto none = DetectRimBoundary(empty);
    if (none.error != RimError::RimNotFound) {
        std::fprintf(stderr, "FAIL: expected RimNotFound\n");
        return 1;
    }

    MeshData disk;
    disk.vertices.push_back(Point3{0, 0, 0});
    constexpr int nDisk = 24;
    for (int j = 0; j < nDisk; ++j) {
        const double t = 2.0 * std::numbers::pi * static_cast<double>(j) /
                         static_cast<double>(nDisk);
        disk.vertices.push_back(Point3{std::cos(t), std::sin(t), 0});
    }
    for (int j = 0; j < nDisk; ++j) {
        disk.faces.push_back({0, 1 + j, 1 + (j + 1) % nDisk});
    }
    const int a = static_cast<int>(disk.vertices.size());
    disk.vertices.push_back(Point3{2, 0, 0});
    disk.vertices.push_back(Point3{2, 1, 0});
    disk.faces.push_back({1, a, a + 1});
    const auto branched = DetectRimBoundary(disk);
    if (branched.error != RimError::Ok || branched.rimPoints.size() != static_cast<std::size_t>(nDisk)) {
        std::fprintf(stderr, "FAIL: branch/antenna rim n=%zu expected %d err=%d\n",
                     branched.rimPoints.size(), nDisk, static_cast<int>(branched.error));
        return 1;
    }

    MeshData closed = makeOpenCylinderShell(50.0, 100.0, 5.0, 180, 20);
    addTopCap(closed, 180, 20);
    const auto sealed = DetectRimBoundary(closed);
    if (sealed.error != RimError::RimNotFound || sealed.loopCount != 0) {
        std::fprintf(stderr, "FAIL: sealed shell error=%d loops=%d\n",
                     static_cast<int>(sealed.error), sealed.loopCount);
        return 1;
    }

    using DongArch::KArch::Testing::MakeCylinderShell;
    using DongArch::KArch::Testing::SyntheticParams;

    SyntheticParams openP;
    openP.radialSeg = 180;
    openP.heightSeg = 20;
    openP.capTop = false;
    openP.capBottom = true;
    openP.tiltDeg = 15.0;
    const auto tilt = DetectRim(MakeCylinderShell(50.0, 100.0, 5.0, openP).toMesh(),
                                RimMode::Boundary);
    if (tilt.error != RimError::Ok || tilt.rimPoints.size() != 180) {
        std::fprintf(stderr, "FAIL: tilt15 n=%zu err=%d\n", tilt.rimPoints.size(),
                     static_cast<int>(tilt.error));
        return 1;
    }

    SyntheticParams sealedP;
    sealedP.radialSeg = 180;
    sealedP.heightSeg = 20;
    sealedP.capTop = true;
    sealedP.capBottom = true;
    const auto autoSealed = DetectRim(MakeCylinderShell(50.0, 100.0, 5.0, sealedP).toMesh(),
                                      RimMode::Auto);
    if (autoSealed.error != RimError::Ok || autoSealed.usedMode != RimMode::Curvature ||
        autoSealed.warning != "RIM_MODE_FALLBACK" || autoSealed.rimPoints.size() < 20) {
        std::fprintf(stderr, "FAIL: auto sealed mode=%d warn=%s n=%zu err=%d\n",
                     static_cast<int>(autoSealed.usedMode), autoSealed.warning.c_str(),
                     autoSealed.rimPoints.size(), static_cast<int>(autoSealed.error));
        return 1;
    }
    double rCurv = 0.0;
    for (const auto& p : autoSealed.rimPoints) {
        rCurv += std::hypot(p.x, p.y);
    }
    rCurv /= static_cast<double>(autoSealed.rimPoints.size());
    if (std::fabs(rCurv - 50.0) > 0.5) {
        std::fprintf(stderr, "FAIL: curvature rMean=%.6f\n", rCurv);
        return 1;
    }

    SyntheticParams bothP = openP;
    bothP.tiltDeg = 0.0;
    bothP.capTop = false;
    bothP.capBottom = false;
    const auto both = DetectRimBoundary(MakeCylinderShell(50.0, 100.0, 5.0, bothP).toMesh());
    if (both.error != RimError::Ok || both.loopCount != 4 || both.rimPoints.size() != 180 ||
        std::fabs(both.meanZ - 100.0) > 1e-9) {
        std::fprintf(stderr, "FAIL: both-open loops=%d n=%zu z=%.6f err=%d\n", both.loopCount,
                     both.rimPoints.size(), both.meanZ, static_cast<int>(both.error));
        return 1;
    }

    openP.tiltDeg = 0.0;
    const auto openMesh = MakeCylinderShell(50.0, 100.0, 5.0, openP).toMesh();
    const std::vector<Point3> three = {
        Point3{50.0, 0.0, 100.0},
        Point3{0.0, 50.0, 100.0},
        Point3{-50.0, 0.0, 100.0},
    };
    const auto man = DetectRim(openMesh, RimMode::Manual, three, 1.0);
    if (man.error != RimError::Ok || man.usedMode != RimMode::Manual ||
        man.rimPoints.size() < 20) {
        std::fprintf(stderr, "FAIL: manual n=%zu err=%d\n", man.rimPoints.size(),
                     static_cast<int>(man.error));
        return 1;
    }

    // 안·바깥 입술 고리의 평균 높이가 0.5 mm 안이면 같은 높이로 보고 넓이가 가장 큰 바깥 고리 (스캔 잡음).
    // 안 입술은 반경을 번갈아 ±0.5 mm 흔들어(지그재그 경계) 바깥보다 길게 만든다 — 길이로는 안쪽이 뽑힌다.
    MeshData lip = makeOpenCylinderShell(50.0, 100.0, 5.0, 180, 20);
    const int outerTop = 20 * 180;
    const int innerTop = 21 * 180 + 20 * 180;
    for (int j = 0; j < 180; ++j) {
        Point3& v = lip.vertices[static_cast<std::size_t>(innerTop + j)];
        const double s = (45.0 + ((j % 2 == 0) ? 0.5 : -0.5)) / 45.0;
        v.x *= s;
        v.y *= s;
        v.z += 0.01;
    }
    auto ringLen = [&](int base) {
        double len = 0.0;
        for (int j = 0; j < 180; ++j) {
            const Point3& a = lip.vertices[static_cast<std::size_t>(base + j)];
            const Point3& b = lip.vertices[static_cast<std::size_t>(base + (j + 1) % 180)];
            len += std::hypot(a.x - b.x, a.y - b.y, a.z - b.z);
        }
        return len;
    };
    const double innerLen = ringLen(innerTop);
    const double outerLen = ringLen(outerTop);
    const auto tie = DetectRimBoundary(lip);
    double rTie = 0.0;
    for (const auto& p : tie.rimPoints) {
        rTie += std::hypot(p.x, p.y);
    }
    rTie /= static_cast<double>(tie.rimPoints.size());
    for (int j = 0; j < 180; ++j) {
        lip.vertices[static_cast<std::size_t>(innerTop + j)].z += 0.99;
    }
    const auto high = DetectRimBoundary(lip);
    double rHigh = 0.0;
    for (const auto& p : high.rimPoints) {
        rHigh += std::hypot(p.x, p.y);
    }
    rHigh /= static_cast<double>(high.rimPoints.size());
    std::printf("jaggedInnerLip len=%.3f>%.3f | +0.01 n=%zu meanZ=%.3f rMean=%.6f | +1.00 n=%zu meanZ=%.3f rMean=%.6f\n",
                innerLen, outerLen, tie.rimPoints.size(), tie.meanZ, rTie, high.rimPoints.size(), high.meanZ,
                rHigh);
    if (!(innerLen > outerLen) || tie.error != RimError::Ok || std::fabs(rTie - 50.0) >= 1e-9 ||
        std::fabs(tie.meanZ - 100.0) > 1e-9) {
        std::fprintf(stderr, "FAIL: noisy jagged lip tie picked inner rMean=%.6f\n", rTie);
        return 1;
    }
    if (high.error != RimError::Ok || std::fabs(rHigh - 45.0) >= 1e-9 || std::fabs(high.meanZ - 101.0) > 1e-9) {
        std::fprintf(stderr, "FAIL: inner lip 1 mm higher not picked rMean=%.6f\n", rHigh);
        return 1;
    }

    std::printf("tilt15 n=%zu sealedCurv n=%zu r=%.3f manual n=%zu bothOpen loops=%d\n",
                tilt.rimPoints.size(), autoSealed.rimPoints.size(), rCurv, man.rimPoints.size(),
                both.loopCount);
    std::puts("PASS");
    return 0;
}
