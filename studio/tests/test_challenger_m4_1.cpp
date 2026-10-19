// DongArch3D Studio — Milestone 4 (Final Milestone)
// Challenger 1: Empirical White-Box Adversarial Hardening
// Probes BVH traversal, plane rotation, spatial hash chaining under degenerate
// inputs, numerical stability, and 4.3M-face real artifact invariance.

#include "ArtifactDoc.h"
#include "DimensionAnalyzer.h"
#include "MeshLoader.h"
#include "MeshSlicer.h"
#include "Plate.h"
#include "SyntheticMeshGen.h"

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <numbers>
#include <vector>

using namespace studio;
namespace KA = DongArch::KArch;
namespace KAT = DongArch::KArch::Testing;

static int g_totalChecks = 0;
static int g_passedChecks = 0;
static int g_failedChecks = 0;

#define CHALLENGE_CHECK(cond, desc, ...)                                       \
  do {                                                                         \
    ++g_totalChecks;                                                           \
    if (cond) {                                                                \
      std::printf("  [PASS] " desc "\n", ##__VA_ARGS__);                       \
      ++g_passedChecks;                                                        \
    } else {                                                                   \
      std::printf("  [FAIL] " desc "\n", ##__VA_ARGS__);                       \
      ++g_failedChecks;                                                        \
    }                                                                          \
  } while (0)

static bool nearVal(double a, double b, double tol = 1e-6) {
  return std::fabs(a - b) <= tol;
}

// ----------------------------------------------------------------------------
// Suite 1: BVH Construction & Traversal White-Box Hardening
// ----------------------------------------------------------------------------
static void testSuite1_BvhDegenerate() {
  std::printf(
      "\n=== [Suite 1] BVH Construction & Traversal Degenerate Probes ===\n");

  // 1.1 Empty mesh
  {
    KA::MeshData emptyMesh;
    KA::MeshBVH bvh;
    bvh.build(emptyMesh);
    CHALLENGE_CHECK(bvh.empty(), "1.1: BVH of empty mesh is empty");
    std::vector<int> cands;
    bvh.queryPlane(KA::Plane{{0, 1, 0}, 0}, 1e-6, cands);
    CHALLENGE_CHECK(cands.empty(),
                    "1.1: queryPlane on empty BVH returns 0 candidates");
    auto res = KA::Slice(emptyMesh, KA::Plane{{0, 1, 0}, 0}, &bvh);
    CHALLENGE_CHECK(res.empty(), "1.1: Slice on empty BVH returns 0 polylines");
  }

  // 1.2 Point-cloud mesh (all vertices at origin, degenerate faces)
  {
    KA::MeshData pointMesh;
    for (int i = 0; i < 300; ++i) {
      pointMesh.vertices.push_back({0.0, 0.0, 0.0});
    }
    for (int i = 0; i < 200; ++i) {
      pointMesh.faces.push_back({0, 0, 0});
    }
    KA::MeshBVH bvh;
    bvh.build(pointMesh);
    CHALLENGE_CHECK(!bvh.empty(),
                    "1.2: Point-mesh BVH builds without infinite recursion");
    std::vector<int> cands;
    bvh.queryPlane(KA::Plane{{0, 1, 0}, 0}, 1e-6, cands);
    CHALLENGE_CHECK(!cands.empty(), "1.2: queryPlane touches origin plane");
    auto res = KA::Slice(pointMesh, KA::Plane{{0, 1, 0}, 0}, &bvh);
    CHALLENGE_CHECK(
        res.empty(),
        "1.2: Slice on all-degenerate triangles produces 0 segments");
  }

  // 1.3 Out-of-bounds vertex index protection
  {
    KA::MeshData corruptedMesh;
    corruptedMesh.vertices.push_back({0.0, 0.0, 0.0});
    corruptedMesh.vertices.push_back({10.0, 0.0, 0.0});
    corruptedMesh.vertices.push_back({0.0, 10.0, 0.0});
    corruptedMesh.faces.push_back({0, 1, 2});
    corruptedMesh.faces.push_back({-1, 1, 2});    // negative index
    corruptedMesh.faces.push_back({0, 99999, 2}); // overflow index
    corruptedMesh.faces.push_back({0, 1, -5});    // negative index

    KA::MeshBVH bvh;
    bvh.build(corruptedMesh);
    CHALLENGE_CHECK(!bvh.empty(),
                    "1.3: BVH builds safely despite out-of-bounds indices");
    auto res = KA::Slice(corruptedMesh, KA::Plane{{0, 0, 1}, 0}, &bvh);
    CHALLENGE_CHECK(res.empty() || !res.empty(),
                    "1.3: Slice does not crash on corrupted indices");
  }

  // 1.4 Extreme coordinate scales (1e6 mm vs 1e-6 mm)
  {
    KA::MeshData hugeMesh;
    hugeMesh.vertices.push_back({-1e6, -1e6, 0.0});
    hugeMesh.vertices.push_back({1e6, -1e6, 0.0});
    hugeMesh.vertices.push_back({0.0, 1e6, 0.0});
    hugeMesh.faces.push_back({0, 1, 2});
    KA::MeshBVH bvhHuge;
    bvhHuge.build(hugeMesh);
    CHALLENGE_CHECK(!bvhHuge.empty(),
                    "1.4: BVH handles 1e6 coordinate scale without overflow");
    std::vector<int> cands;
    bvhHuge.queryPlane(KA::Plane{{0, 1, 0}, 0}, 1e-6, cands);
    CHALLENGE_CHECK(!cands.empty(), "1.4: Huge mesh plane query succeeds");

    KA::MeshData tinyMesh;
    tinyMesh.vertices.push_back({-1e-6, -1e-6, 0.0});
    tinyMesh.vertices.push_back({1e-6, -1e-6, 0.0});
    tinyMesh.vertices.push_back({0.0, 1e-6, 0.0});
    tinyMesh.faces.push_back({0, 1, 2});
    KA::MeshBVH bvhTiny;
    bvhTiny.build(tinyMesh);
    CHALLENGE_CHECK(!bvhTiny.empty(),
                    "1.4: BVH handles 1e-6 coordinate scale without underflow");
  }

  // 1.5 Exactly coplanar triangle test
  {
    KA::MeshData coplanarMesh;
    coplanarMesh.vertices.push_back({-10.0, 0.0, -10.0});
    coplanarMesh.vertices.push_back({10.0, 0.0, -10.0});
    coplanarMesh.vertices.push_back({0.0, 0.0, 10.0});
    coplanarMesh.faces.push_back({0, 1, 2});
    KA::MeshBVH bvh;
    bvh.build(coplanarMesh);
    auto res = KA::Slice(coplanarMesh, KA::Plane{{0, 1, 0}, 0}, &bvh);
    CHALLENGE_CHECK(res.empty(), "1.5: Triangle lying completely in plane "
                                 "produces no spurious segments");
  }
}

// ----------------------------------------------------------------------------
// Suite 2: Virtual Plane Rotation Numerical Stability & Invariance
// ----------------------------------------------------------------------------
static void testSuite2_PlaneRotationStability() {
  std::printf(
      "\n=== [Suite 2] Virtual Plane Rotation Numerical Stability ===\n");

  ArtifactDoc doc;
  doc.loadSample(true); // Confirmed sample pottery

  const Dims5 initialDims = doc.dims();
  CHALLENGE_CHECK(initialDims.ok, "2.0: Confirmed sample has valid dimensions");

  // 2.1 Full 360-degree sweep at 1-degree steps: verify no crash, non-empty
  // section, and dimension invariance
  bool allSectionsNonEmpty = true;
  double maxDiffH = 0.0;
  double maxDiffRim = 0.0;
  double maxDiffBelly = 0.0;
  double maxDiffBase = 0.0;

  for (int deg = 0; deg < 360; ++deg) {
    doc.previewFrontAngle(static_cast<double>(deg));
    const auto sec = doc.displaySection();
    if (sec.empty()) {
      allSectionsNonEmpty = false;
    }
  }
  CHALLENGE_CHECK(allSectionsNonEmpty, "2.1: 360-degree preview sweep produces "
                                       "non-empty sections at all angles");

  // 2.2 Discrete commits at cardinal & arbitrary angles
  const double checkAngles[] = {0.0,   45.0,  90.0,  135.0,    180.0,  225.0,
                                270.0, 315.0, -30.0, -90.0,    -180.0, 360.0,
                                720.0, 1e-7,  -1e-7, 89.999999};

  for (double ang : checkAngles) {
    doc.setFrontAngle(ang);
    const Dims5 d = doc.dims();
    maxDiffH = std::max(maxDiffH, std::fabs(d.height - initialDims.height));
    maxDiffRim = std::max(maxDiffRim, std::fabs(d.rim - initialDims.rim));
    maxDiffBelly =
        std::max(maxDiffBelly, std::fabs(d.belly - initialDims.belly));
    maxDiffBase = std::max(maxDiffBase, std::fabs(d.base - initialDims.base));
  }

  CHALLENGE_CHECK(maxDiffH <= 0.001,
                  "2.2: Height invariant under rotation (maxDiff=%.4f mm)",
                  maxDiffH);
  CHALLENGE_CHECK(
      maxDiffRim <= 0.001,
      "2.2: Rim diameter invariant under rotation (maxDiff=%.4f mm)",
      maxDiffRim);
  CHALLENGE_CHECK(
      maxDiffBelly <= 0.001,
      "2.2: Belly diameter invariant under rotation (maxDiff=%.4f mm)",
      maxDiffBelly);
  CHALLENGE_CHECK(
      maxDiffBase <= 0.001,
      "2.2: Base diameter invariant under rotation (maxDiff=%.4f mm)",
      maxDiffBase);

  // 2.3 Rapid toggling between preview and commit
  for (int i = 0; i < 100; ++i) {
    doc.previewFrontAngle(static_cast<double>(i * 3.7));
    if (i % 5 == 0) {
      doc.commitPreview();
      CHALLENGE_CHECK(!doc.isPreviewing(),
                      "2.3: isPreviewing is false after commit");
    }
  }
  doc.commitPreview();
}

// ----------------------------------------------------------------------------
// Suite 3: Spatial Hash Grid Segment Chaining Pathological Probes
// ----------------------------------------------------------------------------
static void testSuite3_SpatialHashChaining() {
  std::printf(
      "\n=== [Suite 3] Spatial Hash Grid Segment Chaining Probes ===\n");

  // 3.1 Concentric closed rings: inner and outer wall must remain distinct
  // closed loops
  {
    KAT::SyntheticParams sp;
    sp.radialSeg = 120;
    sp.heightSeg = 80;
    sp.capTop = false;
    sp.capBottom = false;
    const auto vessel = KAT::MakeTorusProfileVessel(80.0, 60.0, 160.0, 6.0, sp);
    KA::MeshData mesh = vessel.toMesh();
    KA::FillMeshStats(mesh);

    // Slice at mid-height Z = 80
    const auto loops = KA::Slice(mesh, KA::Plane{{0, 0, 1}, -80.0});
    CHALLENGE_CHECK(loops.size() >= 2,
                    "3.1: Horizontal slice of double-wall cylinder yields at "
                    "least 2 loops (got %zu)",
                    loops.size());

    // Verify loops are closed: distance between start and end point <= 1e-4
    bool loopsClosed = true;
    for (const auto &lp : loops) {
      if (lp.pts.size() < 3)
        continue;
      const double d2 = (lp.pts.front().x - lp.pts.back().x) *
                            (lp.pts.front().x - lp.pts.back().x) +
                        (lp.pts.front().y - lp.pts.back().y) *
                            (lp.pts.front().y - lp.pts.back().y) +
                        (lp.pts.front().z - lp.pts.back().z) *
                            (lp.pts.front().z - lp.pts.back().z);
      if (d2 > 1e-4) {
        loopsClosed = false;
      }
    }
    CHALLENGE_CHECK(loopsClosed,
                    "3.1: Concentric rings are strictly closed polylines");
  }

  // 3.2 Dense vertex star / fan intersection (stress hash bucket and segment
  // used-flags)
  {
    KA::MeshData fanMesh;
    fanMesh.vertices.push_back(
        {0.0, 0.0, 0.0}); // Center hub on slice plane Y=0
    const int N = 100;
    for (int i = 0; i < N; ++i) {
      const double th = 2.0 * std::numbers::pi * double(i) / double(N);
      fanMesh.vertices.push_back(
          {10.0 * std::cos(th), 5.0 * std::sin(th), 10.0 * std::cos(th)});
    }
    for (int i = 0; i < N; ++i) {
      fanMesh.faces.push_back({0, 1 + i, 1 + ((i + 1) % N)});
    }

    KA::MeshBVH bvh(fanMesh);
    const auto polylines = KA::Slice(fanMesh, KA::Plane{{0, 1, 0}, 0}, &bvh);
    CHALLENGE_CHECK(
        !polylines.empty(),
        "3.2: Dense 100-face fan sliced successfully without infinite loop");
  }

  // 3.3 Micro-segment suppression
  {
    KA::MeshData microMesh;
    // Triangle with edge length 1e-8 mm intersecting plane
    microMesh.vertices.push_back({-1e-8, -1.0, 0.0});
    microMesh.vertices.push_back({1e-8, 1.0, 0.0});
    microMesh.vertices.push_back({0.0, 0.0, 1e-8});
    microMesh.faces.push_back({0, 1, 2});
    auto res = KA::Slice(microMesh, KA::Plane{{0, 1, 0}, 0});
    CHALLENGE_CHECK(
        res.empty(),
        "3.3: Micro-triangle below weld threshold produces 0 segments");
  }
}

// ----------------------------------------------------------------------------
// Suite 4: State Machine Transitions & Unit Rescaling
// ----------------------------------------------------------------------------
static void testSuite4_StateTransitions() {
  std::printf(
      "\n=== [Suite 4] State Machine Transitions & Unit Rescaling ===\n");

  ArtifactDoc doc;
  doc.loadSample(false); // Loaded but unconfirmed

  CHALLENGE_CHECK(!doc.poseConfirmed(),
                  "4.1: Freshly loaded sample has poseConfirmed=false");
  CHALLENGE_CHECK(doc.dims().height == 0.0,
                  "4.1: Unconfirmed doc has height=0");

  // Attempting setFrontAngle before confirmPose must be a no-op
  doc.setFrontAngle(45.0);
  CHALLENGE_CHECK(doc.frontAngle() == 0.0,
                  "4.2: setFrontAngle before confirmPose is safely ignored");

  // Confirm unit and pose
  doc.setUnitConfirmed(true);
  doc.confirmPose();
  CHALLENGE_CHECK(doc.poseConfirmed(),
                  "4.3: confirmPose succeeds after unit confirmed");
  const double originalHeight = doc.dims().height;
  CHALLENGE_CHECK(originalHeight > 100.0,
                  "4.3: Sample height is valid (%.2f mm)", originalHeight);

  // Unit rescaling by factor 10.0 (e.g. cm -> mm)
  doc.rescaleUnits(10.0);
  CHALLENGE_CHECK(!doc.poseConfirmed(), "4.4: rescaleUnits unconfirms pose");
  doc.setUnitConfirmed(true);
  doc.confirmPose();
  const double scaledHeight = doc.dims().height;
  CHALLENGE_CHECK(
      nearVal(scaledHeight, originalHeight * 10.0, 0.5),
      "4.4: Rescaled height matches factor 10 (got %.2f vs expected %.2f)",
      scaledHeight, originalHeight * 10.0);

  // Revert unit rescaling
  doc.rescaleUnits(0.1);
  doc.setUnitConfirmed(true);
  doc.confirmPose();
  CHALLENGE_CHECK(nearVal(doc.dims().height, originalHeight, 0.5),
                  "4.5: Reverted height matches original (got %.2f vs %.2f)",
                  doc.dims().height, originalHeight);
}

// ----------------------------------------------------------------------------
// Suite 5: 4.3M Face Real Artifact Master BVH Traversal (if 6183.obj available)
// ----------------------------------------------------------------------------
static void testSuite5_RealArtifactMasterBvh(const QString &objPath) {
  std::printf("\n=== [Suite 5] 4.3M Real Artifact Master BVH Traversal ===\n");

  if (objPath.isEmpty() || !QFile::exists(objPath)) {
    std::printf("  [SKIP] 6183.obj not found. Skipping Suite 5.\n");
    return;
  }

  ArtifactDoc doc;
  QString err;
  const bool ok = doc.loadFile(objPath, &err);
  CHALLENGE_CHECK(ok, "5.1: Real artifact loaded successfully");
  doc.confirmPose();
  CHALLENGE_CHECK(doc.poseConfirmed(), "5.1: Real artifact pose confirmed");

  const auto &bvh = doc.bvhMaster();
  CHALLENGE_CHECK(!bvh.empty(), "5.2: Master BVH is non-empty on 4.3M faces");
  CHALLENGE_CHECK(bvh.nodes().size() > 10000,
                  "5.2: Master BVH contains %zu nodes (>10k)",
                  bvh.nodes().size());

  // Query plane across 8 orientations: ensure candidates count is consistent
  // and prunes >98% of faces
  const size_t totalFaces = doc.meshMaster().faces.size();
  for (int ang = 0; ang < 360; ang += 45) {
    const double rad = double(ang) * std::numbers::pi / 180.0;
    KA::Plane pl{{std::sin(rad), std::cos(rad), 0.0}, 0.0};
    std::vector<int> cands;
    cands.reserve(16384);
    bvh.queryPlane(pl, 1e-6, cands);
    const double prunePct =
        100.0 * (1.0 - double(cands.size()) / double(totalFaces));
    CHALLENGE_CHECK(cands.size() > 500 && prunePct > 98.0,
                    "5.3: Orientation %d deg: %zu candidates (pruned %.2f%%)",
                    ang, cands.size(), prunePct);
  }
}

int main(int argc, char **argv) {
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  qputenv("QT_QPA_PLATFORM", "offscreen");
  QCoreApplication app(argc, argv);

  QString objPath;
  for (int i = 1; i < argc; ++i) {
    if (argv[i] && argv[i][0] != '\0') {
      QString candidate = QString::fromLocal8Bit(argv[i]);
      if (!QFileInfo(candidate).isFile()) {
        candidate = QString::fromUtf8(argv[i]);
      }
      if (QFileInfo(candidate).isFile()) {
        objPath = candidate;
        break;
      }
    }
  }
  if (objPath.isEmpty()) {
    const QByteArray envVal = qgetenv("DONGARCH_BENCH_OBJ");
    if (!envVal.isEmpty()) {
      QString candidate = QString::fromLocal8Bit(envVal.constData()).trimmed();
      if (!QFileInfo(candidate).isFile()) {
        candidate = QString::fromUtf8(envVal.constData()).trimmed();
      }
      if (QFileInfo(candidate).isFile()) {
        objPath = candidate;
      }
    }
  }

  std::printf("================================================================"
              "======\n");
  std::printf(
      "[CHALLENGER M4-1] Tier 5 White-Box Adversarial Hardening Suite\n");
  std::printf("================================================================"
              "======\n");

  testSuite1_BvhDegenerate();
  testSuite2_PlaneRotationStability();
  testSuite3_SpatialHashChaining();
  testSuite4_StateTransitions();
  testSuite5_RealArtifactMasterBvh(objPath);

  std::printf("\n=============================================================="
              "========\n");
  std::printf("[CHALLENGER M4-1] SUMMARY: Total=%d, Passed=%d, Failed=%d\n",
              g_totalChecks, g_passedChecks, g_failedChecks);
  std::printf("================================================================"
              "======\n");

  return g_failedChecks ? 1 : 0;
}
